# Super 同行基准测试方案

> **文档版本：** 6.0（2026-08-29）  
> **状态：** 正式拓扑 = **4 台同规格主机并行 · 一机一臂 · 每臂 3 轮**；方法学定案，Lab 待跑  
> **执行入口：** `benchmark/benchmark_all.sh`（每台机设 `BENCH_ARM`）+ `scripts/`  
> **公开 docs：** [vs PM2](../docs/content/docs/04-production-scenarios/migrations/vs-pm2.md) **不得再写死 RSS 数字**；cgroup 强制不是 OSS 内置

---

## 0. 产品初衷（本方案尺子）

Super 的定位：**轻量、API-first 的原生进程守护**，替代 Supervisor / PM2 一类工具——单二进制、管好生命周期，边端与普通规格机器也能跑，配置与 HTTP/CLI 灵活够用。

本 Lab **要对照 / 要证**的只有：

| # | 主张 | 怎么证 |
|---|------|--------|
| 1 | **Daemon 不肥、不泄漏** | idle / soak 下 daemon-set RSS 有界、不持续爬升（与 supervisord / pm2 同机种对照） |
| 2 | **场景够用** | crash 自启、日志吞吐、多程序、冷启动 / status / reload 能跑通且可对比 |
| 3 | **灵活便捷（事实，不打分）** | Day-0 ONB 矩阵；不合成「更好用」分 |

**明确不是：** 半机人造泄漏比拼；为大内存门控上大机；综合总分；「谁更省内存」营销话术。

**Lab 规格：** 四台**同规格**普通业务机（建议 **4～8 GiB**）。负载参数适配该规格，**禁止**倒逼 20GiB+。

---

## 0a. 定案摘要（已确认）

| 决策项 | 定案 |
|--------|------|
| 版本 | 各自**最新稳定版** dated snapshot；super = 最新 **release tag** |
| **四组齐全** | `super-oss` / `super-pro` / `supervisord` / `pm2`，均为独立测量对象 |
| **正式拓扑** | **4 台同规格主机并行 · 一机一臂 · 每臂 3 轮** |
| **禁止（对外结论）** | 同一台设备交错跑多家后用作公开数字 |
| 同机交错 | 仅 `MODE=colocated` 烟测，禁止进入发布物 |
| 轮数 | Phase A = 每臂 1 缩短轮；Phase B = 每臂 **3** 轮 |
| 主指标 | 跑数前锁死（`primary_metrics.json`）；其余 secondary |
| RSS | 主表 = daemon 集合（PM2 含 God+agent）；辅表 = tree RSS（RSS 非 PSS） |
| 无泄漏主证 | STB-3；RES-1 为 idle 基线 |
| STB-2 | 默认 **`N=10` × `cap=64MiB`**；daemon 存活 + tree RSS 有界 |
| PRO | 正式测量但**单列**：报告与 OSS 并排，标注 licensed plugins；不把 PRO 数字说成 OSS 能力 |
| 安全 | 定性矩阵 + 探针；产品默认 vs bench 配置分表 |
| ONB | 事实矩阵，不打分 |
| vs-pm2.md | 无写死 MB、无 OSS 原生 cgroup |

---

## 1. 正式拓扑：一机一臂 · 并行

```
# 四台同规格云主机（同镜像、同 vCPU/RAM/磁盘、尽量同可用区）
host-oss: BENCH_ARM=super-oss     PHASE=B ./benchmark_all.sh
host-pro: BENCH_ARM=super-pro     PHASE=B ./benchmark_all.sh   # + SUPER_BENCH_*
host-sv:  BENCH_ARM=supervisord   PHASE=B ./benchmark_all.sh
host-pm2: BENCH_ARM=pm2           PHASE=B ./benchmark_all.sh

# 汇总：四份 results/ 拷到分析机
python3 analysis/summarize.py --merge oss pro sv pm2 --out report/
```

**单机内循环（只跑本机 arm）：**

```
for round in 1..3:
  for scenario in RES/STB…:
    generate configs (本 arm only; 四机同 N / 同 seed / 同 cap)
    runner
    plot
  MGT + SEC
  if arm==super-pro and cgroup: STB-2-PRO
  round cooldown
```

**跨机公平契约：**

1. 同云厂商、同规格（vCPU / RAM / 磁盘）、同 OS 镜像、同内核  
2. 同 `PHASE` / `N` / `CAP_MB` / duration；同 payload 二进制（同 sha）  
3. crash seed 跨臂一致（generator 同一 `seed_base`）  
4. 各机 manifest 记录版本；汇总时逐项 diff  
5. 禁止在一台机上跑两个 arm（连「顺便对比」也不行）  

---

## 1b. 烟测拓扑：同机交错（非发布）

`MODE=colocated PHASE=A`：单机 order-4 Latin square 四臂切换 —— **只验证脚本能跑通**。结果目录写入 `colocated_smoke=true`，检查表禁止用于对外数字。

---

## 2. 切换门控

- **一机一臂正式模式：** 场景之间做轻量 quiet（可选）；**没有**「换竞品」四家切换。
- **同机烟测：** 保留 `switch_gate.sh`（teardown → drop_caches → quiet → load/mem 恢复）；失败作废轮。

---

## 3. Payload 与公平性（P0）

| 语义 | payloads `--mode` | 行为 |
|------|-------------------|------|
| idle | `idle` | 睡眠 |
| crash | `crash-random --seed {base+i}` | 跨臂同一崩溃时间线 |
| log | `log-throughput` | 持续写到 SIGTERM |
| mem-leak | `mem-eat --cap-mb 64` | 涨到 cap 后 hold |

- Super：`$SUPER_ROOT/conf/conf.d/*.json` include，不是 `[[program]]`
- `startsecs=0`，retry=3，`autorestart=true`；四臂关 rotation
- PM2：`PM2_HOME` 隔离；superd：只杀跟踪 PID
- supervisor inet `:9001` = bench 控制面，非产品默认

### 默认规模（4～8 GiB）

| 场景 | N | 备注 |
|------|---|------|
| RES-1 / STB-3 / MGT | 50 | idle |
| STB-1 | 30 | crash |
| RES-2 / STB-4 | 10 | log |
| STB-2 / STB-2-PRO | 10 | mem；`CAP_MB=64` |

**N 梯度（RES-1 可扩展性）：** 每臂先跑 `N ∈ {50, 200, 500}` idle 各 60s，采集 daemon-set RSS vs N 折线，回答「随管理进程数增加，daemon 开销怎么涨」。Phase A / B 都跑。

**RAM 门控：** `N × cap_mb < MemAvailable/2`（`scripts/ram_gate.sh`）。默认 `10×64` 小规格即可；不够则降 cap/N 并记 manifest，禁止为过门控上大机。

---

## 4. 预注册主指标

| ID | 主指标 | 对应主张 |
|----|--------|----------|
| RES-1 | daemon-set RSS 中位数 | 轻量控制面 |
| RES-2 | child lines/s vs bare | 日志场景 |
| STB-1 | daemon 存活 + restart_sum | crash 自愈 |
| STB-2 | daemon 存活 + tree RSS 有界 | 轻量压力有界 |
| STB-3 | daemon RSS 漂移 | **无泄漏主证** |
| STB-4 | daemon 存活 + 吞吐 | 持续日志 |
| MGT-1/2/3 | cold poll / status p95 / reload | 可管理 |
| STB-2-PRO | cgroup 遏制（仅 pro 机） | PRO 事实 |

CPU%：sysinfo 0.30，单核相对；两次 refresh；丢弃首样本；采样 500ms；单调钟。
ONB 不进 `primary_metrics.json`。

---

## 5. OSS / PRO

- **super-oss：** 无 plugins / 无 license
- **super-pro：** 同 superd + security + isolation + auth_secret + 许可证（`SUPER_BENCH_*`）
- 图：四组同轴；OSS 实线、PRO 虚线同色系；**图注写清 PRO = licensed plugins**
- 禁止：PRO 数字 = OSS 能力；「更安全」总结论；综合总分
- 复现：OSS / supervisor / pm2 可公开复现；PRO 需许可证

---

## 6. 安全分表

**产品默认（定性）：** OSS loopback fail-closed；supervisor 默认无 inet；pm2 本地 socket。

**Bench 配置（实证 SEC-1…4）：** 四组均开 loopback 控制面（super `:9002`，supervisor inet `:9001`）。PRO 仍 loopback + 认证；不要把 PRO 改成 0.0.0.0 再比暴露面。

SEC-2：OSS 无认证预期可达；PRO 无凭据预期 401/403。
SEC-3：`lab_all_root=true` 时记录 uid，不比较特权模型。

## 6a. ONB — Day-0 上手事实（定性，不打分）

事实矩阵：运行时 / 最小文件 / 控制面（默认 vs bench）/ 日志 / 额外步骤。只陈述路径、版本、文件列表；禁止易上手分、TFP 秒表。

---

## 7. 时间预算

- 单臂 Phase B：约 3 轮 ×（6 场景 + MGT/SEC）≈ **5–9h**
- 4 台并行墙钟 ≈ 单臂时间（并行无叠加）
- Phase A（1 轮/臂）：约 1–2h

---

## 8. 检查表

- [ ] 四组各有隔离、干净的正式结果；对外数字 **非** colocated 产物
- [ ] 四机 manifest 可对齐（规格 / 镜像 / 版本 / N / cap / payload sha）
- [ ] 主张对齐初衷；STB-2 ≤ 10×64；无泄漏看 STB-3
- [ ] 主指标预注册；安全分表；ONB 不打分
- [ ] vs-pm2 无写死 MB、无 OSS 原生 cgroup
- [ ] PRO 单列，未写成 OSS 能力
- [ ] 图含 IQR；报告含 Limitations + 版本 + 机器规格 + 原始数据包

---

## 9. Out of scope

K8s / 多机编排产品对比；PM2 cluster；ui/notify；OSS cgroup 强制；综合总分；易上手分；把同机交错结果当公开发布；把 600s soak 写成「长期运行」。

---

## 附录 — 命令

```bash
cd benchmark && cargo build --release

# 四台机分别：
BENCH_ARM=super-oss   PHASE=B ./benchmark_all.sh
BENCH_ARM=super-pro   PHASE=B ./benchmark_all.sh   # + SUPER_BENCH_*
BENCH_ARM=supervisord PHASE=B ./benchmark_all.sh
BENCH_ARM=pm2         PHASE=B ./benchmark_all.sh

# 仅烟测（禁止用于对外比分）：
MODE=colocated PHASE=A ./benchmark_all.sh
```

脚本：`scripts/{switch_gate,collect_manifest,ram_gate,cgroup_gate,sec_probes,mgt_run,bare_log_baseline,onboard_facts}.sh`
