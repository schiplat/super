#!/usr/bin/env bash
# Refresh verifying keys from Manager into common/keys/.
#
# - make fetch-keys: optional maintainer sync (then commit for OSS/CI).
# - Release CI: runs this before packaging official binaries.
# - make build / PR CI: do NOT call this — use committed *.public.key only.
#
# Env (configure these; Release CI → hzbd/super Actions secrets):
#   MANAGER_BASE, MANAGER_PATH_PREFIX, MANAGER_TOKEN, PRODUCT_ID
# Also: REQUIRE_MANAGER_KEYRING, KEEP_LEGACY_PUBLIC_KEY
#
# Also loads KEY=VALUE from repo-root `.env` when present (gitignored).
#
# After a successful fetch, commit updated common/keys/*.public.key so the next
# OSS/CI/Release build embeds the new ring.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OSS_KEYS="${OSS_KEYS:-$ROOT/common/keys}"
PRODUCT_ID="${PRODUCT_ID:-super-pro}"
REQUIRE="${REQUIRE_MANAGER_KEYRING:-1}"
KEEP_LEGACY="${KEEP_LEGACY_PUBLIC_KEY:-0}"

load_dotenv() {
  local f="$ROOT/.env"
  [[ -f "$f" ]] || return 0
  while IFS= read -r line || [[ -n "$line" ]]; do
    line="${line%$'\r'}"
    [[ -z "$line" || "$line" =~ ^[[:space:]]*# ]] && continue
    if [[ "$line" =~ ^([A-Za-z_][A-Za-z0-9_]*)=(.*)$ ]]; then
      local key="${BASH_REMATCH[1]}"
      local val="${BASH_REMATCH[2]}"
      if [[ "$val" =~ ^\"(.*)\"$ ]]; then
        val="${BASH_REMATCH[1]}"
      elif [[ "$val" =~ ^\'(.*)\'$ ]]; then
        val="${BASH_REMATCH[1]}"
      fi
      if [[ -z "${!key+x}" ]]; then
        export "$key=$val"
      fi
    fi
  done <"$f"
}

load_dotenv
REQUIRE="${REQUIRE_MANAGER_KEYRING:-$REQUIRE}"
KEEP_LEGACY="${KEEP_LEGACY_PUBLIC_KEY:-$KEEP_LEGACY}"

require_on() {
  case "$(printf '%s' "$REQUIRE" | tr '[:upper:]' '[:lower:]')" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

keep_legacy_on() {
  case "$(printf '%s' "$KEEP_LEGACY" | tr '[:upper:]' '[:lower:]')" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

fail_or_skip() {
  local msg="$1"
  if require_on; then
    echo "ERROR: $msg (REQUIRE_MANAGER_KEYRING is set)" >&2
    echo "Hint: configure MANAGER_BASE, MANAGER_PATH_PREFIX, MANAGER_TOKEN, PRODUCT_ID" >&2
    echo "      (env, super/.env, or hzbd/super Actions secrets)." >&2
    echo "      OSS contributors: skip this script; make build uses committed keys." >&2
    exit 1
  fi
  echo "NOTICE: $msg — leaving common/keys/ unchanged"
  exit 0
}

token="${MANAGER_TOKEN:-}"
if [[ -z "$token" ]]; then
  fail_or_skip "MANAGER_TOKEN is not set"
fi

if [[ -z "${MANAGER_BASE:-}" ]]; then
  if require_on; then
    fail_or_skip "MANAGER_BASE is not set"
  fi
  base="http://127.0.0.1:8787"
else
  base="$MANAGER_BASE"
fi
base="${base%/}"
# Normalize like manager-server: single segment → "/{segment}" (default /pi).
prefix_raw="${MANAGER_PATH_PREFIX:-pi}"
prefix_raw="${prefix_raw#/}"
prefix_raw="${prefix_raw%/}"
if [[ -z "$prefix_raw" || "$prefix_raw" == *"/"* ]]; then
  fail_or_skip "MANAGER_PATH_PREFIX must be a single path segment (got '${MANAGER_PATH_PREFIX:-}')"
fi
prefix="/${prefix_raw}"
api_root="${base}${prefix}/api/v1"
mkdir -p "$OSS_KEYS"

echo "==> GET ${api_root}/products/${PRODUCT_ID}/public-keyring"
json=$(curl -fsS \
  -H "Authorization: Bearer $token" \
  -H "Accept: application/json" \
  "${api_root}/products/${PRODUCT_ID}/public-keyring") || {
  fail_or_skip "Manager keyring request failed"
}

KEEP_LEGACY_PUBLIC_KEY="$KEEP_LEGACY" PRODUCT_ID="$PRODUCT_ID" OSS_KEYS="$OSS_KEYS" python3 - "$json" <<'PY'
import base64, json, os, sys
from pathlib import Path

def sanitize(raw: str) -> str:
    s = "".join(c if (c.isalnum() or c in "-_.") else "_" for c in raw.strip())
    return s or "_"

def truthy(raw: str) -> bool:
    return raw.strip().lower() in ("1", "true", "yes", "on")

data = json.loads(sys.argv[1])
product_id = os.environ["PRODUCT_ID"]
oss = Path(os.environ["OSS_KEYS"])
keep_legacy = truthy(os.environ.get("KEEP_LEGACY_PUBLIC_KEY", "0"))
entries = data.get("entries") or []
if not entries:
    sys.exit("ERROR: keyring has no entries")

prefix = f"{sanitize(product_id)}."
for path in sorted(oss.glob("*.public.key")):
    if path.name.startswith(prefix):
        path.unlink()
        print(f"  removed stale {path.name}")

legacy = oss / "public.key"
if legacy.is_file() and not keep_legacy:
    legacy.unlink()
    print(f"  removed legacy {legacy.name}")

written = 0
for e in entries:
    kid = (e.get("kid") or "").strip()
    b64 = (e.get("public_key_b64") or "").strip()
    if not kid or not b64:
        continue
    raw = base64.b64decode(b64)
    if len(raw) != 32:
        sys.exit(f"ERROR: kid={kid} decoded to {len(raw)} bytes (expected 32)")
    stem = f"{sanitize(product_id)}.{sanitize(kid)}"
    out = oss / f"{stem}.public.key"
    out.write_bytes(raw)
    active = " active" if e.get("active") else ""
    print(f"  wrote {out.name} (32 bytes) kid={kid}{active}")
    written += 1

if written == 0:
    sys.exit("ERROR: no keyring entries written")

if keep_legacy and legacy.is_file():
    print(f"  kept {legacy.name} (KEEP_LEGACY_PUBLIC_KEY)")
print(f"==> {written} verifying key(s) ready under {oss}")
print("==> Commit common/keys/*.public.key so CI/Release embed this ring.")
PY
