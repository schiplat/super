use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Linux cgroup resource limits (CPU / memory). Enforced when the isolation plugin is loaded.
///
/// Input units are user-friendly: `cpu_quota` in cores, memory fields in MB.
/// Event payloads and kernel files keep byte/percent granularity.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, ToSchema)]
pub struct ResourceLimits {
    /// CPU quota in cores (`1.0` = one full core, `0.5` = half a core, `2.0` = two cores).
    #[schema(example = 1.0)]
    pub cpu_quota: Option<f32>,
    /// Memory hard limit in MB (binary, 1 MB = 1024² bytes).
    #[schema(example = 512)]
    pub memory_limit: Option<u64>,
    /// Pre-kill warning threshold as a percentage of `memory_limit`
    /// (1–100; `Some(0)` disables). Defaults to 80 when unset and `memory_limit` is present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_warn_percent: Option<u32>,
    /// Warn when anonymous memory is within this many MB of `memory_limit`
    /// (`Some(0)` disables). Mutually exclusive with `memory_warn_percent` in intent;
    /// takes precedence over the percent default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_warn_headroom: Option<u64>,
    /// Kernel soft limit (`memory.high`) in MB; throttles the cgroup before the
    /// hard `memory_limit` kicks in (`Some(0)` disables, off by default).
    #[schema(example = 448)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_high: Option<u64>,
}
