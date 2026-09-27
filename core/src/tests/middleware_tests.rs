use super::*;

#[test]
fn exempt_paths() {
    assert!(is_auth_exempt("/health"));
    assert!(is_auth_exempt("/api/v1/auth/status"));
    assert!(!is_auth_exempt("/metrics"));
    assert!(!is_auth_exempt("/api/v1/auth/login"));
    assert!(!is_auth_exempt("/api/v1/programs"));
}

#[test]
fn extract_bearer_and_query() {
    let mut h = HeaderMap::new();
    h.insert(
        header::AUTHORIZATION,
        header::HeaderValue::from_static("Bearer abc"),
    );
    assert_eq!(extract_token(&h, None).as_deref(), Some("abc"));
    assert_eq!(
        extract_token(&HeaderMap::new(), Some("token=xyz&id=1")).as_deref(),
        Some("xyz")
    );
}
