---
title: ""
description: "Что такое Project Super и как его установить и эксплуатировать."
width: full
toc: false
---

<div class="home-wrapper">

<section class="hp-section" style="padding-bottom: 2.5rem;">
<div class="hp-container hero">
<div>
<div class="hero-badge">
<span class="hero-badge-dot"></span>Rust · Кроссплатформенность · API-First · Аудит
</div>
<h1 class="hero-title">
Управление процессами, которому<br>можно <em>доверять, запрашивать</em> и менять
</h1>
<p class="hero-desc">Управляйте любым исполняемым файлом через декларативный TOML или REST — health checks, восстановление после сбоев и порядок зависимостей в одном бинарнике на Rust. Каждый lifecycle-событие попадает в запрашиваемый ledger; деплой с откатом; лицензионные плагины — для governance, алертов и лимитов cgroup.</p>
<div class="hero-actions">
<a href="/docs/01-getting-started/quick-start/" class="hp-btn hp-btn--primary">Начать →</a>
<a href="https://github.com/schiplat/super" class="hp-btn hp-btn--outline" target="_blank" rel="noopener">
<svg width="16" height="16" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" viewBox="0 0 24 24"><path d="M9 19c-5 1.5-5-2.5-7-3m14 6v-3.87a3.37 3.37 0 00-.94-2.61c3.14-.35 6.44-1.54 6.44-7A5.44 5.44 0 0020 4.77 5.07 5.07 0 0019.91 1S18.73.65 16 2.48a13.38 13.38 0 00-7 0C6.27.65 5.09 1 5.09 1A5.07 5.07 0 005 4.77a5.44 5.44 0 00-1.5 3.78c0 5.42 3.3 6.61 6.44 7A3.37 3.37 0 009 18.13V22"></path></svg>
  GitHub
</a>
</div>
</div>

<div class="hero-card">
<img src="/images/stack_flow.gif" alt="Super оркестрирует стек сервисов" loading="eager" />
<div class="hero-card-caption">Стек поднимается по зависимостям — upstream проходит health check до старта зависимых.</div>
</div>
</div>

<div class="hp-container">
<nav class="quick-nav" aria-label="Разделы документации">
<a href="/docs/01-getting-started/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M13 10V3L4 14h7v7l9-11h-7z"/></svg>Старт</a>
<a href="/docs/02-essentials/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4"/></svg>Основы</a>
<a href="/docs/03-orchestration/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg>Оркестрация</a>
<a href="/docs/04-production-scenarios/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01"/></svg>Продакшен</a>
<a href="/docs/05-advanced-management/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"/></svg>Продвинутое</a>
<a href="/docs/06-internals/api-reference/"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"/></svg>API</a>
</nav>
</div>
</section>

<section class="hp-section hp-section--alt">
<div class="hp-container">
<div class="hp-label">Возможности</div>
<h2 class="hp-heading">Всё, что должен уметь<br>менеджер процессов — и больше.</h2>
<p class="hp-lead" style="margin-top: 0.75rem;">Декларативный конфиг, оркестрация с учётом health и восстановление после сбоев — плюс запрашиваемый ledger событий, fail-safe доставка и API для CI.</p>

<div class="feat-grid feat-grid--caps">
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"/></svg></div>
<h3>Декларативные стеки</h3>
<p>Описывайте программы в TOML (или JSON). <code>super apply</code> сводит живое состояние к файлу — безопасно гонять из CI на каждом деплое.</p>
<a href="/docs/04-production-scenarios/delivery/declarative-stack" class="feat-card-link">Декларативные стеки →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z"/></svg></div>
<h3>Health-пробы</h3>
<p>TCP, HTTP и exec помечают программы Healthy; обработка выхода следует <code>autorestart</code>, <code>exitcodes</code> и экспоненциальному backoff.</p>
<a href="/docs/03-orchestration/health-checks" class="feat-card-link">Health checks →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"/></svg></div>
<h3>Ограниченные перезапуски</h3>
<p><code>retry_limit</code> останавливает бесконечный флапинг — каждый backoff пишется в историю событий до статуса Fatal.</p>
<a href="/docs/03-orchestration/events/types" class="feat-card-link">Типы событий →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"/></svg></div>
<h3>Любой бинарник, один демон</h3>
<p>Node.js, Python, Go, Rust, shell-скрипты или скомпилированный бинарник — что запускаете из shell, Super может взять под контроль. Один <code>superd</code> управляет всем через <code>command</code>.</p>
<a href="/docs/02-essentials/configuration" class="feat-card-link">Конфигурация →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01"/></svg></div>
<h3>История событий</h3>
<p>Каждое lifecycle-событие попадает в SQLite-ledger — запрашивайте краши, cron и восстановления через CLI или API, без grep логов.</p>
<a href="/docs/03-orchestration/events/history" class="feat-card-link">История событий →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/></svg></div>
<h3>Fail-safe деплой</h3>
<p>Атомарный OTA с авто-откатом, плюс <code>reload --wait</code> и <code>--wait-healthy</code> — изменение считается успешным только после готовности сервисов.</p>
<a href="/docs/04-production-scenarios/delivery/fail-safe-ota" class="feat-card-link">Fail-safe OTA →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg></div>
<h3>Оркестрация зависимостей</h3>
<p>Порядок старта, health-гейты и ступенчатые рестарты — upstream проходит проверки до запуска зависимых.</p>
<a href="/docs/03-orchestration/dependencies" class="feat-card-link">Зависимости →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"/></svg></div>
<h3>Реакции на события</h3>
<p>Локальные скрипты на <code>process_fatal</code> и других системных событиях — отдельно от лицензионных webhook-уведомлений.</p>
<a href="/docs/03-orchestration/events/hooks" class="feat-card-link">Event hooks →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"/></svg></div>
<h3>Cron и конкуренция</h3>
<p>Cron с политиками пересечения — skip, queue или kill — чтобы периодические задачи не копились под нагрузкой.</p>
<a href="/docs/02-essentials/scheduled-tasks" class="feat-card-link">Расписание →</a>
</div>
<div class="feat-card">
<div class="feat-card-icon"><svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9"/></svg></div>
<h3>API control plane</h3>
<p>CLI и скрипты делят одни и те же REST-эндпоинты — логи по WebSocket, Prometheus <code>/metrics</code>, <code>super apply</code> из CI.</p>
<a href="/docs/04-production-scenarios/observability/programmatic-control" class="feat-card-link">Программное управление →</a>
</div>
</div>
</div>
</section>

{{< home-premium >}}

<section class="hp-section hp-section--dark">
<div class="hp-container api-grid">
<div>
<div class="hp-label" style="color: var(--hp-accent);">Для разработчиков</div>
<h2 class="hp-heading">API-first.<br>Каждая операция — HTTP-вызов.</h2>
<p class="hp-lead" style="margin-top: 0.75rem;">CLI использует те же эндпоинты. Скрипты и CI, логи по WebSocket. Отдельный SDK не нужен.</p>

<ul class="api-list">
<li>Декларативный <code>super apply</code> — идемпотентная сходимость стека из CI</li>
<li>CRUD программ, стеков и системной конфигурации</li>
<li>Поток логов по WebSocket</li>
<li>Prometheus-метрики на <code>/metrics</code></li>
<li>Swagger UI при <code>enable_docs = true</code></li>
</ul>
<a href="/docs/06-internals/api-reference" class="hp-btn hp-btn--outline">Справка API →</a>
</div>

<div class="hp-terminal">
<div class="hp-terminal-bar">
<div class="hp-terminal-dots"><span></span><span></span><span></span></div>
<span>~/project/super-demo</span>
<span style="opacity:0">—</span>
</div>
<div class="hp-terminal-body">
<pre><code><span style="color:#5c5a55"># Создать и запустить программу за три вызова</span>
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
<span style="color:#5c5a55"># Старт и стрим</span>
<span style="color:#6b6760">curl -X POST</span> http://127.0.0.1:9002/api/v1/programs/{id}/start
ws://127.0.0.1:9002/ws</code></pre>
</div>
</div>
</div>
</section>

<section class="cta">
<div class="hp-container">
<h2 class="cta-title">Готовы — начинайте.</h2>
<p class="cta-desc">Скачайте бинарник, опишите стек — аудируемое управление процессами за минуты.</p>
<div class="hero-actions" style="justify-content: center;">
<a href="/docs/01-getting-started/quick-start/" class="hp-btn hp-btn--primary">Начать сборку →</a>
<a href="/docs/01-getting-started/installation/" class="hp-btn hp-btn--outline">Установка</a>
<a href="https://github.com/schiplat/super" class="hp-btn hp-btn--outline" target="_blank" rel="noopener">GitHub</a>
</div>
</div>
</section>

{{< contact-form >}}

</div>
