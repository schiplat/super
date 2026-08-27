# Docker image (`containerpi/super`)

Build context is the **repository root** (not this folder).

## Runtime base image

The final stage uses **`gcr.io/distroless/cc-debian13:nonroot`** (Debian 13 / trixie stable). Distroless ships only glibc, OpenSSL, and CA certificates — no `apt`, no Perl — for a minimal runtime attack surface.

Build stages use **`rust:1-trixie`** and **`debian:13-slim`** so compiler and helper stages match the same Debian release family.

The container runs as UID **65532**. When bind-mounting host directories, ensure they are readable/writable by that user (e.g. `chown -R 65532:65532 ./my-super-data`).

## Platforms

Published CI images target **`linux/amd64`** and **`linux/arm64`**. Native `docker build` on your machine uses your host architecture for local testing.

Verify a published image:

```bash
docker buildx imagetools inspect containerpi/super:latest
```

## Build vs run

| Stage | Mount (`-v`)? | Config source |
| :--- | :---: | :--- |
| **`docker build`** | No | `COPY dockerbuild/conf/` bakes `super.toml` into the image at `/app/super/conf/` |
| **`docker run`** (default) | No | Uses the config **inside the image** — ready to use |
| **`docker run`** (custom) | Optional | `-v ./my-conf:/app/super/conf` replaces the baked-in config |

Verify the image starts (distroless has no shell — use the HTTP port or healthcheck):

```bash
docker run --rm -d -p 9002:9002 --name super-test containerpi/super:latest
curl -sf http://localhost:9002/ >/dev/null && echo OK
docker stop super-test
```

## Build

Native arch (local testing):

```bash
cd /path/to/super
docker build -f dockerbuild/Dockerfile -t containerpi/super:latest .
```

Or: `make docker`

Multi-arch publish (requires `docker login`):

```bash
make docker-multi
# or:
docker buildx build --platform linux/amd64,linux/arm64 \
  -f dockerbuild/Dockerfile -t containerpi/super:latest --push .
```

## Run

```bash
docker run --rm -p 9002:9002 containerpi/super:latest
```

HTTP API / OSS notice: http://localhost:9002 (no embedded dashboard in the OSS image)

## Configuration

Two reference profiles ship under `dockerbuild/conf/`:

| File | Profile | Baked into image? |
| :--- | :--- | :---: |
| `super.toml` | **OSS** — `0.0.0.0` + `allow_insecure_public_bind = true`, no license | Yes (default) |
| `super.subscription.example.toml` | **Subscription** — `[license].key`, `auth_secret`, security plugin required | No — copy when mounting custom `conf/` |

| Path in container | Purpose |
| :--- | :--- |
| `/app/super/conf/super.toml` | Daemon settings (OSS default in image) |
| `/app/super/conf/conf.d/*.json` | Optional program stacks on startup |
| `/app/super/data/` | Persisted program registry (`snapshot.json`) |
| `/app/super/logs/` | superd and child process logs |
| `/app/super/run/` | Runtime (optional pidfile); keep `[server].daemon = false` in containers |

> Containers must run `superd` in the **foreground** (image `ENTRYPOINT` already does). Do not set `daemon = true` or pass `--daemon` — `superd` refuses to daemonize as PID 1.

Copy and edit defaults from `dockerbuild/conf/`:

```bash
# OSS — tweak baked-in settings
cp -r dockerbuild/conf ./my-super-conf

# Subscription — start from the licensed example, add plugins/ + license key
cp dockerbuild/conf/super.subscription.example.toml ./my-super-conf/super.toml
# copy plugins/*.so into ./my-super-plugins/ and mount as /app/super/plugins
docker run --rm -p 9002:9002 \
  -v ./my-super-conf:/app/super/conf \
  -v ./my-super-plugins:/app/super/plugins \
  -v ./my-super-data:/app/super/data \
  containerpi/super:latest
```

Minimal OSS mount (config only):

```bash
cp -r dockerbuild/conf ./my-super-conf
docker run --rm -p 9002:9002 \
  -v ./my-super-conf:/app/super/conf \
  -v ./my-super-data:/app/super/data \
  containerpi/super:latest
```

To enable the sample stack, rename `conf.d/example-stack.json.example` to `conf.d/example-stack.json`.

## Publish to Docker Hub

### GitHub Actions (recommended)

Workflow: [`.github/workflows/docker-publish.yml`](../.github/workflows/docker-publish.yml)

| Trigger | Tags pushed |
| :--- | :--- |
| Push to `master` (relevant paths) | `containerpi/super:latest` (`linux/amd64`, `linux/arm64`) |
| Push tag `v*` | semver tags + `latest` (`linux/amd64`, `linux/arm64`) |
| Manual **workflow_dispatch** | Same rules as above |

Add repository secrets (**Settings → Secrets → Actions**):

| Secret | Value |
| :--- | :--- |
| `DOCKERHUB_USERNAME` | Docker Hub username (e.g. `containerpi`) |
| `DOCKERHUB_TOKEN` | [Access token](https://hub.docker.com/settings/security) with **Read & Write** |

Release example:

```bash
git tag v1.1.9
git push origin v1.1.9
```

### Manual push

```bash
docker buildx build --platform linux/amd64,linux/arm64 \
  -f dockerbuild/Dockerfile \
  -t containerpi/super:latest \
  -t containerpi/super:1.1.9 \
  --push .
```
