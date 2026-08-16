//! The initial-greeting generator — v4's `lib/chat/initial-greeting.ts`
//! (`generateGreetingMessage`).
//!
//! Asks the configured LLM to produce a short in-character greeting for a fresh
//! chat, over the **streaming** model boundary (v4 consumes `streamMessage` and
//! concatenates the chunks server-side — the same path a normal reply takes).
//! Returns `{content, contentFilterDetected}` where `contentFilterDetected` flags
//! the "LLM burned completion tokens but returned nothing" case (a likely content
//! filter), which the route ladder uses to trigger the Concierge uncensored
//! reroute.
//!
//! The route-side ladder (`autoGenerateFirstMessage` — participant → profile →
//! key → context → the four-attempt retry matrix + the Concierge reroute) is the
//! `handleCreate` spine's (sub-unit 8), composed over this core; its branch
//! matrix rides the capstone.
//!
//! `logLLMCall` (a `CHAT_MESSAGE` row) is an optional injected config
//! ([`GreetingLog`], the `CheapLlmTaskExecutor::with_logging` precedent) — the
//! standalone differential drives this DB-free; the spine attaches logging.

use crate::cheap_llm::build_character_cache_key;
use crate::clock::now_unix_ms;
use crate::db::runtime::Db;
use crate::jsstr::js_trim;
use crate::model::stream::StreamMessage;
use crate::model::stream::{StreamError, StreamParams, StreamUsage, StreamingCompletionProvider};
use crate::services::llm_logging::{
    self, log_type, LogContext, LogLlmCallParams, LogRequest, LogRequestMessage, LogResponse,
    LogUsage,
};
use serde_json::Value;

/// A memory about another participant, for the greeting context (v4
/// `ParticipantMemoryForGreeting`).
#[derive(Clone, Debug, Default)]
pub struct ParticipantMemory {
    pub about_character_name: String,
    pub summary: String,
}

/// Project context for the greeting (v4 `ProjectContextForGreeting`).
#[derive(Clone, Debug, Default)]
pub struct ProjectContext {
    pub name: String,
    pub description: Option<String>,
    pub instructions: Option<String>,
}

/// A greeting request (v4 `GreetingRequest`).
#[derive(Clone, Debug, Default)]
pub struct GreetingRequest {
    pub system_prompt: String,
    pub character_name: String,
    pub provider: String,
    pub model_name: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i64>,
    pub top_p: Option<f64>,
    pub profile_parameters: Option<Value>,
    pub participant_memories: Vec<ParticipantMemory>,
    pub project_context: Option<ProjectContext>,
    pub recent_conversations_block: Option<String>,
    pub character_id: Option<String>,
}

/// v4 `GreetingResult`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GreetingResult {
    pub content: String,
    /// Reasoning / chain-of-thought text captured from a thinking model while it
    /// composed the greeting (v4 `23af7146`). DISPLAY ONLY — persisted onto the
    /// greeting message so the Salon renders its thinking fold like any other
    /// turn, and never fed back to any model. Empty when the model produced
    /// none.
    pub reasoning_content: String,
    /// v4 `contentFilterDetected`: the LLM burned completion tokens but returned
    /// no content (a likely content filter).
    pub content_filter_detected: bool,
}

/// Optional `logLLMCall` wiring (v4's fire-and-forget `CHAT_MESSAGE` write). The
/// spine attaches this; the standalone differential runs DB-free.
pub struct GreetingLog<'a> {
    pub db: &'a Db,
    pub user_id: &'a str,
    pub chat_id: Option<&'a str>,
    pub log_context: LogContext,
}

/// v4 `buildContextSection`: the project + memories + recent-conversations block
/// appended to the base system prompt.
fn build_context_section(
    project_context: Option<&ProjectContext>,
    participant_memories: &[ParticipantMemory],
    recent_conversations_block: Option<&str>,
) -> String {
    let mut sections: Vec<String> = Vec::new();

    if let Some(pc) = project_context {
        let mut parts = vec![format!("## Project Context: {}", pc.name)];
        if let Some(d) = pc.description.as_deref().filter(|s| !s.is_empty()) {
            parts.push(d.to_string());
        }
        if let Some(i) = pc.instructions.as_deref().filter(|s| !s.is_empty()) {
            parts.push(format!("### Project Instructions\n{i}"));
        }
        sections.push(parts.join("\n"));
    }

    if !participant_memories.is_empty() {
        let mut parts = vec!["## What You Remember About Other Participants".to_string()];
        // Group memories by character name (v4 preserves first-seen order).
        let mut order: Vec<String> = Vec::new();
        let mut by_char: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for m in participant_memories {
            by_char
                .entry(m.about_character_name.clone())
                .or_insert_with(|| {
                    order.push(m.about_character_name.clone());
                    Vec::new()
                })
                .push(m.summary.clone());
        }
        for name in &order {
            parts.push(format!("\nAbout {name}:"));
            for summary in &by_char[name] {
                parts.push(format!("- {summary}"));
            }
        }
        sections.push(parts.join("\n"));
    }

    if let Some(block) = recent_conversations_block.filter(|s| !s.is_empty()) {
        sections.push(block.to_string());
    }

    sections.join("\n\n")
}

/// v4 `generateGreetingMessage`: build the augmented prompt, stream a greeting,
/// accumulate, and return `{content, contentFilterDetected}`. A mid-stream error
/// is logged (when logging is configured) then propagated (v4 `throw err`).
pub async fn generate_greeting_message<S: StreamingCompletionProvider>(
    streaming: &S,
    req: &GreetingRequest,
    log: Option<&GreetingLog<'_>>,
) -> Result<GreetingResult, StreamError> {
    let context_section = build_context_section(
        req.project_context.as_ref(),
        &req.participant_memories,
        req.recent_conversations_block.as_deref(),
    );
    let base_prompt_with_context = if context_section.is_empty() {
        req.system_prompt.clone()
    } else {
        format!("{}\n\n{}", req.system_prompt, context_section)
    };
    let augmented_system_prompt = format!(
        "{base_prompt_with_context}\n\nYou are starting a brand new conversation. Before the user says anything, open with a concise greeting that fits {}'s established voice. Keep it to one or two sentences.",
        req.character_name
    );

    let messages = vec![
        StreamMessage::system(augmented_system_prompt),
        StreamMessage::user(
            "The chat is beginning now. Greet the user immediately in-character and invite them to engage.",
        ),
    ];

    let params = StreamParams {
        messages: messages.clone(),
        model: req.model_name.clone(),
        temperature: req.temperature,
        max_tokens: req.max_tokens,
        top_p: req.top_p,
        tools: None,
        web_search_enabled: false,
        profile_parameters: req.profile_parameters.clone(),
        cache_key: build_character_cache_key(req.character_id.as_deref()),
        previous_response_id: None,
        stop: Vec::new(),
        // v4 sets no `requestTimeoutMs` on any streaming call (P4.D83).
        request_timeout_ms: None,
    };

    let start = now_unix_ms();
    let mut accumulated = String::new();
    // Providers emit `reasoningContent` CUMULATIVELY — the full thinking-so-far
    // on every chunk that grows it, not a delta — so this is an ASSIGNMENT, not
    // a concatenation. The same contract the Salon's streaming path relies on.
    let mut accumulated_reasoning = String::new();
    let mut final_usage: Option<StreamUsage> = None;
    let mut stream_error: Option<StreamError> = None;

    let mut rx = streaming
        .stream_message(&req.provider, req.base_url.as_deref(), &params)
        .await;
    while let Some(item) = rx.recv().await {
        match item {
            Ok(chunk) => {
                if !chunk.content.is_empty() {
                    accumulated.push_str(&chunk.content);
                }
                // v4's `if (chunk.reasoningContent)` — JS truthiness, so an
                // empty string does NOT clear what came before.
                if let Some(r) = chunk.reasoning_content.as_deref().filter(|r| !r.is_empty()) {
                    accumulated_reasoning = r.to_string();
                }
                if let Some(u) = chunk.usage {
                    final_usage = Some(u);
                }
            }
            Err(e) => {
                stream_error = Some(e);
                break;
            }
        }
    }

    // v4's `finally` — log the call (fire-and-forget) whether it threw or not.
    if let Some(log) = log {
        let duration_ms = now_unix_ms() - start;
        let usage = final_usage.as_ref().map(|u| LogUsage {
            prompt_tokens: Some(u.prompt_tokens),
            completion_tokens: Some(u.completion_tokens),
            total_tokens: Some(u.total_tokens),
        });
        let params = LogLlmCallParams {
            user_id: log.user_id.to_string(),
            log_type: log_type::CHAT_MESSAGE.to_string(),
            message_id: None,
            chat_id: log.chat_id.map(str::to_string),
            character_id: req.character_id.clone(),
            provider: req.provider.clone(),
            model_name: req.model_name.clone(),
            // v4 `0cde7fbc` did NOT touch `lib/chat/initial-greeting.ts` — this
            // call site sets no profile id on either side.
            connection_profile_id: None,
            image_profile_id: None,
            request: LogRequest {
                messages: messages
                    .iter()
                    .map(|m| LogRequestMessage {
                        role: m.role_str().to_string(),
                        content: m.content().to_string(),
                        attachments: None,
                    })
                    .collect(),
                temperature: req.temperature,
                max_tokens: req.max_tokens,
                tools: None,
            },
            response: LogResponse {
                content: accumulated.clone(),
                error: stream_error.as_ref().map(|e| e.message.clone()),
                finish_reason: None,
                tool_calls: None,
            },
            usage,
            cache_usage: None,
            raw_provider_usage: None,
            request_hashes: None,
            duration_ms: Some(duration_ms as f64),
        };
        llm_logging::log_llm_call(log.db, params, &log.log_context).await;
    }

    if let Some(e) = stream_error {
        return Err(e);
    }

    let trimmed = js_trim(&accumulated).to_string();
    let trimmed_reasoning = js_trim(&accumulated_reasoning).to_string();
    let content_filter_detected = trimmed.is_empty()
        && final_usage
            .as_ref()
            .map(|u| u.completion_tokens > 0)
            .unwrap_or(false);

    Ok(GreetingResult {
        content: trimmed,
        reasoning_content: trimmed_reasoning,
        content_filter_detected,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_section_layout() {
        let pc = ProjectContext {
            name: "Voyage".into(),
            description: Some("A sea trek.".into()),
            instructions: Some("Be nautical.".into()),
        };
        let mems = vec![
            ParticipantMemory {
                about_character_name: "Bo".into(),
                summary: "likes tea".into(),
            },
            ParticipantMemory {
                about_character_name: "Bo".into(),
                summary: "fears storms".into(),
            },
        ];
        let out =
            build_context_section(Some(&pc), &mems, Some("### Recent Conversations\n- prior"));
        assert!(out.contains(
            "## Project Context: Voyage\nA sea trek.\n### Project Instructions\nBe nautical."
        ));
        assert!(out.contains("## What You Remember About Other Participants\n\nAbout Bo:\n- likes tea\n- fears storms"));
        assert!(out.contains("### Recent Conversations\n- prior"));
    }

    #[test]
    fn empty_context_section() {
        assert_eq!(build_context_section(None, &[], None), "");
    }
}
