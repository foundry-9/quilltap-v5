//! The three cheap-LLM tasks the context-summary service drives (v4
//! `lib/memory/cheap-llm-tasks/chat-tasks.ts`): `foldChatSummary`,
//! `generateTitleFromSummary`, `generateHelpChatTitleFromSummary` — prompt
//! building + response handling byte-for-byte.
//!
//! Each is a thin wrapper over the ported [`CheapLlmTaskExecutor`] (v4
//! `executeCheapLLMTask`), with the exact system/user message pair v4 builds and
//! the exact response cleanup its `parseResponse` closure applies. The two
//! title-consideration tasks parse through [`super::title_verdict`] (v4
//! `3c041e46` extracted the same body out of `chat-tasks.ts`). None of the
//! three passes an uncensored fallback, a max-tokens override, or a characterId
//! to the executor — so the provider call is temperature 0.3, max-tokens 2048,
//! `cacheKey = None` (matching v4's `buildCharacterCacheKey(undefined)`).

use crate::cheap_llm::CheapLlmSelection;
use crate::jsstr::{js_trim, utf16_len, utf16_truncate};
use crate::model::completion::{CompletionMessage, CompletionProvider};
use crate::services::cheap_llm_exec::{CheapLlmTaskExecutor, CheapLlmTaskResult};

use super::prompt_text::{
    CHAT_TITLE_CONSIDERATION_PROMPT, CHAT_TITLE_FROM_SUMMARY_PROMPT, CHAT_TITLE_PROMPT,
    FOLD_SUMMARY_PROMPT, HELP_CHAT_TITLE_CONSIDERATION_PROMPT, HELP_CHAT_TITLE_FROM_SUMMARY_PROMPT,
    HELP_CHAT_TITLE_PROMPT,
};
// === P4.D110 (v4 `3c041e46`, bug 96): the shared title-verdict parser ===
use super::title_verdict::{parse_title_verdict, strip_edge_quotes, TitleVerdict};
// === end P4.D110 ===

/// A conversation message the fold task renders (v4 `ChatMessage`: `role` is
/// already lowercased `'user' | 'assistant'`).
#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    /// The message's wall-clock stamp (v4's `ChatMessage & { createdAt? }`).
    /// When present, its DATE is rendered inline so the fold summary's
    /// Timeline section can date events from the transcript instead of
    /// guessing. Only [`fold_chat_summary`] reads it — the title tasks build
    /// their own rendering.
    pub created_at: Option<String>,
}

/// v4's `parseResponse` for the title tasks — since `dcab791c2` (the 4.9.0
/// dedup sweep) the four inline generator cleaners are collapsed onto
/// `cleanTitle`: trim, strip at most one leading and one trailing quote, then
/// trim AGAIN (padding tucked inside the quotes no longer survives), then cap
/// to 60 UTF-16 units (over-length → first 57 + `...`). The cap is measured
/// after the second trim, so a padded quoted title that used to truncate can
/// now survive whole.
fn clean_title(content: &str) -> String {
    let title = js_trim(&strip_edge_quotes(js_trim(content))).to_string();
    if utf16_len(&title) > 60 {
        format!("{}...", utf16_truncate(&title, 57))
    } else {
        title
    }
}

/// v4 `foldChatSummary`: fold a batch of new turns into the running summary.
/// Frames the call as an update task over the five-section structure (the fifth
/// being the append-only dated Timeline — the episodic spine's fold-side half);
/// the user content pairs the prior summary (or the first-fold placeholder)
/// with the rendered new turns, each prefixed by its date when stamped.
/// `parseResponse` is `content.trim()`.
pub async fn fold_chat_summary<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    prior_summary: Option<&str>,
    new_turns: &[ChatMessage],
    selection: &CheapLlmSelection,
) -> CheapLlmTaskResult<String> {
    let new_turns_text = new_turns
        .iter()
        .map(|m| {
            // v4: `[${m.createdAt.slice(0, 10)}] ` when the turn is stamped.
            let stamp = match m.created_at.as_deref().filter(|s| !s.is_empty()) {
                Some(created_at) => format!("[{}] ", utf16_truncate(created_at, 10)),
                None => String::new(),
            };
            format!("{stamp}{}: {}", m.role.to_uppercase(), m.content)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    // v4: `priorSummary && priorSummary.trim().length > 0 ? priorSummary.trim() : '(none …)'`.
    let prior_summary_block = match prior_summary {
        Some(p) if !js_trim(p).is_empty() => js_trim(p).to_string(),
        _ => "(none — this is the first fold)".to_string(),
    };

    let user_content = format!(
        "# Prior summary\n{prior_summary_block}\n\n# New turns to fold in\n{new_turns_text}"
    );

    let messages = vec![
        CompletionMessage::system(FOLD_SUMMARY_PROMPT),
        CompletionMessage::user(user_content),
    ];

    executor
        .execute(
            completion,
            selection,
            messages,
            |content| js_trim(content).to_string(),
            None,
            None,
            None,
            Some("fold-chat-summary"),
        )
        .await
}

/// v4 `generateTitleFromSummary`: a literary title from the summary.
pub async fn generate_title_from_summary<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    summary: &str,
    selection: &CheapLlmSelection,
) -> CheapLlmTaskResult<String> {
    let messages = vec![
        CompletionMessage::system(CHAT_TITLE_FROM_SUMMARY_PROMPT),
        CompletionMessage::user(format!("Summary:\n{summary}")),
    ];
    executor
        .execute(
            completion,
            selection,
            messages,
            clean_title,
            None,
            None,
            None,
            Some("title-from-summary"),
        )
        .await
}

/// v4 `generateHelpChatTitleFromSummary`: a practical (non-literary) title from
/// the summary for help/support chats. Same cleanup, different prompt body.
pub async fn generate_help_chat_title_from_summary<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    summary: &str,
    selection: &CheapLlmSelection,
) -> CheapLlmTaskResult<String> {
    let messages = vec![
        CompletionMessage::system(HELP_CHAT_TITLE_FROM_SUMMARY_PROMPT),
        CompletionMessage::user(format!("Summary:\n{summary}")),
    ];
    executor
        .execute(
            completion,
            selection,
            messages,
            clean_title,
            None,
            None,
            None,
            Some("title-from-summary"),
        )
        .await
}

// ===========================================================================
// considerTitleUpdate / considerHelpChatTitleUpdate (P4.6ao — the TITLE_UPDATE
// handler's two cheap-LLM tasks)
// ===========================================================================

/// The shared user message both consideration tasks build: the current title, the
/// context line, and the conversation window (each message's content truncated to
/// 500 UTF-16 units, role UPPERCASED).
fn consideration_user_content(
    current_title: &str,
    recent_messages: &[ChatMessage],
    existing_summary_or_title: Option<&str>,
) -> String {
    let conversation_text = recent_messages
        .iter()
        .map(|m| {
            format!(
                "{}: {}",
                m.role.to_uppercase(),
                utf16_truncate(&m.content, 500)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    // `existingSummaryOrTitle ? … : 'No previous context'` — JS-falsy, so an
    // EMPTY string takes the else branch too.
    let context_info = match existing_summary_or_title.filter(|s| !s.is_empty()) {
        Some(ctx) => format!("Previous context: {ctx}"),
        None => "No previous context".to_string(),
    };

    format!(
        "Current Title: \"{current_title}\"\n\n{context_info}\n\nRecent Messages:\n{conversation_text}"
    )
}

/// v4 `considerTitleUpdate`: does this chat need a new (literary) title?
///
/// `chat_id` names the chat in the parser's warn lines (v4 threads its own
/// `chatId` argument into `parseTitleVerdict`); the per-site `taskLabel` it
/// passes is `'consider-title-update'`, which is NOT the same string as the
/// `task_type` below — v4 happens to spell both the same here, and spells the
/// label differently on the help arm.
pub async fn consider_title_update<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    current_title: &str,
    recent_messages: &[ChatMessage],
    existing_summary_or_title: Option<&str>,
    selection: &CheapLlmSelection,
    chat_id: Option<&str>,
) -> CheapLlmTaskResult<TitleVerdict> {
    let messages = vec![
        CompletionMessage::system(CHAT_TITLE_CONSIDERATION_PROMPT),
        CompletionMessage::user(consideration_user_content(
            current_title,
            recent_messages,
            existing_summary_or_title,
        )),
    ];
    executor
        .execute(
            completion,
            selection,
            messages,
            |content| parse_title_verdict(content, "consider-title-update", chat_id),
            None,
            None,
            None,
            Some("consider-title-update"),
        )
        .await
}

/// v4 `considerHelpChatTitleUpdate`: the same evaluation for a help-like chat —
/// same user message, same parser, a practical (non-literary) system prompt.
/// v4 passes the SAME `'consider-title-update'` task type as the literary arm —
/// but a DIFFERENT parser `taskLabel` (`'consider-help-chat-title-update'`), so
/// a warn line names which of the two evaluators produced the unreadable
/// verdict.
pub async fn consider_help_chat_title_update<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    current_title: &str,
    recent_messages: &[ChatMessage],
    existing_summary_or_title: Option<&str>,
    selection: &CheapLlmSelection,
    chat_id: Option<&str>,
) -> CheapLlmTaskResult<TitleVerdict> {
    let messages = vec![
        CompletionMessage::system(HELP_CHAT_TITLE_CONSIDERATION_PROMPT),
        CompletionMessage::user(consideration_user_content(
            current_title,
            recent_messages,
            existing_summary_or_title,
        )),
    ];
    executor
        .execute(
            completion,
            selection,
            messages,
            |content| parse_title_verdict(content, "consider-help-chat-title-update", chat_id),
            None,
            None,
            None,
            Some("consider-title-update"),
        )
        .await
}

// ===========================================================================
// The MANUAL title generators (v4 `titleChat` / `titleHelpChat`)
// ===========================================================================
//
// These are NOT the JOB's evaluators. v4 has two distinct title paths and this
// port keeps both, because v4 does:
//
//   - the `TITLE_UPDATE` job asks "does this need a new title?" through
//     `considerTitleUpdate` (a verdict + a suggestion, `CHAT_TITLE_CONSIDERATION_PROMPT`);
//   - the MANUAL `?action=regenerate-title` asks "title this" outright through
//     `titleChat` (`CHAT_TITLE_PROMPT`), with a different weighting of the
//     transcript and a different clamp.
//
// The order's tier-2 item 7 asked for an audit that the two share ONE
// implementation; the answer is that v4 itself has two, by design, and v5
// mirrors that. (P4.9E3A)

/// v4 `titleChat`'s transcript rendering: the last 100 messages, with the last
/// TEN in full to 500 chars and everything earlier truncated to 150, joined by a
/// blank line and labelled with the upper-cased role.
fn title_conversation_text(messages: &[ChatMessage]) -> String {
    let capped: &[ChatMessage] = if messages.len() > 100 {
        &messages[messages.len() - 100..]
    } else {
        messages
    };
    let recent_threshold = capped.len().saturating_sub(10);
    capped
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let limit = if i >= recent_threshold { 500 } else { 150 };
            let text = if utf16_len(&m.content) > limit {
                format!("{}...", utf16_truncate(&m.content, limit))
            } else {
                m.content.clone()
            };
            format!("{}: {}", m.role.to_uppercase(), text)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// v4 `titleHelpChat`'s rendering: the last 20 messages, each to 500 chars.
fn help_title_conversation_text(messages: &[ChatMessage]) -> String {
    let capped: &[ChatMessage] = if messages.len() > 20 {
        &messages[messages.len() - 20..]
    } else {
        messages
    };
    capped
        .iter()
        .map(|m| {
            let text = if utf16_len(&m.content) > 500 {
                format!("{}...", utf16_truncate(&m.content, 500))
            } else {
                m.content.clone()
            };
            format!("{}: {}", m.role.to_uppercase(), text)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The MANUAL generators' cleaner: the same trim + [`strip_edge_quotes`] +
/// second trim (v4 `dcab791c2` — see [`clean_title`]) the evaluators use, but
/// with the caller's clamp — `titleChat` caps at **50** (57/60 belongs to the
/// evaluator and to `titleHelpChat`).
fn clean_generated_title(content: &str, max: usize) -> String {
    let title = js_trim(&strip_edge_quotes(js_trim(content))).to_string();
    if utf16_len(&title) > max {
        format!("{}...", utf16_truncate(&title, max - 3))
    } else {
        title
    }
}

/// v4 `titleChat`: generate a literary title outright (the MANUAL entrance).
/// `existing_title`, when non-empty, appends v4's "Current title / Update only
/// if…" rider to the system prompt.
pub async fn title_chat<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    messages: &[ChatMessage],
    existing_title: Option<&str>,
    selection: &CheapLlmSelection,
    chat_id: Option<&str>,
) -> CheapLlmTaskResult<String> {
    let mut prompt = CHAT_TITLE_PROMPT.to_string();
    if let Some(t) = existing_title.filter(|t| !t.is_empty()) {
        prompt.push_str(&format!(
            "\n\nCurrent title: \"{t}\"\nUpdate only if the conversation has evolved significantly."
        ));
    }
    let llm_messages = vec![
        CompletionMessage::system(prompt),
        CompletionMessage::user(title_conversation_text(messages)),
    ];
    let _ = chat_id;
    executor
        .execute(
            completion,
            selection,
            llm_messages,
            |content| clean_generated_title(content, 50),
            None,
            None,
            None,
            Some("title-chat"),
        )
        .await
}

/// v4 `titleHelpChat`: the practical counterpart. The rider is appended only
/// when the existing title does NOT already start with `Help:` (v4's own gate),
/// and the clamp is 60 rather than 50. v4 passes the SAME `'title-chat'` task
/// type as the literary arm.
pub async fn title_help_chat<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    messages: &[ChatMessage],
    existing_title: Option<&str>,
    selection: &CheapLlmSelection,
    chat_id: Option<&str>,
) -> CheapLlmTaskResult<String> {
    let mut prompt = HELP_CHAT_TITLE_PROMPT.to_string();
    if let Some(t) = existing_title.filter(|t| !t.is_empty() && !t.starts_with("Help:")) {
        prompt.push_str(&format!(
            "\n\nCurrent title: \"{t}\"\nUpdate only if the conversation topic has shifted significantly."
        ));
    }
    let llm_messages = vec![
        CompletionMessage::system(prompt),
        CompletionMessage::user(help_title_conversation_text(messages)),
    ];
    let _ = chat_id;
    executor
        .execute(
            completion,
            selection,
            llm_messages,
            |content| clean_generated_title(content, 60),
            None,
            None,
            None,
            Some("title-chat"),
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consideration_context_line_falls_back_when_empty() {
        // JS-falsy: an empty existing summary takes the 'No previous context' arm.
        let c = consideration_user_content("T", &[], Some(""));
        assert!(c.contains("No previous context"));
        let c = consideration_user_content("T", &[], Some("prior"));
        assert!(c.contains("Previous context: prior"));
    }

    #[test]
    fn clean_title_strips_quotes_and_caps() {
        assert_eq!(
            clean_title("  \"A Quiet Rebellion\"  "),
            "A Quiet Rebellion"
        );
        assert_eq!(clean_title("'single'"), "single");
        // 61 'a's → first 57 + '...'.
        let long = "a".repeat(61);
        let out = clean_title(&long);
        assert_eq!(utf16_len(&out), 60);
        assert!(out.ends_with("..."));
        assert_eq!(&out[..57], &"a".repeat(57));
    }

    #[test]
    fn clean_title_only_strips_one_each() {
        // Only one leading + one trailing quote removed.
        assert_eq!(clean_title("\"\"double\"\""), "\"double\"");
    }

    /// v4 `dcab791c2` collapsed the four inline generator cleaners onto
    /// `cleanTitle`, which trims a SECOND time after stripping the wrapping
    /// quote pair — padding tucked inside the quotes no longer survives into
    /// the stored title.
    #[test]
    fn clean_title_trims_again_after_stripping_quotes() {
        assert_eq!(clean_title("\" A Padded Title \""), "A Padded Title");
        assert_eq!(clean_generated_title("' padded '", 50), "padded");
    }

    /// The length cap is measured AFTER the second trim, so a padded quoted
    /// title that used to be truncated can now survive whole (v4's cap-60
    /// vector from the `dcab791c2` neutrality measurement).
    #[test]
    fn clean_title_caps_after_the_second_trim() {
        // 58 chars + a leading/trailing space inside the quotes = 60 before
        // the second trim (would truncate), 58 after it (survives whole).
        let inner = "b".repeat(58);
        let raw = format!("\" {inner} \"");
        assert_eq!(clean_title(&raw), inner);
    }
}
