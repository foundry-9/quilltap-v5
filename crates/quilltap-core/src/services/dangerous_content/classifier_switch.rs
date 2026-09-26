//! The classifier's switch (v4
//! `lib/services/dangerous-content/classifier-switch.ts`, NEW at `4d370a90f`,
//! #75) — the Concierge moving a chat to Unmoderated on the chat-level danger
//! classifier's verdict.
//!
//! In v4 the classifier job runs in the forked job child, where its writes are
//! buffered and a compare-and-set cannot report whether it landed; so the job
//! records only its telemetry and the decision to move the chat is made here,
//! in the parent, against the chat as it stands now — after the job dispatcher
//! commits the batch, or straight away when the job ran in the parent. v5 has
//! no forked child: the job runs in-process and calls this directly (v4's
//! parent branch), and the write applier's commit hook calls it for a batch
//! that carries a dangerous classification (`write_apply.rs`).

use crate::db::chats_read;
use crate::db::runtime::Db;
use crate::services::concierge_notifications::ConciergeDangerDetails;

use super::chat_override::{get_concierge_state, is_classifier_on_duty, ConciergeState};
use super::manual_flip::{
    apply_concierge_flip_with, ApplyConciergeFlipOptions, ConciergeAnnouncer, FlipBy, FlipReason,
};
use super::refusal_ledger::is_job_child;

/// v4 `maybeSwitchAfterClassification(chatId, verdict)`. Moves a chat to
/// Unmoderated on a dangerous verdict, if it is still Moderated. Re-reads the
/// chat, so an operator's newer choice (Locked, or Unmoderated by hand) is left
/// alone, and `apply_concierge_flip`'s compare-and-set closes the gap between
/// that read and the write. Never fails; answers `switched`. Logger
/// `ConciergeClassifierSwitch`.
pub async fn maybe_switch_after_classification<An: ConciergeAnnouncer>(
    db: &Db,
    announcer: &An,
    chat_id: &str,
    verdict: Option<&ConciergeDangerDetails>,
) -> bool {
    // Dead by construction in v5 (v4 `4d370a90f`'s job-child refusal): v5's
    // job runner is in-process, so `is_job_child()` is constantly false. The
    // arm is kept so the branch stays visible beside v4's.
    if is_job_child() {
        tracing::warn!(
            target: "quilltap::concierge_classifier_switch",
            chat_id = %chat_id,
            "Classifier switch refused in the job child; the parent decides"
        );
        return false;
    }

    let chat = match db.read_main(|c| chats_read::find_by_id(c, chat_id)) {
        Ok(Some(chat)) => chat,
        Ok(None) => {
            tracing::debug!(
                target: "quilltap::concierge_classifier_switch",
                chat_id = %chat_id,
                "Classifier switch skipped: chat not found"
            );
            return false;
        }
        Err(error) => return failed(chat_id, &error.to_string()),
    };
    if !is_classifier_on_duty(Some(&chat)) {
        tracing::info!(
            target: "quilltap::concierge_classifier_switch",
            chat_id = %chat_id,
            state = get_concierge_state(Some(&chat)).as_str(),
            "Classifier switch skipped: the chat is no longer Moderated"
        );
        return false;
    }

    let options = ApplyConciergeFlipOptions {
        by: FlipBy::Concierge,
        reason: Some(FlipReason::Classifier),
        refusals: None,
        classification: verdict.cloned(),
    };
    match apply_concierge_flip_with(
        db,
        announcer,
        chat_id,
        ConciergeState::Unmoderated,
        &chat,
        &options,
    )
    .await
    {
        Ok(result) => {
            tracing::info!(
                target: "quilltap::concierge_classifier_switch",
                chat_id = %chat_id,
                switched = result.changed,
                "Classifier verdict applied"
            );
            result.changed
        }
        Err(error) => failed(chat_id, &error.to_string()),
    }
}

/// v4's catch: `logger.error('Classifier switch failed', { chatId, error })`.
fn failed(chat_id: &str, error: &str) -> bool {
    tracing::error!(
        target: "quilltap::concierge_classifier_switch",
        chat_id = %chat_id,
        error = %error,
        "Classifier switch failed"
    );
    false
}
