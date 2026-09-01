//! What counts as *activity* in a chat, and which timestamp the UI shows for it.
//!
//! Port of v4's `lib/chat/chat-activity.ts` (`735d9408c`, bug 112) — the
//! chokepoint that ended three different answers to "what counts as a message"
//! living in one codebase.
//!
//! A chat's `updatedAt` moves whenever anything about the row changes — a
//! generated story background landing, a context summary being folded, a
//! Concierge reroute, a token-cost tally. None of that is the conversation
//! moving forward, so none of it belongs in a "last updated" column the reader
//! scans to find where they left off.
//!
//! `lastMessageAt` is the answer to the question actually being asked: **when
//! did a character last post content?** — the human user or an LLM, speaking as
//! themselves. THE single source of truth for that judgement is
//! [`is_character_authored_message`] here; the SQLite mirror of it is
//! [`CHARACTER_AUTHORED_MESSAGE_FILTER`]. Change one and you must change the
//! other, or the live bump and the backfill will disagree.
//!
//! ## The two spellings are not exactly equivalent — deliberately, as in v4
//!
//! The in-memory predicate tests JS **truthiness** (`if (m.systemSender)`),
//! which lets an empty string through as "absent"; the SQL mirror tests
//! `systemSender IS NULL`, which does not. v4 ships both spellings knowingly —
//! the columns record absence as NULL, so the seam is unreachable in practice —
//! and this port mirrors both rather than unifying them, so the two
//! implementations stay diffable against v4 line for line. The harness pins the
//! `''` edge in the direction v4 actually measures.

use serde_json::Value;

/// Did a character — the human user or an LLM — post this as content?
///
/// Included, deliberately: **whispers** (`targetParticipantIds` non-empty). A
/// character murmuring to one other character is still a character speaking; a
/// room full of whispering shouldn't read as a room gone quiet. (There is no
/// `targetParticipantIds` test here at all — they count by omission.)
///
/// Excluded, deliberately:
/// - **Non-`message` events** (`context-summary`, `system`) — bookkeeping.
/// - **Staff / personified-feature announcements** (`systemSender` set: Lantern,
///   Aurora, Librarian, Concierge, Prospero, Host, Commonplace Book, Ariel,
///   Carina, Suparṇā, Pascal). These persist as `type: 'message'` rows, which is
///   precisely why "any message row" is the wrong test — a background image
///   finishing rendering would otherwise float a months-dead chat to the top of
///   the list.
/// - **Announcement bubbles** (`customAnnouncer` set) — an announcement wearing
///   a name is still an announcement, not the character speaking.
/// - **`SYSTEM` and `TOOL` roles** — a raw tool-result row is machinery, not
///   posted content.
pub fn is_character_authored_message(event: &Value) -> bool {
    if event.get("type").and_then(Value::as_str) != Some("message") {
        return false;
    }
    is_character_authored_parts(
        event.get("role").and_then(Value::as_str).unwrap_or(""),
        event.get("systemSender"),
        event.get("customAnnouncer"),
    )
}

/// The predicate's field-level core, shared by the JSON spelling above and the
/// typed one the write path uses (`ChatEventInput` is already narrowed to the
/// `message` arm there, so the `type` test lives at each caller). Both
/// `system_sender` and `custom_announcer` are tested for **JS truthiness**, as
/// v4's `if (m.systemSender)` / `if (m.customAnnouncer)` are.
pub fn is_character_authored_parts(
    role: &str,
    system_sender: Option<&Value>,
    custom_announcer: Option<&Value>,
) -> bool {
    if role != "USER" && role != "ASSISTANT" {
        return false;
    }
    if js_truthy(system_sender) {
        return false;
    }
    if js_truthy(custom_announcer) {
        return false;
    }
    true
}

/// The SQLite mirror of [`is_character_authored_message`], for indexed lookups
/// that must not load and Zod-validate a whole transcript. v4 spells it as a
/// `QueryFilter` object spread alongside a `chatId`; v5's repositories write SQL
/// directly, so it is the WHERE fragment — one string, so the live lookup and
/// the recompute pass cannot drift.
///
/// `systemSender: null` / `customAnnouncer: null` translate to `IS NULL`, which
/// is how both columns record "absent" (they default to NULL).
pub const CHARACTER_AUTHORED_MESSAGE_FILTER: &str =
    "type = 'message' AND role IN ('USER', 'ASSISTANT') \
     AND systemSender IS NULL AND customAnnouncer IS NULL";

/// The timestamp to sort and display a chat by: when a character last posted,
/// falling back to when the chat was created.
///
/// The fallback is `createdAt`, **not** `updatedAt` — a chat where only the
/// Staff has ever spoken has had no conversational activity at all, and dating
/// it by the last background image regenerated is the very drift this module
/// exists to stop. `createdAt` is the honest, and stable, answer.
///
/// v4's `chat.lastMessageAt ?? chat.createdAt` is **nullish** coalescing: an
/// empty-string `lastMessageAt` wins over `createdAt` (and then reads as time 0
/// through [`chat_activity_time`]). v5's chats arrive as JSON, where an absent
/// key and a JSON `null` are both "nullish". A missing `createdAt`, and a
/// present-but-non-string `lastMessageAt`, are both shapes v4's types forbid
/// (`lastMessageAt` is a TEXT column); they degrade to `""`, i.e. time 0.
pub fn chat_activity_at(chat: &Value) -> &str {
    match chat.get("lastMessageAt") {
        Some(Value::String(s)) => s,
        Some(v) if !v.is_null() => "",
        _ => chat.get("createdAt").and_then(Value::as_str).unwrap_or(""),
    }
}

/// [`chat_activity_at`] as epoch milliseconds, for comparators. An unparseable
/// timestamp reports 0 rather than NaN, so comparators stay total (v4:
/// `Number.isNaN(ms) ? 0 : ms`).
pub fn chat_activity_time(chat: &Value) -> i64 {
    crate::clock::iso_to_ms(chat_activity_at(chat)).unwrap_or(0)
}

/// Newest-activity-first comparator for chat lists (v4 `byChatActivityDesc`).
/// Ties are resolved by sort stability only — use it with a stable sort, as
/// `Array.prototype.sort` is.
pub fn by_chat_activity_desc(a: &Value, b: &Value) -> std::cmp::Ordering {
    chat_activity_time(b).cmp(&chat_activity_time(a))
}

/// JS truthiness over a JSON value — v4's `if (m.systemSender)`.
fn js_truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_)) | Some(Value::Object(_)) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_empty_string_system_sender_is_absent_to_the_predicate_but_not_to_the_sql() {
        // The deliberate seam between the two spellings (module header): the
        // in-memory predicate tests truthiness, the SQL mirror tests IS NULL.
        assert!(is_character_authored_message(&json!({
            "type": "message", "role": "USER", "systemSender": ""
        })));
        assert!(CHARACTER_AUTHORED_MESSAGE_FILTER.contains("systemSender IS NULL"));
    }

    #[test]
    fn an_empty_announcer_object_is_still_an_announcement() {
        // `{}` is truthy in JS — an announcement bubble with no fields is still
        // an announcement.
        assert!(!is_character_authored_message(&json!({
            "type": "message", "role": "USER", "customAnnouncer": {}
        })));
    }

    #[test]
    fn activity_falls_back_to_created_at_never_updated_at() {
        let chat = json!({
            "lastMessageAt": Value::Null,
            "createdAt": "2026-01-01T00:00:00.000Z",
            "updatedAt": "2026-05-01T00:00:00.000Z",
        });
        assert_eq!(chat_activity_at(&chat), "2026-01-01T00:00:00.000Z");
    }
}
