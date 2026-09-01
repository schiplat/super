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
<span class="hero-badge-dot"></span>Rust · API-First · One Binary
</div>
<h1 class="hero-title">
  Run your services<br>with <em>confidence</em>
</h1>
<p class="hero-desc">A lightweight process manager that handles restarts, startup order, health checks, and OTA updates — define programs in TOML or over REST.</p>
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
<h2 class="hp-heading">One binary.<br>Zero dependencies.</h2>
<p class="hp-lead" style="margin-top: 0.75rem;">Everything you need to run, watch, and recover services in production.</p>

<div class="feat-grid" style="margin-top: 2.5rem;">
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"/></svg></div>
<h3>Lifecycle Hooks</h3>
<p>Shell scripts at <code>pre_start</code>, <code>post_start</code>, <code>post_stop</code> — prepare environments, notify services, or clean up.</p>
<a href="/docs/03-orchestration/lifecycle-hooks" class="feat-card-link">Hooks →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"/></svg></div>
<h3>Event Hooks</h3>
<p>React to <code>process_fatal</code> and system events with local scripts — Supervisor-style, API-driven.</p>
<a href="/docs/03-orchestration/events" class="feat-card-link">Events →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z"/></svg></div>
<h3>Auto-Recovery</h3>
<p>Supervisor-compatible <code>autorestart</code>, <code>exitcodes</code>, <code>startsecs</code> — seamless migration.</p>
<a href="/docs/04-production-scenarios/migrations/vs-supervisor" class="feat-card-link">vs Supervisor →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"/></svg></div>
<h3>Cron Scheduling</h3>
<p>Run programs on cron expressions. Periodic jobs without an external scheduler or crontab.</p>
<a href="/docs/02-essentials/scheduled-tasks" class="feat-card-link">Scheduling →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9"/></svg></div>
<h3>HTTP Ops</h3>
<p>One REST API for CLI, scripts, and remote control. Add Bearer auth via plugins.</p>
<a href="/docs/04-production-scenarios/observability/programmatic-control" class="feat-card-link">API →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"/></svg></div>
<h3>Live Logs</h3>
<p>Real-time log streams over WebSockets. Monitor processes from anywhere.</p>
<a href="/docs/02-essentials/logging" class="feat-card-link">Logging →</a>
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
<p class="cta-desc">Download the binary, define your stack, and have services running in minutes.</p>
<div class="hero-actions" style="justify-content: center;">
<a href="/docs/01-getting-started/quick-start/" class="hp-btn hp-btn--primary">Start Building →</a>
<a href="/docs/01-getting-started/installation/" class="hp-btn hp-btn--outline">Install</a>
<a href="https://github.com/schiplat/super" class="hp-btn hp-btn--outline" target="_blank" rel="noopener">GitHub</a>
</div>
</div>
</section>

{{< contact-form >}}

</div>
