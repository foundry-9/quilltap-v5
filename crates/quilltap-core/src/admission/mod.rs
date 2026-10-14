//! Port of v4's `lib/memory/admission/` PURE half (`8af764514`, Phase C of
//! the memory system refactor; the TF-IDF thresholds from `4ae0c4ca6`) — the
//! one decision every memory write goes through, and THE ONE COMBINE.
//!
//! The stateful half (`admit`, `neighbours`, `apply`, `prepare`, `links`, the
//! rate ledger, the dry-run book, `update_memory`) is
//! [`crate::services::admission`]'s (work order P4.D290).

pub mod decide;
pub mod describe;
pub mod fold;
pub mod novel_details;
pub mod outcome;
pub mod thresholds;
pub mod types;

pub use decide::{decide_admission, occasions_are_distinct, subject_of, AdmissionCandidateView};
pub use describe::describe_admission;
pub use fold::{fold_into, FoldCaps, FoldPatch, FoldPlan, FoldSource, FoldSurvivor, FOLD_CAPS};
pub use novel_details::extract_novel_details;
pub use outcome::{admission_inserted, admission_reinforced, is_near_duplicate_decision};
pub use thresholds::{
    admission_thresholds_for_provider, AdmissionThresholds, ADMISSION_THRESHOLDS,
    ADMISSION_THRESHOLDS_TFIDF, ADMISSION_TOP_K,
};
pub use types::{
    AdmissionAction, AdmissionActionKind, AdmissionDecision, AdmissionNeighbour, AdmissionOutcome,
    AdmissionRow, AdmissionTrace, LinkReason, NeighbourAssessment, NeighbourExclusion,
    RejectReason, Verdict,
};
