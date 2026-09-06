# Part 5/5 — Common Super maintenance.
# Avoid `restart @djangoapp` (nginx depends_on web → WAITING spam → flapping).
#
# Do NOT `Type 'cmd; echo MARK'` then Wait+Screen /MARK/ — the typed line is already
# on screen, so Wait returns before the command finishes. Wait for prompt (Wait+Line)
# or output that is not a substring of the Type text (e.g. "edge OK").
#
# Timeouts: edge wait_edge ≤15s; other CLI waits ≤30s; Sleep 10s after group stop.
Output out/05-maintenance.mp4
Output out/05-maintenance.gif

Set Shell "bash"
Set FontSize 15
Set Width 1200
Set Height 740
Set TypingSpeed 28ms
Set Theme "Catppuccin Mocha"
Set WaitTimeout 30s

# Scratch/probes: TMPDIR=/tmp only. APP_ROOT/run is nginx runtime (pid), not test data —
# bootstrap should create it; keep a safety mkdir so restart nginx does not Fatal mid-tape.
Hide
Type `export SUPER_ROOT=/opt/super-django-demo SUPER_SERVER=http://127.0.0.1:9012 DEMO_APP_ROOT=/srv/djangoapp TMPDIR=/tmp PATH=/usr/local/bin:$PATH`
Enter
Type `mkdir -p /srv/djangoapp/run`
Enter
Type `clear`
Enter
Show

Type `echo "=== Part 5/5 — Common Super maintenance ==="`
Enter
Sleep 1s

Type `super --server "$SUPER_SERVER" restart @djangoapp --dry-run`
Enter
Wait+Line@15s />$/
Sleep 2s

Type `super --server "$SUPER_SERVER" restart web --wait-healthy --timeout 30`
Enter
Wait+Line@30s />$/
Type `/srv/djangoapp/scripts/wait_edge.sh http://127.0.0.1:8088/health/ 15`
Enter
Wait+Screen@20s /edge OK/
Wait+Line@10s />$/
Sleep 2s

Type `super --server "$SUPER_SERVER" restart nginx --wait-healthy --timeout 30`
Enter
Wait+Line@30s />$/
Type `/srv/djangoapp/scripts/wait_edge.sh http://127.0.0.1:8088/health/ 15`
Enter
Wait+Screen@20s /edge OK/
Wait+Line@10s />$/
Sleep 2s

Type `curl -sS http://127.0.0.1:8088/health/; echo`
Enter
Wait+Line@10s />$/
Sleep 2s

Type `echo "Group stop/start (respects depends_on: web → nginx)…"`
Enter
Sleep 1s

Type `super --server "$SUPER_SERVER" stop @djangoapp -y`
Enter
Wait+Line@30s />$/
Sleep 10s

Type `super --server "$SUPER_SERVER" start @djangoapp -y`
Enter
Wait+Line@30s />$/
Sleep 2s

Type `/srv/djangoapp/scripts/wait_edge.sh http://127.0.0.1:8088/health/ 15`
Enter
Wait+Screen@20s /edge OK/
Wait+Line@10s />$/
Sleep 2s

Type `super --server "$SUPER_SERVER" list`
Enter
Wait+Line@15s />$/
Sleep 3s

Type `curl -sS -o /dev/null -w "edge HTTP %{http_code}\n" http://127.0.0.1:8088/health/`
Enter
Wait+Line@10s />$/
Sleep 1s
Type `curl -sS http://127.0.0.1:8088/health/; echo`
Enter
Wait+Line@10s />$/
Sleep 2s

Type `echo "Part 5 done — story complete. Edge :8088 Healthy."`
Enter
Sleep 2s
