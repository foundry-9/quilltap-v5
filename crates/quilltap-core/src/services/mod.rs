//! Phase-3 **services** — the first code that makes decisions rather than just
//! persisting rows. Each service sits on the trusted Phase-2 data layer (repos +
//! the partitioned apply path) and the Phase-3 foundations (the writer-task
//! runtime [`crate::db::runtime`] and the model boundary [`crate::model`]), so any
//! failure localizes to the service, not the store.
//!
//! Ported so far:
//!
//! * [`memory_gate`] — the pre-write similarity gate (v4 `createMemoryWithGate` +
//!   `runMemoryGate`): the append-or-reinforce decision. The first model-dependent
//!   service, verified tier-3 → tier-2 (a canned embedding injected identically on
//!   both differential sides, then a structural DB diff).

pub mod memory_gate;
