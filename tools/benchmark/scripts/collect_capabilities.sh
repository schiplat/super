#!/usr/bin/env bash
# Collect the product-capability matrix (no scores) into OUT.json.
# Runs on the analysis host after merging the four arm result dirs.
set -euo pipefail
OUT="${1:-capabilities.json}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/capabilities.sh
source "$SCRIPT_DIR/capabilities.sh"
capabilities_json | python3 -c "import json,sys; json.dump(json.load(sys.stdin), open(sys.argv[1],'w'), indent=2); print('wrote', sys.argv[1])" "$OUT"
