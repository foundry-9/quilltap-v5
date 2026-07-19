//! Custom-tool LLM consults — the real invoker behind
//! [`execute_custom_tool`](super::custom_tools::execute_custom_tool)'s
//! `llm_invoke` seam (v4 `lib/pascal/llm-consult.ts`).
//!
//! A definition with an `llm` block wants a generated answer folded into its
//! outcome table. This module is the one place that turns that want into an
//! actual provider call: it resolves the instance's cheap-LLM selection (the
//! same machinery titling and memory extraction ride), reroutes to the
//! uncensored profile when the chat is flagged dangerous, poses the rendered
//! prompt as a single user message with no framing of ours — the author's
//! prompt is the whole conversation — and hands back the raw answer.
//!
//! Failure semantics live one level up, in `resolve_llm_consult`
//! ([`super::custom_tools`]): whatever goes wrong here becomes
//! `LlmInvokeResult::Failed { reason }`, and the execution core swaps in the
//! author's `errorMessage`. Nothing that fails here ever reaches the fiction.
//!
//! ## The 60 s timeout
//!
//! v4 wraps each consult in `withTimeout(…, CONSULT_TIMEOUT_MS)`, rejecting a
//! hung provider so it becomes the author's error message rather than a wedged
//! chat. Timers are host-side in this port (the standing rule the
//! answer-confirmation service records), so [`CONSULT_TIMEOUT_MS`] and
//! [`consult_timeout_reason`] live here while the timer itself is
//! `quilltap-host`'s `TimeoutConsult` decorator (P4.6bd), which wraps the WHOLE
//! invoker — the boundary v4 puts `withTimeout` at — and maps elapsed to the
//! ordinary `Failed { reason }` this module's reason string names.

use serde_json::Value;

use crate::cheap_llm::{
    get_cheap_llm_provider, resolve_uncensored_cheap_llm_selection, CheapLlmProfile,
    CheapLlmSelection, DangerousContentSettings,
};
use crate::db::runtime::Db;
use crate::db::{chats_read, connection_profiles};
use crate::model::completion::{CompletionMessage, CompletionProvider};
use crate::pascal::custom_tools::{LlmInvokeOptions, LlmInvokeResult, LlmInvoker};
use crate::services::cheap_llm_exec::{CheapLlmLogConfig, CheapLlmTaskExecutor};
use crate::services::dangerous_content::chat_override::is_chat_active_dangerous;
use crate::services::dangerous_content::resolver::resolve_dangerous_content_settings;
use crate::services::image_job_common::{
    cheap_llm_config_from_settings, cheap_llm_profile_from_value,
};
use crate::services::llm_logging::LogContext;

/// Hard ceiling on one consult, in milliseconds (v4 `CONSULT_TIMEOUT_MS`). The
/// cheap-LLM pipeline carries no timeout of its own (provider HTTP clients
/// govern), but a consult blocks a tool call in a live turn — a hung provider
/// must become the author's error message, not a wedged chat.
///
/// The timer itself is host-side (`quilltap-core` has no tokio timer driver in
/// its default build): `quilltap-host/src/spine.rs`'s `TimeoutConsult` wraps
/// [`CustomToolLlmInvoker`] with `tokio::time::timeout(CONSULT_TIMEOUT_MS, …)`,
/// mapping elapsed → `LlmInvokeResult::Failed { reason:
/// consult_timeout_reason(CONSULT_TIMEOUT_MS) }` (the P4.d8 deferral, closed by
/// P4.6bd).
pub const CONSULT_TIMEOUT_MS: i64 = 60_000;

/// v4's timeout rejection message, verbatim:
/// `` `the consult timed out after ${Math.round(ms / 1000)}s` ``.
pub fn consult_timeout_reason(ms: i64) -> String {
    // JS `Math.round` is half-up toward +Infinity; the division is float.
    let seconds = (ms as f64 / 1000.0 + 0.5).floor();
    format!(
        "the consult timed out after {}s",
        crate::pascal::js_value::number_to_string(seconds)
    )
}

/// Who is consulting, and from where (v4 `CustomToolConsultContext`).
#[derive(Debug, Clone)]
pub struct CustomToolConsultContext {
    pub user_id: String,
    /// The chat the run belongs to — drives Concierge rerouting and log
    /// attribution. `None` for Pascal's Workbench, whose bench runs belong to
    /// no room and are never rerouted.
    pub chat_id: Option<String>,
}

/// The assembly-level consult seam (P4.6bd). v4 needs no such thing — its
/// `buildCustomToolLlmInvoker` reaches the wire ambiently — but v5's engine
/// holds no `CompletionProvider` (the trait is RPITIT, so it cannot be erased
/// directly; see `api::custom_tools`'s dyn-compatibility note). So the erased
/// object is this RUNNER, matching the `image_generation` idiom: the host's
/// impl holds the wire config and rebuilds the concrete provider per consult,
/// which is what keeps a cheap-model change live on the very next roll and
/// puts the request's user/chat on the log row. The per-request `db` +
/// `context` are passed per call because the seam is assembled once per unlock
/// while every consult belongs to a specific request.
pub trait ConsultRunner: Send + Sync {
    fn consult<'a>(
        &'a self,
        db: &'a Db,
        context: CustomToolConsultContext,
        prompt: &'a str,
        options: LlmInvokeOptions,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = LlmInvokeResult> + Send + 'a>>;
}

/// The per-request [`LlmInvoker`] an entrance builds over the assembled
/// [`ConsultRunner`] — the shape `execute_custom_tool`'s `llm_invoke` seam
/// consumes. One per run, like v4's one-invoker-per-context.
pub struct SeamConsultInvoker<'a> {
    pub runner: &'a dyn ConsultRunner,
    pub db: &'a Db,
    pub context: CustomToolConsultContext,
}

impl LlmInvoker for SeamConsultInvoker<'_> {
    fn invoke<'b>(
        &'b self,
        prompt: &'b str,
        options: LlmInvokeOptions,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = LlmInvokeResult> + Send + 'b>> {
        self.runner
            .consult(self.db, self.context.clone(), prompt, options)
    }
}

/// A [`ConsultRunner`] over one held provider — the differential harness's
/// runner (a `CannedCompletionProvider` behind the real invoker), and the
/// simplest possible host wiring. The production host does NOT use this: its
/// runner rebuilds the wire provider per consult and adds the 60 s timeout
/// decorator (`quilltap-host`'s spine).
pub struct ProviderConsultRunner<C> {
    pub completion: C,
}

impl<C> ConsultRunner for ProviderConsultRunner<C>
where
    C: CompletionProvider + Send + Sync,
{
    fn consult<'a>(
        &'a self,
        db: &'a Db,
        context: CustomToolConsultContext,
        prompt: &'a str,
        options: LlmInvokeOptions,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = LlmInvokeResult> + Send + 'a>> {
        Box::pin(async move {
            let invoker = CustomToolLlmInvoker::new(db, &self.completion, context);
            invoker.invoke(prompt, options).await
        })
    }
}

/// Token budget for one consult, scaled from the effective output cap so a
/// long-form consult is not starved at the provider (v4 `consultMaxTokens`).
/// ~3 characters per token is a safe under-estimate for English-with-markdown,
/// and the floor matches the cheap-LLM pipeline's own (it clamps anything lower
/// up to 2048 anyway). The ceiling keeps a runaway `maxOutput` from requesting
/// an absurd budget — providers clamp to their models' real limits beyond it
/// regardless.
///
/// Float arithmetic throughout, mirroring JS: `Math.min(Math.max(Math.ceil(
/// chars / 3), 2048), 32_768)`.
pub fn consult_max_tokens(max_output_chars: f64) -> f64 {
    let ceil = (max_output_chars / 3.0).ceil();
    // `clamp` rather than `max().min()`: Rust's `f64::max` IGNORES a NaN input
    // (`NaN.max(2048.0)` is 2048.0) where JS's `Math.max` propagates it, and
    // `clamp` propagates like JS. The caller's cap is an integer so NaN is
    // unreachable, but the faithful form costs nothing.
    ceil.clamp(2048.0, 32_768.0)
}

/// The real invoker an entrance hands to `execute_custom_tool`. One per run
/// context; each invocation resolves settings and profiles FRESH, so a
/// cheap-model change is live on the very next roll.
pub struct CustomToolLlmInvoker<'a, C> {
    pub db: &'a Db,
    pub completion: &'a C,
    pub context: CustomToolConsultContext,
}

impl<'a, C> CustomToolLlmInvoker<'a, C>
where
    C: CompletionProvider + Send + Sync,
{
    pub fn new(db: &'a Db, completion: &'a C, context: CustomToolConsultContext) -> Self {
        Self {
            db,
            completion,
            context,
        }
    }

    /// v4's private `consult`. Every failure route returns `Failed { reason }`.
    async fn consult(&self, prompt: &str, options: LlmInvokeOptions) -> LlmInvokeResult {
        let user_id = self.context.user_id.clone();
        let chat_id = self.context.chat_id.clone();

        // v4 reads chatSettings + connections in parallel; the port's reads are
        // synchronous against the same connection, so they are sequential here.
        let uid = user_id.clone();
        let chat_settings = match self
            .db
            .read_main(move |c| crate::db::chat_settings::find_by_user_id(c, &uid))
        {
            Ok(s) => s,
            Err(e) => return failed(e.to_string()),
        };

        let uid = user_id.clone();
        let profile_values = match self
            .db
            .read_main(move |c| connection_profiles::find_by_user_id(c, &uid))
        {
            Ok(p) => p,
            Err(e) => return failed(e.to_string()),
        };
        if profile_values.is_empty() {
            return failed("no connection profiles are configured");
        }
        let profiles: Vec<CheapLlmProfile> = profile_values
            .iter()
            .map(cheap_llm_profile_from_value)
            .collect();

        let chat: Option<Value> = match &chat_id {
            Some(id) => {
                let id = id.clone();
                match self.db.read_main(move |c| chats_read::find_by_id(c, &id)) {
                    Ok(c) => c,
                    Err(e) => return failed(e.to_string()),
                }
            }
            None => None,
        };

        // Selection priority is the standard cheap-LLM ladder; `profiles[0]` is
        // only the last-resort "current profile" the ladder falls back to when
        // nothing is marked cheap — a consult has no chat-turn profile of its
        // own to offer.
        let cheap_settings = chat_settings
            .as_ref()
            .and_then(|s| s.get("cheapLLMSettings"));
        let mut selection: CheapLlmSelection = get_cheap_llm_provider(
            &profiles[0],
            &cheap_llm_config_from_settings(cheap_settings),
            &profiles,
            false,
            None,
        );

        let global_danger = chat_settings
            .as_ref()
            .and_then(|s| s.get("dangerousContentSettings"))
            .and_then(|d| serde_json::from_value(d.clone()).ok());
        let resolved_danger =
            resolve_dangerous_content_settings(global_danger, chat.as_ref()).settings;
        let danger_settings = DangerousContentSettings {
            mode: resolved_danger.mode,
            uncensored_text_profile_id: resolved_danger.uncensored_text_profile_id,
        };
        // A null chat is NEVER dangerous, which is exactly why the Workbench
        // bench run is never rerouted.
        let dangerous = is_chat_active_dangerous(chat.as_ref());
        if dangerous {
            selection = resolve_uncensored_cheap_llm_selection(
                selection,
                true,
                Some(&danger_settings),
                &profiles,
            );
        }

        let max_tokens = consult_max_tokens(options.max_output_chars as f64);

        // v4's `logger.debug('Custom-tool consult dispatching', {…})` — the
        // same eight fields at the same point. The core has no logging crate;
        // `eprintln!` is the module convention (as in `help_doc_sync`).
        // `promptLength` is JS `prompt.length`, a UTF-16 count.
        eprintln!(
            "[pascal.llm-consult] Custom-tool consult dispatching \
             (chatId {}, provider {}, model {}, dangerous {}, promptLength {}, \
             maxOutputChars {}, maxTokens {})",
            chat_id.as_deref().unwrap_or("null"),
            selection.provider,
            selection.model_name,
            dangerous,
            crate::jsstr::utf16_len(prompt),
            options.max_output_chars,
            max_tokens
        );

        let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
            db: self.db.clone(),
            user_id: user_id.clone(),
            chat_id: chat_id.clone(),
            message_id: None,
            ctx: LogContext::none(),
        });

        let task = executor
            .execute(
                self.completion,
                &selection,
                // The author's rendered prompt is the entire conversation. No
                // system framing of ours: what the oracle is asked is the
                // author's to write.
                vec![CompletionMessage::user(prompt)],
                // v4's identity parse — the raw content IS the answer.
                |content: &str| content.to_string(),
                Some(&crate::cheap_llm::UncensoredFallbackOptions {
                    danger_settings: &danger_settings,
                    available_profiles: &profiles,
                    is_dangerous_chat: Some(dangerous),
                }),
                Some(max_tokens),
                None,
                Some("custom-tool-consult"),
            )
            .await;

        match (task.success, task.result) {
            (true, Some(output)) => LlmInvokeResult::Answered {
                output,
                provider: Some(selection.provider.clone()),
                model: Some(selection.model_name.clone()),
            },
            _ => failed(
                task.error
                    .unwrap_or_else(|| "the model returned nothing".into()),
            ),
        }
    }
}

fn failed(reason: impl Into<String>) -> LlmInvokeResult {
    LlmInvokeResult::Failed {
        reason: reason.into(),
    }
}

impl<C> LlmInvoker for CustomToolLlmInvoker<'_, C>
where
    C: CompletionProvider + Send + Sync,
{
    fn invoke<'b>(
        &'b self,
        prompt: &'b str,
        options: LlmInvokeOptions,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = LlmInvokeResult> + Send + 'b>> {
        Box::pin(self.consult(prompt, options))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pascal::custom_tool_types::MAX_LLM_OUTPUT_LENGTH;

    #[test]
    fn consult_max_tokens_matches_v4() {
        // The floor: anything under 6,144 characters clamps up to 2,048.
        assert_eq!(consult_max_tokens(1.0), 2048.0);
        assert_eq!(consult_max_tokens(6144.0), 2048.0);
        // Just past the floor, the ceil is what counts.
        assert_eq!(consult_max_tokens(6145.0), 2049.0);
        // The default cap.
        assert_eq!(consult_max_tokens(MAX_LLM_OUTPUT_LENGTH as f64), 2667.0);
        // The ceiling: 98,304 characters is exactly 32,768 tokens.
        assert_eq!(consult_max_tokens(98_304.0), 32_768.0);
        assert_eq!(consult_max_tokens(100_000.0), 32_768.0);
    }

    #[test]
    fn timeout_reason_matches_v4() {
        assert_eq!(
            consult_timeout_reason(CONSULT_TIMEOUT_MS),
            "the consult timed out after 60s"
        );
        // `Math.round` is half-up: 1,500 ms rounds to 2s, 1,499 to 1s.
        assert_eq!(
            consult_timeout_reason(1_500),
            "the consult timed out after 2s"
        );
        assert_eq!(
            consult_timeout_reason(1_499),
            "the consult timed out after 1s"
        );
    }
}
