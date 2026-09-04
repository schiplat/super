#!/usr/bin/env bash
# MGT-1 cold start, MGT-2 control latency, MGT-3 config apply without daemon replace.
# Official interfaces only: super CLI / curl, supervisorctl, pm2.
set -euo pipefail
TARGET="${1:?arm}"
INSTANCE="${2:?instance dir}"
OUT="${3:?output json}"
N="${4:?process count}"
TOKEN="${SUPER_BENCH_AUTH_SECRET:-}"

python3 - "$TARGET" "$INSTANCE" "$OUT" "$N" "$TOKEN" <<'PY'
import json, os, subprocess, sys, time
from pathlib import Path

target, instance, out, n, token = sys.argv[1], Path(sys.argv[2]), Path(sys.argv[3]), int(sys.argv[4]), sys.argv[5]
env = os.environ.copy()
if target == "pm2":
    env["PM2_HOME"] = str(instance / "pm2-home")
if target in ("super-oss", "super-pro"):
    env["SUPER_ROOT"] = str(instance.resolve())

def run(cmd, timeout=60):
    t0 = time.perf_counter()
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, env=env)
    dt = (time.perf_counter() - t0) * 1000
    return p.returncode, p.stdout, p.stderr, dt

def curl_super(path, method="GET"):
    cmd = ["curl", "-sS", "-o", "/tmp/super-bench-curl.out", "-w", "%{http_code}",
           "-X", method, f"http://127.0.0.1:9002{path}"]
    if token:
        cmd[1:1] = ["-H", f"Authorization: Bearer {token}"]
    return run(cmd)

result = {"target": target, "n": n, "ops": []}

# MGT-1: assume daemon already started by runner/orchestrator; poll until N running.
t_deadline = time.time() + 60
mgt1_ms = None
while time.time() < t_deadline:
    if target in ("super-oss", "super-pro"):
        code, so, se, _ = curl_super("/api/v1/programs")
        body = Path("/tmp/super-bench-curl.out").read_text() if Path("/tmp/super-bench-curl.out").exists() else ""
        running = body.count('"state":"Running"') + body.count('"state":"Healthy"')
    elif target == "supervisord":
        conf = instance / "supervisord.conf"
        code, so, se, _ = run(["supervisorctl", "-c", str(conf), "status"])
        running = sum(1 for l in so.splitlines() if "RUNNING" in l)
    else:
        code, so, se, _ = run(["pm2", "jlist"])
        running = so.count('"status":"online"')
    if running >= n:
        mgt1_ms = (60 - (t_deadline - time.time())) * 1000
        break
    time.sleep(0.2)
result["mgt1_cold_poll_ms"] = mgt1_ms
result["ops"].append({"id": "MGT-1", "ok": mgt1_ms is not None, "ms": mgt1_ms})

# MGT-2: 20 status calls
lat = []
for _ in range(20):
    if target in ("super-oss", "super-pro"):
        _, _, _, dt = curl_super("/api/v1/programs")
    elif target == "supervisord":
        _, _, _, dt = run(["supervisorctl", "-c", str(instance / "supervisord.conf"), "status"])
    else:
        _, _, _, dt = run(["pm2", "list"])
    lat.append(dt)
lat.sort()
result["mgt2_status_p50_ms"] = lat[len(lat)//2]
result["mgt2_status_p95_ms"] = lat[int(len(lat)*0.95)-1]
result["ops"].append({"id": "MGT-2", "p50": result["mgt2_status_p50_ms"], "p95": result["mgt2_status_p95_ms"]})

# MGT-3: apply a no-op config refresh; record whether daemon pid changed.
def daemon_pid():
    if target in ("super-oss", "super-pro"):
        code, so, _, _ = run(["pgrep", "-n", "superd"])
        return so.strip()
    if target == "supervisord":
        p = instance / "run" / "supervisord.pid"
        return p.read_text().strip() if p.exists() else ""
    p = instance / "pm2-home" / "pm2.pid"
    return p.read_text().strip() if p.exists() else ""

before = daemon_pid()
t0 = time.perf_counter()
if target in ("super-oss", "super-pro"):
    cmd = ["curl", "-sS", "-X", "POST", "http://127.0.0.1:9002/api/v1/system/reload"]
    if token:
        cmd.extend(["-H", f"Authorization: Bearer {token}"])
    run(cmd)
    n_ops = 1
elif target == "supervisord":
    run(["supervisorctl", "-c", str(instance / "supervisord.conf"), "reread"])
    run(["supervisorctl", "-c", str(instance / "supervisord.conf"), "update"])
    n_ops = 2
else:
    run(["pm2", "reloadLogs"])
    n_ops = 1
elapsed = (time.perf_counter() - t0) * 1000
after = daemon_pid()
result["mgt3"] = {
    "ops": n_ops,
    "ms": elapsed,
    "daemon_pid_unchanged": before == after and before != "",
    "semantic": "reload_or_reread_update_or_reloadLogs",
}
result["ops"].append({"id": "MGT-3", **result["mgt3"]})

out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(result, indent=2) + "\n")
print(f"wrote {out}")
PY
