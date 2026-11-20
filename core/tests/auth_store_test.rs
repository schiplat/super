//! auth.db store: config/secret round-trips, session lifecycle, single-use
//! pending state, and audit retention.

use common::{RoleRule, RuleMatch, SsoConfig, UserRole};
use super_core::auth::AuthStore;
use super_core::auth::store::{AuditEvent, NewSession, PendingAuth, SECRET_GRACE_SECS};

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn config() -> SsoConfig {
    SsoConfig {
        enabled: true,
        issuer: "https://idp.example.com".into(),
        client_id: "super".into(),
        public_base_url: "https://super.example.com".into(),
        scopes: "openid profile email".into(),
        groups_claim: "groups".into(),
        token_auth_method: common::TokenAuthMethod::ClientSecretBasic,
        role_rules: vec![RoleRule {
            r#match: RuleMatch {
                sub: Some("u1".into()),
                ..Default::default()
            },
            role: UserRole::Admin,
        }],
        session_ttl_secs: 12 * 60 * 60,
        idle_timeout_secs: 60 * 60,
        allow_insecure_issuer: false,
    }
}

async fn open() -> (tempfile::TempDir, AuthStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = AuthStore::open(&dir.path().join("data/auth.db"))
        .await
        .unwrap();
    (dir, store)
}

#[tokio::test]
async fn config_round_trip_and_revision_bump() {
    let (_dir, store) = open().await;
    assert!(store.load_config().await.unwrap().is_none());
    assert_eq!(store.config_revision().await.unwrap(), 0);

    let rev1 = store.save_config(&config(), Some("root")).await.unwrap();
    let stored = store.load_config().await.unwrap().unwrap();
    assert_eq!(stored.config, config());
    assert_eq!(stored.revision, rev1);
    assert_eq!(stored.updated_by.as_deref(), Some("root"));

    let rev2 = store.save_config(&config(), None).await.unwrap();
    assert!(rev2 > rev1, "{rev2} should exceed {rev1}");
    assert_eq!(store.config_revision().await.unwrap(), rev2);
}

#[tokio::test]
async fn invalid_config_is_rejected_before_writing() {
    let (_dir, store) = open().await;
    let mut bad = config();
    bad.issuer = "http://idp.example.com".into();
    assert!(store.save_config(&bad, None).await.is_err());
    assert!(store.load_config().await.unwrap().is_none());
}

#[tokio::test]
async fn toggle_without_config_reports_missing() {
    let (_dir, store) = open().await;
    assert!(!store.set_enabled(false, None).await.unwrap());
    store.save_config(&config(), None).await.unwrap();
    assert!(store.set_enabled(false, None).await.unwrap());
    assert!(!store.load_config().await.unwrap().unwrap().config.enabled);
}

#[tokio::test]
async fn config_survives_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.db");
    {
        let store = AuthStore::open(&path).await.unwrap();
        store.save_config(&config(), None).await.unwrap();
    }
    let store = AuthStore::open(&path).await.unwrap();
    assert_eq!(store.load_config().await.unwrap().unwrap().config, config());
}

#[cfg(unix)]
#[tokio::test]
async fn database_and_sidecars_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, store) = open().await;
    store.save_config(&config(), None).await.unwrap();
    let path = store.path().to_path_buf();
    for suffix in ["", "-wal", "-shm"] {
        let mut p = path.as_os_str().to_owned();
        p.push(suffix);
        let p = std::path::PathBuf::from(p);
        if p.exists() {
            let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "{} is 0{:03o}", p.display(), mode);
        }
    }
}

#[tokio::test]
async fn secret_rotation_keeps_the_old_value_for_a_window() {
    let (_dir, store) = open().await;
    assert!(store.load_secret().await.unwrap().is_none());

    store.put_secret("first-secret").await.unwrap();
    let record = store.load_secret().await.unwrap().unwrap();
    assert_eq!(record.current(), "first-secret");
    assert_eq!(record.previous_if_valid(now()), None);

    store.put_secret("second-secret").await.unwrap();
    let record = store.load_secret().await.unwrap().unwrap();
    assert_eq!(record.current(), "second-secret");
    assert_eq!(record.previous_if_valid(now()), Some("first-secret"));
    assert_eq!(
        record.previous_if_valid(now() + SECRET_GRACE_SECS + 1),
        None,
        "grace window must close"
    );

    // Re-submitting the current value must not clobber the grace window.
    store.put_secret("second-secret").await.unwrap();
    let record = store.load_secret().await.unwrap().unwrap();
    assert_eq!(record.previous_if_valid(now()), Some("first-secret"));

    assert!(store.put_secret("short").await.is_err());
    assert!(store.put_secret("has\nnewline").await.is_err());
}

#[tokio::test]
async fn secret_debug_is_redacted() {
    let (_dir, store) = open().await;
    store.put_secret("super-secret-value").await.unwrap();
    let record = store.load_secret().await.unwrap().unwrap();
    let rendered = format!("{record:?}");
    assert!(!rendered.contains("super-secret-value"), "{rendered}");
    assert!(rendered.contains("***"), "{rendered}");
}

fn session(id: &str, hash: u8, created_at: u64) -> NewSession {
    NewSession {
        id: id.into(),
        token_sha256: vec![hash; 32],
        sub: "u1".into(),
        email: Some("ops@example.com".into()),
        name: Some("Ops".into()),
        groups_json: r#"["admins"]"#.into(),
        created_at,
        expires_at: created_at + 3600,
        ip: Some("127.0.0.1".into()),
        user_agent: Some("test".into()),
    }
}

#[tokio::test]
async fn session_lifecycle() {
    let (_dir, store) = open().await;
    let created = now();
    store
        .insert_session(&session("s1", 1, created))
        .await
        .unwrap();
    store
        .insert_session(&session("s2", 2, created))
        .await
        .unwrap();
    assert_eq!(store.count_active_sessions(created).await.unwrap(), 2);

    let found = store
        .find_session_by_token_hash(&[1u8; 32])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.id, "s1");
    assert_eq!(found.sub, "u1");
    assert_eq!(found.groups_json, r#"["admins"]"#);

    // Exchange tickets are single-use.
    assert!(store.mark_exchange_used(&[1u8; 32], created).await.unwrap());
    assert!(!store.mark_exchange_used(&[1u8; 32], created).await.unwrap());

    store.touch_session("s1", created + 60).await.unwrap();
    assert_eq!(
        store
            .find_session_by_token_hash(&[1u8; 32])
            .await
            .unwrap()
            .unwrap()
            .last_seen_at,
        created + 60
    );

    assert_eq!(store.list_sessions(created).await.unwrap().len(), 2);
    assert!(store.revoke_session("s2").await.unwrap());
    assert!(!store.revoke_session("s2").await.unwrap());
    assert_eq!(store.count_active_sessions(created).await.unwrap(), 1);
    assert_eq!(store.list_sessions(created).await.unwrap().len(), 1);

    // "Revoke all except me" leaves the current session alone.
    assert_eq!(store.revoke_all(Some("s1")).await.unwrap(), 0);
    assert_eq!(store.revoke_all(None).await.unwrap(), 1);
    assert_eq!(store.count_active_sessions(created).await.unwrap(), 0);
}

#[tokio::test]
async fn expired_sessions_are_not_active() {
    let (_dir, store) = open().await;
    let created = now();
    store
        .insert_session(&session("s1", 1, created))
        .await
        .unwrap();
    assert_eq!(
        store.count_active_sessions(created + 3601).await.unwrap(),
        0
    );
    assert!(
        store
            .list_sessions(created + 3601)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn pending_state_is_single_use() {
    let (_dir, store) = open().await;
    let created = now();
    store
        .insert_pending(&PendingAuth {
            state: "state-1".into(),
            nonce: "nonce-1".into(),
            verifier: "verifier-1".into(),
            return_to: "/programs".into(),
            created_at: created,
            expires_at: created + 600,
        })
        .await
        .unwrap();

    let taken = store.take_pending("state-1").await.unwrap().unwrap();
    assert_eq!(taken.nonce, "nonce-1");
    assert_eq!(taken.return_to, "/programs");
    assert!(store.take_pending("state-1").await.unwrap().is_none());
    assert!(store.take_pending("unknown").await.unwrap().is_none());
}

#[tokio::test]
async fn audit_round_trip_and_retention() {
    let (_dir, store) = open().await;
    store
        .audit(AuditEvent {
            kind: "login_success",
            sub: Some("u1"),
            email: Some("ops@example.com"),
            ip: Some("127.0.0.1"),
            ua: Some("test-agent"),
            detail: None,
        })
        .await
        .unwrap();

    let rows = store.list_audit(10).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, "login_success");
    assert_eq!(rows[0].sub.as_deref(), Some("u1"));

    // A "now" far enough ahead makes the row older than the retention window.
    let removed = store.prune_audit(now() + 31 * 24 * 60 * 60).await.unwrap();
    assert_eq!(removed, 1);
    assert!(store.list_audit(10).await.unwrap().is_empty());
}
