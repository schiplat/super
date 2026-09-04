#!/usr/bin/env bash
# Collect a dated snapshot manifest (versions + host). No secrets.
set -euo pipefail
OUT="${1:?output json path}"
BENCH_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
python3 - "$OUT" "$BENCH_ROOT" <<'PY'
import json, os, platform, shutil, subprocess, sys, time
from pathlib import Path

out, bench = Path(sys.argv[1]), Path(sys.argv[2])

def run(cmd):
    try:
        return subprocess.check_output(cmd, text=True, stderr=subprocess.DEVNULL).strip()
    except Exception:
        return ""

def which_ver(bin, args):
    if not shutil.which(bin):
        return ""
    return run([bin, *args])

mem_kb = 0
for line in open("/proc/meminfo"):
    if line.startswith("MemTotal:"):
        mem_kb = int(line.split()[1])
        break

nproc = os.cpu_count() or 0
load = open("/proc/loadavg").read().split()[:3]
ulimit_n = run(["bash", "-lc", "ulimit -n"])
ulimit_u = run(["bash", "-lc", "ulimit -u"])

manifest = {
    "snapshot_date": time.strftime("%Y-%m-%d"),
    "collected_unix": int(time.time()),
    "host": {
        "hostname": platform.node(),
        "arch": platform.machine(),
        "kernel": platform.release(),
        "os_image": run(["bash", "-lc", "grep PRETTY_NAME /etc/os-release | cut -d= -f2 | tr -d '\"'"]),
        "hypervisor": run(["bash", "-lc", "systemd-detect-virt 2>/dev/null || true"]),
        "nproc": nproc,
        "mem_total_kb": mem_kb,
        "ulimit_n": ulimit_n,
        "ulimit_u": ulimit_u,
        "loadavg": load,
        "swap_total_kb": next((int(l.split()[1]) for l in open("/proc/meminfo") if l.startswith("SwapTotal:")), 0),
    },
    "versions": {
        "superd": which_ver("superd", ["--version"]),
        "super": which_ver("super", ["--version"]),
        "supervisord": which_ver("supervisord", ["--version"]),
        "pm2": which_ver("pm2", ["--version"]),
        "node": which_ver("node", ["--version"]),
        "python": which_ver("python3", ["--version"]),
        "rustc": which_ver("rustc", ["--version"]),
        "sysinfo": "0.30",
    },
    "git": {
        # tools/benchmark → repo root
        "super_sha": run(["git", "-C", str(bench.parent.parent), "rev-parse", "HEAD"]),
        "super_describe": run(["git", "-C", str(bench.parent.parent), "describe", "--tags", "--always"]),
        "benchmark_sha": run(["git", "-C", str(bench.parent.parent), "log", "-1", "--format=%H", "--", "tools/benchmark"]),
    },
    "methodology": {
        "latest_stable_snapshot": True,
        "arms": ["super-oss", "super-pro", "supervisord", "pm2"],
        "topology": os.environ.get("MODE", "one-host-one-arm"),
        "arm": os.environ.get("BENCH_ARM", ""),
        "repeats_phase_b": 3,
        "loop": "scenario_outer",
        "switch_gate": {
            "quiet_sec": int(os.environ.get("SUPER_BENCH_QUIET_SEC", "30")),
            "max_wait_sec": int(os.environ.get("SUPER_BENCH_GATE_MAX_WAIT", "180")),
            "loadavg_delta_max": float(os.environ.get("SUPER_BENCH_LOAD_DELTA", "0.5")),
            "mem_ratio": float(os.environ.get("SUPER_BENCH_MEM_RATIO", "0.85")),
            "drop_caches": os.environ.get("SUPER_BENCH_DROP_CACHES", "1") == "1",
            "on_fail": "abort_round",
        },
        "round_cooldown_sec": 60,
        "cpu_relative_to_single_core": True,
        "rss_not_pss": True,
        "log_rotation_disabled": True,
    },
}
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(manifest, indent=2) + "\n")
print(f"wrote {out}")
PY
