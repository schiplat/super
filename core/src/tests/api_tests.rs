use super::*;
use axum::body::Body;

fn req_with(content_type: &str, body: &str) -> Request {
    Request::builder()
        .method("PUT")
        .header(axum::http::header::CONTENT_TYPE, content_type)
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn stack_body_json_default() {
    let body = r#"{"services":[{"name":"web","command":"/bin/true"}]}"#;
    let StackBody(stack) = StackBody::from_request(req_with("application/json", body), &())
        .await
        .unwrap();
    assert_eq!(stack.services.len(), 1);
    assert_eq!(stack.services[0].name.as_deref(), Some("web"));
}

#[tokio::test]
async fn stack_body_toml_content_type() {
    let body = r#"
[[services]]
name = "web"
command = "/bin/true"
health_check = { type = "tcp", port = 8080 }
"#;
    let StackBody(stack) = StackBody::from_request(req_with("application/toml", body), &())
        .await
        .unwrap();
    let web = &stack.services[0];
    assert_eq!(web.name.as_deref(), Some("web"));
    assert!(matches!(
        web.health_check,
        Some(common::HealthCheck::Tcp { .. })
    ));
}

#[tokio::test]
async fn stack_body_toml_error_carries_location() {
    let body = "[[services]]\nname = \"web\"\ncommand = [1, 2]\n";
    let err = StackBody::from_request(req_with("application/toml", body), &())
        .await
        .unwrap_err();
    let msg = err.1.to_string();
    assert!(msg.starts_with("request body:"), "{msg}");
    assert!(msg.contains("line 3") || msg.contains('3'), "{msg}");
}

#[tokio::test]
async fn stack_body_toml_rejects_unknown_field() {
    let body = "[[services]]\nname = \"web\"\ncommand = \"/bin/true\"\nbogus_field = 1\n";
    let err = StackBody::from_request(req_with("application/toml", body), &())
        .await
        .unwrap_err();
    assert!(err.1.to_string().contains("bogus_field"));
}

#[tokio::test]
async fn stack_body_json_error_keeps_field_path() {
    let body = r#"{"services":[{"name":"web","command":"/bin/true","bogus":1}]}"#;
    let err = StackBody::from_request(req_with("application/json", body), &())
        .await
        .unwrap_err();
    let msg = err.1.to_string();
    assert!(
        msg.contains("services[0]") && msg.contains("bogus"),
        "{msg}"
    );
}

// ---- request timeout ([server].request_timeout_secs) ----

/// A service that never answers, to exercise the wall-clock budget.
#[derive(Clone, Copy)]
struct NeverService;

impl tower::Service<Request> for NeverService {
    type Response = Response;
    type Error = std::convert::Infallible;
    type Future = std::future::Pending<Result<Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: Request) -> Self::Future {
        std::future::pending()
    }
}

#[tokio::test]
async fn request_timeout_answers_408_with_json_envelope() {
    use tower::Layer as _;
    let mut svc = request_timeout_layer(1).unwrap().layer(NeverService);
    let req = Request::builder().body(Body::empty()).unwrap();
    let resp = tower::Service::call(&mut svc, req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::REQUEST_TIMEOUT);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "error");
    assert_eq!(json["message"], "request timeout");
}

#[tokio::test]
async fn request_timeout_zero_disables_limit() {
    assert!(request_timeout_layer(0).is_none());
}

#[test]
fn config_default_request_timeout_is_30s() {
    let cfg = common::config::ServerSection::default();
    assert_eq!(cfg.request_timeout_secs, 30);
}
