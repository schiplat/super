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

/// The decoder is the exact counterpart of `encodeURIComponent` / the CLI's
/// `percent_encode_component`: `%XX` decodes, a literal `+` stays `+`.
#[test]
fn percent_decode_round_trips_component_encoding() {
    assert_eq!(percent_decode("abc-1.2_3~"), "abc-1.2_3~");
    assert_eq!(percent_decode("a%2Bb"), "a+b");
    assert_eq!(percent_decode("a+b"), "a+b");
    assert_eq!(percent_decode("%26%3D%25"), "&=%");
    assert_eq!(percent_decode("%E4%B8%AD"), "中");
    // Malformed input is kept verbatim instead of failing the request.
    assert_eq!(percent_decode("100%"), "100%");
    assert_eq!(percent_decode("%zz"), "%zz");
    assert_eq!(percent_decode("%2"), "%2");
}
