//! The three cheap-LLM tasks the context-summary service drives (v4
//! `lib/memory/cheap-llm-tasks/chat-tasks.ts`): `foldChatSummary`,
//! `generateTitleFromSummary`, `generateHelpChatTitleFromSummary` — prompt
//! building + response handling byte-for-byte.
//!
//! Each is a thin wrapper over the ported [`CheapLlmTaskExecutor`] (v4
//! `executeCheapLLMTask`), with the exact system/user message pair v4 builds and
//! the exact response cleanup its `parseResponse` closure applies. None of the
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
use crate::memory_tasks::strip_code_fences;

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

/// v4's `parseResponse` for the title tasks: `content.trim()`, strip at most one
/// leading and one trailing quote (`/^["']|["']$/g`), then cap to 60 UTF-16
/// units (over-length → first 57 + `...`).
fn clean_title(content: &str) -> String {
    let mut title = js_trim(content).to_string();
    title = strip_edge_quotes(&title);
    if utf16_len(&title) > 60 {
        format!("{}...", utf16_truncate(&title, 57))
    } else {
        title
    }
}

/// `str.replace(/^["']|["']$/g, '')` — remove one leading quote char if present
/// AND one trailing quote char if present (the alternation is anchored, so the
/// global flag can match at most twice: once at start, once at end).
fn strip_edge_quotes(s: &str) -> String {
    let mut chars: Vec<char> = s.chars().collect();
    if matches!(chars.first(), Some('"' | '\'')) {
        chars.remove(0);
    }
    if matches!(chars.last(), Some('"' | '\'')) {
        chars.pop();
    }
    chars.into_iter().collect()
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

/// v4's `considerTitleUpdate` / `considerHelpChatTitleUpdate` result payload.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TitleConsideration {
    pub needs_new_title: bool,
    pub reason: String,
    pub suggested_title: Option<String>,
}

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

/// The shared parser: strip fences, `JSON.parse`, then coerce. `needsNewTitle` is
/// a STRICT `=== true`; `reason` falls back through `||` (so an empty string
/// becomes the placeholder); `suggestedTitle` is trimmed, de-quoted (ONE quote at
/// each end), capped at 60 UTF-16 units, and `|| null`-ed (so `''` → `None`).
/// Any parse failure → the "Failed to parse response" no-op result.
fn parse_consideration(content: &str) -> TitleConsideration {
    let clean = strip_code_fences(content);
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&clean) else {
        return TitleConsideration {
            needs_new_title: false,
            reason: "Failed to parse response".to_string(),
            suggested_title: None,
        };
    };

    // `parsed.suggestedTitle` — only cleaned when it's a truthy STRING (v4 guards
    // `if (suggestedTitle && typeof suggestedTitle === 'string')`); any other
    // truthy value passes through to the `|| null` below unchanged, and every
    // non-string is dropped there because it can't be a title.
    let suggested_title = match parsed.get("suggestedTitle").and_then(|v| v.as_str()) {
        Some(raw) if !raw.is_empty() => {
            let t = strip_edge_quotes(js_trim(raw));
            let t = if utf16_len(&t) > 60 {
                format!("{}...", utf16_truncate(&t, 57))
            } else {
                t
            };
            // `suggestedTitle || null` — a cleaned-to-empty title is falsy.
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        }
        _ => None,
    };

    TitleConsideration {
        needs_new_title: parsed.get("needsNewTitle") == Some(&serde_json::Value::Bool(true)),
        // `parsed.reason || 'No reason provided'`.
        reason: match parsed.get("reason").and_then(|v| v.as_str()) {
            Some(r) if !r.is_empty() => r.to_string(),
            _ => "No reason provided".to_string(),
        },
        suggested_title,
    }
}

/// v4 `considerTitleUpdate`: does this chat need a new (literary) title?
pub async fn consider_title_update<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    current_title: &str,
    recent_messages: &[ChatMessage],
    existing_summary_or_title: Option<&str>,
    selection: &CheapLlmSelection,
) -> CheapLlmTaskResult<TitleConsideration> {
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
            parse_consideration,
            None,
            None,
            None,
            Some("consider-title-update"),
        )
        .await
}

/// v4 `considerHelpChatTitleUpdate`: the same evaluation for a help-like chat —
/// same user message, same parser, a practical (non-literary) system prompt.
/// v4 passes the SAME `'consider-title-update'` task type as the literary arm.
pub async fn consider_help_chat_title_update<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    current_title: &str,
    recent_messages: &[ChatMessage],
    existing_summary_or_title: Option<&str>,
    selection: &CheapLlmSelection,
) -> CheapLlmTaskResult<TitleConsideration> {
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
            parse_consideration,
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

/// The MANUAL generators' cleaner: the same trim + [`strip_edge_quotes`] the
/// evaluators use, but with the caller's clamp — `titleChat` caps at **50**
/// (57/60 belongs to the evaluator and to `titleHelpChat`).
fn clean_generated_title(content: &str, max: usize) -> String {
    let title = strip_edge_quotes(js_trim(content));
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
    fn consideration_parses_and_cleans() {
        let r = parse_consideration(
            r#"{"needsNewTitle":true,"reason":"topic shifted","suggestedTitle":"  \"A Ballonet Betrayed\"  "}"#,
        );
        assert!(r.needs_new_title);
        assert_eq!(r.reason, "topic shifted");
        assert_eq!(r.suggested_title.as_deref(), Some("A Ballonet Betrayed"));
    }

    #[test]
    fn consideration_defaults_and_strictness() {
        // `needsNewTitle` is a STRICT === true: the string "true" is not enough.
        let r = parse_consideration(r#"{"needsNewTitle":"true"}"#);
        assert!(!r.needs_new_title);
        assert_eq!(r.reason, "No reason provided");
        assert_eq!(r.suggested_title, None);
        // A null title stays None.
        let r = parse_consideration(r#"{"needsNewTitle":true,"suggestedTitle":null}"#);
        assert!(r.needs_new_title);
        assert_eq!(r.suggested_title, None);
    }

    #[test]
    fn consideration_unparseable_is_a_no_op() {
        let r = parse_consideration("I am not JSON");
        assert!(!r.needs_new_title);
        assert_eq!(r.reason, "Failed to parse response");
        assert_eq!(r.suggested_title, None);
    }

    #[test]
    fn consideration_title_is_capped_at_60() {
        let long = "b".repeat(80);
        let r = parse_consideration(&format!(r#"{{"suggestedTitle":"{long}"}}"#));
        let t = r.suggested_title.unwrap();
        assert_eq!(utf16_len(&t), 60);
        assert!(t.ends_with("..."));
    }

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
}
