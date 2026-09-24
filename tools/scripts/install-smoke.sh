#!/usr/bin/env bash
# Build a local fake release tree and smoke-test install.sh against it.
# Usage: tools/scripts/install-smoke.sh [--user|--no-service]
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

case "$MODE" in
  --user|--no-service) ;;
  *) echo "unknown mode: $MODE (use --user or --no-service)" >&2; exit 1 ;;
esac

# Keep the installer HOME untouched so --no-service cannot alter the invoking
# user's shell files, even when PREFIX points into that user's home.
TMP_ROOT="${TMPDIR:-/tmp}"
mkdir -p "$TMP_ROOT"
STAGE="$(mktemp -d "$TMP_ROOT/super-install-smoke.XXXXXX")"
SRV="$STAGE/www"
SRV_PID=""
DAEMON_PID=""
cleanup() {
  if [ -n "$DAEMON_PID" ]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
  if [ -n "$SRV_PID" ]; then
    kill "$SRV_PID" 2>/dev/null || true
    wait "$SRV_PID" 2>/dev/null || true
  fi
  rm -rf "$STAGE"
}
trap cleanup EXIT

# Test-only state is isolated under STAGE. Do not modify HOME/ZDOTDIR or
# configure an OS service; the daemon is launched directly and reaped below.
export SUPER_ROOT="$STAGE/super-root"
export TMPDIR="$STAGE/tmp"
mkdir -p "$TMPDIR" "$SUPER_ROOT"

# An explicit environment variable can select an offline Swagger UI archive;
# let Cargo's build script consume it when one is provided by CI/local config.
echo "==> building release binaries"
cargo build --release -p superd -p super-cli

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
  cp packaging/contrib/launchd/com.schiplat.superd.plist "$STAGE/$NAME/contrib/" 2>/dev/null || true
  cp packaging/contrib/rc.d/superd "$STAGE/$NAME/contrib/rc.d/" 2>/dev/null || true
fi
bash .github/scripts/write-release-readme.sh "$VER" "$PLATFORM" "$STAGE/$NAME"

(
  cd "$STAGE"
  # macOS bsdtar may store AppleDouble/resource-fork metadata. Exclude it on
  # all platforms, and use metadata-suppression options only with bsdtar.
  if [[ "$OS" == "Darwin" ]]; then
    COPYFILE_DISABLE=1 tar --no-xattrs --no-mac-metadata --exclude='.DS_Store' --exclude='._*' -czf "$SRV/v${VER}/${NAME}.tar.gz" "$NAME"
  else
    tar --exclude='.DS_Store' --exclude='._*' -czf "$SRV/v${VER}/${NAME}.tar.gz" "$NAME"
  fi
)
if tar -tzf "$SRV/v${VER}/${NAME}.tar.gz" | grep -E '(^|/)(\._[^/]*|\.DS_Store)$'; then
  echo "release archive contains macOS metadata entries" >&2
  exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$SRV/v${VER}" && sha256sum "${NAME}.tar.gz" > SHA256SUMS)
else
  (cd "$SRV/v${VER}" && shasum -a 256 "${NAME}.tar.gz" > SHA256SUMS)
fi
printf '{"tag_name":"v%s"}\n' "$VER" > "$SRV/latest.json"

PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$SRV" >"$TMPDIR/http.log" 2>&1 &
SRV_PID=$!
for _ in 1 2 3 4 5 6 7 8 9 10; do
  if curl -fsS "http://127.0.0.1:${PORT}/latest.json" >/dev/null; then
    break
  fi
  sleep 0.5
done
curl -fsS "http://127.0.0.1:${PORT}/latest.json" >/dev/null

PREFIX="$STAGE/prefix"
ROOT_DIR="$STAGE/super-root"
HOME="$STAGE/home"
ZDOTDIR="$STAGE/home"
XDG_CONFIG_HOME="$STAGE/home/.config"
export HOME ZDOTDIR XDG_CONFIG_HOME
mkdir -p "$PREFIX/bin" "$HOME" "$XDG_CONFIG_HOME"

INSTALL_ARGS=(--user --version "$VER" --base-url "http://127.0.0.1:${PORT}" --prefix "$PREFIX" --root "$ROOT_DIR" --no-sudo --no-service)
echo "==> running isolated install.sh ${INSTALL_ARGS[*]}"
if ! INSTALL_OUTPUT="$(sh "$ROOT/install.sh" "${INSTALL_ARGS[@]}" 2>&1)"; then
  printf '%s\n' "$INSTALL_OUTPUT" >&2
  echo "initial isolated install failed" >&2
  exit 1
fi
printf '%s\n' "$INSTALL_OUTPUT"

if [[ "$INSTALL_OUTPUT" == *"launchd"* || "$INSTALL_OUTPUT" == *"systemd"* || "$INSTALL_OUTPUT" == *"rc.d"* ]]; then
  echo "install unexpectedly registered a host service" >&2
  exit 1
fi

# Reinstall over an existing installation: preserve config and keep one profile hook.
printf '\n# user-owned smoke sentinel\n' >> "$ROOT_DIR/conf/super.toml"
cp "$ROOT_DIR/conf/super.toml" "$TMPDIR/super.toml.before-reinstall"
if ! sh "$ROOT/install.sh" "${INSTALL_ARGS[@]}" > "$TMPDIR/reinstall.log" 2>&1; then
  cat "$TMPDIR/reinstall.log" >&2
  echo "repeat install failed" >&2
  exit 1
fi
cmp "$TMPDIR/super.toml.before-reinstall" "$ROOT_DIR/conf/super.toml"
echo "==> repeat install preserved config"
PROFILE_HOOKS="$(grep -Fxc '# >>> Project Super >>>' "$HOME/.profile" || true)"
if [ "$PROFILE_HOOKS" -ne 1 ]; then
  echo "expected one Project Super profile hook, found $PROFILE_HOOKS" >&2
  cat "$HOME/.profile" >&2
  exit 1
fi
echo "==> repeat install profile hook remained unique"

# Fail before binary replacement at each artifact-validation boundary. Existing
# binaries and user config must remain byte-for-byte unchanged.
cp "$PREFIX/bin/superd" "$TMPDIR/superd.before-invalid-releases"
cp "$PREFIX/bin/super" "$TMPDIR/super.before-invalid-releases"
cp "$ROOT_DIR/conf/super.toml" "$TMPDIR/config.before-invalid-releases"
mkdir -p "$SRV/fail-download/v${VER}" "$SRV/fail-checksum/v${VER}" "$SRV/fail-extract/v${VER}"
cp "$SRV/v${VER}/${NAME}.tar.gz" "$SRV/fail-checksum/v${VER}/${NAME}.tar.gz"
printf '%064d  %s\n' 0 "$NAME.tar.gz" > "$SRV/fail-checksum/v${VER}/SHA256SUMS"
printf 'not a gzip archive\n' > "$SRV/fail-extract/v${VER}/${NAME}.tar.gz"
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$SRV/fail-extract/v${VER}" && sha256sum "$NAME.tar.gz" > SHA256SUMS)
else
  (cd "$SRV/fail-extract/v${VER}" && shasum -a 256 "$NAME.tar.gz" > SHA256SUMS)
fi

expect_preinstall_failure() {
  _scenario="$1"
  _expected="$2"
  _base_url="http://127.0.0.1:${PORT}/fail-${_scenario}"
  _fail_args=(--user --version "$VER" --base-url "$_base_url" --prefix "$PREFIX" --root "$ROOT_DIR" --no-sudo --no-service)
  if sh "$ROOT/install.sh" "${_fail_args[@]}" > "$TMPDIR/failure-${_scenario}.log" 2>&1; then
    cat "$TMPDIR/failure-${_scenario}.log" >&2
    echo "installer unexpectedly succeeded for $_scenario failure" >&2
    exit 1
  fi
  if ! grep -qiE "$_expected" "$TMPDIR/failure-${_scenario}.log"; then
    cat "$TMPDIR/failure-${_scenario}.log" >&2
    echo "installer failure did not report expected $_scenario diagnostic" >&2
    exit 1
  fi
  cmp "$TMPDIR/superd.before-invalid-releases" "$PREFIX/bin/superd"
  cmp "$TMPDIR/super.before-invalid-releases" "$PREFIX/bin/super"
  cmp "$TMPDIR/config.before-invalid-releases" "$ROOT_DIR/conf/super.toml"
  echo "==> $_scenario failure preserved installed binaries and config"
}

expect_preinstall_failure download 'download failed'
expect_preinstall_failure checksum 'checksum mismatch'
expect_preinstall_failure extract 'gzip|tar|archive'

# Add markers to the previous binaries, then fail while promoting the second
# new executable. The installer must restore this exact prior pair.
cp "$PREFIX/bin/superd" "$TMPDIR/superd.unmarked"
cp "$PREFIX/bin/super" "$TMPDIR/super.unmarked"
printf '\nold-superd-marker\n' >> "$PREFIX/bin/superd"
printf '\nold-super-cli-marker\n' >> "$PREFIX/bin/super"
cp "$PREFIX/bin/superd" "$TMPDIR/superd.good"
cp "$PREFIX/bin/super" "$TMPDIR/super.good"
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum "$PREFIX/bin/superd" "$PREFIX/bin/super" > "$TMPDIR/binaries.before-failure"
else
  shasum -a 256 "$PREFIX/bin/superd" "$PREFIX/bin/super" > "$TMPDIR/binaries.before-failure"
fi
FAIL_MV_DIR="$STAGE/fail-mv-bin"
mkdir -p "$FAIL_MV_DIR"
cat > "$FAIL_MV_DIR/mv" <<'EOF'
#!/bin/sh
case "${1-}" in
  "$PREFIX/bin"/.super.new.*)
    if [ "${2-}" = "$PREFIX/bin/super" ] && [ ! -e "$STAGE/mv-failed-once" ]; then
      : > "$STAGE/mv-failed-once"
      echo 'injected promotion failure' >&2
      exit 97
    fi
    ;;
esac
exec /bin/mv "$@"
EOF
export PREFIX STAGE
chmod +x "$FAIL_MV_DIR/mv"
if PATH="$FAIL_MV_DIR:$PATH" sh "$ROOT/install.sh" "${INSTALL_ARGS[@]}" > "$TMPDIR/failed-upgrade.log" 2>&1; then
  echo "injected failed reinstall unexpectedly succeeded" >&2
  exit 1
fi
cat "$TMPDIR/failed-upgrade.log"
if ! grep -q 'Restoring previous binaries after incomplete install' "$TMPDIR/failed-upgrade.log"; then
  cat "$TMPDIR/failed-upgrade.log" >&2
  echo "failed reinstall did not run binary rollback" >&2
  exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum "$PREFIX/bin/superd" "$PREFIX/bin/super" > "$TMPDIR/binaries.after-failure"
else
  shasum -a 256 "$PREFIX/bin/superd" "$PREFIX/bin/super" > "$TMPDIR/binaries.after-failure"
fi
cmp "$TMPDIR/binaries.before-failure" "$TMPDIR/binaries.after-failure"
cmp "$TMPDIR/superd.good" "$PREFIX/bin/superd"
cmp "$TMPDIR/super.good" "$PREFIX/bin/super"
echo "==> failed upgrade restored old binaries"
shopt -s nullglob
LEFTOVERS=("$PREFIX/bin/.superd.new."* "$PREFIX/bin/.super.new."* "$PREFIX/bin/.superd.old."* "$PREFIX/bin/.super.old."*)
shopt -u nullglob
if [ "${#LEFTOVERS[@]}" -ne 0 ]; then
  printf 'leftover transaction files: %s\n' "${LEFTOVERS[@]}" >&2
  exit 1
fi
echo "==> failed upgrade left no transaction files"

# Install without a service, then launch a foreground daemon only inside the
# temporary SUPER_ROOT. Its lifecycle is tied to this script's EXIT trap.
export PATH="$PREFIX/bin:$PATH"
"$PREFIX/bin/superd" --foreground >"$TMPDIR/superd.log" 2>&1 &
DAEMON_PID=$!

READY=0
for _ in 1 2 3 4 5 6 7 8 9 10; do
  if super doctor >"$TMPDIR/doctor.log" 2>&1; then
    READY=1
    break
  fi
  if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
    cat "$TMPDIR/superd.log" >&2
    echo "temporary superd exited before becoming healthy" >&2
    exit 1
  fi
  sleep 1
done
if [ "$READY" -ne 1 ]; then
  cat "$TMPDIR/doctor.log" >&2
  cat "$TMPDIR/superd.log" >&2
  echo "temporary daemon did not become healthy" >&2
  exit 1
fi
cat "$TMPDIR/doctor.log"

echo "==> CLI round-trip"
super add --name smoke-demo --autostart -- sleep 60
super list | grep -q smoke-demo
super stop smoke-demo --yes
super remove smoke-demo --yes
super shutdown >/dev/null 2>&1 || true
kill "$DAEMON_PID" 2>/dev/null || true
wait "$DAEMON_PID" 2>/dev/null || true
DAEMON_PID=""

echo "==> install-smoke OK ($PLATFORM, no host service)"
