# Contrib templates for packaging and manual installs
#
# Repo path: `packaging/contrib/`. Release tarballs still place these files under
# top-level `contrib/` for `install.sh` and extract-and-configure workflows.
#
# | Path | Purpose |
# |---|---|
# | `super.toml.default` | Minimal `$SUPER_ROOT/conf/super.toml` |
# | `conf.d/demo.toml.example` | Sample stack (copy → `demo.toml` to activate) |
# | `systemd/superd.service` | Linux systemd unit (`Type=simple`, foreground) |
# | `launchd/com.schiplat.superd.plist` | macOS launchd (RunAtLoad + KeepAlive) |
# | `rc.d/superd` | FreeBSD rc.d (`daemon(8)` + `--foreground`, boot via `superd_enable`) |
#
# Preferred install path: `install.sh` (embeds the same defaults and enables the
# OS service). These files ship in release tarballs under `contrib/` for
# extract-and-configure workflows.
