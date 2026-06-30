//! `quilltap-sqlite3mc-sys` — a **link-only** crate. It has no Rust API.
//!
//! Its [`build.rs`](../build.rs) compiles the vendored SQLite3MultipleCiphers
//! amalgamation (sqleet / ChaCha20-Poly1305 default cipher, the one every real
//! Quilltap database uses — NOT SQLCipher) and emits the link directives so
//! `libsqlite3-sys` (via `rusqlite`, with `buildtime_bindgen` and no bundled
//! feature) binds against the cipher-correct `sqlite3`.
//!
//! Crates that need the cipher-correct SQLite depend on this crate so its build
//! script runs and its link flags propagate to the final binary. Reference it as
//! `use quilltap_sqlite3mc_sys as _;` to keep the dependency in the crate graph.
//!
//! It lives apart from `quilltap-core` purely to keep the expensive 12 MB C
//! compile out of the per-commit version-bump churn (see `build.rs`).
