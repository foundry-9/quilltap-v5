//! The context-compression **service half** (Phase-3 Unit-3 wave 3) — v4
//! `applyContextCompression` (`lib/chat/context/compression.ts:191`) +
//! `compressConversationHistory` (`lib/memory/cheap-llm-tasks/compression-tasks.ts`),
//! over the ported pure sizing leaves ([`crate::context_compression`]) and the
//! cheap-LLM executor ([`super::cheap_llm_exec`]).
//!
//! `applyContextCompression` splits the conversation into a sliding window,
//! summarizes everything before the window via the cheap-LLM task, and reports
//! the token savings. It makes **no DB writes** — the diff is result-object
//! equivalence — so the tier-3 differential compares the full
//! [`ContextCompressionResult`] JSON plus the recorded canned-completion keys.
//!
//! ## System-prompt compression is permanently disabled (v4 why-comment)
//!
//! v4 keeps a fresh, uncompressed per-character identity prompt on every turn
//! (each character has its own identity and it must not be compressed away).
//! So `compressSystemPrompt` is **never executed**: the result's
//! `compressedSystemPrompt` is always absent and the system-prompt token
//! counts are always `0`. We reproduce that result shape exactly rather than
//! porting the dead execution path (v4's `compressSystemPrompt` export is
//! unused by the send path).
//!
//! ## Deferred (tracked, both-sides-mocked)
//!
//! Inherited from [`super::cheap_llm_exec`]: API-key acquisition
//! (`getApiKeyForCheapLLMSelection`) and the fire-and-forget `logLLMCall` sit
//! host-side, so the tier-3 oracle mocks both (a constant key, a no-op logger)
//! and the port's boundary starts at the provider call. No compression-specific
//! deferral.

use serde::Serialize;

use crate::cheap_llm::{CheapLlmSelection, UncensoredFallbackOptions};
use crate::context_compression::{split_messages_for_compression, CompressibleMessage};
use crate::jsstr::{js_trim, utf16_len};
use crate::model::completion::{CompletionMessage, CompletionProvider};

use super::cheap_llm_exec::CheapLlmTaskExecutor;

/// v4's `MESSAGE_COMPRESSION_PROMPT` template (the system prompt for the
/// conversation-history compression task), with `{{userName}}` /
/// `{{characterName}}` / `{{targetTokens}}` placeholders. Reproduced verbatim.
const MESSAGE_COMPRESSION_PROMPT: &str = "You are a context compression assistant. Your job is to read a conversation between {{userName}} and {{characterName}}, and compress the older messages into a concise summary that preserves critical information while drastically reducing token count.

**What to PRESERVE:**
- Decisions made and conclusions reached
- Active projects, tasks, and goals
- Technical details that affect ongoing work (file paths, configurations, error messages, code solutions)
- Emotional or personal context that affects how {{characterName}} should engage
- Unresolved questions or threads
- Key facts about preferences, workflow, or situation

**What to DROP:**
- Exact wording of back-and-forth exchanges
- Redundant tool calls and their full results (keep only the outcome)
- Superseded information (if a bug was fixed, you don't need the original broken behavior)
- Conversational pleasantries and filler
- Verbose code snippets (summarize what was changed/fixed)
- Tangential side topics that have concluded

**Output Format:**
Provide a structured summary in plain text using these sections:

### Current Context
[One paragraph: What is being worked on? What is the immediate goal or task?]

### Recent Decisions & Outcomes
[Bullet list: What was decided, fixed, or accomplished in the older messages?]

### Active Threads
[Bullet list: What topics, questions, or tasks are still in progress or unresolved?]

### User State & Preferences
[One paragraph: Any important context about the user's situation, emotional state, preferences, or constraints.]

### Technical Details
[Bullet list or short paragraphs: File paths, commands, configurations, error messages, or code changes that may be referenced again.]

**Target length:** {{targetTokens}} tokens.";

/// v4 `estimateTokens`: rough token estimate, 1 token ≈ 4 characters, where
/// "characters" is the JS `String.length` (UTF-16 code units) and the divide is
/// `Math.ceil`.
fn estimate_tokens(s: &str) -> i64 {
    let len = utf16_len(s) as f64;
    (len / 4.0).ceil() as i64
}

/// Options for context compression (v4 `ContextCompressionOptions`). The token
/// targets are integers in practice (the settings schema bounds
/// `compressionTargetTokens` to [300, 2000]); v4's `String(targetTokens)`
/// substitution therefore renders a plain integer, which `i64::to_string`
/// reproduces byte-for-byte.
pub struct ContextCompressionOptions<'a> {
    /// Number of recent messages to keep in full (the sliding window size).
    pub window_size: i64,
    /// Target token count for the compressed conversation history.
    pub compression_target_tokens: i64,
    /// The resolved cheap-LLM selection for the compression call.
    pub selection: CheapLlmSelection,
    /// Character name for the compression prompt.
    pub character_name: String,
    /// User character name for the compression prompt.
    pub user_name: String,
    /// Uncensored-fallback options, present only when the caller supplied BOTH
    /// danger settings and available profiles (v4's `dangerSettings &&
    /// availableProfiles` gate).
    pub uncensored_fallback: Option<UncensoredFallbackOptions<'a>>,
}

/// Details about a compression that was applied (v4
/// `ContextCompressionResult.compressionDetails`). System-prompt fields are
/// always `0` (system-prompt compression is disabled).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressionDetails {
    pub original_message_count: i64,
    pub compressed_message_count: i64,
    pub window_message_count: i64,
    pub original_history_tokens: i64,
    pub compressed_history_tokens: i64,
    pub original_system_prompt_tokens: i64,
    pub compressed_system_prompt_tokens: i64,
    pub total_savings: i64,
}

/// Result of context compression (v4 `ContextCompressionResult`). Optional
/// fields are omitted when absent, matching v4's `JSON.stringify` dropping
/// `undefined` — `compressedSystemPrompt` is therefore never present.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextCompressionResult {
    /// Whether compression was applied.
    pub compression_applied: bool,
    /// Compressed conversation history (present only when applied).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compressed_history: Option<String>,
    /// Compressed system prompt — always `None` (disabled), so always omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compressed_system_prompt: Option<String>,
    /// Details about the compression (present only when applied).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression_details: Option<CompressionDetails>,
    /// Non-fatal errors that occurred during compression.
    pub warnings: Vec<String>,
}

/// The compressed text + token estimates a successful compression yields (v4
/// `CompressionResult`).
struct HistoryCompression {
    compressed_text: String,
    original_tokens: i64,
    compressed_tokens: i64,
}

/// v4 `compressConversationHistory`: format the older messages as
/// `<speaker>: <content>` blocks, build the system prompt from the template,
/// and run the shared cheap-LLM compression task (max-tokens 4000). Returns the
/// parsed [`HistoryCompression`] on success, or an error string (v4's
/// `historyResult.error`) on a `{ success: false }` task result.
///
/// The `parseCompressionResult` step trims the model output and re-estimates
/// its token count; the `originalTokens` is the estimate of the *joined
/// conversation text*, not of the individual messages.
#[allow(clippy::too_many_arguments)]
async fn compress_conversation_history<C: CompletionProvider>(
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    messages: &[CompressibleMessage],
    character_name: &str,
    user_name: &str,
    target_tokens: i64,
    selection: &CheapLlmSelection,
    uncensored_fallback: Option<&UncensoredFallbackOptions<'_>>,
) -> Result<HistoryCompression, String> {
    // Format messages for compression: user → userName, assistant →
    // characterName, anything else → "System"; blocks joined by a blank line.
    let conversation_text = messages
        .iter()
        .map(|m| {
            let speaker = match m.role.as_str() {
                "user" => user_name,
                "assistant" => character_name,
                _ => "System",
            };
            format!("{speaker}: {}", m.content)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let system_prompt = MESSAGE_COMPRESSION_PROMPT
        .replace("{{userName}}", user_name)
        .replace("{{characterName}}", character_name)
        // `String(targetTokens)` — the token target is always an integer.
        .replace("{{targetTokens}}", &target_tokens.to_string());

    let user_content =
        format!("Compress the following conversation history:\n\n{conversation_text}");
    let original_tokens = estimate_tokens(&conversation_text);

    let llm_messages = vec![
        CompletionMessage::system(system_prompt),
        CompletionMessage::user(user_content),
    ];

    // The shared execution wrapper: `runCompression` calls `executeCheapLLMTask`
    // with the standard `parseCompressionResult` parser, maxTokens = 4000, no
    // messageId, no characterId. The parser trims and re-estimates.
    let task = executor
        .execute(
            completion,
            selection,
            llm_messages,
            |content: &str| {
                let compressed_text = js_trim(content).to_string();
                let compressed_tokens = estimate_tokens(&compressed_text);
                HistoryCompression {
                    compressed_text,
                    original_tokens,
                    compressed_tokens,
                }
            },
            uncensored_fallback,
            Some(4000.0),
            None,
            Some("compress-conversation-history"),
        )
        .await;

    if task.success {
        // v4 gates on `historyResult.success && historyResult.result` — a
        // success with a present result. The executor always populates
        // `result` on success, mirroring that.
        task.result.ok_or_else(|| "undefined".to_string())
    } else {
        // v4: `Failed to compress conversation history: ${historyResult.error}`.
        // `historyResult.error` is the error string; a missing error stringifies
        // as JS `undefined`.
        Err(task.error.unwrap_or_else(|| "undefined".to_string()))
    }
}

/// v4 `applyContextCompression`: split the conversation at the sliding window,
/// compress everything before it via the cheap-LLM task, and report the token
/// savings. Makes no DB writes.
///
/// System-prompt compression is disabled (see the module docs) — the result
/// carries no `compressedSystemPrompt` and its system-prompt token counts are
/// always `0`.
pub async fn apply_context_compression<C: CompletionProvider>(
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    messages: &[CompressibleMessage],
    _system_prompt: &str,
    options: &ContextCompressionOptions<'_>,
) -> ContextCompressionResult {
    let mut warnings: Vec<String> = Vec::new();

    // Split into the messages to compress (older) and the window (recent).
    let split = split_messages_for_compression(messages, options.window_size);

    // Nothing to compress → early return (all within the window size).
    if split.messages_to_compress.is_empty() {
        return ContextCompressionResult {
            compression_applied: false,
            compressed_history: None,
            compressed_system_prompt: None,
            compression_details: None,
            warnings: vec!["No messages to compress (all within window size)".to_string()],
        };
    }

    let mut compressed_history: Option<String> = None;
    let mut history_compression: Option<HistoryCompression> = None;

    // Compress conversation history. v4 wraps the call in a try/catch: a
    // `{ success: false }` result pushes a "Failed to compress …" warning, while
    // a thrown error would push an "Error during conversation compression: …"
    // warning. Here the executor never throws — it converts every error into
    // `{ success: false, error }` — so only the "Failed to compress" branch is
    // reachable (the catch branch is v4 defensive coding around a never-throwing
    // callee; matched by the executor's total-function contract).
    match compress_conversation_history(
        completion,
        executor,
        &split.messages_to_compress,
        &options.character_name,
        &options.user_name,
        options.compression_target_tokens,
        &options.selection,
        options.uncensored_fallback.as_ref(),
    )
    .await
    {
        Ok(result) => {
            compressed_history = Some(result.compressed_text.clone());
            history_compression = Some(result);
        }
        Err(error) => {
            warnings.push(format!("Failed to compress conversation history: {error}"));
        }
    }

    // System prompt compression is disabled — always use a fresh per-character
    // prompt. Each character has their own identity and it must not be
    // compressed away.
    let compressed_system_prompt: Option<String> = None;

    let compression_applied = compressed_history.is_some();

    if compression_applied {
        let original_history_tokens = history_compression
            .as_ref()
            .map(|r| r.original_tokens)
            .unwrap_or(0);
        let compressed_history_tokens = history_compression
            .as_ref()
            .map(|r| r.compressed_tokens)
            .unwrap_or(0);
        // System-prompt compression is disabled, so these are always 0.
        let original_system_prompt_tokens = 0;
        let compressed_system_prompt_tokens = 0;

        let history_savings = original_history_tokens - compressed_history_tokens;
        let system_prompt_savings = 0;
        let total_savings = history_savings + system_prompt_savings;

        return ContextCompressionResult {
            compression_applied: true,
            compressed_history,
            compressed_system_prompt,
            compression_details: Some(CompressionDetails {
                original_message_count: messages.len() as i64,
                compressed_message_count: split.messages_to_compress.len() as i64,
                window_message_count: split.window_messages.len() as i64,
                original_history_tokens,
                compressed_history_tokens,
                original_system_prompt_tokens,
                compressed_system_prompt_tokens,
                total_savings,
            }),
            warnings,
        };
    }

    // Neither succeeded — return without compression applied.
    ContextCompressionResult {
        compression_applied: false,
        compressed_history: None,
        compressed_system_prompt: None,
        compression_details: None,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cheap_llm::{CheapLlmProfile, DangerousContentSettings, UncensoredFallbackOptions};
    use crate::model::completion::{CannedCompletionProvider, CompletionMessage};

    fn selection() -> CheapLlmSelection {
        CheapLlmSelection {
            provider: "ANTHROPIC".to_string(),
            model_name: "claude-haiku".to_string(),
            base_url: None,
            connection_profile_id: Some("cur".to_string()),
            is_local: false,
            profile_parameters: None,
        }
    }

    fn msg(role: &str, content: &str) -> CompressibleMessage {
        CompressibleMessage {
            role: role.to_string(),
            content: content.to_string(),
        }
    }

    /// Reproduce the exact system+user messages the port builds so the canned
    /// provider can be keyed on them (mirrors the harness's oracle-recorded
    /// replay).
    fn built_messages(
        messages: &[CompressibleMessage],
        character_name: &str,
        user_name: &str,
        target_tokens: i64,
    ) -> Vec<CompletionMessage> {
        let conversation_text = messages
            .iter()
            .map(|m| {
                let speaker = match m.role.as_str() {
                    "user" => user_name,
                    "assistant" => character_name,
                    _ => "System",
                };
                format!("{speaker}: {}", m.content)
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let system_prompt = MESSAGE_COMPRESSION_PROMPT
            .replace("{{userName}}", user_name)
            .replace("{{characterName}}", character_name)
            .replace("{{targetTokens}}", &target_tokens.to_string());
        vec![
            CompletionMessage::system(system_prompt),
            CompletionMessage::user(format!(
                "Compress the following conversation history:\n\n{conversation_text}"
            )),
        ]
    }

    #[test]
    fn estimate_tokens_ceils_over_utf16_len() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2); // 5/4 → ceil 2
                                                 // An astral char is two UTF-16 code units → 2/4 → ceil 1.
        assert_eq!(estimate_tokens("\u{1F600}"), 1);
    }

    #[tokio::test]
    async fn happy_path_applies_compression() {
        // 4 messages, windowSize 2 → compress the first 2.
        let messages = vec![
            msg("user", "let's plan the trip"),
            msg("assistant", "sounds good, where to?"),
            msg("user", "recent one"),
            msg("assistant", "recent reply"),
        ];
        let to_compress = &messages[..2];
        let built = built_messages(to_compress, "Aurora", "Charlie", 800);
        let provider = CannedCompletionProvider::new().with_response(
            "ANTHROPIC",
            "claude-haiku",
            Some(0.3),
            &built,
            "### Current Context\nPlanning a trip.",
            None,
        );
        let exec = CheapLlmTaskExecutor::new();
        let opts = ContextCompressionOptions {
            window_size: 2,
            compression_target_tokens: 800,
            selection: selection(),
            character_name: "Aurora".to_string(),
            user_name: "Charlie".to_string(),
            uncensored_fallback: None,
        };
        let result = apply_context_compression(&provider, &exec, &messages, "sys", &opts).await;

        assert!(result.compression_applied);
        assert_eq!(
            result.compressed_history.as_deref(),
            Some("### Current Context\nPlanning a trip.")
        );
        assert!(result.compressed_system_prompt.is_none());
        let details = result.compression_details.unwrap();
        assert_eq!(details.original_message_count, 4);
        assert_eq!(details.compressed_message_count, 2);
        assert_eq!(details.window_message_count, 2);
        assert_eq!(details.original_system_prompt_tokens, 0);
        assert_eq!(details.compressed_system_prompt_tokens, 0);
        assert_eq!(
            details.total_savings,
            details.original_history_tokens - details.compressed_history_tokens
        );
        assert!(result.warnings.is_empty());
    }

    #[tokio::test]
    async fn no_messages_to_compress_returns_early() {
        // 2 messages, windowSize 5 → nothing to compress.
        let messages = vec![msg("user", "hi"), msg("assistant", "hello")];
        let provider = CannedCompletionProvider::new();
        let exec = CheapLlmTaskExecutor::new();
        let opts = ContextCompressionOptions {
            window_size: 5,
            compression_target_tokens: 800,
            selection: selection(),
            character_name: "Aurora".to_string(),
            user_name: "Charlie".to_string(),
            uncensored_fallback: None,
        };
        let result = apply_context_compression(&provider, &exec, &messages, "sys", &opts).await;

        assert!(!result.compression_applied);
        assert!(result.compressed_history.is_none());
        assert!(result.compression_details.is_none());
        assert_eq!(
            result.warnings,
            vec!["No messages to compress (all within window size)".to_string()]
        );
    }

    #[tokio::test]
    async fn llm_failure_populates_warning() {
        let messages = vec![
            msg("user", "one"),
            msg("assistant", "two"),
            msg("user", "recent"),
        ];
        let to_compress = &messages[..1];
        let built = built_messages(to_compress, "Aurora", "Charlie", 800);
        // The provider fails every call with a fixed message; the executor
        // converts it to { success: false, error }, and apply pushes the
        // "Failed to compress …" warning.
        let provider = CannedCompletionProvider::new().with_failure(
            "ANTHROPIC",
            "claude-haiku",
            Some(0.3),
            &built,
            "provider exploded",
        );
        let exec = CheapLlmTaskExecutor::new();
        let opts = ContextCompressionOptions {
            window_size: 2,
            compression_target_tokens: 800,
            selection: selection(),
            character_name: "Aurora".to_string(),
            user_name: "Charlie".to_string(),
            uncensored_fallback: None,
        };
        let result = apply_context_compression(&provider, &exec, &messages, "sys", &opts).await;

        assert!(!result.compression_applied);
        assert!(result.compressed_history.is_none());
        assert!(result.compression_details.is_none());
        assert_eq!(
            result.warnings,
            vec!["Failed to compress conversation history: provider exploded".to_string()]
        );
    }

    #[tokio::test]
    async fn empty_response_falls_back_to_uncensored_and_applies() {
        // The safe provider returns empty; the uncensored fallback answers.
        let messages = vec![
            msg("user", "spicy one"),
            msg("assistant", "spicy two"),
            msg("user", "recent"),
        ];
        let to_compress = &messages[..1];
        let built = built_messages(to_compress, "Aurora", "Charlie", 800);
        let provider = CannedCompletionProvider::new()
            .with_response("ANTHROPIC", "claude-haiku", Some(0.3), &built, "", None)
            .with_response("OLLAMA", "dolphin", Some(0.3), &built, "summary text", None);
        let exec = CheapLlmTaskExecutor::new();

        let danger = DangerousContentSettings {
            mode: "AUTO_ROUTE".to_string(),
            uncensored_text_profile_id: Some("u1".to_string()),
        };
        let profiles = vec![
            CheapLlmProfile {
                id: "cur".to_string(),
                provider: "ANTHROPIC".to_string(),
                model_name: "claude-haiku".to_string(),
                ..Default::default()
            },
            CheapLlmProfile {
                id: "u1".to_string(),
                provider: "OLLAMA".to_string(),
                model_name: "dolphin".to_string(),
                ..Default::default()
            },
        ];
        let opts = ContextCompressionOptions {
            window_size: 2,
            compression_target_tokens: 800,
            selection: selection(),
            character_name: "Aurora".to_string(),
            user_name: "Charlie".to_string(),
            uncensored_fallback: Some(UncensoredFallbackOptions {
                danger_settings: &danger,
                available_profiles: &profiles,
                is_dangerous_chat: None,
            }),
        };
        let result = apply_context_compression(&provider, &exec, &messages, "sys", &opts).await;

        assert!(result.compression_applied);
        assert_eq!(result.compressed_history.as_deref(), Some("summary text"));
    }
}
