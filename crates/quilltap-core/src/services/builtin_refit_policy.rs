//! When the Built-in (TF-IDF) vocabulary needs a refit — P4.D292, a port of
//! v4 `lib/embedding/builtin-refit-policy.ts` (`4ae0c4ca6`, at the
//! `83334b9a5` pin).
//!
//! "A refit rebuilds the vocabulary from every memory and help doc and then
//! re-embeds all of them (`EMBEDDING_REFIT` → `EMBEDDING_REINDEX_ALL`): a few
//! hundred jobs, a new vector width, and a window in which stored vectors and
//! fresh ones cannot be compared (bug 237). A memory written between refits is
//! embedded against the current vocabulary, its new words out of vocabulary
//! until the next fit — the ordinary TF-IDF trade — so a single edit is never
//! a reason to refit." The vocabulary is refit when it is:
//!
//!  - `unfitted` — there is no vocabulary row (or its `fittedAt` is empty);
//!  - `text-shape` — stamped with an older `textShapeVersion` than
//!    [`LEXICAL_TEXT_SHAPE_VERSION`] (0: fitted, and its memories embedded,
//!    with anchor lines before the BUILTIN path left them out);
//!  - `drift` — the memories created since the fit reach
//!    max([`REFIT_MIN_NEW_MEMORIES`], [`REFIT_DRIFT_FRACTION`] × all).
//!
//! "The counts read the corpus the refit fits on — every memory row
//! (`countCorpusSince`); `handleEmbeddingRefit` fits every memory of every
//! character holding one — so a due refit always has something to fit and
//! clears its own reason."
//!
//! [`evaluate_refit_need`] is the pure rule; [`assess_builtin_refit`] reads
//! the counts; [`enqueue_refit_if_due`] is the ONE serialised
//! check-then-enqueue, used by the debounced
//! [`schedule_refit`](crate::services::embedding_job_scheduler::schedule_refit)
//! and by [`check_builtin_refit`] (the instance default profile, from boot
//! PHASE 3.8 and the daily DATA pass's step 10).
//!
//! **Serialisation.** v4 chains every call on a process-wide promise tail
//! (`refitChain`, on `processSingleton` for bug 235's bundle copies — NO-PORT:
//! one Rust process, the static IS process-wide). The port is a
//! `tokio::sync::Mutex<()>` held across assess → in-hand → enqueue; v4 chains
//! on failure too, and the guard releases on drop either way.
//!
//! **Lines.** Every line carries v4's child-logger binding `module:
//! 'embedding:builtin-refit-policy'` FIRST (v4's `child()` spreads its context
//! ahead of the call's). Each is emitted on the CALLER's thread — none inside a
//! `Db::write` closure — so the thread-scoped capture rig sees them.

use serde_json::{json, Value};

use crate::db::runtime::Db;
use crate::db::DbError;
use crate::embedding_scale::is_lexical_embedding_provider;
pub use crate::memory_embedding_text::LEXICAL_TEXT_SHAPE_VERSION;
use crate::pascal::js_value::number_to_string;

/// v4's child-logger binding.
const MODULE: &str = "embedding:builtin-refit-policy";

/// "Fewest new memories that count as drift, however small the corpus."
pub const REFIT_MIN_NEW_MEMORIES: f64 = 25.0;

/// "Share of the corpus that, created since the fit, counts as drift."
pub const REFIT_DRIFT_FRACTION: f64 = 0.1;

/// v4 `BuiltinRefitReason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinRefitReason {
    Unfitted,
    TextShape,
    Drift,
}

impl BuiltinRefitReason {
    /// v4's wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            BuiltinRefitReason::Unfitted => "unfitted",
            BuiltinRefitReason::TextShape => "text-shape",
            BuiltinRefitReason::Drift => "drift",
        }
    }
}

/// v4 `RefitNeedInput`.
#[derive(Debug, Clone, PartialEq)]
pub struct RefitNeedInput {
    /// The vocabulary's `fittedAt`, or `None` when there is none.
    pub fitted_at: Option<String>,
    /// The vocabulary's `textShapeVersion` (`None` = v4's `null` / absent,
    /// read as 0).
    pub text_shape_version: Option<f64>,
    /// Memories created at or after `fitted_at`.
    pub new_since_fit: f64,
    /// All memories.
    pub total_memories: f64,
}

/// v4 `driftThreshold(totalMemories)` — `Math.max(25, Math.ceil(0.1 ×
/// total))`, kept as v4's f64 expression (it agrees with the integer
/// `ceil(t / 10)` for every `t` in 0..200,000 — measured at planning).
pub fn drift_threshold(total_memories: f64) -> f64 {
    crate::jsnum::math_max(
        REFIT_MIN_NEW_MEMORIES,
        (REFIT_DRIFT_FRACTION * total_memories).ceil(),
    )
}

/// v4 `evaluateRefitNeed` — why the vocabulary needs a refit, or `None` when
/// it does not. Pure; ordered: `!fittedAt` (null OR `''`) → `unfitted`;
/// `(textShapeVersion ?? 0) < LEXICAL_TEXT_SHAPE_VERSION` → `text-shape`;
/// `newSinceFit >= driftThreshold(totalMemories)` → `drift`.
pub fn evaluate_refit_need(input: &RefitNeedInput) -> Option<BuiltinRefitReason> {
    if input.fitted_at.as_deref().is_none_or(str::is_empty) {
        return Some(BuiltinRefitReason::Unfitted);
    }
    if input.text_shape_version.unwrap_or(0.0) < LEXICAL_TEXT_SHAPE_VERSION as f64 {
        return Some(BuiltinRefitReason::TextShape);
    }
    if input.new_since_fit >= drift_threshold(input.total_memories) {
        return Some(BuiltinRefitReason::Drift);
    }
    None
}

/// v4 `RefitAssessment` — the input the rule read, with its reason.
#[derive(Debug, Clone, PartialEq)]
pub struct RefitAssessment {
    pub input: RefitNeedInput,
    pub reason: Option<BuiltinRefitReason>,
}

fn js_opt_str(v: Option<&str>) -> &str {
    v.unwrap_or("null")
}

/// v4 `assessBuiltinRefit(profile)`: `findByProfileId` (FALLBACK `null`),
/// `countCorpusSince(fittedAt)` (FALLBACK `{0, 0}`), the rule, and DEBUG
/// `Assessed the TF-IDF vocabulary {module, profileId, fittedAt,
/// textShapeVersion, newSinceFit, totalMemories, driftThreshold, reason}` (v4's
/// `{ profileId, ...input, driftThreshold, reason }`). `textShapeVersion` is
/// the vocabulary's value or 0 (v4 `vocabulary?.textShapeVersion ?? 0`). The
/// `Err` arm is reached only inside the strict-repository scope.
pub fn assess_builtin_refit(
    conn: &rusqlite::Connection,
    profile_id: &str,
) -> Result<RefitAssessment, DbError> {
    let vocabulary = crate::db::fallback::tfidf_vocabulary_by_profile_or_none(profile_id, || {
        crate::db::tfidf_vocabulary::find_by_profile_id(conn, profile_id)
    })?;
    let fitted_at = vocabulary.as_ref().map(|v| v.fitted_at.clone());
    let text_shape_version = vocabulary.as_ref().map_or(0.0, |v| v.text_shape_version);
    // v4 `since ? count(...) : 0` — an EMPTY `fittedAt` is falsy and counts
    // nothing new. `count_corpus_since` (P4.D288's read) counts on `Some("")`,
    // so the empty string is filtered here (HANDOFF: the body's own
    // `Some(s) if !s.is_empty()`, `memories_read.rs`).
    let (total, created_since) = crate::db::memories_read::count_corpus_since(
        conn,
        fitted_at.as_deref().filter(|s| !s.is_empty()),
    )?;
    let input = RefitNeedInput {
        fitted_at,
        text_shape_version: Some(text_shape_version),
        new_since_fit: created_since as f64,
        total_memories: total as f64,
    };
    let reason = evaluate_refit_need(&input);
    tracing::debug!(
        target: "quilltap::embedding",
        module = MODULE,
        profileId = %profile_id,
        fittedAt = %js_opt_str(input.fitted_at.as_deref()),
        textShapeVersion = %number_to_string(text_shape_version),
        newSinceFit = %number_to_string(input.new_since_fit),
        totalMemories = %number_to_string(input.total_memories),
        driftThreshold = %number_to_string(drift_threshold(input.total_memories)),
        reason = %js_opt_str(reason.map(BuiltinRefitReason::as_str)),
        "Assessed the TF-IDF vocabulary"
    );
    Ok(RefitAssessment { input, reason })
}

/// v4 `RefitInHand` — a refit or full reindex already pending or running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefitInHand {
    /// `EMBEDDING_REFIT` | `EMBEDDING_REINDEX_ALL`.
    pub job_type: &'static str,
    /// `PENDING` | `PROCESSING`.
    pub status: &'static str,
}

impl RefitInHand {
    /// v4's `{ type, status }`.
    pub fn to_json(self) -> Value {
        json!({ "type": self.job_type, "status": self.status })
    }
}

/// v4 `refitInHand()`: for `EMBEDDING_REFIT` then `EMBEDDING_REINDEX_ALL`, the
/// first PENDING active job, else the first PROCESSING one ("queried by
/// status, so a pending job in backoff is always seen"); `None` when neither
/// type has one.
pub fn refit_in_hand(conn: &rusqlite::Connection) -> Result<Option<RefitInHand>, DbError> {
    let repo = crate::db::background_jobs::BackgroundJobsRepository::new(conn);
    for job_type in ["EMBEDDING_REFIT", "EMBEDDING_REINDEX_ALL"] {
        let active = repo.find_active_by_type(job_type)?;
        let status = if active.iter().any(|j| j.status == "PENDING") {
            Some("PENDING")
        } else if active.iter().any(|j| j.status == "PROCESSING") {
            Some("PROCESSING")
        } else {
            None
        };
        if let Some(status) = status {
            return Ok(Some(RefitInHand { job_type, status }));
        }
    }
    Ok(None)
}

/// A decision's reason — a due reason, or `forced` (v4 `BuiltinRefitReason |
/// 'forced'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefitDecisionReason {
    Due(BuiltinRefitReason),
    Forced,
}

impl RefitDecisionReason {
    pub fn as_str(self) -> &'static str {
        match self {
            RefitDecisionReason::Due(r) => r.as_str(),
            RefitDecisionReason::Forced => "forced",
        }
    }
}

/// Why nothing was enqueued for a due reason (v4 `'nothing-to-fit' |
/// RefitInHand`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefitHeldBy {
    NothingToFit,
    InHand(RefitInHand),
}

/// v4 `RefitDecision`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefitDecision {
    pub reason: Option<RefitDecisionReason>,
    pub enqueued: bool,
    pub held_by: Option<RefitHeldBy>,
}

impl RefitDecision {
    /// v4's object in its key order (`heldBy` only when set).
    pub fn to_json(self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert(
            "reason".into(),
            self.reason.map_or(Value::Null, |r| r.as_str().into()),
        );
        out.insert("enqueued".into(), self.enqueued.into());
        match self.held_by {
            Some(RefitHeldBy::NothingToFit) => {
                out.insert("heldBy".into(), "nothing-to-fit".into());
            }
            Some(RefitHeldBy::InHand(h)) => {
                out.insert("heldBy".into(), h.to_json());
            }
            None => {}
        }
        Value::Object(out)
    }
}

/// v4's `refitChain` — the process-wide serialisation of check-then-enqueue.
static REFIT_CHAIN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// v4 `enqueueRefitIfDue(profile, { force, source })`: "Enqueue a
/// refit-with-reindex for `profile` when one is due — or always, with `force` —
/// unless a refit is already in hand. Serialised process-wide: the check and
/// the enqueue are one step, so boot, the maintenance pass and a debounced
/// memory edit firing together enqueue one refit, not three."
///
/// Unforced: the assessment; no reason → `{null, false}`; a due reason over an
/// EMPTY corpus → DEBUG `TF-IDF refit due but there are no memories to fit
/// {module, profileId, reason, source}` → held by `nothing-to-fit`. (A forced
/// call skips both — it enqueues even on an empty corpus.) Then the in-hand
/// check: "A forced request (an import) is held only by a *pending* refit,
/// which will read the grown corpus when it runs; a running one may have read
/// it already" → INFO `TF-IDF refit due but one is already in hand; not
/// enqueueing another {module, profileId, reason, inHand, source}`. Else
/// `enqueueEmbeddingRefit(profile.userId, { profileId, triggerReindex: true })`
/// and INFO `TF-IDF vocabulary refit enqueued {module, profileId, reason,
/// source}`. `profile_user_id` is the PROFILE's owner (v4 `profile.userId`).
pub async fn enqueue_refit_if_due(
    db: &Db,
    profile_id: &str,
    profile_user_id: &str,
    force: bool,
    source: &str,
) -> Result<RefitDecision, DbError> {
    let _chain = REFIT_CHAIN.lock().await;
    let mut reason = force.then_some(RefitDecisionReason::Forced);
    if !force {
        let pid = profile_id.to_string();
        let assessment = db.read_main(move |conn| assess_builtin_refit(conn, &pid))?;
        let Some(due) = assessment.reason else {
            return Ok(RefitDecision {
                reason: None,
                enqueued: false,
                held_by: None,
            });
        };
        reason = Some(RefitDecisionReason::Due(due));
        // "No memories: the refit has nothing to fit and would leave the
        // reason standing."
        if assessment.input.total_memories == 0.0 {
            tracing::debug!(
                target: "quilltap::embedding",
                module = MODULE,
                profileId = %profile_id,
                reason = due.as_str(),
                source = %source,
                "TF-IDF refit due but there are no memories to fit"
            );
            return Ok(RefitDecision {
                reason,
                enqueued: false,
                held_by: Some(RefitHeldBy::NothingToFit),
            });
        }
    }
    let reason_str = reason.map(RefitDecisionReason::as_str);
    let in_hand = db.read_main(refit_in_hand)?;
    if let Some(in_hand) = in_hand {
        if !force || in_hand.status == "PENDING" {
            tracing::info!(
                target: "quilltap::embedding",
                module = MODULE,
                profileId = %profile_id,
                reason = %js_opt_str(reason_str),
                inHandJson = %in_hand.to_json(),
                source = %source,
                "TF-IDF refit due but one is already in hand; not enqueueing another"
            );
            return Ok(RefitDecision {
                reason,
                enqueued: false,
                held_by: Some(RefitHeldBy::InHand(in_hand)),
            });
        }
    }
    crate::services::queue_service::enqueue_embedding_refit(db, profile_user_id, profile_id, true)
        .await?;
    tracing::info!(
        target: "quilltap::embedding",
        module = MODULE,
        profileId = %profile_id,
        reason = %js_opt_str(reason_str),
        source = %source,
        "TF-IDF vocabulary refit enqueued"
    );
    Ok(RefitDecision {
        reason,
        enqueued: true,
        held_by: None,
    })
}

/// v4 `BuiltinRefitCheck`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinRefitCheck {
    pub profile_id: Option<String>,
    pub reason: Option<BuiltinRefitReason>,
    pub enqueued: bool,
}

impl BuiltinRefitCheck {
    /// v4's `{ profileId, reason, enqueued }`.
    pub fn to_json(&self) -> Value {
        json!({
            "profileId": self.profile_id,
            "reason": self.reason.map(BuiltinRefitReason::as_str),
            "enqueued": self.enqueued,
        })
    }
}

/// v4 `checkBuiltinRefit()` — "Refit the instance default profile's
/// vocabulary when it is due (BUILTIN only). Boot and the scheduled
/// maintenance pass call it. Never throws." The INSTANCE default
/// (`findInstanceDefault`, whoever owns it — not `scheduleRefit`'s per-user
/// `findDefault`); none, or a non-lexical provider → `{ profileId: profile?.id
/// ?? null, reason: null, enqueued: false }`; else the decision with `forced`
/// mapped to `null` (unreachable — the check never forces). Any failure → WARN
/// `TF-IDF refit check failed {module, error}` and all-null.
pub async fn check_builtin_refit(db: &Db) -> BuiltinRefitCheck {
    let outcome: Result<BuiltinRefitCheck, DbError> = async {
        let profile = db.read_main(crate::db::embedding_profiles::find_instance_default)?;
        let profile = match profile {
            Some(p) if is_lexical_embedding_provider(Some(&p.provider)) => p,
            other => {
                return Ok(BuiltinRefitCheck {
                    profile_id: other.map(|p| p.id),
                    reason: None,
                    enqueued: false,
                })
            }
        };
        let decision =
            enqueue_refit_if_due(db, &profile.id, &profile.user_id, false, "check").await?;
        Ok(BuiltinRefitCheck {
            profile_id: Some(profile.id),
            reason: match decision.reason {
                Some(RefitDecisionReason::Due(r)) => Some(r),
                Some(RefitDecisionReason::Forced) | None => None,
            },
            enqueued: decision.enqueued,
        })
    }
    .await;
    outcome.unwrap_or_else(|e| {
        tracing::warn!(
            target: "quilltap::embedding",
            module = MODULE,
            error = %crate::db::fallback::error_text(&e),
            "TF-IDF refit check failed"
        );
        BuiltinRefitCheck {
            profile_id: None,
            reason: None,
            enqueued: false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(fitted: Option<&str>, version: Option<f64>, new: f64, total: f64) -> RefitNeedInput {
        RefitNeedInput {
            fitted_at: fitted.map(str::to_string),
            text_shape_version: version,
            new_since_fit: new,
            total_memories: total,
        }
    }

    #[test]
    fn the_rule_in_v4_order() {
        let t = Some("2026-01-01T00:00:00.000Z");
        assert_eq!(
            evaluate_refit_need(&input(None, Some(1.0), 0.0, 0.0)),
            Some(BuiltinRefitReason::Unfitted)
        );
        assert_eq!(
            evaluate_refit_need(&input(Some(""), Some(0.0), 99.0, 1.0)),
            Some(BuiltinRefitReason::Unfitted),
            "an empty fittedAt is falsy"
        );
        assert_eq!(
            evaluate_refit_need(&input(t, None, 0.0, 0.0)),
            Some(BuiltinRefitReason::TextShape)
        );
        assert_eq!(
            evaluate_refit_need(&input(t, Some(0.5), 0.0, 0.0)),
            Some(BuiltinRefitReason::TextShape)
        );
        assert_eq!(evaluate_refit_need(&input(t, Some(2.0), 24.0, 0.0)), None);
        assert_eq!(
            evaluate_refit_need(&input(t, Some(1.0), 25.0, 249.0)),
            Some(BuiltinRefitReason::Drift)
        );
        assert_eq!(evaluate_refit_need(&input(t, Some(1.0), 25.0, 251.0)), None);
    }

    #[test]
    fn the_drift_bar() {
        for (total, bar) in [
            (0.0, 25.0),
            (249.0, 25.0),
            (250.0, 25.0),
            (251.0, 26.0),
            (10_000.0, 1000.0),
        ] {
            assert_eq!(drift_threshold(total), bar, "total {total}");
        }
    }

    #[test]
    fn decision_objects_in_v4_key_order() {
        let held = RefitDecision {
            reason: Some(RefitDecisionReason::Due(BuiltinRefitReason::Drift)),
            enqueued: false,
            held_by: Some(RefitHeldBy::InHand(RefitInHand {
                job_type: "EMBEDDING_REFIT",
                status: "PENDING",
            })),
        };
        assert_eq!(
            serde_json::to_string(&held.to_json()).unwrap(),
            r#"{"reason":"drift","enqueued":false,"heldBy":{"type":"EMBEDDING_REFIT","status":"PENDING"}}"#
        );
        let none = RefitDecision {
            reason: None,
            enqueued: false,
            held_by: None,
        };
        assert_eq!(
            serde_json::to_string(&none.to_json()).unwrap(),
            r#"{"reason":null,"enqueued":false}"#
        );
    }
}
