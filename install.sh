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

download() {
  # download <url> <dest> — curl when available, else FreeBSD base fetch(1).
  if have curl; then
    curl -fsSL "$1" -o "$2"
  else
    fetch -qo "$2" "$1"
  fi
}

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

need tar
need uname
# curl exists on Linux/macOS and any FreeBSD with packages, but a pristine
# FreeBSD base system only ships fetch(1) — download() falls back to it.
if ! have curl && ! have fetch; then
  die "required tool not found: curl (or fetch on FreeBSD)"
fi
# sha256(1) is the FreeBSD base-system checksum tool; sha256sum/shasum cover
# GNU userland and macOS.
if ! have sha256sum && ! have shasum && ! have sha256; then
  die "required tool not found: sha256sum, shasum, or sha256"
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

# --- Download to a temp dir ---------------------------------------------------
TMP="$(mktemp -d 2>/dev/null || mktemp -d -t super-install)"
BIN_TX_ID="$(basename "$TMP")"
INSTALL_FILE_TX_JOURNAL="$TMP/install-file-journal"
INSTALL_FILE_TX_SEEN=""
: > "$INSTALL_FILE_TX_JOURNAL"
trap 'rm -rf "$TMP"' EXIT

# --- Resolve version ----------------------------------------------------------
if [ -z "$VERSION" ]; then
  log "Resolving latest release..."
  if [ -n "$BASE_URL_OPT" ]; then
    download "${BASE_URL_OPT%/}/latest.json" "$TMP/latest.json" \
      || die "could not download latest.json from ${BASE_URL_OPT%/}"
  else
    download "https://api.github.com/repos/$REPO/releases/latest" "$TMP/latest.json" \
      || die "could not query the latest release; pass --version X.Y.Z"
  fi
  VERSION="$(grep '"tag_name"' "$TMP/latest.json" | head -1 | sed -E 's/.*"v?([^"]+)".*/\1/')"
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

log "Downloading $ARCHIVE..."
download "$ARCHIVE_URL" "$TMP/$ARCHIVE" \
  || die "download failed (does release v$VERSION have a $PLATFORM build?): $ARCHIVE_URL"

log "Downloading SHA256SUMS..."
download "$SUMS_URL" "$TMP/SHA256SUMS" \
  || die "could not download SHA256SUMS for verification"

# --- Verify checksum ----------------------------------------------------------
log "Verifying checksum..."
EXPECTED="$(grep " ${ARCHIVE}\$" "$TMP/SHA256SUMS" | awk '{print $1}')"
[ -n "$EXPECTED" ] || die "no checksum entry for $ARCHIVE in SHA256SUMS"

if have sha256sum; then
  ACTUAL="$(sha256sum "$TMP/$ARCHIVE" | awk '{print $1}')"
elif have shasum; then
  ACTUAL="$(shasum -a 256 "$TMP/$ARCHIVE" | awk '{print $1}')"
else
  # FreeBSD base: sha256 -q prints the bare digest.
  ACTUAL="$(sha256 -q "$TMP/$ARCHIVE" | awk '{print $1}')"
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

BIN_TX_ACTIVE=0
BIN_TX_DIR=""
BIN_TX_HAD_SUPERD=0
BIN_TX_HAD_SUPER=0
BIN_TX_ROLLBACK_FAILED=0
SERVICE_TX_ACTIVE=0
SERVICE_TX_KIND=""
SERVICE_TX_ROLLBACK_FAILED=0
SERVICE_TX_KEEP_TMP=0
SYSTEMD_TX_UNIT=""
SYSTEMD_TX_UNIT_DIR=""
SYSTEMD_TX_UNIT_SUDO=""
SYSTEMD_TX_SCOPE=""
SYSTEMD_TX_HAD_UNIT=0
SYSTEMD_TX_WAS_ENABLED=0
SYSTEMD_TX_WAS_ACTIVE=0
SYSTEMD_TX_ENABLE_ATTEMPTED=0
INSTALL_FILE_TX_ACTIVE=0
INSTALL_FILE_TX_COUNT=0
SYSTEMD_TX_BACKUP=""
SYSTEMD_TX_START_ATTEMPTED=0
LAUNCHD_TX_ACTIVE=0
LAUNCHD_TX_REGISTERED=0
LAUNCHD_TX_REGISTER_ATTEMPTED=0
LAUNCHD_TX_PLIST=""
LAUNCHD_LABEL="com.schiplat.superd"
LAUNCHD_TX_BACKUP=""
LAUNCHD_TX_HAD_PLIST=0
LAUNCHD_TX_WAS_LOADED=0
LAUNCHD_TX_DOMAIN=""
LAUNCHD_TX_SUDO=""
RC_TX_ACTIVE=0
RC_TX_SCRIPT=""
RC_TX_CONF=""
RC_TX_SCRIPT_BACKUP=""
RC_TX_SERVICE_CMD="service"
RC_TX_HAD_SCRIPT=0
RC_TX_HAD_CONF=0
RC_TX_WAS_ACTIVE=0
RC_TX_START_ATTEMPTED=0

rollback_install_files() {
  [ "$INSTALL_FILE_TX_ACTIVE" -eq 1 ] || return 0
  _files_failed=0
  _entry=0
  while IFS='|' read -r _target _backup _existed; do
    _entry=$((_entry + 1))
    [ -n "$_target" ] || continue
    if [ "$_existed" -eq 1 ]; then
      if [ -e "$_backup" ]; then
        if ! run_for "$(dirname "$_target")" cp -p "$_backup" "$_target"; then
          SERVICE_TX_KEEP_TMP=1
          _files_failed=1
          printf 'install.sh: configuration rollback failed: could not restore %s.\n' "$_target" >&2
          printf '  Previous file is preserved at %s; restore it with:\n' "$_backup" >&2
          printf '  cp -p "%s" "%s"\n' "$_backup" "$_target" >&2
        fi
      else
        SERVICE_TX_KEEP_TMP=1
        _files_failed=1
        printf 'install.sh: configuration rollback failed: backup is missing for %s (%s).\n' "$_target" "$_backup" >&2
      fi
    else
      if ! run_for "$(dirname "$_target")" rm -f "$_target"; then
        _files_failed=1
        printf 'install.sh: configuration rollback failed: could not remove newly created file %s. Remove it manually if appropriate.\n' "$_target" >&2
      fi
    fi
  done < "$INSTALL_FILE_TX_JOURNAL"
  INSTALL_FILE_TX_ACTIVE=0
  return "$_files_failed"
}

record_install_file() {
  _file_target="$1"
  case ":$INSTALL_FILE_TX_SEEN:" in
    *":$_file_target:"*) return 0 ;;
  esac
  INSTALL_FILE_TX_SEEN="${INSTALL_FILE_TX_SEEN}${INSTALL_FILE_TX_SEEN:+:}$_file_target"
  _file_backup="$TMP/install-file.$INSTALL_FILE_TX_COUNT.before.$BIN_TX_ID"
  if [ -e "$_file_target" ]; then
    cp -p "$_file_target" "$_file_backup" 2>/dev/null || {
      [ -n "$SUDO" ] && needs_elev "$_file_target" && $SUDO cp -p "$_file_target" "$_file_backup" \
        || die "could not snapshot existing file before update: $_file_target"
    }
    printf '%s|%s|1\n' "$_file_target" "$_file_backup" >> "$INSTALL_FILE_TX_JOURNAL"
  else
    printf '%s||0\n' "$_file_target" >> "$INSTALL_FILE_TX_JOURNAL"
  fi
  INSTALL_FILE_TX_COUNT=$((INSTALL_FILE_TX_COUNT + 1))
  INSTALL_FILE_TX_ACTIVE=1
}

report_restore_failure() {
  _restore_backup="$1"
  _restore_target="$2"
  BIN_TX_ROLLBACK_FAILED=1
  SERVICE_TX_KEEP_TMP=1
  printf 'install.sh: rollback failed: could not restore %s to %s.\n' "$_restore_backup" "$_restore_target" >&2
  printf '  The previous binary is preserved at %s. Restore it with:\n' "$_restore_backup" >&2
  printf '  mv -f "%s" "%s"\n' "$_restore_backup" "$_restore_target" >&2
}

systemd_tx_call() {
  if [ -n "$SYSTEMD_TX_UNIT_SUDO" ]; then
    $SYSTEMD_TX_UNIT_SUDO systemctl $SYSTEMD_TX_SCOPE "$@"
  else
    systemctl $SYSTEMD_TX_SCOPE "$@"
  fi
}

report_service_restore_failure() {
  _service_target="$1"
  _service_backup="$2"
  SERVICE_TX_ROLLBACK_FAILED=1
  SERVICE_TX_KEEP_TMP=1
  printf 'install.sh: service rollback failed: could not restore %s.\n' "$_service_target" >&2
  if [ -n "$_service_backup" ] && [ -e "$_service_backup" ]; then
    printf '  Previous service file is preserved at %s. Restore it with:\n' "$_service_backup" >&2
    printf '  cp -p "%s" "%s" && systemctl %s daemon-reload\n' "$_service_backup" "$_service_target" "$SYSTEMD_TX_SCOPE" >&2
  else
    printf '  Remove the incomplete service file at %s and run systemctl daemon-reload.\n' "$_service_target" >&2
  fi
}

report_launchd_restore_failure() {
  _launch_target="$1"
  _launch_backup="$2"
  SERVICE_TX_ROLLBACK_FAILED=1
  SERVICE_TX_KEEP_TMP=1
  printf 'install.sh: launchd rollback failed: could not restore %s.\n' "$_launch_target" >&2
  if [ -n "$_launch_backup" ] && [ -e "$_launch_backup" ]; then
    printf '  Previous plist is preserved at %s. Restore it with:\n' "$_launch_backup" >&2
    printf '  cp -p "%s" "%s" && launchctl bootstrap %s "%s"\n' \
      "$_launch_backup" "$_launch_target" "$LAUNCHD_TX_DOMAIN" "$_launch_target" >&2
  else
    printf '  Remove the incomplete plist at %s and unload %s manually.\n' "$_launch_target" "$LAUNCHD_TX_DOMAIN" >&2
  fi
}

rollback_launchd_install() {
  [ "$LAUNCHD_TX_ACTIVE" -eq 1 ] || return 0
  SERVICE_TX_ROLLBACK_FAILED=0
  LAUNCHCTL_SUDO="$LAUNCHD_TX_SUDO"

  if [ "$LAUNCHD_TX_REGISTER_ATTEMPTED" -eq 1 ] || [ "$LAUNCHD_TX_WAS_LOADED" -eq 1 ]; then
    if ! run_launchctl bootout "$LAUNCHD_TX_DOMAIN/$LAUNCHD_LABEL" >/dev/null 2>&1; then
      if run_launchctl print "$LAUNCHD_TX_DOMAIN/$LAUNCHD_LABEL" >/dev/null 2>&1; then
        SERVICE_TX_ROLLBACK_FAILED=1
        printf 'install.sh: launchd rollback failed: could not unload %s before restoring its plist.\n' "$LAUNCHD_TX_DOMAIN/$LAUNCHD_LABEL" >&2
      fi
    fi
  fi
  if [ "$LAUNCHD_TX_WAS_ENABLED" -eq 0 ] && [ "$LAUNCHD_TX_ENABLE_ATTEMPTED" -eq 1 ]; then
    run_launchctl disable "$LAUNCHD_TX_DOMAIN/$LAUNCHD_LABEL" >/dev/null 2>&1 || {
      SERVICE_TX_ROLLBACK_FAILED=1
      printf 'install.sh: launchd rollback failed: could not disable %s.\n' "$LAUNCHD_TX_DOMAIN/$LAUNCHD_LABEL" >&2
    }
  fi
  if [ "$LAUNCHD_TX_HAD_PLIST" -eq 1 ]; then
    if ! run_for "$(dirname "$LAUNCHD_TX_PLIST")" cp -p "$LAUNCHD_TX_BACKUP" "$LAUNCHD_TX_PLIST"; then
      report_launchd_restore_failure "$LAUNCHD_TX_PLIST" "$LAUNCHD_TX_BACKUP"
    fi
  elif [ -e "$LAUNCHD_TX_PLIST" ]; then
    if ! run_for "$(dirname "$LAUNCHD_TX_PLIST")" rm -f "$LAUNCHD_TX_PLIST"; then
      report_launchd_restore_failure "$LAUNCHD_TX_PLIST" ""
    fi
  fi
  if [ "$LAUNCHD_TX_WAS_LOADED" -eq 1 ] && [ "$LAUNCHD_TX_HAD_PLIST" -eq 1 ]; then
    if ! run_launchctl bootstrap "$LAUNCHD_TX_DOMAIN" "$LAUNCHD_TX_PLIST" >/dev/null 2>&1; then
      SERVICE_TX_ROLLBACK_FAILED=1
      printf 'install.sh: launchd rollback failed: previous job %s could not be bootstrapped.\n' "$LAUNCHD_TX_DOMAIN/$LAUNCHD_LABEL" >&2
    fi
  fi
  LAUNCHD_TX_REGISTERED=0
  LAUNCHD_TX_REGISTER_ATTEMPTED=0
  return "$SERVICE_TX_ROLLBACK_FAILED"
}

report_rc_restore_failure() {
  _rc_target="$1"
  _rc_backup="$2"
  SERVICE_TX_ROLLBACK_FAILED=1
  SERVICE_TX_KEEP_TMP=1
  printf 'install.sh: rc.d rollback failed: could not restore %s.\n' "$_rc_target" >&2
  if [ -n "$_rc_backup" ] && [ -e "$_rc_backup" ]; then
    printf '  Previous file is preserved at %s. Restore it with:\n' "$_rc_backup" >&2
    printf '  cp -p "%s" "%s"\n' "$_rc_backup" "$_rc_target" >&2
  else
    printf '  Remove the incomplete file at %s manually.\n' "$_rc_target" >&2
  fi
}

rollback_rc_install() {
  [ "$RC_TX_ACTIVE" -eq 1 ] || return 0
  SERVICE_TX_ROLLBACK_FAILED=0
  if [ "$RC_TX_START_ATTEMPTED" -eq 1 ] && [ "$RC_TX_WAS_ACTIVE" -eq 0 ]; then
    if ! run_for "$RC_TX_SERVICE_PATH" "$RC_TX_SERVICE_CMD" superd stop >/dev/null 2>&1; then
      SERVICE_TX_ROLLBACK_FAILED=1
      printf 'install.sh: rc.d rollback failed: could not stop newly started superd.\n' >&2
    fi
  fi
  if [ "$RC_TX_HAD_SCRIPT" -eq 1 ]; then
    if ! run_for "$(dirname "$RC_TX_SCRIPT")" cp -p "$RC_TX_SCRIPT_BACKUP" "$RC_TX_SCRIPT"; then
      report_rc_restore_failure "$RC_TX_SCRIPT" "$RC_TX_SCRIPT_BACKUP"
    fi
  elif [ -e "$RC_TX_SCRIPT" ]; then
    run_for "$(dirname "$RC_TX_SCRIPT")" rm -f "$RC_TX_SCRIPT" || report_rc_restore_failure "$RC_TX_SCRIPT" ""
  fi
  if [ "$RC_TX_HAD_CONF" -eq 1 ]; then
    if ! run_for "$(dirname "$RC_TX_CONF")" cp -p "$RC_TX_CONF_BACKUP" "$RC_TX_CONF"; then
      report_rc_restore_failure "$RC_TX_CONF" "$RC_TX_CONF_BACKUP"
    fi
  elif [ -e "$RC_TX_CONF" ]; then
    run_for "$(dirname "$RC_TX_CONF")" rm -f "$RC_TX_CONF" || report_rc_restore_failure "$RC_TX_CONF" ""
  fi
  if [ "$RC_TX_WAS_ACTIVE" -eq 1 ] && [ "$RC_TX_START_ATTEMPTED" -eq 1 ]; then
    if ! run_for "$RC_TX_SERVICE_PATH" "$RC_TX_SERVICE_CMD" superd start >/dev/null 2>&1; then
      SERVICE_TX_ROLLBACK_FAILED=1
      printf 'install.sh: rc.d rollback failed: previously running superd could not be restarted.\n' >&2
    fi
  fi
  RC_TX_ACTIVE=0
  return "$SERVICE_TX_ROLLBACK_FAILED"
}

rollback_service_install() {
  [ "$SERVICE_TX_ACTIVE" -eq 1 ] || return 0
  case "$SERVICE_TX_KIND" in
    systemd) rollback_systemd_install ;;
    launchd) rollback_launchd_install ;;
    rc.d) rollback_rc_install ;;
    *) return 0 ;;
  esac
}

rollback_systemd_install() {
  [ "$SERVICE_TX_ACTIVE" -eq 1 ] && [ "$SERVICE_TX_KIND" = "systemd" ] || return 0
  SERVICE_TX_ROLLBACK_FAILED=0
  _service_file="$SYSTEMD_TX_UNIT_DIR/$SYSTEMD_TX_UNIT"

  if [ "$SYSTEMD_TX_START_ATTEMPTED" -eq 1 ] && [ "$SYSTEMD_TX_WAS_ACTIVE" -eq 0 ]; then
    if ! systemd_tx_call stop "$SYSTEMD_TX_UNIT" >/dev/null 2>&1; then
      SERVICE_TX_ROLLBACK_FAILED=1
      printf 'install.sh: service rollback failed: could not stop newly started unit %s; stop it manually before retrying.\n' "$SYSTEMD_TX_UNIT" >&2
    fi
  fi
  if [ "$SYSTEMD_TX_ENABLE_ATTEMPTED" -eq 1 ] && [ "$SYSTEMD_TX_WAS_ENABLED" -eq 0 ]; then
    systemd_tx_call disable "$SYSTEMD_TX_UNIT" >/dev/null 2>&1 || {
      SERVICE_TX_ROLLBACK_FAILED=1
      printf 'install.sh: service rollback failed: could not disable %s.\n' "$SYSTEMD_TX_UNIT" >&2
    }
  fi

  if [ "$SYSTEMD_TX_HAD_UNIT" -eq 1 ]; then
    if ! run_for "$SYSTEMD_TX_UNIT_DIR" cp -p "$SYSTEMD_TX_BACKUP" "$_service_file"; then
      _retained_backup="$SYSTEMD_TX_UNIT_DIR/.${SYSTEMD_TX_UNIT}.super-backup.$BIN_TX_ID"
      if run_for "$SYSTEMD_TX_UNIT_DIR" cp -p "$SYSTEMD_TX_BACKUP" "$_retained_backup"; then
        report_service_restore_failure "$_service_file" "$_retained_backup"
      else
        report_service_restore_failure "$_service_file" "$SYSTEMD_TX_BACKUP"
      fi
    fi
  elif [ -e "$_service_file" ]; then
    if ! run_for "$SYSTEMD_TX_UNIT_DIR" rm -f "$_service_file"; then
      report_service_restore_failure "$_service_file" ""
    fi
  fi

  if ! systemd_tx_call daemon-reload; then
    SERVICE_TX_ROLLBACK_FAILED=1
    printf 'install.sh: service rollback failed: systemd daemon-reload failed; run it manually after restoring %s.\n' "$_service_file" >&2
  fi
  if [ "$SYSTEMD_TX_WAS_ACTIVE" -eq 1 ]; then
    systemd_tx_call restart "$SYSTEMD_TX_UNIT" >/dev/null 2>&1 || {
      SERVICE_TX_ROLLBACK_FAILED=1
      printf 'install.sh: service rollback failed: previous active unit %s could not be restarted.\n' "$SYSTEMD_TX_UNIT" >&2
    }
  fi

  SERVICE_TX_ACTIVE=0
  return "$SERVICE_TX_ROLLBACK_FAILED"
}

rollback_binary_install() {
  [ "$BIN_TX_ACTIVE" -eq 1 ] || return 0
  BIN_TX_ROLLBACK_FAILED=0
  info "Restoring previous binaries after incomplete install..."

  # Rename backups directly over promoted binaries. If restoration fails, the
  # old file remains at its backup path and the current destination is untouched.
  if [ -e "$BIN_TX_DIR/.superd.old.$BIN_TX_ID" ]; then
    if ! run_for "$BIN_TX_DIR" mv -f "$BIN_TX_DIR/.superd.old.$BIN_TX_ID" "$BIN_TX_DIR/superd"; then
      report_restore_failure "$BIN_TX_DIR/.superd.old.$BIN_TX_ID" "$BIN_TX_DIR/superd"
    fi
  elif [ "$BIN_TX_HAD_SUPERD" -eq 0 ] && [ ! -e "$BIN_TX_DIR/.superd.new.$BIN_TX_ID" ]; then
    if ! run_for "$BIN_TX_DIR" rm -f "$BIN_TX_DIR/superd"; then
      BIN_TX_ROLLBACK_FAILED=1
      printf 'install.sh: rollback failed: could not remove incomplete binary %s.\n' "$BIN_TX_DIR/superd" >&2
    fi
  fi
  if [ -e "$BIN_TX_DIR/.super.old.$BIN_TX_ID" ]; then
    if ! run_for "$BIN_TX_DIR" mv -f "$BIN_TX_DIR/.super.old.$BIN_TX_ID" "$BIN_TX_DIR/super"; then
      report_restore_failure "$BIN_TX_DIR/.super.old.$BIN_TX_ID" "$BIN_TX_DIR/super"
    fi
  elif [ "$BIN_TX_HAD_SUPER" -eq 0 ] && [ ! -e "$BIN_TX_DIR/.super.new.$BIN_TX_ID" ]; then
    if ! run_for "$BIN_TX_DIR" rm -f "$BIN_TX_DIR/super"; then
      BIN_TX_ROLLBACK_FAILED=1
      printf 'install.sh: rollback failed: could not remove incomplete binary %s.\n' "$BIN_TX_DIR/super" >&2
    fi
  fi
  BIN_TX_ACTIVE=0
  return "$BIN_TX_ROLLBACK_FAILED"
}

install_exit_cleanup() {
  _cleanup_status="$1"
  if [ "$_cleanup_status" -eq 0 ]; then
    BIN_TX_ACTIVE=0
    if [ -n "$BIN_TX_DIR" ]; then
      run_for "$BIN_TX_DIR" rm -f \
        "$BIN_TX_DIR/.superd.old.$BIN_TX_ID" "$BIN_TX_DIR/.super.old.$BIN_TX_ID" 2>/dev/null || true
    fi
  else
    if ! rollback_service_install; then
      printf 'install.sh: installation failed and service rollback was incomplete. Review the recovery commands above before retrying.\n' >&2
    fi
    if ! rollback_binary_install; then
      printf 'install.sh: installation failed and binary rollback was incomplete. Review the recovery commands above before retrying.\n' >&2
    fi
    if ! rollback_install_files; then
      printf 'install.sh: installation failed and configuration/profile rollback was incomplete. Review the recovery commands above before retrying.\n' >&2
    fi
  fi
  if [ "$SERVICE_TX_KEEP_TMP" -eq 1 ]; then
    if [ -n "$BIN_TX_DIR" ]; then
      run_for "$BIN_TX_DIR" rm -f "$BIN_TX_DIR/.superd.new.$BIN_TX_ID" "$BIN_TX_DIR/.super.new.$BIN_TX_ID" || {
        printf 'install.sh: cleanup failed: temporary binaries remain in %s.\n' "$BIN_TX_DIR" >&2
      }
    fi
    printf 'install.sh: recovery backups retained in %s\n' "$TMP" >&2
  else
    if [ -n "$BIN_TX_DIR" ]; then
      run_for "$BIN_TX_DIR" rm -f "$BIN_TX_DIR/.superd.new.$BIN_TX_ID" "$BIN_TX_DIR/.super.new.$BIN_TX_ID" || true
    fi
    rm -rf "$TMP"
  fi
  return "$_cleanup_status"
}
# Preserve the original exit code before cleanup commands change `$?`.
trap 'INSTALL_EXIT_STATUS=$?; install_exit_cleanup "$INSTALL_EXIT_STATUS"' EXIT

write_file() {
  # write_file <dest>  (contents on stdin)
  _wf_dest="$1"
  _wf_dir="$(dirname "$_wf_dest")"
  run_for "$_wf_dir" mkdir -p "$_wf_dir"
  record_install_file "$_wf_dest"
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
BIN_TX_DIR="$BIN_DIR"
if [ -e "$BIN_DIR/superd" ]; then BIN_TX_HAD_SUPERD=1; fi
if [ -e "$BIN_DIR/super" ]; then BIN_TX_HAD_SUPER=1; fi

# Stage binaries beside their destinations, then promote with same-filesystem
# renames. Keep the previous pair until both new names are live.
run_for "$BIN_DIR" cp "$ROOT_DIR/bin/superd" "$BIN_DIR/.superd.new.$BIN_TX_ID"
run_for "$BIN_DIR" cp "$ROOT_DIR/bin/super" "$BIN_DIR/.super.new.$BIN_TX_ID"
run_for "$BIN_DIR" chmod +x "$BIN_DIR/.superd.new.$BIN_TX_ID" "$BIN_DIR/.super.new.$BIN_TX_ID"
BIN_TX_ACTIVE=1
if [ "$BIN_TX_HAD_SUPERD" -eq 1 ]; then
  run_for "$BIN_DIR" mv "$BIN_DIR/superd" "$BIN_DIR/.superd.old.$BIN_TX_ID"
fi
if [ "$BIN_TX_HAD_SUPER" -eq 1 ]; then
  run_for "$BIN_DIR" mv "$BIN_DIR/super" "$BIN_DIR/.super.old.$BIN_TX_ID"
fi
run_for "$BIN_DIR" mv "$BIN_DIR/.superd.new.$BIN_TX_ID" "$BIN_DIR/superd"
run_for "$BIN_DIR" mv "$BIN_DIR/.super.new.$BIN_TX_ID" "$BIN_DIR/super"

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
      record_install_file "$CONF"
      run_for "$CONF" cp "$ROOT_DIR/contrib/super.toml.default" "$CONF"
    else
      record_install_file "$CONF"
      write_file "$CONF" <<'EOF'
# Project Super — generated by install.sh
# Docs: https://super.docs.sconts.com/docs/02-essentials/configuration/

[server]
host = "127.0.0.1"
port = 9002
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
  record_install_file "$EXAMPLE"
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

  record_install_file "$SUPER_ROOT/env.sh"
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
  if needs_elev "$_uh_target" && [ "$(id -u)" -ne 0 ] && [ -z "$SUDO" ]; then
    return 0
  fi
  record_install_file "$_uh_target"
  if [ ! -e "$_uh_target" ]; then
    if needs_elev "$_uh_target"; then
      [ -n "$SUDO" ] || [ "$(id -u)" -eq 0 ] || return 0
      run_for "$_uh_target" touch "$_uh_target" 2>/dev/null || return 0
    else
      touch "$_uh_target" 2>/dev/null || return 0
    fi
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
      record_install_file /etc/profile.d/super.sh
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
      record_install_file "$env_file"
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
      record_install_file /etc/paths.d/super
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
  SERVICE_TX_KIND="systemd"
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

  SYSTEMD_TX_UNIT="$unit_name"
  SYSTEMD_TX_UNIT_DIR="$unit_dir"
  SYSTEMD_TX_UNIT_SUDO="$unit_sudo"
  SYSTEMD_TX_SCOPE="$scope"
  _unit_file="$unit_dir/$unit_name"
  SYSTEMD_TX_HAD_UNIT=0
  SYSTEMD_TX_BACKUP=""
  SYSTEMD_TX_ENABLE_ATTEMPTED=0
  SYSTEMD_TX_START_ATTEMPTED=0
  SYSTEMD_TX_WAS_ENABLED=0
  SYSTEMD_TX_WAS_ACTIVE=0
  if systemd_tx_call is-enabled "$unit_name" >/dev/null 2>&1; then
    SYSTEMD_TX_WAS_ENABLED=1
  fi
  if systemd_tx_call is-active "$unit_name" >/dev/null 2>&1; then
    SYSTEMD_TX_WAS_ACTIVE=1
  fi
  if [ -e "$_unit_file" ]; then
    SYSTEMD_TX_HAD_UNIT=1
    SYSTEMD_TX_BACKUP="$TMP/${unit_name}.before.$BIN_TX_ID"
    if ! run_for "$unit_dir" cp -p "$_unit_file" "$SYSTEMD_TX_BACKUP"; then
      die "could not snapshot existing service unit before update: $_unit_file"
    fi
  fi
  SERVICE_TX_ACTIVE=1

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

  if ! systemd_tx_call daemon-reload; then
    die "systemd daemon-reload failed after writing $_unit_file"
  fi
  SYSTEMD_TX_ENABLE_ATTEMPTED=1
  if ! systemd_tx_call enable "$unit_name"; then
    die "systemd enable failed for $unit_name"
  fi
  if [ "$DO_START" -eq 1 ]; then
    SYSTEMD_TX_START_ATTEMPTED=1
    if ! systemd_tx_call restart "$unit_name" && ! systemd_tx_call start "$unit_name"; then
      die "systemd start failed for $unit_name"
    fi
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
  LAUNCHD_LABEL="$label"
  plist_path="$plist_dir/$label.plist"
  if [ -n "$plist_sudo" ]; then
    if ! $plist_sudo mkdir -p "$plist_dir"; then
      die "could not create launchd directory $plist_dir"
    fi
  elif ! mkdir -p "$plist_dir"; then
    die "could not create launchd directory $plist_dir"
  fi
  LAUNCHD_TX_DOMAIN="$domain"
  LAUNCHD_TX_SUDO="$plist_sudo"
  LAUNCHD_TX_PLIST="$plist_path"
  LAUNCHD_TX_HAD_PLIST=0
  LAUNCHD_TX_BACKUP=""
  LAUNCHD_TX_WAS_LOADED=0
  LAUNCHD_TX_WAS_ENABLED=1
  LAUNCHD_TX_ENABLE_ATTEMPTED=0
  LAUNCHD_TX_REGISTERED=0
  LAUNCHD_TX_REGISTER_ATTEMPTED=0
  if [ -e "$plist_path" ]; then
    LAUNCHD_TX_HAD_PLIST=1
    LAUNCHD_TX_BACKUP="$TMP/$(basename "$plist_path").before.$BIN_TX_ID"
    cp -p "$plist_path" "$LAUNCHD_TX_BACKUP" || die "could not snapshot existing launchd plist: $plist_path"
  fi
  LAUNCHCTL_SUDO="$plist_sudo"
  if run_launchctl print "$domain/$label" >/dev/null 2>&1; then
    LAUNCHD_TX_WAS_LOADED=1
  fi
  _launch_disabled="$(run_launchctl print-disabled "$domain" 2>/dev/null || true)"
  case "$_launch_disabled" in
    *"$label"*"=> disabled"*) LAUNCHD_TX_WAS_ENABLED=0 ;;
  esac
  SERVICE_TX_KIND="launchd"
  SERVICE_TX_ACTIVE=1
  LAUNCHD_TX_ACTIVE=1

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
    run_for "$plist_dir" chmod 644 "$plist_path"
  else
    printf '%s\n' "$plist_body" >"$plist_path"
    chmod 644 "$plist_path"
  fi

  # Prefer modern bootstrap API (with elevation when needed); fall back to load.
  # With --no-start only the plist is written: bootstrapping starts the job
  # immediately (RunAtLoad + KeepAlive), which would contradict --no-start.
  # RunAtLoad picks the new plist up at the next login/boot instead. An old
  # loaded job keeps running untouched, mirroring the systemd enable-only path.
  LAUNCHCTL_SUDO="$plist_sudo"
  if [ "$DO_START" -eq 1 ]; then
    if [ "$LAUNCHD_TX_WAS_LOADED" -eq 1 ]; then
      run_launchctl bootout "$domain/$label" || die "could not unload existing launchd job $domain/$label"
    fi
    LAUNCHD_TX_REGISTER_ATTEMPTED=1
    if ! run_launchctl bootstrap "$domain" "$plist_path"; then
      # Older macOS without bootstrap/kickstart.
      if ! run_launchctl load -w "$plist_path"; then
        die "launchd failed to register $plist_path"
      fi
    fi
    LAUNCHD_TX_REGISTERED=1
    LAUNCHD_TX_ENABLE_ATTEMPTED=1
    run_launchctl enable "$domain/$label" || die "launchd failed to enable $domain/$label"
    if ! run_launchctl kickstart -k "$domain/$label" && ! run_launchctl kickstart "$domain/$label"; then
      die "launchd failed to start $domain/$label"
    fi
  else
    info "launchd: plist installed; superd starts at next login/boot (--no-start)"
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

  if [ "${SUPER_INSTALL_SMOKE:-0}" = 1 ]; then
    rc_dir="${SUPER_INSTALL_SMOKE_RC_DIR:-/usr/local/etc/rc.d}"
    conf_dir="${SUPER_INSTALL_SMOKE_RC_CONF_DIR:-/etc/rc.conf.d}"
    service_cmd="${SUPER_INSTALL_SMOKE_SERVICE_CMD:-service}"
  else
    rc_dir="/usr/local/etc/rc.d"
    conf_dir="/etc/rc.conf.d"
    service_cmd="service"
  fi
  rc_script="$rc_dir/superd"
  conf_d="$conf_dir/superd"
  RC_TX_SCRIPT="$rc_script"
  RC_TX_CONF="$conf_d"
  RC_TX_SCRIPT_BACKUP="$TMP/rc-script.before.$BIN_TX_ID"
  RC_TX_CONF_BACKUP="$TMP/rc-conf.before.$BIN_TX_ID"
  RC_TX_HAD_SCRIPT=0
  RC_TX_HAD_CONF=0
  RC_TX_WAS_ACTIVE=0
  RC_TX_START_ATTEMPTED=0
  RC_TX_SERVICE_CMD="$service_cmd"
  RC_TX_SERVICE_PATH="$(command -v "$service_cmd" 2>/dev/null || printf '%s' "$service_cmd")"
  if [ -e "$rc_script" ]; then
    RC_TX_HAD_SCRIPT=1
    cp -p "$rc_script" "$RC_TX_SCRIPT_BACKUP" || die "could not snapshot existing rc.d script: $rc_script"
  fi
  if [ -e "$conf_d" ]; then
    RC_TX_HAD_CONF=1
    cp -p "$conf_d" "$RC_TX_CONF_BACKUP" || die "could not snapshot existing rc config: $conf_d"
  fi
  if run_for "$rc_dir" "$service_cmd" superd status >/dev/null 2>&1; then
    RC_TX_WAS_ACTIVE=1
  fi
  SERVICE_TX_KIND="rc.d"
  SERVICE_TX_ACTIVE=1
  RC_TX_ACTIVE=1

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
  run_for "$conf_dir" mkdir -p "$conf_dir"
  write_file "$conf_d" <<EOF
# Project Super — managed by install.sh
superd_enable="YES"
superd_root="$SUPER_ROOT"
superd_bin="$SUPERD_BIN"
EOF
  info "wrote $conf_d (superd_enable=YES)"

  if [ "$DO_START" -eq 1 ]; then
    RC_TX_START_ATTEMPTED=1
    if have "$service_cmd"; then
      run_for "$(command -v "$service_cmd")" "$service_cmd" superd restart 2>/dev/null \
        || run_for "$(command -v "$service_cmd")" "$service_cmd" superd start \
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
