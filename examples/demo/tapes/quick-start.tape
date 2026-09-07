# Quick Start demo — deploy → create program → day-to-day ops.
# README sized: mono theme (white cmds / gray output) keeps gif ≤300KB.
# Width sized so `super list` (~110 cols of box-drawing) fits without wrap:
#   cells ≈ Width / (FontSize * 0.6) — keep ≥130 with COLUMNS matched.
# Prereq: ./scripts/bootstrap-quickstart.sh
Output out/quick-start.mp4

Set Shell "fish"
Set FontSize 15
Set Width 1280
Set Height 700
Set TypingSpeed 12ms
# Near-monochrome: gray body text; brightWhite for command lines (see mark_cmd).
Set Theme {"name": "ReadmeMono", "background": "#1a1b26", "foreground": "#9aa0a6", "cursor": "#e8eaed", "selection": "#3b4261", "black": "#1a1b26", "red": "#9aa0a6", "green": "#9aa0a6", "yellow": "#9aa0a6", "blue": "#9aa0a6", "magenta": "#9aa0a6", "cyan": "#9aa0a6", "white": "#c0c4cc", "brightBlack": "#6b7280", "brightRed": "#e8eaed", "brightGreen": "#e8eaed", "brightYellow": "#e8eaed", "brightBlue": "#e8eaed", "brightMagenta": "#e8eaed", "brightCyan": "#e8eaed", "brightWhite": "#ffffff"}
Set WaitTimeout 25s

Hide
Type `set -gx SUPER_ROOT /tmp/super-quickstart-demo; set -gx TMPDIR /tmp; set -gx COLUMNS 140`
Enter
Type `source $SUPER_ROOT/record.fish; cd $SUPER_ROOT; set -gx COLUMNS 140`
Enter
# Disable CLI/ls colors so output stays gray (smaller gif palette).
Type `set -gx NO_COLOR 1; set -gx CLICOLOR 0; set -gx LS_COLORS ''`
Enter
Type `function fish_prompt; set_color white; echo -n '> '; set_color normal; end`
Enter
Type `function mark_cmd --on-event fish_preexec; printf '\033[1A\033[2K\033[1;37m> %s\033[0m\n' $argv; end`
Enter
Type `clear`
Enter
Show

# --- 1) Deploy ---
Type `echo "=== 1) Deploy Super ==="`
Enter
Sleep 400ms
Type `ls bin conf`
Enter
Wait+Line@8s />$/
Sleep 500ms
Type `sed -n '1,10p' conf/super.toml`
Enter
Wait+Line@8s />$/
Sleep 600ms
Type `superd --daemon`
Enter
Wait+Line@12s />$/
Sleep 600ms
Type `curl -sS http://127.0.0.1:9002/health; echo`
Enter
Wait+Line@10s />$/
Sleep 700ms

# --- 2) Create program ---
Type `echo "=== 2) Create program ==="`
Enter
Sleep 400ms
Type `super add --name demo-web --autostart python3 -m http.server $DEMO_WEB_PORT`
Enter
Wait+Line@15s />$/
Sleep 600ms
Type `super list`
Enter
Wait+Line@10s />$/
Sleep 600ms
Type `super info demo-web`
Enter
Wait+Line@12s />$/
Sleep 700ms
Type `curl -sS -o /dev/null -w "HTTP %{http_code}\n" http://127.0.0.1:$DEMO_WEB_PORT/`
Enter
Wait+Line@8s />$/
Sleep 700ms

# --- 3) Day-to-day ops ---
Type `echo "=== 3) Day-to-day ops ==="`
Enter
Sleep 400ms
Type `super logs demo-web --tail 3`
Enter
Wait+Line@12s />$/
Sleep 600ms
Type `super restart demo-web --wait-healthy --timeout 20`
Enter
Wait+Line@25s />$/
Sleep 600ms
Type `super stop demo-web -y`
Enter
Wait+Line@15s />$/
Sleep 500ms
Type `super start demo-web -y`
Enter
Wait+Line@15s />$/
Sleep 600ms
Type `super list`
Enter
Wait+Line@10s />$/
Sleep 900ms
