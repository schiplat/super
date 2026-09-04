# Super benchmark suite — product-capability matrix (no scores).
# Collected by collect_capabilities.sh; merged into the report's capability table.
# Rows are product capabilities that differentiate the four arms; values are facts.

# 1 = built-in OSS; plugin = needs licensed plugin; tool = external add-on; none = absent.
capabilities_json() {
  cat <<'JSON'
{
  "control_plane": {
    "super-oss":     "REST+CLI",
    "super-pro":     "REST+CLI (+auth)",
    "supervisord":   "XML-RPC + supervisorctl",
    "pm2":           "pm2 CLI / local socket"
  },
  "log_rotation": {
    "super-oss":     1,
    "super-pro":     1,
    "supervisord":   1,
    "pm2":           "plugin (pm2-logrotate)"
  },
  "dependency_orchestration": {
    "super-oss":     1,
    "super-pro":     1,
    "supervisord":   "partial (startsecs only)",
    "pm2":           "none"
  },
  "health_checks": {
    "super-oss":     1,
    "super-pro":     1,
    "supervisord":   "none",
    "pm2":           "partial (uptime/status)"
  },
  "single_binary_no_runtime": {
    "super-oss":     1,
    "super-pro":     "plugin-host (needs .so/.dylib)",
    "supervisord":   "python",
    "pm2":           "node"
  },
  "config_hot_reload": {
    "super-oss":     1,
    "super-pro":     1,
    "supervisord":   1,
    "pm2":           1
  },
  "ota_atomic_update": {
    "super-oss":     1,
    "super-pro":     1,
    "supervisord":   "none",
    "pm2":           "none"
  },
  "cgroup_limits": {
    "super-oss":     "none",
    "super-pro":     "isolation plugin",
    "supervisord":   "none",
    "pm2":           "none"
  },
  "auth_rbac_audit": {
    "super-oss":     "none",
    "super-pro":     "security plugin",
    "supervisord":   "basic (password)",
    "pm2":           "none"
  }
}
JSON
}
