# Part 3/5 — Scale Celery worker count (numprocs 2 → 3 → 2).
# Continues after Part 2. Next: view logs.
# Chain scale && wait_workers_healthy; Wait+Line for prompt (no echo MARK).
# After 3→2: Sleep 10s then list. Scratch: TMPDIR=/tmp.
Output out/03-scale-workers.mp4
Output out/03-scale-workers.gif

Set Shell "bash"
Set FontSize 15
Set Width 1200
Set Height 740
Set TypingSpeed 28ms
Set Theme "Catppuccin Mocha"
Set WaitTimeout 30s

Hide
Type `export SUPER_ROOT=/opt/super-django-demo SUPER_SERVER=http://127.0.0.1:9012 DEMO_APP_ROOT=/srv/djangoapp TMPDIR=/tmp PATH=/usr/local/bin:$PATH`
Enter
Type `clear`
Enter
Show

Type `echo "=== Part 3/5 — Scale worker count (Super numprocs) ==="`
Enter
Sleep 1s

Type `super --server "$SUPER_SERVER" list | grep -E "Name|worker-"`
Enter
Wait+Line@15s />$/
Sleep 2s

Type `/srv/djangoapp/scripts/scale_workers.sh 3 && /srv/djangoapp/scripts/wait_workers_healthy.sh 25`
Enter
Wait+Line@30s />$/
Sleep 2s

Type `super --server "$SUPER_SERVER" list | grep -E "Name|worker-"`
Enter
Wait+Line@15s />$/
Sleep 3s

Type `/srv/djangoapp/scripts/scale_workers.sh 2 && /srv/djangoapp/scripts/wait_workers_healthy.sh 25`
Enter
Wait+Line@30s />$/
# After 3→2: settle 10s before the list check.
Sleep 10s

Type `super --server "$SUPER_SERVER" list | grep -E "Name|worker-"`
Enter
Wait+Line@15s />$/
Sleep 3s

Type `echo "Part 3 done — scaled 2→3→2. Next: Part 4 view logs."`
Enter
Sleep 2s
