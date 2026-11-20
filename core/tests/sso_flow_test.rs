//! End-to-end SSO flow over the real router: login redirect, callback,
//! one-time exchange, RBAC on program endpoints, and logout revocation.
//!
//! The IdP is a wiremock server; the RSA key in `fixtures/` is a throwaway
//! generated for these tests.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use common::{RoleRule, RuleMatch, SsoConfig, TokenAuthMethod, UserRole};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::{Value, json};
use super_core::auth::sso::SsoRuntime;
use super_core::auth::{AuthGate, AuthState, AuthStore, install_core_auth};
use tower::util::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request as WmRequest, ResponseTemplate};

#[path = "fixtures/test_key.rs"]
mod test_key;

use test_key::{TEST_JWK_N, TEST_RSA_KEY};

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn config(issuer: &str) -> SsoConfig {
    SsoConfig {
        enabled: true,
        issuer: issuer.into(),
        client_id: "super".into(),
        public_base_url: "http://127.0.0.1:9002".into(),
        scopes: "openid profile email".into(),
        groups_claim: "groups".into(),
        token_auth_method: TokenAuthMethod::ClientSecretBasic,
        role_rules: vec![
            RoleRule {
                r#match: RuleMatch {
                    sub: Some("u1".into()),
                    ..Default::default()
                },
                role: UserRole::Admin,
            },
            RoleRule {
                r#match: RuleMatch {
                    groups_any: Some(vec!["viewers".into()]),
                    ..Default::default()
                },
                role: UserRole::Viewer,
            },
        ],
        session_ttl_secs: 3600,
        idle_timeout_secs: 600,
        allow_insecure_issuer: true,
    }
}

fn sign(claims: &Value) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("k1".into());
    let key = EncodingKey::from_rsa_pem(TEST_RSA_KEY.as_bytes()).unwrap();
    jsonwebtoken::encode(&header, claims, &key).unwrap()
}

async fn mount_discovery(server: &MockServer) {
    let doc = json!({
        "issuer": server.uri(),
        "authorization_endpoint": format!("{}/authorize", server.uri()),
        "token_endpoint": format!("{}/token", server.uri()),
        "jwks_uri": format!("{}/jwks", server.uri()),
        "code_challenge_methods_supported": ["S256"],
        "grant_types_supported": ["authorization_code"],
        "response_types_supported": ["code"],
        "token_endpoint_auth_methods_supported": ["client_secret_basic"]
    });
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(doc))
        .mount(server)
        .await;
}

async fn mount_jwks(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/jwks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "keys": [{
                "kty": "RSA",
                "kid": "k1",
                "use": "sig",
                "alg": "RS256",
                "n": TEST_JWK_N,
                "e": "AQAB"
            }]
        })))
        .mount(server)
        .await;
}

/// The token endpoint signs a token bound to whatever nonce the current login
/// put in the slot, for whatever subject the test selected.
async fn mount_token(
    server: &MockServer,
    nonce: Arc<Mutex<String>>,
    subject: Arc<Mutex<(String, Vec<String>)>>,
) {
    let issuer = server.uri();
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(move |_request: &WmRequest| {
            let nonce = nonce.lock().unwrap().clone();
            let (sub, groups) = subject.lock().unwrap().clone();
            let claims = json!({
                "iss": issuer,
                "aud": "super",
                "sub": sub,
                "exp": now() + 300,
                "iat": now(),
                "nonce": nonce,
                "email": format!("{sub}@example.com"),
                "email_verified": true,
                "name": sub,
                "groups": groups,
            });
            ResponseTemplate::new(200).set_body_json(json!({ "id_token": sign(&claims) }))
        })
        .mount(server)
        .await;
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

fn auth_request(method: &str, uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

fn header_string(response: &axum::response::Response, name: header::HeaderName) -> String {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn query_param(url: &str, key: &str) -> String {
    let query = url.split_once('?').map(|(_, query)| query).unwrap_or("");
    for pair in query.split('&') {
        if let Some((name, value)) = pair.split_once('=')
            && name == key
        {
            return value.to_string();
        }
    }
    panic!("missing {key} in {url}");
}

async fn begin_login(app: &Router) -> (String, String) {
    let response = app
        .clone()
        .oneshot(get("/api/v1/auth/sso/login?return=%2Fprograms"))
        .await
        .unwrap();
    assert!(response.status().is_redirection(), "{}", response.status());
    let location = header_string(&response, header::LOCATION);
    assert!(
        location.contains("code_challenge_method=S256"),
        "{location}"
    );
    assert!(location.contains("response_type=code"), "{location}");
    (
        query_param(&location, "state"),
        query_param(&location, "nonce"),
    )
}

async fn complete_login(app: &Router, oauth_state: &str) -> (String, String) {
    let response = app
        .clone()
        .oneshot(get(&format!(
            "/api/v1/auth/sso/callback?code=code-1&state={oauth_state}"
        )))
        .await
        .unwrap();
    assert!(response.status().is_redirection(), "callback must redirect");
    assert_eq!(header_string(&response, header::LOCATION), "/?sso=ok");
    let cookie = header_string(&response, header::SET_COOKIE);
    assert!(cookie.contains("HttpOnly"), "{cookie}");
    assert!(cookie.contains("SameSite=Lax"), "{cookie}");
    let token = cookie
        .split(';')
        .next()
        .unwrap()
        .trim()
        .strip_prefix("super_sso=")
        .expect("exchange cookie")
        .to_string();
    assert!(token.starts_with("su_"), "{token}");
    (token, cookie)
}

async fn exchange(app: &Router, cookie: &str) -> (StatusCode, Option<Value>) {
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/sso/exchange")
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    if !status.is_success() {
        return (status, None);
    }
    let cleared = header_string(&response, header::SET_COOKIE);
    assert!(cleared.contains("Max-Age=0"), "{cleared}");
    (status, Some(json_body(response).await))
}

/// Run login → callback → exchange for one subject and return the session.
async fn login(
    app: &Router,
    nonce_slot: &Arc<Mutex<String>>,
    subject_slot: &Arc<Mutex<(String, Vec<String>)>>,
    sub: &str,
    groups: &[&str],
) -> (String, String, Value) {
    *subject_slot.lock().unwrap() = (
        sub.to_string(),
        groups.iter().map(|group| group.to_string()).collect(),
    );
    let (oauth_state, nonce) = begin_login(app).await;
    *nonce_slot.lock().unwrap() = nonce;
    let (token, cookie) = complete_login(app, &oauth_state).await;
    let (status, body) = exchange(app, &cookie).await;
    assert_eq!(status, StatusCode::OK, "exchange must succeed");
    (token, cookie, body.unwrap())
}

#[tokio::test]
async fn oidc_login_exchange_rbac_and_logout() {
    let idp = MockServer::start().await;
    mount_discovery(&idp).await;
    mount_jwks(&idp).await;

    let nonce = Arc::new(Mutex::new(String::new()));
    let subject = Arc::new(Mutex::new(("u1".to_string(), vec!["admins".to_string()])));
    mount_token(&idp, nonce.clone(), subject.clone()).await;

    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(AuthStore::open(&dir.path().join("auth.db")).await.unwrap());
    store.put_secret("client-secret").await.unwrap();
    store.save_config(&config(&idp.uri()), None).await.unwrap();
    let runtime = Arc::new(SsoRuntime::load(store.clone()).await.unwrap());
    let state = AuthState::with_sso(None, Some(runtime), AuthGate::Sso, false);
    let app = {
        let api = Router::new().route(
            "/api/v1/programs",
            axum::routing::get(|| async { "ok" }).post(|| async { "created" }),
        );
        install_core_auth(api, state)
    };

    // No session, no API.
    let response = app.clone().oneshot(get("/api/v1/programs")).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // Status drives the login page.
    let response = app
        .clone()
        .oneshot(get("/api/v1/auth/status"))
        .await
        .unwrap();
    let status = json_body(response).await;
    assert_eq!(status["mode"], "sso");
    assert_eq!(status["auth_secret_login_allowed"], false);
    assert_eq!(status["sso"]["enabled"], true);
    assert_eq!(status["sso"]["login_path"], "/api/v1/auth/sso/login");

    // Admin session.
    let (admin_token, admin_cookie, admin_body) =
        login(&app, &nonce, &subject, "u1", &["admins"]).await;
    assert_eq!(admin_body["token"], admin_token);
    assert_eq!(admin_body["return_to"], "/programs");
    assert_eq!(admin_body["user"]["role"], "admin");

    // The exchange cookie is single-use.
    let (replay_status, _) = exchange(&app, &admin_cookie).await;
    assert_eq!(replay_status, StatusCode::UNAUTHORIZED);

    // Admin can read, mutate, and see the config view without the secret.
    for (method, uri) in [
        ("GET", "/api/v1/programs"),
        ("POST", "/api/v1/programs"),
        ("GET", "/api/v1/auth/sso/sessions"),
    ] {
        let response = app
            .clone()
            .oneshot(auth_request(method, uri, &admin_token))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{method} {uri}");
    }
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/auth/sso/config", &admin_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let config_view = json_body(response).await;
    assert_eq!(config_view["configured"], true);
    assert_eq!(config_view["gate"], "sso");
    assert!(
        !config_view.to_string().contains("client-secret"),
        "the client secret must never leave the store: {config_view}"
    );

    // Viewer session: read-only.
    let (viewer_token, _viewer_cookie, viewer_body) =
        login(&app, &nonce, &subject, "u2", &["viewers"]).await;
    assert_eq!(viewer_body["user"]["role"], "viewer");

    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/programs", &viewer_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    for (method, uri) in [
        ("POST", "/api/v1/programs"),
        ("GET", "/api/v1/auth/sso/sessions"),
        ("GET", "/api/v1/auth/sso/config"),
    ] {
        let response = app
            .clone()
            .oneshot(auth_request(method, uri, &viewer_token))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "viewer must not {method} {uri}"
        );
    }

    // Logout revokes immediately.
    let response = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/auth/logout", &admin_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/programs", &admin_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // A forged state never mints a session.
    let response = app
        .clone()
        .oneshot(get("/api/v1/auth/sso/callback?code=x&state=bogus"))
        .await
        .unwrap();
    assert!(response.status().is_redirection(), "{}", response.status());
    assert_eq!(
        header_string(&response, header::LOCATION),
        "/?auth_error=state"
    );
}
