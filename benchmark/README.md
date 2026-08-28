# Super Benchmark Suite

Same-workload comparison of **super-oss**, **super-pro** (licensed plugins), **supervisord**, and **pm2**.
Methodology: [`BENCHMARK_PLAN.md`](./BENCHMARK_PLAN.md). Public docs do **not** quote a fixed RSS range; measure here.

## What this suite actually runs

| ID | Payload | N (default) | Primary metric |
|----|---------|-------------|----------------|
| RES-1 | idle | 100 | daemon-set RSS median |
| RES-2 | log-throughput (sustained) | 20 | child lines/s vs bare `/dev/null` |
| STB-1 | crash-random (per-program seed) | 50 | daemon alive + restart sum |
| STB-2 | mem-eat **capped** (default 512 MiB/proc) | 20 | daemon alive, tree RSS bounded |
| STB-3 | idle soak | 100 | RSS drift |
| STB-4 | sustained log | 20 | daemon alive + throughput |
| MGT-1/2/3 | idle + official CLIs | 100 | cold poll / status p95 / reload |
| SEC-1…4 | probes | — | listen / unauth HTTP / uid / file mode |
| ONB | facts script | — | deps / file counts / control plane (**no score**) |
| STB-2-PRO | mem-eat + cgroup `memory_limit` | 20 | containment (**not** a cross-tool score) |

Loop is **scenario-outer**: for each scenario, run the four arms in a **Latin square** order. A switch gate that fails **aborts the round** (does not start the next arm on a dirty VM).

Phase B uses **4 rounds** (Latin square order 4). Not 5.

## Prerequisites

- Rust 1.85+, Python 3.10+ (`pip install -r analysis/requirements.txt`)
- `superd` / `super` on `PATH` (or `SUPERD_BIN=...`)
- `supervisord` + `supervisorctl` (PyPI **or** distro — record which in the manifest)
- `pm2` + Node (record exact versions)
- Linux recommended (cgroup gate for STB-2-PRO)
- Root is OK for the lab VM; **SEC-3 then cannot claim uid isolation** (all arms run as the same user)

## Build

```bash
cd benchmark
cargo build --release
```

## Phase A (method smoke, 1 shortened round)

OSS + supervisor + pm2 only:

```bash
SKIP_PRO=1 PHASE=A ./benchmark_all.sh
```

With licensed Super (plugins + license, **no keys in git**):

```bash
export SUPER_BENCH_AUTH_SECRET='…'
export SUPER_BENCH_LICENSE_FILE=/path/to/license.jwt   # or SUPER_BENCH_LICENSE_KEY
export SUPER_BENCH_PLUGINS_DIR=/path/to/plugins         # security.* and isolation.* (no lib prefix)
PHASE=A ./benchmark_all.sh
```

## Phase B (publication)

```bash
PHASE=B ./benchmark_all.sh
```

Overnight (~10h class with gates). Results under `results/B_<utc>/`: CSV, `report.png` (OSS/PRO same axes), `summary.json`, `manifest.json`, `invalid_rounds.txt` if a gate aborted a round.

## RAM / cgroup gates

```bash
./scripts/ram_gate.sh 20 512    # N * cap_mb must be < half MemAvailable
./scripts/cgroup_gate.sh        # required for STB-2-PRO
./scripts/onboard_facts.sh /tmp/onboard.json --generated ./configs   # Day-0 facts, no score
```

STB-2 touches pages up to `--cap-mb` then **holds**. It does not grow forever.

## Layout

```
payloads/     # workload binary
generator/    # 4-arm configs; Super programs via conf/conf.d/*.json include
runner/       # SUPER_ROOT + --foreground; daemon-set RSS; tree RSS; drop first CPU sample
scripts/      # switch_gate, manifest, sec_probes, mgt_run, ram_gate, cgroup_gate, onboard_facts
analysis/     # plot.py (dual Super series) + summarize.py (IQR)
benchmark_all.sh
```

PM2 uses a private `PM2_HOME` per instance so `pm2 kill` does not touch a user daemon.
Superd is stopped by **tracked PID**, not `pkill superd`.
