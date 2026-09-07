---
title: "5. Extensibility (Open Kernel)"
linkTitle: "Extensibility"
weight: 5
---

Super is not a black box. It exposes a native Rust `Extension` trait, letting you hook custom logic into the process lifecycle — env injection, pre-flight start gates, auditing, metrics — either compiled into your own binary (OSS) or as signed runtime plugins (licensed).
