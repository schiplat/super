---
title: "Environment Variables"
weight: 4.5
description: "Public environment variables that configure superd and the super CLI."
---

Most configuration lives in `conf/super.toml` (see [Config Reference](/docs/06-internals/config-reference)). A small set of **public environment variables** lets you configure the daemon and CLI *without* touching config files — useful for containers, systemd units, and one-off overrides.

## Runtime layout

### `SUPER_ROOT`

Instance root directory: holds `conf/`, `data/`, `logs/`, `run/`, and `plugins/`. Shared by `superd`, the `super` CLI, and licensed plugins so path resolution stays consistent.

Resolution order for the daemon:

1. `SUPER_ROOT` (if set, non-empty)
2. Directory layout inferred from the executable (`<root>/bin/superd` exists → `<root>`)
3. Current working directory

The CLI's offline tools (`super check`, `super doctor`) additionally probe `super.toml`, `conf/super.toml`, and `/etc/super/super.toml` when `SUPER_ROOT` is unset.

```bash
export SUPER_ROOT=/opt/super
superd                  # reads /opt/super/conf/super.toml
```

Relative Unix socket paths (`--server unix://run/superd.sock`, `[server] socket`) and relative pidfiles resolve under `SUPER_ROOT`. See [Config Reference — Instance layout](/docs/06-internals/config-reference#instance-layout-super_root).

## CLI authentication

### `SUPER_TOKEN`

Access token for CLI → daemon requests. Equivalent to `super --token <TOKEN>`, and takes precedence over a saved `~/.super/cli.json` login for that invocation. Only relevant when the `security` plugin is loaded; OSS daemons accept requests without auth on loopback.

```bash
export SUPER_TOKEN=sk-...
super list
```

## License (licensed deployments)

### `SUPER_LICENSE`

Base64-encoded signed subscription key, in the same format as `[license].key`. Overrides the key from `super.toml` — useful in containers where the key is injected as an env var instead of written to disk.

```bash
export SUPER_LICENSE="eyJhbGciOiJFZDI1NTE5Iiwia2lkIjoia183Y2I5NTJhZiJ9..."
superd
```

### `SUPER_LICENSE_STRICT`

Force strict license verification — equivalent to `[license].strict = true`. When set to `1` / `true` / `yes`, an invalid or incompatible key **refuses startup** instead of degrading to OSS mode. Recommended for production licensed deployments.

```bash
export SUPER_LICENSE_STRICT=1
superd
```

Without `SUPER_LICENSE_STRICT`, startup still hard-fails when the key does not verify and any of these deployment signals is present: plugin libraries in `$SUPER_ROOT/plugins/`, an `auth_secret` configured, or a non-loopback bind. See [Authentication — license verification](/docs/05-advanced-management/authentication).

## Variables injected into children and hooks

The following are **not** read by `superd` — they are *written into the environment* of managed processes and hook/event scripts:

| Variable | Where it appears |
| :--- | :--- |
| `SUPER_ID`, `SUPER_NAME`, `SUPER_HOSTNAME`, `SUPER_GROUP` | Managed child processes and lifecycle hooks ([Lifecycle Hooks](/docs/03-orchestration/lifecycle-hooks)) |
| `SUPER_PID`, `SUPER_EXIT_CODE`, `SUPER_UPTIME_SECS` | Lifecycle hook scripts (post-start / pre-stop / post-stop) |
| `SUPER_PROCESS_NUM`, `SUPER_PROCESS_TOTAL` | `numprocs > 1` instances (`worker-0`, `worker-1`, …) |
| `SUPER_EVENT`, `SUPER_USAGE_BYTES`, `SUPER_LIMIT_BYTES`, `SUPER_WARN_BYTES`, `SUPER_RETRY_COUNT`, … | OSS `[[event_hooks]]` scripts ([System Events](/docs/03-orchestration/system-events)) |

These variables are set by the daemon at spawn time; you should not set them yourself.
