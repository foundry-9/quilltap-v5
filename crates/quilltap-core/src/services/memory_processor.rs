//! v4 deleted `lib/memory/memory-processor.ts` in `ac6348c5f` (memory
//! refactor Phase D): per-turn extraction is the ONE pipeline in
//! [`crate::services::extraction`] (work order P4.D296). What survives here is
//! a forwarding re-export of the canon loaders, which moved to
//! [`crate::services::canon_loaders`] (L-9), for the two callers outside the
//! extraction lane — `wardrobe_instructions.rs` (`read_vault_text_file_conn`)
//! and `services/consolidation.rs` (P4.D297's). HANDOFF (§S.5 named handoff
//! (8)): re-point both imports onto `services::canon_loaders`, then delete
//! this file.

// The canon loaders moved to `services::canon_loaders` (work order P4.D296
// L-9). Two callers outside the extraction lane still import them from here.
// LANE-REMOVE(P4.D296): v4 deleted memory-processor.ts in ac6348c5f
pub use super::canon_loaders::{
    load_canon_for_observer_about_subject, load_canon_for_self_with_commonplace,
    load_commonplace_canon, read_vault_text_file, read_vault_text_file_conn, CanonSubject,
};
