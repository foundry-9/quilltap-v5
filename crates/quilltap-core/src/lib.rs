//! quilltap-core — the portable engine.
//!
//! Phase-0 surface is intentionally tiny: just the `dbkey` module, which
//! recovers the SQLCipher master key from the on-disk `quilltap.dbkey` file
//! the way the current app does. Everything else (repos, services, the
//! Request/Response/Event boundary) lands in later phases.

pub mod dbkey;
pub mod memory_weighting;
