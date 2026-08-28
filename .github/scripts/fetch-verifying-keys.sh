#!/usr/bin/env bash
# Refresh verifying keys from Manager into common/keys/ (build tree).
#
# Intended use:
# - Release CI: run before packaging so official binaries embed Manager's live ring
#   (upserted on top of whatever is already checked out under common/keys/).
# - make fetch-keys: local/debug only — inspect Manager output; do NOT commit results
#   unless a maintainer deliberately curates a key into the repo by hand.
# - make build / PR CI: never call this — embed only committed (hand-picked) keys.
#
# Env (all required — no script defaults; Release CI → hzbd/super Actions secrets):
#   MANAGER_BASE, MANAGER_PATH_PREFIX, MANAGER_TOKEN, PRODUCT_ID
# Also: REQUIRE_MANAGER_KEYRING
#
# Also loads KEY=VALUE from repo-root `.env` when present (gitignored).
# Missing/empty required vars fail closed when REQUIRE_MANAGER_KEYRING is on.
#
# Upserts Manager entries by kid; never deletes existing *.public.key files.
# Git `common/keys/` stays a hand-curated set; Release CI fetch is ephemeral to the job.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OSS_KEYS="${OSS_KEYS:-$ROOT/common/keys}"
REQUIRE="${REQUIRE_MANAGER_KEYRING:-1}"

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

require_on() {
  case "$(printf '%s' "$REQUIRE" | tr '[:upper:]' '[:lower:]')" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

fail_or_skip() {
  local msg="$1"
  if require_on; then
    echo "ERROR: $msg (REQUIRE_MANAGER_KEYRING is set)" >&2
    echo "Hint: set all of MANAGER_BASE, MANAGER_PATH_PREFIX, MANAGER_TOKEN, PRODUCT_ID" >&2
    echo "      (env, super/.env, or hzbd/super Actions secrets). No script defaults." >&2
    echo "      OSS contributors: skip this script; make build uses committed keys." >&2
    exit 1
  fi
  echo "NOTICE: $msg — leaving common/keys/ unchanged"
  exit 0
}

# Required — no defaults (empty Actions secrets must fail closed).
token="${MANAGER_TOKEN:-}"
base="${MANAGER_BASE:-}"
prefix_raw="${MANAGER_PATH_PREFIX:-}"
PRODUCT_ID="${PRODUCT_ID:-}"

if [[ -z "${token// }" ]]; then
  fail_or_skip "MANAGER_TOKEN is not set"
fi
if [[ -z "${base// }" ]]; then
  fail_or_skip "MANAGER_BASE is not set"
fi
if [[ -z "${prefix_raw// }" ]]; then
  fail_or_skip "MANAGER_PATH_PREFIX is not set"
fi
if [[ -z "${PRODUCT_ID// }" ]]; then
  fail_or_skip "PRODUCT_ID is not set"
fi

base="${base%/}"
# Strip accidental API/path tails so BASE is scheme://host[:port] only.
while true; do
  case "$base" in
    */api/v1) base="${base%/api/v1}" ;;
    */api) base="${base%/api}" ;;
    *) break ;;
  esac
  base="${base%/}"
done

# Normalize like manager-server: single segment → "/{segment}".
prefix_raw="${prefix_raw#/}"
prefix_raw="${prefix_raw%/}"
if [[ -z "$prefix_raw" || "$prefix_raw" == *"/"* ]]; then
  fail_or_skip "MANAGER_PATH_PREFIX must be a single path segment (got '${MANAGER_PATH_PREFIX}')"
fi
prefix="/${prefix_raw}"
# Footgun: MANAGER_BASE=https://host/pi + PREFIX=pi → /pi/pi/api/...
if [[ "$base" == *"$prefix" ]]; then
  echo "NOTICE: MANAGER_BASE already ends with ${prefix} — stripping so path is not doubled" >&2
  base="${base%"$prefix"}"
  base="${base%/}"
fi
api_root="${base}${prefix}/api/v1"
mkdir -p "$OSS_KEYS"

url="${api_root}/products/${PRODUCT_ID}/public-keyring"
echo "==> GET ${url}"
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
http_code="$(
  curl -sS -o "$tmp" -w '%{http_code}' \
    -H "Authorization: Bearer $token" \
    -H "Accept: application/json" \
    "$url"
)" || {
  fail_or_skip "Manager keyring curl failed (network/TLS)"
}
json="$(cat "$tmp")"
preview="$(printf '%s' "$json" | tr '\n' ' ' | head -c 240)"

if [[ "$http_code" != "200" ]]; then
  fail_or_skip "Manager keyring HTTP ${http_code} — body: ${preview:-<empty>}
Hint: product must exist with a 32-byte private_key; token needs products.read.
      MANAGER_BASE=scheme://host (no path prefix); set MANAGER_PATH_PREFIX separately"
fi

if [[ -z "$json" ]]; then
  fail_or_skip "Manager keyring returned empty body (HTTP 200)
Hint: check MANAGER_BASE / MANAGER_PATH_PREFIX (avoid doubling the path prefix)"
fi

if ! printf '%s' "$json" | python3 -c 'import json,sys; json.load(sys.stdin)' 2>/dev/null; then
  fail_or_skip "Manager keyring response is not JSON (HTTP ${http_code}) — body: ${preview:-<empty>}
Hint: wrong URL often returns Admin SPA HTML; do not put the path prefix in MANAGER_BASE"
fi

PRODUCT_ID="$PRODUCT_ID" OSS_KEYS="$OSS_KEYS" python3 - "$json" <<'PY'
import base64, json, os, sys
from pathlib import Path

def sanitize(raw: str) -> str:
    s = "".join(c if (c.isalnum() or c in "-_.") else "_" for c in raw.strip())
    return s or "_"

data = json.loads(sys.argv[1])
product_id = os.environ["PRODUCT_ID"]
oss = Path(os.environ["OSS_KEYS"])
entries = data.get("entries") or []
if not entries:
    sys.exit("ERROR: keyring has no entries")

# Cumulative ring: never delete existing *.public.key.
# Only upsert kids returned by Manager so old verifying keys stay embeddable.
added = updated = unchanged = 0
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
    active = " active" if e.get("active") else ""
    if out.is_file():
        if out.read_bytes() == raw:
            print(f"  unchanged {out.name} kid={kid}{active}")
            unchanged += 1
        else:
            out.write_bytes(raw)
            print(f"  updated {out.name} (32 bytes) kid={kid}{active}")
            updated += 1
    else:
        out.write_bytes(raw)
        print(f"  added {out.name} (32 bytes) kid={kid}{active}")
        added += 1

written = added + updated + unchanged
if written == 0:
    sys.exit("ERROR: no keyring entries written")

print(
    f"==> upserted from Manager: added={added} updated={updated} unchanged={unchanged}"
)
print(f"==> keys directory (build tree): {oss}")
for path in sorted(oss.glob("*.public.key")):
    print(f"  present {path.name} ({path.stat().st_size} bytes)")
print(
    "==> Release CI: these files are for this build only. "
    "Do not commit unless deliberately curating a key into the repo."
)
PY
