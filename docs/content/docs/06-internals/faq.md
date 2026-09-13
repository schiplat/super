---
title: "Technical FAQ"
weight: 2
description: "Common questions about system integration and internals."
---

## Super vs Systemd?

**Q: Why use Super when Systemd exists?**

**A:** Systemd is a **System-level** init system. Super is an **Application-level** supervisor.
*   **Use Systemd** to boot the OS and start the Super daemon.
*   **Use Super** to manage your application stack (API, Worker, DB).
*   **Why?**
    1.  **Docker**: Systemd is heavy/impossible to run inside containers. Super is native to Docker.
    2.  **Unified API**: Systemd varies by Linux distro. Super provides a consistent JSON API across Ubuntu, Alpine, macOS, and Dev containers.
    3.  **App-Aware**: Systemd doesn't understand "Health Checks" via HTTP or "Atomic Binary Swaps".

## Zombie Processes

**Q: How does Super handle Zombies?**

**A:** Super does **not** act as a global zombie reaper. Managed apps must follow the [Managed Program Requirements](/docs/02-essentials/process-management-contract): run in the foreground, do not escape the process group, and rely on the host init (systemd, Tini) for PID 1 duties.

In short, Super tracks direct children with `child.wait()` and tears down process groups on stop; it does not reap zombies from misbehaving grandchild processes.

For deployment patterns (including `tini` in Docker), see [Container Deployment](/docs/04-production-scenarios/stability/zombie-reaping-in-containers).

## Log Truncation

**Q: Why are my log lines cut off?**

**A:** To protect the daemon's memory stability and WebSocket bandwidth, Super truncates any single log line longer than **`max_line_size_kb`** (default **16KB**).
If an application goes into a loop printing 100MB lines, it would otherwise crash the supervisor (OOM). We prioritize the stability of the management plane over the completeness of a runaway log line. Raise `[child_logging].max_line_size_kb` if you need longer lines locally. On-disk child logs are also bounded by rotation (`max_size_mb` / `max_backups`) — see [Logging — Retention and completeness](/docs/02-essentials/logging#retention-and-completeness).

## Lost Admin Token

**Q: What do I do if I lose my Admin Access Token?**

**A:** Access tokens are shown **only once** when created — the plaintext `sk-...` secret is returned by `super token create` / the API and then discarded; only its SHA-256 hash is persisted in `$SUPER_ROOT/data/tokens.json`, so a lost secret **cannot be recovered from storage**. How to regain access depends on the situation:

| Situation | How to regain access |
| :--- | :--- |
| `auth_secret` still enabled | Just sign in with `auth_secret` (`super login <auth_secret>` or the Dashboard) — it coexists with tokens. Then revoke the lost token (`super token revoke <id>`) and create a replacement. |
| `auth_secret` disabled, another Admin token exists | Sign in with the remaining Admin token, revoke the lost one, create a new Admin token. |
| `auth_secret` disabled, **all** Admin tokens were **deleted** | **Automatic self-heal.** `superd` re-enables `auth_secret` as soon as no Admin token records remain — `ensure_auth_secret_policy` runs on every login/status call, before the disabled check. Sign in with `auth_secret` and create a new token. |
| `auth_secret` disabled, Admin tokens still stored but secrets forgotten | **Filesystem rescue** (needs write access to `$SUPER_ROOT/data/`, i.e. the user running `superd`): stop `superd`, then either set `auth_secret_disabled` to `false` in `data/auth_settings.json` (or delete that file — the default is `false`), or delete `data/tokens.json` to clear all token records, then start `superd` and sign in with `auth_secret`. |

See [Authentication — Optional: disable `auth_secret`](/docs/02-essentials/authentication#optional-disable-auth_secret) for the disable/recovery model.

## Corrupt `tokens.json`

**Q: `superd` refuses to start — “Failed to parse Access Token store …/data/tokens.json”. What now?**

**A:** With the `security` plugin loaded, Access Token records live in `$SUPER_ROOT/data/tokens.json` (SHA-256 hashes only). If that file exists but is **not valid JSON** (disk full mid-write, manual edit, restore of a truncated copy, …), **startup fails closed**. Super does **not** silently replace it with `[]` — that would wipe every token and can re-enable bootstrap `auth_secret` via the “no Admin tokens left” self-heal.

### Prefer: restore a good copy

1. Stop `superd`.
2. Restore `$SUPER_ROOT/data/tokens.json` from your backup (same host snapshot, config management, etc.).
3. Ensure permissions are owner-only: `chmod 600 "$SUPER_ROOT/data/tokens.json"`.
4. Start `superd` again.

### No backup: reset the store

This **deletes all Access Token records**. Anyone who only had an `sk-…` token must be issued a new one.

1. Stop `superd`.
2. Keep the bad file for forensics:
   ```bash
   mv "$SUPER_ROOT/data/tokens.json" "$SUPER_ROOT/data/tokens.json.corrupt"
   ```
3. If you previously **disabled** `auth_secret` login, re-enable it before starting (otherwise you may have no Bearer left after the empty store):
   - Edit `$SUPER_ROOT/data/auth_settings.json` and set `"auth_secret_disabled": false`, **or**
   - Delete `auth_settings.json` (default is enabled).
4. Confirm `[server].auth_secret` is still set in `conf/super.toml`.
5. Start `superd`. It creates a new empty `tokens.json`.
6. Sign in with `auth_secret` (`super login …` or the Dashboard), then recreate Admin / Operator / Viewer tokens.

Related: [Lost Admin Token](#lost-admin-token) · [Authentication](/docs/02-essentials/authentication/).
