# Resolved — Part 5 / edge `:8088` after maintenance

**Status: FIXED (product + demo, verified on lab 2026-09-06).**
Renamed from OPEN: all four failure modes traced and closed. Kept here as a record of the investigation.

## Lab verification convention

- **Scratch / probes:** `/tmp` (`TMPDIR=/tmp`) only.
- **Instance runtime:** `$DEMO_SUPER_ROOT` / `$DEMO_APP_ROOT` (incl. `run/` for nginx pid).

## Root causes & fixes

### 1. `rsync --delete` wiped `run/` (demo-side, FIXED)

`pid /srv/djangoapp/run/nginx.pid`; example tree has no `run/`. Deploy rsync removed
destination `run/` while old nginx kept listening; next `restart nginx` →
`open(…/nginx.pid)` ENOENT → flapping → Fatal.

**Fix:** `scripts/bootstrap.sh` excludes `run/` and creates it **after** rsync.
Ad-hoc deploys must do the same.

### 2. Dependency-gated spawns counted as starts → false flapping (product, FIXED)

`spawn_program` recorded flapping **before** the `depends_on` gate: every
Waiting→spawn retry (one per dependency health transition) was booked as a start.
A crash-looped-then-recovered program (or a slow dependency boot) could then be
marked **Fatal the moment its dependency went Healthy** — group
`stop`/`start @djangoapp` reproducibly killed nginx ~1s after web turned Healthy.

**Fix (`super/core/src/manager/controller.rs`):** dependency check moved **before**
flapping accounting; the tracker is only fed when a real spawn is about to run.
Regression tests: `core/tests/waiting_flapping_test.rs`
(`waiting_retries_do_not_count_as_starts`, `crash_loop_still_fatales`).

### 3. Faster health checks (mitigation, kept)

`interval_secs` / `start_period_secs = 1` on web/nginx/worker/beat reduce probe lag;
necessary for the ≤15s edge ceiling but never the root fix.

### 4. Remaining product notes (documented, low priority)

- Group `start` reports `Success` even when a Waiting member was later Fatal'ed;
  batch results should surface post-start failures.
- `restart --wait-healthy` now waits for a **PID change** before Healthy
  (CLI, uncommitted WIP earlier); worth a targeted test.

## Verified (lab, fixed `superd` 1.5.5)

| Step | Result |
| :--- | :--- |
| `restart web --wait-healthy` → immediate curl `:8088` | HTTP 200, PID changed |
| `restart nginx --wait-healthy` → immediate curl `:8088` | HTTP 200, PID changed |
| `stop @djangoapp` → 10s → `start @djangoapp` (×2) | nginx Waiting→Healthy in ~3s; edge 200; no flapping |
| `cargo test --workspace` | pass (one unrelated OTA timing flake passes in isolation) |
