---
title: "Developer Guide"
weight: 8
description: "Build Project Super from source and extend it with the in-process Extension trait."
---

Project Super is an MIT-licensed Rust codebase that is built to be transparent. The daemon is a thin `superd` binary over the `super-core` library, so you can build the whole project from source, inspect how it works, and hook your own logic into the process lifecycle without forking the core.

This guide is for developers who want to:

*   Build the `superd` daemon and the `super` CLI from source.
*   Understand how the repository and its crates are organized.
*   Extend `super-core` with custom Rust logic via the `Extension` trait.
*   Contribute fixes and features upstream.

### In this section

*   [**Building from Source**](./building-from-source): toolchain requirements, workspace layout, build / test commands, and running a local dev instance.
*   [**Writing Extensions**](./writing-extensions): the in-process `Extension` trait — the supported way to add custom lifecycle logic to a `super-core` host.

> [!NOTE]
> Contribution workflow (pull requests, commit style, CI gates) lives in [`CONTRIBUTING.md`](https://github.com/schiplat/super/blob/master/CONTRIBUTING.md) in the repository. For architecture rationale and system diagrams, see [Design Philosophy](/docs/06-internals/design-philosophy).
