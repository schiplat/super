#!/usr/bin/env bash
# Write README into a release archive directory (all platforms).
# Usage: write-release-readme.sh <version> <platform-slug> <dest-dir>
# Example: write-release-readme.sh 1.1.8 linux-amd64 super-1.1.8-linux-amd64
set -euo pipefail

version="${1:?version required}"
platform="${2:?platform slug required (e.g. linux-amd64)}"
dest_dir="${3:?destination directory required}"

platform_label() {
  case "$1" in
    linux-amd64)   echo "Linux x86_64 (amd64)" ;;
    linux-arm64)   echo "Linux ARM64 (aarch64)" ;;
    windows-amd64) echo "Windows x86_64 (amd64)" ;;  # reserved; not in release CI yet
    macos-amd64)   echo "macOS Intel (x86_64)" ;;
    macos-arm64)   echo "macOS Apple Silicon (ARM64)" ;;
    freebsd-amd64) echo "FreeBSD x86_64 (amd64)" ;;
    *)             echo "$1" ;;
  esac
}

label="$(platform_label "$platform")"
archive="super-${version}-${platform}"

if [[ "$platform" == windows-* ]]; then
  quick_start="  Expand-Archive ${archive}.zip
  cd ${archive}
  .\\bin\\superd.exe"
  bin_superd="bin/superd.exe"
  bin_super="bin/super.exe"
else
  quick_start="  tar xzf ${archive}.tar.gz
  cd ${archive}
  ./bin/superd"
  bin_superd="bin/superd"
  bin_super="bin/super"
fi

cat > "${dest_dir}/README" <<EOF
Project Super ${version} — ${label}
=========================================

Project Super is an API-first, lightweight process orchestrator for the edge.
It replaces tools like Supervisor or PM2 with a single Rust binary.

This archive was built for ${label}.

Contents
--------
  ${bin_superd}   Daemon (default port 9002)
  ${bin_super}    Command-line client
  contrib/        Default config + systemd / launchd / rc.d templates
  LICENSE         MIT license (if included)

Recommended install
-------------------
  curl -fsSL https://github.com/schiplat/super/releases/latest/download/install.sh | sh

  Creates SUPER_ROOT (/opt/super or ~/.super), writes conf/super.toml, and
  enables an OS service (systemd on Linux, launchd on macOS, rc.d on FreeBSD).

Manual quick start
------------------
${quick_start}

  export SUPER_ROOT=\$PWD   # or copy contrib/ into /opt/super and set SUPER_ROOT
  # Linux:    see contrib/systemd/superd.service
  # macOS:    see contrib/launchd/com.schiplat.superd.plist
  # FreeBSD:  see contrib/rc.d/superd

Open http://127.0.0.1:9002/ — OSS edition shows an HTML notice (no built-in dashboard).
Use the \`super\` CLI or /api/v1/* for process management. An optional UI plugin
from a subscription package provides the full web dashboard.

Source & documentation
----------------------
  Upstream:  https://github.com/schiplat/super
  Docs:      https://super.docs.sconts.com/docs/
  Release:   https://github.com/schiplat/super/releases/tag/v${version}
  License:   MIT License

Built by the Project Super GitHub Actions release workflow from tag v${version}.
EOF

echo "Wrote ${dest_dir}/README"
