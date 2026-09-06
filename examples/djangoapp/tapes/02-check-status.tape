# Part 2/5 — Check service status (list / info / edge health).
# Continues after Part 1. Next: scale workers.
Output out/02-check-status.mp4
Output out/02-check-status.gif

Set Shell "bash"
Set FontSize 15
Set Width 1200
Set Height 740
Set TypingSpeed 28ms
Set Theme "Catppuccin Mocha"

Hide
Type `export SUPER_ROOT=/opt/super-django-demo SUPER_SERVER=http://127.0.0.1:9012 PATH=/usr/local/bin:$PATH`
Enter
Type `clear`
Enter
Show

Type `echo "=== Part 2/5 — Check djangoapp status ==="`
Enter
Sleep 1s

Type `super --server "$SUPER_SERVER" list`
Enter
Sleep 3s

Type `super --server "$SUPER_SERVER" info web`
Enter
Sleep 2s

Type `super --server "$SUPER_SERVER" info nginx`
Enter
Sleep 2s

Type `super --server "$SUPER_SERVER" info worker-0`
Enter
Sleep 2s

Type `curl -sS http://127.0.0.1:8088/health/ && echo && curl -sS -o /dev/null -w "edge HTTP %{http_code}\n" http://127.0.0.1:8088/`
Enter
Sleep 2s

Type `echo "Part 2 done — all Healthy. Next: Part 3 scale workers."`
Enter
Sleep 2s
