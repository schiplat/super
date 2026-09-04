# Super Benchmark Suite

Same-workload comparison of **super-oss**, **super-pro** (licensed plugins), **supervisord**, and **pm2** — four arms on **four like-for-like hosts, one arm per host, 3 rounds each**.
Methodology: [`BENCHMARK_PLAN.md`](./BENCHMARK_PLAN.md) **v6**. Ordinary Lab hosts (~**4–8 GiB**). Public docs do **not** quote a fixed RSS range.

## What we are proving

1. **Daemon stays lean / does not leak** — mainly **STB-3** (+ RES-1)
2. **Real supervisor scenarios work** — crash, logs, multi-program, cold start / status / reload
3. **Day-0 facts (no score)** — ONB matrix

## What this suite actually runs

| ID | Payload | N (default) | Primary metric |
|----|---------|-------------|----------------|
| RES-1 | idle | 50 | daemon-set RSS median |
| RES-2 | log-throughput | 10 | child lines/s vs bare `/dev/null` |
| STB-1 | crash-random | 30 | daemon alive + restart sum |
| STB-2 | mem-eat capped (**64 MiB/proc**) | 10 | daemon alive, tree RSS bounded |
| STB-3 | idle soak | 50 | **daemon RSS drift (no-leak primary)** |
| STB-4 | sustained log | 10 | daemon alive + throughput |
| MGT-1/2/3 | idle + official CLIs | 50 | cold poll / status p95 / reload |
| SEC-1…4 | probes | — | listen / unauth HTTP / uid / file mode |
| ONB | facts script | — | deps / files / control plane (**no score**) |
| STB-2-PRO | super-pro host + cgroup | 10 | containment (**not** a peer score) |

## Fairness: one host per arm

Four arms on the **same host**, interleaved, lets prior runs pollute page cache / memory / CPU for the next — not fair. Formal numbers come from **four like-for-like hosts, one arm each**, same OS, same spec, same `N`/`CAP_MB`/crash seeds/payload sha, and a manifest diff.

`MODE=colocated` on a single host is a **smoke test only** — its results are marked `publishable:false` and must never go into the public report.

## Prerequisites

- Rust 1.85+, Python 3.10+ (`pip install -r analysis/requirements.txt`)
- `superd` / `super` on `PATH` (or `SUPERD_BIN=...`)
- `supervisord` + `supervisorctl`; `pm2` + Node (record versions)
- Linux recommended for STB-2-PRO; **4–8 GiB** RAM enough for defaults

## Build / run

```bash
cd tools/benchmark && cargo build --release

# On each of 4 like-for-like hosts, one arm:
BENCH_ARM=super-oss   PHASE=B ./benchmark_all.sh
BENCH_ARM=super-pro   PHASE=B ./benchmark_all.sh   # + SUPER_BENCH_* license env
BENCH_ARM=supervisord PHASE=B ./benchmark_all.sh
BENCH_ARM=pm2         PHASE=B ./benchmark_all.sh

# Smoke only (never publish):
MODE=colocated PHASE=A ./benchmark_all.sh
```

Phase A = 1 shortened round; Phase B = **3** rounds per arm.

```bash
./scripts/ram_gate.sh 10 64
```

PM2 uses a private `PM2_HOME`. Superd is stopped by **tracked PID**, not `pkill superd`.
