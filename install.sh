#!/usr/bin/env sh
# Install Project Super (superd + super CLI) from GitHub Releases.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/schiplat/super/master/install.sh | sh
#   curl -fsSL ... | sh -s -- --version 1.3.1 --prefix /usr/local
#
# Options:
#   --version X.Y.Z   Install a specific release (default: latest)
#   --prefix DIR      Install base dir; binaries go to DIR/bin (default: auto)
#   --no-sudo         Do not use sudo even if the prefix is not writable
#   -h, --help        Show this help
#
# The script verifies the SHA256 of the downloaded archive against the
# release's SHA256SUMS before installing.

set -eu

REPO="schiplat/super"
VERSION=""
PREFIX=""
USE_SUDO="auto"

log()  { printf '%s\n' "$*"; }
info() { printf '  %s\n' "$*"; }
die()  { printf 'install.sh: %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "required tool not found: $1"; }

usage() { sed -n '2,18p' "$0"; }

# --- Parse args ---------------------------------------------------------------
while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="${2:?--version needs a value}"; shift 2 ;;
    --version=*) VERSION="${1#*=}"; shift ;;
    --prefix) PREFIX="${2:?--prefix needs a value}"; shift 2 ;;
    --prefix=*) PREFIX="${1#*=}"; shift ;;
    --no-sudo) USE_SUDO="no"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown option: $1 (try --help)" ;;
  esac
done

need curl
need tar
need uname
need sha256sum || need shasum

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
  VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep '"tag_name"' | head -1 | sed -E 's/.*"v?([^"]+)".*/\1/')"
  [ -n "$VERSION" ] || die "could not determine latest release; pass --version X.Y.Z"
fi
# Strip a leading v if the user passed one.
VERSION="${VERSION#v}"
info "Version: $VERSION"

ARCHIVE="super-${VERSION}-${PLATFORM}.tar.gz"
BASE_URL="https://github.com/$REPO/releases/download/v${VERSION}"
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

if command -v sha256sum >/dev/null 2>&1; then
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

# --- Choose prefix ------------------------------------------------------------
if [ -z "$PREFIX" ]; then
  if [ -w /usr/local/bin ] || [ "$(id -u)" -eq 0 ]; then
    PREFIX="/usr/local"
  else
    PREFIX="$HOME/.local"
  fi
fi
BIN_DIR="$PREFIX/bin"

# Decide whether we need sudo to write into BIN_DIR.
SUDO=""
if [ "$USE_SUDO" != "no" ] && [ "$(id -u)" -ne 0 ]; then
  if [ ! -w "$BIN_DIR" ] && [ ! -w "$PREFIX" ]; then
    if command -v sudo >/dev/null 2>&1; then
      SUDO="sudo"
    fi
  fi
fi

log "Installing to $BIN_DIR..."
$SUDO mkdir -p "$BIN_DIR"
$SUDO cp "$ROOT_DIR/bin/superd" "$ROOT_DIR/bin/super" "$BIN_DIR/"
$SUDO chmod +x "$BIN_DIR/superd" "$BIN_DIR/super"

# --- Done ---------------------------------------------------------------------
log ""
log "Installed:"
info "$BIN_DIR/superd"
info "$BIN_DIR/super"
log ""

# PATH hint if the bin dir is not on PATH.
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) log "NOTE: $BIN_DIR is not on your PATH. Add it, e.g.:"
     info "export PATH=\"$BIN_DIR:\$PATH\""
     log "" ;;
esac

if command -v superd >/dev/null 2>&1; then
  info "superd $(superd --version 2>/dev/null || echo "$VERSION")"
fi

cat <<EOF

Quick start:
  superd                        # foreground (default; use under systemd/Docker)
  # superd --daemon             # optional Unix self-daemonize (pidfile: run/superd.pid)
  super add --name demo sleep 1000
  super list
  super doctor                  # diagnose config + daemon + license

Docs: https://super.docs.sconts.com/docs/
EOF
