---
title: "Installation"
weight: 1
description: "Install Super via Docker, GitHub Releases, or build from source."
---

Project Super ships as static binaries (`superd`, `super`) with no runtime dependencies on Python or a JVM. Building from source requires **Rust 1.85+** (stable; edition 2024).

## Supported platforms

Super is **cross-platform** in the sense that the same `super.toml` and API workflow work wherever you run `superd` — but **how** you install depends on your OS:

| Platform | Native binaries | Typical install |
| :--- | :---: | :--- |
| **Linux** (amd64, arm64) | ✅ [GitHub Releases](https://github.com/schiplat/super/releases) | `install.sh` (systemd), Docker, or tarball |
| **macOS** (Intel, Apple Silicon) | ✅ Releases | `install.sh` (launchd) or tarball |
| **FreeBSD** (amd64) | ✅ Releases | `install.sh` (rc.d) or tarball |
| **Windows** | ❌ Not published | [Docker](#method-1-docker-recommended) on the host (Docker Desktop or WSL2) |

> [!NOTE]
> **Windows:** there is no native `superd.exe` release today. Run the official Linux container image locally, or develop on WSL2/Linux/macOS. See the warning under [Method 3](#method-3-github-releases-or-build-from-source) for details.

On any supported host, set `SUPER_ROOT`, place config under `conf/super.toml`, and use the same CLI/API — whether you extracted a tarball or started a container.

## Method 1: Docker (Recommended)

The official OSS image ships `superd` and `super` (API + CLI). There is no embedded web dashboard — install the optional UI plugin from your subscription package for the full control plane.

### Pull and run

The image ships with a default config at `/app/super/conf/super.toml` (`host = "0.0.0.0"`, port `9002`, and `allow_insecure_public_bind = true` so the container can listen on all interfaces). **The OSS image has no API authentication** — on the host, bind to loopback unless you deploy the `security` plugin and a valid license.

```bash
docker pull containerpi/super:latest

docker run --rm -p 127.0.0.1:9002:9002 containerpi/super:latest
```

Open **http://127.0.0.1:9002** for the OSS HTML notice and HTTP API. Add programs via the CLI or API (or load the `ui` plugin for the dashboard).

Images are published for **linux/amd64** and **linux/arm64**. Docker picks the matching manifest for your host (`docker buildx imagetools inspect containerpi/super:latest`).

### Custom configuration

Mount your own `conf/` (and optionally `data/` for persistence):

```bash
docker run --rm -p 127.0.0.1:9002:9002 \
  -v /path/to/conf:/app/super/conf \
  -v /path/to/data:/app/super/data \
  containerpi/super:latest
```

Place `super.toml` under `/path/to/conf/`. Reference profiles in `dockerbuild/conf/`:

- **`super.toml`** — OSS default baked into the image (`allow_insecure_public_bind = true` for container networking). How to structure the file and every supported key: [Configuration](/docs/02-essentials/configuration) and [Config Reference](/docs/06-internals/config-reference).
- **`super.subscription.example.toml`** — subscription template (not a runtime config — copy its contents into `super.toml`) with `[license].key`, `auth_secret`, and security plugin expectations. Parameter details: [Config Reference](/docs/06-internals/config-reference). Mandatory security plugin + `auth_secret` at startup: [Licensed deployments require security](/docs/05-advanced-management/authentication#licensed-deployments-require-security).

Drop stack files into `conf/conf.d/*` (TOML by default; legacy `.json` also works) to seed programs on startup.

If you bind to `0.0.0.0` or another non-loopback address, set `allow_insecure_public_bind = true` in `[server]` (or load the **`security` plugin`). The repo's `example/conf/super.toml` sets this to `false` for local-only deployments.

If you add licensed plugins, **`security.so` and `auth_secret` are required** for startup — security is included with every subscription. See [Licensed deployments require security](/docs/05-advanced-management/authentication#licensed-deployments-require-security).

### Build from this repository

```bash
git clone https://github.com/schiplat/super.git
cd super
docker build -f dockerbuild/Dockerfile -t containerpi/super:latest .
```

Or: `make docker`. See [dockerbuild/README.md](https://github.com/schiplat/super/blob/master/dockerbuild/README.md) for publish notes.

### Use as a base image in your stack

```dockerfile
FROM ubuntu:22.04

COPY --from=containerpi/super:latest /usr/local/bin/superd /usr/local/bin/superd
COPY --from=containerpi/super:latest /usr/local/bin/super /usr/local/bin/super

COPY conf/ /app/super/conf/

RUN apt-get update && apt-get install -y --no-install-recommends tini \
    && rm -rf /var/lib/apt/lists/*

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/superd"]
```

For container signal handling and `tini` guidance, see [Zombie reaping in containers](/docs/04-production-scenarios/stability/zombie-reaping-in-containers). For how Super compares to Supervisor or shell entrypoints, see [vs Supervisor](/docs/04-production-scenarios/migrations/vs-supervisor).

## Method 2: `install.sh` (recommended on Linux / macOS / FreeBSD)

One-liner from the **latest GitHub Release** asset (pinned to the tagged binaries):

```bash
curl -fsSL https://github.com/schiplat/super/releases/latest/download/install.sh | sh
```

Bleeding-edge script from `master` (may download a different release than the script revision):  
`curl -fsSL https://raw.githubusercontent.com/schiplat/super/master/install.sh | sh`

What it does by default:

1. Downloads the matching release archive and verifies `SHA256SUMS`
2. Installs `superd` + `super` into `/usr/local/bin` (or `~/.local/bin` if not writable)
3. Creates a minimal instance root:
   - **System install:** `/opt/super`
   - **User install:** `~/.super`
   - Layout: `conf/super.toml`, `conf/conf.d/`, `data/`, `logs/`, `run/`, `plugins/`, plus `env.sh`
4. Wires **login environment** so `SUPER_ROOT` (and `bin` on `PATH` when needed) is set automatically:
   - **Linux (system):** `/etc/profile.d/super.sh` + `SUPER_ROOT` in `/etc/environment`
   - **macOS (system):** `/etc/paths.d/super` + hooks in `/etc/zprofile` (and bash/profile)
   - **User install:** marked block in `~/.zprofile` / `~/.profile` (and existing `~/.bash_profile` when present)
5. Enables and starts an OS service (boot-persistent):
   - **Linux:** `superd.service` via systemd (`enable --now`)
   - **macOS:** `com.schiplat.superd` via launchd (`RunAtLoad` + `KeepAlive`)
   - **FreeBSD:** `/usr/local/etc/rc.d/superd` + `/etc/rc.conf.d/superd` (`superd_enable=YES`); `--user` uses `superd --daemon`

Useful flags:

| Flag | Effect |
| :--- | :--- |
| `--version X.Y.Z` | Pin a release |
| `--prefix DIR` | Binary prefix (`DIR/bin`) |
| `--root DIR` | Instance `SUPER_ROOT` |
| `--user` / `--system` | Force per-user or system-wide service |
| `--no-service` | Binaries + instance only (no systemd/launchd/rc.d) |
| `--no-start` | Install/enable service but do not start yet |
| `--no-init` | Skip creating `SUPER_ROOT` |
| `--no-sudo` | Never elevate |
| `--base-url URL` | Local/CI fake release server (see `scripts/install-smoke.sh`) |

After install (open a **new** terminal so login env applies):

```bash
super doctor
super add --name demo --autostart sleep 3600
super list
```

In the same shell as the installer, run `source /opt/super/env.sh` (or `~/.super/env.sh`) once.

### Upgrade / reinstall

Re-running `install.sh` is safe as an upgrade path:

- Overwrites `superd` / `super` binaries and refreshes the OS unit / plist / rc.d script
- **Keeps** an existing `$SUPER_ROOT/conf/super.toml` (does not clobber local edits)
- Rewrites `$SUPER_ROOT/env.sh` and login hooks; restarts the service when start is enabled

Pin with `--version X.Y.Z` when you need a specific release.

### Uninstall (manual)

There is no `--uninstall` yet. Typical cleanup:

```bash
# Linux (system)
sudo systemctl disable --now superd
sudo rm -f /etc/systemd/system/superd.service /etc/profile.d/super.sh
sudo systemctl daemon-reload
# optional: remove SUPER_ROOT= from /etc/environment
# optional: sudo rm -rf /opt/super /usr/local/bin/superd /usr/local/bin/super

# macOS (system)
sudo launchctl bootout system/com.schiplat.superd
sudo rm -f /Library/LaunchDaemons/com.schiplat.superd.plist /etc/paths.d/super
# remove Project Super marker blocks from /etc/zprofile (and bash/profile) if present

# FreeBSD (system)
sudo service superd stop
sudo sysrc -f /etc/rc.conf.d/superd superd_enable=NO
sudo rm -f /usr/local/etc/rc.d/superd /etc/rc.conf.d/superd
```

User installs: `systemctl --user disable --now superd` or `launchctl bootout gui/$(id -u)/com.schiplat.superd`, then remove `~/.super` and `~/.local/bin/super{,d}` as needed.

> [!IMPORTANT]
> `install.sh` sets `SUPER_ROOT` for login sessions (see above). Always keep the daemon’s service `Environment=SUPER_ROOT=…` in sync with `$SUPER_ROOT/env.sh`. Binaries under `/usr/local/bin` must not infer the instance root from the executable path — that would incorrectly pick `/usr/local`.
>
> **System install + Unix socket:** default `socket = "run/superd.sock"` is mode `0600` owned by the service user (often root). Non-root CLI either uses `sudo -E super …`, `super --server http://127.0.0.1:9002 …` (loopback TCP still listens), or set `socket_mode = "0660"` and share a group on `$SUPER_ROOT/run`.

## Method 3: GitHub Releases or build from source

Pre-built archives are published on [GitHub Releases](https://github.com/schiplat/super/releases). Prefer [`install.sh`](#method-2-installsh-recommended-on-linux--macos--freebsd) when you want systemd/launchd/rc.d wired automatically. For a manual extract:

| Archive | Platform |
| :--- | :--- |
| `super-{version}-linux-amd64.tar.gz` | Linux x86_64 |
| `super-{version}-linux-arm64.tar.gz` | Linux ARM64 |
| `super-{version}-macos-amd64.tar.gz` | macOS Intel |
| `super-{version}-macos-arm64.tar.gz` | macOS Apple Silicon |
| `super-{version}-freebsd-amd64.tar.gz` | FreeBSD x86_64 |

Each archive contains `bin/superd`, `bin/super`, `contrib/` (default config + unit templates), and a `README`. A `SHA256SUMS` file is attached to every release.

> [!WARNING]
> Pre-built **Windows** binaries are **not published** at this time. On Windows hosts, use [Docker](#method-1-docker-recommended) (Docker Desktop or WSL2) — see [Supported platforms](#supported-platforms). You can also build from source on Linux, macOS, or FreeBSD.

To build locally (requires **Rust 1.85+**):

```bash
git clone https://github.com/schiplat/super.git
cd super
make build

./target/release/superd --help
./target/release/super --version
```

## Method 4: Systemd / launchd / rc.d (manual)

### Linux — systemd

`/etc/systemd/system/superd.service` (also in `contrib/systemd/superd.service`):

```ini
[Unit]
Description=Project Super Process Manager
Documentation=https://super.docs.sconts.com/docs/
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
# Must stay in the foreground. Do not set [server].daemon = true or pass --daemon.
ExecStart=/usr/local/bin/superd --foreground
Restart=on-failure
RestartSec=2
Environment="SUPER_ROOT=/opt/super"
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now superd
sudo systemctl status superd
```

Per-user units live under `~/.config/systemd/user/` (`systemctl --user …`). For boot without an interactive login: `loginctl enable-linger $USER`.

### macOS — launchd

macOS has no systemd. Use **launchd** (what `install.sh` installs) so `superd` stays up across reboots and crashes — same idea as a systemd unit: keep the process in the **foreground** (`--foreground`), let the OS restart it (`KeepAlive`).

System-wide: `/Library/LaunchDaemons/com.schiplat.superd.plist`  
Per-user: `~/Library/LaunchAgents/com.schiplat.superd.plist`

Templates ship in `contrib/launchd/`. After copying and editing paths / `SUPER_ROOT`:

```bash
# system
sudo launchctl bootstrap system /Library/LaunchDaemons/com.schiplat.superd.plist
# user
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.schiplat.superd.plist
```

### FreeBSD — rc.d

FreeBSD uses **rc.d** (what `install.sh` installs under `/usr/local/etc/rc.d/superd`). The script wraps `superd --foreground` with [`daemon(8)`](https://man.freebsd.org/cgi/man.cgi?daemon) (`-r` restarts on exit). Do **not** set `[server].daemon` or pass `--daemon` when using rc.d.

```bash
# enable + paths (install.sh writes /etc/rc.conf.d/superd)
sysrc -f /etc/rc.conf.d/superd superd_enable=YES
sysrc -f /etc/rc.conf.d/superd superd_root=/opt/super
sysrc -f /etc/rc.conf.d/superd superd_bin=/usr/local/bin/superd

service superd start
service superd status
```

Template: `contrib/rc.d/superd`. Per-user installs (`--user`) have no rc.d — use `superd --daemon` instead.

> [!NOTE]
> Default layout is `$SUPER_ROOT/conf/super.toml`. Set `SUPER_ROOT` if your layout differs (see [Environment Variables](/docs/06-internals/environment-variables#super_root)).
>
> **Daemonize without systemd / launchd / rc.d:** `superd --daemon` (or `[server] daemon = true`) writes `$SUPER_ROOT/run/superd.pid` by default. Do **not** combine that with an OS service unit. Stop with `super shutdown` as usual.
