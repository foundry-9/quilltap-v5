//! v4 `lib/services/external-prompt-generator.service.ts` — Aurora's
//! "Generate External Prompt" (`p4.9k`, P4.9K1): synthesize a character's
//! fields into one self-contained second-person Markdown prompt for pasting
//! into Claude Desktop / ChatGPT Custom Instructions / a similar host, with ONE
//! model call.
//!
//! Ported as the file stands at the `f699da6f6` pin, which CLOSES the two
//! riders the P4.D82/P4.D83 round banked for "the future generators lane":
//! `f933ba9c` (a bare `getModelContextLimit` where a profile is in hand became
//! the profile-first `getSafeInputLimit(provider, modelName, maxTokens,
//! profile)` — the 4-arg form here) and `d89babc4` (the sampling read via
//! `profileParams(profile)` — `profile_params_value` here).
//!
//! Every refusal is a `{success: false, error}` RESULT, never a throw; the route
//! turns it into its 500 (`result.error || 'Generation failed'`). A DB read
//! failure is the one thing that DOES propagate (v4's `findById` throws), so the
//! runner returns `Result<ExternalPromptResult, DbError>`.
//!
//! ## The api key
//!
//! v4 resolves `profile.apiKeyId` → `key_value` and hands the string to
//! `provider.sendMessage(params, apiKey)`. v5's [`CompletionProvider`] takes no
//! key: the host's `WireCompletionProvider` resolves one per provider from the
//! key source (the tree-wide host key scan, recorded at P4.D93). The lookup is
//! still performed here — its ABSENCE is not observable, its presence keeps the
//! read order faithful — and the resolved string is dropped.

use serde::Serialize;
use serde_json::Value;

use crate::api::system_qtap::js_truthy;
use crate::cheap_llm::{build_character_cache_key, profile_params_value};
use crate::db::runtime::Db;
use crate::db::{api_keys, characters_read, connection_profiles, DbError};
use crate::jsstr::utf16_len;
use crate::model::completion::{
    CompletionMessage, CompletionParams, CompletionProvider, CompletionResponse,
};
use crate::pascal::js_value::to_js_string;
use crate::provider_manifest::Registry;
use crate::services::llm_logging::{
    log_llm_call, LogContext, LogLlmCallParams, LogRequest, LogRequestMessage, LogResponse,
    LogUsage,
};

/// The `llm_logs.type` this service writes (v4 `LLMLogType` `EXTERNAL_PROMPT`).
pub const LOG_TYPE_EXTERNAL_PROMPT: &str = "EXTERNAL_PROMPT";

// ============================================================================
// Types (v4 `external-prompt-generator.service.ts:25-37`)
// ============================================================================

/// v4 `ExternalPromptRequest` — the Zod-validated body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalPromptRequest {
    pub connection_profile_id: String,
    pub system_prompt_id: String,
    pub scenario_id: Option<String>,
    pub max_tokens: i64,
}

/// v4 `ExternalPromptResult` — `{success, prompt, tokensUsed, error?}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalPromptResult {
    pub success: bool,
    pub prompt: String,
    pub tokens_used: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ExternalPromptResult {
    fn failure(error: impl Into<String>) -> Self {
        ExternalPromptResult {
            success: false,
            prompt: String::new(),
            tokens_used: 0,
            error: Some(error.into()),
        }
    }
}

// ============================================================================
// Meta-prompt (wire bytes)
// ============================================================================

/// v4 `META_SYSTEM_PROMPT` (byte-exact) — the system message of the one call.
pub const META_SYSTEM_PROMPT: &str = r#"You are a prompt engineering expert. Your task is to generate a standalone system prompt for an AI character, suitable for pasting into external tools like Claude Desktop, ChatGPT Custom Instructions, or similar hosted environments.

**Requirements:**
- Write the entire prompt in second person ("You are [Name]. You always...", etc.)
- Output in Markdown format with clear sections
- The prompt must be completely self-contained — it should include all personality, behavioral, speech pattern, and contextual information needed for an AI to portray this character without any additional context
- Capture the character's voice, mannerisms, and personality authentically
- Include guidance on how the character responds to different types of interactions
- If a scenario is provided, weave the setting naturally into the prompt
- If physical appearance or clothing details are provided, include them so the character can reference their own appearance
- Do NOT include meta-instructions about being an AI or breaking character
- Do NOT reference Quilltap or any external system
- The prompt should read as a coherent, well-structured character brief

Stay within the token budget specified by the user. Be thorough but concise."#;

// ============================================================================
// Helpers
// ============================================================================

/// JS `${x}` for a field that may be absent — absent renders `undefined`, an
/// explicit `null` renders `null`.
fn interp(v: Option<&Value>) -> String {
    match v {
        None => "undefined".to_string(),
        Some(x) => to_js_string(x),
    }
}

/// v4 `buildUserMessage` (byte-exact). Every `if (character.x)` is JS
/// truthiness over the overlaid character's raw value; `clothingContent` is
/// v4's dead parameter (always `undefined` at the one call site) and is kept
/// so the section order is transcribed whole.
pub fn build_user_message(
    character: &Value,
    system_prompt: &Value,
    scenario_content: Option<&str>,
    description_content: Option<&str>,
    clothing_content: Option<&str>,
    max_tokens: Option<i64>,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    // v4 `${maxTokens || 4000}` — falsy (0 / undefined) takes the default.
    let target = match max_tokens {
        Some(n) if n != 0 => n.to_string(),
        _ => "4000".to_string(),
    };
    parts.push(format!(
        "Generate a standalone system prompt for the following character. Target approximately {target} tokens for the output."
    ));
    parts.push(String::new());

    parts.push(format!("# Character: {}", interp(character.get("name"))));
    if js_truthy(character.get("title")) {
        parts.push(format!("**Title:** {}", interp(character.get("title"))));
    }
    // v4 `character.aliases?.length > 0`.
    if let Some(aliases) = character.get("aliases").and_then(Value::as_array) {
        if !aliases.is_empty() {
            parts.push(format!(
                "**Also known as:** {}",
                aliases
                    .iter()
                    .map(|a| match a {
                        Value::Null => String::new(),
                        other => to_js_string(other),
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    if js_truthy(character.get("pronouns")) {
        let p = character.get("pronouns").unwrap();
        parts.push(format!(
            "**Pronouns:** {}/{}/{}",
            interp(p.get("subject")),
            interp(p.get("object")),
            interp(p.get("possessive"))
        ));
    }
    parts.push(String::new());

    for (key, heading) in [
        ("description", "## Description"),
        ("manifesto", "## Manifesto"),
        ("personality", "## Personality"),
    ] {
        if js_truthy(character.get(key)) {
            parts.push(heading.to_string());
            parts.push(interp(character.get(key)));
            parts.push(String::new());
        }
    }

    parts.push("## System Prompt".to_string());
    parts.push(format!(
        "**Prompt name:** {}",
        interp(system_prompt.get("name"))
    ));
    parts.push(interp(system_prompt.get("content")));
    parts.push(String::new());

    if let Some(s) = scenario_content.filter(|s| !s.is_empty()) {
        parts.push("## Scenario / Setting".to_string());
        parts.push(s.to_string());
        parts.push(String::new());
    }

    if let Some(d) = description_content.filter(|d| !d.is_empty()) {
        parts.push("## Physical Appearance".to_string());
        parts.push(d.to_string());
        parts.push(String::new());
    }

    if let Some(c) = clothing_content.filter(|c| !c.is_empty()) {
        parts.push("## Clothing / Attire".to_string());
        parts.push(c.to_string());
        parts.push(String::new());
    }

    if js_truthy(character.get("firstMessage")) {
        parts.push("## Typical First Message".to_string());
        parts.push("(This shows how the character typically opens a conversation:)".to_string());
        parts.push(interp(character.get("firstMessage")));
        parts.push(String::new());
    }

    if js_truthy(character.get("exampleDialogues")) {
        parts.push("## Example Dialogues".to_string());
        parts.push("(These show the character's typical voice and interaction style:)".to_string());
        parts.push(interp(character.get("exampleDialogues")));
        parts.push(String::new());
    }

    parts.join("\n")
}

/// v4 `desc.fullDescription || desc.completePrompt || desc.longPrompt ||
/// desc.mediumPrompt || desc.shortPrompt || undefined` — the first JS-truthy
/// value, stringified.
fn most_detailed_description(desc: &Value) -> Option<String> {
    for key in [
        "fullDescription",
        "completePrompt",
        "longPrompt",
        "mediumPrompt",
        "shortPrompt",
    ] {
        if js_truthy(desc.get(key)) {
            return Some(to_js_string(desc.get(key).unwrap()));
        }
    }
    None
}

fn find_by_id<'a>(items: Option<&'a Value>, id: &str) -> Option<&'a Value> {
    items
        .and_then(Value::as_array)?
        .iter()
        .find(|it| it.get("id").and_then(Value::as_str) == Some(id))
}

// ============================================================================
// The service (v4 `generateExternalPrompt`)
// ============================================================================

/// v4 `generateExternalPrompt(characterId, request, userId, repos)`. `Err` is
/// a DB failure escaping the reads (v4's throw); every modelled refusal is an
/// `Ok` result with `success: false`.
pub async fn generate_external_prompt<CMP: CompletionProvider>(
    db: &Db,
    completion: &CMP,
    character_id: &str,
    request: &ExternalPromptRequest,
    user_id: &str,
) -> Result<ExternalPromptResult, DbError> {
    tracing::info!(
        target: "quilltap::external_prompt_generator",
        character_id = %character_id,
        connection_profile_id = %request.connection_profile_id,
        system_prompt_id = %request.system_prompt_id,
        scenario_id = %request.scenario_id.as_deref().unwrap_or("(none)"),
        max_tokens = request.max_tokens,
        "Starting external prompt generation"
    );

    // Resolve connection profile and API key.
    let pid = request.connection_profile_id.clone();
    let Some(profile) = db.read_main(move |c| connection_profiles::find_by_id(c, &pid))? else {
        tracing::warn!(
            target: "quilltap::external_prompt_generator",
            connection_profile_id = %request.connection_profile_id,
            "Connection profile not found"
        );
        return Ok(ExternalPromptResult::failure(
            "Connection profile not found",
        ));
    };

    // v4 `if (profile.apiKeyId)` → `findApiKeyByIdAndUserId` → `key_value`,
    // else `''`. Resolved for read-order fidelity; the provider seam resolves
    // its own (module doc).
    let mut _api_key = String::new();
    if let Some(key_id) = profile
        .get("apiKeyId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        let key_id = key_id.to_string();
        let uid = user_id.to_string();
        if let Some(key) =
            db.read_main(move |c| api_keys::find_by_id_and_user_id(c, &key_id, &uid))?
        {
            _api_key = key.key_value;
        }
    }

    // Fetch character data (the overlaid read; a broken vault THROWS in v4 and
    // propagates as `Err` here).
    let cid = character_id.to_string();
    let Some(character) = db.read_main(|main| {
        db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &cid))
    })?
    else {
        tracing::warn!(
            target: "quilltap::external_prompt_generator",
            character_id = %character_id,
            "Character not found"
        );
        return Ok(ExternalPromptResult::failure("Character not found"));
    };

    // Find selected system prompt.
    let Some(system_prompt) = find_by_id(character.get("systemPrompts"), &request.system_prompt_id)
    else {
        tracing::warn!(
            target: "quilltap::external_prompt_generator",
            system_prompt_id = %request.system_prompt_id,
            "System prompt not found"
        );
        return Ok(ExternalPromptResult::failure("System prompt not found"));
    };

    // Resolve optional data. v4 `if (request.scenarioId)` (truthy) → find →
    // `scenarioContent = scenario.content` (then `if (scenarioContent)` at the
    // section — a null content is omitted).
    let scenario_content: Option<String> = request
        .scenario_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|sid| find_by_id(character.get("scenarios"), sid))
        .and_then(|sc| sc.get("content"))
        .filter(|c| js_truthy(Some(c)))
        .map(to_js_string);

    // Use the most detailed available description.
    let description_content: Option<String> = character
        .get("physicalDescription")
        .filter(|d| js_truthy(Some(d)))
        .and_then(most_detailed_description);

    // Build the user message with all character data.
    let user_message = build_user_message(
        &character,
        system_prompt,
        scenario_content.as_deref(),
        description_content.as_deref(),
        None,
        Some(request.max_tokens),
    );

    // Estimate input tokens (rough: 1 token ≈ 4 characters) — `.length` is
    // UTF-16 code units.
    let estimated_input_tokens =
        ((utf16_len(META_SYSTEM_PROMPT) + utf16_len(&user_message)) as f64 / 4.0).ceil() as i64;
    let provider = profile
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let model_name = profile
        .get("modelName")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    // `f933ba9c`: the profile-first window. The built-in registry carries no
    // per-model context rows, so `model_info` / `fallback_pricing` are empty
    // here (the `chat_create` recent-conversations precedent).
    let safe_input_limit = crate::model_context::get_safe_input_limit(
        &provider,
        &model_name,
        &[],
        &[],
        Registry::built_in().default_context_window(&provider),
        request.max_tokens,
        profile
            .get("maxContext")
            .and_then(Value::as_f64)
            .map(|f| f as i64),
    );

    if estimated_input_tokens > safe_input_limit {
        let error_msg = format!(
            "Character data is too large for the selected model's context window at the requested output size. Estimated input: ~{estimated_input_tokens} tokens, safe limit: ~{safe_input_limit} tokens. Try reducing the token limit or selecting a model with a larger context window."
        );
        tracing::warn!(
            target: "quilltap::external_prompt_generator",
            character_id = %character_id,
            estimated_input_tokens,
            safe_input_limit,
            max_tokens = request.max_tokens,
            "Input exceeds safe context limit"
        );
        return Ok(ExternalPromptResult::failure(error_msg));
    }

    // Create LLM provider and generate. v4 `profile.baseUrl ?? undefined` —
    // NULLISH, so an empty-string baseUrl is passed as-is.
    let base_url = profile
        .get("baseUrl")
        .and_then(Value::as_str)
        .map(str::to_string);
    let messages = vec![
        CompletionMessage::system(META_SYSTEM_PROMPT),
        CompletionMessage::user(user_message.clone()),
    ];
    let params = CompletionParams {
        messages: messages.clone(),
        model: model_name.clone(),
        temperature: Some(0.7),
        max_tokens: Some(request.max_tokens),
        strict_max_tokens: false,
        top_p: None,
        cache_key: build_character_cache_key(Some(character_id)),
        profile_parameters: profile_params_value(&profile),
        attachments: Vec::new(),
        request_timeout_ms: None,
    };

    let start_ms = crate::clock::now_unix_ms();
    let response: CompletionResponse = match completion
        .send_message(&provider, base_url.as_deref(), &params)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            // v4 `error instanceof Error ? error.message : 'Generation failed'`
            // — a `CompletionError` is always an Error with a message.
            let error_message = e.message;
            tracing::error!(
                target: "quilltap::external_prompt_generator",
                character_id = %character_id,
                error = %error_message,
                "External prompt generation failed"
            );
            return Ok(ExternalPromptResult::failure(error_message));
        }
    };
    let duration_ms = crate::clock::now_unix_ms() - start_ms;

    // v4 `if (!response?.content)` — JS falsy: an empty answer is a refusal.
    if response.content.is_empty() {
        tracing::error!(
            target: "quilltap::external_prompt_generator",
            "No response content from LLM"
        );
        return Ok(ExternalPromptResult::failure("No response from model"));
    }

    // v4 `response.usage?.totalTokens || 0`.
    let tokens_used = response
        .usage
        .map(|u| u.total_tokens)
        .filter(|t| *t != 0)
        .unwrap_or(0);

    tracing::info!(
        target: "quilltap::external_prompt_generator",
        character_id = %character_id,
        duration_ms,
        tokens_used,
        output_length = utf16_len(&response.content),
        "External prompt generated successfully"
    );

    // Log the LLM call (fire and forget — never throws).
    let _ = log_llm_call(
        db,
        LogLlmCallParams {
            user_id: user_id.to_string(),
            log_type: LOG_TYPE_EXTERNAL_PROMPT.to_string(),
            message_id: None,
            chat_id: None,
            character_id: Some(character_id.to_string()),
            provider: provider.clone(),
            model_name: model_name.clone(),
            connection_profile_id: None,
            image_profile_id: None,
            request: LogRequest {
                messages: vec![
                    LogRequestMessage {
                        role: "system".to_string(),
                        content: META_SYSTEM_PROMPT.to_string(),
                        attachments: None,
                    },
                    LogRequestMessage {
                        role: "user".to_string(),
                        content: user_message,
                        attachments: None,
                    },
                ],
                temperature: Some(0.7),
                max_tokens: Some(request.max_tokens),
                tools: None,
            },
            response: LogResponse {
                content: response.content.clone(),
                error: None,
                finish_reason: None,
                tool_calls: None,
            },
            usage: response.usage.map(|u| LogUsage {
                prompt_tokens: Some(u.prompt_tokens),
                completion_tokens: Some(u.completion_tokens),
                total_tokens: Some(u.total_tokens),
            }),
            cache_usage: None,
            raw_provider_usage: None,
            request_hashes: None,
            duration_ms: Some(duration_ms as f64),
        },
        &LogContext::none(),
    )
    .await;

    Ok(ExternalPromptResult {
        success: true,
        prompt: response.content,
        tokens_used,
        error: None,
    })
}
