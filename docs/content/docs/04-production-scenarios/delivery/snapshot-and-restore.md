---
title: "Snapshot Persistence & Restore"
weight: 3
description: "Back up, restore, and recover from data/snapshot.json — Super's authoritative program state store."
---

## What the snapshot is

Every program definition created via the CLI, the HTTP API, stack files, or the dashboard is persisted to `$SUPER_ROOT/data/snapshot.json`. It is the **single authoritative source** for what Super runs: at startup, `superd` loads it before anything else, and the `[include]` stack is applied afterwards.

## When it is written

Any change to the program registry (add / update / delete / apply stack) marks the state dirty and flushes it to `snapshot.json` asynchronously. Each save is crash-safe:

- the previous file is first copied to `snapshot.json.bak`;
- the new content is written to a temporary file, flushed, and **atomically renamed** over the main file;
- the file is created with `0600` permissions, so only the daemon user can read it.

Because writes are atomic, a running daemon never exposes a half-written snapshot — but a copy taken while running may lag the very latest change, so stop the daemon for a truly up-to-date backup.

## Backup & restore

**Back up** (stop the daemon first, or accept a possibly-lagged copy):

```bash
# with superd stopped
cp $SUPER_ROOT/data/snapshot.json /backup/snapshot.$(date +%F).json
# the sibling .bak holds the previous generation if you want it too
```

**Restore** — replace the file and start the daemon:

```bash
pkill superd
cp /backup/snapshot.2026-09-01.json $SUPER_ROOT/data/snapshot.json
$SUPER_ROOT/bin/superd &
```

Restoring is **byte-exact**: program UUIDs are preserved, so anything that references a UUID (API scripts, dashboard links, CI) keeps working.

## Corruption & auto-recovery

At startup `superd` tries, in order:

1. **`snapshot.json`** — if it parses, done.
2. **`snapshot.json.bak`** — used only when the main file *exists but is corrupt/unreadable* (a `Successfully recovered state from backup!` line is logged).
3. Otherwise:
   - **main file missing** → treated as a fresh install: start with empty state (the `[include]` stack still applies afterwards);
   - **main file corrupt and the backup missing or also corrupt** → `superd` **refuses to start** with `FATAL: Configuration corruption detected!`, to avoid silently wiping your data. Repair `snapshot.json` manually (or restore from your own backup) and start again.

> **Note:** because step 2 only kicks in for a *corrupt* main file, a deleted `snapshot.json` will *not* be auto-recovered from `.bak` — it starts empty. If you notice this after a mistake, copy `snapshot.json.bak` back to `snapshot.json` yourself; it holds the previous generation of the full state.

## Snapshot vs. stack files

The snapshot is the **runtime truth**; stack files are the **desired state**. Both give you a way to back up or restore a system — pick based on your goal:

| Scenario | Recommended approach | Notes |
| :--- | :--- | :--- |
| Same-host backup / restore, disaster rollback | Back up / restore `data/snapshot.json` directly (while `superd` is stopped) | Program UUIDs are **preserved** — a byte-exact restore. Anything that references a UUID (API calls, dashboard links, CI scripts) keeps working. |
| Cross-instance migration, declarative versioning, human-readable editing | `super export` → `super apply` | Output is human-readable and Git-friendly, but UUIDs are **reassigned** (apply matches services by name), and a non-Admin export has `env` values **masked**. |

Whichever approach you use, `[include]` stacks are re-applied at startup and on `super reload` — **after** the snapshot is loaded. If an included stack defines a program with the same name as one you just restored, the included definition **overrides** it, so confirm your `[include]` files won't clobber the state you are restoring.

See also [Declarative Stacks](/docs/04-production-scenarios/delivery/declarative-stack/) for the apply / export workflow.
