#!/usr/bin/env bash
# Build a local fake release tree and smoke-test install.sh against it.
# Usage: tools/scripts/install-smoke.sh [--user|--system-skip-service]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

MODE="${1:---user}"
VER="0.0.0-smoke"
OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
  Linux) OS_PART=linux ;;
  Darwin) OS_PART=macos ;;
  FreeBSD) OS_PART=freebsd ;;
  *) echo "unsupported OS: $OS" >&2; exit 1 ;;
esac
case "$ARCH" in
  x86_64|amd64) ARCH_PART=amd64 ;;
  arm64|aarch64) ARCH_PART=arm64 ;;
  *) echo "unsupported arch: $ARCH" >&2; exit 1 ;;
esac
PLATFORM="${OS_PART}-${ARCH_PART}"
NAME="super-${VER}-${PLATFORM}"

echo "==> building release binaries"
cargo build --release -p superd -p super-cli

STAGE="$(mktemp -d)"
SRV="$STAGE/www"
trap 'kill ${SRV_PID:-0} 2>/dev/null || true; rm -rf "$STAGE"' EXIT

mkdir -p "$SRV/v${VER}" "$STAGE/$NAME/bin" \
  "$STAGE/$NAME/contrib/conf.d" \
  "$STAGE/$NAME/contrib/systemd" \
  "$STAGE/$NAME/contrib/launchd" \
  "$STAGE/$NAME/contrib/rc.d"

cp target/release/superd target/release/super "$STAGE/$NAME/bin/"
chmod +x "$STAGE/$NAME/bin/"*
if [[ -f LICENSE ]]; then cp LICENSE "$STAGE/$NAME/"; fi
if [[ -d packaging/contrib ]]; then
  cp packaging/contrib/super.toml.default "$STAGE/$NAME/contrib/" 2>/dev/null || true
  cp packaging/contrib/README.md "$STAGE/$NAME/contrib/" 2>/dev/null || true
  cp packaging/contrib/conf.d/demo.toml.example "$STAGE/$NAME/contrib/conf.d/" 2>/dev/null || true
  cp packaging/contrib/systemd/superd.service "$STAGE/$NAME/contrib/systemd/" 2>/dev/null || true
  cp packaging/contrib/launchd/com.schiplat.superd.plist "$STAGE/$NAME/contrib/launchd/" 2>/dev/null || true
  cp packaging/contrib/rc.d/superd "$STAGE/$NAME/contrib/rc.d/" 2>/dev/null || true
fi
bash .github/scripts/write-release-readme.sh "$VER" "$PLATFORM" "$STAGE/$NAME"

(
  cd "$STAGE"
  tar -czf "$SRV/v${VER}/${NAME}.tar.gz" "$NAME"
)
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$SRV/v${VER}" && sha256sum "${NAME}.tar.gz" > SHA256SUMS)
else
  (cd "$SRV/v${VER}" && shasum -a 256 "${NAME}.tar.gz" > SHA256SUMS)
fi
printf '{"tag_name":"v%s"}\n' "$VER" > "$SRV/latest.json"

PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$SRV" >/tmp/super-install-smoke-http.log 2>&1 &
SRV_PID=$!
for i in 1 2 3 4 5; do
  curl -fsS "http://127.0.0.1:${PORT}/latest.json" >/dev/null && break
  sleep 0.5
done

PREFIX="$STAGE/prefix"
ROOT_DIR="$STAGE/super-root"
mkdir -p "$PREFIX/bin"

INSTALL_ARGS=(--version "$VER" --base-url "http://127.0.0.1:${PORT}" --prefix "$PREFIX" --root "$ROOT_DIR" --no-sudo)
case "$MODE" in
  --user)
    INSTALL_ARGS+=(--user)
    ;;
  --no-service)
    INSTALL_ARGS+=(--no-service)
    ;;
  *)
    echo "unknown mode: $MODE (use --user or --no-service)" >&2
    exit 1
    ;;
esac

echo "==> running install.sh ${INSTALL_ARGS[*]}"
sh "$ROOT/install.sh" "${INSTALL_ARGS[@]}"

# shellcheck disable=SC1090
# shellcheck disable=SC1091
. "$ROOT_DIR/env.sh"
export PATH="$PREFIX/bin:$PATH"

# Give the OS service / --daemon a moment to bind.
sleep 2

echo "==> doctor"
super doctor

echo "==> CLI round-trip"
super add --name smoke-demo --autostart -- sleep 60
super list | grep -q smoke-demo
super stop smoke-demo --yes
super remove smoke-demo --yes

echo "==> install-smoke OK ($PLATFORM)"
