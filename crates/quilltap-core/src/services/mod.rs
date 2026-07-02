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
//! * [`memory_service`] — the cascade-delete family (v4 `deleteMemoryWithVector` +
//!   the three `deleteMemoriesBy*WithVectors` cascades): the vector-store-aware
//!   wrappers around the deletion chokepoint. No model call; verified by a plain
//!   tier-2 differential.
//! * [`housekeeping`] — the retention sweep (v4 `runHousekeeping` /
//!   `needsHousekeeping`): protection-gated policy deletions, the opt-in
//!   stored-vector similarity merge, and cap enforcement, applied through the
//!   chokepoint. No model call; verified by a plain tier-2 differential.

pub mod housekeeping;
pub mod memory_gate;
pub mod memory_service;
