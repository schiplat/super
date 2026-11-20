//! OIDC client against a fake IdP (wiremock): discovery validation,
//! authorization URL shape, code exchange incl. rotation grace, and ID-token
//! verification incl. one-shot JWKS refetch.
//!
//! The RSA key below is a throwaway generated for this test and used nowhere
//! else. Never reuse it.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use common::auth_sso::{SsoConfig, TokenAuthMethod};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::{Value, json};
use super_core::auth::sso::client::{OidcClient, OidcError};
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

#[path = "fixtures/test_key.rs"]
mod test_key;

use test_key::{TEST_JWK_N, TEST_RSA_KEY};

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn config(issuer: &str) -> Arc<SsoConfig> {
    Arc::new(SsoConfig {
        enabled: true,
        issuer: issuer.into(),
        client_id: "super".into(),
        public_base_url: "http://127.0.0.1:9002".into(),
        scopes: "openid profile email".into(),
        groups_claim: "groups".into(),
        token_auth_method: TokenAuthMethod::ClientSecretBasic,
        role_rules: Vec::new(),
        session_ttl_secs: 12 * 60 * 60,
        idle_timeout_secs: 60 * 60,
        allow_insecure_issuer: true,
    })
}

fn jwks(kid: &str) -> Value {
    json!({
        "keys": [{
            "kty": "RSA",
            "kid": kid,
            "use": "sig",
            "alg": "RS256",
            "n": TEST_JWK_N,
            "e": "AQAB"
        }]
    })
}

fn discovery_doc(issuer: &str) -> Value {
    json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/authorize"),
        "token_endpoint": format!("{issuer}/token"),
        "jwks_uri": format!("{issuer}/jwks"),
        "code_challenge_methods_supported": ["S256"],
        "grant_types_supported": ["authorization_code"],
        "response_types_supported": ["code"],
        "token_endpoint_auth_methods_supported": ["client_secret_basic"]
    })
}

async fn mount_discovery(server: &MockServer, body: Value) {
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

async fn mount_jwks(server: &MockServer, kid: &str) {
    Mock::given(method("GET"))
        .and(path("/jwks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(jwks(kid)))
        .mount(server)
        .await;
}

fn claims(issuer: &str, audience: &str, nonce: &str) -> Value {
    json!({
        "iss": issuer,
        "aud": audience,
        "sub": "u1",
        "exp": now() + 300,
        "iat": now(),
        "nonce": nonce,
        "email": "ops@example.com",
        "email_verified": true,
        "name": "Ops",
        "groups": ["admins", "ops"]
    })
}

fn sign(claims: &Value, kid: &str) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.into());
    let key = EncodingKey::from_rsa_pem(TEST_RSA_KEY.as_bytes()).unwrap();
    jsonwebtoken::encode(&header, claims, &key).unwrap()
}

#[tokio::test]
async fn discovery_authorization_url_and_identity() {
    let server = MockServer::start().await;
    let issuer = server.uri();
    mount_discovery(&server, discovery_doc(&issuer)).await;
    mount_jwks(&server, "k1").await;

    let client = OidcClient::new(config(&issuer), "client-secret", None).unwrap();
    let redirect = "http://127.0.0.1:9002/api/v1/auth/sso/callback";
    let url = client
        .authorization_url("state-1", "nonce-1", "challenge-1", redirect)
        .await
        .unwrap();

    assert!(url.starts_with(&format!("{issuer}/authorize?")), "{url}");
    for needle in [
        "response_type=code",
        "client_id=super",
        "state=state-1",
        "nonce=nonce-1",
        "code_challenge=challenge-1",
        "code_challenge_method=S256",
        "scope=openid+profile+email",
    ] {
        assert!(url.contains(needle), "missing {needle} in {url}");
    }
    assert!(
        url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A9002%2F"),
        "{url}"
    );

    let token = sign(&claims(&issuer, "super", "nonce-1"), "k1");
    let identity = client.verify_id_token(&token, "nonce-1").await.unwrap();
    assert_eq!(identity.sub, "u1");
    assert_eq!(identity.email.as_deref(), Some("ops@example.com"));
    assert!(identity.email_verified);
    assert_eq!(identity.name.as_deref(), Some("Ops"));
    assert_eq!(identity.groups, vec!["admins", "ops"]);
}

#[tokio::test]
async fn code_exchange_sends_basic_auth_and_verifier() {
    let server = MockServer::start().await;
    let issuer = server.uri();
    mount_discovery(&server, discovery_doc(&issuer)).await;

    let basic = BASE64_STANDARD.encode("super:client-secret");
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(header("authorization", format!("Basic {basic}")))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=code-1"))
        .and(body_string_contains("code_verifier=verifier-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id_token": sign(&claims(&issuer, "super", "n"), "k1"),
            "token_type": "Bearer",
            "expires_in": 3600
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = OidcClient::new(config(&issuer), "client-secret", None).unwrap();
    let response = client
        .exchange_code("code-1", "verifier-1", "http://127.0.0.1:9002/cb")
        .await
        .unwrap();
    assert!(response.id_token.is_some());
    server.verify().await;
}

#[tokio::test]
async fn rotation_grace_retries_with_the_previous_secret() {
    let server = MockServer::start().await;
    let issuer = server.uri();
    mount_discovery(&server, discovery_doc(&issuer)).await;

    let new_basic = BASE64_STANDARD.encode("super:new-secret");
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(header("authorization", format!("Basic {new_basic}")))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "error": "invalid_client",
            "error_description": "unknown client secret"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let old_basic = BASE64_STANDARD.encode("super:old-secret");
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(header("authorization", format!("Basic {old_basic}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id_token": sign(&claims(&issuer, "super", "n"), "k1")
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = OidcClient::new(config(&issuer), "new-secret", Some("old-secret".into())).unwrap();
    let response = client
        .exchange_code("code-1", "verifier-1", "http://127.0.0.1:9002/cb")
        .await
        .unwrap();
    assert!(response.id_token.is_some());
    server.verify().await;
}

#[tokio::test]
async fn invalid_client_without_previous_secret_is_an_idp_error() {
    let server = MockServer::start().await;
    let issuer = server.uri();
    mount_discovery(&server, discovery_doc(&issuer)).await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "error": "invalid_client"
        })))
        .mount(&server)
        .await;

    let client = OidcClient::new(config(&issuer), "new-secret", None).unwrap();
    let error = client
        .exchange_code("code-1", "verifier-1", "http://127.0.0.1:9002/cb")
        .await
        .unwrap_err();
    match error {
        OidcError::Idp { error, .. } => assert_eq!(error, "invalid_client"),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn wrong_nonce_expired_or_wrong_audience_are_rejected() {
    let server = MockServer::start().await;
    let issuer = server.uri();
    mount_discovery(&server, discovery_doc(&issuer)).await;
    mount_jwks(&server, "k1").await;

    let client = OidcClient::new(config(&issuer), "client-secret", None).unwrap();

    let wrong_nonce = sign(&claims(&issuer, "super", "other"), "k1");
    assert!(matches!(
        client.verify_id_token(&wrong_nonce, "nonce-1").await,
        Err(OidcError::InvalidToken(_))
    ));

    let mut expired = claims(&issuer, "super", "nonce-1");
    expired["exp"] = json!(now() - 300);
    assert!(matches!(
        client
            .verify_id_token(&sign(&expired, "k1"), "nonce-1")
            .await,
        Err(OidcError::InvalidToken(_))
    ));

    let wrong_aud = sign(&claims(&issuer, "someone-else", "nonce-1"), "k1");
    assert!(matches!(
        client.verify_id_token(&wrong_aud, "nonce-1").await,
        Err(OidcError::InvalidToken(_))
    ));
}

#[tokio::test]
async fn symmetric_algorithms_are_rejected() {
    let server = MockServer::start().await;
    let issuer = server.uri();
    mount_discovery(&server, discovery_doc(&issuer)).await;
    mount_jwks(&server, "k1").await;

    let header = Header::new(Algorithm::HS256);
    let key = EncodingKey::from_secret(b"client-secret");
    let token = jsonwebtoken::encode(&header, &claims(&issuer, "super", "nonce-1"), &key).unwrap();

    let client = OidcClient::new(config(&issuer), "client-secret", None).unwrap();
    match client.verify_id_token(&token, "nonce-1").await {
        Err(OidcError::InvalidToken(reason)) => {
            assert!(reason.contains("not allowed"), "{reason}");
        }
        other => panic!("expected an algorithm rejection, got {other:?}"),
    }
}

/// One JWKS refetch is allowed for an unknown kid, then the rate limit holds
/// even if the token keeps naming a key we do not have.
#[tokio::test]
async fn unknown_kid_refetches_once_then_is_rate_limited() {
    struct Sequence {
        calls: Arc<AtomicUsize>,
        first: Value,
        later: Value,
    }

    impl Respond for Sequence {
        fn respond(&self, _request: &Request) -> ResponseTemplate {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            let body = if call == 0 {
                self.first.clone()
            } else {
                self.later.clone()
            };
            ResponseTemplate::new(200).set_body_json(body)
        }
    }

    let server = MockServer::start().await;
    let issuer = server.uri();
    mount_discovery(&server, discovery_doc(&issuer)).await;

    let calls = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .and(path("/jwks"))
        .respond_with(Sequence {
            calls: calls.clone(),
            first: json!({ "keys": [] }),
            later: jwks("k1"),
        })
        .mount(&server)
        .await;

    let client = OidcClient::new(config(&issuer), "client-secret", None).unwrap();
    let token = sign(&claims(&issuer, "super", "nonce-1"), "k1");
    let identity = client.verify_id_token(&token, "nonce-1").await.unwrap();
    assert_eq!(identity.sub, "u1");
    assert_eq!(calls.load(Ordering::SeqCst), 2, "one initial + one refetch");

    // A forged kid with no rotation behind it must not generate more fetches.
    let forged = sign(&claims(&issuer, "super", "nonce-1"), "k9");
    assert!(matches!(
        client.verify_id_token(&forged, "nonce-1").await,
        Err(OidcError::UnknownKid(_))
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 2, "rate limit must hold");
}

#[tokio::test]
async fn discovery_issuer_mismatch_is_rejected() {
    let server = MockServer::start().await;
    let issuer = server.uri();
    let mut doc = discovery_doc(&issuer);
    doc["issuer"] = json!("http://127.0.0.1:1/other");
    mount_discovery(&server, doc).await;

    let client = OidcClient::new(config(&issuer), "client-secret", None).unwrap();
    match client.discovery().await {
        Err(OidcError::Misconfigured(reason)) => {
            assert!(reason.contains("does not match"), "{reason}");
        }
        other => panic!("expected issuer mismatch, got {other:?}"),
    }
}

#[tokio::test]
async fn plain_http_endpoints_outside_loopback_are_rejected() {
    let server = MockServer::start().await;
    let issuer = server.uri();
    let mut doc = discovery_doc(&issuer);
    doc["token_endpoint"] = json!("http://idp.example.com/token");
    mount_discovery(&server, doc).await;

    let client = OidcClient::new(config(&issuer), "client-secret", None).unwrap();
    match client.discovery().await {
        Err(OidcError::Misconfigured(reason)) => {
            assert!(reason.contains("token_endpoint"), "{reason}");
        }
        other => panic!("expected endpoint rejection, got {other:?}"),
    }
}

#[tokio::test]
async fn idp_without_pkce_s256_is_rejected() {
    let server = MockServer::start().await;
    let issuer = server.uri();
    let mut doc = discovery_doc(&issuer);
    doc["code_challenge_methods_supported"] = json!(["plain"]);
    mount_discovery(&server, doc).await;

    let client = OidcClient::new(config(&issuer), "client-secret", None).unwrap();
    match client.discovery().await {
        Err(OidcError::Misconfigured(reason)) => {
            assert!(reason.contains("S256"), "{reason}");
        }
        other => panic!("expected PKCE rejection, got {other:?}"),
    }
}
