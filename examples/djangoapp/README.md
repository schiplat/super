# Django application under Super (OSS example)

Runnable companion to the public tutorial
[A Django application under Super](../../docs/content/docs/02-tutorials/django-postgres-celery.md):
**nginx → Gunicorn + Celery worker/Beat** against **external** Postgres/Redis, managed only by OSS `superd` / `super` (no licensed plugins).

| Layer | What |
| :--- | :--- |
| Data plane | `compose.yml` — Postgres `:15432`, Redis `:16379` (host loopback) |
| App plane | Django `demoapp` — `/health/` (DB+Redis), `WorkItem` + Celery tasks, enqueue API |
| Edge | nginx under Super on `127.0.0.1:8088` |
| Super | `SUPER_ROOT=/opt/super-django-demo`, API `http://127.0.0.1:9012` |

## App design (Django + Celery)

| Piece | Role |
| :--- | :--- |
| `demoapp.models.WorkItem` | Async job row (`pending` → `running` → `done`/`failed`) |
| `demoapp.tasks.ping` | Beat heartbeat + `celery inspect` friendliness |
| `demoapp.tasks.busy_work` | Sleepy task for `--autoscale` demos (`enqueue_burst.sh`) |
| `demoapp.tasks.process_work_item` | Updates `WorkItem` while sleeping |
| `GET/POST /api/work/enqueue/` | Create row + enqueue (demo; loopback only) |
| `GET /api/work/<uuid>/` | Poll job status |
| `manage.py enqueue_work` / `demo_tick` | CLI enqueue; Super-cron tick every 5m (`0 */5 * * * *`) |

## Quick start (lab host)

```bash
sudo EXAMPLE_SRC=/path/to/examples/djangoapp ./scripts/bootstrap.sh
./scripts/verify.sh
```

Override with `DEMO_APP_ROOT` / `DEMO_SUPER_ROOT` (defaults: `/srv/djangoapp`, `/opt/super-django-demo`). Bootstrap rewrites `/srv/djangoapp` placeholders in stack/nginx to `DEMO_APP_ROOT`.

## Demo story (VHS Parts 1–5)

Record **in order** (`./scripts/record.sh`):

| Part | Tape | What it shows |
| :--- | :--- | :--- |
| **1** | `01-create-services` | `stop`/`remove` `@djangoapp` → `apply` → `list` |
| **2** | `02-check-status` | `list` / `info` / curl `/health/` |
| **3** | `03-scale-workers` | `scale_workers.sh 3` then `2` (+ 10s settle before list) |
| **4** | `04-view-logs` | `logs --tail` + `--follow` |
| **5** | `05-maintenance` | dry-run / restart web\|nginx / **restart @djangoapp (two-phase)** / stop+start / **wait_edge ≤15s** |

```bash
export VHS_NO_SANDBOX=true TMPDIR=/tmp
./scripts/record.sh
```

> **Edge check ≤ 15s** for final nginx `:8088` probe. Other VHS/CLI wait ceilings are **30s**. Scratch under `/tmp`.

Manual walkthrough:

```bash
export SUPER_SERVER=http://127.0.0.1:9012 SUPER_ROOT=/opt/super-django-demo TMPDIR=/tmp

# health + enqueue
curl -sS http://127.0.0.1:8088/health/; echo
curl -sS "http://127.0.0.1:8088/api/work/enqueue/?seconds=1"; echo

# scale
./scripts/scale_workers.sh 3 && ./scripts/wait_workers_healthy.sh 25
./scripts/scale_workers.sh 2 && ./scripts/wait_workers_healthy.sh 25
sleep 10

# maintenance — group ops are ordered (restart @djangoapp is a safe two-phase bounce)
super --server "$SUPER_SERVER" restart @djangoapp -y
./scripts/wait_edge.sh http://127.0.0.1:8088/health/ 15
```

> **Note:** Group operations follow `depends_on` order: a group restart stops everything in reverse dependency order, then starts everything in dependency order ([ordered group operations](../../docs/content/docs/03-orchestration/dependencies.md#ordered-group-operations)).

## Worker scaling

| Layer | Mechanism |
| :--- | :--- |
| **Horizontal** | `./scripts/scale_workers.sh N` — `numprocs` → `worker-0`… (prune=false; script stop+remove leftovers) |
| **Vertical** | Celery `--autoscale=2,1` in `scripts/run-worker.sh` |

```bash
./scripts/enqueue_burst.sh 20 3
```

## Layout

```text
compose.yml
.conf / nginx / demoapp / myproject
scripts/bootstrap.sh verify.sh scale_workers.sh wait_edge.sh …
tapes/01-…05-*.tape
```

## Notes

- **OSS only** — no license / plugins / Dashboard.
- Keep **`prune = false`**.
- Super programs: `web`, `nginx`, `worker-*`, `beat`, `clearsessions`, `demo-tick`.
