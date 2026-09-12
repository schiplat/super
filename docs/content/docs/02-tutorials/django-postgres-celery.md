---
title: "A Django application under Super"
weight: 1
description: "nginx, Gunicorn, Celery worker and Beat against external Postgres and Redis — bootstrap, daily ops, and upgrades."
---

This guided tour runs a **Django** application under Super the way most production hosts do it:

| Layer | What runs where |
| :--- | :--- |
| **Edge (under Super)** | **nginx** terminates HTTP(S) on `:80` / `:443` and reverse-proxies to Gunicorn on loopback. (Cloud load balancers or distro nginx+systemd are fine alternatives — same proxy role.) |
| **App plane (under Super)** | `web` (Gunicorn), `worker` + `beat` (Celery), plus Super **cron** for `manage.py` housekeeping — group `@djangoapp`. |
| **Data plane (outside Super)** | Managed **Postgres** and **Redis**. Backups, HA, and patching stay with that platform. |

Super does **not** replace your database or broker product. It keeps the edge and app processes foregrounded, ordered, restarted, and observable.

| Role | Program | Why it is here |
| :--- | :--- | :--- |
| Edge | `nginx` | Public (or VPC) HTTP entry; proxies to Gunicorn; waits until `web` is Healthy |
| HTTP app | `web` (Gunicorn) | Serves Django on `127.0.0.1:8000`; HTTP health for readiness |
| Async work | `worker` (Celery) | Background tasks; exec health via `celery inspect ping` |
| Scheduler | `beat` (Celery Beat) | Periodic Celery schedules; exec health probes the broker |
| Housekeeping | `clearsessions` | Super [scheduled task](/docs/02-essentials/scheduled-tasks) (OSS cron) |

### Stack workflow

**Data plane first**, then Super starts **web → nginx** (nginx `depends_on` Healthy Gunicorn so clients are not served 502s during boot), with **worker** and **beat** alongside:

```mermaid
flowchart TB
  subgraph data["Data plane — outside Super"]
    PG["Postgres<br/>managed / dedicated"]
    RD["Redis<br/>managed / dedicated"]
  end

  subgraph superd["Edge + app — superd · group djangoapp"]
    NGX["nginx<br/>:80 / :443 → Healthy"]
    WEB["web · Gunicorn<br/>:8000 /health/ → Healthy"]
    WK["worker · Celery<br/>inspect ping → Healthy"]
    BEAT["beat · Celery Beat<br/>broker ping → Healthy"]
    CRON["clearsessions<br/>Super cron"]
  end

  WEB -->|"depends_on"| NGX
  PG -.->|"DATABASE_URL"| WEB
  PG -.->|"DATABASE_URL"| WK
  PG -.->|"migrate / clearsessions"| CRON
  RD -.->|"CELERY_BROKER_URL"| WEB
  RD -.->|"CELERY_BROKER_URL"| WK
  RD -.->|"CELERY_BROKER_URL"| BEAT
```

**Runtime** — clients hit nginx; Celery worker + beat share the external broker and DB:

```mermaid
flowchart LR
  U["Client"] -->|"HTTP :80 / :443"| NGX["nginx"]
  NGX -->|"proxy_pass<br/>127.0.0.1:8000"| WEB["web<br/>Gunicorn + Django"]
  WEB -->|"SQL"| PG["Postgres"]
  WEB -->|"enqueue"| RD["Redis"]
  RD -->|"consume"| WK["worker<br/>Celery"]
  BEAT["beat"] -->|"periodic enqueue"| RD
  WK -->|"SQL / side effects"| PG
  CRON["clearsessions<br/>Super cron"] -->|"manage.py"| PG
```

You will use **declarative stacks**, **`depends_on`**, **health checks**, **groups**, logs, restarts, snapshots, and a release-directory upgrade path. Licensed plugins stay in a short 💎 appendix so the OSS path stands alone.

> [!TIP]
> **Runnable demo source (OSS).** Clone and follow the companion under
> [`examples/djangoapp/`](https://github.com/schiplat/super/tree/master/examples/djangoapp)
> ([README](https://github.com/schiplat/super/blob/master/examples/djangoapp/README.md)):
> Compose for Postgres/Redis, Django `demoapp`, nginx edge, and a declarative
> `@djangoapp` stack (`conf/conf.d/djangoapp.toml`). Lab defaults use Super API
> `:9012` and nginx `:8088` so they can coexist with another Super/nginx on the
> same host. The TOML samples **below** use production-shaped paths
> (`/srv/myapp`, nginx `:80`, Super `:9002`) and more conservative health
> intervals — same architecture as the demo, different ports and tuning for a
> quiet lab.

> [!NOTE]
> **Assumptions.** You already have [binaries + an instance root](/docs/01-getting-started/installation/#instance-layout-and-config), a Django project that runs under Gunicorn, **nginx** installed on the host, and **reachable** Postgres + Redis endpoints. Paths below use `/srv/myapp` and `$SUPER_ROOT=/opt/super`.

> [!IMPORTANT]
> Programs under Super must stay in the **foreground**: nginx with `daemon off`, Gunicorn without `--daemon`, Celery without `--detach`. See [Managed Program Requirements](/docs/02-essentials/process-management-contract).

---

## 1. Provision Postgres and Redis (outside Super)

Pick whatever your org already uses. Super only needs connection URLs.

| Approach | Typical use |
| :--- | :--- |
| **Managed cloud** | AWS RDS + ElastiCache, GCP Cloud SQL + Memorystore, Azure Database + Cache, … |
| **Shared company cluster** | DBA-owned Postgres; Redis Sentinel/Cluster run by platform team |
| **Separate Compose / VM** | `docker compose` **only** for `postgres` + `redis` on a data host, or distro packages under **systemd** — still not under `superd` |

Checklist before starting Super:

1. Create database/role (or use the managed console). Apply network allowlists so the app host can reach `:5432` / `:6379` (or TLS ports).
2. Confirm connectivity from the app host:
   ```bash
   pg_isready -h db.example.com -p 5432 -U myapp
   redis-cli -h redis.example.com -p 6379 PING   # or redis-cli --tls …
   ```
3. Put secrets in a file Super will load (not in Git):

   ```bash
   # /srv/myapp/.env  — mode 0600, owned by the deploy user
   DATABASE_URL=postgres://myapp:REDACTED@db.example.com:5432/myapp
   CELERY_BROKER_URL=redis://:REDACTED@redis.example.com:6379/0
   DJANGO_SETTINGS_MODULE=myproject.settings
   ```

   See [Environment & Secrets](/docs/02-essentials/environment-secrets).

4. Run migrations **once** against that database (from a deploy box or CI), before or right after first `web` start:

   ```bash
   cd /srv/myapp && set -a && . ./.env && set +a
   .venv/bin/python manage.py migrate
   ```

> [!TIP]
> Make Django’s `/health/` (or `/healthz`) verify **DB connectivity** (and optionally the broker). Super’s HTTP probe on `web` then means “process up **and** data plane reachable,” which is what you want for load balancers and `restart --wait-healthy`.

---

## 2. Instance layout

```bash
export SUPER_ROOT=/opt/super
mkdir -p "$SUPER_ROOT/conf/conf.d" \
  "$SUPER_ROOT/data" "$SUPER_ROOT/logs" "$SUPER_ROOT/run" "$SUPER_ROOT/plugins"
chmod 755 "$SUPER_ROOT/run" "$SUPER_ROOT/logs" "$SUPER_ROOT/data"
```

Minimal daemon config:

```bash
cat > "$SUPER_ROOT/conf/super.toml" <<'EOF'
[server]
host = "127.0.0.1"
port = 9002
allow_insecure_public_bind = false
socket = "run/superd.sock"

[storage]
data_file = "data/snapshot.json"
events_file = "data/events.db"
log_dir = "logs"

[include]
files = ["conf/conf.d/*.toml"]
EOF

super check
```

Details: [Installation — Instance layout](/docs/01-getting-started/installation/#instance-layout-and-config).

---

## 3. nginx reverse proxy (edge)

Gunicorn stays on **loopback** (`127.0.0.1:8000`). nginx is the process that listens on the host network (and optionally terminates TLS).

Minimal config (adjust paths / `server_name` / certificates):

```nginx
# /srv/myapp/nginx/nginx.conf
worker_processes auto;
error_log /dev/stderr warn;
pid /tmp/myapp-nginx.pid;

events { worker_connections 1024; }

http {
  access_log /dev/stdout combined;
  # Upstream is Super-managed Gunicorn — never expose :8000 publicly.
  upstream django_web {
    server 127.0.0.1:8000;
    keepalive 32;
  }

  server {
    listen 80;
    server_name app.example.com;

    # Optional: TLS — listen 443 ssl; ssl_certificate …; ssl_certificate_key …;

    location / {
      proxy_http_version 1.1;
      proxy_set_header Host              $host;
      proxy_set_header X-Forwarded-For   $proxy_add_x_forwarded_for;
      proxy_set_header X-Forwarded-Proto $scheme;
      proxy_set_header Connection        "";
      proxy_pass http://django_web;
    }
  }
}
```

Validate before wiring Super:

```bash
nginx -t -c /srv/myapp/nginx/nginx.conf
```

> [!NOTE]
> Prefer **this** nginx under Super (`daemon off`) on a single app VM so `@djangoapp` restarts and logs stay in one place. If your platform already runs nginx via systemd or a cloud LB in front of the VM, skip the `nginx` program and point that edge at Gunicorn (or only at this host’s `:80` if something else still proxies).

---

## 4. Declare the app stack

Create `$SUPER_ROOT/conf/conf.d/djangoapp.toml`. Edge + application processes — no Postgres/Redis binaries.

The lab companion ships the same shape at
[`examples/djangoapp/conf/conf.d/djangoapp.toml`](https://github.com/schiplat/super/blob/master/examples/djangoapp/conf/conf.d/djangoapp.toml)
(paths under `/srv/djangoapp`, nginx TCP probe on `:8088`, faster health
cadence, `numprocs = 2` workers + `demo-tick` cron). Use that file when
following the [demo README](https://github.com/schiplat/super/blob/master/examples/djangoapp/README.md);
use the production-shaped sample below when wiring a real host.
```toml
# conf/conf.d/djangoapp.toml
#
# CRITICAL: keep prune = false unless you intentionally want Super to DELETE
# every managed program that is missing from this file on apply/reload.
# prune = true + an incomplete/empty stack will wipe nginx, web, worker, beat, …
# (your whole @djangoapp fleet) from Super's registry in one shot.
prune = false

[[services]]
name = "web"
group = "djangoapp"
command = "/srv/myapp/.venv/bin/gunicorn"
args = [
  "myproject.wsgi:application",
  "--bind", "127.0.0.1:8000",
  "--workers", "3",
  "--access-logfile", "-",
  "--error-logfile", "-",
]
cwd = "/srv/myapp"
env_file = "/srv/myapp/.env"
autostart = true
autorestart = "unexpected"
priority = 40
startsecs = 10
# Prefer a Django view that checks DB (and optionally Redis) before returning 200.
health_check = {
  type = "http",
  url = "http://127.0.0.1:8000/health/",
  start_period_secs = 15,
  interval_secs = 10,
  timeout_secs = 5,
  max_failures = 3,
}

[[services]]
name = "nginx"
group = "djangoapp"
command = "/usr/sbin/nginx"
args = ["-c", "/srv/myapp/nginx/nginx.conf", "-g", "daemon off;"]
autostart = true
autorestart = "unexpected"
priority = 45
startsecs = 3
# Do not accept public traffic until Gunicorn is Healthy.
depends_on = ["web"]
health_check = { type = "tcp", port = 80, start_period_secs = 1, interval_secs = 5 }

[[services]]
name = "worker"
group = "djangoapp"
command = "/srv/myapp/.venv/bin/celery"
args = ["-A", "myproject", "worker", "--loglevel=INFO", "--concurrency=2"]
cwd = "/srv/myapp"
env_file = "/srv/myapp/.env"
autostart = true
autorestart = "unexpected"
priority = 50
startsecs = 20
# Exec probes do not inherit env_file — source .env in the command.
# `inspect ping` reaches this worker over the broker (needs Redis up).
health_check = {
  type = "exec",
  command = "set -a && . /srv/myapp/.env && set +a && cd /srv/myapp && .venv/bin/celery -A myproject inspect ping --timeout 5",
  interval_secs = 30,
  timeout_secs = 10,
  start_period_secs = 20,
  max_failures = 3,
}

[[services]]
name = "beat"
group = "djangoapp"
command = "/srv/myapp/.venv/bin/celery"
args = ["-A", "myproject", "beat", "--loglevel=INFO"]
cwd = "/srv/myapp"
env_file = "/srv/myapp/.env"
autostart = true
autorestart = "unexpected"
priority = 60
startsecs = 10
# Beat has no inspect API — probe broker reachability (same URL the process uses).
health_check = {
  type = "exec",
  command = "set -a && . /srv/myapp/.env && set +a && redis-cli -u \"$CELERY_BROKER_URL\" PING | grep -q PONG",
  interval_secs = 30,
  timeout_secs = 5,
  start_period_secs = 10,
  max_failures = 3,
}

# --- Super cron: Django housekeeping (not Celery Beat) --------------------

[[services]]
name = "clearsessions"
group = "djangoapp"
command = "/srv/myapp/.venv/bin/python"
args = ["manage.py", "clearsessions"]
cwd = "/srv/myapp"
env_file = "/srv/myapp/.env"
cron = "0 0 3 * * *"          # 03:00 every day (sec min hour …)
on_overlap = "skip"
autostart = true              # ignored for boot; cron owns the schedule
```

> [!CAUTION]
> **`prune = false` is mandatory for day-to-day stacks.** `prune` defaults to **false** when omitted — Super only removes programs when a stack explicitly sets `prune = true`. With prune enabled, `super apply` / `[include]` reload **deletes every Super-managed program that is not listed in the stack file**. A typo, an empty `[[services]]` list, or applying the wrong file can wipe your entire `@djangoapp` fleet in one operation — **irreversible** in Super. External Postgres/Redis data is not dropped, but you lose process definitions until you recreate them. `super apply` and the Dashboard Stack Editor print an apply diff (including every program that would be removed) and require typing **`confirmed`** (`--force-prune` for CLI automation). Prefer leaving `prune = false`. See [Declarative stacks](/docs/04-production-scenarios/delivery/declarative-stack).

### What this stack teaches

| Super feature | How it shows up |
| :--- | :--- |
| `[include]` stacks | `conf/conf.d/djangoapp.toml` — [Declarative stacks](/docs/04-production-scenarios/delivery/declarative-stack) |
| **`prune = false`** | **Keep this.** `prune = true` deletes unmanaged-by-this-file programs on apply — easy to wipe the whole fleet |
| Edge + app | `nginx` proxies to loopback Gunicorn; data plane stays external |
| `depends_on` + health | `nginx` waits for `web` **Healthy** — [Dependencies](/docs/03-orchestration/dependencies) |
| HTTP / TCP / exec probes | Gunicorn `/health/`; nginx TCP `:80`; worker `inspect ping`; beat `redis-cli PING` — [Health checks](/docs/03-orchestration/health-checks) |
| `group` | Batch control: `super restart @djangoapp` |
| Cron vs Beat | `clearsessions` = Super cron; `beat` = Celery scheduler — [Scheduled tasks](/docs/02-essentials/scheduled-tasks) |

> [!NOTE]
> **`depends_on` and external services.** `depends_on` only references **other Super programs** (here: `nginx` → `web`). You cannot `depends_on` managed RDS/Redis — gate those with HTTP/exec health, `start_period_secs`, and app-level retries.

---

## 5. Start and verify

```bash
export SUPER_ROOT=/opt/super
superd                          # foreground; or use your OS service
```

In another shell:

```bash
export SUPER_ROOT=/opt/super
super doctor
super list
```

Expect roughly:

1. `web` → Running → **Healthy** when `/health/` returns 2xx (DB reachable if your view checks it)
2. `nginx` starts only after `web` is Healthy → **Healthy** on TCP `:80`
3. `worker` → **Healthy** when `celery inspect ping` succeeds (broker + worker reply)
4. `beat` → **Healthy** when `redis-cli … PING` returns PONG
5. `clearsessions` stays Stopped until the next cron tick

```bash
super info web
super info nginx
super info worker
super info beat
super logs nginx --tail 50
super logs worker --tail 50 --follow
super logs beat --tail 50
curl -sS -o /dev/null -w '%{http_code}\n' http://127.0.0.1/health/
```

If `web` never becomes Healthy, fix data-plane connectivity first (`pg_isready` / `redis-cli PING` from the same host), then inspect Django logs. If `nginx` stays Starting, check `super info web` — dependents wait for **Healthy**, not merely Running. If `worker` / `beat` stay unhealthy, confirm Redis and that exec probes can `source /srv/myapp/.env` (exec checks do not load `env_file` automatically).

---

## 6. Day-to-day operations

### Status and logs

```bash
super list
super top                       # optional TUI
super logs nginx --tail 100
super logs web --tail 100
super logs worker --follow
super logs beat --tail 50
super events worker --tail 20
```

Child logs live under `$SUPER_ROOT/logs/` — [Logging](/docs/02-essentials/logging).

### Restart with readiness

After a settings or code change that only needs a process bounce (DB/broker stay up):

```bash
super restart web --wait-healthy --timeout 60
# nginx depends_on web — restart edge after upstream is Healthy again:
super restart nginx --wait-healthy --timeout 30
super restart worker --wait-healthy --timeout 60
super restart beat --wait-healthy --timeout 30
```

Or bounce the whole group: `super restart @djangoapp -y`. The restart runs as a two-phase cycle — everything stops in reverse dependency order, then everything starts in dependency order, so `web` is Healthy before `nginx` comes back ([ordered group operations](/docs/03-orchestration/dependencies/#ordered-group-operations)).

### Scale Celery workers

**A — higher concurrency** (single process): edit `args` in the stack (e.g. `--concurrency=4`), then `super apply` / reload. Imperative alternative: [`super update … --args`](/docs/06-internals/cli-reference/#update).

**B — multiple worker processes** via `numprocs`:

```toml
[[services]]
name = "worker"
group = "djangoapp"
numprocs = 2
process_name = "worker-{num}"
# … same command / env_file / …
```

Then `super apply "$SUPER_ROOT/conf/conf.d/djangoapp.toml"`.

### Group actions

```bash
super restart @djangoapp --dry-run
super restart @djangoapp -y
super stop @djangoapp -y
super start @djangoapp -y
```

Stopping `@djangoapp` does **not** touch Postgres or Redis — they are owned by the data plane.

### Reload declarative config

```bash
# edit conf/conf.d/djangoapp.toml  — leave prune = false
super check
super apply "$SUPER_ROOT/conf/conf.d/djangoapp.toml"
# or: super reload
```

```bash
super export > /srv/myapp/ops/current-stack.toml
```

---

## 7. Failure drills (learn the edges)

**Broker unreachable.** Break Redis credentials or security group briefly → `worker` / `beat` exec probes fail (unhealthy → possible health-restart); Celery tasks error in `super logs worker`. Restore the broker; confirm both return Healthy.

**Database flap.** If `/health/` checks the DB, Super marks `web` unhealthy and may health-restart after `max_failures` ([Health checks](/docs/03-orchestration/health-checks)). While `web` is down, nginx may return 502 to clients — restore Postgres, confirm `web` then `nginx` are Healthy.

**nginx config error.** Break `proxy_pass`, `super restart nginx` → Fatal / crash loop in `super logs nginx`. Fix `nginx -t -c …`, then `super start nginx`.

**Fatal worker / beat.** Point `CELERY_BROKER_URL` at a closed port in `.env`, restart the program, watch `retry_limit` → `Fatal` after repeated health failures. Fix `.env`, `super start worker` / `super start beat`.

**Cron.** Wait for the clearsessions tick (or set `cron` near-term), then `super events clearsessions --type cron_exit`.

---

## 8. Upgrade weekend

Django releases are usually a **tree** (code + virtualenv + static assets), not a single binary. Prefer a normal deploy pipeline; use Super for ordered restart and health gates afterward.

1. **Snapshot** program definitions (so you can restore Super config if a bad apply happens):

   ```bash
   super export > /srv/myapp/ops/backup-$(date +%F).toml
   ```

   See [Snapshot & restore](/docs/04-production-scenarios/delivery/snapshot-and-restore).

2. **Ship the app tree** with whatever you already use, for example:
   - rsync / CI build archive unpack into `/srv/myapp/releases/20260906T1500/`
   - `python -m venv` + `pip install -r requirements.txt` (or reuse a built wheelhouse)
   - flip a symlink: `/srv/myapp/current` → that release (point `cwd` / venv paths at `current`)
   - `collectstatic` if nginx or a CDN serves static files

   Keep Gunicorn/Celery command paths stable (via `current`) so `djangoapp.toml` does not change every release.

3. **Migrate** against the existing managed database (CI job or bastion) — do not restart Postgres for an app release:

   ```bash
   cd /srv/myapp/current && set -a && . /srv/myapp/.env && set +a
   .venv/bin/python manage.py migrate
   ```

4. **Bounce edge + app** (data plane stays up):

   ```bash
   super restart web --wait-healthy --timeout 90
   super restart nginx --wait-healthy --timeout 30
   super restart worker --wait-healthy --timeout 90
   super restart beat --wait-healthy --timeout 30
   ```

5. Smoke-test via nginx (`curl -I http://app.example.com/health/`) and a sample Celery task (including one Beat schedule if you defined any); skim `super events web` / `nginx` / `worker` / `beat`.

---

## 9. Celery Beat vs Super cron

| | **Celery Beat** (`beat`) | **Super cron** (`clearsessions`) |
| :--- | :--- | :--- |
| Runs | Long-lived process | Short job per tick |
| Schedules | Celery task signatures | Any executable (`manage.py`, scripts) |
| Overlap | Celery’s model | `on_overlap` = skip / queue / kill |
| Use when | Domain tasks through the Celery pipeline | Ops commands that should not occupy a worker slot |

---

## 10. 💎 Licensed extras (same app stack)

Same binaries and `djangoapp.toml`. After `[license].key`, `auth_secret`, and plugins under `$SUPER_ROOT/plugins/` ([Editions](/docs/07-editions/)):

| Plugin | Fit for this tutorial |
| :--- | :--- |
| **security** | Token auth / RBAC / audit when the API leaves loopback |
| **ui** | Dashboard for `nginx` / `web` / `worker` / `beat` |
| **notify** | IM/webhook when `worker` or `web` goes Fatal |
| **isolation** (Linux) | cgroup caps on `worker` so a poison task cannot starve Gunicorn |

Enable checklist: [Licensed deployments require security](/docs/02-essentials/authentication#licensed-deployments-require-security).

---

## Appendix: laptop all-in-one (optional)

For a **throwaway laptop demo** you may run Postgres/Redis as Super programs (or a local Compose file). That is convenient for learning; it is **not** how production data planes are usually operated (no proper HA, backup tooling, or upgrade story). If you do it locally, keep them in a separate stack file (e.g. `conf/conf.d/lab-datastores.toml`) and a different group so `super restart @djangoapp` never stops your database by accident.

---

## Checklist

- [ ] Postgres + Redis provisioned **outside** Super; `pg_isready` / `redis-cli PING` from the app host
- [ ] `/srv/myapp/.env` with `DATABASE_URL` + `CELERY_BROKER_URL` (mode `0600`)
- [ ] nginx config (`daemon off`); `nginx -t -c /srv/myapp/nginx/nginx.conf`
- [ ] `SUPER_ROOT` + `conf/super.toml` + `super check`
- [ ] `djangoapp.toml` has **`prune = false`** (never apply a stack with `prune = true` unless the file is the full desired inventory)
- [ ] `djangoapp.toml`: `web`, `nginx` (`depends_on` web), `worker` + `beat` (exec health), Super cron
- [ ] `/health/` DB-aware; `superd` up; `web` / `nginx` / `worker` / `beat` Healthy; curl via `:80`
- [ ] Logs / `restart --wait-healthy` / `@djangoapp` group ops
- [ ] Export before upgrades; migrate on the managed DB; bounce web → nginx → worker → beat

## Next reading

*   [Getting Started](/docs/01-getting-started/) — [Installation](/docs/01-getting-started/installation/) · [Quick Start](/docs/01-getting-started/quick-start/)
*   [Process operations](/docs/02-essentials/process-control) · [Configuration](/docs/02-essentials/configuration)
*   [Dependencies](/docs/03-orchestration/dependencies) · [Health checks](/docs/03-orchestration/health-checks) · [Events](/docs/03-orchestration/events)
*   [Declarative stacks](/docs/04-production-scenarios/delivery/declarative-stack) · [Snapshot & restore](/docs/04-production-scenarios/delivery/snapshot-and-restore)
*   [Feature matrix](/docs/07-editions/feature-matrix) (OSS vs licensed)
