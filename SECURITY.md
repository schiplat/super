# Security Policy

## Supported versions

Security fixes are applied to the latest release on the `master` branch and backported to recent tagged releases at maintainers' discretion.

## Reporting a vulnerability

**Please do not open public GitHub issues for security vulnerabilities.**

Email **support@ddl.sconts.com** with:

- Description of the issue and impact
- Steps to reproduce
- Affected version(s)
- Any suggested fix (optional)

We aim to acknowledge reports within **72 hours** and will coordinate disclosure after a fix is available.

## OSS security model

The Community Edition (`superd`) does **not** implement API authentication by default. Shipped example configs use `host = "127.0.0.1"` and **`allow_insecure_public_bind = false`**, so the daemon **refuses startup** on a non-loopback bind unless you explicitly opt in or load the **security** plugin.

To bind on `0.0.0.0` or another network-facing address without the security plugin, set `allow_insecure_public_bind = true` and accept that the REST API is open to anyone who can reach the port (OSS only). **Licensed deployments must load the bundled `security` plugin** — startup fails otherwise. For token-based auth and RBAC, use `auth_secret` with the **security** plugin and a valid `[license].key` in `conf/super.toml`.

### Built-in safeguards (OSS)

Super applies defensive defaults even when no plugins are loaded:

| Safeguard | Behaviour |
| :--- | :--- |
| **Bind policy** | Fail-closed on non-loopback unless `allow_insecure_public_bind = true` or `security` plugin auth is active |
| **Log path confinement** | Custom program log paths must stay under `storage.log_dir` |
| **OTA fetch policy** | Remote artifact URLs must use HTTPS; link-local / metadata targets blocked |
| **Health probes** | HTTP(S) URLs only for outbound health checks |
| **Plugin loading** | Only files under `$SUPER_ROOT/plugins/` matching the signed license |
| **Stack includes** | `[include].files` outside `SUPER_ROOT` ignored |
| **Secret display** | API/CLI mask env values whose keys look sensitive |
| **Docs surface** | Swagger UI disabled by default (`enable_docs = false`) |

Full user-facing detail: [Configuration — OSS security defaults](https://super.docs.sconts.com/docs/02-essentials/configuration/#oss-security-defaults-fail-closed).

## Known limitation: secrets in child `/proc/<pid>/environ`

Managed processes receive `env` / `env_file` values through `execve` environment, so **sensitive values appear in `/proc/<pid>/environ`** (readable by any process running as the same UID — no ptrace permission needed). Display masking in the API/CLI is a UI layer only and does not change this. This is the same posture as other process managers (PM2 keeps them plaintext in `dump.pm2`; supervisor in its config; only systemd avoids it via privileged credential mounts), and it is mitigated by the same measures as those tools: keep secrets out of `env` where possible (`--env-file` with `chmod 600` keeps values out of `snapshot.json`), and note that dropping privileges via `user:` does not hide a service's own secrets from itself or same-UID processes.

**Status: evaluated, not implementing now.** A privileged-channel injection (memfd / fd or a credentials directory, opt-in per program) would require child cooperation and is logged as a future hardening direction rather than a GA blocker. Tracked in the changelog.

## Security self-audit

We hold the codebase to the following public standards, checked on every release branch:

- **Dependency vulnerabilities** — `cargo audit` runs against the RustSec advisory database; release branches must be clean of known-vulnerable dependencies before tagging. As of `1.4.0` the workspace scan reports **no vulnerabilities and no warnings**.
- **`unsafe` code** — `unsafe` is confined to the plugin C-ABI boundary (`core/src/plugin/`), one `pre_exec` setgroups call (`core/src/process.rs`), and test-only environment manipulation. Every `unsafe` block carries a `// SAFETY:` comment stating its invariant; these are reviewed on change.
- **Fuzz/edge inputs** — the daemon must not panic on malformed config, API payloads, or plugin responses; OTA and plugin-load paths degrade to logged errors instead.
- **CI gates** — `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, and the full integration-test suite must pass before merge.

If you find a gap between these standards and the code, that is a security bug — please report it as above.

> **Licensed plugins:** Optional subscription capabilities load as signed plugins with a vendor-supplied `[license].key`. See the [feature matrix](https://super.docs.sconts.com/docs/07-editions/feature-matrix/) and [authentication](https://super.docs.sconts.com/docs/05-advanced-management/authentication/).
