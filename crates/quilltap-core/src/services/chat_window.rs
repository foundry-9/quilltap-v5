//! Which chats fall in a time window (v4 `lib/scriptorium/chat-window.ts`,
//! 4.12.1 `6121f59b9`; P4.D375, contract C8 — this module is P4.D375's alone).
//!
//! One place for the question "which conversations happened during this
//! period?", asked by the search tool (window-first conversation search and
//! the window census) and by per-turn retrospective recall (unsummarized chats
//! in the mini-recap). A chat's span is `[createdAt, lastMessageAt]`, falling
//! back to `updatedAt` and then `createdAt` — never `updatedAt` first, because
//! a retitle or a maintenance write bumps it without anything having happened.
//!
//! **The read's failure lines (A.7's correction).** v4 reads
//! `repos.chats.findByCharacterId` / `findByUserId`, both of which reach
//! `findByFilter` — a FALLBACK `safeQuery` that answers `[]` with `Error
//! finding entities by filter` before anything can throw. So outside
//! `withStrictRepositoryFailures` a failed read is that line and an empty
//! window; v4's own WARN `Could not read chats for a time window` fires only
//! inside the strict scope, where the fallback rethrows. ⚠ Recorded gap
//! (`CHATS_BY_CHARACTER_RETHROW_LINE_ABSENT`, P4.D375 item 23): under the
//! strict scope v4's `findByCharacterId` adds its own RETHROW-mode ERROR
//! `Failed to find chats by character ID` between the two; v5 has no home for
//! that line for ANY caller of `chats_read::find_by_character_id`, so it is not
//! logged here either (a follow-ups item — porting it moves every caller).

use rusqlite::Connection;
use serde_json::Value;

use crate::db::{chats_read, fallback};
use crate::memory_scoring::date_ms;
use crate::memory_scoring::js_sort::js_array_sort_by;
use crate::recall_tags::TimeWindow;

/// The log target for this module's lines (v4's `ChatWindow` service logger).
pub const CHAT_WINDOW_LOG_TARGET: &str = "quilltap::chat_window";

/// v4 `ChatSpan` — epoch milliseconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChatSpan {
    pub start: f64,
    pub end: f64,
}

/// A chat's string field as JS truthiness reads it: a non-empty string, or
/// `None` (absent, `null`, a non-string, or `""`).
fn truthy_str<'a>(chat: &'a Value, key: &str) -> Option<&'a str> {
    chat.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// v4 `chatSpanOf` — `[createdAt, lastMessageAt ?? updatedAt ?? createdAt]` in
/// ms; `None` when `createdAt` is unparseable. A truthy-but-unparseable
/// `lastMessageAt` falls through to `updatedAt`; the end is never before the
/// start (`Math.max`).
pub fn chat_span_of(chat: &Value) -> Option<ChatSpan> {
    // `Date.parse(chat.createdAt)` — an absent `createdAt` is `Date.parse(
    // undefined)`, NaN.
    let start = chat
        .get("createdAt")
        .and_then(Value::as_str)
        .map(date_ms)
        .unwrap_or(f64::NAN);
    if !start.is_finite() {
        return None;
    }
    for key in ["lastMessageAt", "updatedAt"] {
        let Some(candidate) = truthy_str(chat, key) else {
            continue;
        };
        let end = date_ms(candidate);
        if end.is_finite() {
            return Some(ChatSpan {
                start,
                end: crate::jsnum::math_max(start, end),
            });
        }
    }
    Some(ChatSpan { start, end: start })
}

/// v4 `chatOverlapsWindow` — true when the chat's span overlaps `[from, to]`
/// (ISO strings, inclusive). An unparseable bound overlaps nothing.
pub fn chat_overlaps_window(chat: &Value, window: &TimeWindow) -> bool {
    let Some(span) = chat_span_of(chat) else {
        return false;
    };
    let from = date_ms(&window.from);
    let to = date_ms(&window.to);
    if !from.is_finite() || !to.is_finite() {
        return false;
    }
    span.start <= to && span.end >= from
}

/// v4 `newestFirst` — by the end of the chat's span (`?? 0` for a span-less
/// chat, which the overlap filter has already removed).
fn newest_first(a: &Value, b: &Value) -> f64 {
    chat_span_of(b).map(|s| s.end).unwrap_or(0.0) - chat_span_of(a).map(|s| s.end).unwrap_or(0.0)
}

/// v4 `findChatsInWindow` — the chats a character (or, with no `character_id`,
/// the user — the operator surface) took part in whose span overlaps the
/// window, newest first, `exclude_chat_id` left out. Reads the chats ONCE. No
/// `chatType` filter: help, Brahma and autonomous chats count.
pub fn find_chats_in_window(
    conn: &Connection,
    character_id: Option<&str>,
    user_id: &str,
    window: &TimeWindow,
    exclude_chat_id: Option<&str>,
) -> Vec<Value> {
    let read = match character_id {
        Some(cid) => fallback::find_by_filter_strict_aware("chats", || {
            chats_read::find_by_character_id(conn, cid)
        }),
        None => fallback::find_by_filter_strict_aware("chats", || {
            chats_read::find_by_user_id(conn, user_id)
        }),
    };
    let candidates = match read {
        Ok(rows) => rows,
        Err(error) => {
            // Reached only inside the strict scope (module doc).
            tracing::warn!(
                target: CHAT_WINDOW_LOG_TARGET,
                characterId = character_id,
                userId = %user_id,
                error = %fallback::error_text(&error),
                "Could not read chats for a time window"
            );
            return Vec::new();
        }
    };
    let mut in_window: Vec<Value> = candidates
        .iter()
        .filter(|chat| {
            chat.get("id").and_then(Value::as_str) != exclude_chat_id
                && chat_overlaps_window(chat, window)
        })
        .cloned()
        .collect();
    js_array_sort_by(&mut in_window, newest_first);
    tracing::debug!(
        target: CHAT_WINDOW_LOG_TARGET,
        characterId = character_id,
        from = %window.from,
        to = %window.to,
        candidates = candidates.len(),
        inWindow = in_window.len(),
        "Chats in time window"
    );
    in_window
}

#[cfg(test)]
mod tests {
    //! v4 `__tests__/unit/lib/scriptorium/chat-window.test.ts`'s first five
    //! `it`s; the differential is `chat_window_equivalence`.
    use super::*;
    use serde_json::json;

    fn chat(id: &str, created: &str, last: Option<&str>, updated: Option<&str>) -> Value {
        let updated = updated.or(last).unwrap_or(created);
        let mut c = json!({ "id": id, "createdAt": created, "updatedAt": updated });
        if let Some(l) = last {
            c["lastMessageAt"] = json!(l);
        }
        c
    }

    fn window(from: &str, to: &str) -> TimeWindow {
        TimeWindow {
            from: from.into(),
            to: to.into(),
        }
    }

    #[test]
    fn ends_at_last_message_at_not_updated_at() {
        let c = chat(
            "a",
            "2026-10-28T10:00:00.000Z",
            Some("2026-10-28T11:00:00.000Z"),
            Some("2026-11-04T12:00:00.000Z"),
        );
        assert_eq!(
            chat_span_of(&c),
            Some(ChatSpan {
                start: date_ms("2026-10-28T10:00:00.000Z"),
                end: date_ms("2026-10-28T11:00:00.000Z"),
            })
        );
    }

    #[test]
    fn falls_back_to_updated_at_then_created_at() {
        let c = chat(
            "a",
            "2026-10-28T10:00:00.000Z",
            None,
            Some("2026-10-29T00:00:00.000Z"),
        );
        assert_eq!(
            chat_span_of(&c).unwrap().end,
            date_ms("2026-10-29T00:00:00.000Z")
        );
        let c = json!({ "createdAt": "2026-10-28T10:00:00.000Z", "updatedAt": "garbage", "lastMessageAt": null });
        assert_eq!(
            chat_span_of(&c).unwrap().end,
            date_ms("2026-10-28T10:00:00.000Z")
        );
        // A truthy-but-unparseable lastMessageAt falls through to updatedAt.
        let c = json!({ "createdAt": "2026-10-28T10:00:00.000Z", "updatedAt": "2026-10-30T00:00:00.000Z", "lastMessageAt": "soon" });
        assert_eq!(
            chat_span_of(&c).unwrap().end,
            date_ms("2026-10-30T00:00:00.000Z")
        );
        // An end before the start is lifted to the start.
        let c = json!({ "createdAt": "2026-10-28T10:00:00.000Z", "lastMessageAt": "2026-10-01T00:00:00.000Z" });
        assert_eq!(
            chat_span_of(&c).unwrap().end,
            date_ms("2026-10-28T10:00:00.000Z")
        );
    }

    #[test]
    fn null_when_created_at_is_unparseable() {
        let c = json!({ "createdAt": "nope", "updatedAt": "nope", "lastMessageAt": null });
        assert_eq!(chat_span_of(&c), None);
    }

    #[test]
    fn a_chat_retitled_today_whose_last_message_was_last_week_is_not_in_today() {
        let today = window("2026-11-04T06:00:00.000Z", "2026-11-05T05:59:59.999Z");
        let c = chat(
            "a",
            "2026-10-28T10:00:00.000Z",
            Some("2026-10-28T11:00:00.000Z"),
            Some("2026-11-04T12:00:00.000Z"),
        );
        assert!(!chat_overlaps_window(&c, &today));
    }

    #[test]
    fn a_chat_that_started_before_the_window_and_ran_into_it_overlaps() {
        let today = window("2026-11-04T06:00:00.000Z", "2026-11-05T05:59:59.999Z");
        let c = chat(
            "a",
            "2026-11-03T22:00:00.000Z",
            Some("2026-11-04T07:00:00.000Z"),
            None,
        );
        assert!(chat_overlaps_window(&c, &today));
        // An unparseable bound overlaps nothing.
        assert!(!chat_overlaps_window(
            &c,
            &window("x", "2026-11-05T05:59:59.999Z")
        ));
    }
}
