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

# Linux systemd is exercised through a fake systemctl in PATH; avoid touching
# the host's launchd/rc.d service manager on other smoke platforms.
if [[ "$OS" == "Linux" ]]; then
  # Fail after initialization while installing the user service. The existing
  # config and profile must be restored exactly; newly-created env files removed.
  cp "$ROOT_DIR/conf/super.toml" "$TMPDIR/config.before-poststep-failure"
  cp "$HOME/.profile" "$TMPDIR/profile.before-poststep-failure"
  cp "$ROOT_DIR/env.sh" "$TMPDIR/env.before-poststep-failure"
  FAIL_SYSTEMCTL_DIR="$STAGE/fail-systemctl-bin"
  SYSTEMCTL_STATE="$STAGE/fail-systemctl-state"
  mkdir -p "$FAIL_SYSTEMCTL_DIR" "$SYSTEMCTL_STATE"
  cat > "$FAIL_SYSTEMCTL_DIR/systemctl" <<'EOF'
#!/bin/sh
case "${1-}" in
  --user) shift ;;
esac
case "${1-}" in
  is-enabled|is-active) exit 1 ;;
  daemon-reload)
    if [ ! -e "$SYSTEMCTL_STATE/initial-reload-failed" ]; then
      : > "$SYSTEMCTL_STATE/initial-reload-failed"
      echo "injected systemctl failure: ${1-}" >&2
      exit 96
    fi
    exit 0
    ;;
  enable|disable|stop|restart|start) exit 0 ;;
esac
exit 0
EOF
  chmod +x "$FAIL_SYSTEMCTL_DIR/systemctl"
  export SYSTEMCTL_STATE
  SERVICE_ARGS=(--user --version "$VER" --base-url "http://127.0.0.1:${PORT}" --prefix "$PREFIX" --root "$ROOT_DIR" --no-sudo)
  if PATH="$FAIL_SYSTEMCTL_DIR:$PATH" sh "$ROOT/install.sh" "${SERVICE_ARGS[@]}" > "$TMPDIR/failed-service-install.log" 2>&1; then
    cat "$TMPDIR/failed-service-install.log" >&2
    echo "install unexpectedly succeeded with injected service failure" >&2
    exit 1
  fi
  if ! grep -q 'systemd daemon-reload failed' "$TMPDIR/failed-service-install.log"; then
    cat "$TMPDIR/failed-service-install.log" >&2
    echo "systemd failure was not reported at the expected boundary" >&2
    exit 1
  fi
  cmp "$TMPDIR/config.before-poststep-failure" "$ROOT_DIR/conf/super.toml"
  cmp "$TMPDIR/profile.before-poststep-failure" "$HOME/.profile"
  cmp "$TMPDIR/env.before-poststep-failure" "$ROOT_DIR/env.sh"
  if [[ -e "$XDG_CONFIG_HOME/systemd/user/superd.service" ]]; then
    echo "failed service installation left a unit file" >&2
    exit 1
  fi
  echo "==> service registration failure restored config/profile and removed partial unit"

  # Repeat with an existing user unit and inject a start-stage failure. The
  # existing unit and enablement must survive; the newly-written state rolls back.
  SERVICE_STATE="$STAGE/fake-systemd-state"
  mkdir -p "$SERVICE_STATE" "$XDG_CONFIG_HOME/systemd/user"
  printf '[Unit]\nDescription=old smoke unit\n' > "$XDG_CONFIG_HOME/systemd/user/superd.service"
  cp "$XDG_CONFIG_HOME/systemd/user/superd.service" "$TMPDIR/old-user-unit"
  cp "$HOME/.profile" "$TMPDIR/profile.before-unit-rollback"
  cp "$ROOT_DIR/env.sh" "$TMPDIR/env.before-unit-rollback"
  cat > "$FAIL_SYSTEMCTL_DIR/systemctl" <<'EOF'
#!/bin/sh
case "${1-}" in
  --user) shift ;;
esac
case "${1-}" in
  is-enabled|is-active) exit 1 ;;
  daemon-reload|enable) exit 0 ;;
  restart|start)
    echo 'injected systemd start failure' >&2
    exit 96
    ;;
  disable|stop) exit 0 ;;
esac
exit 0
EOF
  export SERVICE_STATE
  if PATH="$FAIL_SYSTEMCTL_DIR:$PATH" sh "$ROOT/install.sh" "${SERVICE_ARGS[@]}" > "$TMPDIR/failed-unit-rollback.log" 2>&1; then
    cat "$TMPDIR/failed-unit-rollback.log" >&2
    echo "install unexpectedly succeeded with injected start failure" >&2
    exit 1
  fi
  if ! grep -q 'systemd start failed' "$TMPDIR/failed-unit-rollback.log"; then
    cat "$TMPDIR/failed-unit-rollback.log" >&2
    echo "service start failure was not reported" >&2
    exit 1
  fi
  cmp "$TMPDIR/old-user-unit" "$XDG_CONFIG_HOME/systemd/user/superd.service"
  cmp "$TMPDIR/profile.before-unit-rollback" "$HOME/.profile"
  cmp "$TMPDIR/env.before-unit-rollback" "$ROOT_DIR/env.sh"
  cmp "$TMPDIR/config.before-poststep-failure" "$ROOT_DIR/conf/super.toml"
  echo "==> service start failure restored existing unit and user state"
fi

# Exercise service registration/start failure rollback on platforms where the
# OS service interface can be safely replaced by a PATH-local test double.
if [[ "$OS" == "Darwin" ]]; then
  LAUNCHCTL_DIR="$STAGE/fake-launchctl-bin"
  LAUNCHCTL_STATE="$STAGE/fake-launchctl-state"
  mkdir -p "$LAUNCHCTL_DIR" "$LAUNCHCTL_STATE" "$HOME/Library/LaunchAgents"
  cat > "$LAUNCHCTL_DIR/launchctl" <<'EOF'
#!/bin/sh
printf '%s\n' "$*" >> "$LAUNCHCTL_STATE/calls"
case "${1-}" in
  print)
    [ -e "$LAUNCHCTL_STATE/loaded" ] && exit 0
    exit 1
    ;;
  print-disabled)
    echo 'No disabled services.'
    exit 0
    ;;
  bootout)
    rm -f "$LAUNCHCTL_STATE/loaded"
    exit 0
    ;;
  bootstrap|load)
    : > "$LAUNCHCTL_STATE/loaded"
    exit 0
    ;;
  enable)
    if [ "${LAUNCHCTL_FAIL_ENABLE:-0}" = 1 ]; then exit 96; fi
    exit 0
    ;;
  kickstart)
    if [ "${LAUNCHCTL_FAIL_START:-0}" = 1 ]; then exit 97; fi
    exit 0
    ;;
esac
exit 0
EOF
  chmod +x "$LAUNCHCTL_DIR/launchctl"
  LAUNCHD_PLIST="$HOME/Library/LaunchAgents/com.schiplat.superd.plist"
  LAUNCH_SERVICE_ARGS=(--user --version "$VER" --base-url "http://127.0.0.1:${PORT}" --prefix "$PREFIX" --root "$ROOT_DIR" --no-sudo)
  if PATH="$LAUNCHCTL_DIR:$PATH" LAUNCHCTL_STATE="$LAUNCHCTL_STATE" LAUNCHCTL_FAIL_ENABLE=1 \
    sh "$ROOT/install.sh" "${LAUNCH_SERVICE_ARGS[@]}" > "$TMPDIR/launchd-register-failure.log" 2>&1; then
    cat "$TMPDIR/launchd-register-failure.log" >&2
    echo "install unexpectedly succeeded with injected launchd enable failure" >&2
    exit 1
  fi
  grep -q 'launchd failed to enable' "$TMPDIR/launchd-register-failure.log"
  if [[ -e "$LAUNCHD_PLIST" || -e "$LAUNCHCTL_STATE/loaded" ]]; then
    echo "failed launchd registration left a plist or loaded job" >&2
    exit 1
  fi
  echo "==> launchd registration failure removed partial plist and unloaded job"

  printf '<plist>old user launchd definition</plist>\n' > "$LAUNCHD_PLIST"
  cp "$LAUNCHD_PLIST" "$TMPDIR/launchd-plist.before-start-failure"
  : > "$LAUNCHCTL_STATE/loaded"
  if PATH="$LAUNCHCTL_DIR:$PATH" LAUNCHCTL_STATE="$LAUNCHCTL_STATE" LAUNCHCTL_FAIL_START=1 \
    sh "$ROOT/install.sh" "${LAUNCH_SERVICE_ARGS[@]}" > "$TMPDIR/launchd-start-failure.log" 2>&1; then
    cat "$TMPDIR/launchd-start-failure.log" >&2
    echo "install unexpectedly succeeded with injected launchd start failure" >&2
    exit 1
  fi
  grep -q 'launchd failed to start' "$TMPDIR/launchd-start-failure.log"
  cmp "$TMPDIR/launchd-plist.before-start-failure" "$LAUNCHD_PLIST"
  [[ -e "$LAUNCHCTL_STATE/loaded" ]]
  echo "==> launchd start failure restored prior plist and loaded job"

  # --no-start must stop at the plist: bootstrapping starts the job immediately
  # (RunAtLoad + KeepAlive), so nothing may bootstrap, load, or kickstart it.
  NO_START_SERVICE_ARGS=("${LAUNCH_SERVICE_ARGS[@]}" --no-start)
  rm -f "$LAUNCHD_PLIST" "$LAUNCHCTL_STATE/calls" "$LAUNCHCTL_STATE/loaded"
  if PATH="$LAUNCHCTL_DIR:$PATH" LAUNCHCTL_STATE="$LAUNCHCTL_STATE" \
    sh "$ROOT/install.sh" "${NO_START_SERVICE_ARGS[@]}" > "$TMPDIR/launchd-no-start.log" 2>&1; then
    :
  else
    cat "$TMPDIR/launchd-no-start.log" >&2
    echo "--no-start launchd install failed" >&2
    exit 1
  fi
  if grep -Eq '^(bootstrap|kickstart|load)( |$)' "$LAUNCHCTL_STATE/calls" 2>/dev/null; then
    echo "--no-start launchd install registered or started the job" >&2
    cat "$LAUNCHCTL_STATE/calls" >&2
    exit 1
  fi
  grep -q 'starts at next login/boot' "$TMPDIR/launchd-no-start.log" || {
    cat "$TMPDIR/launchd-no-start.log" >&2
    echo "--no-start launchd install did not report the deferred start" >&2
    exit 1
  }
  [[ -e "$LAUNCHD_PLIST" ]] || { echo "--no-start launchd install wrote no plist" >&2; exit 1; }
  echo "==> --no-start launchd install only wrote the plist (starts at next login/boot)"
fi

if [[ "$OS" == "FreeBSD" ]]; then
  RC_DIR="$STAGE/fake-rc.d"
  RC_CONF_DIR="$STAGE/fake-rc.conf.d"
  SERVICE_BIN="$STAGE/fake-service-bin"
  if [[ -e /etc/rc.conf.d/superd || -e /usr/local/etc/rc.d/superd ]]; then
    echo "refusing to run rc.d service tests against host-managed paths" >&2
    exit 1
  fi
  mkdir -p "$RC_DIR" "$RC_CONF_DIR" "$SERVICE_BIN"
  cat > "$SERVICE_BIN/service" <<'EOF'
#!/bin/sh
case "${2-}" in
  status) exit 1 ;;
  restart|start) echo 'injected rc.d startup failure' >&2; exit 96 ;;
  stop) exit 0 ;;
esac
exit 0
EOF
  chmod +x "$SERVICE_BIN/service"
  RC_SERVICE_ARGS=(--system --version "$VER" --base-url "http://127.0.0.1:${PORT}" --prefix "$PREFIX" --root "$ROOT_DIR" --no-sudo)
  if PATH="$SERVICE_BIN:$PATH" SUPER_INSTALL_SMOKE=1 SUPER_INSTALL_SMOKE_RC_DIR="$RC_DIR" \
    SUPER_INSTALL_SMOKE_RC_CONF_DIR="$RC_CONF_DIR" SUPER_INSTALL_SMOKE_SERVICE_CMD="$SERVICE_BIN/service" \
    sh "$ROOT/install.sh" "${RC_SERVICE_ARGS[@]}" > "$TMPDIR/rc-register-failure.log" 2>&1; then
    cat "$TMPDIR/rc-register-failure.log" >&2
    echo "install unexpectedly succeeded with injected rc.d start failure" >&2
    exit 1
  fi
  grep -q 'service superd start failed' "$TMPDIR/rc-register-failure.log"
  if [[ -e "$RC_DIR/superd" || -e "$RC_CONF_DIR/superd" ]]; then
    echo "failed rc.d registration left partial service files" >&2
    exit 1
  fi
  echo "==> rc.d registration failure removed partial service files"

  printf '# existing rc.d service\n' > "$RC_DIR/superd"
  printf 'superd_enable="NO"\n# existing setting\n' > "$RC_CONF_DIR/superd"
  cp "$RC_DIR/superd" "$TMPDIR/rc-script.before-start-failure"
  cp "$RC_CONF_DIR/superd" "$TMPDIR/rc-conf.before-start-failure"
  if PATH="$SERVICE_BIN:$PATH" SUPER_INSTALL_SMOKE=1 SUPER_INSTALL_SMOKE_RC_DIR="$RC_DIR" \
    SUPER_INSTALL_SMOKE_RC_CONF_DIR="$RC_CONF_DIR" SUPER_INSTALL_SMOKE_SERVICE_CMD="$SERVICE_BIN/service" \
    sh "$ROOT/install.sh" "${RC_SERVICE_ARGS[@]}" > "$TMPDIR/rc-start-failure.log" 2>&1; then
    cat "$TMPDIR/rc-start-failure.log" >&2
    echo "install unexpectedly succeeded with injected rc.d start failure on upgrade" >&2
    exit 1
  fi
  grep -q 'service superd start failed' "$TMPDIR/rc-start-failure.log"
  cmp "$TMPDIR/rc-script.before-start-failure" "$RC_DIR/superd"
  cmp "$TMPDIR/rc-conf.before-start-failure" "$RC_CONF_DIR/superd"
  echo "==> rc.d start failure restored pre-existing service files"
fi

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

# Inject a second failure while restoring the daemon backup. The installer must
# report the exact preserved backup and recovery command instead of deleting it.
FAIL_ROLLBACK_MV_DIR="$STAGE/fail-rollback-mv-bin"
mkdir -p "$FAIL_ROLLBACK_MV_DIR"
cat > "$FAIL_ROLLBACK_MV_DIR/mv" <<'EOF'
#!/bin/sh
if [ "${1-}" = "-f" ]; then
  shift
fi
case "${1-}" in
  "$PREFIX/bin"/.super.new.*)
    if [ "${2-}" = "$PREFIX/bin/super" ] && [ ! -e "$STAGE/promotion-failed-once" ]; then
      : > "$STAGE/promotion-failed-once"
      echo 'injected promotion failure before rollback-error case' >&2
      exit 97
    fi
    ;;
  "$PREFIX/bin"/.superd.old.*)
    if [ "${2-}" = "$PREFIX/bin/superd" ] && [ ! -e "$STAGE/restore-failed-once" ]; then
      : > "$STAGE/restore-failed-once"
      echo 'injected rollback restore failure' >&2
      exit 98
    fi
    ;;
esac
exec /bin/mv "$@"
EOF
chmod +x "$FAIL_ROLLBACK_MV_DIR/mv"
if PATH="$FAIL_ROLLBACK_MV_DIR:$PATH" sh "$ROOT/install.sh" "${INSTALL_ARGS[@]}" > "$TMPDIR/failed-rollback.log" 2>&1; then
  echo "upgrade with injected rollback failure unexpectedly succeeded" >&2
  exit 1
fi
if ! grep -q 'rollback failed: could not restore' "$TMPDIR/failed-rollback.log"; then
  cat "$TMPDIR/failed-rollback.log" >&2
  echo "rollback failure was not reported" >&2
  exit 1
fi
shopt -s nullglob
ROLLBACK_BACKUPS=("$PREFIX/bin/.superd.old."*)
shopt -u nullglob
if [ "${#ROLLBACK_BACKUPS[@]}" -ne 1 ]; then
  printf 'expected one preserved superd backup, found %s\n' "${#ROLLBACK_BACKUPS[@]}" >&2
  cat "$TMPDIR/failed-rollback.log" >&2
  exit 1
fi
cmp "$TMPDIR/superd.good" "${ROLLBACK_BACKUPS[0]}"
grep -F "mv -f \"${ROLLBACK_BACKUPS[0]}\" \"$PREFIX/bin/superd\"" "$TMPDIR/failed-rollback.log" >/dev/null
grep -F "The previous binary is preserved at ${ROLLBACK_BACKUPS[0]}" "$TMPDIR/failed-rollback.log" >/dev/null
echo "==> failed rollback reported and preserved the old superd binary"
# Exercise the emitted recovery action, then verify a clean restored pair.
/bin/mv -f "${ROLLBACK_BACKUPS[0]}" "$PREFIX/bin/superd"
cmp "$TMPDIR/superd.good" "$PREFIX/bin/superd"
cmp "$TMPDIR/super.good" "$PREFIX/bin/super"
shopt -s nullglob
LEFTOVERS=("$PREFIX/bin/.superd.new."* "$PREFIX/bin/.super.new."* "$PREFIX/bin/.superd.old."* "$PREFIX/bin/.super.old."*)
shopt -u nullglob
if [ "${#LEFTOVERS[@]}" -ne 0 ]; then
  printf 'leftover transaction files after manual recovery: %s\n' "${LEFTOVERS[@]}" >&2
  exit 1
fi
echo "==> reported rollback recovery restored binaries and cleared transaction files"

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
