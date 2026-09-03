#!/usr/bin/env sh
# Install Project Super (superd + super CLI) from GitHub Releases.
#
# Usage:
#   curl -fsSL https://github.com/schiplat/super/releases/latest/download/install.sh | sh
#   curl -fsSL ... | sh -s -- --version 1.5.1 --prefix /usr/local
#
# Options:
#   --version X.Y.Z   Install a specific release (default: latest)
#   --prefix DIR      Install base dir; binaries go to DIR/bin (default: auto)
#   --root DIR        Instance root SUPER_ROOT (default: /opt/super or ~/.super)
#   --user            Force a per-user install (user systemd / LaunchAgent)
#   --system          Force a system-wide install (needs root/sudo)
#   --no-service      Skip systemd / launchd setup
#   --no-start        Install service but do not start it yet
#   --no-init         Do not create SUPER_ROOT layout / default config
#   --no-sudo         Do not use sudo even if the prefix is not writable
#   --base-url URL    Download base (default: GitHub Releases). For local smoke:
#                     URL/vX.Y.Z/ARCHIVE and URL/latest.json
#   -h, --help        Show this help
#
# After install (default):
#   - Writes a minimal SUPER_ROOT (conf/, data/, logs/, run/, plugins/)
#   - Wires login env (profile.d / zprofile / paths.d) so SUPER_ROOT is set
#   - Linux: systemd unit, enabled on boot, started now
#   - macOS: launchd plist (LaunchDaemon or LaunchAgent), RunAtLoad + KeepAlive
#   - FreeBSD: rc.d via /usr/local/etc/rc.d/superd (boot-enabled); --user uses --daemon

set -eu

REPO="schiplat/super"
VERSION=""
PREFIX=""
SUPER_ROOT_OPT=""
USE_SUDO="auto"
INSTALL_MODE=""   # system | user | "" (auto)
DO_SERVICE=1
DO_START=1
DO_INIT=1
BASE_URL_OPT=""

log()  { printf '%s\n' "$*"; }
info() { printf '  %s\n' "$*"; }
die()  { printf 'install.sh: %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }
need() { have "$1" || die "required tool not found: $1"; }

usage() {
  cat <<'EOF'
Install Project Super (superd + super CLI) from GitHub Releases.

Usage:
  curl -fsSL https://github.com/schiplat/super/releases/latest/download/install.sh | sh
  curl -fsSL ... | sh -s -- --version 1.5.1 --prefix /usr/local

Options:
  --version X.Y.Z   Install a specific release (default: latest)
  --prefix DIR      Install base dir; binaries go to DIR/bin (default: auto)
  --root DIR        Instance root SUPER_ROOT (default: /opt/super or ~/.super)
  --user            Force a per-user install (user systemd / LaunchAgent)
  --system          Force a system-wide install (needs root/sudo)
  --no-service      Skip systemd / launchd setup
  --no-start        Install service but do not start it yet
  --no-init         Do not create SUPER_ROOT layout / default config
  --no-sudo         Do not use sudo even if the prefix is not writable
  --base-url URL    Download base for local/CI smoke (see script header)
  -h, --help        Show this help
EOF
}

# --- Parse args ---------------------------------------------------------------
while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="${2:?--version needs a value}"; shift 2 ;;
    --version=*) VERSION="${1#*=}"; shift ;;
    --prefix) PREFIX="${2:?--prefix needs a value}"; shift 2 ;;
    --prefix=*) PREFIX="${1#*=}"; shift ;;
    --root|--super-root) SUPER_ROOT_OPT="${2:?--root needs a value}"; shift 2 ;;
    --root=*|--super-root=*) SUPER_ROOT_OPT="${1#*=}"; shift ;;
    --user) INSTALL_MODE="user"; shift ;;
    --system) INSTALL_MODE="system"; shift ;;
    --no-service) DO_SERVICE=0; shift ;;
    --no-start) DO_START=0; shift ;;
    --no-init) DO_INIT=0; shift ;;
    --no-sudo) USE_SUDO="no"; shift ;;
    --base-url) BASE_URL_OPT="${2:?--base-url needs a value}"; shift 2 ;;
    --base-url=*) BASE_URL_OPT="${1#*=}"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown option: $1 (try --help)" ;;
  esac
done

need curl
need tar
need uname
if ! have sha256sum && ! have shasum; then
  die "required tool not found: sha256sum or shasum"
fi

# --- Detect platform ----------------------------------------------------------
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)  OS_PART="linux" ;;
  Darwin) OS_PART="macos" ;;
  FreeBSD) OS_PART="freebsd" ;;
  *) die "unsupported OS: $OS (build from source: https://github.com/$REPO)" ;;
esac

case "$ARCH" in
  x86_64|amd64) ARCH_PART="amd64" ;;
  arm64|aarch64) ARCH_PART="arm64" ;;
  *) die "unsupported architecture: $ARCH" ;;
esac

PLATFORM="${OS_PART}-${ARCH_PART}"
info "Detected platform: $PLATFORM"

# --- Resolve version ----------------------------------------------------------
if [ -z "$VERSION" ]; then
  log "Resolving latest release..."
  if [ -n "$BASE_URL_OPT" ]; then
    VERSION="$(curl -fsSL "${BASE_URL_OPT%/}/latest.json" \
      | grep '"tag_name"' | head -1 | sed -E 's/.*"v?([^"]+)".*/\1/')"
  else
    VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
      | grep '"tag_name"' | head -1 | sed -E 's/.*"v?([^"]+)".*/\1/')"
  fi
  [ -n "$VERSION" ] || die "could not determine latest release; pass --version X.Y.Z"
fi
# Strip a leading v if the user passed one.
VERSION="${VERSION#v}"
info "Version: $VERSION"

ARCHIVE="super-${VERSION}-${PLATFORM}.tar.gz"
if [ -n "$BASE_URL_OPT" ]; then
  BASE_URL="${BASE_URL_OPT%/}/v${VERSION}"
else
  BASE_URL="https://github.com/$REPO/releases/download/v${VERSION}"
fi
ARCHIVE_URL="${BASE_URL}/${ARCHIVE}"
SUMS_URL="${BASE_URL}/SHA256SUMS"

# --- Download to a temp dir ---------------------------------------------------
TMP="$(mktemp -d 2>/dev/null || mktemp -d -t super-install)"
trap 'rm -rf "$TMP"' EXIT

log "Downloading $ARCHIVE..."
curl -fsSL "$ARCHIVE_URL" -o "$TMP/$ARCHIVE" \
  || die "download failed (does release v$VERSION have a $PLATFORM build?): $ARCHIVE_URL"

log "Downloading SHA256SUMS..."
curl -fsSL "$SUMS_URL" -o "$TMP/SHA256SUMS" \
  || die "could not download SHA256SUMS for verification"

# --- Verify checksum ----------------------------------------------------------
log "Verifying checksum..."
EXPECTED="$(grep " ${ARCHIVE}\$" "$TMP/SHA256SUMS" | awk '{print $1}')"
[ -n "$EXPECTED" ] || die "no checksum entry for $ARCHIVE in SHA256SUMS"

if have sha256sum; then
  ACTUAL="$(sha256sum "$TMP/$ARCHIVE" | awk '{print $1}')"
else
  ACTUAL="$(shasum -a 256 "$TMP/$ARCHIVE" | awk '{print $1}')"
fi

[ "$EXPECTED" = "$ACTUAL" ] || die "checksum mismatch!
  expected: $EXPECTED
  actual:   $ACTUAL
Aborting — the archive may be corrupted or tampered with."
info "Checksum OK"

# --- Extract ------------------------------------------------------------------
tar -xzf "$TMP/$ARCHIVE" -C "$TMP"
ROOT_DIR="$TMP/super-${VERSION}-${PLATFORM}"
[ -d "$ROOT_DIR/bin" ] || die "unexpected archive layout: bin/ not found"

# --- Choose prefix / install mode / SUPER_ROOT --------------------------------
if [ -z "$PREFIX" ]; then
  if [ -w /usr/local/bin ] || [ "$(id -u)" -eq 0 ]; then
    PREFIX="/usr/local"
  else
    PREFIX="$HOME/.local"
  fi
fi
BIN_DIR="$PREFIX/bin"

if [ -z "$INSTALL_MODE" ]; then
  case "$PREFIX" in
    "$HOME"/*|"$HOME") INSTALL_MODE="user" ;;
    *) INSTALL_MODE="system" ;;
  esac
fi

if [ -n "$SUPER_ROOT_OPT" ]; then
  SUPER_ROOT="$SUPER_ROOT_OPT"
elif [ "$INSTALL_MODE" = "user" ]; then
  SUPER_ROOT="${HOME}/.super"
else
  SUPER_ROOT="/opt/super"
fi

# Privilege helpers: only elevate for paths that are not writable by the user.
SUDO=""
if [ "$USE_SUDO" != "no" ] && [ "$(id -u)" -ne 0 ] && have sudo; then
  SUDO="sudo"
fi

needs_elev() {
  # needs_elev <path> — true if we cannot create/write this path as the current user.
  # Use _ne_* names: POSIX sh has no locals; avoid clobbering callers.
  _ne_path="$1"
  if [ "$(id -u)" -eq 0 ] || [ "$USE_SUDO" = "no" ]; then
    return 1
  fi
  if [ -e "$_ne_path" ]; then
    [ ! -w "$_ne_path" ]
    return $?
  fi
  _ne_parent="$(dirname "$_ne_path")"
  while [ ! -d "$_ne_parent" ]; do
    _ne_parent="$(dirname "$_ne_parent")"
    [ "$_ne_parent" = "/" ] && break
  done
  [ ! -w "$_ne_parent" ]
}

run_for() {
  # run_for <path> <cmd...> — sudo only when <path> needs elevation.
  _rf_path="$1"
  shift
  if needs_elev "$_rf_path"; then
    [ -n "$SUDO" ] || die "cannot write $_rf_path (re-run with sudo, or use --user / --prefix \"\$HOME/.local\")"
    $SUDO "$@"
  else
    "$@"
  fi
}

write_file() {
  # write_file <dest>  (contents on stdin)
  _wf_dest="$1"
  _wf_dir="$(dirname "$_wf_dest")"
  run_for "$_wf_dir" mkdir -p "$_wf_dir"
  if needs_elev "$_wf_dest"; then
    [ -n "$SUDO" ] || die "cannot write $_wf_dest"
    $SUDO tee "$_wf_dest" >/dev/null
  else
    cat >"$_wf_dest"
  fi
}

# launchctl wrapper; set LAUNCHCTL_SUDO=sudo (or empty) before calling.
run_launchctl() {
  if [ -n "${LAUNCHCTL_SUDO:-}" ]; then
    $LAUNCHCTL_SUDO launchctl "$@"
  else
    launchctl "$@"
  fi
}

if [ "$INSTALL_MODE" = "system" ] && [ "$(id -u)" -ne 0 ] && [ "$USE_SUDO" = "no" ]; then
  if [ "$DO_SERVICE" -eq 1 ] || { [ "$DO_INIT" -eq 1 ] && needs_elev "$SUPER_ROOT"; }; then
    die "system install needs root (re-run with sudo, or pass --user / --prefix \"\$HOME/.local\")"
  fi
fi
if [ "$INSTALL_MODE" = "system" ] && [ "$(id -u)" -ne 0 ] && [ -z "$SUDO" ]; then
  if [ "$DO_SERVICE" -eq 1 ] || { [ "$DO_INIT" -eq 1 ] && needs_elev "$SUPER_ROOT"; }; then
    die "system install needs root (install sudo, or pass --user / --prefix \"\$HOME/.local\")"
  fi
fi

# --- Install binaries ---------------------------------------------------------
log "Installing binaries to $BIN_DIR..."
run_for "$BIN_DIR" mkdir -p "$BIN_DIR"
run_for "$BIN_DIR" cp "$ROOT_DIR/bin/superd" "$ROOT_DIR/bin/super" "$BIN_DIR/"
run_for "$BIN_DIR" chmod +x "$BIN_DIR/superd" "$BIN_DIR/super"

# Prefer absolute paths in service files.
SUPERD_BIN="$BIN_DIR/superd"
SUPER_BIN="$BIN_DIR/super"
if [ -x "$SUPERD_BIN" ]; then
  :
elif have realpath; then
  SUPERD_BIN="$(realpath "$BIN_DIR/superd")"
  SUPER_BIN="$(realpath "$BIN_DIR/super")"
fi

# --- Init instance root -------------------------------------------------------
init_super_root() {
  log "Initializing instance root at $SUPER_ROOT..."
  run_for "$SUPER_ROOT" mkdir -p \
    "$SUPER_ROOT/conf/conf.d" \
    "$SUPER_ROOT/data" \
    "$SUPER_ROOT/logs" \
    "$SUPER_ROOT/run" \
    "$SUPER_ROOT/plugins"
  # Socket parent must not be group/world-writable (superd refuses).
  run_for "$SUPER_ROOT/run" chmod 755 "$SUPER_ROOT/run" 2>/dev/null || true
  run_for "$SUPER_ROOT/logs" chmod 755 "$SUPER_ROOT/logs" 2>/dev/null || true
  run_for "$SUPER_ROOT/data" chmod 755 "$SUPER_ROOT/data" 2>/dev/null || true

  CONF="$SUPER_ROOT/conf/super.toml"
  if [ -f "$CONF" ]; then
    info "keeping existing config: $CONF"
  else
    # Prefer packaged default when present in the release archive.
    if [ -f "$ROOT_DIR/contrib/super.toml.default" ]; then
      run_for "$CONF" cp "$ROOT_DIR/contrib/super.toml.default" "$CONF"
    else
      write_file "$CONF" <<'EOF'
# Project Super — generated by install.sh
# Docs: https://super.docs.sconts.com/docs/02-essentials/configuration/

[server]
host = "127.0.0.1"
port = 9002
allow_insecure_public_bind = false
shutdown_timeout = 10
enable_docs = false
# Local CLI prefers this socket when SUPER_ROOT is set (see env.sh).
socket = "run/superd.sock"
# Default 0600 (owner only). System installs run as root → non-root CLI needs either:
#   sudo -E super …   or   super --server http://127.0.0.1:9002 …
# For a shared group: set socket_mode = "0660", chgrp the run/ dir, restart superd.
# socket_mode = "0600"   # "0600" | "0640" | "0660" — never world-writable
# Keep false under systemd / launchd / Docker. Use `superd --daemon` only without an OS service.
# daemon = false

[logging]
log_level = "info"
log_max_mb = 50
log_backups = 3

[child_logging]
max_size_mb = 10
max_backups = 5
max_line_size_kb = 64

[storage]
data_file = "data/snapshot.json"
events_file = "data/events.db"
events_keep_days = 30
log_dir = "logs"

[include]
files = ["conf/conf.d/*.toml"]

# =============================================================================
# Subscription / Pro (optional) — COMMENTED ON PURPOSE (OSS default)
# =============================================================================
# Uncomment only after you have a subscription license key and plugin libraries
# from your vendor. Docs:
#   https://super.docs.sconts.com/docs/07-editions/
#   https://super.docs.sconts.com/docs/05-advanced-management/authentication/
#
# Enable checklist:
#   1. Drop plugin libs into $SUPER_ROOT/plugins/ (no "lib" prefix), e.g.:
#        plugins/security.so|.dylib   — API auth / RBAC / audit (required)
#        plugins/ui.so|.dylib         — web dashboard
#        plugins/notify.so|.dylib     — IM / webhook notifications
#        plugins/isolation.so|.dylib  — Linux cgroup CPU/memory limits
#   2. Uncomment auth_secret and [license] below; fill in real values
#   3. Restart superd — licensed mode hard-requires security + auth_secret
#
# Related files (also subscription; create when needed):
#   conf/notify.toml     — notify plugin channels / templates
#   conf/conf.d/*.toml   — per-program resource_limits (isolation plugin)
# =============================================================================

# Root Admin bootstrap secret for the security plugin. Use a long random string.
# auth_secret = "CHANGE-ME-strong-random-secret"

# [license]
# # Prefer true in production so an invalid key refuses startup (no silent OSS fall-back).
# strict = true
# key = "PASTE-BASE64-LICENSE-KEY"
EOF
    fi
    info "wrote $CONF"
  fi

  EXAMPLE="$SUPER_ROOT/conf/conf.d/demo.toml.example"
  if [ ! -f "$EXAMPLE" ]; then
    if [ -f "$ROOT_DIR/contrib/conf.d/demo.toml.example" ]; then
      run_for "$EXAMPLE" cp "$ROOT_DIR/contrib/conf.d/demo.toml.example" "$EXAMPLE"
    else
      write_file "$EXAMPLE" <<'EOF'
# Copy to demo.toml (drop .example) to seed a sample program on start/reload.
prune = false

[[services]]
name = "demo"
command = "/bin/sleep"
args = ["3600"]
autostart = true
autorestart = "unexpected"
exitcodes = [0]
startsecs = 1
retry_limit = 3
EOF
    fi
  fi

  write_file "$SUPER_ROOT/env.sh" <<EOF
# Project Super instance environment (sourced by login shells via install.sh hooks).
# Manual: source $SUPER_ROOT/env.sh
export SUPER_ROOT="$SUPER_ROOT"
case ":\$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) export PATH="$BIN_DIR:\$PATH" ;;
esac
EOF
  info "wrote $SUPER_ROOT/env.sh"

  install_login_env
}

# Replace or append the Project Super marker block in a shell profile file.
upsert_shell_hook() {
  _uh_target="$1"
  [ -n "$_uh_target" ] || return 0
  _uh_dir="$(dirname "$_uh_target")"
  if [ ! -d "$_uh_dir" ]; then
    run_for "$_uh_dir" mkdir -p "$_uh_dir" 2>/dev/null || return 0
  fi
  if [ ! -e "$_uh_target" ]; then
    if needs_elev "$_uh_target"; then
      [ -n "$SUDO" ] || [ "$(id -u)" -eq 0 ] || return 0
      run_for "$_uh_target" touch "$_uh_target" 2>/dev/null || return 0
    else
      touch "$_uh_target" 2>/dev/null || return 0
    fi
  fi
  if needs_elev "$_uh_target" && [ "$(id -u)" -ne 0 ] && [ -z "$SUDO" ]; then
    return 0
  fi

  _uh_tmp="$TMP/super-hook-$$"
  if needs_elev "$_uh_target" && [ "$(id -u)" -ne 0 ]; then
    $SUDO cat "$_uh_target" 2>/dev/null
  else
    cat "$_uh_target" 2>/dev/null
  fi | awk '
    BEGIN {skip=0}
    /^# >>> Project Super >>>$/ {skip=1; next}
    /^# <<< Project Super <<<$/ {skip=0; next}
    skip==0 {print}
  ' >"$_uh_tmp" || : >"$_uh_tmp"

  # One blank line before the hook when the previous line has content.
  if [ -s "$_uh_tmp" ]; then
    _uh_last="$(tail -n 1 "$_uh_tmp")"
    [ -n "$_uh_last" ] && printf '\n' >>"$_uh_tmp"
  fi
  cat >>"$_uh_tmp" <<EOF
# >>> Project Super >>>
[ -r "$SUPER_ROOT/env.sh" ] && . "$SUPER_ROOT/env.sh"
# <<< Project Super <<<
EOF

  if needs_elev "$_uh_target" && [ "$(id -u)" -ne 0 ]; then
    $SUDO cp "$_uh_tmp" "$_uh_target"
  else
    cp "$_uh_tmp" "$_uh_target"
  fi
  rm -f "$_uh_tmp"
  info "updated $_uh_target"
}

# Idempotently ensure login shells load $SUPER_ROOT/env.sh.
install_login_env() {
  log "Configuring login environment (SUPER_ROOT + PATH)..."

  if [ "$INSTALL_MODE" = "system" ]; then
    # Linux (and OSes with profile.d): drop-in for /etc/profile.
    if [ "$OS" = "Linux" ] || [ -d /etc/profile.d ]; then
      run_for /etc/profile.d mkdir -p /etc/profile.d
      write_file /etc/profile.d/super.sh <<EOF
# Project Super — loaded by login shells (/etc/profile).
# Managed by install.sh; edit $SUPER_ROOT/env.sh to change values.
[ -r "$SUPER_ROOT/env.sh" ] && . "$SUPER_ROOT/env.sh"
EOF
      info "wrote /etc/profile.d/super.sh"
    fi

    # pam_env / display-manager sessions that read /etc/environment (SUPER_ROOT only).
    if [ "$OS" = "Linux" ]; then
      env_file="/etc/environment"
      tmp="$TMP/super-environment-$$"
      if [ -f "$env_file" ]; then
        if needs_elev "$env_file" && [ "$(id -u)" -ne 0 ]; then
          $SUDO grep -v '^SUPER_ROOT=' "$env_file" >"$tmp" 2>/dev/null || : >"$tmp"
        else
          grep -v '^SUPER_ROOT=' "$env_file" >"$tmp" 2>/dev/null || : >"$tmp"
        fi
      else
        : >"$tmp"
      fi
      printf 'SUPER_ROOT="%s"\n' "$SUPER_ROOT" >>"$tmp"
      if needs_elev "$env_file" && [ "$(id -u)" -ne 0 ]; then
        if [ -n "$SUDO" ]; then
          $SUDO cp "$tmp" "$env_file"
          info "set SUPER_ROOT in $env_file"
        fi
      else
        cp "$tmp" "$env_file"
        info "set SUPER_ROOT in $env_file"
      fi
      rm -f "$tmp"
    fi

    # macOS: path_helper + zsh/bash login files (no /etc/profile.d by default).
    if [ "$OS" = "Darwin" ]; then
      run_for /etc/paths.d mkdir -p /etc/paths.d
      write_file /etc/paths.d/super <<EOF
$BIN_DIR
EOF
      info "wrote /etc/paths.d/super"
      upsert_shell_hook /etc/zprofile
      upsert_shell_hook /etc/bashrc
      upsert_shell_hook /etc/profile
    fi

    # FreeBSD: no profile.d; hook /etc/profile.
    if [ "$OS" = "FreeBSD" ]; then
      upsert_shell_hook /etc/profile
    fi
  else
    # Per-user shells — do NOT create ~/.bash_profile just because ~/.bashrc
    # exists: bash login would then skip ~/.profile and drop the user's PATH
    # setup (nvm, cargo, …).
    upsert_shell_hook "${ZDOTDIR:-$HOME}/.zprofile"
    if [ -f "$HOME/.bash_profile" ]; then
      upsert_shell_hook "$HOME/.bash_profile"
    elif [ -f "$HOME/.bash_login" ]; then
      upsert_shell_hook "$HOME/.bash_login"
    else
      upsert_shell_hook "$HOME/.profile"
    fi
    if [ -f "$HOME/.bashrc" ]; then
      upsert_shell_hook "$HOME/.bashrc"
    fi
  fi

  log "Login env configured. Open a new terminal (or re-login) so SUPER_ROOT is set."
  info "this shell: source $SUPER_ROOT/env.sh"
}

if [ "$DO_INIT" -eq 1 ]; then
  init_super_root
fi

# --- Service helpers ----------------------------------------------------------
systemd_available() {
  [ "$OS" = "Linux" ] || return 1
  have systemctl || return 1
  # Containers / chroots without a real systemd often lack this.
  [ -d /run/systemd/system ] || [ -d /sys/fs/cgroup/systemd ] || [ -d /sys/fs/cgroup/system.slice ]
}

install_systemd() {
  unit_name="superd.service"
  if [ "$INSTALL_MODE" = "user" ]; then
    unit_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
    scope="--user"
    wanted_by="default.target"
    unit_sudo=""
  else
    unit_dir="/etc/systemd/system"
    scope=""
    wanted_by="multi-user.target"
    unit_sudo="$SUDO"
  fi

  log "Installing systemd unit ($INSTALL_MODE)..."
  mkdir_cmd="mkdir"
  tee_cmd="tee"
  systemctl_cmd="systemctl"
  if [ -n "$unit_sudo" ]; then
    mkdir_cmd="$unit_sudo mkdir"
    tee_cmd="$unit_sudo tee"
    systemctl_cmd="$unit_sudo systemctl"
  fi

  # shellcheck disable=SC2086
  $mkdir_cmd -p "$unit_dir"
  # shellcheck disable=SC2086
  $tee_cmd "$unit_dir/$unit_name" >/dev/null <<EOF
[Unit]
Description=Project Super Process Manager
Documentation=https://super.docs.sconts.com/docs/
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
# Must stay in the foreground. Do not set [server].daemon = true or pass --daemon.
ExecStart="$SUPERD_BIN" --foreground
Restart=on-failure
RestartSec=2
Environment="SUPER_ROOT=$SUPER_ROOT"
LimitNOFILE=65536

[Install]
WantedBy=$wanted_by
EOF

  # shellcheck disable=SC2086
  $systemctl_cmd $scope daemon-reload
  # shellcheck disable=SC2086
  $systemctl_cmd $scope enable "$unit_name"
  if [ "$DO_START" -eq 1 ]; then
    # shellcheck disable=SC2086
    $systemctl_cmd $scope restart "$unit_name" || \
      $systemctl_cmd $scope start "$unit_name"
    info "systemd: enabled and started ($unit_dir/$unit_name)"
  else
    info "systemd: enabled (not started; pass without --no-start to start)"
  fi

  if [ "$INSTALL_MODE" = "user" ]; then
    log ""
    log "NOTE: user units start at login. For boot without an interactive login:"
    info "loginctl enable-linger $(id -un)"
  fi

  SERVICE_KIND="systemd"
}

install_launchd() {
  label="com.schiplat.superd"
  if [ "$INSTALL_MODE" = "user" ]; then
    plist_dir="$HOME/Library/LaunchAgents"
    domain="gui/$(id -u)"
    plist_sudo=""
  else
    plist_dir="/Library/LaunchDaemons"
    domain="system"
    plist_sudo="$SUDO"
    [ -n "$plist_sudo" ] || [ "$(id -u)" -eq 0 ] || \
      die "macOS system LaunchDaemon needs root (re-run with sudo, or pass --user)"
  fi
  plist_path="$plist_dir/$label.plist"

  log "Installing launchd plist ($INSTALL_MODE)..."
  if [ -n "$plist_sudo" ]; then
    $plist_sudo mkdir -p "$plist_dir"
  else
    mkdir -p "$plist_dir"
  fi

  # Stdout/err capture early boot failures before superd opens its own logs.
  out_log="$SUPER_ROOT/logs/launchd.out.log"
  err_log="$SUPER_ROOT/logs/launchd.err.log"

  plist_body=$(cat <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>$label</string>
  <key>ProgramArguments</key>
  <array>
    <string>$SUPERD_BIN</string>
    <string>--foreground</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>SUPER_ROOT</key>
    <string>$SUPER_ROOT</string>
  </dict>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>$out_log</string>
  <key>StandardErrorPath</key>
  <string>$err_log</string>
</dict>
</plist>
EOF
)

  if [ -n "$plist_sudo" ]; then
    printf '%s\n' "$plist_body" | $plist_sudo tee "$plist_path" >/dev/null
    $plist_sudo chmod 644 "$plist_path"
  else
    printf '%s\n' "$plist_body" >"$plist_path"
    chmod 644 "$plist_path"
  fi

  # Prefer modern bootstrap API (with elevation when needed); fall back to load.
  LAUNCHCTL_SUDO="$plist_sudo"
  run_launchctl bootout "$domain/$label" >/dev/null 2>&1 || true
  if run_launchctl bootstrap "$domain" "$plist_path" >/dev/null 2>&1; then
    run_launchctl enable "$domain/$label" >/dev/null 2>&1 || true
    if [ "$DO_START" -eq 1 ]; then
      run_launchctl kickstart -k "$domain/$label" >/dev/null 2>&1 \
        || run_launchctl kickstart "$domain/$label" >/dev/null 2>&1 \
        || true
    fi
  else
    # Older macOS without bootstrap/kickstart.
    run_launchctl unload "$plist_path" >/dev/null 2>&1 || true
    run_launchctl load -w "$plist_path"
    if [ "$DO_START" -eq 0 ]; then
      run_launchctl unload "$plist_path" >/dev/null 2>&1 || true
      info "launchd: plist installed but not running (--no-start)"
    fi
  fi

  info "launchd: $plist_path (RunAtLoad + KeepAlive)"
  SERVICE_KIND="launchd"
  if [ "$INSTALL_MODE" = "user" ]; then
    SERVICE_DOMAIN="gui/$(id -u)"
  else
    SERVICE_DOMAIN="system"
  fi
}

print_freebsd_hints() {
  log ""
  log "FreeBSD user / no-service mode. Options:"
  info "1) Self-daemonize: SUPER_ROOT=$SUPER_ROOT $SUPERD_BIN --daemon"
  info "2) System service: re-run with sudo (no --user) to install rc.d"
  info "3) Manual rc.d: see contrib/rc.d/superd"
}

install_freebsd_rc() {
  if [ "$INSTALL_MODE" = "user" ]; then
    log "FreeBSD: per-user rc.d is not used; starting with self-daemonize..."
    print_freebsd_hints
    if [ "$DO_START" -eq 1 ]; then
      SUPER_ROOT="$SUPER_ROOT" "$SUPER_BIN" shutdown >/dev/null 2>&1 || true
      SUPER_ROOT="$SUPER_ROOT" "$SUPERD_BIN" --daemon \
        || die "failed to start superd --daemon (check $SUPER_ROOT/logs)"
      info "started: SUPER_ROOT=$SUPER_ROOT $SUPERD_BIN --daemon"
    fi
    SERVICE_KIND="daemon"
    return
  fi

  rc_dir="/usr/local/etc/rc.d"
  rc_script="$rc_dir/superd"
  conf_d="/etc/rc.conf.d/superd"

  log "Installing FreeBSD rc.d service..."
  run_for "$rc_dir" mkdir -p "$rc_dir"

  # Concrete defaults baked in; /etc/rc.conf.d/superd can still override.
  write_file "$rc_script" <<EOF
#!/bin/sh

# PROVIDE: superd
# REQUIRE: LOGIN FILESYSTEMS NETWORKING
# KEYWORD: shutdown

. /etc/rc.subr

name="superd"
desc="Project Super process manager"
rcvar="superd_enable"

load_rc_config \${name}

: "\${superd_enable:=NO}"
: "\${superd_root:=$SUPER_ROOT}"
: "\${superd_bin:=$SUPERD_BIN}"
: "\${superd_user:=root}"
: "\${superd_flags:=--foreground}"

pidfile="\${superd_pidfile:-\${superd_root}/run/superd.rc.pid}"
procname="\${superd_bin}"

start_precmd="superd_prestart"
start_cmd="superd_start"

superd_prestart()
{
	if [ ! -x "\${superd_bin}" ]; then
		err 1 "superd binary not found or not executable: \${superd_bin}"
	fi
	if [ ! -d "\${superd_root}" ]; then
		err 1 "SUPER_ROOT does not exist: \${superd_root}"
	fi
	mkdir -p "\${superd_root}/run" "\${superd_root}/logs" "\${superd_root}/data"
}

# Custom start so SUPER_ROOT / binary paths with spaces are not word-split.
superd_start()
{
	/usr/sbin/daemon -f -P "\${pidfile}" -r -u "\${superd_user}" \\
		/usr/bin/env "SUPER_ROOT=\${superd_root}" \\
		"\${superd_bin}" \${superd_flags}
}

run_rc_command "\$1"
EOF
  run_for "$rc_script" chmod 755 "$rc_script"
  info "wrote $rc_script"

  # Isolated enable flags (preferred over editing /etc/rc.conf).
  run_for /etc/rc.conf.d mkdir -p /etc/rc.conf.d
  write_file "$conf_d" <<EOF
# Project Super — managed by install.sh
superd_enable="YES"
superd_root="$SUPER_ROOT"
superd_bin="$SUPERD_BIN"
EOF
  info "wrote $conf_d (superd_enable=YES)"

  if [ "$DO_START" -eq 1 ]; then
    if have service; then
      run_for /usr/sbin/service service superd restart 2>/dev/null \
        || run_for /usr/sbin/service service superd start \
        || die "service superd start failed"
    else
      run_for "$rc_script" "$rc_script" restart 2>/dev/null \
        || run_for "$rc_script" "$rc_script" start \
        || die "rc.d superd start failed"
    fi
    info "rc.d: enabled and started"
  else
    info "rc.d: enabled (not started; pass without --no-start to start)"
  fi

  SERVICE_KIND="rc.d"
}

# --- Install OS service -------------------------------------------------------
SERVICE_KIND=""
SERVICE_DOMAIN=""

if [ "$DO_SERVICE" -eq 1 ]; then
  case "$OS" in
    Linux)
      if systemd_available; then
        install_systemd
      else
        log "systemd not available — skipping service install."
        info "Start manually: SUPER_ROOT=$SUPER_ROOT $SUPERD_BIN --daemon"
        info "Or re-run with a host that has systemd, or pass --no-service"
      fi
      ;;
    Darwin)
      install_launchd
      ;;
    FreeBSD)
      install_freebsd_rc
      ;;
  esac
fi

# --- Done ---------------------------------------------------------------------
log ""
log "Installed:"
info "$SUPERD_BIN"
info "$SUPER_BIN"
if [ "$DO_INIT" -eq 1 ]; then
  info "SUPER_ROOT=$SUPER_ROOT"
fi
log ""

case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) log "NOTE: $BIN_DIR is not on your PATH. Add it, e.g.:"
     info "export PATH=\"$BIN_DIR:\$PATH\""
     log "" ;;
esac

if [ "$DO_INIT" -eq 1 ]; then
  log "Environment: SUPER_ROOT will load on next login (see hooks above)."
  info "this shell only: source $SUPER_ROOT/env.sh"
  log ""
fi

# Version probe (may fail if PATH not updated yet).
if [ -x "$SUPERD_BIN" ]; then
  info "superd $($SUPERD_BIN --version 2>/dev/null || echo "$VERSION")"
fi

# Quick health check when we started a service.
if [ "$DO_SERVICE" -eq 1 ] && [ "$DO_START" -eq 1 ] && [ -n "$SERVICE_KIND" ]; then
  sleep 1
  if SUPER_ROOT="$SUPER_ROOT" "$SUPER_BIN" doctor >/dev/null 2>&1; then
    info "super doctor: OK"
  else
    info "super doctor: run \`source $SUPER_ROOT/env.sh && super doctor\` to diagnose"
  fi
fi

if [ "$INSTALL_MODE" = "system" ] && [ "$DO_INIT" -eq 1 ]; then
  log ""
  log "NOTE: system install runs superd as root; run/superd.sock is owner-only (0600)."
  info "non-root CLI:  sudo -E super list"
  info "            or  super --server http://127.0.0.1:9002 list"
  info "shared group:   socket_mode = \"0660\" in conf/super.toml + chgrp on run/"
fi

cat <<EOF

Quick start:
  # New login shells already have SUPER_ROOT (re-open the terminal if needed).
  # Current shell: source $SUPER_ROOT/env.sh
  super add --name demo --autostart sleep 3600
  super list
  super doctor

Service:
EOF

case "$SERVICE_KIND" in
  systemd)
    if [ "$INSTALL_MODE" = "user" ]; then
      cat <<EOF
  systemctl --user status superd
  systemctl --user restart superd
  journalctl --user -u superd -f
EOF
    else
      cat <<EOF
  systemctl status superd
  systemctl restart superd
  journalctl -u superd -f
EOF
    fi
    ;;
  launchd)
    cat <<EOF
  launchctl print ${SERVICE_DOMAIN}/com.schiplat.superd
  # logs: $SUPER_ROOT/logs/  (plus launchd.out.log / launchd.err.log)
  # stop:  super shutdown
  #        launchctl bootout ${SERVICE_DOMAIN}/com.schiplat.superd
EOF
    ;;
  rc.d)
    cat <<EOF
  service superd status
  service superd restart
  # logs: $SUPER_ROOT/logs/
  # disable: sysrc -f /etc/rc.conf.d/superd superd_enable=NO
EOF
    ;;
  daemon)
    cat <<EOF
  SUPER_ROOT=$SUPER_ROOT $SUPERD_BIN --daemon
  super shutdown
EOF
    ;;
  *)
    cat <<EOF
  SUPER_ROOT=$SUPER_ROOT $SUPERD_BIN --daemon   # without OS service
  super shutdown
EOF
    ;;
esac

cat <<EOF

Docs: https://super.docs.sconts.com/docs/
EOF
