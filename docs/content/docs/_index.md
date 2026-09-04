---
title: "Documentation Center"
weight: 1
description: "Comprehensive guides, architectural deep dives, and API references for Project Super."
---

Welcome to the **Project Super** documentation. Whether you are setting up your first process, orchestrating a complex edge topology, or migrating from legacy tools, this guide covers everything you need.

[← Project Super homepage](/)

---

## OSS vs Licensed — how to read this documentation

Project Super ships as **one open-source binary** (`superd` + `super`) — there are no separate installers or editions. Two **run modes** differ only by whether licensed plugins are loaded:

| | **OSS** | **Licensed** |
| :--- | :--- | :--- |
| Binary | `superd` + `super` (MIT, free) | Same binaries — nothing to reinstall |
| Plugins | none | `[license].key` + `plugins/*` from your subscription (Super Pro) |
| Adds | Core process management, OTA, health checks, cron, event hooks | API auth/RBAC/audit (`security`), cgroups limits (`isolation`, Linux), notifications (`notify`), Dashboard (`ui` plugin) |
| How to enable | install & run | add a license key and plugin libraries |

Pages mark licensed capabilities with **💎** / "Licensed"; "Super Pro" is the brand name of the commercial plugin set ([Get Super Pro](/go/pro/)). The authoritative edition model, terminology, and switching guide live in [Editions](/docs/07-editions/).

---

## Start Here

If you are new to Project Super, start with the basics to get your daemon up and running.

{{< cards >}}
  {{< card link="/docs/01-getting-started" title="Getting Started" subtitle="Installation, Quick Start, and first-time setup." >}}
  {{< card link="/docs/02-essentials" title="Core Essentials" subtitle="Configuration (TOML), Logging, and Process Operations." >}}
{{< /cards >}}

---

## Core Features & Orchestration

Unlock the full potential of Super with advanced orchestration and dependency management.

{{< cards >}}
  {{< card link="/docs/03-orchestration" title="Orchestration" subtitle="Manage dependencies, health checks, and Atomic OTA updates." >}}
  {{< card link="/docs/05-advanced-management" title="Advanced Management" subtitle="Dashboard, Security, Authentication, RBAC, and Audit Logging." >}}
{{< /cards >}}

---

## Production & Architecture

Best practices for running Super in mission-critical environments.

{{< cards >}}
  {{< card link="/docs/04-production-scenarios" title="Production Scenarios" subtitle="Migration guides (PM2/Supervisor), Stability patterns, and Observability." >}}
  {{< card link="/docs/06-internals" title="Internals & Reference" subtitle="CLI / Config / API References and Design Philosophy." >}}
{{< /cards >}}

---

## Developer Guide

Build Super from source, understand the codebase, or hook custom logic into the process lifecycle.

{{< cards >}}
  {{< card link="/docs/09-development" title="Developer Guide" subtitle="Repository layout and the in-process Extension trait." >}}
  {{< card link="/docs/09-development/building-from-source" title="Building from Source" subtitle="Toolchain, workspace crates, test commands, and a local dev instance." >}}
  {{< card link="/docs/09-development/writing-extensions" title="Writing Extensions" subtitle="Hook custom Rust logic into the super-core lifecycle." >}}
{{< /cards >}}

