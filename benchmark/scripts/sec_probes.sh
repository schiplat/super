#!/usr/bin/env bash
# Default-bind / unauthenticated control / uid / file-mode probes.
# Uses the BENCHmark control-plane config (loopback + supervisor inet :9001).
# Product-default posture is documented separately in BENCHMARK_PLAN §7.
set -euo pipefail
TARGET="${1:?super-oss|super-pro|supervisord|pm2}"
INSTANCE="${2:?instance dir}"
OUT="${3:?output json}"

python3 - "$TARGET" "$INSTANCE" "$OUT" <<'PY'
import json, os, stat, subprocess, sys, time
from pathlib import Path

target, instance, out = sys.argv[1], Path(sys.argv[2]), Path(sys.argv[3])
env = os.environ.copy()

def run(cmd, env=env, timeout=8):
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, env=env)
        return p.returncode, p.stdout, p.stderr
    except Exception as e:
        return 99, "", str(e)

def ss():
    code, so, se = run(["ss", "-tlnp"])
    return so + se

result = {
    "target": target,
    "ts": time.time(),
    "lab_uid": os.getuid(),
    "lab_all_root": os.getuid() == 0,
    "note": "SEC-3 is non-informative when the lab user is root for every arm.",
}

if target in ("super-oss", "super-pro"):
    result["listen"] = [l for l in ss().splitlines() if "9002" in l]
    headers = []
    token = env.get("SUPER_BENCH_AUTH_SECRET")
    # SEC-2: unauthenticated GET
    code, so, se = run(["curl", "-sS", "-o", "/dev/null", "-w", "%{http_code}",
                        "http://127.0.0.1:9002/api/v1/programs"])
    result["unauth_programs_http"] = so.strip() or se.strip()
    if target == "super-pro":
        result["sec2_expected"] = "401_or_403"
    else:
        result["sec2_expected"] = "200_on_loopback_no_auth"
    cfg = instance / "conf" / "super.toml"
    if cfg.exists():
        result["config_mode"] = oct(cfg.stat().st_mode & 0o777)
    # daemon / child uids
    code, so, _ = run(["ps", "-o", "user=,pid=,comm=", "-C", "superd"])
    result["ps_daemon"] = so.strip()
    code, so, _ = run(["ps", "-o", "user=,pid=,comm=", "-C", "payloads"])
    result["ps_payloads"] = so.strip()

elif target == "supervisord":
    result["listen"] = [l for l in ss().splitlines() if "9001" in l]
    code, so, se = run(["curl", "-sS", "-o", "/dev/null", "-w", "%{http_code}",
                        "http://127.0.0.1:9001/"])
    result["unauth_inet_http"] = so.strip() or se.strip()
    result["sec2_expected"] = "200_if_no_htpasswd_in_bench_config"
    conf = instance / "supervisord.conf"
    if conf.exists():
        result["config_mode"] = oct(conf.stat().st_mode & 0o777)
    code, so, _ = run(["ps", "-o", "user=,pid=,comm=", "-C", "supervisord"])
    result["ps_daemon"] = so.strip()

elif target == "pm2":
    home = instance / "pm2-home"
    env = {**env, "PM2_HOME": str(home)}
    sock = list(home.glob("**/*.sock"))
    result["sockets"] = [str(s) for s in sock]
    result["pm2_home_mode"] = oct(home.stat().st_mode & 0o777) if home.exists() else None
    result["sec2_expected"] = "local_socket_file_perms"
    code, so, _ = run(["ps", "-o", "user=,pid=,comm="], env=env)
    result["ps_sample"] = "\n".join(l for l in so.splitlines() if "PM2" in l or "pm2" in l)[:2000]

out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(result, indent=2) + "\n")
print(f"wrote {out}")
PY
