---
title: "Instant Metrics"
weight: 1
description: "Zero-config monitoring with the built-in Prometheus endpoint."
---

In traditional setups, monitoring specific processes usually requires installing a sidecar agent like `process-exporter` or writing custom scripts to parse `ps` output.

Super eliminates this friction by providing a **built-in Prometheus metrics endpoint**.

## Zero Configuration

As soon as `superd` starts, metrics are available. No plugins, no configuration needed.

When API auth is **off** (default loopback OSS without `auth_secret`):

```bash
$ curl http://localhost:9002/metrics
```

When auth is **on** (`auth_secret` or the `security` plugin), scrape with Bearer:

```bash
$ curl -H "Authorization: Bearer $SUPER_TOKEN" http://localhost:9002/metrics
```

**Sample Output:**

```text
# HELP super_process_up Process status
# TYPE super_process_up gauge
super_process_up{id="...",name="api-server",group="backend"} 1
super_process_cpu_percent{id="...",name="api-server",group="backend"} 2.5
super_process_memory_bytes{id="...",name="api-server",group="backend"} 45000000
super_process_uptime_seconds{id="...",name="api-server",group="backend"} 1200
super_process_restart_count{id="...",name="api-server",group="backend"} 0
```

## Integrating with Prometheus

Add Super to your `prometheus.yml` scrape configs.

Without auth (loopback OSS, no `auth_secret`):

```yaml
scrape_configs:
  - job_name: 'super'
    static_configs:
      - targets: ['127.0.0.1:9002']
```

With auth enabled, pass the same Bearer secret / Access Token:

```yaml
scrape_configs:
  - job_name: 'super'
    authorization:
      type: Bearer
      credentials: 'your-auth-secret-or-sk-token'
    static_configs:
      - targets: ['127.0.0.1:9002']
```

## Useful PromQL Queries

With these metrics, you can create a powerful Grafana dashboard in minutes.

### 1. Detect Flapping Services
Find services that have restarted more than 5 times in the last hour.

```promql
increase(super_process_restart_count[1h]) > 5
```

### 2. High Memory Usage Alert
Alert if any process uses more than 1GB of RAM.

```promql
super_process_memory_bytes > 1073741824
```

### 3. Service Down Alert
Check if any managed service is not running.

```promql
super_process_up == 0
```

## Plugin metrics 💎

With licensed plugins loaded, `superd` exports additional metrics (e.g. from `notify`):

*   `super_cgroup_enforced_total`: Number of processes with active resource limits.
*   `super_notify_sent_total`: Statistics on successful/failed webhook notifications.
