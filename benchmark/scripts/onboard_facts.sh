#!/usr/bin/env bash
# Day-0 onboarding facts (ONB matrix). No score.
#
# Task (locked): files/deps needed so *one* managed process can be declared
# in config and queried via the official control interface.
#
# Usage:
#   ./scripts/onboard_facts.sh OUT.json
#   ./scripts/onboard_facts.sh OUT.json --generated DIR
#
# DIR is generator output (super-oss/, super-pro/, supervisord/, pm2/).
set -euo pipefail
OUT="${1:?output json}"
shift || true
GENERATED=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --generated)
      GENERATED="${2:?}"
      shift 2
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 1
      ;;
  esac
done

python3 - "$OUT" "${GENERATED}" <<'PY'
import json, os, re, shutil, subprocess, sys, time
from pathlib import Path

out = Path(sys.argv[1])
generated = Path(sys.argv[2]) if sys.argv[2] else None


def which_ver(bin_name, args=("--version",)):
    path = shutil.which(bin_name)
    if not path:
        return {"on_path": False, "path": None, "version": None}
    try:
        p = subprocess.run([bin_name, *args], capture_output=True, text=True, timeout=5)
        ver = (p.stdout or p.stderr).strip().splitlines()[:1]
        version = ver[0] if ver else ""
    except Exception as e:
        version = f"error:{e}"
    return {"on_path": True, "path": path, "version": version}


def count_files(root: Path):
    files = []
    if not root.is_dir():
        return files
    for p in sorted(root.rglob("*")):
        if p.is_file():
            files.append(str(p.relative_to(root)))
    return files


def parse_super_toml(path: Path):
    host, port, auth, license_tbl = "127.0.0.1", 9002, False, False
    if not path.exists():
        return None
    text = path.read_text()
    m = re.search(r'(?m)^\s*host\s*=\s*"([^"]+)"', text)
    if m:
        host = m.group(1)
    m = re.search(r'(?m)^\s*port\s*=\s*(\d+)', text)
    if m:
        port = int(m.group(1))
    if re.search(r'(?m)^\s*auth_secret\s*=', text):
        auth = True
    if re.search(r'(?m)^\s*\[license\]', text):
        license_tbl = True
    return {
        "kind": "http",
        "bind": f"{host}:{port}",
        "auth_secret_in_file": auth,
        "license_section_in_file": license_tbl,
    }


def parse_supervisor_conf(path: Path):
    inet = None
    if not path.exists():
        return {"inet_http_server": None}
    text = path.read_text()
    m = re.search(r"(?ms)\[inet_http_server\]\s*port\s*=\s*(\S+)", text)
    if m:
        inet = m.group(1).strip()
    return {"kind": "xmlrpc_http" if inet else "none_until_configured", "inet_http_server": inet}


def arm_block(name, generated):
    block = {
        "arm": name,
        "runtime_deps_observed": {},
        "min_config_files": [],
        "config_file_count": 0,
        "control_plane_bench": {},
        "control_plane_product_default": {},
        "logging_notes": [],
        "pro_extra_steps": [],
        "footguns_documented": [],
    }
    if generated and (generated / name).is_dir():
        files = count_files(generated / name)
        block["min_config_files"] = files
        block["config_file_count"] = len(files)

    if name in ("super-oss", "super-pro"):
        block["runtime_deps_observed"] = {
            "superd": which_ver("superd"),
            "super": which_ver("super"),
            "python": which_ver("python3"),
            "node": which_ver("node"),
        }
        toml = (generated / name / "conf" / "super.toml") if generated else None
        bench = parse_super_toml(toml) if toml else {
            "kind": "http",
            "bind": "127.0.0.1:9002",
            "auth_secret_in_file": name == "super-pro",
        }
        block["control_plane_bench"] = bench
        block["control_plane_product_default"] = {
            "kind": "http",
            "bind": "127.0.0.1:9002",
            "auth": False if name == "super-oss" else "security_plugin_plus_auth_secret",
            "note": "OSS: loopback, no API auth. Non-loopback fail-closed without allow_insecure_public_bind or security plugin.",
        }
        block["logging_notes"] = [
            "Child stdout/stderr files under storage.log_dir; rotation is built into OSS (bench disables backups).",
        ]
        block["footguns_documented"] = [
            "Programs are not [[program]] in super.toml; they load from [include] JSON or the API/snapshot.",
            "SUPER_ROOT must contain conf/super.toml (env SUPER_CONFIG is not how superd reads config).",
        ]
        if name == "super-pro":
            block["pro_extra_steps"] = [
                "Copy security.* and isolation.* into $SUPER_ROOT/plugins/ (no lib prefix).",
                "Set [license].key (vendor-issued; not in git) and auth_secret.",
                "Licensed startup hard-fails without security plugin + auth_secret.",
            ]
            plugins = os.environ.get("SUPER_BENCH_PLUGINS_DIR")
            block["plugins_dir_env_set"] = bool(plugins)
            if plugins and Path(plugins).is_dir():
                block["plugin_files_in_env_dir"] = [
                    p.name for p in Path(plugins).iterdir() if p.is_file()
                ]
    elif name == "supervisord":
        block["runtime_deps_observed"] = {
            "supervisord": which_ver("supervisord"),
            "supervisorctl": which_ver("supervisorctl"),
            "python": which_ver("python3"),
        }
        conf = (generated / name / "supervisord.conf") if generated else None
        block["control_plane_bench"] = parse_supervisor_conf(conf) if conf else {}
        block["control_plane_product_default"] = {
            "kind": "no_inet_until_configured",
            "inet_http_server": None,
            "note": "Stock supervisord has no inet HTTP; bench opens 127.0.0.1:9001 for supervisorctl.",
        }
        block["logging_notes"] = [
            "Per-program stdout/stderr files; rotation via logfile_maxbytes (bench sets 0).",
        ]
        block["footguns_documented"] = [
            "XML-RPC inet without username/password is reachable on that bind.",
            "Python runtime required.",
        ]
    elif name == "pm2":
        block["runtime_deps_observed"] = {
            "pm2": which_ver("pm2"),
            "node": which_ver("node"),
            "npm": which_ver("npm"),
        }
        block["control_plane_bench"] = {
            "kind": "local_js_rpc",
            "pm2_home_isolated": True,
            "tcp": None,
        }
        block["control_plane_product_default"] = {
            "kind": "unix_socket_under_PM2_HOME",
            "note": "~/.pm2 by default; this bench uses a private PM2_HOME per instance.",
        }
        block["logging_notes"] = [
            "out_file/error_file in ecosystem; rotation typically needs pm2-logrotate (not installed by this suite).",
        ]
        block["footguns_documented"] = [
            "Non-Node apps are fork mode; cluster mode is Node-only (out of scope for this bench).",
            "pm2 kill affects the PM2_HOME in use — bench isolates PM2_HOME so it does not kill a user daemon.",
        ]
    return block


report = {
    "schema": "onboard_facts.v1",
    "scoring": False,
    "task_locked": (
        "From a clean instance directory: declare one managed process in the "
        "tool's usual config and query it via the official control interface "
        "(super CLI/HTTP, supervisorctl, pm2). This file records facts only."
    ),
    "not_measured": [
        "time-to-first-process stopwatch (ONB-1 deferred)",
        "doc-reading time",
        "subjective ease scores",
    ],
    "distinction": {
        "ONB": "Day-0: dependencies, files, control-plane shape",
        "MGT": "Day-2: daemon already running, operation latency",
    },
    "collected_unix": int(time.time()),
    "generated_dir": str(generated) if generated else None,
    "arms": {
        name: arm_block(name, generated)
        for name in ("super-oss", "super-pro", "supervisord", "pm2")
    },
}

out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(report, indent=2) + "\n")
print(f"wrote {out}")
PY
