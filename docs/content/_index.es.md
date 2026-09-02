---
title: ""
description: "Qué es Project Super y cómo instalarlo y operarlo."
width: full
toc: false
---

<div class="home-wrapper">

<section class="hp-section" style="padding-bottom: 2.5rem;">
<div class="hp-container hero">
<div>
<div class="hero-badge">
<span class="hero-badge-dot"></span>Rust · Multiplataforma · API-first · Auditable
</div>
<h1 class="hero-title">
Control de procesos que puedes<br><em>consultar, cambiar</em> y confiar
</h1>
<p class="hero-desc">Gestiona cualquier ejecutable con TOML declarativo o REST — health checks, recuperación ante fallos y orden de dependencias en un binario Rust. Cada evento de ciclo de vida va a un ledger consultable; despliega con rollback; añade plugins con licencia para gobernanza, alertas y límites cgroup.</p>
<div class="hero-actions">
<a href="/docs/01-getting-started/quick-start/" class="hp-btn hp-btn--primary">Empezar →</a>
<a href="https://github.com/schiplat/super" class="hp-btn hp-btn--outline" target="_blank" rel="noopener">
<svg width="16" height="16" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" viewBox="0 0 24 24"><path d="M9 19c-5 1.5-5-2.5-7-3m14 6v-3.87a3.37 3.37 0 00-.94-2.61c3.14-.35 6.44-1.54 6.44-7A5.44 5.44 0 0020 4.77 5.07 5.07 0 0019.91 1S18.73.65 16 2.48a13.38 13.38 0 00-7 0C6.27.65 5.09 1 5.09 1A5.07 5.07 0 005 4.77a5.44 5.44 0 00-1.5 3.78c0 5.42 3.3 6.61 6.44 7A3.37 3.37 0 009 18.13V22"></path></svg>
  GitHub
</a>
</div>
</div>

<div class="hero-card">
<img src="/images/stack_flow.gif" alt="Super orquestando un stack de servicios" loading="eager" />
<div class="hero-card-caption">Stack arrancando en orden de dependencias — los upstream pasan health checks antes de iniciar dependientes.</div>
</div>
</div>

<div class="hp-container">
<nav class="quick-nav" aria-label="Secciones de documentación">
<a href="/docs/01-getting-started/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M13 10V3L4 14h7v7l9-11h-7z"/></svg>Inicio</a>
<a href="/docs/02-essentials/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4"/></svg>Esenciales</a>
<a href="/docs/03-orchestration/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg>Orquestación</a>
<a href="/docs/04-production-scenarios/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01"/></svg>Producción</a>
<a href="/docs/05-advanced-management/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"/></svg>Avanzado</a>
<a href="/docs/06-internals/api-reference/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"/></svg>API</a>
</nav>
</div>
</section>

<section class="hp-section hp-section--alt">
<div class="hp-container">
<div class="hp-label">Capacidades</div>
<h2 class="hp-heading">Todo lo que un gestor de procesos<br>debe hacer — y más.</h2>
<p class="hp-lead" style="margin-top: 0.75rem;">Config declarativa, orquestación con health y recuperación ante fallos — más un ledger de eventos consultable, entrega fail-safe y una API usable desde CI.</p>

<div class="feat-grid feat-grid--caps">
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"/></svg></div>
<h3>Stacks declarativos</h3>
<p>Define programas en TOML (o JSON). <code>super apply</code> converge el estado vivo al archivo — seguro en CI en cada deploy.</p>
<a href="/docs/04-production-scenarios/delivery/declarative-stack" class="feat-card-link">Stacks declarativos →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z"/></svg></div>
<h3>Sondas de salud</h3>
<p>Comprobaciones TCP, HTTP y exec marcan programas Healthy; la salida respeta <code>autorestart</code>, <code>exitcodes</code> y backoff exponencial.</p>
<a href="/docs/03-orchestration/health-checks" class="feat-card-link">Health checks →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"/></svg></div>
<h3>Reinicios acotados</h3>
<p><code>retry_limit</code> detiene el flapping infinito — cada backoff se registra en el historial de eventos antes de pasar a Fatal.</p>
<a href="/docs/03-orchestration/events/types" class="feat-card-link">Tipos de evento →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"/></svg></div>
<h3>Cualquier binario, un demonio</h3>
<p>Node.js, Python, Go, Rust, scripts shell o un binario compilado — lo que lances desde un shell, Super puede tomarlo. Un <code>superd</code> lo gestiona todo vía <code>command</code>.</p>
<a href="/docs/02-essentials/configuration" class="feat-card-link">Configuración →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01"/></svg></div>
<h3>Historial de eventos</h3>
<p>Cada evento de ciclo de vida llega a un ledger SQLite — consulta crashes, cron y recuperaciones por CLI o API sin greppear logs.</p>
<a href="/docs/03-orchestration/events/history" class="feat-card-link">Historial →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/></svg></div>
<h3>Despliegues fail-safe</h3>
<p>OTA atómico con rollback automático, más <code>reload --wait</code> y <code>--wait-healthy</code> para que el cambio termine solo cuando los servicios estén listos.</p>
<a href="/docs/04-production-scenarios/delivery/fail-safe-ota" class="feat-card-link">OTA fail-safe →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg></div>
<h3>Orquestación de dependencias</h3>
<p>Orden de arranque, puertas de salud y reinicios escalonados — los upstream pasan checks antes de iniciar dependientes.</p>
<a href="/docs/03-orchestration/dependencies" class="feat-card-link">Dependencias →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"/></svg></div>
<h3>Reacciones a eventos</h3>
<p>Ejecuta scripts locales en <code>process_fatal</code> y otros eventos del sistema — distinto de las notificaciones webhook con licencia.</p>
<a href="/docs/03-orchestration/events/hooks" class="feat-card-link">Event hooks →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"/></svg></div>
<h3>Cron y concurrencia</h3>
<p>Cron con políticas de solapamiento — skip, queue o kill — para que los jobs periódicos no se acumulen bajo carga.</p>
<a href="/docs/02-essentials/scheduled-tasks" class="feat-card-link">Programación →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9"/></svg></div>
<h3>Plano de control API</h3>
<p>CLI y scripts comparten los mismos endpoints REST — logs por WebSocket, métricas Prometheus, <code>super apply</code> desde CI.</p>
<a href="/docs/04-production-scenarios/observability/programmatic-control" class="feat-card-link">Control programático →</a>
</div>
</div>
</div>
</section>

{{< home-premium >}}

<section class="hp-section hp-section--dark">
<div class="hp-container api-grid">
<div>
<div class="hp-label" style="color: var(--hp-accent);">Experiencia de desarrollo</div>
<h2 class="hp-heading">API-first.<br>Cada operación es una llamada HTTP.</h2>
<p class="hp-lead" style="margin-top: 0.75rem;">La CLI usa los mismos endpoints. Opera con scripts y CI. Logs por WebSocket. Sin SDK propio.</p>

<ul class="api-list">
<li><code>super apply</code> declarativo — convergencia idempotente del stack desde CI</li>
<li>CRUD de programas, stacks y config del sistema</li>
<li>Streaming de logs por WebSocket</li>
<li>Métricas Prometheus en <code>/metrics</code></li>
<li>Swagger UI con <code>enable_docs = true</code></li>
</ul>
<a href="/docs/06-internals/api-reference" class="hp-btn hp-btn--outline">Referencia API →</a>
</div>

<div class="hp-terminal">
<div class="hp-terminal-bar">
<div class="hp-terminal-dots"><span></span><span></span><span></span></div>
<span>~/project/super-demo</span>
<span style="opacity:0">—</span>
</div>
<div class="hp-terminal-body">
<pre><code><span style="color:#5c5a55"># Crear e iniciar un programa en tres llamadas</span>
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
<span style="color:#5c5a55"># Iniciar y transmitir</span>
<span style="color:#6b6760">curl -X POST</span> http://127.0.0.1:9002/api/v1/programs/{id}/start
ws://127.0.0.1:9002/ws</code></pre>
</div>
</div>
</div>
</section>

<section class="cta">
<div class="hp-container">
<h2 class="cta-title">Listo cuando tú lo estés.</h2>
<p class="cta-desc">Descarga el binario, define tu stack y ten control de procesos auditable en minutos.</p>
<div class="hero-actions" style="justify-content: center;">
<a href="/docs/01-getting-started/quick-start/" class="hp-btn hp-btn--primary">Empezar a construir →</a>
<a href="/docs/01-getting-started/installation/" class="hp-btn hp-btn--outline">Instalar</a>
<a href="https://github.com/schiplat/super" class="hp-btn hp-btn--outline" target="_blank" rel="noopener">GitHub</a>
</div>
</div>
</section>

{{< contact-form >}}

</div>
