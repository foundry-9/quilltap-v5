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

    // v4 reads through `repos.chats.findById`, a fallback `safeQuery`: a failed
    // read logs `Error finding entity by ID` and answers `null`, so it takes the
    // not-found arm below — never this function's catch (the unification review).
    let chat = crate::db::fallback::find_by_id_or_none("chats", chat_id, || {
        db.read_main(|c| Ok(chats_read::find_by_id_or_none(c, chat_id)))
    });
    let Some(chat) = chat else {
        tracing::debug!(
            target: "quilltap::concierge_classifier_switch",
            chat_id = %chat_id,
            "Classifier switch skipped: chat not found"
        );
        return false;
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

#[cfg(test)]
mod tests {
    //! v4 `4d370a90f` `__tests__/unit/lib/services/dangerous-content/
    //! classifier-switch.test.ts`, mirrored as planted-row arms over a real
    //! provisioned instance (P4.D226 Tier 1 item 5 — added at the round's
    //! unification, where the review found the module untested), plus the
    //! repository's fallback read (a failed read is v4's not-found arm) and
    //! the order's double-switch pin.
    use super::*;
    use crate::db::runtime::DbPaths;
    use crate::services::dangerous_content::manual_flip::NoConciergeAnnouncer;
    use serde_json::json;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAT: &str = "c1000000-0000-4000-8000-0000000000c5";
    const NOW: &str = "2026-09-28T00:00:00.000Z";

    fn provisioned() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("rs");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let db = Db::open(
            DbPaths {
                main: data.join("quilltap.db"),
                mount_index: Some(data.join("quilltap-mount-index.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        (dir, db)
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    /// Seed the chat, then plant its stored Concierge mode (`None` = the
    /// column left NULL, which reads as Moderated).
    fn seed_chat(rt: &tokio::runtime::Runtime, db: &Db, mode: Option<&'static str>) {
        rt.block_on(async {
            let create: crate::db::chats::ChatCreate = serde_json::from_value(json!({
                "userId": crate::api::SINGLE_USER_ID,
                "title": "The Classified Room",
                "participants": [],
            }))
            .unwrap();
            let opts = crate::db::chats::CreateOptions {
                id: CHAT.to_string(),
                created_at: NOW.to_string(),
                updated_at: NOW.to_string(),
            };
            db.write(move |w| {
                w.main().chats().create(&create, &opts)?;
                if let Some(mode) = mode {
                    w.main().connection().execute(
                        "UPDATE chats SET conciergeMode = ?1, conciergeModeSetBy = 'operator', \
                         conciergeModeReason = 'manual' WHERE id = ?2",
                        rusqlite::params![mode, CHAT],
                    )?;
                }
                Ok(())
            })
            .await
            .unwrap();
        });
    }

    fn stored(db: &Db) -> (Option<String>, Option<String>, Option<String>) {
        db.read_main(|c| {
            Ok(c.query_row(
                "SELECT conciergeMode, conciergeModeSetBy, conciergeModeReason FROM chats WHERE id = ?1",
                [CHAT],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?)
        })
        .unwrap()
    }

    fn switch(rt: &tokio::runtime::Runtime, db: &Db, chat_id: &str) -> (bool, Vec<String>) {
        crate::test_support::captured_with(|| {
            rt.block_on(maybe_switch_after_classification(
                db,
                &NoConciergeAnnouncer,
                chat_id,
                None,
            ))
        })
    }

    fn line<'a>(lines: &'a [String], needle: &str) -> &'a str {
        lines
            .iter()
            .find(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("no line containing {needle:?}: {lines:#?}"))
    }

    /// v4: "moves a Moderated chat to Unmoderated, by the Concierge, on the
    /// classifier's word" — and says so.
    #[test]
    fn moves_a_moderated_chat() {
        let (_d, db) = provisioned();
        let rt = rt();
        seed_chat(&rt, &db, None);
        let (switched, lines) = switch(&rt, &db, CHAT);
        assert!(switched);
        assert_eq!(
            stored(&db),
            (
                Some("unmoderated".into()),
                Some("concierge".into()),
                Some("classifier".into())
            )
        );
        let l = line(&lines, "Classifier verdict applied");
        assert!(
            l.starts_with("INFO quilltap::concierge_classifier_switch"),
            "{l}"
        );
        assert!(l.contains("switched=true"), "{l}");
    }

    /// v4 `it.each(['locked', 'unmoderated'])`: "leaves a chat the operator
    /// has already decided about alone".
    #[test]
    fn leaves_locked_and_unmoderated_alone() {
        for mode in ["locked", "unmoderated"] {
            let (_d, db) = provisioned();
            let rt = rt();
            seed_chat(&rt, &db, Some(mode));
            let (switched, lines) = switch(&rt, &db, CHAT);
            assert!(!switched, "{mode}");
            assert_eq!(stored(&db).0.as_deref(), Some(mode));
            let l = line(
                &lines,
                "Classifier switch skipped: the chat is no longer Moderated",
            );
            assert!(
                l.starts_with("INFO quilltap::concierge_classifier_switch"),
                "{l}"
            );
            assert!(l.contains(&format!("state={mode}")), "{l}");
            assert!(lines
                .iter()
                .all(|l| !l.contains("Classifier verdict applied")));
        }
    }

    /// v4: "does nothing for a chat that no longer exists".
    #[test]
    fn a_missing_chat_is_skipped_quietly() {
        let (_d, db) = provisioned();
        let rt = rt();
        let (switched, lines) = switch(&rt, &db, CHAT);
        assert!(!switched);
        let l = line(&lines, "Classifier switch skipped: chat not found");
        assert!(
            l.starts_with("DEBUG quilltap::concierge_classifier_switch"),
            "{l}"
        );
        assert!(lines
            .iter()
            .all(|l| !l.contains("Classifier switch failed")));
    }

    /// v4 reads through `repos.chats.findById`, a fallback `safeQuery`: a
    /// failed read logs the repository's ERROR and takes the not-found arm —
    /// never the switch's own `Classifier switch failed` catch.
    #[test]
    fn a_failed_read_is_the_repository_fallback_not_the_catch() {
        let dir = tempfile::tempdir().unwrap();
        // A main partition with no `chats` table: the read errors.
        let db = Db::open_main(dir.path().join("main.db"), PEPPER).unwrap();
        let rt = rt();
        let (switched, lines) = switch(&rt, &db, CHAT);
        assert!(!switched);
        let err = line(&lines, "Error finding entity by ID");
        assert!(err.starts_with("ERROR quilltap::db "), "{err}");
        assert!(err.contains("collection=chats"), "{err}");
        line(&lines, "Classifier switch skipped: chat not found");
        assert!(
            lines
                .iter()
                .all(|l| !l.contains("Classifier switch failed")),
            "{lines:#?}"
        );
    }

    /// The order's double-switch pin: a second dangerous verdict finds the chat
    /// already Unmoderated and changes nothing (no second flip, no second
    /// announcement).
    #[test]
    fn a_second_verdict_does_not_switch_again() {
        let (_d, db) = provisioned();
        let rt = rt();
        seed_chat(&rt, &db, None);
        assert!(switch(&rt, &db, CHAT).0);
        let (again, lines) = switch(&rt, &db, CHAT);
        assert!(!again);
        line(
            &lines,
            "Classifier switch skipped: the chat is no longer Moderated",
        );
        assert_eq!(stored(&db).1.as_deref(), Some("concierge"));
    }
}
