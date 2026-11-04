use super::*;

/// Build a response through the Unix variant — no daemon needed; handlers only
/// touch `status()` / `text()` / `json()`, which the Unix transport serves
/// from an in-memory buffer.
fn unix_resp(status: reqwest::StatusCode, body: &str) -> ApiResponse {
    ApiResponse::Unix {
        status,
        body: bytes::Bytes::from(body.to_owned()),
    }
}

// --- api_error_from_body ---

#[test]
fn api_error_prefers_server_json_message() {
    let err = api_error_from_body(
        reqwest::StatusCode::UNAUTHORIZED,
        r#"{"message":"invalid token"}"#,
    );
    assert_eq!(err.to_string(), "invalid token");
}

#[test]
fn api_error_includes_status_and_body_fallback() {
    let err = api_error_from_body(reqwest::StatusCode::BAD_GATEWAY, "upstream connect error");
    let msg = err.to_string();
    assert!(msg.starts_with("request failed (HTTP 502"), "{msg}");
    assert!(msg.contains("upstream connect error"), "{msg}");
}

#[test]
fn api_error_without_body_states_the_status_only() {
    let err = api_error_from_body(reqwest::StatusCode::UNAUTHORIZED, "  \n");
    assert_eq!(err.to_string(), "request failed (HTTP 401 Unauthorized)");
}

// --- check_resp ---

#[tokio::test]
async fn check_resp_ok_on_success() {
    check_resp(unix_resp(reqwest::StatusCode::OK, ""))
        .await
        .unwrap();
}

#[tokio::test]
async fn check_resp_fails_with_server_message() {
    let err = check_resp(unix_resp(
        reqwest::StatusCode::UNAUTHORIZED,
        r#"{"message":"invalid token"}"#,
    ))
    .await
    .unwrap_err();
    assert_eq!(err.to_string(), "invalid token");
}

#[tokio::test]
async fn check_resp_json_message_wins_over_status_phrase() {
    let err = check_resp(unix_resp(
        reqwest::StatusCode::NOT_FOUND,
        r#"{"message":"program not found"}"#,
    ))
    .await
    .unwrap_err();
    assert_eq!(err.to_string(), "program not found");
}

#[tokio::test]
async fn check_resp_friendly_messages_for_empty_bodies() {
    let err = check_resp(unix_resp(reqwest::StatusCode::FORBIDDEN, ""))
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "Action Forbidden (403).");

    let err = check_resp(unix_resp(reqwest::StatusCode::NOT_FOUND, ""))
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "Resource Not Found (404).");

    let err = check_resp(unix_resp(reqwest::StatusCode::INTERNAL_SERVER_ERROR, ""))
        .await
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Failed with status: 500 Internal Server Error"
    );
}

#[tokio::test]
async fn check_resp_fails_with_raw_body() {
    let err = check_resp(unix_resp(reqwest::StatusCode::BAD_GATEWAY, "upstream down"))
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "upstream down");
}
