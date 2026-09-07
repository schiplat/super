#!/usr/bin/env bash
# Record a lean Quick Start walkthrough with VHS (docs-sized outputs).
# Docs: https://super.docs.sconts.com/docs/01-getting-started/quick-start/
#
# Requires: vhs, ttyd, ffmpeg, Chromium; python3; release superd/super.
# Prefer VHS_NO_SANDBOX=true. Scratch under TMPDIR=/tmp.
#
# Outputs (gitignored): examples/demo/tapes/out/quick-start.{mp4,gif}
# Prefer readable quality; keep mp4 docs-friendly (gif may be larger).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXAMPLE_SRC="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="${OUT_DIR:-$EXAMPLE_SRC/tapes/out}"

export TMPDIR="${TMPDIR:-/tmp}"
# Pin instance root — do not inherit ambient SUPER_ROOT from the lab shell.
export SUPER_ROOT="${SUPER_ROOT_OVERRIDE:-/tmp/super-quickstart-demo}"
export SUPER_SERVER="${SUPER_SERVER:-http://127.0.0.1:9002}"
export VHS_NO_SANDBOX="${VHS_NO_SANDBOX:-true}"

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing: $1 (install vhs + ttyd + ffmpeg)" >&2
    exit 1
  }
}

need vhs
need ttyd
need ffmpeg

echo "NOTE: TMPDIR=$TMPDIR  SUPER_ROOT=$SUPER_ROOT  SUPER_SERVER=$SUPER_SERVER"
"$SCRIPT_DIR/bootstrap-quickstart.sh"

mkdir -p "$OUT_DIR"
rm -f "$OUT_DIR"/quick-start.gif "$OUT_DIR"/quick-start.mp4 2>/dev/null || true

cd "$EXAMPLE_SRC/tapes"
echo "==> vhs quick-start.tape (mp4 only; gif built below)"
vhs quick-start.tape

mp4="$OUT_DIR/quick-start.mp4"
gif="$OUT_DIR/quick-start.gif"
raw="$OUT_DIR/quick-start.raw.mp4"

[[ -s "$mp4" ]] || {
  echo "vhs did not produce $mp4" >&2
  exit 1
}
mv "$mp4" "$raw"

# Mono README gif (few colors → small file; target ≤300KB).
echo "==> ffmpeg gif"
ffmpeg -y -i "$raw" \
  -vf "fps=5,scale=900:-1:flags=lanczos,split[s0][s1];[s0]palettegen=max_colors=16:stats_mode=diff[p];[s1][p]paletteuse=dither=none" \
  -loop 0 "$OUT_DIR/quick-start.raw.gif" >/dev/null 2>&1
if command -v gifsicle >/dev/null 2>&1; then
  gifsicle -O3 --lossy=40 -o "$gif" "$OUT_DIR/quick-start.raw.gif"
  rm -f "$OUT_DIR/quick-start.raw.gif"
  # Still over budget? one more pass.
  gif_b=$(wc -c <"$gif" | tr -d ' ')
  if [[ "$gif_b" -gt 300000 ]]; then
    gifsicle -O3 --lossy=80 -o "$gif" "$gif"
  fi
else
  mv "$OUT_DIR/quick-start.raw.gif" "$gif"
fi

# Compact mp4 archive.
echo "==> ffmpeg mp4"
ffmpeg -y -i "$raw" -an -c:v libx264 -crf 28 -preset slow -pix_fmt yuv420p \
  -vf "scale=900:-2" -movflags +faststart "$mp4" >/dev/null 2>&1
rm -f "$raw"

mp4_b=$(wc -c <"$mp4" | tr -d ' ')
gif_b=$(wc -c <"$gif" | tr -d ' ')
echo "sizes: mp4=${mp4_b}B gif=${gif_b}B (README gif target ≤300000)"
if [[ "$gif_b" -gt 300000 ]]; then
  echo "warning: gif exceeds 300KB — shorten the tape further" >&2
fi

echo "Outputs under $OUT_DIR"
ls -la "$OUT_DIR"/quick-start.* 2>/dev/null || ls -la "$OUT_DIR"
