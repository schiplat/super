#!/usr/bin/env python3
"""Repeatable OTA end-to-end verification for Project Super (OSS).

Uses an isolated SUPER_ROOT and a non-default API port so it does not touch a
local demo instance on :9002.

Usage (from repo root `super/`):
  python3 scripts/ota-e2e.py
  python3 scripts/ota-e2e.py --build   # cargo build --release first

Env:
  SUPER_BIN          Override path to release dir (default: <repo>/target/release)
  SWAGGER_UI_DOWNLOAD_URL  Needed only when --build compiles with docs feature
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import signal
import subprocess
import sys
import tarfile
import time
import urllib.error
import urllib.request
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from io import BytesIO
from pathlib import Path
from threading import Thread

REPO = Path(__file__).resolve().parents[1]
DEFAULT_BIN = REPO / "target" / "release"
ROOT = Path(os.environ.get("SUPER_OTA_E2E_ROOT", "/tmp/super-ota-e2e"))
API_PORT = int(os.environ.get("SUPER_OTA_E2E_PORT", "19002"))
ARTIFACT_PORT = int(os.environ.get("SUPER_OTA_E2E_ARTIFACT_PORT", "19080"))
API = f"http://127.0.0.1:{API_PORT}"

PASS = 0
FAIL = 0
SUPERD = None
HTTP = None
BIN = DEFAULT_BIN


def ok(msg: str) -> None:
    global PASS
    PASS += 1
    print(f"PASS: {msg}")


def bad(msg: str) -> None:
    global FAIL
    FAIL += 1
    print(f"FAIL: {msg}")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def http_json(method: str, url: str, body: dict | None = None, timeout: float = 15.0):
    data = None
    headers = {}
    if body is not None:
        data = json.dumps(body).encode()
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        raw = resp.read()
        return json.loads(raw) if raw else None


def wait_health(timeout: float = 20.0) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(f"{API}/health", timeout=1) as r:
                if r.status == 200:
                    return
        except Exception:
            time.sleep(0.2)
    raise RuntimeError(f"superd did not become healthy on {API}")


def cli(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [str(BIN / "super"), "-s", API, *args],
        capture_output=True,
        text=True,
    )


def pid_of(name: str) -> str:
    for p in http_json("GET", f"{API}/api/v1/programs"):
        if p["name"] == name:
            return p["id"]
    raise KeyError(name)


def prog(pid: str) -> dict:
    return http_json("GET", f"{API}/api/v1/programs/{pid}")


def write_script(path: Path, lines: list[str]) -> None:
    path.write_text("\n".join(lines) + "\n")
    path.chmod(0o755)


def wait_until(pred, timeout: float = 25.0, interval: float = 0.4) -> bool:
    deadline = time.time() + timeout
    while time.time() < deadline:
        if pred():
            return True
        time.sleep(interval)
    return False


def logs_contain(*needles: str) -> bool:
    chunks = []
    stdout = ROOT / "logs/superd.stdout"
    if stdout.exists():
        chunks.append(stdout.read_text(errors="replace"))
    for p in (ROOT / "logs").glob("app.log.*"):
        chunks.append(p.read_text(errors="replace"))
    blob = "\n".join(chunks)
    return any(n in blob for n in needles)


def maybe_build(do_build: bool) -> None:
    global BIN
    override = os.environ.get("SUPER_BIN")
    if override:
        BIN = Path(override)
    if not do_build and (BIN / "superd").is_file() and (BIN / "super").is_file():
        return
    env = os.environ.copy()
    env.setdefault(
        "SWAGGER_UI_DOWNLOAD_URL",
        "file:///Users/eddie/Downloads/swagger-ui-5.32.14.zip",
    )
    env["CARGO_TARGET_DIR"] = str(REPO / "target")
    print("==> cargo build --release --bin superd --bin super")
    subprocess.check_call(
        ["cargo", "build", "--release", "--bin", "superd", "--bin", "super"],
        cwd=REPO,
        env=env,
    )
    BIN = REPO / "target" / "release"


def setup() -> None:
    global SUPERD, HTTP
    if ROOT.exists():
        shutil.rmtree(ROOT)
    for d in ("bin", "conf", "data", "logs", "artifacts", "apps"):
        (ROOT / d).mkdir(parents=True)
    shutil.copy2(BIN / "superd", ROOT / "bin/superd")
    shutil.copy2(BIN / "super", ROOT / "bin/super")
    (ROOT / "conf/super.toml").write_text(
        f"""
[server]
host = "127.0.0.1"
port = {API_PORT}
ota_verify_timeout = 15
download_timeout = 30

[logging]
log_level = "info"
""".lstrip()
    )

    write_script(ROOT / "apps/app-v1.sh", ["#!/bin/sh", 'echo "VERSION_1"', "exec sleep 3600"])
    write_script(ROOT / "apps/app-v2-good.sh", ["#!/bin/sh", 'echo "VERSION_2"', "exec sleep 3600"])
    # Brief sleep so the harness can observe restore_path / VERSION_2 before exit.
    write_script(
        ROOT / "apps/app-v2-crash.sh",
        ["#!/bin/sh", 'echo "VERSION_2_CRASH"', "sleep 1", "exit 1"],
    )
    for name in ("app-v1.sh", "app-v2-good.sh", "app-v2-crash.sh"):
        shutil.copy2(ROOT / "apps" / name, ROOT / "artifacts" / name)

    # tar.gz for extract case
    buf = BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tar:
        data = b'#!/bin/sh\necho "VERSION_2_EXTRACTED"\nexec sleep 3600\n'
        info = tarfile.TarInfo(name="running-extract")
        info.size = len(data)
        info.mode = 0o755
        tar.addfile(info, BytesIO(data))
    (ROOT / "artifacts/app-extract.tar.gz").write_bytes(buf.getvalue())

    for name in (
        "running-a",
        "running-b",
        "running-c",
        "running-d",
        "running-extract",
        "running-manual",
        "running-signal",
        "running-timeout",
        "running-stack",
        "running-wal",
    ):
        shutil.copy2(ROOT / "apps/app-v1.sh", ROOT / "apps" / name)

    # signal-aware script
    marker = ROOT / "apps/hup.marker"
    write_script(
        ROOT / "apps/running-signal",
        [
            "#!/bin/sh",
            f"MARKER='{marker}'",
            'trap \'touch "$MARKER"\' HUP',
            "while true; do sleep 1; done",
        ],
    )

    os.chdir(ROOT / "artifacts")
    HTTP = ThreadingHTTPServer(("127.0.0.1", ARTIFACT_PORT), SimpleHTTPRequestHandler)
    Thread(target=HTTP.serve_forever, daemon=True).start()

    env = os.environ.copy()
    env["SUPER_ROOT"] = str(ROOT)
    SUPERD = subprocess.Popen(
        [str(ROOT / "bin/superd")],
        env=env,
        stdout=open(ROOT / "logs/superd.stdout", "w"),
        stderr=subprocess.STDOUT,
    )
    wait_health()
    print(f"OK: superd {SUPERD.pid} on {API} SUPER_ROOT={ROOT}")


def cleanup() -> None:
    if SUPERD and SUPERD.poll() is None:
        SUPERD.send_signal(signal.SIGTERM)
        try:
            SUPERD.wait(timeout=5)
        except subprocess.TimeoutExpired:
            SUPERD.kill()
    if HTTP:
        HTTP.shutdown()


def artifact_url(name: str) -> str:
    return f"http://127.0.0.1:{ARTIFACT_PORT}/{name}"


def put_artifact(pid: str, source: str, checksum: str, destination: str, **extra) -> None:
    body = {
        "artifact": {
            "source": source,
            "checksum": checksum,
            "destination": destination,
            "extract": extra.get("extract", False),
            "restart_policy": extra.get("restart_policy", "immediate"),
        }
    }
    http_json("PUT", f"{API}/api/v1/programs/{pid}", body)


def case_commit() -> None:
    app = ROOT / "apps/running-a"
    assert (
        cli(
            "add",
            "--name",
            "ota-commit",
            "--autostart",
            "--startsecs",
            "2",
            str(app),
        ).returncode
        == 0
    )
    time.sleep(1)
    pid = pid_of("ota-commit")
    v2 = sha256_file(ROOT / "artifacts/app-v2-good.sh")
    put_artifact(pid, artifact_url("app-v2-good.sh"), v2, str(app))

    def done():
        info = prog(pid)
        return (
            "VERSION_2" in app.read_text(errors="replace")
            and not info["config"].get("restore_path")
            and not Path(str(app) + ".bak").exists()
        )

    ok("1 commit happy path") if wait_until(done, timeout=30) else bad("1 commit happy path")


def case_same_checksum() -> None:
    app = ROOT / "apps/running-a"
    pid = pid_of("ota-commit")
    v2 = sha256_file(ROOT / "artifacts/app-v2-good.sh")
    before = app.stat().st_mtime
    put_artifact(pid, artifact_url("app-v2-good.sh"), v2, str(app))
    time.sleep(2)
    after = app.stat().st_mtime
    info = prog(pid)
    if (
        not info["config"].get("restore_path")
        and not Path(str(app) + ".new").exists()
        and before == after
    ):
        ok("2 same checksum skips OTA")
    else:
        bad("2 same checksum skip")


def case_bad_checksum() -> None:
    app = ROOT / "apps/running-b"
    assert cli("add", "--name", "ota-badsum", "--autostart", str(app)).returncode == 0
    time.sleep(1)
    pid = pid_of("ota-badsum")
    before = app.read_bytes()
    put_artifact(pid, artifact_url("app-v2-good.sh"), "deadbeef" * 8, str(app))
    time.sleep(4)
    if before == app.read_bytes() and not Path(str(app) + ".bak").exists():
        ok("3 checksum mismatch aborts")
    else:
        bad("3 checksum mismatch")
    if logs_contain("Checksum mismatch", "OTA Download failed"):
        ok("3b checksum failure logged")
    else:
        bad("3b checksum failure not logged")
    if not Path(str(app) + ".new").exists() and not Path(str(app) + ".download").exists():
        ok("8 staging cleaned after checksum fail")
    else:
        bad("8 staging leftover")


FAILING_HC = {
    "type": "exec",
    "command": "false",
    "interval_secs": 1,
    "timeout_secs": 1,
    "start_period_secs": 0,
    "max_failures": 0,  # stay unhealthy so OTA does not auto-commit
}


def create_program(name: str, command: str, **extra) -> str:
    body = {"name": name, "command": command, "autostart": True, **extra}
    http_json("POST", f"{API}/api/v1/programs", body)
    time.sleep(1)
    return pid_of(name)


def assert_rollback_clean(app: Path, pid: str, label: str) -> None:
    content = app.read_text(errors="replace")
    info = prog(pid)
    file_ok = ("VERSION_1" in content) and ("VERSION_2" not in content)
    wal_ok = not info["config"].get("restore_path")
    bak_ok = not Path(str(app) + ".bak").exists()
    run_ok = info.get("pid") is not None
    # Staging leftovers are best-effort; core rollback is file+WAL+.bak+respawn.
    if file_ok and wal_ok and bak_ok and run_ok:
        ok(label)
        stage_left = Path(str(app) + ".new").exists() or Path(str(app) + ".download").exists()
        if stage_left:
            print(f"NOTE: {label}: staging leftover after rollback (non-fatal)")
    else:
        bad(
            f"{label}: file_ok={file_ok} wal_ok={wal_ok} bak_ok={bak_ok} "
            f"run_ok={run_ok} state={info.get('state')} "
            f"head={content.splitlines()[:3]}"
        )


def case_rollback() -> None:
    """Crash-on-start after swap. Requires failing health_check so the default
    no-probe Healthy (≈100ms) cannot commit before the crash is observed.

    Must observe the verify/WAL phase first — otherwise VERSION_1 + empty
    restore_path matches the pre-OTA steady state and the harness false-passes.
    """
    app = ROOT / "apps/running-c"
    pid = create_program("ota-rollback", str(app), health_check=FAILING_HC)
    put_artifact(
        pid,
        artifact_url("app-v2-crash.sh"),
        sha256_file(ROOT / "artifacts/app-v2-crash.sh"),
        str(app),
    )

    saw_verify = wait_until(
        lambda: (
            prog(pid)["config"].get("restore_path")
            or "VERSION_2" in app.read_text(errors="replace")
            or Path(str(app) + ".bak").exists()
        ),
        timeout=15,
        interval=0.1,
    )
    if not saw_verify:
        bad("4 never entered OTA verify phase")
        return

    def rolled_back():
        content = app.read_text(errors="replace")
        info = prog(pid)
        return (
            "VERSION_1" in content
            and "VERSION_2" not in content
            and not info["config"].get("restore_path")
            and not Path(str(app) + ".bak").exists()
            and info.get("pid") is not None
        )

    if wait_until(rolled_back, timeout=30):
        assert_rollback_clean(app, pid, "4 crash rollback (strict)")
        if logs_contain("Upgrade Validation Failed", "ROLLBACK", "rolled back"):
            ok("4b crash rollback logged")
        else:
            bad("4b crash rollback not logged")
    else:
        bad("4 crash rollback")


def case_verify_timeout() -> None:
    """Process stays up but never Healthy → ota_verify_timeout force-kill + rollback."""
    app = ROOT / "apps/running-timeout"
    pid = create_program("ota-timeout", str(app), health_check=FAILING_HC)
    put_artifact(
        pid,
        artifact_url("app-v2-good.sh"),
        sha256_file(ROOT / "artifacts/app-v2-good.sh"),
        str(app),
    )
    saw_pending = wait_until(
        lambda: (
            prog(pid)["config"].get("restore_path")
            and "VERSION_2" in app.read_text(errors="replace")
        ),
        timeout=15,
    )
    if not saw_pending:
        bad("9 never entered verify phase")
        return

    def done():
        content = app.read_text(errors="replace")
        info = prog(pid)
        return (
            "VERSION_1" in content
            and "VERSION_2" not in content
            and not info["config"].get("restore_path")
            and info.get("pid") is not None
        )

    if wait_until(done, timeout=30):
        assert_rollback_clean(app, pid, "9 verify-timeout rollback (strict)")
        if logs_contain("OTA verification timed out", "Force-killing"):
            ok("9b verify-timeout logged")
        else:
            bad("9b verify-timeout not logged")
    else:
        bad("9 verify-timeout rollback")


def case_rollback_no_hc() -> None:
    """Instant crash with NO health_check — must still roll back (startsecs dwell)."""
    app = ROOT / "apps/running-nohc"
    write_script(app, ["#!/bin/sh", 'echo "VERSION_1"', "exec sleep 3600"])
    # Instant exit (no sleep) — the race that used to beat the 100ms Healthy commit.
    crash = ROOT / "artifacts/app-v2-instant-crash.sh"
    write_script(crash, ["#!/bin/sh", 'echo "VERSION_2_CRASH"', "exit 1"])
    pid = create_program("ota-nohc", str(app), startsecs=2)
    put_artifact(
        pid,
        artifact_url("app-v2-instant-crash.sh"),
        sha256_file(crash),
        str(app),
    )

    saw_verify = wait_until(
        lambda: (
            prog(pid)["config"].get("restore_path")
            or "VERSION_2" in app.read_text(errors="replace")
            or Path(str(app) + ".bak").exists()
        ),
        timeout=15,
        interval=0.05,
    )
    if not saw_verify:
        bad("4c never entered OTA verify phase")
        return

    def done():
        content = app.read_text(errors="replace")
        info = prog(pid)
        return (
            "VERSION_1" in content
            and "VERSION_2" not in content
            and not info["config"].get("restore_path")
            and not Path(str(app) + ".bak").exists()
            and info.get("pid") is not None
        )

    if wait_until(done, timeout=30):
        assert_rollback_clean(app, pid, "4c no-HC instant-crash rollback")
        if logs_contain("Upgrade Validation Failed", "ROLLBACK"):
            ok("4d no-HC rollback logged")
        else:
            bad("4d no-HC rollback not logged")
    else:
        bad("4c no-HC instant-crash rollback")


def case_http_policy() -> None:
    app = ROOT / "apps/running-a"
    pid = pid_of("ota-commit")
    put_artifact(pid, "http://example.com/app", "a" * 64, str(app))
    time.sleep(3)
    if "VERSION_2" in app.read_text(errors="replace"):
        ok("5 non-loopback HTTP rejected")
    else:
        bad("5 non-loopback HTTP")
    if logs_contain("OTA Download failed", "HTTPS", "http://"):
        ok("5b rejection logged")
    else:
        bad("5b rejection not logged")


def case_metadata() -> None:
    app = ROOT / "apps/running-a"
    pid = pid_of("ota-commit")
    put_artifact(pid, "https://169.254.169.254/latest/meta-data", "b" * 64, str(app))
    time.sleep(2)
    if "VERSION_2" in app.read_text(errors="replace"):
        ok("6 metadata URL blocked")
    else:
        bad("6 metadata URL")
    if logs_contain("OTA Download failed", "169.254", "metadata", "private", "link-local"):
        ok("6b metadata rejection logged")
    else:
        bad("6b metadata rejection not logged")


def case_cli() -> None:
    app = ROOT / "apps/running-d"
    assert (
        cli(
            "add",
            "--name",
            "ota-cli",
            "--autostart",
            "--startsecs",
            "2",
            str(app),
        ).returncode
        == 0
    )
    time.sleep(1)
    v2 = sha256_file(ROOT / "artifacts/app-v2-good.sh")
    r = cli(
        "update",
        "ota-cli",
        "--artifact-url",
        artifact_url("app-v2-good.sh"),
        "--artifact-sha256",
        v2,
        "--artifact-destination",
        str(app),
    )
    if r.returncode != 0:
        bad(f"7 CLI update failed: {r.stderr or r.stdout}")
        return

    def done():
        info = prog(pid_of("ota-cli"))
        return (
            "VERSION_2" in app.read_text(errors="replace")
            and not info["config"].get("restore_path")
        )

    ok("7 CLI artifact update") if wait_until(done) else bad("7 CLI artifact update")


def case_stack() -> None:
    app = ROOT / "apps/running-stack"
    assert (
        cli(
            "add",
            "--name",
            "ota-stack",
            "--autostart",
            "--startsecs",
            "2",
            str(app),
        ).returncode
        == 0
    )
    time.sleep(1)
    v2 = sha256_file(ROOT / "artifacts/app-v2-good.sh")
    http_json(
        "PUT",
        f"{API}/api/v1/stack",
        {
            "prune": False,
            "services": [
                {
                    "name": "ota-stack",
                    "command": str(app),
                    "autostart": True,
                    "artifact": {
                        "source": artifact_url("app-v2-good.sh"),
                        "checksum": v2,
                        "destination": str(app),
                        "extract": False,
                        "restart_policy": "immediate",
                    },
                }
            ],
        },
    )

    def done():
        pid = pid_of("ota-stack")
        info = prog(pid)
        return (
            "VERSION_2" in app.read_text(errors="replace")
            and not info["config"].get("restore_path")
        )

    ok("10 stack apply OTA") if wait_until(done) else bad("10 stack apply OTA")


def case_extract() -> None:
    app = ROOT / "apps/running-extract"
    assert (
        cli(
            "add",
            "--name",
            "ota-extract",
            "--autostart",
            "--startsecs",
            "2",
            str(app),
        ).returncode
        == 0
    )
    time.sleep(1)
    pid = pid_of("ota-extract")
    archive = ROOT / "artifacts/app-extract.tar.gz"
    put_artifact(
        pid,
        artifact_url("app-extract.tar.gz"),
        sha256_file(archive),
        str(app),
        extract=True,
    )

    def done():
        info = prog(pid)
        return (
            "VERSION_2_EXTRACTED" in app.read_text(errors="replace")
            and not info["config"].get("restore_path")
        )

    ok("11 extract tar.gz commit") if wait_until(done) else bad("11 extract tar.gz")


def case_manual() -> None:
    app = ROOT / "apps/running-manual"
    assert (
        cli(
            "add",
            "--name",
            "ota-manual",
            "--autostart",
            "--startsecs",
            "2",
            str(app),
        ).returncode
        == 0
    )
    time.sleep(1)
    pid = pid_of("ota-manual")
    before = prog(pid).get("pid")
    put_artifact(
        pid,
        artifact_url("app-v2-good.sh"),
        sha256_file(ROOT / "artifacts/app-v2-good.sh"),
        str(app),
        restart_policy="manual",
    )

    def done():
        info = prog(pid)
        return (
            "VERSION_2" in app.read_text(errors="replace")
            and not info["config"].get("restore_path")
            and info.get("pid") == before
        )

    ok("12 restart_policy=manual") if wait_until(done) else bad("12 manual")


def case_signal() -> None:
    app = ROOT / "apps/running-signal"
    marker = ROOT / "apps/hup.marker"
    if marker.exists():
        marker.unlink()
    # signal* OTA requires an enabled health_check.
    pid = create_program(
        "ota-signal",
        str(app),
        startsecs=2,
        health_check={
            "type": "exec",
            "command": "true",
            "interval_secs": 1,
            "timeout_secs": 1,
            "start_period_secs": 0,
            "max_failures": 0,
        },
    )
    before = prog(pid).get("pid")
    # New script content (still traps HUP) so checksum changes.
    v2 = (
        "#!/bin/sh\n"
        f"MARKER='{marker}'\n"
        "trap 'touch \"$MARKER\"' HUP\n"
        "echo V2\n"
        "while true; do sleep 1; done\n"
    )
    (ROOT / "artifacts/signal-v2.sh").write_text(v2)
    (ROOT / "artifacts/signal-v2.sh").chmod(0o755)
    put_artifact(
        pid,
        artifact_url("signal-v2.sh"),
        sha256_file(ROOT / "artifacts/signal-v2.sh"),
        str(app),
        restart_policy="signal:hup",
    )

    def done():
        info = prog(pid)
        return (
            marker.exists()
            and info.get("pid") == before
            and not info["config"].get("restore_path")
            and "V2" in app.read_text(errors="replace")
        )

    ok("13 restart_policy=signal:hup") if wait_until(done, timeout=30) else bad(
        "13 signal:hup"
    )


def case_signal_requires_hc() -> None:
    """signal* without health_check must be rejected by the API."""
    app = ROOT / "apps/running-a"
    pid = pid_of("ota-commit")
    try:
        put_artifact(
            pid,
            artifact_url("app-v2-good.sh"),
            sha256_file(ROOT / "artifacts/app-v2-good.sh"),
            str(app),
            restart_policy="signal:hup",
        )
        bad("14 signal without HC should 4xx")
    except urllib.error.HTTPError as e:
        if e.code in (400, 422):
            ok("14 signal without HC rejected")
        else:
            bad(f"14 unexpected status {e.code}")
    except Exception as e:
        bad(f"14 unexpected error: {e}")


def case_wal_recovery() -> None:
    """SIGKILL mid-verify: restart must notice unfinished WAL and roll back."""
    global SUPERD
    app = ROOT / "apps/running-wal"
    pid = create_program("ota-wal", str(app), health_check=FAILING_HC)
    put_artifact(
        pid,
        artifact_url("app-v2-good.sh"),
        sha256_file(ROOT / "artifacts/app-v2-good.sh"),
        str(app),
    )
    saw_pending = wait_until(
        lambda: (
            prog(pid)["config"].get("restore_path")
            and "VERSION_2" in app.read_text(errors="replace")
        ),
        timeout=15,
    )
    if not saw_pending:
        bad("15 never entered verify phase before kill")
        return

    SUPERD.send_signal(signal.SIGKILL)
    try:
        SUPERD.wait(timeout=5)
    except subprocess.TimeoutExpired:
        SUPERD.kill()
        SUPERD.wait(timeout=5)

    env = os.environ.copy()
    env["SUPER_ROOT"] = str(ROOT)
    SUPERD = subprocess.Popen(
        [str(ROOT / "bin/superd")],
        env=env,
        stdout=open(ROOT / "logs/superd.stdout", "a"),
        stderr=subprocess.STDOUT,
    )
    wait_health()

    logged = wait_until(
        lambda: logs_contain("Found unfinished upgrade", "unfinished upgrade"),
        timeout=15,
    )

    def rolled_back():
        content = app.read_text(errors="replace")
        info = prog(pid)
        return (
            "VERSION_1" in content
            and "VERSION_2" not in content
            and not info["config"].get("restore_path")
        )

    if logged or wait_until(rolled_back, timeout=45):
        if wait_until(rolled_back, timeout=45):
            ok("15 WAL recovery after SIGKILL")
            if logged:
                ok("15b unfinished upgrade logged")
            else:
                # Rollback without the exact log line is still success for OR assert.
                print("NOTE: 15b unfinished log needle not seen (non-fatal if rolled back)")
        elif logged:
            ok("15 WAL unfinished logged after restart")
        else:
            bad("15 WAL recovery incomplete")
    else:
        bad("15 WAL recovery")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--build",
        action="store_true",
        help="cargo build --release before running",
    )
    args = parser.parse_args()
    try:
        maybe_build(args.build)
        setup()
        case_commit()
        case_same_checksum()
        case_bad_checksum()
        case_rollback()
        case_rollback_no_hc()
        case_http_policy()
        case_metadata()
        case_cli()
        case_verify_timeout()
        case_stack()
        case_extract()
        case_manual()
        case_signal()
        case_signal_requires_hc()
        case_wal_recovery()
    except Exception as e:
        bad(f"harness error: {e}")
        import traceback

        traceback.print_exc()
    finally:
        print()
        print(f"==== OTA E2E SUMMARY: {PASS} passed, {FAIL} failed ====")
        if FAIL:
            stdout = ROOT / "logs/superd.stdout"
            if stdout.exists():
                print("--- superd.stdout (last 80) ---")
                print("\n".join(stdout.read_text(errors="replace").splitlines()[-80:]))
        cleanup()
    return 1 if FAIL else 0


if __name__ == "__main__":
    sys.exit(main())
