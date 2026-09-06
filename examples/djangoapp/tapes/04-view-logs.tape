# Part 4/5 — View app logs (history + live follow).
# Continues after Part 3. Next: maintenance.
Output out/04-view-logs.mp4
Output out/04-view-logs.gif

Set Shell "bash"
Set FontSize 14
Set Width 1200
Set Height 760
Set TypingSpeed 26ms
Set Theme "Catppuccin Mocha"

Hide
Type `export SUPER_ROOT=/opt/super-django-demo SUPER_SERVER=http://127.0.0.1:9012 PATH=/usr/local/bin:$PATH`
Enter
Type `clear`
Enter
Show

Type `echo "=== Part 4/5 — View djangoapp logs ==="`
Enter
Sleep 1s

Type `super --server "$SUPER_SERVER" logs web --tail 12`
Enter
Sleep 2s

Type `super --server "$SUPER_SERVER" logs nginx --tail 8`
Enter
Sleep 2s

Type `super --server "$SUPER_SERVER" logs worker-0 --tail 15`
Enter
Sleep 2s

Type `super --server "$SUPER_SERVER" logs beat --tail 8`
Enter
Sleep 2s

Type `echo "(live follow web for 5s — health checks keep writing)"`
Enter
Type `timeout --signal=INT 5 super --server "$SUPER_SERVER" logs web --tail 3 --follow || true`
Enter
Sleep 6s

Type `echo "Part 4 done. Next: Part 5 common maintenance."`
Enter
Sleep 2s
