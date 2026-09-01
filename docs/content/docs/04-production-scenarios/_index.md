---
title: "Production Scenarios"
weight: 4
description: "Real-world patterns: Migrations, Stability, and Automation."
---

Features are useful, but **solutions** are what matter in production. This section moves beyond the "how-to" and focuses on the "why" and "when".

We address common operational nightmares—from "zombie processes" in Docker containers to "bricking" edge devices during updates—and show how Super solves them architecturally.

### In this section

#### 1. Migrations & Comparisons
Why switch? See how Super compares to the old guard.
*   [**vs Supervisor**](./migrations/vs-supervisor): Goodbye XML-RPC and Python dependencies.
*   [**vs PM2**](./migrations/vs-pm2): Saving memory and escaping the Node.js runtime tax.

#### 2. Observability & Integration
*   [**Instant Metrics**](./observability/instant-metrics): Zero-config Prometheus integration.
*   [**Programmable Ops**](./observability/programmatic-control): Building automation with the API.

#### 3. Stability Governance
*   [**Preventing Cascading Failures**](./stability/preventing-cascading-failures): Using orchestration to stop boot loops.
*   [**Container Deployment**](./stability/zombie-reaping-in-containers): Running Super in Docker — scope, limits, and `tini` integration.
*   [**Resource Isolation**](./stability/resource-isolation): Stopping "noisy neighbors" with Cgroups.

#### 4. Delivery & State
*   [**Fail-Safe OTA**](./delivery/fail-safe-ota): Updating edge devices without fear.
*   [**Declarative Stack**](./delivery/declarative-stack): GitOps-style infrastructure management.
*   [**Snapshot Persistence & Restore**](./delivery/snapshot-and-restore): Backing up and restoring Super's authoritative program state.
