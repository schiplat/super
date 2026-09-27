use super::ArtifactConfig;

#[test]
fn artifact_timeouts_default_to_60() {
    let art = ArtifactConfig::default();
    assert_eq!(art.download_timeout, 60);
    assert_eq!(art.verify_timeout, 60);
}

#[test]
fn artifact_timeouts_omitted_in_json_use_defaults() {
    let art: ArtifactConfig = serde_json::from_str(
        r#"{
                "source": "https://example.com/a",
                "checksum": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "destination": "/tmp/a"
            }"#,
    )
    .unwrap();
    assert_eq!(art.download_timeout, 60);
    assert_eq!(art.verify_timeout, 60);
    assert_eq!(art.restart_policy, "immediate");
    assert!(!art.extract);
}

#[test]
fn artifact_timeouts_explicit_zero_and_custom() {
    let zero: ArtifactConfig = serde_json::from_str(
        r#"{
                "source": "https://example.com/a",
                "checksum": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "destination": "/tmp/a",
                "download_timeout": 0,
                "verify_timeout": 0
            }"#,
    )
    .unwrap();
    assert_eq!(zero.download_timeout, 0);
    assert_eq!(zero.verify_timeout, 0);

    let custom: ArtifactConfig = serde_json::from_str(
        r#"{
                "source": "https://example.com/a",
                "checksum": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "destination": "/tmp/a",
                "download_timeout": 1200,
                "verify_timeout": 90
            }"#,
    )
    .unwrap();
    assert_eq!(custom.download_timeout, 1200);
    assert_eq!(custom.verify_timeout, 90);
}
