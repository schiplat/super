# Quick Start demo recording (OSS)

Terminal walkthrough for the public
[Quick Start](https://super.docs.sconts.com/docs/01-getting-started/quick-start/)
(CLI path on the host — not the Docker/busybox tabs).

## Story (three acts)

1. **Deploy** — instance layout, `conf/super.toml`, `superd --daemon`, `/health`
2. **Create** — `super add demo-web`, `list`, `info`, HTTP probe
3. **Day-to-day ops** — `logs`, `restart --wait-healthy`, `stop` / `start`, `list`

Canvas: **1280×700** / FontSize 15 / `COLUMNS=140` (matched so `super list` does not wrap).
**Mono theme** (white cmds / gray output + `NO_COLOR`) keeps README gif **≤300KB**.
Recording shell is **fish** (`fish_preexec` repaints command lines).

## Run

```bash
# Binaries: set SUPER_BIN=… or use /tmp/super-demo/bin or super/target/release

cd examples/demo
export VHS_NO_SANDBOX=true TMPDIR=/tmp
./scripts/record-quickstart.sh
```

Outputs land in `tapes/out/`: **`quick-start.gif` is committed** (README embed);
`*.mp4` and other lab artifacts stay gitignored.
```bash
# Prepare layout only (interactive):
./scripts/bootstrap-quickstart.sh
source /tmp/super-quickstart-demo/record.env
cd "$SUPER_ROOT"
superd --daemon
```

Requires: `vhs`, `ttyd`, `ffmpeg`, Chromium, `python3`, **`fish`** (command-line
syntax highlighting), and release `superd`/`super`.
