//! Query distillation (v4 `extractMemorySearchKeywords`, `memory-tasks.ts`) — the
//! cheap-LLM task that turns the recent conversation window into a natural-language
//! paraphrase + keyword bag + a turn-level temporal/context guess, used to drive
//! the dynamic-head memory search.
//!
//! Closes the `BuildContextSeams::distill_memory_search` seam (W4.6a).
//! build_context consumes `paraphrase` (preferred) then `keywords`; `temporal` /
//! `context` are parsed for fidelity (the search-leg recall re-rank that consumes
//! them is a tracked memory-service deferral). The prompt body is the byte-exact
//! `MEMORY_KEYWORD_EXTRACTION_PROMPT` in the generated `prompt_text` submodule.

use serde_json::Value;

use crate::cheap_llm::CheapLlmSelection;
use crate::memory_tasks::strip_code_fences;
use crate::model::completion::{CompletionMessage, CompletionProvider};
use crate::recall_tags::{ContextTag, TemporalTag};
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;

use super::prompt_text::MEMORY_KEYWORD_EXTRACTION_PROMPT;

/// v4 `MemorySearchExtraction` (the subset build_context consumes). `paraphrase`
/// wins over `keywords` as the embedding query.
#[derive(Clone, Debug, Default)]
pub struct DistilledSearch {
    pub keywords: Vec<String>,
    pub temporal: Option<TemporalTag>,
    pub context: Option<ContextTag>,
    pub paraphrase: Option<String>,
}

/// A conversation message for the distill window (already role-lowercased + the
/// assistant slice tool-stripped by the caller, matching v4's `recentForDistill`).
#[derive(Clone, Debug)]
pub struct DistillMessage {
    pub role: String,
    pub content: String,
}

/// v4 `extractMemorySearchKeywords`: build the conversation text (last 20 messages,
/// each capped at 500 chars), call the cheap LLM, and parse the response. Returns
/// `None` when the task fails (v4 `{ success: false }`) so the caller falls back to
/// the raw recent-window query.
pub async fn distill_memory_search<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    recent_messages: &[DistillMessage],
    character_name: &str,
    selection: &CheapLlmSelection,
    character_id: &str,
) -> Option<DistilledSearch> {
    // v4: `recentMessages.slice(-20)`.
    let start = recent_messages.len().saturating_sub(20);
    let capped = &recent_messages[start..];
    let conversation_text = capped
        .iter()
        .map(|m| {
            let speaker = match m.role.as_str() {
                "user" => "User",
                "assistant" => "Character",
                _ => "System",
            };
            // v4: `content.length > 500 ? content.substring(0, 500) + '...' : content`
            // (`.length`/`.substring` are UTF-16 code-unit ops).
            let content = if crate::jsstr::utf16_len(&m.content) > 500 {
                format!("{}...", crate::jsstr::utf16_truncate(&m.content, 500))
            } else {
                m.content.clone()
            };
            format!("{speaker}: {content}")
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let user_content = format!(
        "Character: {character_name}\n\nRecent conversation:\n{conversation_text}\n\nExtract keywords for searching {character_name}'s memories:"
    );

    let messages = vec![
        CompletionMessage::system(MEMORY_KEYWORD_EXTRACTION_PROMPT),
        CompletionMessage::user(user_content),
    ];

    let result = executor
        .execute(
            completion,
            selection,
            messages,
            parse_extraction,
            None,
            None,
            Some(character_id),
        )
        .await;

    if !result.success {
        return None;
    }
    result.result
}

/// v4's `parseResponse` closure for `extractMemorySearchKeywords`. Accepts either
/// the object shape `{ keywords, temporal, context, paraphrase }` or a bare
/// keyword array; a parse failure → `{ keywords: [] }` (still a success value).
fn parse_extraction(content: &str) -> DistilledSearch {
    let clean = strip_code_fences(content);
    let parsed: Value = match serde_json::from_str(&clean) {
        Ok(v) => v,
        Err(_) => return DistilledSearch::default(),
    };

    // `Array.isArray(parsed) ? parsed : parsed?.keywords`.
    let raw_keywords: Option<&Vec<Value>> = if parsed.is_array() {
        parsed.as_array()
    } else {
        parsed.get("keywords").and_then(Value::as_array)
    };
    let keywords = raw_keywords
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str())
                // `typeof item === 'string' && item.trim().length > 0`.
                .filter(|s| !crate::jsstr::js_trim(s).is_empty())
                .map(str::to_string)
                .take(10)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // temporal / context only exist on the object shape.
    let obj = if parsed.is_array() {
        None
    } else {
        parsed.as_object()
    };
    let raw_temporal = obj
        .and_then(|o| o.get("temporal"))
        .and_then(Value::as_str)
        .map(|s| crate::jsstr::js_trim(s).to_lowercase())
        .unwrap_or_default();
    let raw_context = obj
        .and_then(|o| o.get("context"))
        .and_then(Value::as_str)
        .map(|s| crate::jsstr::js_trim(s).to_lowercase())
        .unwrap_or_default();
    // v4 validates against the closed vocabularies; an invalid value → undefined.
    let temporal = TemporalTag::from_kw(&raw_temporal);
    let context = ContextTag::from_kw(&raw_context);

    let raw_paraphrase = obj
        .and_then(|o| o.get("paraphrase"))
        .and_then(Value::as_str)
        .map(|s| crate::jsstr::js_trim(s).to_string())
        .unwrap_or_default();
    let paraphrase = if raw_paraphrase.is_empty() {
        None
    } else {
        Some(raw_paraphrase)
    };

    DistilledSearch {
        keywords,
        temporal,
        context,
        paraphrase,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_object_shape() {
        let out = parse_extraction(
            r#"{"keywords":["a","b"," ","c"],"temporal":"Present","context":"relationships","paraphrase":"They talk."}"#,
        );
        assert_eq!(out.keywords, vec!["a", "b", "c"]);
        assert_eq!(out.temporal, Some(TemporalTag::Present));
        assert_eq!(out.context, Some(ContextTag::Relationships));
        assert_eq!(out.paraphrase.as_deref(), Some("They talk."));
    }

    #[test]
    fn parse_bare_array_shape() {
        let out = parse_extraction(r#"["x","y"]"#);
        assert_eq!(out.keywords, vec!["x", "y"]);
        assert_eq!(out.temporal, None);
        assert_eq!(out.context, None);
        assert_eq!(out.paraphrase, None);
    }

    #[test]
    fn parse_failure_yields_empty() {
        let out = parse_extraction("not json at all");
        assert!(out.keywords.is_empty());
        assert!(out.paraphrase.is_none());
    }

    #[test]
    fn keywords_capped_at_ten() {
        let arr: Vec<String> = (0..15).map(|i| format!("\"k{i}\"")).collect();
        let json = format!("{{\"keywords\":[{}]}}", arr.join(","));
        let out = parse_extraction(&json);
        assert_eq!(out.keywords.len(), 10);
    }

    #[test]
    fn code_fences_stripped() {
        let out = parse_extraction("```json\n{\"keywords\":[\"z\"]}\n```");
        assert_eq!(out.keywords, vec!["z"]);
    }
}
