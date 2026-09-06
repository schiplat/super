# Part 1/5 — Create @djangoapp under Super (stop/remove old → apply stack → list).
# Story: create services. Next: check status.
Output out/01-create-services.mp4
Output out/01-create-services.gif

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

Type `echo "=== Part 1/5 — Create djangoapp services under Super ==="`
Enter
Sleep 1s

Type `super --server "$SUPER_SERVER" stop @djangoapp -y`
Enter
Sleep 3s

Type `super --server "$SUPER_SERVER" remove @djangoapp -y`
Enter
Sleep 2s

Type `super --server "$SUPER_SERVER" list`
Enter
Sleep 2s

Type `super --server "$SUPER_SERVER" apply --file "$SUPER_ROOT/conf/conf.d/djangoapp.toml"`
Enter
Sleep 4s

Type `super --server "$SUPER_SERVER" list`
Enter
Sleep 3s

Type `echo "Part 1 done — services created. Next: Part 2 check status."`
Enter
Sleep 2s
