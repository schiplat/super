---
title: "Quick Start"
weight: 2
description: "Start the server and manage processes dynamically via the API."
---

In this guide, we will start the **Super** daemon with a minimal configuration and use its **REST API** to dynamically register and start a "Hello World" web server.

> **Before adding programs:** Read the [Process Management Contract](/docs/02-essentials/process-management-contract) — managed apps must run in the foreground and must not daemonize or escape Super's process group.

## 1. Minimal Configuration

Create a file named `super.toml`. We only need to configure the server port.

```toml
# super.toml

[server]
host = "127.0.0.1"
port = 9002
# OSS has no API auth. superd refuses non-loopback bind unless you opt in here
# or load the security plugin. Keep false for local-only deployments.
allow_insecure_public_bind = false
```

> **Note**: OSS has no API authentication. The default bind is `127.0.0.1` with `allow_insecure_public_bind = false`, so `superd` will **not** start on a public address (e.g. `0.0.0.0`) unless you deliberately set that flag to `true` or load the **`security` plugin** — see [Authentication](/docs/05-advanced-management/authentication). Use a firewall or reverse proxy if you expose the API another way.

If you use the repo's [example config](https://github.com/hzbd/super/blob/master/example/conf/super.toml), it also binds to port **9002** — keep CLI/API URLs in sync with your `super.toml`.

## 2. Start the Daemon (OSS)

Run the daemon in the foreground (default — required under systemd/Docker):

```bash
superd
```

Without systemd, you may detach with `superd --daemon` (writes `$SUPER_ROOT/run/superd.pid` by default). Control programs and stop the daemon the same way either way (`super …`, `super shutdown`).

Expected output includes `Super Core starting...` and the listen address.

## 3. Create Program via API

Open a new terminal. Use the CLI or `curl` to register a program:

{{< tabs >}}
  {{< tab name="CLI" >}}
  ```bash
  super add --name demo-web \
    --autostart python3 -m http.server 8080
  ```
  {{< /tab >}}
  {{< tab name="REST API" >}}
  ```bash
  curl -X POST http://127.0.0.1:9002/api/v1/programs \
    -H "Content-Type: application/json" \
    -d '{
        "name": "demo-web",
        "command": "python3",
        "args": ["-m", "http.server", "8080"],
        "autostart": true,
        "health_check": {
            "type": "tcp",
            "port": 8080
        }
    }'
  ```
  {{< /tab >}}
{{< /tabs >}}

On success, the API returns a JSON array with the new program ID.

## 4. Verify Status

```bash
super list
```

```bash
curl http://127.0.0.1:8080
# Directory listing HTML from the managed Python server
```

## 5. Web UI

Open **[http://127.0.0.1:9002](http://127.0.0.1:9002)**.

**OSS only:** You will see a short HTML notice — there is **no built-in dashboard**. Manage processes with the `super` CLI or `/api/v1/*` (see [Web UI](/docs/05-advanced-management/web-ui)).

**With the `ui` plugin:** The full dashboard (process list, logs, controls) is served from `plugins/ui.{so,dylib}`.

---

## Next Steps

*   [API Reference](/docs/06-internals/api-reference) — stop, restart, historical logs
*   [Configuration](/docs/02-essentials/configuration) — persistent `super.toml`
*   [Dependency Orchestration](/docs/03-orchestration/dependencies)

---

## Appendix: Licensed Plugins 💎

Commercial features use the **same OSS `superd` and `super` binaries** — drop licensed `.so` / `.dylib` files under `$SUPER_ROOT/plugins/` and add `[license].key` to `conf/super.toml`.

**Prerequisites:**

```bash
$SUPER_ROOT/
  conf/super.toml           # [license].key + auth_secret (when subscribed)
  plugins/                  # Authorized libraries from subscription package
  run/                      # Optional: superd.pid when using --daemon
  data/  logs/
```

**Install licensed plugins** (from your subscription delivery package):

```bash
# Copy official plugin libraries into the instance
cp /path/to/subscription/plugins/* "$SUPER_ROOT/plugins/"
```

Restart `superd` after updating plugins or the subscription key.

**API authentication** (requires `security` plugin + `auth_secret` for startup):

```bash
# First-time bootstrap (no Access Tokens yet):
./target/release/super login <auth_secret>
./target/release/super token create admin --role admin

# Prefer sk-... for routine CLI use; optionally disable auth_secret from the Dashboard.
./target/release/super token list
```

See [Authentication](/docs/05-advanced-management/authentication#optional-disable-auth_secret).