---
title: ""
description: "Project Super 是什么，以及如何安装与使用。"
width: full
toc: false
---

<div class="home-wrapper">

<section class="hp-section" style="padding-bottom: 2.5rem;">
<div class="hp-container hero">
<div>
<div class="hero-badge">
<span class="hero-badge-dot"></span>Rust · 跨平台 · API 优先 · 可审计
</div>
<h1 class="hero-title">
可查询、可变更、<br><em>可信任</em>的进程控制
</h1>
<p class="hero-desc">用声明式 TOML 或 REST 管理任意可执行文件 —— 健康检查、崩溃恢复与依赖启动顺序，尽在一个 Rust 二进制中。每个生命周期事件写入可查询账本，部署带回滚；需要治理、告警与 cgroup 限额时再加载许可插件。</p>
<p class="hero-desc" style="font-size: 0.875rem; color: #9c9890; margin-top: -0.5rem;">文档正文目前为英文；切换语言仅影响本站首页。</p>
<div class="hero-actions">
<a href="/docs/01-getting-started/quick-start/" class="hp-btn hp-btn--primary">快速开始 →</a>
<a href="https://github.com/schiplat/super" class="hp-btn hp-btn--outline" target="_blank" rel="noopener">
<svg width="16" height="16" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" viewBox="0 0 24 24"><path d="M9 19c-5 1.5-5-2.5-7-3m14 6v-3.87a3.37 3.37 0 00-.94-2.61c3.14-.35 6.44-1.54 6.44-7A5.44 5.44 0 0020 4.77 5.07 5.07 0 0019.91 1S18.73.65 16 2.48a13.38 13.38 0 00-7 0C6.27.65 5.09 1 5.09 1A5.07 5.07 0 005 4.77a5.44 5.44 0 00-1.5 3.78c0 5.42 3.3 6.61 6.44 7A3.37 3.37 0 009 18.13V22"></path></svg>
  GitHub
</a>
</div>
</div>

<div class="hero-card">
<img src="/images/stack_flow.gif" alt="Super 编排服务栈" loading="eager" />
<div class="hero-card-caption">服务栈按依赖顺序启动 —— 上游通过健康检查后，依赖项才会启动。</div>
</div>
</div>

<div class="hp-container">
<nav class="quick-nav" aria-label="文档分区">
<a href="/docs/01-getting-started/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M13 10V3L4 14h7v7l9-11h-7z"/></svg>快速开始</a>
<a href="/docs/02-essentials/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4"/></svg>基础</a>
<a href="/docs/03-orchestration/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg>编排</a>
<a href="/docs/04-production-scenarios/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01"/></svg>生产场景</a>
<a href="/docs/05-advanced-management/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"/></svg>高级管理</a>
<a href="/docs/06-internals/api-reference/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"/></svg>API 参考</a>
</nav>
</div>
</section>

<section class="hp-section hp-section--alt">
<div class="hp-container">
<div class="hp-label">核心能力</div>
<h2 class="hp-heading">进程管理器该有的能力，<br>以及更多。</h2>
<p class="hp-lead" style="margin-top: 0.75rem;">声明式配置、感知健康的编排与崩溃恢复 —— 外加可查询事件账本、失败安全交付，以及可从 CI 驱动的 API。</p>

<div class="feat-grid feat-grid--caps">
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"/></svg></div>
<h3>声明式栈</h3>
<p>用 TOML（或 JSON）定义程序。<code>super apply</code> 将运行态收敛到文件 —— 适合每次部署在 CI 中执行。</p>
<a href="/docs/04-production-scenarios/delivery/declarative-stack" class="feat-card-link">声明式栈 →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z"/></svg></div>
<h3>健康探测</h3>
<p>TCP、HTTP 与 exec 探测将程序标为 Healthy；退出处理遵循 <code>autorestart</code>、<code>exitcodes</code> 与指数退避。</p>
<a href="/docs/03-orchestration/health-checks" class="feat-card-link">健康检查 →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"/></svg></div>
<h3>有界重启</h3>
<p><code>retry_limit</code> 阻止无限抖动 —— 每次退避都会记入事件历史，直至状态变为 Fatal。</p>
<a href="/docs/03-orchestration/events/types" class="feat-card-link">事件类型 →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"/></svg></div>
<h3>任意二进制，一个守护进程</h3>
<p>Node.js、Python、Go、Rust、shell 脚本或编译产物 —— 能在 shell 里启动的，Super 都能接管。一个 <code>superd</code> 通过 <code>command</code> 统一管理。</p>
<a href="/docs/02-essentials/configuration" class="feat-card-link">配置 →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01"/></svg></div>
<h3>事件历史</h3>
<p>每个生命周期事件落入 SQLite 账本 —— 用 CLI 或 API 查询崩溃、cron 运行与恢复，而不必 grep 日志。</p>
<a href="/docs/03-orchestration/events/history" class="feat-card-link">事件历史 →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/></svg></div>
<h3>失败安全部署</h3>
<p>原子 OTA 与自动回滚，加上 <code>reload --wait</code> 与 <code>--wait-healthy</code>，配置变更仅在服务就绪后才算成功。</p>
<a href="/docs/04-production-scenarios/delivery/fail-safe-ota" class="feat-card-link">失败安全 OTA →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg></div>
<h3>依赖编排</h3>
<p>启动顺序、健康门控与错峰重启 —— 上游通过检查后，依赖项才会启动。</p>
<a href="/docs/03-orchestration/dependencies" class="feat-card-link">依赖 →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"/></svg></div>
<h3>事件反应</h3>
<p>在 <code>process_fatal</code> 等系统事件上运行本地脚本 —— 与许可版 webhook 通知区分开。</p>
<a href="/docs/03-orchestration/events/hooks" class="feat-card-link">事件钩子 →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"/></svg></div>
<h3>Cron 与并发</h3>
<p>Cron 调度配合重叠策略 —— skip、queue 或 kill —— 避免周期任务在负载下堆积。</p>
<a href="/docs/02-essentials/scheduled-tasks" class="feat-card-link">定时任务 →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9"/></svg></div>
<h3>API 控制面</h3>
<p>CLI 与脚本共用同一套 REST 端点 —— WebSocket 拉日志、Prometheus 拉指标，在 CI 中运行 <code>super apply</code>。</p>
<a href="/docs/04-production-scenarios/observability/programmatic-control" class="feat-card-link">可编程控制 →</a>
</div>
</div>
</div>
</section>

{{< home-premium >}}

<section class="hp-section hp-section--dark">
<div class="hp-container api-grid">
<div>
<div class="hp-label" style="color: var(--hp-accent);">开发者体验</div>
<h2 class="hp-heading">API 优先。<br>每次操作都是一次 HTTP 调用。</h2>
<p class="hp-lead" style="margin-top: 0.75rem;">CLI 与你使用相同的端点。用脚本和 CI 驱动运维，经 WebSocket 拉取日志，无需专用 SDK。</p>

<ul class="api-list">
<li>声明式 <code>super apply</code> —— 从 CI 幂等收敛栈</li>
<li>程序、栈与系统配置的 CRUD</li>
<li>实时 WebSocket 日志流</li>
<li>Prometheus 指标：<code>/metrics</code></li>
<li>开启 <code>enable_docs = true</code> 时提供 Swagger UI</li>
</ul>
<a href="/docs/06-internals/api-reference" class="hp-btn hp-btn--outline">完整 API 参考 →</a>
</div>

<div class="hp-terminal">
<div class="hp-terminal-bar">
<div class="hp-terminal-dots"><span></span><span></span><span></span></div>
<span>~/project/super-demo</span>
<span style="opacity:0">—</span>
</div>
<div class="hp-terminal-body">
<pre><code><span style="color:#5c5a55"># 三次调用创建并启动程序</span>
<span style="color:#6b6760">curl -X POST</span> http://127.0.0.1:9002/api/v1/programs \
  -H <span style="color:#c4b87a">"Content-Type: application/json"</span> \
  -d <span style="color:#c4b87a">'{
    "name": "api-server",
    "command": "./app",
    "depends_on": ["postgres", "redis"],
    "health_check": {
      "type": "http",
      "url": "http://127.0.0.1:8080/health"
    }
  }'</span>
<span style="color:#5c5a55"># 启动并流式拉取日志</span>
<span style="color:#6b6760">curl -X POST</span> http://127.0.0.1:9002/api/v1/programs/{id}/start
ws://127.0.0.1:9002/ws</code></pre>
</div>
</div>
</div>
</section>

<section class="cta">
<div class="hp-container">
<h2 class="cta-title">准备好了就开始。</h2>
<p class="cta-desc">下载二进制、定义栈，几分钟内即可获得可审计的进程控制。</p>
<div class="hero-actions" style="justify-content: center;">
<a href="/docs/01-getting-started/quick-start/" class="hp-btn hp-btn--primary">开始构建 →</a>
<a href="/docs/01-getting-started/installation/" class="hp-btn hp-btn--outline">安装</a>
<a href="https://github.com/schiplat/super" class="hp-btn hp-btn--outline" target="_blank" rel="noopener">GitHub</a>
</div>
</div>
</section>

{{< contact-form >}}

</div>
