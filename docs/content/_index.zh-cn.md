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
<span class="hero-badge-dot"></span>Rust · API 优先 · 单一二进制
</div>
<h1 class="hero-title">
自信地<br>运行你的服务
</h1>
<p class="hero-desc">轻量级进程管理器：自动重启、依赖启动顺序、健康检查与 OTA 更新 —— 用 TOML 或 REST 定义程序。</p>
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
<h2 class="hp-heading">一个二进制。<br>零运行时依赖。</h2>
<p class="hp-lead" style="margin-top: 0.75rem;">在生产中运行、监控与恢复服务所需的一切。</p>

<div class="feat-grid" style="margin-top: 2.5rem;">
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"/></svg></div>
<h3>生命周期钩子</h3>
<p>在 <code>pre_start</code>、<code>post_start</code>、<code>post_stop</code> 运行脚本 —— 准备环境、通知服务或清理资源。</p>
<a href="/docs/03-orchestration/lifecycle-hooks" class="feat-card-link">Hooks →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"/></svg></div>
<h3>事件钩子</h3>
<p>用本地脚本响应 <code>process_fatal</code> 等系统事件 —— Supervisor 风格，API 驱动。</p>
<a href="/docs/03-orchestration/events" class="feat-card-link">Events →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z"/></svg></div>
<h3>自动恢复</h3>
<p>兼容 Supervisor 的 <code>autorestart</code>、<code>exitcodes</code>、<code>startsecs</code> —— 迁移顺畅。</p>
<a href="/docs/04-production-scenarios/migrations/vs-supervisor" class="feat-card-link">对比 Supervisor →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"/></svg></div>
<h3>Cron 调度</h3>
<p>按 cron 表达式运行程序。无需外部调度器或 crontab。</p>
<a href="/docs/02-essentials/scheduled-tasks" class="feat-card-link">定时任务 →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9"/></svg></div>
<h3>HTTP 运维</h3>
<p>同一套 REST API 覆盖 CLI、脚本与远程控制。可通过插件启用 Bearer 鉴权。</p>
<a href="/docs/04-production-scenarios/observability/programmatic-control" class="feat-card-link">API →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"/></svg></div>
<h3>实时日志</h3>
<p>经 WebSocket 实时推送日志流。随时随地监控进程。</p>
<a href="/docs/02-essentials/logging" class="feat-card-link">日志 →</a>
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
<p class="cta-desc">下载二进制、定义栈，几分钟内即可跑起服务。</p>
<div class="hero-actions" style="justify-content: center;">
<a href="/docs/01-getting-started/quick-start/" class="hp-btn hp-btn--primary">开始构建 →</a>
<a href="/docs/01-getting-started/installation/" class="hp-btn hp-btn--outline">安装</a>
<a href="https://github.com/schiplat/super" class="hp-btn hp-btn--outline" target="_blank" rel="noopener">GitHub</a>
</div>
</div>
</section>

{{< contact-form >}}

</div>
