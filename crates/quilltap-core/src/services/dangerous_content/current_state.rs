//! The chat's Concierge state as it stands *now* (v4
//! `lib/services/dangerous-content/current-state.ts`, NEW at `4d370a90f`, #75)
//! — for decisions taken at the moment a provider refuses, not when the
//! request began.
//!
//! A turn or a picture reads its chat once, before a provider call that can
//! take many seconds. If the operator locks the chat meanwhile, a failover gate
//! that trusts that snapshot would send the refused request to the uncensored
//! desk anyway, which is exactly what Locked promises never happens. The
//! failover chokepoints ask this instead of the snapshot.

use crate::db::chats_read;
use crate::db::runtime::Db;

use super::chat_override::{get_concierge_state, ConciergeState};

/// v4 `readCurrentConciergeState(chatId, snapshot?)`. Re-reads the chat's
/// Concierge state; falls back to `snapshot` when there is no chat to read (a
/// failed read is the repository's not-found, as in v4), and never fails. A missing snapshot reads as Moderated,
/// like a missing column. Logger `ConciergeCurrentState`.
pub fn read_current_concierge_state(
    db: &Db,
    chat_id: Option<&str>,
    snapshot: Option<ConciergeState>,
) -> ConciergeState {
    let fallback = snapshot.unwrap_or(ConciergeState::Moderated);
    // v4 `if (!chatId) return fallback` — JS falsiness: an empty id too.
    let Some(chat_id) = chat_id.filter(|id| !id.is_empty()) else {
        return fallback;
    };
    // v4 reads through `repos.chats.findById`, a fallback `safeQuery` that logs
    // and answers `null` on a failed read — so a read error IS the not-found
    // arm; v4's own `catch` here is unreachable (the unification review).
    let chat = crate::db::fallback::find_by_id_or_none("chats", chat_id, || {
        db.read_main(|c| Ok(chats_read::find_by_id_or_none(c, chat_id)))
    });
    let Some(chat) = chat else {
        tracing::debug!(
            target: "quilltap::concierge_current_state",
            chat_id = %chat_id,
            snapshot = fallback.as_str(),
            "Current Concierge state: chat not found; using the snapshot"
        );
        return fallback;
    };
    let state = get_concierge_state(Some(&chat));
    if state != fallback {
        tracing::info!(
            target: "quilltap::concierge_current_state",
            chat_id = %chat_id,
            snapshot = fallback.as_str(),
            current = state.as_str(),
            "Concierge state changed since the request began"
        );
    }
    state
}

/// v4 `readCurrentConciergeOnDuty(userId, snapshot)` (`3b463d6b1`, #76) —
/// whether the Concierge is on duty *now*, the global half of the
/// refusal-time check. A policy resolved when the turn began says
/// `failover_allowed`; if the operator switched the Concierge off while the
/// provider was thinking, the refusal must not reach the uncensored desk.
/// Falls back to `snapshot` when there is no user or the read fails, and
/// never fails. Logger `ConciergeCurrentState`.
pub fn read_current_concierge_on_duty(db: &Db, user_id: Option<&str>, snapshot: bool) -> bool {
    // v4 `if (!userId) return snapshot` — JS falsiness: an empty id too.
    let Some(user_id) = user_id.filter(|id| !id.is_empty()) else {
        return snapshot;
    };
    let uid = user_id.to_string();
    match db.read_main(move |c| crate::db::chat_settings::find_by_user_id(c, &uid)) {
        Ok(settings) => {
            let on_duty = super::resolver::read_concierge_settings(settings.as_ref()).enabled;
            if on_duty != snapshot {
                tracing::info!(
                    target: "quilltap::concierge_current_state",
                    user_id = %user_id,
                    snapshot,
                    current = on_duty,
                    "Concierge on-duty switch changed since the request began"
                );
            }
            on_duty
        }
        Err(error) => {
            tracing::warn!(
                target: "quilltap::concierge_current_state",
                user_id = %user_id,
                snapshot,
                error = %error,
                "Could not re-read the Concierge on-duty switch; using the snapshot"
            );
            snapshot
        }
    }
}

#[cfg(test)]
mod tests {
    //! P4.D227 (v4 `3b463d6b1`, #76): `readCurrentConciergeOnDuty`'s three
    //! exits and its two lines — the INFO only when the switch MOVED, the WARN
    //! (and the snapshot) when the read fails, silence with no user.
    use super::*;
    use crate::test_support::captured_with;

    fn db_with(settings: Option<&str>) -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_main(
            dir.path().join("main.db"),
            "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=",
        )
        .unwrap();
        if let Some(cell) = settings {
            // The re-dumped fresh `chat_settings` DDL + a row through the real
            // create path, then the cell under test.
            let schema: serde_json::Value =
                serde_json::from_str(include_str!("../provisioning/fresh_schema.json")).unwrap();
            let ddl = schema["main"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(serde_json::Value::as_str)
                .find(|s| s.starts_with("CREATE TABLE \"chat_settings\" ("))
                .unwrap()
                .to_string();
            let cell = cell.to_string();
            db.write_blocking(move |w| {
                let c = w.main().connection();
                c.execute_batch(&ddl)?;
                crate::db::chat_settings::update_for_user(
                    c,
                    "u1",
                    &[],
                    "2026-09-26T00:00:00.000Z",
                )?;
                c.execute("UPDATE chat_settings SET conciergeSettings = ?1", [cell])?;
                Ok(())
            })
            .unwrap();
        }
        (dir, db)
    }

    #[test]
    fn a_moved_switch_is_logged_and_answered() {
        let (_d, db) = db_with(Some("{\"enabled\":false}"));
        let (got, lines) = captured_with(|| read_current_concierge_on_duty(&db, Some("u1"), true));
        assert!(!got, "the current switch, not the snapshot");
        let line = lines
            .iter()
            .find(|l| l.contains("Concierge on-duty switch changed since the request began"))
            .unwrap_or_else(|| panic!("the INFO: {lines:#?}"));
        assert!(
            line.starts_with("INFO quilltap::concierge_current_state"),
            "{line}"
        );
        assert!(
            line.contains("user_id=u1")
                && line.contains("snapshot=true")
                && line.contains("current=false"),
            "{line}"
        );
    }

    #[test]
    fn an_unmoved_switch_is_silent() {
        let (_d, db) = db_with(Some("{\"enabled\":true}"));
        let (got, lines) = captured_with(|| read_current_concierge_on_duty(&db, Some("u1"), true));
        assert!(got);
        assert!(
            lines.iter().all(|l| !l.contains("on-duty switch")),
            "{lines:#?}"
        );
    }

    #[test]
    fn a_failed_read_warns_and_keeps_the_snapshot() {
        // No `chat_settings` table at all: the read errors.
        let (_d, db) = db_with(None);
        for snapshot in [true, false] {
            let (got, lines) =
                captured_with(|| read_current_concierge_on_duty(&db, Some("u1"), snapshot));
            assert_eq!(got, snapshot);
            let line = lines
                .iter()
                .find(|l| {
                    l.contains("Could not re-read the Concierge on-duty switch; using the snapshot")
                })
                .unwrap_or_else(|| panic!("the WARN: {lines:#?}"));
            assert!(
                line.starts_with("WARN quilltap::concierge_current_state"),
                "{line}"
            );
            assert!(line.contains(&format!("snapshot={snapshot}")), "{line}");
        }
    }

    // --- v4 `4d370a90f` `current-state.test.ts`, mirrored (added at the
    // round's unification, where the review found `read_current_concierge_state`
    // untested) over a real provisioned instance.

    const CHAT: &str = "c1000000-0000-4000-8000-0000000000c6";

    fn chat_db(mode: Option<&'static str>) -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("rs");
        std::fs::create_dir_all(&data).unwrap();
        let pepper = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
        crate::services::provisioning::provision_fresh_instance(&data, pepper).unwrap();
        let db = Db::open(
            crate::db::runtime::DbPaths {
                main: data.join("quilltap.db"),
                mount_index: Some(data.join("quilltap-mount-index.db")),
                llm_logs: None,
            },
            pepper,
        )
        .unwrap();
        let create: crate::db::chats::ChatCreate = serde_json::from_value(serde_json::json!({
            "userId": crate::api::SINGLE_USER_ID,
            "title": "The Current Room",
            "participants": [],
        }))
        .unwrap();
        let opts = crate::db::chats::CreateOptions {
            id: CHAT.to_string(),
            created_at: "2026-09-28T00:00:00.000Z".to_string(),
            updated_at: "2026-09-28T00:00:00.000Z".to_string(),
        };
        db.write_blocking(move |w| {
            w.main().chats().create(&create, &opts)?;
            if let Some(mode) = mode {
                w.main().connection().execute(
                    "UPDATE chats SET conciergeMode = ?1 WHERE id = ?2",
                    rusqlite::params![mode, CHAT],
                )?;
            }
            Ok(())
        })
        .unwrap();
        (dir, db)
    }

    /// v4: "prefers the stored state over the snapshot" — and logs the move.
    #[test]
    fn the_stored_state_wins_over_the_snapshot() {
        let (_d, db) = chat_db(Some("locked"));
        let (got, lines) = captured_with(|| {
            read_current_concierge_state(&db, Some(CHAT), Some(ConciergeState::Unmoderated))
        });
        assert_eq!(got, ConciergeState::Locked);
        let line = lines
            .iter()
            .find(|l| l.contains("Concierge state changed since the request began"))
            .unwrap_or_else(|| panic!("the INFO: {lines:#?}"));
        assert!(
            line.starts_with("INFO quilltap::concierge_current_state"),
            "{line}"
        );
        assert!(
            line.contains("snapshot=unmoderated") && line.contains("current=locked"),
            "{line}"
        );
        // Unchanged → silent.
        let (same, quiet) = captured_with(|| {
            read_current_concierge_state(&db, Some(CHAT), Some(ConciergeState::Locked))
        });
        assert_eq!(same, ConciergeState::Locked);
        assert!(quiet.is_empty(), "{quiet:#?}");
    }

    /// v4: "falls back to the snapshot without a chat id" (silently).
    #[test]
    fn no_chat_id_answers_the_snapshot_silently() {
        let (_d, db) = chat_db(Some("locked"));
        for id in [None, Some("")] {
            let (got, lines) = captured_with(|| {
                read_current_concierge_state(&db, id, Some(ConciergeState::Unmoderated))
            });
            assert_eq!(got, ConciergeState::Unmoderated);
            assert!(lines.is_empty(), "{lines:#?}");
        }
    }

    /// v4: "falls back to the snapshot when the chat is gone".
    #[test]
    fn a_missing_chat_answers_the_snapshot() {
        let (_d, db) = chat_db(None);
        let (got, lines) = captured_with(|| {
            read_current_concierge_state(
                &db,
                Some("no-such-chat"),
                Some(ConciergeState::Unmoderated),
            )
        });
        assert_eq!(got, ConciergeState::Unmoderated);
        let line = lines
            .iter()
            .find(|l| l.contains("Current Concierge state: chat not found; using the snapshot"))
            .unwrap_or_else(|| panic!("the DEBUG: {lines:#?}"));
        assert!(
            line.starts_with("DEBUG quilltap::concierge_current_state"),
            "{line}"
        );
        assert!(line.contains("snapshot=unmoderated"), "{line}");
    }

    /// v4 "falls back when the read fails": its test mocks `findById` to
    /// throw, but the REAL repository is a fallback `safeQuery` — the failed
    /// read logs the repository's ERROR and takes the not-found arm, so no
    /// "Could not re-read" line exists on either side.
    #[test]
    fn a_failed_read_is_the_repository_fallback() {
        let (_d, db) = db_with(None); // no `chats` table at all
        let (got, lines) = captured_with(|| {
            read_current_concierge_state(&db, Some(CHAT), Some(ConciergeState::Unmoderated))
        });
        assert_eq!(got, ConciergeState::Unmoderated);
        let err = lines
            .iter()
            .find(|l| l.contains("Error finding entity by ID"))
            .unwrap_or_else(|| panic!("the repository ERROR: {lines:#?}"));
        assert!(
            err.starts_with("ERROR quilltap::db ") && err.contains("collection=chats"),
            "{err}"
        );
        assert!(lines
            .iter()
            .any(|l| l.contains("Current Concierge state: chat not found; using the snapshot")));
    }

    /// v4: "a missing snapshot reads as Moderated".
    #[test]
    fn a_missing_snapshot_reads_as_moderated() {
        let (_d, db) = chat_db(None);
        let got = read_current_concierge_state(&db, None, None);
        assert_eq!(got, ConciergeState::Moderated);
        let (stored, _) = captured_with(|| read_current_concierge_state(&db, Some(CHAT), None));
        assert_eq!(
            stored,
            ConciergeState::Moderated,
            "a NULL column reads as Moderated"
        );
    }

    #[test]
    fn no_user_answers_the_snapshot_silently() {
        let (_d, db) = db_with(None);
        for user in [None, Some("")] {
            let (got, lines) = captured_with(|| read_current_concierge_on_duty(&db, user, false));
            assert!(!got);
            assert!(lines.is_empty(), "{lines:#?}");
        }
    }
}
