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
use crate::model::stream_watchdog::{watch_stream, StallBudgets, StallWatchdogContext};
use crate::services::llm_logging::{
    self, log_type, LogContext, LogLlmCallParams, LogRequest, LogRequestMessage, LogResponse,
    LogUsage,
};
use serde_json::Value;

/// The greeting's own stall budgets, tighter than the Salon's defaults (v4
/// `GREETING_FIRST_CHUNK_TIMEOUT_MS` / `GREETING_IDLE_TIMEOUT_MS`, bug 141).
///
/// This call is short and low-context — a sentence or two, no history — and it
/// runs inside the blocking Green Room dialog, where every second is a second
/// the operator spends looking at a dialog they cannot dismiss. A model that has
/// not begun a two-sentence opener in ninety seconds is not going to.
const GREETING_FIRST_CHUNK_TIMEOUT_MS: u64 = 90_000;
const GREETING_IDLE_TIMEOUT_MS: u64 = 60_000;

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

    // v4 `f90144ac4` (bug 141): the greeting is the ONE streaming consumer that
    // bypasses the Salon's funnel, so it wears the watchdog itself — a provider
    // that took the request, answered with headers and then went quiet used to
    // hold the whole create open, and with it the Green Room dialog the operator
    // cannot dismiss. The loop below is unchanged: a stall arrives as an
    // ordinary mid-stream `Err`, which is logged onto the `llm_logs` row and
    // rethrown exactly like any other (v4 `catch { streamError = …; throw err }`).
    let rx = streaming
        .stream_message(&req.provider, req.base_url.as_deref(), &params)
        .await;
    let mut rx = watch_stream(
        rx,
        StallBudgets {
            first_chunk_ms: GREETING_FIRST_CHUNK_TIMEOUT_MS,
            idle_ms: GREETING_IDLE_TIMEOUT_MS,
        },
        StallWatchdogContext {
            provider: &req.provider,
            model_name: &req.model_name,
            context: "initial-greeting",
            // v4's `logContext` spells the ids from `generateGreetingMessage`'s
            // own optional params; in v5 the two chat-scoped ones ride the
            // `GreetingLog` seam, which is `Some` on exactly the calls v4 has
            // them on.
            user_id: log.map(|l| l.user_id),
            chat_id: log.and_then(|l| l.chat_id),
            character_id: req.character_id.as_deref(),
            message_id: None,
        },
    );
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
    use crate::model::stream::{StreamChunk, StreamChunkResult, StreamErrorKind};
    use std::sync::{Arc, Mutex};
    use tokio::sync::mpsc;

    /// A provider that yields what it was given and then goes quiet FOREVER —
    /// the shape bug 141 is about. The `Sender`s are HELD (a dropped one closes
    /// the channel, which is the one thing a stalled socket does not do).
    #[derive(Default)]
    struct SilentAfter {
        prefix: Vec<String>,
        held: Arc<Mutex<Vec<mpsc::Sender<StreamChunkResult>>>>,
    }

    impl StreamingCompletionProvider for SilentAfter {
        fn stream_message(
            &self,
            _provider: &str,
            _base_url: Option<&str>,
            _params: &StreamParams,
        ) -> impl std::future::Future<Output = mpsc::Receiver<StreamChunkResult>> + Send {
            let prefix = self.prefix.clone();
            let held = Arc::clone(&self.held);
            async move {
                let (tx, rx) = mpsc::channel(prefix.len().max(1));
                for c in &prefix {
                    tx.try_send(Ok(StreamChunk::content(c))).unwrap();
                }
                held.lock().unwrap().push(tx);
                rx
            }
        }
    }

    fn greeting_request() -> GreetingRequest {
        GreetingRequest {
            system_prompt: "You are Aria, a knight.".into(),
            character_name: "Aria".into(),
            provider: "DEEPSEEK".into(),
            model_name: "deepseek-v4-flash".into(),
            api_key: "test-key".into(),
            character_id: Some("char-1".into()),
            ..Default::default()
        }
    }

    /// v4 bug 141: the greeting wears its OWN budgets (90s / 60s), not the
    /// Salon's 240s / 120s. Nothing else in this file can tell them apart, and
    /// no NDJSON corpus can observe a wall-clock timeout at all, so this is the
    /// pin for both numbers.
    #[tokio::test(start_paused = true)]
    async fn a_silent_provider_stalls_the_greeting_at_the_first_chunk_budget() {
        let provider = SilentAfter::default();
        let err = generate_greeting_message(&provider, &greeting_request(), None)
            .await
            .expect_err("a provider that never speaks must not resolve");
        assert!(err.is_stalled(), "{err:?}");
        assert_eq!(
            err.message,
            "Provider stream never sent a first chunk within 90000ms"
        );
        assert_eq!(
            err.kind,
            StreamErrorKind::Stalled {
                budget_ms: GREETING_FIRST_CHUNK_TIMEOUT_MS,
                chunks_received: 0,
                provider: Some("DEEPSEEK".into()),
                model_name: Some("deepseek-v4-flash".into()),
            }
        );
    }

    /// Once a chunk has landed the tighter between-chunks budget applies, and
    /// the count on the error is what the consumer actually saw.
    #[tokio::test(start_paused = true)]
    async fn a_provider_that_goes_quiet_mid_greeting_stalls_at_the_idle_budget() {
        let provider = SilentAfter {
            prefix: vec!["Well met".into()],
            ..Default::default()
        };
        let err = generate_greeting_message(&provider, &greeting_request(), None)
            .await
            .expect_err("a provider that goes quiet must not resolve");
        assert_eq!(
            err.message,
            "Provider stream went quiet for 60000ms after 1 chunk(s)"
        );
        assert!(matches!(
            err.kind,
            StreamErrorKind::Stalled {
                budget_ms: GREETING_IDLE_TIMEOUT_MS,
                chunks_received: 1,
                ..
            }
        ));
    }

    /// The `llm_logs` row v4 writes in its `finally` carries the stall's own
    /// sentence — the only place a silence is distinguishable from a refusal
    /// after the fact. v5 already spelled `error: streamError.message`; bug 141
    /// changes what can land there, so this pins the pair.
    #[tokio::test(start_paused = true)]
    async fn the_stall_reaches_the_llm_logs_row() {
        use crate::db::runtime::DbPaths;
        use crate::db::Writer;

        const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
        let dir = tempfile::tempdir().unwrap();
        let main_path = dir.path().join("main.db");
        let ll_path = dir.path().join("llm-logs.db");
        drop(Writer::open_writable(&main_path, PEPPER).unwrap());
        {
            let w = Writer::open_writable(&ll_path, PEPPER).unwrap();
            w.connection()
                .execute_batch(
                    "CREATE TABLE llm_logs (\
                       id TEXT PRIMARY KEY, userId TEXT, type TEXT, messageId TEXT, \
                       chatId TEXT, characterId TEXT, autonomousRunId TEXT, provider TEXT, \
                       modelName TEXT, connectionProfileId TEXT, imageProfileId TEXT, \
                       request TEXT, response TEXT, usage TEXT, \
                       cacheUsage TEXT, rawProviderUsage TEXT, requestHashes TEXT, \
                       durationMs REAL, createdAt TEXT, updatedAt TEXT);",
                )
                .unwrap();
        }
        let db = crate::db::runtime::Db::open(
            DbPaths {
                main: main_path,
                mount_index: None,
                llm_logs: Some(ll_path),
            },
            PEPPER,
        )
        .unwrap();

        let log = GreetingLog {
            db: &db,
            user_id: "user-1",
            chat_id: Some("chat-7"),
            log_context: LogContext::none(),
        };
        let provider = SilentAfter::default();
        let err = generate_greeting_message(&provider, &greeting_request(), Some(&log))
            .await
            .expect_err("a provider that never speaks must not resolve");
        assert!(err.is_stalled(), "{err:?}");

        let responses: Vec<String> = db
            .read_llm_logs(|conn| {
                let mut stmt = conn.prepare("SELECT response FROM llm_logs")?;
                let out = stmt
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(out)
            })
            .unwrap();
        assert_eq!(responses.len(), 1, "one CHAT_MESSAGE row: {responses:?}");
        let response: Value = serde_json::from_str(&responses[0]).unwrap();
        assert_eq!(
            response["error"].as_str(),
            Some("Provider stream never sent a first chunk within 90000ms"),
            "the greeting row must carry the stall\u{2019}s own sentence: {response}"
        );
    }

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
