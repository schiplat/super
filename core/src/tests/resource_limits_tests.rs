use super::{apply_resource_limits_patch, validate_resource_limits_patch};
use common::ResourceLimits;

#[test]
fn patch_applies_new_limits() {
    let mut existing = None;
    apply_resource_limits_patch(
        &mut existing,
        ResourceLimits {
            cpu_quota: Some(0.5),
            memory_limit: Some(512),
            memory_warn_percent: Some(80),
            memory_warn_headroom: None,
            memory_high: Some(448),
        },
    );
    let limits = existing.unwrap();
    assert_eq!(limits.cpu_quota, Some(0.5));
    assert_eq!(limits.memory_limit, Some(512));
    assert_eq!(limits.memory_warn_percent, Some(80));
    assert_eq!(limits.memory_high, Some(448));
}

#[test]
fn patch_sentinels_clear_fields() {
    let mut existing = Some(ResourceLimits {
        cpu_quota: Some(0.5),
        memory_limit: Some(512),
        memory_warn_percent: Some(80),
        memory_warn_headroom: None,
        memory_high: Some(448),
    });
    apply_resource_limits_patch(
        &mut existing,
        ResourceLimits {
            cpu_quota: Some(-1.0),
            memory_limit: Some(0),
            memory_warn_percent: Some(0),
            memory_warn_headroom: None,
            memory_high: Some(0),
        },
    );
    assert!(existing.is_none());
}

#[test]
fn patch_allows_removal_sentinels_in_validation() {
    validate_resource_limits_patch(&ResourceLimits {
        cpu_quota: Some(-1.0),
        memory_limit: Some(0),
        memory_warn_percent: Some(0),
        memory_warn_headroom: None,
        memory_high: None,
    })
    .unwrap();
}
