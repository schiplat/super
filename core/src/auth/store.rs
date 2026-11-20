//! SQLite store for SSO configuration, the OIDC client secret, local sessions,
//! pending authorization state, and login audit.
//!
//! Opened eagerly by superd in non-plugin mode so the active gate can be
//! resolved at startup. WAL with a small pool: session lookups happen on every
//! authenticated request and must not serialize behind each other, while
//! writes are rare (login, logout, config change).
//!
//! The whole file is credential-grade. Nothing here returns a session token:
//! only token hashes are stored, and the secret is exposed solely through
//! [`SecretRecord`], whose `Debug` is redacted.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, bail};
use common::{RoleRule, SsoConfig, TokenAuthMethod};
use sqlx::Row;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

/// Upper bound on concurrent sessions; the oldest idle one is dropped first.
pub const MAX_SESSIONS: i64 = 1000;
/// Upper bound on in-flight login attempts (10-minute TTL each).
pub const MAX_PENDING: i64 = 256;
/// Upper bound on in-flight CLI device codes.
pub const MAX_CLI_PENDING: i64 = 128;
/// How long the previous client secret stays usable after rotation.
pub const SECRET_GRACE_SECS: u64 = 15 * 60;
/// Audit retention.
pub const AUDIT_RETENTION_SECS: u64 = 30 * 24 * 60 * 60;
pub const AUDIT_MAX_ROWS: i64 = 50_000;

#[derive(Clone)]
pub struct AuthStore {
    pool: SqlitePool,
    path: PathBuf,
}

/// A stored configuration row plus its audit metadata.
#[derive(Debug, Clone)]
pub struct StoredConfig {
    pub config: SsoConfig,
    pub revision: i64,
    pub updated_at: u64,
    pub updated_by: Option<String>,
}

/// The OIDC client secret. Never serialized, never logged.
pub struct SecretRecord {
    secret: String,
    previous_secret: Option<String>,
    previous_expires_at: Option<u64>,
    updated_at: u64,
}

impl SecretRecord {
    pub fn current(&self) -> &str {
        &self.secret
    }

    /// Previous secret while the rotation grace window is open.
    pub fn previous_if_valid(&self, now: u64) -> Option<&str> {
        match (&self.previous_secret, self.previous_expires_at) {
            (Some(secret), Some(expires_at)) if expires_at > now => Some(secret),
            _ => None,
        }
    }

    pub fn updated_at(&self) -> u64 {
        self.updated_at
    }
}

impl std::fmt::Debug for SecretRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretRecord")
            .field("secret", &"***")
            .field(
                "previous_secret",
                &self.previous_secret.as_ref().map(|_| "***"),
            )
            .field("previous_expires_at", &self.previous_expires_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct NewSession {
    pub id: String,
    /// SHA-256 of the `su_` token; the token itself is never stored.
    pub token_sha256: Vec<u8>,
    pub sub: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub groups_json: String,
    /// Whether the IdP asserted a verified email. Email-based role rules
    /// depend on this, and the decision is made per request.
    pub email_verified: bool,
    /// Where to send the browser after the one-time exchange.
    pub return_to: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionRow {
    pub id: String,
    pub sub: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub groups_json: String,
    pub email_verified: bool,
    pub return_to: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub last_seen_at: u64,
    pub revoked_at: Option<u64>,
    pub exchange_used_at: Option<u64>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PendingAuth {
    pub state: String,
    pub nonce: String,
    pub verifier: String,
    pub return_to: String,
    pub created_at: u64,
    pub expires_at: u64,
}

/// Device-code CLI sign-in request. `approved_at` plus the identity columns
/// are filled when a signed-in browser confirms the user code; the CLI's
/// session is only created when it polls, so no token is ever stored here.
#[derive(Debug, Clone)]
pub struct CliPending {
    pub user_code: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub approved_at: Option<u64>,
    pub sub: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
    pub groups_json: Option<String>,
    pub email_verified: Option<bool>,
}

/// One audit row to write.
#[derive(Debug, Clone, Copy, Default)]
pub struct AuditEvent<'a> {
    pub kind: &'a str,
    pub sub: Option<&'a str>,
    pub email: Option<&'a str>,
    pub ip: Option<&'a str>,
    pub ua: Option<&'a str>,
    pub detail: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct AuditRow {
    pub at: u64,
    pub kind: String,
    pub sub: Option<String>,
    pub email: Option<String>,
    pub ip: Option<String>,
    pub ua: Option<String>,
    pub detail: Option<String>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn truncate_chars(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

fn session_from_row(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<SessionRow> {
    Ok(SessionRow {
        id: row.try_get("id")?,
        sub: row.try_get("sub")?,
        email: row.try_get("email")?,
        name: row.try_get("name")?,
        groups_json: row.try_get("groups_json")?,
        email_verified: row.try_get("email_verified")?,
        return_to: row.try_get("return_to")?,
        created_at: row.try_get::<i64, _>("created_at")? as u64,
        expires_at: row.try_get::<i64, _>("expires_at")? as u64,
        last_seen_at: row.try_get::<i64, _>("last_seen_at")? as u64,
        revoked_at: row
            .try_get::<Option<i64>, _>("revoked_at")?
            .map(|v| v as u64),
        exchange_used_at: row
            .try_get::<Option<i64>, _>("exchange_used_at")?
            .map(|v| v as u64),
        ip: row.try_get("ip")?,
        user_agent: row.try_get("user_agent")?,
    })
}

fn cli_pending_from_row(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<CliPending> {
    Ok(CliPending {
        user_code: row.try_get("user_code")?,
        created_at: row.try_get::<i64, _>("created_at")? as u64,
        expires_at: row.try_get::<i64, _>("expires_at")? as u64,
        approved_at: row
            .try_get::<Option<i64>, _>("approved_at")?
            .map(|v| v as u64),
        sub: row.try_get("sub")?,
        email: row.try_get("email")?,
        name: row.try_get("name")?,
        groups_json: row.try_get("groups_json")?,
        email_verified: row.try_get::<Option<bool>, _>("email_verified")?,
    })
}

impl AuthStore {
    /// Open (creating if needed) the auth store. The file is owner-only from
    /// creation; sidecars are tightened after migrations materialize them.
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("create auth data dir: {}", parent.display()))?;
        }
        if !path.exists() {
            // Create empty (valid) database with 0600 before SQLite opens it,
            // so there is no window where the file exists with a umask mode.
            common::sensitive_file::write_owner_only(path, b"")
                .with_context(|| format!("create auth database: {}", path.display()))?;
        }

        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .busy_timeout(Duration::from_secs(5))
            .foreign_keys(true)
            .statement_cache_capacity(64);
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await
            .context("open auth database")?;

        sqlx::migrate!("./auth_migrations")
            .run(&pool)
            .await
            .context("run auth database migrations")?;

        for (failed, error) in common::sensitive_file::ensure_sqlite_owner_only(path) {
            tracing::warn!(
                path = %failed.display(),
                error = %error,
                "could not restrict auth database file mode to 0600"
            );
        }

        Ok(Self {
            pool,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    // ---------------------------------------------------------------- config

    pub async fn load_config(&self) -> anyhow::Result<Option<StoredConfig>> {
        let row = sqlx::query(
            "SELECT enabled, issuer, client_id, public_base_url, scopes, groups_claim, \
             token_auth_method, role_rules, session_ttl_secs, idle_timeout_secs, \
             allow_insecure_issuer, revision, updated_at, updated_by \
             FROM sso_config WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .context("load SSO config")?;

        let Some(row) = row else { return Ok(None) };
        let role_rules_raw: String = row.try_get("role_rules")?;
        let role_rules: Vec<RoleRule> =
            serde_json::from_str(&role_rules_raw).context("parse role_rules")?;
        let method_raw: String = row.try_get("token_auth_method")?;
        let token_auth_method = TokenAuthMethod::parse(&method_raw)
            .with_context(|| format!("unknown token_auth_method {method_raw:?}"))?;

        Ok(Some(StoredConfig {
            config: SsoConfig {
                enabled: row.try_get("enabled")?,
                issuer: row.try_get("issuer")?,
                client_id: row.try_get("client_id")?,
                public_base_url: row.try_get("public_base_url")?,
                scopes: row.try_get("scopes")?,
                groups_claim: row.try_get("groups_claim")?,
                token_auth_method,
                role_rules,
                session_ttl_secs: row.try_get::<i64, _>("session_ttl_secs")? as u64,
                idle_timeout_secs: row.try_get::<i64, _>("idle_timeout_secs")? as u64,
                allow_insecure_issuer: row.try_get("allow_insecure_issuer")?,
            },
            revision: row.try_get("revision")?,
            updated_at: row.try_get::<i64, _>("updated_at")? as u64,
            updated_by: row.try_get("updated_by")?,
        }))
    }

    /// Validate and replace the configuration. Returns the new revision.
    pub async fn save_config(
        &self,
        config: &SsoConfig,
        updated_by: Option<&str>,
    ) -> anyhow::Result<i64> {
        let errors = config.validate();
        if !errors.is_empty() {
            bail!("invalid SSO config: {}", errors.join("; "));
        }
        let role_rules = serde_json::to_string(&config.role_rules)?;
        let now = now_secs() as i64;
        sqlx::query(
            "INSERT INTO sso_config \
             (id, enabled, issuer, client_id, public_base_url, scopes, groups_claim, \
              token_auth_method, role_rules, session_ttl_secs, idle_timeout_secs, \
              allow_insecure_issuer, revision, updated_at, updated_by) \
             VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET \
              enabled = excluded.enabled, issuer = excluded.issuer, \
              client_id = excluded.client_id, public_base_url = excluded.public_base_url, \
              scopes = excluded.scopes, groups_claim = excluded.groups_claim, \
              token_auth_method = excluded.token_auth_method, role_rules = excluded.role_rules, \
              session_ttl_secs = excluded.session_ttl_secs, \
              idle_timeout_secs = excluded.idle_timeout_secs, \
              allow_insecure_issuer = excluded.allow_insecure_issuer, \
              revision = sso_config.revision + 1, updated_at = excluded.updated_at, \
              updated_by = excluded.updated_by",
        )
        .bind(config.enabled)
        .bind(&config.issuer)
        .bind(&config.client_id)
        .bind(&config.public_base_url)
        .bind(&config.scopes)
        .bind(&config.groups_claim)
        .bind(config.token_auth_method.as_str())
        .bind(&role_rules)
        .bind(config.session_ttl_secs as i64)
        .bind(config.idle_timeout_secs as i64)
        .bind(config.allow_insecure_issuer)
        .bind(now)
        .bind(updated_by)
        .execute(&self.pool)
        .await
        .context("save SSO config")?;
        self.config_revision().await
    }

    /// Enable/disable without touching the rest of the configuration.
    /// Returns false when no configuration exists yet.
    pub async fn set_enabled(
        &self,
        enabled: bool,
        updated_by: Option<&str>,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE sso_config SET enabled = ?, revision = revision + 1, \
             updated_at = ?, updated_by = ? WHERE id = 1",
        )
        .bind(enabled)
        .bind(now_secs() as i64)
        .bind(updated_by)
        .execute(&self.pool)
        .await
        .context("toggle SSO")?;
        Ok(result.rows_affected() > 0)
    }

    /// Monotonic config revision; an out-of-process writer bumps it, so a
    /// background poll can notice a CLI `--offline` change.
    pub async fn config_revision(&self) -> anyhow::Result<i64> {
        let revision: Option<i64> =
            sqlx::query_scalar("SELECT revision FROM sso_config WHERE id = 1")
                .fetch_optional(&self.pool)
                .await
                .context("read config revision")?;
        Ok(revision.unwrap_or(0))
    }

    // ---------------------------------------------------------------- secret

    pub async fn load_secret(&self) -> anyhow::Result<Option<SecretRecord>> {
        let row = sqlx::query(
            "SELECT secret, previous_secret, previous_expires_at, updated_at \
             FROM sso_secret WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .context("load SSO client secret")?;
        let Some(row) = row else { return Ok(None) };
        Ok(Some(SecretRecord {
            secret: row.try_get("secret")?,
            previous_secret: row.try_get("previous_secret")?,
            previous_expires_at: row
                .try_get::<Option<i64>, _>("previous_expires_at")?
                .map(|v| v as u64),
            updated_at: row.try_get::<i64, _>("updated_at")? as u64,
        }))
    }

    pub async fn secret_updated_at(&self) -> anyhow::Result<Option<u64>> {
        let updated: Option<i64> =
            sqlx::query_scalar("SELECT updated_at FROM sso_secret WHERE id = 1")
                .fetch_optional(&self.pool)
                .await
                .context("read secret timestamp")?;
        Ok(updated.map(|v| v as u64))
    }

    /// Store a new secret. A different previous value stays usable for
    /// [`SECRET_GRACE_SECS`] so both sides can be rotated without a login
    /// outage. Re-submitting the same value changes nothing.
    pub async fn put_secret(&self, secret: &str) -> anyhow::Result<()> {
        let len = secret.len();
        if !(common::auth_sso::MIN_CLIENT_SECRET_LEN..=common::auth_sso::MAX_CLIENT_SECRET_LEN)
            .contains(&len)
        {
            bail!(
                "client secret must be {}..={} bytes",
                common::auth_sso::MIN_CLIENT_SECRET_LEN,
                common::auth_sso::MAX_CLIENT_SECRET_LEN
            );
        }
        if secret.chars().any(char::is_control) {
            bail!("client secret must not contain control characters");
        }

        let mut tx = self.pool.begin().await?;
        let current: Option<String> =
            sqlx::query_scalar("SELECT secret FROM sso_secret WHERE id = 1")
                .fetch_optional(&mut *tx)
                .await?;
        let (previous, previous_expires_at) = match current {
            Some(old) if old != secret => {
                (Some(old), Some((now_secs() + SECRET_GRACE_SECS) as i64))
            }
            Some(_) => {
                // Same value: keep the existing grace window untouched.
                let existing: Option<(Option<String>, Option<i64>)> = sqlx::query_as(
                    "SELECT previous_secret, previous_expires_at FROM sso_secret WHERE id = 1",
                )
                .fetch_optional(&mut *tx)
                .await?;
                existing.unwrap_or((None, None))
            }
            None => (None, None),
        };
        sqlx::query(
            "INSERT INTO sso_secret (id, secret, previous_secret, previous_expires_at, updated_at) \
             VALUES (1, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET secret = excluded.secret, \
              previous_secret = excluded.previous_secret, \
              previous_expires_at = excluded.previous_expires_at, \
              updated_at = excluded.updated_at",
        )
        .bind(secret)
        .bind(previous)
        .bind(previous_expires_at)
        .bind(now_secs() as i64)
        .execute(&mut *tx)
        .await
        .context("store SSO client secret")?;
        tx.commit().await?;
        Ok(())
    }

    // -------------------------------------------------------------- sessions

    pub async fn insert_session(&self, session: &NewSession) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        let now = now_secs() as i64;
        sqlx::query("DELETE FROM sessions WHERE expires_at < ? OR revoked_at IS NOT NULL")
            .bind(now)
            .execute(&mut *tx)
            .await?;
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE revoked_at IS NULL")
                .fetch_one(&mut *tx)
                .await?;
        if count >= MAX_SESSIONS {
            sqlx::query(
                "DELETE FROM sessions WHERE id IN (\
                 SELECT id FROM sessions WHERE revoked_at IS NULL \
                 ORDER BY last_seen_at ASC LIMIT ?)",
            )
            .bind(count - MAX_SESSIONS + 1)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO sessions \
             (id, token_sha256, sub, email, name, groups_json, email_verified, return_to, \
              created_at, expires_at, last_seen_at, ip, user_agent) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&session.id)
        .bind(&session.token_sha256)
        .bind(&session.sub)
        .bind(&session.email)
        .bind(&session.name)
        .bind(&session.groups_json)
        .bind(session.email_verified)
        .bind(&session.return_to)
        .bind(session.created_at as i64)
        .bind(session.expires_at as i64)
        .bind(session.created_at as i64)
        .bind(&session.ip)
        .bind(&session.user_agent)
        .execute(&mut *tx)
        .await
        .context("insert session")?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn find_session_by_token_hash(
        &self,
        token_sha256: &[u8],
    ) -> anyhow::Result<Option<SessionRow>> {
        let row = sqlx::query(
            "SELECT id, sub, email, name, groups_json, email_verified, return_to, \
             created_at, expires_at, last_seen_at, revoked_at, exchange_used_at, ip, user_agent \
             FROM sessions WHERE token_sha256 = ?",
        )
        .bind(token_sha256)
        .fetch_optional(&self.pool)
        .await
        .context("lookup session")?;
        row.as_ref().map(session_from_row).transpose()
    }

    /// Session by its opaque id (the middleware puts it in `token_id`).
    pub async fn find_session_by_id(&self, id: &str) -> anyhow::Result<Option<SessionRow>> {
        let row = sqlx::query(
            "SELECT id, sub, email, name, groups_json, email_verified, return_to, \
             created_at, expires_at, last_seen_at, revoked_at, exchange_used_at, ip, user_agent \
             FROM sessions WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("lookup session by id")?;
        row.as_ref().map(session_from_row).transpose()
    }

    pub async fn touch_session(&self, id: &str, now: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE sessions SET last_seen_at = ? WHERE id = ?")
            .bind(now as i64)
            .bind(id)
            .execute(&self.pool)
            .await
            .context("touch session")?;
        Ok(())
    }

    /// Consume the one-time browser exchange. False means already used,
    /// revoked, or unknown.
    pub async fn mark_exchange_used(&self, token_sha256: &[u8], now: u64) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE sessions SET exchange_used_at = ? \
             WHERE token_sha256 = ? AND exchange_used_at IS NULL AND revoked_at IS NULL",
        )
        .bind(now as i64)
        .bind(token_sha256)
        .execute(&self.pool)
        .await
        .context("consume exchange ticket")?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn list_sessions(&self, now: u64) -> anyhow::Result<Vec<SessionRow>> {
        let rows = sqlx::query(
            "SELECT id, sub, email, name, groups_json, email_verified, return_to, \
             created_at, expires_at, last_seen_at, revoked_at, exchange_used_at, ip, user_agent \
             FROM sessions WHERE revoked_at IS NULL AND expires_at > ? \
             ORDER BY created_at DESC",
        )
        .bind(now as i64)
        .fetch_all(&self.pool)
        .await
        .context("list sessions")?;
        rows.iter().map(session_from_row).collect()
    }

    pub async fn revoke_session(&self, id: &str) -> anyhow::Result<bool> {
        let result =
            sqlx::query("UPDATE sessions SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL")
                .bind(now_secs() as i64)
                .bind(id)
                .execute(&self.pool)
                .await
                .context("revoke session")?;
        Ok(result.rows_affected() == 1)
    }

    /// Revoke every active session, optionally keeping the caller's own.
    pub async fn revoke_all(&self, keep_id: Option<&str>) -> anyhow::Result<u64> {
        let result = match keep_id {
            Some(keep) => {
                sqlx::query(
                    "UPDATE sessions SET revoked_at = ? WHERE revoked_at IS NULL AND id <> ?",
                )
                .bind(now_secs() as i64)
                .bind(keep)
                .execute(&self.pool)
                .await
            }
            None => {
                sqlx::query("UPDATE sessions SET revoked_at = ? WHERE revoked_at IS NULL")
                    .bind(now_secs() as i64)
                    .execute(&self.pool)
                    .await
            }
        }
        .context("revoke sessions")?;
        Ok(result.rows_affected())
    }

    pub async fn count_active_sessions(&self, now: u64) -> anyhow::Result<i64> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM sessions WHERE revoked_at IS NULL AND expires_at > ?",
        )
        .bind(now as i64)
        .fetch_one(&self.pool)
        .await
        .context("count sessions")
    }

    // --------------------------------------------------------------- pending

    /// Insert a pending authorization, pruning expired ones and keeping the
    /// table bounded (it is never a user-visible list, just a 10-minute queue).
    pub async fn insert_pending(&self, pending: &PendingAuth) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        let now = now_secs() as i64;
        sqlx::query("DELETE FROM pending WHERE expires_at < ?")
            .bind(now)
            .execute(&mut *tx)
            .await?;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pending")
            .fetch_one(&mut *tx)
            .await?;
        if count >= MAX_PENDING {
            sqlx::query(
                "DELETE FROM pending WHERE state IN \
                 (SELECT state FROM pending ORDER BY created_at ASC LIMIT ?)",
            )
            .bind(count - MAX_PENDING + 1)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO pending (state, nonce, verifier, return_to, created_at, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&pending.state)
        .bind(&pending.nonce)
        .bind(&pending.verifier)
        .bind(&pending.return_to)
        .bind(pending.created_at as i64)
        .bind(pending.expires_at as i64)
        .execute(&mut *tx)
        .await
        .context("insert pending authorization")?;
        tx.commit().await?;
        Ok(())
    }

    /// Single-use read: the row is deleted whether or not it is still valid,
    /// so a replayed `state` can never create a second session.
    pub async fn take_pending(&self, state: &str) -> anyhow::Result<Option<PendingAuth>> {
        let row = sqlx::query(
            "DELETE FROM pending WHERE state = ? \
             RETURNING state, nonce, verifier, return_to, created_at, expires_at",
        )
        .bind(state)
        .fetch_optional(&self.pool)
        .await
        .context("consume pending authorization")?;
        let Some(row) = row else { return Ok(None) };
        Ok(Some(PendingAuth {
            state: row.try_get("state")?,
            nonce: row.try_get("nonce")?,
            verifier: row.try_get("verifier")?,
            return_to: row.try_get("return_to")?,
            created_at: row.try_get::<i64, _>("created_at")? as u64,
            expires_at: row.try_get::<i64, _>("expires_at")? as u64,
        }))
    }

    // ------------------------------------------------------- CLI device codes

    /// Start a device-code CLI sign-in. Expired rows are pruned and the table
    /// is capped so a flood cannot grow it without bound.
    pub async fn insert_cli_pending(
        &self,
        device_sha256: &[u8],
        user_code: &str,
        now: u64,
        expires_at: u64,
    ) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM cli_pending WHERE expires_at < ?")
            .bind(now as i64)
            .execute(&mut *tx)
            .await?;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cli_pending")
            .fetch_one(&mut *tx)
            .await?;
        if count >= MAX_CLI_PENDING {
            sqlx::query(
                "DELETE FROM cli_pending WHERE device_sha256 IN \
                 (SELECT device_sha256 FROM cli_pending ORDER BY created_at ASC LIMIT ?)",
            )
            .bind(count - MAX_CLI_PENDING + 1)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO cli_pending (device_sha256, user_code, created_at, expires_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(device_sha256)
        .bind(user_code)
        .bind(now as i64)
        .bind(expires_at as i64)
        .execute(&mut *tx)
        .await
        .context("insert CLI sign-in request")?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn find_cli_pending(
        &self,
        device_sha256: &[u8],
    ) -> anyhow::Result<Option<CliPending>> {
        let row = sqlx::query(
            "SELECT user_code, created_at, expires_at, approved_at, sub, email, name, \
             groups_json, email_verified FROM cli_pending WHERE device_sha256 = ?",
        )
        .bind(device_sha256)
        .fetch_optional(&self.pool)
        .await
        .context("lookup CLI sign-in request")?;
        row.as_ref().map(cli_pending_from_row).transpose()
    }

    /// Confirm a user code with the approving session's identity claims.
    pub async fn approve_cli(
        &self,
        user_code: &str,
        session: &SessionRow,
        now: u64,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE cli_pending SET approved_at = ?, sub = ?, email = ?, name = ?, \
             groups_json = ?, email_verified = ? \
             WHERE user_code = ? AND approved_at IS NULL AND expires_at > ?",
        )
        .bind(now as i64)
        .bind(&session.sub)
        .bind(&session.email)
        .bind(&session.name)
        .bind(&session.groups_json)
        .bind(session.email_verified)
        .bind(user_code)
        .bind(now as i64)
        .execute(&self.pool)
        .await
        .context("approve CLI sign-in request")?;
        Ok(result.rows_affected() == 1)
    }

    /// Single-use claim of an approved request (the poll's last step).
    pub async fn take_cli_pending(
        &self,
        device_sha256: &[u8],
    ) -> anyhow::Result<Option<CliPending>> {
        let row = sqlx::query(
            "DELETE FROM cli_pending WHERE device_sha256 = ? \
             RETURNING user_code, created_at, expires_at, approved_at, sub, email, name, \
             groups_json, email_verified",
        )
        .bind(device_sha256)
        .fetch_optional(&self.pool)
        .await
        .context("consume CLI sign-in request")?;
        row.as_ref().map(cli_pending_from_row).transpose()
    }

    // ----------------------------------------------------------------- audit

    pub async fn audit(&self, event: AuditEvent<'_>) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO auth_events (at, kind, sub, email, ip, ua, detail) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(now_secs() as i64)
        .bind(event.kind)
        .bind(event.sub)
        .bind(event.email)
        .bind(event.ip)
        .bind(event.ua.map(|ua| truncate_chars(ua, 256)))
        .bind(event.detail.map(|detail| truncate_chars(detail, 512)))
        .execute(&self.pool)
        .await
        .context("insert auth audit event")?;
        Ok(())
    }

    pub async fn list_audit(&self, limit: i64) -> anyhow::Result<Vec<AuditRow>> {
        let rows = sqlx::query(
            "SELECT at, kind, sub, email, ip, ua, detail FROM auth_events \
             ORDER BY at DESC, id DESC LIMIT ?",
        )
        .bind(limit.clamp(1, 500))
        .fetch_all(&self.pool)
        .await
        .context("list auth audit events")?;
        rows.into_iter()
            .map(|row| {
                Ok(AuditRow {
                    at: row.try_get::<i64, _>("at")? as u64,
                    kind: row.try_get("kind")?,
                    sub: row.try_get("sub")?,
                    email: row.try_get("email")?,
                    ip: row.try_get("ip")?,
                    ua: row.try_get("ua")?,
                    detail: row.try_get("detail")?,
                })
            })
            .collect()
    }

    /// Retention: older than the window or beyond the row cap.
    pub async fn prune_audit(&self, now: u64) -> anyhow::Result<u64> {
        let cutoff = now.saturating_sub(AUDIT_RETENTION_SECS) as i64;
        let mut removed = sqlx::query("DELETE FROM auth_events WHERE at < ?")
            .bind(cutoff)
            .execute(&self.pool)
            .await
            .context("prune auth audit events")?
            .rows_affected();
        removed += sqlx::query(
            "DELETE FROM auth_events WHERE id NOT IN \
             (SELECT id FROM auth_events ORDER BY id DESC LIMIT ?)",
        )
        .bind(AUDIT_MAX_ROWS)
        .execute(&self.pool)
        .await
        .context("trim auth audit events")?
        .rows_affected();
        Ok(removed)
    }

    /// Hourly housekeeping: drop expired/revoked sessions, expired pending
    /// authorizations, and audit rows past their retention window.
    pub async fn prune_ephemeral(&self, now: u64) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM sessions WHERE expires_at < ? OR revoked_at IS NOT NULL")
            .bind(now as i64)
            .execute(&self.pool)
            .await
            .context("prune sessions")?;
        sqlx::query("DELETE FROM pending WHERE expires_at < ?")
            .bind(now as i64)
            .execute(&self.pool)
            .await
            .context("prune pending authorizations")?;
        sqlx::query("DELETE FROM cli_pending WHERE expires_at < ?")
            .bind(now as i64)
            .execute(&self.pool)
            .await
            .context("prune CLI sign-in requests")?;
        self.prune_audit(now).await?;
        Ok(())
    }
}
