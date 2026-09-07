//! Subprompt edits → the chats that carry them (v4
//! `lib/subprompts/chat-fanout.ts` — P4.D163 unit 3).
//!
//! A subprompt's text is baked into every chat's compiled identity stack for
//! the seats that have it ticked on, so an edit or a deletion has to reach
//! those chats or the running conversation keeps speaking from yesterday's
//! draft. This is the one place that does that fan-out:
//!
//! - **update** — recompile the stack of every LLM-controlled seat of this
//!   character whose selection includes the subprompt.
//! - **delete** — strip the id from those seats' selections first (so the
//!   record never points at a file that no longer exists), then recompile.
//!
//! Everything fails soft: a chat that cannot be recompiled logs and is left to
//! the read-through fallback, exactly as the compiler itself behaves.
//!
//! The recompile and the realtime publish are SEAMS ([`FanoutSeams`]) so the
//! storage differential can record the `(chatId, participantId)` calls on
//! both sides while P4.D164's block render is still landing (v4's own test
//! mocks the same two modules); production passes
//! [`ProductionFanoutSeams`], which calls the real compiler and the real bus.

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::db::chats_participants::ChatParticipantsRepository;
use crate::db::{chats_read, DbError};
use crate::realtime::bus::publish_realtime;
use crate::realtime::types::RealtimeTopic;
use crate::services::system_prompt_compiler::compile_identity_stack_for_participant;

/// The tracing target (v4 `createServiceLogger('Subprompts:Fanout')`).
pub const LOG_TARGET: &str = "quilltap::subprompts::fanout";

/// The two side effects the fan-out performs per seat / per chat.
pub trait FanoutSeams: Send + Sync {
    /// v4 `compileIdentityStackForParticipant(current, seat.id)`.
    fn compile(
        &self,
        main: &Connection,
        mount: &Connection,
        chat: &Value,
        participant_id: &str,
    ) -> Result<(), DbError>;
    /// v4 `publishRealtime('chats', chat.id)`.
    fn publish_chat(&self, chat_id: &str);
    /// The ROUTES' `publishRealtime('characters', characterId)` after every
    /// successful write — on the same seam so one recorder sees both topics.
    fn publish_character(&self, character_id: &str);
}

/// The real compiler + the real realtime bus.
pub struct ProductionFanoutSeams;

impl FanoutSeams for ProductionFanoutSeams {
    fn compile(
        &self,
        main: &Connection,
        mount: &Connection,
        chat: &Value,
        participant_id: &str,
    ) -> Result<(), DbError> {
        compile_identity_stack_for_participant(main, mount, chat, participant_id)
    }
    fn publish_chat(&self, chat_id: &str) {
        publish_realtime(RealtimeTopic::Chats, Some(chat_id));
    }
    fn publish_character(&self, character_id: &str) {
        publish_realtime(RealtimeTopic::Characters, Some(character_id));
    }
}

/// v4 `SubpromptFanoutResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SubpromptFanoutResult {
    /// Chats that had at least one seat carrying the subprompt.
    pub chats_touched: usize,
    /// Seats recompiled.
    pub seats_recompiled: usize,
}

/// v4 `options: { removeSelection?: boolean }`.
#[derive(Debug, Clone, Copy, Default)]
pub struct FanoutOptions {
    pub remove_selection: bool,
}

fn seat_str<'a>(seat: &'a Value, key: &str) -> Option<&'a str> {
    seat.get(key).and_then(Value::as_str)
}

fn seat_ids(seat: &Value) -> Vec<String> {
    seat.get("selectedSubpromptIds")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Recompile every seat of `character_id` that has `subprompt_id` in play.
/// With `remove_selection`, the id is dropped from each such seat's selection
/// before the recompile.
///
/// The seat filter is v4's four conjuncts: `characterId` match ∧ `controlledBy
/// !== 'user'` (an absent key is `undefined !== 'user'` — LLM) ∧ `status !==
/// 'removed'` ∧ `(selectedSubpromptIds ?? []).some(lower === wanted)`. The
/// per-seat try/catch wraps BOTH the strip and the recompile; a failure warns
/// and the loop continues with the next seat. v4's `updateParticipant` returns
/// the updated chat, which becomes `current` for the recompile; v5's returns a
/// bool, so `current` is re-read on `true`.
pub fn fan_out_subprompt_change(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    subprompt_id: &str,
    options: FanoutOptions,
    seams: &dyn FanoutSeams,
) -> SubpromptFanoutResult {
    let wanted = subprompt_id.to_lowercase();
    let mut result = SubpromptFanoutResult::default();

    let chats = match chats_read::find_by_character_id(main, character_id) {
        Ok(chats) => chats,
        Err(error) => {
            tracing::warn!(
                target: LOG_TARGET,
                character_id = %character_id,
                subprompt_id = %subprompt_id,
                error = %error,
                "Could not list chats for subprompt fan-out"
            );
            return result;
        }
    };

    for chat in chats {
        let chat_id = chat
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let seats: Vec<Value> = chat
            .get("participants")
            .and_then(Value::as_array)
            .map(|ps| {
                ps.iter()
                    .filter(|p| {
                        seat_str(p, "characterId") == Some(character_id)
                            && seat_str(p, "controlledBy") != Some("user")
                            && seat_str(p, "status") != Some("removed")
                            && seat_ids(p).iter().any(|id| id.to_lowercase() == wanted)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if seats.is_empty() {
            continue;
        }
        result.chats_touched += 1;

        let mut current = chat;
        for seat in &seats {
            let seat_id = seat_str(seat, "id").unwrap_or_default().to_string();
            let attempt = (|| -> Result<(), DbError> {
                if options.remove_selection {
                    let next: Vec<String> = seat_ids(seat)
                        .into_iter()
                        .filter(|id| id.to_lowercase() != wanted)
                        .collect();
                    let updated = ChatParticipantsRepository::new(main).update_participant(
                        &chat_id,
                        &seat_id,
                        &json!({ "selectedSubpromptIds": next }),
                    )?;
                    if updated {
                        if let Some(fresh) = chats_read::find_by_id(main, &chat_id)? {
                            current = fresh;
                        }
                    }
                }
                seams.compile(main, mount, &current, &seat_id)
            })();
            match attempt {
                Ok(()) => result.seats_recompiled += 1,
                Err(error) => {
                    tracing::warn!(
                        target: LOG_TARGET,
                        chat_id = %chat_id,
                        participant_id = %seat_id,
                        character_id = %character_id,
                        subprompt_id = %subprompt_id,
                        error = %error,
                        "Failed to recompile a seat after a subprompt change"
                    );
                }
            }
        }
        seams.publish_chat(&chat_id);
    }

    tracing::info!(
        target: LOG_TARGET,
        character_id = %character_id,
        subprompt_id = %subprompt_id,
        remove_selection = options.remove_selection,
        chats_touched = result.chats_touched,
        seats_recompiled = result.seats_recompiled,
        "Subprompt change fanned out"
    );
    result
}

#[cfg(test)]
mod log_tests {
    //! The fan-out's four lines, capture-pinned (v4 `Subprompts:Fanout`).
    use super::*;
    use crate::db::Writer;
    use crate::test_support::captured_with;
    use std::sync::Mutex;

    const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAR_A: &str = "a1000000-0000-4000-8000-0000000000a1";
    const CHAT_LLM: &str = "c1000000-0000-4000-8000-000000000001";
    const SEAT_LLM: &str = "e1000000-0000-4000-8000-000000000001";

    fn open_pair(dir: &tempfile::TempDir) -> (Writer, Writer) {
        let fx = |n: &str| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../quilltap-web/tests/fixtures")
                .join(n)
        };
        let main = dir.path().join("main.db");
        let mount = dir.path().join("mount.db");
        std::fs::copy(fx("subprompts-main.db"), &main).unwrap();
        std::fs::copy(fx("subprompts-mount.db"), &mount).unwrap();
        (
            Writer::open_writable(&main, TEST_PEPPER).unwrap(),
            Writer::open_writable(&mount, TEST_PEPPER).unwrap(),
        )
    }

    /// A seam that fails the compile for ONE named seat and records the rest.
    struct FailOne {
        seat: &'static str,
        calls: Mutex<Vec<String>>,
    }
    impl FanoutSeams for FailOne {
        fn compile(
            &self,
            _m: &Connection,
            _mo: &Connection,
            _chat: &Value,
            pid: &str,
        ) -> Result<(), DbError> {
            self.calls.lock().unwrap().push(pid.to_string());
            if pid == self.seat {
                return Err(DbError::Internal("boom".into()));
            }
            Ok(())
        }
        fn publish_chat(&self, _chat_id: &str) {}
        fn publish_character(&self, _character_id: &str) {}
    }

    fn line<'a>(lines: &'a [String], needle: &str) -> &'a String {
        lines
            .iter()
            .find(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("no `{needle}` line in {lines:?}"))
    }

    /// `Failed to recompile a seat after a subprompt change` (warn, `{chatId,
    /// participantId, characterId, subpromptId, error}`) for the failing seat
    /// — the loop continues, the other seat counts — then `Subprompt change
    /// fanned out` (info, `{characterId, subpromptId, removeSelection,
    /// chatsTouched, seatsRecompiled}`).
    #[test]
    fn a_failing_seat_warns_and_the_fanout_info_line_carries_the_counts() {
        let dir = tempfile::tempdir().unwrap();
        let (main, mount) = open_pair(&dir);
        let seams = FailOne {
            seat: SEAT_LLM,
            calls: Mutex::new(vec![]),
        };
        let (result, lines) = captured_with(|| {
            fan_out_subprompt_change(
                main.connection(),
                mount.connection(),
                CHAR_A,
                "terse",
                FanoutOptions {
                    remove_selection: true,
                },
                &seams,
            )
        });
        assert_eq!(
            result,
            SubpromptFanoutResult {
                chats_touched: 2,
                seats_recompiled: 1
            }
        );
        let w = line(
            &lines,
            "Failed to recompile a seat after a subprompt change",
        );
        assert!(w.starts_with("WARN quilltap::subprompts::fanout"), "{w}");
        for needle in [
            format!("chat_id={CHAT_LLM}"),
            format!("participant_id={SEAT_LLM}"),
            format!("character_id={CHAR_A}"),
            "subprompt_id=terse".to_string(),
            "error=boom".to_string(),
        ] {
            assert!(w.contains(&needle), "{w} lacks {needle}");
        }
        let i = line(&lines, "Subprompt change fanned out");
        assert!(i.starts_with("INFO quilltap::subprompts::fanout"), "{i}");
        assert!(
            i.contains("remove_selection=true")
                && i.contains("chats_touched=2")
                && i.contains("seats_recompiled=1"),
            "{i}"
        );
        assert_eq!(
            seams.calls.lock().unwrap().len(),
            2,
            "the loop continued past the failure"
        );
    }

    /// `Could not list chats for subprompt fan-out` (warn) + zeros when the
    /// chats listing itself fails (a main partition without the table).
    #[test]
    fn a_failed_chat_listing_warns_and_returns_zeros() {
        let broken = tempfile::tempdir().unwrap();
        let empty = Writer::open_writable(&broken.path().join("empty.db"), TEST_PEPPER).unwrap();
        let seams = FailOne {
            seat: "",
            calls: Mutex::new(vec![]),
        };
        let (result, lines) = captured_with(|| {
            fan_out_subprompt_change(
                empty.connection(),
                empty.connection(),
                CHAR_A,
                "terse",
                FanoutOptions::default(),
                &seams,
            )
        });
        assert_eq!(result, SubpromptFanoutResult::default());
        let w = line(&lines, "Could not list chats for subprompt fan-out");
        assert!(w.starts_with("WARN quilltap::subprompts::fanout"), "{w}");
        assert!(
            w.contains(&format!("character_id={CHAR_A}"))
                && w.contains("subprompt_id=terse")
                && w.contains("error="),
            "{w}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("fanned out")),
            "the info line is NOT reached on the early return: {lines:?}"
        );
    }
}
