//! The shared cheap-LLM execution pipeline (v4
//! `lib/memory/cheap-llm-tasks/core-execution.ts`): temperature handling with
//! the session-level no-custom-temperature cache, the uncensored-provider
//! retry on empty responses, and the parse-and-wrap result shape. Generic over
//! [`CompletionProvider`] — the tier-3 seam sits at the provider call, exactly
//! where the v4 oracle mocks `createLLMProvider`'s returned provider.
//!
//! ## Deferred (tracked, out of scope for this unit)
//!
//!   - **API-key acquisition** (`getApiKeyForCheapLLMSelection`) — resolves
//!     before the provider call in v4 and throws when absent. Key management is
//!     host-side (the canned responder needs none; the real adapter arrives
//!     with the Phase-4 transport work), so the port's boundary starts at the
//!     provider call.
//!   - **`logLLMCall`** — v4 fire-and-forgets a row into the llm-logs DB (error
//!     swallowed, never awaited, so its timing is nondeterministic even in v4).
//!     The `llm_logs` repo is ported; wire the logging service when the host
//!     plumbing lands. The task-type → log-type mapping travels with it.

use std::collections::HashSet;
use std::sync::Mutex;

use crate::cheap_llm::{
    build_character_cache_key, profile_params, CheapLlmSelection, UncensoredFallbackOptions,
};
use crate::jsstr::js_trim;
use crate::model::completion::{
    CompletionError, CompletionMessage, CompletionParams, CompletionProvider, CompletionUsage,
};

/// Result of a cheap LLM task (v4 `CheapLLMTaskResult<T>`).
#[derive(Clone, Debug, PartialEq)]
pub struct CheapLlmTaskResult<T> {
    pub success: bool,
    pub result: Option<T>,
    pub error: Option<String>,
    pub usage: Option<CompletionUsage>,
}

impl<T> CheapLlmTaskResult<T> {
    /// v4's early-return shape: `{ success: true, result, usage: undefined }`.
    pub fn early(result: T) -> Self {
        Self {
            success: true,
            result: Some(result),
            error: None,
            usage: None,
        }
    }
}

/// v4 `shouldAttemptUncensoredFallback`: whether an empty response should be
/// retried on the configured uncensored provider, and with which selection.
/// Only in `AUTO_ROUTE` mode with an uncensored text profile configured; a
/// dangerous-compatible current profile suppresses it unless the chat itself
/// is dangerous (uncensored→uncensored fallback allowed there).
fn should_attempt_uncensored_fallback(
    response_content: &str,
    current_selection: &CheapLlmSelection,
    uncensored_fallback: Option<&UncensoredFallbackOptions<'_>>,
) -> Option<CheapLlmSelection> {
    if !js_trim(response_content).is_empty() {
        return None;
    }
    let options = uncensored_fallback?;
    let danger = options.danger_settings;

    if danger.mode != "AUTO_ROUTE" {
        return None;
    }
    let uncensored_id = danger
        .uncensored_text_profile_id
        .as_deref()
        .filter(|s| !s.is_empty())?;

    let current_profile = current_selection
        .connection_profile_id
        .as_deref()
        .and_then(|id| options.available_profiles.iter().find(|p| p.id == id));
    if current_profile.map(|p| p.is_dangerous_compatible) == Some(true)
        && options.is_dangerous_chat != Some(true)
    {
        return None;
    }

    let uncensored_profile = options
        .available_profiles
        .iter()
        .find(|p| p.id == uncensored_id)?;

    Some(CheapLlmSelection {
        provider: uncensored_profile.provider.clone(),
        model_name: uncensored_profile.model_name.clone(),
        base_url: uncensored_profile
            .base_url
            .clone()
            .filter(|s| !s.is_empty()),
        connection_profile_id: Some(uncensored_profile.id.clone()),
        // v4 hardcodes `isLocal: false` on this path.
        is_local: false,
        profile_parameters: profile_params(uncensored_profile),
    })
}

struct ProviderResponse {
    content: String,
    usage: Option<CompletionUsage>,
}

/// The cheap-LLM task executor. Holds v4's session-level
/// `profilesWithoutCustomTemp` cache — module-global process state in v4,
/// carried here as instance state (the host keeps one executor per process;
/// the differential keeps one per run, matching a fresh v4 module registry).
#[derive(Default)]
pub struct CheapLlmTaskExecutor {
    profiles_without_custom_temp: Mutex<HashSet<String>>,
}

impl CheapLlmTaskExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    /// v4 `sendToProvider` minus the host-side API-key step: build the params
    /// the cheap path sets (strict max-tokens floor of 2048, temperature 0.3
    /// unless the profile is known not to support one, the per-character cache
    /// key, the profile's provider extras) and call the boundary, retrying
    /// without a temperature when the provider rejects it.
    async fn send_to_provider<C: CompletionProvider>(
        &self,
        completion: &C,
        selection: &CheapLlmSelection,
        messages: &[CompletionMessage],
        max_tokens: Option<f64>,
        character_id: Option<&str>,
    ) -> Result<ProviderResponse, CompletionError> {
        let profile_key = format!("{}:{}", selection.provider, selection.model_name);
        let effective_max_tokens = max_tokens.unwrap_or(2048.0).max(2048.0) as i64;

        let params = |temperature: Option<f64>| CompletionParams {
            messages: messages.to_vec(),
            model: selection.model_name.clone(),
            temperature,
            max_tokens: effective_max_tokens,
            // Cheap tasks use strictMaxTokens so providers don't apply
            // reasoning-model minimums that add verbosity and latency.
            strict_max_tokens: true,
            cache_key: build_character_cache_key(character_id),
            profile_parameters: selection.profile_parameters.clone(),
            attachments: Vec::new(),
        };

        let known_no_temp = self
            .profiles_without_custom_temp
            .lock()
            .expect("temp cache lock")
            .contains(&profile_key);
        if known_no_temp {
            let response = completion
                .send_message(
                    &selection.provider,
                    selection.base_url.as_deref(),
                    &params(None),
                )
                .await?;
            return Ok(ProviderResponse {
                content: response.content,
                usage: response.usage,
            });
        }

        // Try with lower temperature for more consistent outputs.
        match completion
            .send_message(
                &selection.provider,
                selection.base_url.as_deref(),
                &params(Some(0.3)),
            )
            .await
        {
            Ok(response) => Ok(ProviderResponse {
                content: response.content,
                usage: response.usage,
            }),
            Err(error) => {
                // If temperature is unsupported, cache that and retry without.
                if error.message.contains("temperature")
                    || error.message.contains("does not support")
                {
                    self.profiles_without_custom_temp
                        .lock()
                        .expect("temp cache lock")
                        .insert(profile_key);
                    let response = completion
                        .send_message(
                            &selection.provider,
                            selection.base_url.as_deref(),
                            &params(None),
                        )
                        .await?;
                    Ok(ProviderResponse {
                        content: response.content,
                        usage: response.usage,
                    })
                } else {
                    Err(error)
                }
            }
        }
    }

    /// v4 `executeCheapLLMTask`: send, maybe retry on the uncensored provider
    /// for an empty response, parse, and wrap — any error becomes
    /// `{ success: false, error }`.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute<C: CompletionProvider, T>(
        &self,
        completion: &C,
        selection: &CheapLlmSelection,
        messages: Vec<CompletionMessage>,
        parse_response: impl Fn(&str) -> T,
        uncensored_fallback: Option<&UncensoredFallbackOptions<'_>>,
        max_tokens: Option<f64>,
        character_id: Option<&str>,
    ) -> CheapLlmTaskResult<T> {
        let attempt = async {
            let mut response = self
                .send_to_provider(completion, selection, &messages, max_tokens, character_id)
                .await?;

            if let Some(uncensored_selection) = should_attempt_uncensored_fallback(
                &response.content,
                selection,
                uncensored_fallback,
            ) {
                let retry = self
                    .send_to_provider(
                        completion,
                        &uncensored_selection,
                        &messages,
                        max_tokens,
                        character_id,
                    )
                    .await?;
                if js_trim(&retry.content).is_empty() {
                    return Err(CompletionError::new(format!(
                        "Empty response from both safe provider ({}/{}) and uncensored provider ({}/{})",
                        selection.provider,
                        selection.model_name,
                        uncensored_selection.provider,
                        uncensored_selection.model_name,
                    )));
                }
                response = retry;
            }

            Ok(response)
        };

        match attempt.await {
            Ok(response) => CheapLlmTaskResult {
                success: true,
                result: Some(parse_response(&response.content)),
                error: None,
                usage: response.usage,
            },
            // v4 `getErrorMessage(error)` — the thrown Error's message.
            Err(error) => CheapLlmTaskResult {
                success: false,
                result: None,
                error: Some(error.message),
                usage: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cheap_llm::{CheapLlmProfile, DangerousContentSettings};
    use crate::model::completion::CannedCompletionProvider;

    fn selection(provider: &str, model: &str) -> CheapLlmSelection {
        CheapLlmSelection {
            provider: provider.to_string(),
            model_name: model.to_string(),
            base_url: None,
            connection_profile_id: Some("cur".to_string()),
            is_local: false,
            profile_parameters: None,
        }
    }

    #[tokio::test]
    async fn temperature_rejection_caches_and_retries() {
        let messages = vec![CompletionMessage::user("hi")];
        let provider = CannedCompletionProvider::new()
            .with_failure("OPENAI", "m", Some(0.3), &messages, "does not support temp")
            .with_response("OPENAI", "m", None, &messages, "ok", None);
        let exec = CheapLlmTaskExecutor::new();
        let sel = selection("OPENAI", "m");

        let r = exec
            .execute(
                &provider,
                &sel,
                messages.clone(),
                |s| s.to_string(),
                None,
                None,
                None,
            )
            .await;
        assert!(r.success);
        assert_eq!(r.result.as_deref(), Some("ok"));

        // Second call goes straight to the no-temperature form (cached) — the
        // 0.3 entry would fail if consulted again, and it isn't.
        let r2 = exec
            .execute(
                &provider,
                &sel,
                messages,
                |s| s.to_string(),
                None,
                None,
                None,
            )
            .await;
        assert!(r2.success);
    }

    #[tokio::test]
    async fn empty_response_falls_back_to_uncensored_provider() {
        let messages = vec![CompletionMessage::user("spicy")];
        let provider = CannedCompletionProvider::new()
            .with_response("ANTHROPIC", "safe", Some(0.3), &messages, "", None)
            .with_response("OLLAMA", "dolphin", Some(0.3), &messages, "answer", None);
        let exec = CheapLlmTaskExecutor::new();
        let sel = selection("ANTHROPIC", "safe");

        let danger = DangerousContentSettings {
            mode: "AUTO_ROUTE".to_string(),
            uncensored_text_profile_id: Some("u1".to_string()),
        };
        let profiles = vec![
            CheapLlmProfile {
                id: "cur".to_string(),
                provider: "ANTHROPIC".to_string(),
                model_name: "safe".to_string(),
                ..Default::default()
            },
            CheapLlmProfile {
                id: "u1".to_string(),
                provider: "OLLAMA".to_string(),
                model_name: "dolphin".to_string(),
                ..Default::default()
            },
        ];
        let options = UncensoredFallbackOptions {
            danger_settings: &danger,
            available_profiles: &profiles,
            is_dangerous_chat: None,
        };

        let r = exec
            .execute(
                &provider,
                &sel,
                messages,
                |s| s.to_string(),
                Some(&options),
                None,
                None,
            )
            .await;
        assert!(r.success);
        assert_eq!(r.result.as_deref(), Some("answer"));
    }
}
