#!/usr/bin/env bash
# Prepare one benchmark lab host for the Super peer benchmark (one arm per host).
#
# For FOUR like-for-like hosts: run this on each, with identical variables, so
# OS, tools, and the SUPER binaries end up version-aligned. Diffs are visible in
# lab_env.txt and later in each arm's manifest.json.
#
# Usage:
#   ./scripts/prepare_lab.sh [--arm super-oss|super-pro|supervisord|pm2]
#       [--super-download URL] [--super-release v1.2.5]
#
# Defaults:
#   - ARM=super-oss
#   - SUPER: if --super-download given, fetch that tarball and install
#     `superd` + `super` into /usr/local/bin (same sha on all hosts).
#   - benchmark: build payloads/generator/runner locally from this tree.
#   - PRO extras (only --arm super-pro): SUPER_BENCH_* must already be exported;
#     plugins_dir is read from $SUPER_BENCH_PLUGINS_DIR (not fetched here).
#
# Produces lab_env.txt next to this script (version dump for cross-host diff).
set -euo pipefail
BENCH_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LAB_ENV="$BENCH_ROOT/lab_env.txt"

ARM="${BENCH_ARM:-super-oss}"
SUPER_URL="${SUPER_DOWNLOAD_URL:-}"
SUPER_TAG="${SUPER_RELEASE_TAG:-}"
SKIP_SUPER="${SKIP_SUPER_INSTALL:-0}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --arm) ARM="${2:?}"; shift 2 ;;
    --super-download) SUPER_URL="${2:?}"; shift 2 ;;
    --super-release) SUPER_TAG="${2:?}"; shift 2 ;;
    --skip-super) SKIP_SUPER=1; shift ;;
    -h|--help)
      sed -n '1,25p' "$0"; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 1 ;;
  esac
done

: > "$LAB_ENV"
dump() { echo "$@" | tee -a "$LAB_ENV"; }

detect_distro() {
  if [[ -f /etc/os-release ]]; then
    . /etc/os-release
    echo "$PRETTY_NAME"
  else
    uname -s
  fi
}

install_base() {
  dump "== base =="
  dump "os=$(detect_distro)"
  dump "arch=$(uname -m) kernel=$(uname -r)"
  if command -v apt-get >/dev/null 2>&1; then
    apt-get update -y
    apt-get install -y git curl ca-certificates build-essential \
      python3 python3-pip python3-venv supervisor 2>/dev/null || true
  elif command -v yum >/dev/null 2>&1; then
    yum install -y git curl make gcc python3 python3-pip supervisor 2>/dev/null || true
  fi
}

install_rust() {
  dump "== rust =="
  if ! command -v cargo >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    export PATH="$HOME/.cargo/bin:$PATH"
  fi
  dump "cargo=$(cargo --version 2>/dev/null || echo missing)"
  dump "rustc=$(rustc --version 2>/dev/null || echo missing)"
}

install_node_pm2() {
  dump "== node/pm2 =="
  if ! command -v node >/dev/null 2>&1; then
    if command -v apt-get >/dev/null 2>&1; then
      apt-get install -y nodejs npm 2>/dev/null || true
    fi
  fi
  if command -v npm >/dev/null 2>&1 && ! command -v pm2 >/dev/null 2>&1; then
    npm install -g pm2
  fi
  dump "node=$(node -v 2>/dev/null || echo missing)"
  dump "npm=$(npm -v 2>/dev/null || echo missing)"
  dump "pm2=$(pm2 -v 2>/dev/null || echo missing)"
}

install_python_deps() {
  dump "== python =="
  dump "python=$(python3 --version 2>/dev/null || echo missing)"
  if ! python3 -c "import matplotlib, pandas" 2>/dev/null; then
    python3 -m pip install --break-system-packages -r "$BENCH_ROOT/analysis/requirements.txt" 2>/dev/null \
      || python3 -m pip install -r "$BENCH_ROOT/analysis/requirements.txt"
  fi
  dump "py-deps=matplotlib/pandas OK"
}

install_super() {
  dump "== super =="
  if [[ "$SKIP_SUPER" == "1" || -z "$SUPER_URL" ]]; then
    if command -v superd >/dev/null 2>&1; then
      dump "superd=from-path $(superd --version 2>/dev/null || echo unknown)"
    else
      dump "superd=SKIPPED (set SUPER_DOWNLOAD_URL or pre-install)"
    fi
    return 0
  fi
  tmp="$(mktemp -d)"
  dump "super-url=$SUPER_URL"
  curl -fsSL "$SUPER_URL" -o "$tmp/super.tar.gz"
  dump "super-sha256=$(sha256sum "$tmp/super.tar.gz" | awk '{print $1}')"
  tar -xzf "$tmp/super.tar.gz" -C "$tmp"
  # The tarball may contain bin/{superd,super} at top level or under bin/.
  local src=""
  for cand in "$tmp/bin/superd" "$tmp/superd"; do
    if [[ -f "$cand" ]]; then src="$cand"; break; fi
  done
  if [[ -z "$src" ]]; then
    echo "super tarball did not contain superd (found: $(find "$tmp" -maxdepth 2 -type f | head))" >&2
    return 1
  fi
  install -m 0755 "$tmp/bin/superd" /usr/local/bin/superd 2>/dev/null || install -m 0755 "$src" /usr/local/bin/superd
  install -m 0755 "$(dirname "$src")/super" /usr/local/bin/super 2>/dev/null || true
  rm -rf "$tmp"
  dump "superd=$(superd --version 2>/dev/null || echo unknown)"
  dump "super=$(super --version 2>/dev/null || echo unknown)"
}

build_bench() {
  dump "== benchmark build =="
  (cd "$BENCH_ROOT" && cargo build --release)
  dump "bench-binaries=$(ls "$BENCH_ROOT/target/release"/{payloads,generator,runner} 2>/dev/null | tr '\n' ' ')"
}

main() {
  dump "arm=$ARM"
  install_base
  install_rust
  install_node_pm2
  install_python_deps
  install_super
  build_bench
  dump "== done =="
  echo
  echo "Environment dump: $LAB_ENV"
  echo "Next: export BENCH_ARM=$ARM PHASE=A and run ./benchmark_all.sh"
}

main
