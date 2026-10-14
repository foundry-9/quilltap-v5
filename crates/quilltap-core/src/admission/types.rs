//! Port of v4 `lib/memory/admission/types.ts` (`8af764514`, Phase C of the
//! memory system refactor) — the shapes admission speaks in.
//!
//! v4's why, carried: pure type declarations — safe to import from the job
//! child, the parent and the host-RPC wire alike.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::thresholds::AdmissionThresholds;

/// A candidate as the row it would become (v4 `AdmissionRow`): the
/// candidate's text, subject and provenance with its resolved event time and
/// its targeting tags written into `keywords`
/// ([`crate::memory_candidate::to_admission_row`] builds it). Plain data.
///
/// v4 types several fields `?: T | null`; every reader treats `undefined`
/// and `null` alike (`x || null`, `x ?? …`), so both are `None` here and the
/// serialised form omits them.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdmissionRow {
    /// The holder: whose Commonplace Book the memory enters.
    pub character_id: String,
    pub content: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,
    /// Group-level tags (UUIDs), never targeting tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub importance: Option<f64>,
    /// Whom the memory is about; `None` (or the holder's own id) for a self
    /// memory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub about_character_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<String>,
    /// The source chat's project, for recall-time scope comparisons.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// `'AUTO' | 'MANUAL' | 'CONSOLIDATED'`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_message_id: Option<String>,
    /// Overrides the row's createdAt/updatedAt with the source message's time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_message_timestamp: Option<String>,
    /// `'user_present' | 'autonomous_room' | 'manual'`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub witnessed_context: Option<String>,
    /// ISO wall-clock event time (not the write clock).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurred_at: Option<String>,
    /// Free-text in-story time, for chats on a fictional timeline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub narrative_time: Option<String>,
    /// Proper nouns of the episode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entities: Option<Vec<String>>,
    /// `'semantic' | 'episodic'`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

// === P4.D289 unit 7: the rest of v4 `types.ts` (around the pre-seeded row) ===

/// Serialize an `f64` the way `JSON.stringify` writes a JS number: an integral
/// value carries no decimal point (`1`, not serde's `1.0`), `-0` writes `0`,
/// and a non-finite value writes `null` (serde_json's own non-finite rule).
pub(crate) fn serialize_js_number<S: serde::Serializer>(x: &f64, s: S) -> Result<S::Ok, S::Error> {
    if x.is_finite() && x.fract() == 0.0 && x.abs() < 9_007_199_254_740_992.0 {
        s.serialize_i64(*x as i64)
    } else {
        s.serialize_f64(*x)
    }
}

/// Why an `insert-linked` candidate was linked rather than folded (v4's
/// `reason: 'band' | 'date-guard' | 'mismatch'`): the strongest of the
/// exclusions met, `date-guard` over `mismatch` over a plain `band`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinkReason {
    Band,
    DateGuard,
    Mismatch,
}

impl LinkReason {
    /// The wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            LinkReason::Band => "band",
            LinkReason::DateGuard => "date-guard",
            LinkReason::Mismatch => "mismatch",
        }
    }
}

/// Why nothing was written (v4's `reason: 'embedding-failed' | 'empty'`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RejectReason {
    EmbeddingFailed,
    Empty,
}

impl RejectReason {
    /// The wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            RejectReason::EmbeddingFailed => "embedding-failed",
            RejectReason::Empty => "empty",
        }
    }
}

/// What admission does with a candidate (v4 `AdmissionAction`). Decided by
/// [`crate::admission::decide_admission`] (pure), applied by the parent's
/// `admit_candidate` (P4.D290).
///
///  - `insert` — a new row, nothing near it.
///  - `insert-linked` — a new row, linked both ways to `link_to`: neighbours in
///    the related band, and closer ones it may not fold into (a different
///    occasion, subject or kind, or an extracted row a hand-written memory
///    must not disappear into). `reason` names the strongest of those.
///  - `reinforce` — fold the candidate into `into` (footnotes, anchors, count).
///  - `absorb` — count the observation on `into` and change nothing else: a
///    near-duplicate, or a reinforcement redirected from a superseded member
///    to its digest (`redirected_from` names the member).
///  - `promote-and-reinforce` — `into` is a cold row nothing supersedes; the
///    world asked for it again, so it returns to the hot tier, then reinforces
///    (`with_footnotes`) or is absorbed into (near-duplicate).
///  - `reject` — nothing is written.
///
/// The JSON is v4's object literal byte for byte (`kind` first, then the
/// variant's fields in declaration order, camelCase; `redirectedFrom` omitted
/// when absent) — pinned with `serde_json::to_string` against v4's
/// `JSON.stringify` in the `admission_decide` family.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AdmissionAction {
    Insert,
    #[serde(rename_all = "camelCase")]
    InsertLinked {
        link_to: Vec<String>,
        reason: LinkReason,
    },
    #[serde(rename_all = "camelCase")]
    Reinforce {
        into: String,
    },
    #[serde(rename_all = "camelCase")]
    Absorb {
        into: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        redirected_from: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    PromoteAndReinforce {
        into: String,
        with_footnotes: bool,
    },
    #[serde(rename_all = "camelCase")]
    Reject {
        reason: RejectReason,
    },
}

impl AdmissionAction {
    /// v4 `action.kind`.
    pub fn kind(&self) -> AdmissionActionKind {
        match self {
            AdmissionAction::Insert => AdmissionActionKind::Insert,
            AdmissionAction::InsertLinked { .. } => AdmissionActionKind::InsertLinked,
            AdmissionAction::Reinforce { .. } => AdmissionActionKind::Reinforce,
            AdmissionAction::Absorb { .. } => AdmissionActionKind::Absorb,
            AdmissionAction::PromoteAndReinforce { .. } => AdmissionActionKind::PromoteAndReinforce,
            AdmissionAction::Reject { .. } => AdmissionActionKind::Reject,
        }
    }

    /// v4's `'into' in action && action.into`: the target row of a reinforce,
    /// absorb or promote-and-reinforce.
    pub fn into_id(&self) -> Option<&str> {
        match self {
            AdmissionAction::Reinforce { into }
            | AdmissionAction::Absorb { into, .. }
            | AdmissionAction::PromoteAndReinforce { into, .. } => Some(into),
            _ => None,
        }
    }
}

/// v4 `AdmissionActionKind = AdmissionAction['kind']`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AdmissionActionKind {
    Insert,
    InsertLinked,
    Reinforce,
    Absorb,
    PromoteAndReinforce,
    Reject,
}

impl AdmissionActionKind {
    /// The wire string (`insert` | `insert-linked` | `reinforce` | `absorb` |
    /// `promote-and-reinforce` | `reject`).
    pub fn as_str(self) -> &'static str {
        match self {
            AdmissionActionKind::Insert => "insert",
            AdmissionActionKind::InsertLinked => "insert-linked",
            AdmissionActionKind::Reinforce => "reinforce",
            AdmissionActionKind::Absorb => "absorb",
            AdmissionActionKind::PromoteAndReinforce => "promote-and-reinforce",
            AdmissionActionKind::Reject => "reject",
        }
    }
}

/// One neighbour as the decider sees it (v4 `AdmissionNeighbour`): a live row
/// (v4's `Memory` JSON — the [`crate::db::memories_read`] row shape) and its
/// cosine to the candidate. A cold member superseded by a digest arrives
/// already replaced by the digest (`via` names the member it was reached
/// through); a cold row whose digest is gone arrives as itself with
/// `dangling_digest` set (v4 reads `danglingDigest === true`, so `false` and
/// absent are one).
///
/// `via` is read for TRUTHINESS, as v4 does: `Some("")` behaves as absent (no
/// `via` in the assessment, no digest redirect).
#[derive(Clone, Debug, PartialEq)]
pub struct AdmissionNeighbour {
    pub memory: Value,
    pub cosine: f64,
    pub via: Option<String>,
    pub dangling_digest: bool,
}

/// Why a close neighbour (≥ the reinforce threshold) could not take the
/// candidate (v4 `NeighbourExclusion`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NeighbourExclusion {
    Subject,
    Kind,
    ManualOverAuto,
    DateGuard,
}

impl NeighbourExclusion {
    /// The wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            NeighbourExclusion::Subject => "subject",
            NeighbourExclusion::Kind => "kind",
            NeighbourExclusion::ManualOverAuto => "manual-over-auto",
            NeighbourExclusion::DateGuard => "date-guard",
        }
    }
}

/// The decider's verdict on one neighbour (v4's `verdict`): below — under the
/// related band · band — linkable · excluded — close but ineligible ·
/// eligible — could fold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    Below,
    Band,
    Excluded,
    Eligible,
}

impl Verdict {
    /// The wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Below => "below",
            Verdict::Band => "band",
            Verdict::Excluded => "excluded",
            Verdict::Eligible => "eligible",
        }
    }
}

/// The decider's verdict on one neighbour, for the trace (v4
/// `NeighbourAssessment`). Serialises as v4's literal `{ id, cosine,
/// ...(via ? { via } : {}), verdict, excludedBy? }` — in that key order, the
/// cosine as a JS number (a NaN cosine writes `null`, as `JSON.stringify`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NeighbourAssessment {
    pub id: String,
    #[serde(serialize_with = "serialize_js_number")]
    pub cosine: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    pub verdict: Verdict,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excluded_by: Option<NeighbourExclusion>,
}

/// What [`crate::admission::decide_admission`] returns: the action and how
/// each neighbour was read (v4 `AdmissionDecision`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AdmissionDecision {
    pub action: AdmissionAction,
    pub assessments: Vec<NeighbourAssessment>,
}

/// Everything about one admission a debug log or a replay would want (v4
/// `AdmissionTrace`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdmissionTrace {
    pub thresholds: AdmissionThresholds,
    pub assessments: Vec<NeighbourAssessment>,
    /// Embedding attempts made for the candidate (0 when the caller supplied
    /// one).
    pub embedding_attempts: u32,
    /// Vectors whose row is gone, ignored by the search (bug 200).
    pub orphans_ignored: u32,
    /// How wide the search went to see past orphans.
    pub searched_k: u32,
    /// Free-form notes from preparation and apply (name-presence flips,
    /// failures).
    pub notes: Vec<String>,
}

/// The result of admitting one candidate (v4 `AdmissionOutcome`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdmissionOutcome {
    pub action: AdmissionAction,
    /// The row as it now stands (v4's `Memory` JSON): the new row for an
    /// insert (with its links), the reinforced or absorbing row otherwise,
    /// `None` for a reject. Never carries its embedding.
    pub memory: Option<Value>,
    /// Details appended to `memory` as `[+]` footnotes.
    pub novel_details: Vec<String>,
    /// Neighbours the new row was linked to (insert-linked only).
    pub linked_ids: Vec<String>,
    pub trace: AdmissionTrace,
}
