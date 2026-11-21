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

fn json_request(method: &str, uri: &str, token: Option<&str>, body: Value) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

async fn cli_poll(app: &Router, device_code: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/auth/sso/cli/poll",
            None,
            json!({ "device_code": device_code }),
        ))
        .await
        .unwrap();
    let status = response.status();
    (status, json_body(response).await)
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

async fn mount_discovery_with_end_session(server: &MockServer) {
    let doc = json!({
        "issuer": server.uri(),
        "authorization_endpoint": format!("{}/authorize", server.uri()),
        "token_endpoint": format!("{}/token", server.uri()),
        "jwks_uri": format!("{}/jwks", server.uri()),
        "end_session_endpoint": format!("{}/end-session", server.uri()),
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

/// All Set-Cookie headers of a response (the callback sets two).
fn set_cookies(response: &axum::response::Response) -> Vec<String> {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(str::to_string)
        .collect()
}

fn cookie_value<'a>(cookies: &'a [String], name: &str) -> Option<&'a str> {
    cookies.iter().find_map(|cookie| {
        cookie
            .split(';')
            .next()
            .and_then(|pair| pair.trim().strip_prefix(&format!("{name}=")))
    })
}

/// A back-channel logout token (`OpenID Connect Back-Channel Logout`):
/// `sub` for a user revocation, `sid` only when the provider cannot name one.
fn logout_token(issuer: &str, sub: Option<&str>, iat_offset: i64, extra: Value) -> Value {
    let mut claims = json!({
        "iss": issuer,
        "aud": "super",
        "iat": now() + iat_offset,
        "events": {
            "http://schemas.openid.net/event/backchannel-logout": {}
        }
    });
    if let Some(sub) = sub {
        claims["sub"] = json!(sub);
    } else {
        claims["sid"] = json!("provider-session-1");
    }
    if !extra.is_null() {
        for (key, value) in extra.as_object().unwrap() {
            claims[key.as_str()] = value.clone();
        }
    }
    claims
}

async fn post_backchannel(app: &Router, logout_token: Option<&str>) -> (StatusCode, Value) {
    let body = match logout_token {
        Some(token) => format!("logout_token={token}"),
        None => String::new(),
    };
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/sso/backchannel-logout")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

async fn get_logout(app: &Router, hint: Option<&str>) -> axum::response::Response {
    let mut builder = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/sso/logout");
    if let Some(hint) = hint {
        builder = builder.header(header::COOKIE, format!("super_sso_hint={hint}"));
    }
    app.clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap()
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
    assert_eq!(header_string(&response, header::LOCATION), "/login?sso=ok");
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
    let state = AuthState::with_sso(None, Some(runtime), AuthGate::Sso, false, None);
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

    // `/auth/me` is the gateway-agnostic profile endpoint the shell uses.
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/auth/me", &admin_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let me = json_body(response).await;
    assert_eq!(me["role"], "admin");
    assert_eq!(me["name"], "u1");

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

    // The profile endpoint reports the viewer role without admin access.
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/auth/me", &viewer_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_body(response).await["role"], "viewer");

    // CLI device-code sign-in: start -> pending -> approve -> poll -> session.
    let response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/auth/sso/cli/start",
            None,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let start = json_body(response).await;
    let device_code = start["device_code"].as_str().unwrap().to_string();
    let user_code = start["user_code"].as_str().unwrap().to_string();
    assert!(
        start["verification_url"]
            .as_str()
            .unwrap()
            .ends_with(&format!("/auth/cli?code={user_code}")),
        "{start}"
    );

    let (status, body) = cli_poll(&app, &device_code).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "pending");

    let response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/auth/sso/cli/approve",
            Some(&admin_token),
            json!({ "user_code": user_code }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let (status, body) = cli_poll(&app, &device_code).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["user"]["role"], "admin");
    let cli_token = body["token"].as_str().unwrap().to_string();
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/programs", &cli_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // The device code is single-use.
    let (status, _) = cli_poll(&app, &device_code).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

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
        "/login?auth_error=state"
    );
}

/// Loopback dev mode: no gate. `/auth/status` keeps its pre-SSO 404 contract
/// (the shell infers "no auth" from it), while non-exempt routes still receive
/// the local root identity so admin endpoints work before any gate exists.
#[tokio::test]
async fn no_gate_mode_keeps_the_open_instance_contract() {
    let app = {
        let api = Router::new().route("/api/v1/programs", axum::routing::get(|| async { "ok" }));
        install_core_auth(
            api,
            AuthState::with_sso(None, None, AuthGate::None, false, None),
        )
    };

    let response = app
        .clone()
        .oneshot(get("/api/v1/auth/status"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = app.clone().oneshot(get("/api/v1/programs")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app.clone().oneshot(get("/api/v1/auth/me")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_body(response).await["token_id"], "root");
}

/// RP-initiated logout: a fresh hint cookie plus an advertised
/// `end_session_endpoint` redirects the browser to the provider; without one
/// of them the sign-out degrades to the local landing page. The probe reports
/// the end-session availability either way.
#[tokio::test]
async fn rp_logout_hops_to_the_provider_only_with_a_fresh_hint() {
    let idp = MockServer::start().await;
    mount_discovery_with_end_session(&idp).await;
    mount_jwks(&idp).await;
    let nonce = Arc::new(Mutex::new(String::new()));
    let subject = Arc::new(Mutex::new(("u1".to_string(), vec!["admins".to_string()])));
    mount_token(&idp, nonce.clone(), subject.clone()).await;

    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(AuthStore::open(&dir.path().join("auth.db")).await.unwrap());
    store.put_secret("client-secret").await.unwrap();
    store.save_config(&config(&idp.uri()), None).await.unwrap();
    let runtime = Arc::new(SsoRuntime::load(store.clone()).await.unwrap());
    let state = AuthState::with_sso(None, Some(runtime.clone()), AuthGate::Sso, false, None);
    let app = {
        let api = Router::new().route("/api/v1/programs", axum::routing::get(|| async { "ok" }));
        install_core_auth(api, state)
    };

    let (admin_token, _cookie, _body) = login(&app, &nonce, &subject, "u1", &["admins"]).await;

    // The probe reports what the settings page shows: issuer match, endpoints,
    // and the end-session availability.
    let response = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/auth/sso/test", &admin_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let probe = json_body(response).await;
    assert_eq!(probe["ok"], true);
    assert_eq!(probe["issuer_match"], true);
    assert_eq!(probe["token_endpoint"], format!("{}/token", idp.uri()));
    assert_eq!(probe["end_session_available"], true);
    assert_eq!(probe["client_auth"], "client_secret_basic");

    // The logout redirect needs the hint cookie, which the callback set
    // alongside the exchange ticket — one more login to capture both cookies.
    let (oauth_state, nonce_value) = begin_login(&app).await;
    *nonce.lock().unwrap() = nonce_value;
    let response = app
        .clone()
        .oneshot(get(&format!(
            "/api/v1/auth/sso/callback?code=code-1&state={oauth_state}"
        )))
        .await
        .unwrap();
    let cookies = set_cookies(&response);
    let hint = cookie_value(&cookies, "super_sso_hint")
        .unwrap()
        .to_string();
    assert!(hint.contains('.'), "the hint carries the raw id_token");

    let response = get_logout(&app, Some(&hint)).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(header_string(&response, header::CACHE_CONTROL), "no-store");
    let location = header_string(&response, header::LOCATION);
    assert!(
        location.starts_with(&format!("{}/end-session?", idp.uri())),
        "{location}"
    );
    assert_eq!(query_param(&location, "client_id"), "super");
    assert_eq!(query_param(&location, "id_token_hint"), hint);
    assert_eq!(
        query_param(&location, "post_logout_redirect_uri"),
        "http%3A%2F%2F127.0.0.1%3A9002%2Flogin%3Fsignedout%3D1"
    );
    let cleared = set_cookies(&response);
    assert!(
        cleared
            .iter()
            .any(|cookie| cookie.starts_with("super_sso_hint=;") && cookie.contains("Max-Age=0")),
        "{cleared:?}"
    );

    // Without a hint: local sign-out only, still no-store.
    let response = get_logout(&app, None).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(header_string(&response, header::CACHE_CONTROL), "no-store");
    assert_eq!(
        header_string(&response, header::LOCATION),
        "/login?signedout=1"
    );

    // An expired hint degrades the same way (the provider would reject it too).
    let expired = sign(&json!({
        "iss": idp.uri(),
        "aud": "super",
        "sub": "u1",
        "exp": now() - 10,
        "iat": now() - 400,
    }));
    let response = get_logout(&app, Some(&expired)).await;
    assert_eq!(
        header_string(&response, header::LOCATION),
        "/login?signedout=1"
    );

    // SSO off: plain redirect to the login page, no signedout marker.
    let state = AuthState::with_sso(None, None, AuthGate::None, false, None);
    let open_app = {
        let api = Router::new().route("/api/v1/programs", axum::routing::get(|| async { "ok" }));
        install_core_auth(api, state)
    };
    let response = get_logout(&open_app, None).await;
    assert_eq!(header_string(&response, header::LOCATION), "/login");
}

/// Back-channel logout: a signed logout token is the credential. Verified
/// tokens always answer 200 (unknown subjects and sid-only tokens included,
/// no account enumeration); anything that does not verify answers 400.
#[tokio::test]
async fn backchannel_logout_revokes_the_subjects_sessions() {
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
    let state = AuthState::with_sso(None, Some(runtime), AuthGate::Sso, false, None);
    let app = {
        let api = Router::new().route("/api/v1/programs", axum::routing::get(|| async { "ok" }));
        install_core_auth(api, state)
    };

    let (admin_token, _cookie, _body) = login(&app, &nonce, &subject, "u1", &["admins"]).await;
    let (viewer_token, _cookie2, _body2) = login(&app, &nonce, &subject, "u2", &["viewers"]).await;

    async fn alive(app: &Router, token: &str) -> StatusCode {
        app.clone()
            .oneshot(auth_request("GET", "/api/v1/programs", token))
            .await
            .unwrap()
            .status()
    }
    assert_eq!(alive(&app, &admin_token).await, StatusCode::OK);
    assert_eq!(alive(&app, &viewer_token).await, StatusCode::OK);

    // A valid token for u1 revokes exactly that identity's sessions.
    let token = sign(&logout_token(&idp.uri(), Some("u1"), 0, Value::Null));
    let (status, body) = post_backchannel(&app, Some(&token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revoked"], 1);
    assert_eq!(alive(&app, &admin_token).await, StatusCode::UNAUTHORIZED);
    assert_eq!(alive(&app, &viewer_token).await, StatusCode::OK);

    // Replays are harmless: revocation is idempotent.
    let (status, body) = post_backchannel(&app, Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["revoked"], 0);

    // An unknown subject answers 200 as well — no account enumeration.
    let token = sign(&logout_token(&idp.uri(), Some("nobody"), 0, Value::Null));
    let (status, body) = post_backchannel(&app, Some(&token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // sid-only: valid, nothing to map.
    let token = sign(&logout_token(&idp.uri(), None, 0, Value::Null));
    let (status, _) = post_backchannel(&app, Some(&token)).await;
    assert_eq!(status, StatusCode::OK);

    // Rejections: nonce present, event missing, stale or future iat, broken
    // signature, empty body.
    let token = sign(&logout_token(
        &idp.uri(),
        Some("u2"),
        0,
        json!({ "nonce": "n" }),
    ));
    let (status, _) = post_backchannel(&app, Some(&token)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "nonce must reject");
    assert_eq!(alive(&app, &viewer_token).await, StatusCode::OK);

    let token = sign(&logout_token(
        &idp.uri(),
        Some("u2"),
        0,
        json!({ "events": {} }),
    ));
    let (status, _) = post_backchannel(&app, Some(&token)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "missing event must reject");

    let token = sign(&logout_token(&idp.uri(), Some("u2"), -400, Value::Null));
    let (status, _) = post_backchannel(&app, Some(&token)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "stale iat must reject");

    let token = sign(&logout_token(&idp.uri(), Some("u2"), 120, Value::Null));
    let (status, _) = post_backchannel(&app, Some(&token)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "future iat must reject");

    let mut tampered = sign(&logout_token(&idp.uri(), Some("u2"), 0, Value::Null));
    // Flip a mid-signature character: the trailing char can encode only
    // padding bits, so popping it sometimes yields the same bytes.
    let mid = tampered.len() - 40;
    let original = tampered.as_bytes()[mid];
    let replacement = if original == b'A' { b'B' } else { b'A' };
    tampered.replace_range(mid..mid + 1, &(replacement as char).to_string());
    let (status, _) = post_backchannel(&app, Some(&tampered)).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "broken signature must reject"
    );

    let (status, _) = post_backchannel(&app, None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "empty body must reject");

    assert_eq!(alive(&app, &viewer_token).await, StatusCode::OK);
}

/// Named API tokens under the static-secret gate. The secret path is
/// untouched (root/Admin, same 401s); a token is an *additional* credential
/// that authenticates with its own role, so OSS RBAC finally applies to real
/// credentials. The JSON shapes are the plugin contract (`common::auth`)
/// because the CLI `super token` commands consume them unchanged.
#[tokio::test]
async fn api_tokens_authenticate_under_the_secret_gate() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(AuthStore::open(&dir.path().join("auth.db")).await.unwrap());
    let runtime = Arc::new(SsoRuntime::load(store.clone()).await.unwrap());
    let state = AuthState::with_sso(
        Some("oss-secret".into()),
        Some(runtime),
        AuthGate::CoreSecret,
        false,
        Some("config".into()),
    );
    let app = {
        let api = Router::new().route(
            "/api/v1/programs",
            axum::routing::get(|| async { "ok" }).post(|| async { "created" }),
        );
        install_core_auth(api, state)
    };

    // The static-secret path is byte-for-byte unchanged: root/Admin identity,
    // and a wrong secret is still a plain 401.
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/auth/me", "oss-secret"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let me = json_body(response).await;
    assert_eq!(me["role"], "admin");
    assert_eq!(me["token_id"], "root");
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/programs", "not-the-secret"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // Status reports the token surface: CRUD available, no admin token yet,
    // and where the break-glass secret came from.
    let response = app
        .clone()
        .oneshot(get("/api/v1/auth/status"))
        .await
        .unwrap();
    let status = json_body(response).await;
    assert_eq!(status["mode"], "secret");
    assert_eq!(status["token_management"], true);
    assert_eq!(status["has_admin_token"], false);
    assert_eq!(status["secret_source"], "config");

    // Create an operator token: the plaintext is returned exactly once and
    // the record only ever carries the prefix.
    let response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/auth/tokens",
            Some("oss-secret"),
            json!({ "name": "ci", "role": "operator" }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let created = json_body(response).await;
    let operator_token = created["token"].as_str().unwrap().to_string();
    assert!(operator_token.starts_with("su_"), "{created}");
    let operator_id = created["record"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["record"]["role"], "operator");
    assert_eq!(created["record"]["name"], "ci");
    assert!(
        created["record"]["token_prefix"]
            .as_str()
            .unwrap()
            .ends_with('…')
    );

    // The list is a bare array (the CLI deserializes Vec<AuthTokenInfo>).
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/auth/tokens", "oss-secret"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let list = json_body(response).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], operator_id);
    assert!(
        !list.to_string().contains(&operator_token),
        "the token plaintext must never leave the create response"
    );

    // The operator token authenticates with its own role: reads OK, writes
    // still need Operator (this one is Operator, so POST passes), and the
    // token-management plane is Admin-only.
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/programs", &operator_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/programs", &operator_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/auth/tokens", &operator_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "minting is Admin");
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/auth/me", &operator_token))
        .await
        .unwrap();
    let me = json_body(response).await;
    assert_eq!(me["role"], "operator");
    assert_eq!(me["token_id"], operator_id);

    // A viewer token cannot mutate.
    let response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/auth/tokens",
            Some("oss-secret"),
            json!({ "name": "dashboard-readonly", "role": "viewer" }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let viewer_token = json_body(response).await["token"]
        .as_str()
        .unwrap()
        .to_string();
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/programs", &viewer_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(auth_request("POST", "/api/v1/programs", &viewer_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // `has_admin_token` flips with the first active admin token.
    let response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/auth/tokens",
            Some("oss-secret"),
            json!({ "name": "ops2", "role": "admin" }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let admin_token = json_body(response).await["token"]
        .as_str()
        .unwrap()
        .to_string();
    let response = app
        .clone()
        .oneshot(get("/api/v1/auth/status"))
        .await
        .unwrap();
    assert_eq!(json_body(response).await["has_admin_token"], true);
    // An admin token can manage tokens, like the secret can.
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/auth/tokens", &admin_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Revocation is immediate and the token leaves the active list.
    let response = app
        .clone()
        .oneshot(auth_request(
            "DELETE",
            &format!("/api/v1/auth/tokens/{operator_id}"),
            "oss-secret",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/programs", &operator_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/auth/tokens", "oss-secret"))
        .await
        .unwrap();
    let list = json_body(response).await;
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert!(!list.to_string().contains("ci"));
}

/// Token expiry: creation validates the date, the list carries it, and an
/// expired token (seeded directly, since the API refuses past dates) answers
/// 401 immediately.
#[tokio::test]
async fn api_tokens_honour_expiry() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(AuthStore::open(&dir.path().join("auth.db")).await.unwrap());
    let runtime = Arc::new(SsoRuntime::load(store.clone()).await.unwrap());
    let state = AuthState::with_sso(
        Some("oss-secret".into()),
        Some(runtime),
        AuthGate::CoreSecret,
        false,
        None,
    );
    let app = {
        let api = Router::new().route("/api/v1/programs", axum::routing::get(|| async { "ok" }));
        install_core_auth(api, state)
    };

    // A past date is rejected.
    let response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/auth/tokens",
            Some("oss-secret"),
            json!({ "name": "stale", "role": "viewer", "expires_at": now() - 10 }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // A future date is accepted and reported back in the list.
    let response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/auth/tokens",
            Some("oss-secret"),
            json!({ "name": "shortlived", "role": "viewer", "expires_at": now() + 3600 }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let created = json_body(response).await;
    let token = created["token"].as_str().unwrap().to_string();
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/auth/tokens", "oss-secret"))
        .await
        .unwrap();
    let list = json_body(response).await;
    assert_eq!(list[0]["expires_at"], now() + 3600);

    // Still valid: authenticates.
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/programs", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Seed an expired token behind the API's back (the API refuses past
    // dates): authentication ends immediately while the row stays listed for
    // operators to see.
    let expired_token = format!("su_{}", super_core::auth::sso::pkce::random_token());
    store
        .insert_api_token(super_core::auth::store::NewApiToken {
            id: "seeded-expired",
            token_sha256: &super_core::auth::sso::pkce::token_hash(&expired_token),
            token_prefix: "su_deadbeef…",
            name: "seeded-expired",
            role: "viewer",
            created_by: "test",
            created_at: (now() - 10) as u64,
            expires_at: Some((now() - 5) as u64),
        })
        .await
        .unwrap();
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/programs", &expired_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = app
        .clone()
        .oneshot(auth_request("GET", "/api/v1/auth/tokens", "oss-secret"))
        .await
        .unwrap();
    let list = json_body(response).await;
    assert_eq!(
        list.as_array().unwrap().len(),
        2,
        "expired row stays listed"
    );
}
