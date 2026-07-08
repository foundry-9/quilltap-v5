//! Writers for terminal-session announcements (Ariel) + the terminal-session
//! reconcile pass — v4 `lib/services/ariel-notifications/writer.ts` and
//! `lib/terminal/reconcile.ts`.
//!
//! When a terminal session is opened or closed in the Salon, Ariel posts
//! synthetic ASSISTANT-role chat messages announcing the session lifecycle,
//! and the PTY manager posts periodic terminal-output summaries between them.
//! These messages are visible to the user and other characters in the chat.
//!
//! The post idiom is the `carina_runner::writer` / W4.6b shape: mint
//! `uuid` + `clock::now_iso`, build the exact v4 `MessageEvent` object literal
//! (`serde_json::json!`), validate into a
//! [`ChatEventInput`](crate::db::chats_messages::ChatEventInput), and insert
//! through the single-writer `add_message` — so the stored row is byte-for-byte
//! the `db::chats_messages` insert marshaling. Errors never propagate — terminal
//! operations must never fail because an announcement couldn't be written
//! (v4's catch-all → `null`; here every failure is `None`).
//!
//! v4 embeds the terminal session id in the message body as an HTML comment
//! (`<!-- terminalSessionId:<id> -->`) so the Salon renderer can associate the
//! message with the session; the embed is mirrored into `opaqueContent` when an
//! opaque body exists so the same renderer hook works for whichever body the
//! context-builder ends up using.
//!
//! The reconcile pass ([`reconcile_terminal_sessions_for_chat`]) lives here too:
//! v4 hosts it in `lib/terminal/reconcile.ts`, but core-side it is a composition
//! of the ported `terminal_sessions` repo and the close-announcement writer. Its
//! one host dependency — "is there a live PTY session with this id?"
//! (`ptyManager.get(...)`) — is the injected `is_live` probe, so the core stays
//! free of PTY runtime state.

use serde_json::{json, Value};

use crate::db::chats_messages::ChatEventInput;
use crate::db::runtime::Db;
use crate::db::{chats_read, terminal_sessions};
use crate::jsstr::{js_trim, utf16_len, utf16_slice_from, utf16_truncate};
use crate::services::primary_stream::to_locale_string;

/// v4 `TERMINAL_OUTPUT_MAX_CHARS` — the whole-body cap before eliding
/// (UTF-16 code units; v4 measures JS `.length`).
const TERMINAL_OUTPUT_MAX_CHARS: usize = 16 * 1024;
/// v4 `TERMINAL_OUTPUT_HEAD_TAIL_CHARS` — the head and tail kept when eliding.
const TERMINAL_OUTPUT_HEAD_TAIL_CHARS: usize = 8 * 1024;

/// The flush reason (v4's `'idle' | 'max-age' | 'session-closed'` union). Feeds
/// both the reason note in the announcement body and the `systemKind` suffix
/// (`terminal-output-<reason>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArielFlushReason {
    Idle,
    MaxAge,
    SessionClosed,
}

impl ArielFlushReason {
    /// The wire string (`idle` / `max-age` / `session-closed`).
    pub fn as_str(&self) -> &'static str {
        match self {
            ArielFlushReason::Idle => "idle",
            ArielFlushReason::MaxAge => "max-age",
            ArielFlushReason::SessionClosed => "session-closed",
        }
    }
}

/// The persisted Ariel message (the shape the callers can inspect / the
/// differential diffs): the minted id plus the full `MessageEvent` object
/// literal that was validated + inserted.
#[derive(Debug, Clone)]
pub struct PostedArielMessage {
    pub id: String,
    pub message: Value,
}

/// v4's private `postArielMessage`: look the chat up (missing chat → skip,
/// `None`), mint id + `createdAt`, embed the terminal session id as an HTML
/// comment in `content` (and in `opaqueContent` when non-null), assemble the
/// exact v4 `MessageEvent` literal (`role: 'ASSISTANT'`, `participantId: null`,
/// `systemSender: 'ariel'`), validate, and insert through `add_message`. Any
/// failure → `None` (v4's catch-all).
async fn post_ariel_message(
    db: &Db,
    chat_id: &str,
    content: String,
    opaque_content: Option<String>,
    system_kind: &str,
    terminal_session_id: &str,
) -> Option<PostedArielMessage> {
    // v4: `repos.chats.findById(chatId)` → if !chat return null.
    let chat_id_owned = chat_id.to_string();
    db.read_main(|main| chats_read::find_by_id(main, &chat_id_owned))
        .ok()
        .flatten()?;

    let message_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();

    // v4 always passes a session id from the three public post functions, so the
    // embed is unconditional here (the `terminalSessionId ? … : content` branch
    // never sees null from a ported caller).
    let content_with_id = format!("<!-- terminalSessionId:{terminal_session_id} -->\n{content}");
    let opaque_with_id: Value = match opaque_content {
        Some(oc) => Value::String(format!(
            "<!-- terminalSessionId:{terminal_session_id} -->\n{oc}"
        )),
        None => Value::Null,
    };

    let message = json!({
        "type": "message",
        "id": message_id,
        "role": "ASSISTANT",
        "content": content_with_id,
        "opaqueContent": opaque_with_id,
        "attachments": [],
        "createdAt": now,
        "participantId": Value::Null,
        "systemSender": "ariel",
        "systemKind": system_kind,
    });

    let event: ChatEventInput = serde_json::from_value(message.clone()).ok()?;
    let chat_id_owned = chat_id.to_string();
    match db
        .write(move |writers| {
            writers
                .main()
                .chat_messages()
                .add_message(&chat_id_owned, &event)
        })
        .await
    {
        Ok(()) => Some(PostedArielMessage {
            id: message_id,
            message,
        }),
        // v4 catches, logs, and returns null on a persistence failure.
        Err(_) => None,
    }
}

/// v4 `ArielSessionOpenedAnnouncement`.
#[derive(Debug, Clone)]
pub struct ArielSessionOpenedAnnouncement {
    pub chat_id: String,
    pub session_id: String,
    /// Optional session label. v4's truthiness check means an empty string
    /// behaves like absent (no ` — "…"` suffix).
    pub label: Option<String>,
    pub shell: String,
    pub cwd: String,
}

/// v4 `postArielSessionOpenedAnnouncement`: announce the opening of a new
/// terminal session with shell + working directory (steampunk voice, byte-exact).
pub async fn post_ariel_session_opened_announcement(
    db: &Db,
    params: &ArielSessionOpenedAnnouncement,
) -> Option<PostedArielMessage> {
    // v4: `params.label ? ` — "${params.label}"` : ''` — JS truthiness, so an
    // empty-string label produces no suffix.
    let label = match params.label.as_deref() {
        Some(l) if !l.is_empty() => format!(" \u{2014} \"{l}\""),
        _ => String::new(),
    };
    let content = format!(
        "Ariel has opened a terminal aboard the **{}** at `{}`{label}.\n\n(Session id: `{}`.)",
        params.shell, params.cwd, params.session_id
    );
    let opaque_content = format!(
        "Terminal session opened: **{}** at `{}`{label}.\n\n(Session id: `{}`.)",
        params.shell, params.cwd, params.session_id
    );

    post_ariel_message(
        db,
        &params.chat_id,
        content,
        Some(opaque_content),
        "session-opened",
        &params.session_id,
    )
    .await
}

/// v4 `ArielTerminalOutputAnnouncement`.
#[derive(Debug, Clone)]
pub struct ArielTerminalOutputAnnouncement {
    pub chat_id: String,
    pub session_id: String,
    /// Output already cleaned by
    /// [`clean_terminal_output`](crate::terminal_clean::clean_terminal_output)
    /// (the PTY manager cleans before posting; no write-time cleaning elsewhere).
    pub cleaned_output: String,
    pub reason: ArielFlushReason,
}

/// v4 `computeFenceLength`: one more backtick than the longest run in the
/// content, minimum 3 — so the fenced block can never be broken by backticks in
/// the terminal output itself.
fn compute_fence_length(content: &str) -> usize {
    let mut longest = 0usize;
    let mut run = 0usize;
    for c in content.chars() {
        if c == '`' {
            run += 1;
            if run > longest {
                longest = run;
            }
        } else {
            run = 0;
        }
    }
    3.max(longest + 1)
}

/// v4 `elideForChat`: bodies over 16 KB (UTF-16 code units) keep an 8 KB head +
/// 8 KB tail around an `… [N characters elided] …` marker (`N` grouped with
/// en-US thousands separators — v4 `toLocaleString('en-US')`).
fn elide_for_chat(input: &str) -> String {
    let len = utf16_len(input);
    if len <= TERMINAL_OUTPUT_MAX_CHARS {
        return input.to_string();
    }
    let head = utf16_truncate(input, TERMINAL_OUTPUT_HEAD_TAIL_CHARS);
    let tail = utf16_slice_from(input, len - TERMINAL_OUTPUT_HEAD_TAIL_CHARS);
    let elided_units = len - utf16_len(&head) - utf16_len(&tail);
    format!(
        "{head}\n\n\u{2026} [{} characters elided] \u{2026}\n\n{tail}",
        to_locale_string(elided_units as i64)
    )
}

/// v4 `postArielTerminalOutputAnnouncement`: a periodic terminal-output summary.
/// Returns `None` when `cleaned_output` is empty or whitespace-only — there is no
/// value in posting a blank message just because a debounce timer fired.
pub async fn post_ariel_terminal_output_announcement(
    db: &Db,
    params: &ArielTerminalOutputAnnouncement,
) -> Option<PostedArielMessage> {
    // v4: `!params.cleanedOutput || params.cleanedOutput.trim().length === 0`.
    if js_trim(&params.cleaned_output).is_empty() {
        return None;
    }

    let elided = elide_for_chat(&params.cleaned_output);
    let fence = "`".repeat(compute_fence_length(&elided));
    // v4: `params.sessionId.slice(0, 8)`.
    let session_label = utf16_truncate(&params.session_id, 8);
    let reason_note = match params.reason {
        ArielFlushReason::SessionClosed => " (final, session closed)",
        ArielFlushReason::MaxAge => " (long-running, periodic flush)",
        ArielFlushReason::Idle => "",
    };

    let content = format!(
        "Terminal output from session `{session_label}`{reason_note}:\n\n{fence}\n{elided}\n{fence}"
    );

    // Terminal output body is already neutral — no persona reference, just the
    // session label and the elided output. Pass null so the swap falls through
    // to `content` for opaque chats too (v4 passes null).
    post_ariel_message(
        db,
        &params.chat_id,
        content,
        None,
        &format!("terminal-output-{}", params.reason.as_str()),
        &params.session_id,
    )
    .await
}

/// v4 `ArielSessionClosedAnnouncement`. `exit_code` is v4's `number | null`
/// (node-pty always reports an integer; `None` = unknown, e.g. reconcile).
#[derive(Debug, Clone)]
pub struct ArielSessionClosedAnnouncement {
    pub chat_id: String,
    pub session_id: String,
    pub exit_code: Option<i64>,
}

/// v4 `postArielSessionClosedAnnouncement`: announce the closing of a terminal
/// session with its exit code (or the could-not-recover phrasing for `None`).
pub async fn post_ariel_session_closed_announcement(
    db: &Db,
    params: &ArielSessionClosedAnnouncement,
) -> Option<PostedArielMessage> {
    // v4: `=== 0` → 'successfully'; `== null` → the restart phrasing; else
    // `with exit code ${code}`.
    let exit_label = match params.exit_code {
        Some(0) => "successfully".to_string(),
        None => "\u{2014} Ariel could not recover an exit code; the session may have ended when the server last restarted".to_string(),
        Some(code) => format!("with exit code {code}"),
    };
    let content = format!(
        "Ariel notes that the terminal session has closed \u{2014} it exited {exit_label}."
    );
    let opaque_content = format!("Terminal session closed \u{2014} it exited {exit_label}.");

    post_ariel_message(
        db,
        &params.chat_id,
        content,
        Some(opaque_content),
        "session-closed",
        &params.session_id,
    )
    .await
}

/// v4 `reconcileTerminalSessionsForChat` (`lib/terminal/reconcile.ts`).
///
/// After a host restart, DB rows for live PTY sessions are stale — the
/// underlying processes are gone but `exitedAt` is still null. This sweeps
/// those orphans for a given chat: marks them exited (`exitedAt = now`,
/// `exitCode = null` — an explicit NULL write) and posts an Ariel close
/// announcement so the user sees what happened.
///
/// `is_live` is the injected host probe (v4 `ptyManager.get(session.id)`): a
/// session with a live PTY is left alone. Errors are swallowed — v4's outer
/// try/catch returns 0 on any failure, dropping a partial count with it.
pub async fn reconcile_terminal_sessions_for_chat(
    db: &Db,
    chat_id: &str,
    is_live: &(dyn Fn(&str) -> bool + Sync),
) -> usize {
    let chat_id_owned = chat_id.to_string();
    let sessions =
        match db.read_main(|main| terminal_sessions::find_by_chat_id(main, &chat_id_owned)) {
            Ok(rows) => rows,
            Err(_) => return 0,
        };

    let mut reconciled = 0usize;
    for session in sessions {
        if session.exited_at.is_some() {
            continue;
        }
        if is_live(&session.id) {
            continue;
        }

        // v4: `const now = new Date().toISOString()` for the patch's `exitedAt`;
        // the base `_update` mints its own `updatedAt = now` (base.repository.ts:359).
        let now = crate::clock::now_iso();
        let session_id = session.id.clone();
        let exited_at = now.clone();
        let write_result = db
            .write(move |writers| {
                let updated_at = crate::clock::now_iso();
                terminal_sessions::mark_session_exited(
                    writers.main().connection(),
                    &session_id,
                    &exited_at,
                    None,
                    &updated_at,
                )
                .map(|_| ())
            })
            .await;
        if write_result.is_err() {
            // v4's try/catch spans the whole sweep — a failure aborts with 0.
            return 0;
        }

        post_ariel_session_closed_announcement(
            db,
            &ArielSessionClosedAnnouncement {
                chat_id: chat_id.to_string(),
                session_id: session.id.clone(),
                exit_code: None,
            },
        )
        .await;

        reconciled += 1;
    }

    reconciled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fence_length_minimum_three() {
        assert_eq!(compute_fence_length("no backticks"), 3);
        assert_eq!(compute_fence_length(""), 3);
    }

    #[test]
    fn fence_length_exceeds_longest_run() {
        // A run of 3 backticks → fence of 4; a run of 5 → 6.
        assert_eq!(compute_fence_length("code ```block``` end"), 4);
        assert_eq!(compute_fence_length("`````"), 6);
        // Two runs — the longest wins.
        assert_eq!(compute_fence_length("`` and ````"), 5);
    }

    #[test]
    fn elide_under_cap_is_identity() {
        let s = "x".repeat(TERMINAL_OUTPUT_MAX_CHARS);
        assert_eq!(elide_for_chat(&s), s);
    }

    #[test]
    fn elide_over_cap_keeps_head_and_tail() {
        // 20 000 units → head 8192 + tail 8192, 3616 elided ("3,616" grouped).
        let s: String = (0..20_000)
            .map(|i| (b'a' + (i % 26) as u8) as char)
            .collect();
        let out = elide_for_chat(&s);
        assert!(out.starts_with(&s[..8192]));
        assert!(out.ends_with(&s[20_000 - 8192..]));
        assert!(out.contains("\u{2026} [3,616 characters elided] \u{2026}"));
    }

    #[test]
    fn elide_counts_utf16_units_not_bytes() {
        // 'é' is 1 UTF-16 unit but 2 UTF-8 bytes: 17 000 of them exceed the
        // 16 384-unit cap by 616 even though a byte-based cap would see 34 000.
        let s: String = std::iter::repeat_n('\u{e9}', 17_000).collect();
        let out = elide_for_chat(&s);
        assert!(out.contains("[616 characters elided]"));
        // Head is 8192 é's (16 384 bytes).
        assert!(out.starts_with(&"\u{e9}".repeat(8192)));
    }

    #[test]
    fn reason_strings() {
        assert_eq!(ArielFlushReason::Idle.as_str(), "idle");
        assert_eq!(ArielFlushReason::MaxAge.as_str(), "max-age");
        assert_eq!(ArielFlushReason::SessionClosed.as_str(), "session-closed");
    }
}
