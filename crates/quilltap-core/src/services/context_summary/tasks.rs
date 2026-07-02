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
    CHAT_TITLE_FROM_SUMMARY_PROMPT, FOLD_SUMMARY_PROMPT, HELP_CHAT_TITLE_FROM_SUMMARY_PROMPT,
};

/// A conversation message the fold task renders (v4 `ChatMessage`: `role` is
/// already lowercased `'user' | 'assistant'`).
#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
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
/// Frames the call as an update task over the four-section structure; the user
/// content pairs the prior summary (or the first-fold placeholder) with the
/// rendered new turns. `parseResponse` is `content.trim()`.
pub async fn fold_chat_summary<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    prior_summary: Option<&str>,
    new_turns: &[ChatMessage],
    selection: &CheapLlmSelection,
) -> CheapLlmTaskResult<String> {
    let new_turns_text = new_turns
        .iter()
        .map(|m| format!("{}: {}", m.role.to_uppercase(), m.content))
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
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

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
