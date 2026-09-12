---
title: "Quick Start"
weight: 2
description: "Start the server and manage processes dynamically via the API."
---

In this guide, we will start the **Super** daemon with a minimal configuration and use its **REST API** to dynamically register and start a demo HTTP server.

> [!NOTE]
> Before adding programs: read the [Managed Program Requirements](/docs/02-essentials/process-management-contract) — managed apps must run in the foreground and must not daemonize or escape Super's process group.

## 1. Minimal Configuration

Create a file named `super.toml`. We only need to configure the server port.

```toml
# super.toml

[server]
host = "127.0.0.1"
port = 9002
# auth_secret = "…"   # optional on loopback; required for non-loopback binds
```

> [!CAUTION]
> Default loopback bind keeps the API open for local CLI/scripts. Non-loopback binds **require** a non-empty [`auth_secret`](/docs/02-essentials/authentication/#oss-admin-secret) (or the daemon refuses to start). Multi-user Access Tokens need the **`security` plugin**. Prefer a firewall or reverse proxy if you expose the API.

If you use the repo's [example config](https://github.com/schiplat/super/blob/master/examples/demo/conf/super.toml), it also binds to port **9002** — keep CLI/API URLs in sync with your `super.toml`.

> [!NOTE]
> **Docker:** the official [`schiplat/super`](https://hub.docker.com/r/schiplat/super) image already ships a default `super.toml` under `/app/super/conf/` — skip to step 2 and use the **Docker** tabs below for the demo program.

## 2. Start the Daemon (OSS)

Run the daemon in the foreground (default — required under systemd). Layout, config init, and `--daemon`: [Installation — Instance layout](/docs/01-getting-started/installation/#instance-layout-and-config) and [Start by hand](/docs/01-getting-started/installation/#manual-start-by-hand).

```bash
superd
```

Without systemd, you may detach with `superd --daemon` (writes `$SUPER_ROOT/run/superd.pid` by default). Control programs and stop the daemon the same way either way (`super …`, `super shutdown`).

Expected output includes `Super Core starting...` and the listen address.

{{< tabs >}}
  {{< tab name="Docker" >}}
  The OSS image is **distroless** — it runs `superd` + `super` only (no shell, no Python). Map the API port and the demo HTTP port. Give the container a name so you can run CLI commands later:

  ```bash
  docker pull schiplat/super:latest

  docker run --rm --name super \
    -p 127.0.0.1:9002:9002 -p 127.0.0.1:8080:8080 \
    schiplat/super:latest
  ```

  The image binds `0.0.0.0` and ships placeholder `auth_secret = "CHANGE-ME-BEFORE-EXPOSE"` in the baked `super.toml`. Core auth is **on** — every business API call needs that Bearer (Dashboard login, `super --token` / `super login`, or `Authorization: Bearer …`). `/health` stays open. Replace the placeholder before publishing beyond the host. Details: [Authentication](/docs/02-essentials/authentication/#oss-admin-secret).

  ```bash
  # Host shell for steps 3–4 (matches the baked placeholder until you change it)
  export SUPER_TOKEN=CHANGE-ME-BEFORE-EXPOSE
  ```

  **`superd` runs inside the container; the CLI does not have to.** With `-p 127.0.0.1:9002:9002`, the HTTP API is on your host — use any option below for steps 3–4:

  | Option | When to use |
  | :--- | :--- |
  | **`curl`** | Zero install; copy/paste the REST examples |
  | **`super --server http://127.0.0.1:9002`** on the **host** | Day-to-day CLI (install the `super` binary from [GitHub Releases](/docs/01-getting-started/installation/#method-3-github-releases-or-build-from-source), or `docker cp` it out of the image — see [Installation — Docker CLI](/docs/01-getting-started/installation/#cli-on-the-host-containerized-superd)) |
  | **`docker exec super super …`** | Quick try without installing anything on the host |

  See [Installation — Docker](/docs/01-getting-started/installation/#method-1-docker-recommended) for custom mounts and subscription plugins.
  {{< /tab >}}
{{< /tabs >}}

Confirm the daemon is up before continuing:

```bash
curl http://127.0.0.1:9002/health
# {"status":"healthy","components":{"manager":"up","persistence":"up","web":"up"}}
```

> [!NOTE]
> The `/health` endpoint is an **unauthenticated liveness probe** — it only says the API is reachable. On default **loopback OSS**, the REST API is open on the same port. With **core auth** (`auth_secret` set) or the `security` plugin, business endpoints require a Bearer while `/health` stays open.

## 3. Create Program via API

Open a new terminal. Pick the example that matches how you run `superd`:

> [!IMPORTANT]
> The **`schiplat/super` image does not include Python 3**. If you start Super with Docker, use the **Docker** tabs — not the Python example.

{{< tabs >}}
  {{< tab name="CLI" >}}
  Requires **Python 3** (`install.sh`, tarball, macOS, or Linux):

  ```bash
  super add --name demo-web \
    --autostart python3 -m http.server 8080
  ```
  {{< /tab >}}
  {{< tab name="REST API" >}}
  Requires **Python 3**:

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
  {{< tab name="Docker (CLI)" >}}
  Uses the static **`busybox`** binary baked into `schiplat/super`.

  **Host CLI** (recommended — install `super` once, talk to the mapped port). Bearer = baked `auth_secret` (or `export SUPER_TOKEN=…` from step 2):

  ```bash
  super --server http://127.0.0.1:9002 --token "$SUPER_TOKEN" add --name demo-web \
    --autostart /usr/local/bin/busybox httpd -f -p 8080
  ```

  **Or via `docker exec`** (no host install; container must be named, e.g. `--name super` in step 2). Inside the container the CLI talks to the local daemon — still pass the same secret:

  ```bash
  docker exec -e SUPER_TOKEN=CHANGE-ME-BEFORE-EXPOSE super \
    /usr/local/bin/super add --name demo-web \
    --autostart /usr/local/bin/busybox httpd -f -p 8080
  ```
  {{< /tab >}}
  {{< tab name="Docker (REST API)" >}}
  Uses the static **`busybox`** binary baked into `schiplat/super`. Include the Bearer (baked placeholder until you change `auth_secret`):

  ```bash
  curl -X POST http://127.0.0.1:9002/api/v1/programs \
    -H "Authorization: Bearer CHANGE-ME-BEFORE-EXPOSE" \
    -H "Content-Type: application/json" \
    -d '{
        "name": "demo-web",
        "command": "/usr/local/bin/busybox",
        "args": ["httpd", "-f", "-p", "8080"],
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
# demo-web should show Running or Healthy
```

> [!NOTE]
> **Docker:** if `superd` runs in a container, auth is on — use `super --server http://127.0.0.1:9002 --token "$SUPER_TOKEN" list` or `docker exec -e SUPER_TOKEN=CHANGE-ME-BEFORE-EXPOSE super /usr/local/bin/super list`.

```bash
curl http://127.0.0.1:8080
# Python http.server: directory listing HTML
# Docker busybox httpd: often 404 on / — the port is up
```

## 5. Dashboard

Open **[http://127.0.0.1:9002](http://127.0.0.1:9002)**.

**OSS:** The embedded Dashboard shell loads (process list, logs, stack editor). Optional [core auth](/docs/02-essentials/authentication/#oss-admin-secret) may prompt for login.

**With subscription plugins:** The **`ui`** plugin adds Pro surfaces (Access Tokens, Notification Settings, hot-reload) when `security` / `notify` are also loaded — see [Dashboard](/docs/02-essentials/web-ui).

---

## Next Steps

*   [Installation](/docs/01-getting-started/installation/) — binaries, `install.sh`, instance layout, OS services
*   [Getting Started](/docs/01-getting-started/) — section overview
*   [A Django application under Super](/docs/02-tutorials/django-postgres-celery/) — Gunicorn and Celery against external Postgres and Redis
*   [API Reference](/docs/06-internals/api-reference) — stop, restart, historical logs
*   [Configuration](/docs/02-essentials/configuration) — persistent `super.toml`
*   [Dependency Orchestration](/docs/03-orchestration/dependencies)

---

## Appendix: Licensed Plugins 💎

Licensed Super Pro capabilities (Dashboard Pro extensions via `ui`, token authentication & RBAC, resource isolation, event notifications, operation audit) ship as **signed plugin libraries** — `.so` on Linux, `.dylib` on macOS. They run on the **same OSS `superd` and `super` binaries**; there is no separate "Pro daemon". Enabling licensed mode only adds three things from your subscription delivery:

| Piece | Where | Notes |
| :--- | :--- | :--- |
| **`[license].key`** | `conf/super.toml` | Signed license authorizing your plugins |
| **`auth_secret`** | `conf/super.toml` | Root bootstrap credential for first sign-in |
| **Plugin libraries** (`security`, `ui`, `notify`, `isolation`, …) | `$SUPER_ROOT/plugins/` | `security` is **required** — multi-user tokens / RBAC / audit |

```text
$SUPER_ROOT/
├── conf/
│   └── super.toml        # [license].key + auth_secret
└── plugins/              # authorized .so / .dylib (filenames match the signed claims)
```

> `$SUPER_ROOT` is resolved from the [`SUPER_ROOT` environment variable](/docs/06-internals/environment-variables#super_root) (then the binary layout, then the working directory).

**Install and restart:**

```bash
# 1. Copy the plugin libraries from your subscription delivery package
cp /path/to/subscription/plugins/* "$SUPER_ROOT/plugins/"

# 2. Add [license].key and auth_secret to conf/super.toml

# 3. Restart superd — it verifies the license, then loads the authorized plugins
```

> [!IMPORTANT]
> Licensed startup **fails fast** instead of silently losing API auth: `security` must load (file present **and** listed in the license claims) and `auth_secret` must be set — see [Licensed deployments require security](/docs/02-essentials/authentication#licensed-deployments-require-security). Without a valid `[license].key`, `superd` runs in OSS mode and ignores `plugins/`.

**First sign-in** — with `security` active, bootstrap an **Admin** Access Token, then prefer `sk-…` tokens for day-to-day use:

```bash
super login <auth_secret>
super token create admin --role admin
super token list
```

**Next.** Day-to-day topics (config, auth, Dashboard) are under **[Essentials](/docs/02-essentials/)**. Licensed governance (RBAC, audit, isolation, notify) is under **[Advanced Management](/docs/05-advanced-management/)**. During the public beta you can request a **free 90-day Super Pro trial** ([Portal claim](https://platform.ddl.sconts.com/portal/claim?product=super-pro&plan=first-trials-001)); compare editions in the [feature matrix](/docs/07-editions/feature-matrix/).
