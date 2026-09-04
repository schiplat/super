# Docker image (`schiplat/super`)

Build context is the **repository root** (not this folder). Formerly `dockerbuild/` — paths elsewhere should use `packaging/docker/`.

## Runtime base image

The final stage uses **`gcr.io/distroless/cc-debian13:nonroot`** (Debian 13 / trixie stable). Distroless ships only glibc, OpenSSL, and CA certificates — no `apt`, no Perl — for a minimal runtime attack surface.

Runtime binaries: `superd`, `super`, `tini`, and a static **`busybox`** at `/usr/local/bin/busybox` (Quick Start / example-stack demos only — not a general-purpose shell).

Build stages use **`rust:1-trixie`** and **`debian:13-slim`** so compiler and helper stages match the same Debian release family.

The container runs as UID **65532**. When bind-mounting host directories, ensure they are readable/writable by that user (e.g. `chown -R 65532:65532 ./my-super-data`).

## Platforms

Published CI images target **`linux/amd64`** and **`linux/arm64`**. Native `docker build` on your machine uses your host architecture for local testing.

Verify a published image:

```bash
docker buildx imagetools inspect schiplat/super:latest
```

## Build vs run

| Stage | Mount (`-v`)? | Config source |
| :--- | :---: | :--- |
| **`docker build`** | No | `COPY packaging/docker/conf/` bakes `super.toml` into the image at `/app/super/conf/` |
| **`docker run`** (default) | No | Uses the config **inside the image** — ready to use |
| **`docker run`** (custom) | Optional | `-v ./my-conf:/app/super/conf` replaces the baked-in config |

Verify the image starts (distroless has no shell — use the HTTP port or healthcheck). **OSS image has no API authentication** — bind to loopback on the host:

```bash
docker run --rm -d -p 127.0.0.1:9002:9002 --name super-test schiplat/super:latest
curl -sf http://127.0.0.1:9002/ >/dev/null && echo OK
docker stop super-test
```

## Build

Native arch (local testing):

```bash
cd /path/to/super
docker build -f packaging/docker/Dockerfile -t schiplat/super:latest .
```

Or: `make docker`

Multi-arch publish (requires `docker login`):

```bash
make docker-multi
# or:
docker buildx build --platform linux/amd64,linux/arm64 \
  -f packaging/docker/Dockerfile -t schiplat/super:latest --push .
```

## Run

The baked-in OSS config listens on `0.0.0.0` inside the container. On the host, map **loopback only** unless you deploy the `security` plugin and a valid license:

```bash
docker run --rm -p 127.0.0.1:9002:9002 schiplat/super:latest
```

HTTP API / OSS notice: http://127.0.0.1:9002 (no embedded dashboard in the OSS image)

## Configuration

Two reference profiles ship under `packaging/docker/conf/`:

| File | Profile | Baked into image? |
| :--- | :--- | :---: |
| `super.toml` | **OSS** — `0.0.0.0` + `allow_insecure_public_bind = true`, no license | Yes (default) |
| `super.subscription.example.toml` | **Subscription** — `[license].key`, `auth_secret`, security plugin required | No — copy when mounting custom `conf/` |

| Path in container | Purpose |
| :--- | :--- |
| `/app/super/conf/super.toml` | Daemon settings (OSS default in image) |
| `/app/super/conf/conf.d/*` | Optional program stacks on startup (TOML default, JSON compatible) |
| `/app/super/data/` | Persisted program registry (`snapshot.json`) |
| `/app/super/logs/` | superd and child process logs |
| `/app/super/run/` | Runtime (optional pidfile); keep `[server].daemon = false` in containers |

> Containers must run `superd` in the **foreground** (image `ENTRYPOINT` already does). Do not set `daemon = true` or pass `--daemon` — `superd` refuses to daemonize as PID 1.

Copy and edit defaults from `packaging/docker/conf/`:

```bash
# OSS — tweak baked-in settings
cp -r packaging/docker/conf ./my-super-conf

# Subscription — start from the licensed example, add plugins/ + license key
cp packaging/docker/conf/super.subscription.example.toml ./my-super-conf/super.toml
# copy plugins/*.so into ./my-super-plugins/ and mount as /app/super/plugins
docker run --rm -p 127.0.0.1:9002:9002 \
  -v ./my-super-conf:/app/super/conf \
  -v ./my-super-plugins:/app/super/plugins \
  -v ./my-super-data:/app/super/data \
  schiplat/super:latest
```

Minimal OSS mount (config only):

```bash
cp -r packaging/docker/conf ./my-super-conf
docker run --rm -p 127.0.0.1:9002:9002 \
  -v ./my-super-conf:/app/super/conf \
  -v ./my-super-data:/app/super/data \
  schiplat/super:latest
```

To enable the sample stack, rename `conf.d/example-stack.toml.example` to `conf.d/example-stack.toml`.

## Publish to Docker Hub

### GitHub Actions (recommended)

Workflow: [`.github/workflows/docker-publish.yml`](../.github/workflows/docker-publish.yml)

| Trigger | Tags pushed |
| :--- | :--- |
| Push tag `v*` | semver tags + `latest` (`linux/amd64`, `linux/arm64`) |
| Manual **workflow_dispatch** | Same as tag publish (`latest` + metadata tags) |

Add repository secrets (**Settings → Secrets → Actions**):

| Secret | Value |
| :--- | :--- |
| `DOCKERHUB_USERNAME` | Docker Hub username (e.g. `schiplat`) — GitHub Actions **variable**, not a secret |
| `DOCKERHUB_TOKEN` | [Access token](https://hub.docker.com/settings/security) with **Read & Write** |

Release example:

```bash
git tag v1.5.3
git push origin v1.5.3
```

### Manual push

```bash
docker buildx build --platform linux/amd64,linux/arm64 \
  -f packaging/docker/Dockerfile \
  -t schiplat/super:latest \
  -t schiplat/super:1.5.3 \
  --push .
```
