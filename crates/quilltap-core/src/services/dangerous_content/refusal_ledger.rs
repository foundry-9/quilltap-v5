//! The Concierge's refusal ledger (port of v4
//! `lib/services/dangerous-content/refusal-ledger.ts`, NEW at `49059fb14`,
//! #74): count the STATED moderation refusals a chat has earned, and — after
//! `autoSwitchAfterRefusals` of them on a Moderated chat (0 = never, and always
//! 0 off duty — the policy folds both into one number, v4 `3b463d6b1`) —
//! switch it to Unmoderated, announcing why (the three states, v4 `4d370a90f`;
//! Monitored → Flagged at `49059fb14`).
//!
//! [`record_moderation_refusal`] is the ledger's only writer of the increment;
//! [`crate::services::dangerous_content::manual_flip::apply_concierge_flip_with`]
//! is the only one that resets it. Both entry points of the switch rule meet in
//! [`maybe_auto_switch_after_refusal`]: an in-process record, and (the ported
//! post-commit hook in `write_apply`) a replayed child batch that incremented
//! a chat's ledger.
//!
//! ## The job-child split has no v5 analogue
//!
//! v4 runs background jobs in a forked child whose writes are buffered and
//! replayed by the parent, so the child records the increment and leaves the
//! DECISION to the parent's commit hook (`isJobChild()` →
//! `QUILLTAP_JOB_CHILD === '1'`). v5's job runner is IN-PROCESS: there is no
//! child, [`is_job_child`] is constantly `false`, and every record decides
//! here. The function is kept (not inlined away) so the two child-only arms
//! remain visible, and so the `M10` mutation proof can flip it.
//!
//! ## Per-chat chaining
//!
//! v4 chains the checks per chat in a process-wide map, so two refusals landing
//! together (a text turn and an image job, say) cannot both read the chat as
//! Moderated before either flip lands and announce twice. v5 holds a per-chat
//! async lock for the same span; with the single writer ordering the writes,
//! the second check then reads the first one's outcome.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use serde_json::Value;

use crate::db::runtime::Db;

use super::chat_override::{get_concierge_state, is_classifier_on_duty, ConciergeState};
use super::manual_flip::{
    apply_concierge_flip_with, ApplyConciergeFlipOptions, FlipBy, FlipReason,
    RealConciergeAnnouncer,
};
use super::refusal::RefusalEvidence;
use super::resolver::resolve_concierge_settings;

/// What was refused (v4 `RefusalRecord.kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefusalKind {
    Text,
    Image,
}

impl RefusalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RefusalKind::Text => "text",
            RefusalKind::Image => "image",
        }
    }
}

/// Which surface was refused (v4 `RefusalRecord.purpose`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefusalPurpose {
    Chat,
    Cheap,
    Tool,
    Lantern,
    Avatar,
    Dialog,
}

impl RefusalPurpose {
    pub fn as_str(self) -> &'static str {
        match self {
            RefusalPurpose::Chat => "chat",
            RefusalPurpose::Cheap => "cheap",
            RefusalPurpose::Tool => "tool",
            RefusalPurpose::Lantern => "lantern",
            RefusalPurpose::Avatar => "avatar",
            RefusalPurpose::Dialog => "dialog",
        }
    }
}

/// v4 `RefusalRecord`.
#[derive(Clone, Debug)]
pub struct RefusalRecord {
    pub chat_id: String,
    pub kind: RefusalKind,
    pub purpose: RefusalPurpose,
    pub refused_profile_id: String,
    pub refused_profile_name: String,
    pub provider: String,
    pub model_name: Option<String>,
    pub evidence: Option<RefusalEvidence>,
    /// Whether the Concierge's reroute answered in the end — for the log only.
    pub rerouted: bool,
}

/// v4 `RecordRefusalResult`: the chat's count after this refusal (`None` when
/// nothing was recorded), and whether it switched the chat to Unmoderated.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecordRefusalResult {
    pub count: Option<i64>,
    pub switched: bool,
}

/// The last refusing provider, carried to the auto-switch announcement (v4
/// `LastRefusal`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LastRefusal {
    pub provider: String,
    pub model_name: Option<String>,
}

/// v4 `isRecordableRefusalEvidence`: the evidence that counts as a STATED
/// refusal. `inferred` never does — an empty body on flagged content is the
/// Concierge's own reading, not the provider's word.
pub fn is_recordable_refusal_evidence(evidence: Option<RefusalEvidence>) -> bool {
    matches!(
        evidence,
        Some(
            RefusalEvidence::TypedError
                | RefusalEvidence::ProviderCode
                | RefusalEvidence::FinishReason
                | RefusalEvidence::MessagePattern
        )
    )
}

/// v4 `isJobChild()` — constantly `false` in v5 (see the module doc).
pub(super) fn is_job_child() -> bool {
    false
}

/// v4's `switchChecks`, as per-chat async locks.
static SWITCH_CHECKS: LazyLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A seam between the auto-switch check's threshold decision and its
/// re-read — v4's own test (`does not overwrite an operator state set while
/// the check was reading`) plants an operator flip exactly there, and a real-DB
/// differential can only reproduce that with a hook at the same point.
/// Production passes [`NoAutoSwitchProbe`].
pub trait AutoSwitchProbe: Sync {
    fn before_reread<'a>(
        &'a self,
        _db: &'a Db,
        _chat_id: &'a str,
    ) -> impl std::future::Future<Output = ()> + Send + 'a {
        async {}
    }
}

/// The production [`AutoSwitchProbe`]: nothing between the reads.
pub struct NoAutoSwitchProbe;
impl AutoSwitchProbe for NoAutoSwitchProbe {}

macro_rules! ledger_line {
    ($level:ident, $rec:expr, $msg:literal $(, $k:ident = $v:expr)*) => {
        tracing::$level!(
            target: "quilltap::concierge_refusal_ledger",
            chat_id = %$rec.chat_id,
            kind = $rec.kind.as_str(),
            purpose = $rec.purpose.as_str(),
            refused_profile_id = %$rec.refused_profile_id,
            refused_profile_name = %$rec.refused_profile_name,
            provider = %$rec.provider,
            model_name = $rec.model_name.as_deref(),
            evidence = $rec.evidence.map(RefusalEvidence::as_str),
            rerouted = $rec.rerouted,
            $($k = $v,)*
            $msg
        )
    };
}

/// v4 `recordModerationRefusal` — record one moderation refusal on its chat,
/// and ask whether it earns the auto-switch. **Never fails**: a ledger failure
/// must not fail the call that was refused.
pub async fn record_moderation_refusal(db: &Db, rec: &RefusalRecord) -> RecordRefusalResult {
    record_moderation_refusal_with(db, rec, &NoAutoSwitchProbe).await
}

/// [`record_moderation_refusal`] with the check's probe (tests only need it).
pub async fn record_moderation_refusal_with<P: AutoSwitchProbe>(
    db: &Db,
    rec: &RefusalRecord,
    probe: &P,
) -> RecordRefusalResult {
    if rec.chat_id.is_empty() {
        ledger_line!(debug, rec, "Refusal not recorded: no chat to record it on");
        return RecordRefusalResult::default();
    }
    if !is_recordable_refusal_evidence(rec.evidence) {
        ledger_line!(
            debug,
            rec,
            "Refusal not recorded: the evidence is not a stated refusal"
        );
        return RecordRefusalResult::default();
    }

    let at = crate::clock::now_iso();
    let refused_by = LastRefusal {
        provider: rec.provider.clone(),
        model_name: rec.model_name.clone(),
    };
    let chat_id = rec.chat_id.clone();
    let provider = rec.provider.clone();
    let model = rec.model_name.clone();
    let written = db
        .write(move |w| {
            Ok(w.main().chats().increment_moderation_refusal_count(
                &chat_id,
                &at,
                Some((provider.as_str(), model.as_deref())),
            ))
        })
        .await;
    let count = match written {
        Ok(count) => count,
        Err(e) => {
            ledger_line!(
                error,
                rec,
                "Failed to record a moderation refusal",
                error = e.to_string()
            );
            return RecordRefusalResult::default();
        }
    };

    if is_job_child() {
        ledger_line!(
            info,
            rec,
            "Moderation refusal recorded (buffered; the parent decides on the auto-switch)"
        );
        return RecordRefusalResult::default();
    }

    ledger_line!(info, rec, "Moderation refusal recorded", count = count);
    let switched =
        maybe_auto_switch_after_refusal_with(db, &rec.chat_id, Some(&refused_by), probe).await;
    RecordRefusalResult {
        count: Some(count),
        switched,
    }
}

/// v4 `maybeAutoSwitchAfterRefusal` — switch a Moderated chat to Unmoderated if its
/// ledger has reached the threshold. Idempotent (a chat that is not Moderated
/// is left alone, and the flip is a no-op on a match); never fails.
pub async fn maybe_auto_switch_after_refusal(
    db: &Db,
    chat_id: &str,
    last_refusal: Option<&LastRefusal>,
) -> bool {
    maybe_auto_switch_after_refusal_with(db, chat_id, last_refusal, &NoAutoSwitchProbe).await
}

/// [`maybe_auto_switch_after_refusal`] with the check's probe.
pub async fn maybe_auto_switch_after_refusal_with<P: AutoSwitchProbe>(
    db: &Db,
    chat_id: &str,
    last_refusal: Option<&LastRefusal>,
    probe: &P,
) -> bool {
    if is_job_child() {
        tracing::warn!(
            target: "quilltap::concierge_refusal_ledger",
            chat_id = %chat_id,
            "Auto-switch check refused in the job child; the parent decides"
        );
        return false;
    }

    let lock = {
        let mut map = SWITCH_CHECKS.lock().unwrap_or_else(|e| e.into_inner());
        map.entry(chat_id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    };
    let switched = {
        let _held = lock.lock().await;
        run_auto_switch_check(db, chat_id, last_refusal, probe).await
    };
    // v4's `finally`: forget the chain once nobody else is waiting on it.
    let mut map = SWITCH_CHECKS.lock().unwrap_or_else(|e| e.into_inner());
    if Arc::strong_count(&lock) <= 2 {
        map.remove(chat_id);
    }
    switched
}

/// v4 `runAutoSwitchCheck`, in v4's order: chat → Moderated → policy →
/// threshold → ledger; the RE-READ only after `count >= threshold`;
/// `switched = result.changed`. v4 `3b463d6b1` (#76): the threshold is the
/// policy's `autoSwitchAfterRefusals`, which folds "the auto-switch is off",
/// "the Concierge is off duty" and "the chat is not Moderated" into one `0`,
/// so the separate mode gate is gone.
async fn run_auto_switch_check<P: AutoSwitchProbe>(
    db: &Db,
    chat_id: &str,
    last_refusal: Option<&LastRefusal>,
    probe: &P,
) -> bool {
    let check = async {
        let id = chat_id.to_string();
        let chat = db.read_main(move |c| crate::db::chats_read::find_by_id(c, &id))?;
        let Some(chat) = chat else {
            tracing::debug!(
                target: "quilltap::concierge_refusal_ledger",
                chat_id = %chat_id,
                "Auto-switch check skipped: chat not found"
            );
            return Ok::<bool, crate::db::DbError>(false);
        };

        // v4 `4d370a90f` (#75): re-keyed on `isClassifierOnDuty` — only a
        // Moderated chat is the Concierge's to move.
        let state = get_concierge_state(Some(&chat));
        if !is_classifier_on_duty(Some(&chat)) {
            tracing::debug!(
                target: "quilltap::concierge_refusal_ledger",
                chat_id = %chat_id,
                state = state.as_str(),
                "Auto-switch check skipped: the chat is not Moderated"
            );
            return Ok(false);
        }

        let user_id = chat
            .get("userId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let chat_settings =
            db.read_main(move |c| crate::db::chat_settings::find_by_user_id(c, &user_id))?;
        let concierge_policy = resolve_concierge_settings(chat_settings.as_ref(), Some(&chat));
        let threshold = concierge_policy.auto_switch_after_refusals;
        let id = chat_id.to_string();
        let count = db
            .read_main(move |c| {
                Ok(crate::db::chats::ChatsRepository::new(c).get_moderation_refusal_ledger(&id))
            })?
            .count;
        let concierge_source = concierge_policy.source.as_str();
        let on_duty = concierge_policy.on_duty;

        macro_rules! decision {
            ($level:ident, $msg:literal $(, $k:ident = $v:expr)*) => {
                tracing::$level!(
                    target: "quilltap::concierge_refusal_ledger",
                    chat_id = %chat_id,
                    count = count,
                    threshold = threshold,
                    concierge_source = concierge_source,
                    on_duty = on_duty,
                    $($k = $v,)*
                    $msg
                )
            };
        }

        if threshold <= 0 {
            decision!(
                debug,
                "Auto-switch check: the auto-switch is off for this chat"
            );
            return Ok(false);
        }
        if count < threshold {
            decision!(debug, "Auto-switch check: below the threshold");
            return Ok(false);
        }

        // The settings and ledger reads above awaited; the operator may have
        // moved the chat meanwhile. Re-read and re-check on the row the flip
        // will actually compare against, so a newer operator choice (Locked,
        // Unmoderated) is never overwritten by a decision made on a stale
        // snapshot.
        probe.before_reread(db, chat_id).await;
        let id = chat_id.to_string();
        let fresh = db.read_main(move |c| crate::db::chats_read::find_by_id(c, &id))?;
        let fresh_state = fresh.as_ref().map(|f| get_concierge_state(Some(f)));
        let Some(fresh) = fresh.filter(|f| is_classifier_on_duty(Some(f))) else {
            decision!(
                info,
                "Auto-switch abandoned: the chat left Moderated during the check",
                // v4 logs `state: null` for a vanished chat.
                state = fresh_state.map_or("null", ConciergeState::as_str)
            );
            return Ok(false);
        };

        let last_provider = last_refusal.map(|l| l.provider.clone()).unwrap_or_default();
        let last_model = last_refusal.and_then(|l| l.model_name.clone());
        let result = apply_concierge_flip_with(
            db,
            &RealConciergeAnnouncer { db },
            chat_id,
            ConciergeState::Unmoderated,
            &fresh,
            &ApplyConciergeFlipOptions {
                by: FlipBy::Concierge,
                reason: Some(FlipReason::Refusals),
                refusals: Some(
                    crate::services::concierge_notifications::ConciergeAutoFlagDetails {
                        count,
                        last_provider,
                        last_model: last_model.clone(),
                    },
                ),
                classification: None,
            },
        )
        .await?;

        decision!(
            info,
            "The Concierge switched a chat to Unmoderated after repeated refusals",
            changed = result.changed,
            last_provider = last_refusal.map(|l| l.provider.as_str()),
            last_model = last_model.as_deref()
        );
        Ok(result.changed)
    };

    match check.await {
        Ok(switched) => switched,
        Err(e) => {
            tracing::error!(
                target: "quilltap::concierge_refusal_ledger",
                chat_id = %chat_id,
                error = %e,
                "Auto-switch check failed"
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PEPPER: &str = "ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=";

    #[test]
    fn only_the_four_stated_evidences_are_recordable() {
        for (e, want) in [
            (Some(RefusalEvidence::TypedError), true),
            (Some(RefusalEvidence::ProviderCode), true),
            (Some(RefusalEvidence::FinishReason), true),
            (Some(RefusalEvidence::MessagePattern), true),
            (Some(RefusalEvidence::Inferred), false),
            (None, false),
        ] {
            assert_eq!(is_recordable_refusal_evidence(e), want, "{e:?}");
        }
    }

    /// v4's outer catch: a check whose reads THROW logs `Auto-switch check
    /// failed` and answers `false` (the tier-3 family cannot reach it — its
    /// fixture always has `chats`).
    #[test]
    fn a_failed_check_logs_the_error_and_never_switches() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.db");
        drop(crate::db::Writer::open_writable(&path, PEPPER).unwrap());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let db = Db::open_main(&path, PEPPER).unwrap();
        let (switched, lines) = crate::test_support::captured_with(|| {
            rt.block_on(maybe_auto_switch_after_refusal(&db, "c1", None))
        });
        assert!(!switched);
        let ours: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("quilltap::concierge_refusal_ledger"))
            .collect();
        assert_eq!(ours.len(), 1, "{lines:?}");
        assert!(
            ours[0].starts_with(
                "ERROR quilltap::concierge_refusal_ledger Auto-switch check failed chat_id=c1 error="
            ),
            "{ours:?}"
        );
        assert!(
            SWITCH_CHECKS.lock().unwrap().get("c1").is_none(),
            "the chain is forgotten once nobody waits on it"
        );
    }
}
