---
title: ""
description: "What Project Super is, and how to install and operate it."
width: full
toc: false
---

<div class="home-wrapper">

<!-- =============================================
  HERO
  ============================================= -->
<section class="hp-section" style="padding-bottom: 2.5rem;">
<div class="hp-container hero">
<div>
<div class="hero-badge">
<span class="hero-badge-dot"></span>Rust · Cross-platform · API-First · Auditable
</div>
<h1 class="hero-title">
  Process control you can<br><em>query, change,</em> and trust
</h1>
<p class="hero-desc">Manage any executable from declarative TOML or REST — health checks, crash recovery, and dependency order in one Rust binary. Record every event in a queryable ledger, deploy with rollback, and add licensed plugins for governance, alerting, and cgroup limits.</p>
<div class="hero-actions">
<a href="/docs/01-getting-started/quick-start/" class="hp-btn hp-btn--primary">Get Started →</a>
<a href="https://github.com/schiplat/super" class="hp-btn hp-btn--outline" target="_blank" rel="noopener">
<svg width="16" height="16" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" viewBox="0 0 24 24"><path d="M9 19c-5 1.5-5-2.5-7-3m14 6v-3.87a3.37 3.37 0 00-.94-2.61c3.14-.35 6.44-1.54 6.44-7A5.44 5.44 0 0020 4.77 5.07 5.07 0 0019.91 1S18.73.65 16 2.48a13.38 13.38 0 00-7 0C6.27.65 5.09 1 5.09 1A5.07 5.07 0 005 4.77a5.44 5.44 0 00-1.5 3.78c0 5.42 3.3 6.61 6.44 7A3.37 3.37 0 009 18.13V22"></path></svg>
  GitHub
</a>
</div>
</div>

<div class="hero-card">
<img src="/images/stack_flow.gif" alt="Super orchestrating a service stack" loading="eager" />
<div class="hero-card-caption">Service stack booting in dependency order — upstreams pass health checks before dependents start.</div>
</div>
</div>

<div class="hp-container">
<nav class="quick-nav" aria-label="Documentation sections">
<a href="/docs/01-getting-started/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M13 10V3L4 14h7v7l9-11h-7z"/></svg>Getting Started</a>
<a href="/docs/02-essentials/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4"/></svg>Essentials</a>
<a href="/docs/03-orchestration/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg>Orchestration</a>
<a href="/docs/04-production-scenarios/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01"/></svg>Production</a>
<a href="/docs/05-advanced-management/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"/></svg>Advanced</a>
<a href="/docs/06-internals/api-reference/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"/></svg>API Reference</a>
</nav>
</div>
</section>

<!-- =============================================
  CAPABILITIES
  ============================================= -->
<section class="hp-section hp-section--alt">
<div class="hp-container">
<div class="hp-label">Capabilities</div>
<h2 class="hp-heading">Everything a process manager<br>should do — and more.</h2>
<p class="hp-lead" style="margin-top: 0.75rem;">Declarative config, health-aware orchestration, and crash recovery — plus a queryable event ledger, fail-safe delivery, and an API you can drive from CI.</p>

<div class="feat-grid feat-grid--caps">
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"/></svg></div>
<h3>Declarative Stacks</h3>
<p>Define programs in TOML (or JSON). <code>super apply</code> converges live state to the file — safe to run from CI on every deploy.</p>
<a href="/docs/04-production-scenarios/delivery/declarative-stack" class="feat-card-link">Declarative stacks →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z"/></svg></div>
<h3>Health &amp; Auto-Recovery</h3>
<p>TCP, HTTP, and exec probes. <code>autorestart</code>, <code>exitcodes</code>, and exponential backoff recover crashed workers; probes also gate dependency startup.</p>
<a href="/docs/03-orchestration/health-checks" class="feat-card-link">Health checks →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"/></svg></div>
<h3>Crash-Loop Protection</h3>
<p><code>retry_limit</code> stops runaway restart storms; events land in the ledger so you can see backoff before a process gives up.</p>
<a href="/docs/03-orchestration/events/types" class="feat-card-link">Event types →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"/></svg></div>
<h3>Any Binary, One Daemon</h3>
<p>Node, Python, Go, Rust, shell — if it runs in a terminal, Super can supervise it. Single static binary on Linux and macOS; no Python or JVM required.</p>
<a href="/docs/02-essentials/configuration" class="feat-card-link">Configuration →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01"/></svg></div>
<h3>Event History</h3>
<p>Every lifecycle event lands in a SQLite ledger — query crashes, cron runs, and recoveries via CLI or API instead of grepping logs.</p>
<a href="/docs/03-orchestration/events/history" class="feat-card-link">Event history →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/></svg></div>
<h3>Fail-Safe Deployments</h3>
<p>Atomic OTA with automatic rollback, plus <code>reload --wait</code> and <code>--wait-healthy</code> so config changes finish only when services are ready.</p>
<a href="/docs/04-production-scenarios/delivery/fail-safe-ota" class="feat-card-link">Fail-safe OTA →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg></div>
<h3>Dependency Orchestration</h3>
<p>Startup order, health gates, and staggered restarts — upstreams pass checks before dependents start.</p>
<a href="/docs/03-orchestration/dependencies" class="feat-card-link">Dependencies →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"/></svg></div>
<h3>Event Reactions</h3>
<p>Run local scripts on <code>process_fatal</code> and other system events — distinct from webhook notifications (licensed).</p>
<a href="/docs/03-orchestration/events/hooks" class="feat-card-link">Event hooks →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"/></svg></div>
<h3>Cron &amp; Concurrency</h3>
<p>Cron schedules with overlap policies — skip, queue, or kill — so periodic jobs do not pile up under load.</p>
<a href="/docs/02-essentials/scheduled-tasks" class="feat-card-link">Scheduling →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9"/></svg></div>
<h3>API Control Plane</h3>
<p>CLI and scripts share the same REST endpoints — stream logs over WebSockets, scrape Prometheus metrics, run <code>super apply</code> from CI.</p>
<a href="/docs/04-production-scenarios/observability/programmatic-control" class="feat-card-link">Programmatic control →</a>
</div>
</div>
</div>
</section>

<!-- =============================================
  PREMIUM PLUGINS (shortcode)
  ============================================= -->
{{< home-premium >}}

<!-- =============================================
  API SHOWCASE
  ============================================= -->
<section class="hp-section hp-section--dark">
<div class="hp-container api-grid">
<div>
<div class="hp-label" style="color: var(--hp-accent);">Developer Experience</div>
<h2 class="hp-heading">API-first.<br>Every operation is an HTTP call.</h2>
<p class="hp-lead" style="margin-top: 0.75rem;">The CLI uses the same endpoints you do. Drive ops from scripts and CI. Stream logs via WebSockets. No custom SDK needed.</p>

<ul class="api-list">
<li>Declarative <code>super apply</code> — idempotent stack convergence from CI</li>
<li>CRUD for programs, stacks, and system config</li>
<li>Real-time WebSocket log streaming</li>
<li>Prometheus metrics at <code>/metrics</code></li>
<li>Swagger UI when <code>enable_docs = true</code></li>
</ul>
<a href="/docs/06-internals/api-reference" class="hp-btn hp-btn--outline">Full API Reference →</a>
</div>

<div class="hp-terminal">
<div class="hp-terminal-bar">
<div class="hp-terminal-dots"><span></span><span></span><span></span></div>
<span>~/project/super-demo</span>
<span style="opacity:0">—</span>
</div>
<div class="hp-terminal-body">
<pre><code><span style="color:#5c5a55"># Create &amp; start a program in three calls</span>
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
<span style="color:#5c5a55"># Start &amp; stream</span>
<span style="color:#6b6760">curl -X POST</span> http://127.0.0.1:9002/api/v1/programs/{id}/start
ws://127.0.0.1:9002/ws</code></pre>
</div>
</div>
</div>
</section>

<!-- =============================================
  CTA
  ============================================= -->
<section class="cta">
<div class="hp-container">
<h2 class="cta-title">Ready when you are.</h2>
<p class="cta-desc">Download the binary, define your stack, and have auditable process control in minutes.</p>
<div class="hero-actions" style="justify-content: center;">
<a href="/docs/01-getting-started/quick-start/" class="hp-btn hp-btn--primary">Start Building →</a>
<a href="/docs/01-getting-started/installation/" class="hp-btn hp-btn--outline">Install</a>
<a href="https://github.com/schiplat/super" class="hp-btn hp-btn--outline" target="_blank" rel="noopener">GitHub</a>
</div>
</div>
</section>

{{< contact-form >}}

</div>
