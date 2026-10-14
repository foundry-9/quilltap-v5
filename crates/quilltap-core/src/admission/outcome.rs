//! Port of v4 `lib/memory/admission/outcome.ts` (`8af764514`, Phase C of the
//! memory system refactor) — questions asked of an admission outcome. Pure;
//! safe anywhere.

use super::thresholds::AdmissionThresholds;
use super::types::{AdmissionAction, AdmissionDecision};

/// True when the outcome wrote a new row (v4 `admissionInserted`: `insert` |
/// `insert-linked`). Takes the action (v4's `Pick<AdmissionOutcome,
/// 'action'>`).
pub fn admission_inserted(action: &AdmissionAction) -> bool {
    matches!(
        action,
        AdmissionAction::Insert | AdmissionAction::InsertLinked { .. }
    )
}

/// True when the outcome counted the observation on an existing row (v4
/// `admissionReinforced`: `reinforce` | `absorb` | `promote-and-reinforce`).
pub fn admission_reinforced(action: &AdmissionAction) -> bool {
    matches!(
        action,
        AdmissionAction::Reinforce { .. }
            | AdmissionAction::Absorb { .. }
            | AdmissionAction::PromoteAndReinforce { .. }
    )
}

/// True when the decision takes the candidate in as a near-duplicate (v4
/// `isNearDuplicateDecision`): its target sits at or above the near-duplicate
/// threshold, whether it is absorbed outright or is a cold row promoted and
/// absorbed. A digest reached through a superseded member in the reinforce
/// band is absorbed by count but is not a near-duplicate. The target is the
/// FIRST assessment whose id is the action's `into`.
pub fn is_near_duplicate_decision(
    decision: &AdmissionDecision,
    thresholds: &AdmissionThresholds,
) -> bool {
    let Some(into) = decision.action.into_id() else {
        return false;
    };
    decision
        .assessments
        .iter()
        .find(|a| a.id == into)
        .is_some_and(|target| target.cosine >= thresholds.near_duplicate)
}
