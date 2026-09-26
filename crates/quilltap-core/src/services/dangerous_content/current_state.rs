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
/// Concierge state; falls back to `snapshot` when there is no chat to read or
/// the read fails, and never fails. A missing snapshot reads as Moderated,
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
    match db.read_main(|c| chats_read::find_by_id(c, chat_id)) {
        Ok(None) => {
            tracing::debug!(
                target: "quilltap::concierge_current_state",
                chat_id = %chat_id,
                snapshot = fallback.as_str(),
                "Current Concierge state: chat not found; using the snapshot"
            );
            fallback
        }
        Ok(Some(chat)) => {
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
        Err(error) => {
            tracing::warn!(
                target: "quilltap::concierge_current_state",
                chat_id = %chat_id,
                snapshot = fallback.as_str(),
                error = %error,
                "Could not re-read the Concierge state; using the snapshot"
            );
            fallback
        }
    }
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
