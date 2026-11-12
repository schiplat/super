# sqlx-sqlite 0.9.0 (vendored)

Copied unmodified from the crates.io `sqlx-sqlite` 0.9.0 package, except for
one line in `Cargo.toml`:

```toml
# upstream: version = ">=0.30.1, <0.38.0"
version = ">=0.30.1, <0.40.0"
```

Reason: `sqlx-sqlite` 0.9.0 caps `libsqlite3-sys` below 0.38, and every release
up to 0.38.2 bundles a SQLite version affected by CVE-2026-119375
(RUSTSEC-2026-0459). `libsqlite3-sys` 0.39.1 bundles SQLite 3.54.1. The public
API used by this crate is unchanged; the full test suite covers the SQLite store.

Remove this directory and the `[patch.crates-io]` entry in `Cargo.toml` once
upstream sqlx allows `libsqlite3-sys >= 0.39`.

License: MIT OR Apache-2.0 (see `LICENSE-MIT`, `LICENSE-APACHE`).
