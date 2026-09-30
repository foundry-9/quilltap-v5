//! On-demand conversation rendering — v4 `lib/scriptorium/render-chat.ts`
//! (`renderChatConversation`, `f7f3d7bf0`).
//!
//! v4 used to keep every chat's rendered Markdown in `chats.renderedMarkdown`.
//! `f7f3d7bf0` dropped the column: only the interchange chunks are stored (for
//! embedding and search), and the Markdown is re-rendered from the messages
//! whenever someone needs to read it — the `CONVERSATION_RENDER` job (which
//! chunks it), `read_conversation`, and `upsert_annotation`. This module is
//! the one place that render happens.
//!
//! ## Not byte-identical to the old stored column (a false premise, measured)
//!
//! The drift ledger's trap ("the on-demand render must match the stored bytes
//! exactly") cannot hold, and v4 has no test claiming it:
//!
//!   - the header's `Current time:` line is the wall clock of the RENDER, so a
//!     live render carries the read moment, never the job's (hence the injected
//!     `now_iso` — production passes [`crate::clock::now_iso`], the
//!     differentials a frozen instant);
//!   - `lastUpdatedAt` is the chat's `updatedAt` at read time;
//!   - the speaker map is bug 161's ONE resolver
//!     ([`super::speaker_names::resolve_speaker_names`] — raw reads, a failed
//!     read swallowed, **no `'User'` fallback**). The old job's private map
//!     named a `controlledBy: 'user'` seat with no character `User`; left
//!     unmapped now, the renderer labels that seat's USER-role lines `User` and
//!     its ASSISTANT-role lines `Assistant` (v4 `conversation-render.test.ts:140`
//!     pins `characterNames.has('participant-2')` false).
//!
//! The proof is therefore a frozen-clock live-vs-live render on both sides.

use std::time::Instant;

use rusqlite::Connection;
use serde_json::Value;

use super::conversation_markdown::{
    render_conversation_markdown, ConversationMetadata, RenderEvent, RenderedConversation,
};
use super::speaker_names::resolve_speaker_names;
use crate::db::{chats_messages_read, DbError};

/// v4 `renderChatConversation(chat)`: render `chat` (a `chats_read` row) from
/// its stored messages. `Ok(None)` when the chat has no events at all; a chat
/// with events but no visible dialogue renders to a header and zero
/// interchanges.
///
/// Order is v4's: the events read first, then the per-seat character reads.
/// `zone` is the zone every timestamp renders in — v4's `toLocale*` calls
/// carry no `timeZone`, so production passes the host's (P4.119).
pub fn render_chat_conversation(
    conn: &Connection,
    chat: &Value,
    now_iso: &str,
    zone: &jiff::tz::TimeZone,
) -> Result<Option<RenderedConversation>, DbError> {
    let started = Instant::now();
    let str_of = |key: &str| {
        chat.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let chat_id = str_of("id");

    let events = chats_messages_read::get_messages(conn, &chat_id)?;
    if events.is_empty() {
        tracing::debug!(
            target: "quilltap::scriptorium",
            chatId = %chat_id,
            "No events to render",
        );
        return Ok(None);
    }

    let participants = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let speaker_names = resolve_speaker_names(conn, &participants);

    let messages: Vec<RenderEvent> = events.iter().map(event_from_json).collect();
    let metadata = ConversationMetadata {
        conversation_id: chat_id.clone(),
        title: str_of("title"),
        created_at: str_of("createdAt"),
        last_updated_at: str_of("updatedAt"),
    };
    let result = render_conversation_markdown(
        &messages,
        speaker_names.entries(),
        Some(&metadata),
        now_iso,
        zone,
    );

    tracing::debug!(
        target: "quilltap::scriptorium",
        chatId = %chat_id,
        events = events.len(),
        interchanges = result.interchanges.len(),
        // JS `string.length` — UTF-16 code units, not bytes.
        markdownLength = result.markdown.encode_utf16().count(),
        durationMs = started.elapsed().as_millis() as u64,
        "Rendered conversation",
    );

    Ok(Some(result))
}

/// Marshal one `getMessages` row into the renderer's input shape. The renderer
/// reads only these six fields; everything else on the event is irrelevant to it.
fn event_from_json(v: &Value) -> RenderEvent {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    RenderEvent {
        id: s("id").unwrap_or_default(),
        type_: s("type"),
        role: s("role"),
        content: s("content"),
        participant_id: s("participantId"),
        created_at: s("createdAt").unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::global_capture::capture as captured_with;
    use serde_json::json;

    const NOW: &str = "2026-09-28T12:00:00.000Z";

    fn conn() -> Connection {
        crate::test_support::global_capture::install();
        let schema: Value = serde_json::from_str(include_str!("provisioning/fresh_schema.json"))
            .expect("fresh_schema.json parses");
        let conn = Connection::open_in_memory().unwrap();
        for s in schema["main"].as_array().unwrap() {
            let s = s.as_str().unwrap();
            if s.starts_with("CREATE TABLE \"chat_messages\"")
                || s.starts_with("CREATE TABLE \"characters\"")
            {
                conn.execute_batch(s).unwrap();
            }
        }
        conn.execute(
            "INSERT INTO characters (id, userId, name, createdAt, updatedAt) \
             VALUES ('00000000-0000-4000-8000-0000000000b1', 'u1', 'Friday', ?1, ?1)",
            [NOW],
        )
        .unwrap();
        conn
    }

    fn message(
        conn: &Connection,
        id: &str,
        role: &str,
        participant: &str,
        content: &str,
        at: &str,
    ) {
        conn.execute(
            "INSERT INTO chat_messages (id, chatId, type, role, content, participantId, createdAt) \
             VALUES (?1, '00000000-0000-4000-8000-0000000000c1', 'message', ?2, ?3, ?4, ?5)",
            rusqlite::params![id, role, content, participant, at],
        )
        .unwrap();
    }

    fn chat() -> Value {
        json!({
            "id": "00000000-0000-4000-8000-0000000000c1",
            "title": "A Quiet Evening",
            "createdAt": "2026-09-01T10:00:00.000Z",
            "updatedAt": "2026-09-02T10:00:00.000Z",
            "participants": [
                {"id": "00000000-0000-4000-8000-0000000000a1", "type": "CHARACTER", "characterId": "00000000-0000-4000-8000-0000000000b1", "controlledBy": "llm"},
                {"id": "00000000-0000-4000-8000-0000000000a2", "type": "CHARACTER", "controlledBy": "user"},
            ],
        })
    }

    /// v4 `:45-48`: zero events → `null` and the DEBUG, no render.
    #[test]
    fn a_chat_with_no_events_renders_nothing_and_says_so() {
        let conn = conn();
        let (out, lines) = captured_with(|| {
            render_chat_conversation(&conn, &chat(), NOW, &jiff::tz::TimeZone::UTC)
        });
        assert!(out.unwrap().is_none());
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("DEBUG quilltap::scriptorium")
                    && l.contains("No events to render")
                    && l.contains(" chatId=00000000-0000-4000-8000-0000000000c1")),
            "{lines:?}"
        );
        assert!(!lines.iter().any(|l| l.contains("Rendered conversation")));
    }

    /// v4 `:59-65`: the render DEBUG carries v4's five fields; the silence leg
    /// is the empty chat above. The header carries the INJECTED clock.
    #[test]
    fn a_render_logs_its_counts_and_carries_the_injected_clock() {
        let conn = conn();
        message(
            &conn,
            "00000000-0000-4000-8000-0000000000e1",
            "USER",
            "00000000-0000-4000-8000-0000000000a2",
            "Good evening.",
            "2026-09-01T10:01:00.000Z",
        );
        message(
            &conn,
            "00000000-0000-4000-8000-0000000000e2",
            "ASSISTANT",
            "00000000-0000-4000-8000-0000000000a1",
            "And to you.",
            "2026-09-01T10:02:00.000Z",
        );
        let (out, lines) = captured_with(|| {
            render_chat_conversation(&conn, &chat(), NOW, &jiff::tz::TimeZone::UTC)
        });
        let rendered = out.unwrap().expect("two events render");
        assert_eq!(rendered.interchanges.len(), 1);
        assert!(rendered.markdown.contains("Current time: "));
        let line = lines
            .iter()
            .find(|l| l.contains("Rendered conversation"))
            .unwrap_or_else(|| panic!("{lines:?}"));
        assert!(line.starts_with("DEBUG quilltap::scriptorium"), "{line}");
        // v4's camelCase field NAMES (P4.126 — v5 had logged snake_case).
        for field in [
            " chatId=00000000-0000-4000-8000-0000000000c1",
            " events=2",
            " interchanges=1",
            &format!(
                " markdownLength={}",
                rendered.markdown.encode_utf16().count()
            ),
            " durationMs=",
        ] {
            assert!(line.contains(field), "{field} missing: {line}");
        }
        assert!(!lines.iter().any(|l| l.contains("No events to render")));
    }

    /// The speaker-map change (v4 `conversation-render.test.ts:140`): a user
    /// seat with no character is left for the renderer's own labels — its
    /// ASSISTANT-role line is `Assistant`, no longer the old job's `User`.
    #[test]
    fn a_characterless_user_seat_is_left_to_the_renderers_labels() {
        let conn = conn();
        message(
            &conn,
            "00000000-0000-4000-8000-0000000000e1",
            "USER",
            "00000000-0000-4000-8000-0000000000a2",
            "Hello there.",
            "2026-09-01T10:01:00.000Z",
        );
        message(
            &conn,
            "00000000-0000-4000-8000-0000000000e2",
            "ASSISTANT",
            "00000000-0000-4000-8000-0000000000a2",
            "Spoken for them.",
            "2026-09-01T10:02:00.000Z",
        );
        message(
            &conn,
            "00000000-0000-4000-8000-0000000000e3",
            "ASSISTANT",
            "00000000-0000-4000-8000-0000000000a1",
            "Quite.",
            "2026-09-01T10:03:00.000Z",
        );
        let rendered = render_chat_conversation(&conn, &chat(), NOW, &jiff::tz::TimeZone::UTC)
            .unwrap()
            .unwrap();
        let md = &rendered.markdown;
        assert!(md.contains("### Message 0 (User)"), "{md}");
        assert!(
            md.contains("### Message 1 (Assistant)"),
            "the user seat's ASSISTANT line: {md}"
        );
        assert!(md.contains("### Message 2 (Friday)"), "{md}");
    }
}
