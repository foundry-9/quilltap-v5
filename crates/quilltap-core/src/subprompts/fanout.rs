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
    crate::services::orchestrator::json_str_array(seat, "selectedSubpromptIds")
}

/// The tracing target of the generalised fan-out (v4
/// `createServiceLogger('SystemPromptCompiler:CharacterFanout')`, `b27e5089a`).
/// The subprompt caller's INFO stays on [`LOG_TARGET`].
pub const CHARACTER_FANOUT_LOG_TARGET: &str = "quilltap::system_prompt_compiler::character_fanout";

/// v4 `CharacterSeatFanoutResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharacterSeatFanoutResult {
    /// Chats that had at least one matching seat.
    pub chats_touched: usize,
    /// Seats recompiled.
    pub seats_recompiled: usize,
}

/// A seat-preparation step: runs before the seat's recompile and returns the
/// updated chat when it wrote one (v4 `prepareSeat`).
pub type PrepareSeat<'a> = &'a dyn Fn(&Value, &Value) -> Result<Option<Value>, DbError>;

/// v4 `CharacterSeatFanoutOptions`.
#[derive(Clone, Copy)]
pub struct CharacterSeatFanoutOptions<'a> {
    /// Why the fan-out runs, for the logs.
    pub reason: &'a str,
    /// Narrows the candidate seats.
    pub seat_filter: Option<&'a dyn Fn(&Value) -> bool>,
    /// Runs before a seat's recompile; returns the updated chat when it wrote one.
    pub prepare_seat: Option<PrepareSeat<'a>>,
}

/// v4 `recompileCharacterSeats` (`system-prompt-compiler/character-fanout.ts`,
/// `b27e5089a`): recompile the identity stack of every matching seat of
/// `character_id`.
///
/// The compiler does not invalidate a chat's cached stack when the character
/// behind a seat is edited; a change that must reach running conversations fans
/// out from here instead. Subprompt edits ([`fan_out_subprompt_change`]) and
/// the Humanoid flag on the physical description (the character PUT) both call
/// it. Every LLM-controlled, non-removed seat of the character is a candidate
/// (an absent `controlledBy` is `undefined !== 'user'` — LLM); `seat_filter`
/// narrows that, and `prepare_seat` may write to the seat (and return the
/// updated chat) before it is recompiled. Everything fails soft: a seat that
/// cannot be recompiled logs and is left to the read-through fallback.
pub fn recompile_character_seats(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    options: CharacterSeatFanoutOptions<'_>,
    seams: &dyn FanoutSeams,
) -> CharacterSeatFanoutResult {
    let mut result = CharacterSeatFanoutResult::default();

    let chats = match chats_read::find_by_character_id(main, character_id) {
        Ok(chats) => chats,
        Err(error) => {
            tracing::warn!(
                target: CHARACTER_FANOUT_LOG_TARGET,
                characterId = %character_id,
                reason = %options.reason,
                error = %error,
                "Could not list chats for identity-stack fan-out"
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
                            && options.seat_filter.is_none_or(|f| f(p))
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if seats.is_empty() {
            continue;
        }
        result.chats_touched += 1;

        let mut current = chat.clone();
        for seat in &seats {
            let seat_id = seat_str(seat, "id").unwrap_or_default().to_string();
            let attempt = (|| -> Result<(), DbError> {
                if let Some(prepare) = options.prepare_seat {
                    // v4 hands `prepareSeat` the chat as LISTED, not `current`.
                    if let Some(updated) = prepare(&chat, seat)? {
                        current = updated;
                    }
                }
                seams.compile(main, mount, &current, &seat_id)
            })();
            match attempt {
                Ok(()) => result.seats_recompiled += 1,
                Err(error) => {
                    tracing::warn!(
                        target: CHARACTER_FANOUT_LOG_TARGET,
                        chatId = %chat_id,
                        participantId = %seat_id,
                        characterId = %character_id,
                        reason = %options.reason,
                        error = %error,
                        "Failed to recompile a seat in an identity-stack fan-out"
                    );
                }
            }
        }
        seams.publish_chat(&chat_id);
    }

    tracing::debug!(
        target: CHARACTER_FANOUT_LOG_TARGET,
        characterId = %character_id,
        reason = %options.reason,
        chatsTouched = result.chats_touched,
        seatsRecompiled = result.seats_recompiled,
        "Identity-stack fan-out complete"
    );
    result
}

/// Recompile every seat of `character_id` that has `subprompt_id` in play.
/// With `remove_selection`, the id is dropped from each such seat's selection
/// before the recompile.
///
/// A thin caller of [`recompile_character_seats`] since v4 `b27e5089a`: the
/// seat filter is `(selectedSubpromptIds ?? []).some(lower === wanted)`, and
/// the strip is the `prepare_seat` (v4's `updateParticipant` returns the
/// updated chat; v5's returns a bool, so the chat is re-read on `true`). Only
/// the INFO stays on this module's logger.
pub fn fan_out_subprompt_change(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    subprompt_id: &str,
    options: FanoutOptions,
    seams: &dyn FanoutSeams,
) -> SubpromptFanoutResult {
    let wanted = subprompt_id.to_lowercase();
    let reason = format!("subprompt {subprompt_id}");
    let seat_filter = |p: &Value| seat_ids(p).iter().any(|id| id.to_lowercase() == wanted);
    let strip = |chat: &Value, seat: &Value| -> Result<Option<Value>, DbError> {
        let chat_id = seat_str(chat, "id").unwrap_or_default();
        let seat_id = seat_str(seat, "id").unwrap_or_default();
        let next: Vec<String> = seat_ids(seat)
            .into_iter()
            .filter(|id| id.to_lowercase() != wanted)
            .collect();
        let updated = ChatParticipantsRepository::new(main).update_participant(
            chat_id,
            seat_id,
            &json!({ "selectedSubpromptIds": next }),
        )?;
        if updated {
            return chats_read::find_by_id(main, chat_id);
        }
        Ok(None)
    };
    let fanout = recompile_character_seats(
        main,
        mount,
        character_id,
        CharacterSeatFanoutOptions {
            reason: &reason,
            seat_filter: Some(&seat_filter),
            prepare_seat: options
                .remove_selection
                .then_some(&strip as PrepareSeat<'_>),
        },
        seams,
    );
    let result = SubpromptFanoutResult {
        chats_touched: fanout.chats_touched,
        seats_recompiled: fanout.seats_recompiled,
    };

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
        let main_w = Writer::open_writable(&main, TEST_PEPPER).unwrap();
        crate::test_support::ensure_p4d171_columns(main_w.connection());
        (main_w, Writer::open_writable(&mount, TEST_PEPPER).unwrap())
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

    /// `Failed to recompile a seat in an identity-stack fan-out` (warn, `{chatId,
    /// participantId, characterId, reason, error}`, the generalised fan-out's
    /// logger) for the failing seat — the loop continues, the other seat
    /// counts — then `Identity-stack fan-out complete` (debug) and `Subprompt
    /// change fanned out` (info, `{characterId, subpromptId, removeSelection,
    /// chatsTouched, seatsRecompiled}`). The two retired texts are absent.
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
        // P4.D349 (v4 `b27e5089a`): the per-seat WARN moved to the generalised
        // fan-out's logger, `subpromptId` → `reason`.
        let w = line(
            &lines,
            "Failed to recompile a seat in an identity-stack fan-out",
        );
        assert!(
            w.starts_with("WARN quilltap::system_prompt_compiler::character_fanout"),
            "{w}"
        );
        for needle in [
            format!("chatId={CHAT_LLM}"),
            format!("participantId={SEAT_LLM}"),
            format!("characterId={CHAR_A}"),
            "reason=subprompt terse".to_string(),
            "error=boom".to_string(),
        ] {
            assert!(w.contains(&needle), "{w} lacks {needle}");
        }
        let d = line(&lines, "Identity-stack fan-out complete");
        assert!(
            d.starts_with("DEBUG quilltap::system_prompt_compiler::character_fanout"),
            "{d}"
        );
        for needle in [
            format!("characterId={CHAR_A}"),
            "reason=subprompt terse".to_string(),
            "chatsTouched=2".to_string(),
            "seatsRecompiled=1".to_string(),
        ] {
            assert!(d.contains(&needle), "{d} lacks {needle}");
        }
        for old in [
            "Failed to recompile a seat after a subprompt change",
            "Could not list chats for subprompt fan-out",
        ] {
            assert!(
                !lines.iter().any(|l| l.contains(old)),
                "the retired text `{old}` is gone: {lines:?}"
            );
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

    /// `Could not list chats for identity-stack fan-out` (warn, v4
    /// `{characterId, reason, error}`) + zeros when the chats listing itself
    /// fails (a main partition without the table). Since v4 `b27e5089a` the
    /// early return is the generalised fan-out's, so the subprompt INFO still
    /// follows — with zero counts — and the completion DEBUG does not.
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
        let w = line(&lines, "Could not list chats for identity-stack fan-out");
        assert!(
            w.starts_with("WARN quilltap::system_prompt_compiler::character_fanout"),
            "{w}"
        );
        assert!(
            w.contains(&format!("characterId={CHAR_A}"))
                && w.contains("reason=subprompt terse")
                && w.contains("error="),
            "{w}"
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Could not list chats for subprompt fan-out")),
            "the retired text is gone: {lines:?}"
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Identity-stack fan-out complete")),
            "the completion DEBUG is not reached on the early return: {lines:?}"
        );
        let i = line(&lines, "Subprompt change fanned out");
        assert!(
            i.contains("chats_touched=0") && i.contains("seats_recompiled=0"),
            "{i}"
        );
    }
}
