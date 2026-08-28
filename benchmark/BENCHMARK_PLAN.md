# Super 同行基准测试方案

> **文档版本：** 4.3（2026-08-28）  
> **状态：** 方法学已按审阅 P0 锁定；**Lab 尚未完成 Phase A**  
> **执行入口：** `benchmark/benchmark_all.sh` + `scripts/`  
> **公开 docs：** [vs PM2](../docs/content/docs/04-production-scenarios/migrations/vs-pm2.md) **不得再写死 RSS 数字**；cgroup 强制不是 OSS 内置

---

## 0. 定案摘要

| 决策项 | 定案 |
|--------|------|
| 版本 | 各自**最新稳定版** dated snapshot；super = 最新 **release tag**（非 master） |
| Arms | `super-oss` / `super-pro` / `supervisord` / `pm2` |
| 循环 | **场景外层** + 场景内 Latin square 换 arm |
| 轮数 | Phase A = **1** 缩短轮；Phase B = **4** 轮（order-4 Latin square，**不用 5**） |
| 门控失败 | **整轮作废**（写入 `invalid_rounds.txt`），不在脏机上开下一 arm |
| 主指标 | 跑数前锁死（`primary_metrics.json`）；其余 secondary |
| RSS | 主表 = daemon 集合；辅表 = tree RSS（**RSS 非 PSS**） |
| PRO | 同场景同 N；图内同图并排；结论单列；**不把 PRO 当 OSS 能力** |
| 安全 | 定性矩阵 + 探针；**默认产品姿态**与 **bench 控制面配置**分表 |
| SEC-3 | 本 lab 若全是 root，只记录事实，**不声称测了特权分离** |
| ONB | Day-0 **定性事实矩阵**（依赖 / 文件数 / 控制面）；**不打分**；与 MGT 分开 |
| vs-pm2.md | 无 MB 数字、无「OSS 原生 cgroup」；Phase B 后如需数字必须引用本 lab snapshot |

---

## 1. 循环顺序（P0，已锁死）

```
for round in 1..R:                     # R=1 (A) or R=4 (B)
  latin = cyclic_square[round]         # 见 scripts/lib.sh
  for scenario in RES/STB…:            # 场景外层
    generate configs ONCE              # 四 arm 共用同一 payload 路径与 crash seed
    for arm in latin:
      switch_gate or ABORT ROUND
      runner
    plot (OSS/PRO 同轴)
  MGT + SEC (仍走门控)
  STB-2-PRO if cgroup gate (PRO only, not a score)
  round cooldown 60s
```

**禁止 arm 外层**（一家跑完全部场景再换家）：mem-eat 会污染同一 arm 的 soak，跨工具也不是同一时刻的机器状态。

Cyclic Latin square（A=super-oss, B=super-pro, C=supervisord, D=pm2）：

| Round | 顺序 |
|-------|------|
| 1 | A B C D |
| 2 | B C D A |
| 3 | C D A B |
| 4 | D A B C |

---

## 2. 切换门控（硬门槛 + 失败即作废轮）

实现：`scripts/switch_gate.sh`。

1. teardown 后 `drop_caches`（root 且 `SUPER_BENCH_DROP_CACHES=1`，默认开，记入 manifest）
2. 静默 `SUPER_BENCH_QUIET_SEC`（默认 30）
3. `loadavg_1 ≤ baseline + 0.5` 且 `MemAvailable ≥ baseline × 0.85`
4. 最多等 180s；失败 **exit 2 → abort round**，不 skip 到下一 arm

基线在**每轮开始**记录。间隙 loadavg/mem 追加 `switch_gate.jsonl`。

---

## 3. Payload 与公平性（P0）

| 语义 | payloads `--mode` | 行为 |
|------|-------------------|------|
| idle | `idle` | 睡眠 |
| crash | `crash-random --seed {base+i}` | 程序 i 跨 arm 同一崩溃时间线 |
| log | `log-throughput` | **持续写到 SIGTERM**；stderr 周期 `BENCH_RESULT:<lps>` |
| mem-leak | `mem-eat --cap-mb 512` | 触碰页面涨到 cap 后 **hold**，禁止无界增长 |

- Super 程序来自 `$SUPER_ROOT/conf/conf.d/*.json` include，**不是** `super.toml` 里的 `[[program]]`（会被忽略）。
- `startsecs=0`，`retry_limit`/`startretries`/`max_restarts` = 3，`autorestart=true`。
- 日志：四 arm 关闭 rotation（`max_backups=0` / `maxbytes=0`）。
- PM2：`PM2_HOME` 隔离；superd：**只杀跟踪 PID**，禁止 `pkill superd`。
- supervisor inet `:9001` 是 **bench 控制面**，不是产品默认。SEC 矩阵分「产品默认」与「bench 配置」两列。

**RAM 门控：** `N × cap_mb < MemAvailable/2`（`scripts/ram_gate.sh`）。20×512MiB 需要 ≥20GiB 可用内存的一半以上；VM 不够就降 N 或 cap，禁止硬跑。

**cgroup 门控：** `scripts/cgroup_gate.sh`。失败则 STB-2-PRO 标记 *not applicable*，不当产品失败。

---

## 4. 预注册主指标

| ID | 主指标 |
|----|--------|
| RES-1 | daemon-set RSS 中位数 |
| RES-2 | 子进程 lines/s vs 裸跑 `/dev/null`（`scripts/bare_log_baseline.sh`） |
| STB-1 | daemon 存活 + restart_sum（super `/metrics`，supervisorctl status，pm2 jlist） |
| STB-2 | daemon 存活 + tree RSS 有界 |
| STB-3 | daemon RSS 漂移（medium soak；Phase B 600s，**不称 long-running**） |
| STB-4 | daemon 存活 + 吞吐 |
| MGT-1 | 冷启动 poll → N running 的 wall ms |
| MGT-2 | status ×20 的 p95 ms |
| MGT-3 | reload/reread+update/reloadLogs 的 ms + daemon PID 是否不变 |
| STB-2-PRO | cgroup 遏制事实（**不参与跨工具对比**） |

CPU% = sysinfo 0.30，相对**单核**；两次 refresh；**丢弃首样本**。采样 500ms。时钟 = monotonic。

ONB **不是**主指标、不进 `primary_metrics.json`。采集脚本：`scripts/onboard_facts.sh`（见 §6a）。

---

## 5. OSS / PRO

- OSS：无 `plugins/`、无 `[license]`。
- PRO：同一 `superd` + `security` + `isolation` + `auth_secret` + 许可证（env：`SUPER_BENCH_*`，公开物不含密钥）。
- 图：`analysis/plot.py` 同轴，`super-oss` 实线 / `super-pro` 虚线，同色系；柱状图 PRO 用 hatch。
- 声称：禁止「PRO 数字 = OSS 能力」；禁止「更安全」总结论；禁止综合总分。
- 第三方：**OSS 轨可复现；PRO 轨需 vendor 许可证，PRO 数字不是可独立复核的科学主张。**

---

## 6. 安全分表

**产品默认（定性）：** OSS loopback fail-closed；supervisor 默认无 inet；pm2 本地 socket。

**Bench 配置（实证 SEC-1…4）：** 四 arm 均开 loopback 控制面（super `:9002`，supervisor inet `:9001`）。PRO 仍绑 loopback + 认证；**不要**把 PRO 改成 0.0.0.0 再和 OSS 比暴露面。

SEC-2：OSS 无认证预期可达；PRO 无凭据预期 401/403。

SEC-3：`lab_all_root=true` 时降级为记录 uid，不比较特权模型。

---

## 6a. ONB — Day-0 上手事实（定性，不打分）

**要回答的问题：** 让 **1 个托管进程** 出现在配置里、并能用**官方控制接口**查到它，各 arm 需要哪些运行时、几个文件、控制面长什么样。

**不回答：** 谁更好用、谁更简单、读文档要多久。禁止合成「易上手分」。秒表 TFP（ONB-1）本版 **不做**。

**与 MGT 的边界：** ONB = 尚未（或刚）装好时的依赖与配置面；MGT = daemon 已在管 N 个进程时的操作延迟。

实现：`scripts/onboard_facts.sh OUT.json [--generated DIR]`。编排在 Phase 开头对 **N=1 idle** 的 generator 输出跑一次，写入 `onboard_facts.json`。

### 矩阵（报告用表；单元格填脚本事实 + 下表产品默认）

| 维度 | super-oss | super-pro | supervisord | pm2 |
|------|-----------|-----------|-------------|-----|
| 运行时 | 静态 `superd`（无 Python/Node） | 同 OSS + 插件 `.so`/`.dylib` | Python + `supervisord` | Node + `pm2` |
| 最小文件（generator N=1） | `conf/super.toml` + `conf/conf.d/*.json` | 同 OSS，另需 `plugins/` 与许可证字段 | `supervisord.conf` | `ecosystem.config.js` |
| 控制面（**产品默认**） | HTTP `127.0.0.1:9002`，无 API 认证 | 认证（security 插件 + `auth_secret`） | **无 inet**，直到显式配置 | `PM2_HOME` 本地 socket |
| 控制面（**bench 配置**） | 同上 loopback | 仍 loopback + 认证 | 打开 `127.0.0.1:9001` 供 supervisorctl | 隔离 `PM2_HOME` |
| 日志 | OSS 内建 rotation（bench 关 backups） | 同 OSS | `logfile_maxbytes`（bench=0） | 文件；rotation 常需 `pm2-logrotate`（本套件不装） |
| 额外步骤 | — | 插件目录、许可证、`auth_secret`；缺则硬失败 | 安装 Python 发行 | 安装 Node |
| 已核实踩坑（文档，非秒表） | 程序不在 `[[program]]`；必须 `SUPER_ROOT/conf/super.toml` | 同 OSS + licensed 硬失败 | inet 无密码则可连 | `pm2 kill` 作用域=当前 `PM2_HOME`；cluster 仅 Node（不测） |

**声称边界：** 只陈述上表与 `onboard_facts.json` 中的路径/版本/文件列表。PRO 额外步骤单列，不与 OSS 比「谁更简单」。禁止「配置更便利」总结论。

---

## 7. 时间预算（场景外层，4 arm，4 轮）

门控约 6 场景 × 4 arm + MGT/SEC ≈ 30 次/轮 × ~40s ≈ 20 min 门控。  
有效负载 Phase B 单轮约 90–120 min。4 轮 + 轮间 60s ≈ **8–12h**。Phase A 缩短 duration，约 1–2h。

---

## 8. 检查表（相对 4.1 的增量）

- [ ] 场景外层，而非 arm 外层
- [ ] 4 轮而非 5；Latin square 平衡
- [ ] 门控失败 → 作废轮，无 skip
- [ ] log payload 持续写；mem-eat 有 cap 且触碰页面
- [ ] RAM 门控、cgroup 门控
- [ ] 主指标预注册
- [ ] bench vs 产品默认 安全分表
- [ ] SEC-3 root lab 不声称 uid 隔离
- [ ] vs-pm2 无写死 MB、无 OSS 原生 cgroup
- [ ] CSV：`time_ms,cpu_usage,memory_mb,total_tree_rss_mb,managed_process_count`
- [ ] 图含 IQR / 双 Super 系列
- [ ] ONB 事实已采集（`onboard_facts.json`）；报告无易上手分、无 TFP 秒表
- [ ] `CpuBurn` 本套件不用；CPU quota 演示未做则保持 out of scope

---

## 9. Out of scope

K8s / systemd 嵌套 / 多机；PM2 cluster；ui/notify；OSS cgroup 强制；综合总分；易上手/迁移分数；ONB-1 秒表；x86 外推；把 600s soak 写成「长期运行」。

---

## 附录 — 命令

```bash
cd benchmark && cargo build --release
SKIP_PRO=1 PHASE=A ./benchmark_all.sh
# PRO:
# export SUPER_BENCH_AUTH_SECRET SUPER_BENCH_LICENSE_FILE SUPER_BENCH_PLUGINS_DIR
PHASE=B ./benchmark_all.sh
```

脚本：`scripts/{switch_gate,collect_manifest,ram_gate,cgroup_gate,sec_probes,mgt_run,bare_log_baseline,onboard_facts}.sh`
