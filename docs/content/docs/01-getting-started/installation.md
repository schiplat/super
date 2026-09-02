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
| **Linux** (amd64, arm64) | ✅ [GitHub Releases](https://github.com/schiplat/super/releases) | Binary tarball, Docker, or systemd |
| **macOS** (Intel, Apple Silicon) | ✅ Releases | Binary tarball or `install.sh` |
| **FreeBSD** (amd64) | ✅ Releases | Binary tarball |
| **Windows** | ❌ Not published | [Docker](#method-1-docker-recommended) on the host (Docker Desktop or WSL2) |

> [!NOTE]
> **Windows:** there is no native `superd.exe` release today. Run the official Linux container image locally, or develop on WSL2/Linux/macOS. See the warning under [Method 2](#method-2-github-releases-or-build-from-source) for details.

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

## Method 2: GitHub Releases or build from source

Pre-built archives are published on [GitHub Releases](https://github.com/schiplat/super/releases). Extract and run `bin/superd`.

| Archive | Platform |
| :--- | :--- |
| `super-{version}-linux-amd64.tar.gz` | Linux x86_64 |
| `super-{version}-linux-arm64.tar.gz` | Linux ARM64 |
| `super-{version}-macos-amd64.tar.gz` | macOS Intel |
| `super-{version}-macos-arm64.tar.gz` | macOS Apple Silicon |
| `super-{version}-freebsd-amd64.tar.gz` | FreeBSD x86_64 |

Each archive contains `bin/superd`, `bin/super`, and a `README` with quick-start steps and source links. A `SHA256SUMS` file is attached to every release.

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

## Method 3: Systemd (VM / bare metal)

### 1. Create unit file

`/etc/systemd/system/superd.service`:

```ini
[Unit]
Description=Project Super Process Manager
After=network.target

[Service]
Type=simple
# Must stay in the foreground. Do not set [server].daemon = true or pass --daemon.
ExecStart=/usr/local/bin/superd --foreground
Restart=always
User=root
Environment=SUPER_ROOT=/opt/super

[Install]
WantedBy=multi-user.target
```

### 2. Enable and start

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now superd
sudo systemctl status superd
```

> [!NOTE]
> Default layout is `$SUPER_ROOT/conf/super.toml`. Set `SUPER_ROOT` if your layout differs (see [Environment Variables](/docs/06-internals/environment-variables#super_root)).
>
> **Daemonize without systemd:** `superd --daemon` (or `[server] daemon = true`) writes `$SUPER_ROOT/run/superd.pid` by default. Do **not** combine that with this unit — `superd` refuses to start if both are detected. Stop with `super shutdown` as usual.
