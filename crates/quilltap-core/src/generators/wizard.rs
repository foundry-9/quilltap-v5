//! v4 `lib/services/character-wizard.service.ts` — the AI Wizard (`p4.9k`,
//! P4.9K2). The first half of this file is the PURE half:
//! [`build_context_prompt`], the context every wizard field-generation call
//! opens with (the prompt CONSTANTS live in the generated
//! [`super::wizard_prompts`]). The second half (P4.9K2 unit 3) is the five
//! generators and the two runners.
//!
//! ## Input shape
//!
//! `existing_data` is read as `serde_json::Value` — v4's `WizardRequest
//! ['existingData']` is an optional bag whose every member is itself optional,
//! and the builder tests each with `?.trim()` (JS optional chaining then a
//! whitespace test) or a plain truthiness check. Reading it as a `Value`
//! reproduces "absent", "null" and "present but blank" as the three DIFFERENT
//! inputs v4 treats them as; a typed struct would collapse the first two.
//!
//! v4's banked riders `40d507cc` (the generators taxonomy) and `4423ad10` (the
//! hair-slot edits) are IN this file at the baseline — it is ported as it
//! stands, which closes both.

use base64::Engine as _;
use serde_json::{json, Map, Value};

use crate::api::system_qtap::js_truthy;
use crate::cheap_llm::profile_params_value;
use crate::db::files::FileFull;
use crate::db::files::FilesRepository;
use crate::db::runtime::Db;
use crate::db::{api_keys, connection_profiles, DbError};
use crate::generators::file_content::{extract_file_content, file_entry_of};
use crate::generators::generated_items::{
    sanitize_generated_wardrobe_items, wardrobe_items_generation_prompt, GeneratedWardrobeItem,
};
use crate::generators::generated_properties::{
    describe_generated_properties, parse_generated_properties, GeneratedProperties,
};
use crate::generators::llm_json::parse_llm_json;
use crate::generators::optimizer::llm_json_failure_message;
use crate::generators::wizard_prompts::{
    field_prompt, HEAD_AND_SHOULDERS_PHYSICAL_PROMPT, PROPERTIES_PROMPT,
};
use crate::jsnum::to_fixed;
use crate::jsstr::js_trim;
use crate::model::completion::{
    CompletionAttachment, CompletionMessage, CompletionParams, CompletionProvider,
};
use crate::pascal::js_value::to_js_string;
use crate::services::activity_kinds::ActivityKind;
use crate::services::activity_registry::track_activity;
use crate::services::file_fallback::profile_supports_mime_type;
use crate::services::file_storage::{download_file, StorageBackend};
use crate::services::llm_logging::{
    log_llm_call, LogContext, LogLlmCallParams, LogRequest, LogRequestMessage, LogResponse,
    LogUsage,
};

/// JS `x?.trim()` followed by a truthiness test — "present, a string, and not
/// all whitespace". A non-string present value takes v4's `.trim` on a
/// non-string, which THROWS; no route shape can produce one (the Zod schema
/// types every member as a string), so this treats it as absent and the
/// divergence is recorded rather than simulated.
fn present_non_blank(v: Option<&Value>) -> Option<&str> {
    match v {
        Some(Value::String(s)) if !js_trim(s).is_empty() => Some(s.as_str()),
        _ => None,
    }
}

/// v4 `buildContextPrompt` (byte-exact).
///
/// Note the shape v4 builds: the opening paragraph ends with a newline, and
/// every appended block OPENS with one — so the joins are `\n` + block, never
/// a separator between blocks. Transcribed literally rather than re-derived,
/// because "which side owns the newline" is exactly what a re-derivation gets
/// wrong.
pub fn build_context_prompt(
    character_name: &str,
    background: &str,
    existing_data: Option<&Value>,
    image_description: Option<&str>,
    document_content: Option<&str>,
) -> String {
    let mut context = String::from(
        "You are a character creation assistant for a roleplay/chat application. You are helping create a character profile that will be used by an AI to roleplay as this character.\n",
    );

    if !js_trim(character_name).is_empty() {
        context.push_str(&format!("\nCharacter Name: {character_name}\n"));
    } else {
        context.push_str(
            "\nNote: The character does not yet have a name. You may be asked to generate one.\n",
        );
    }

    if !js_trim(background).is_empty() {
        context.push_str(&format!("\nBackground/World Context:\n{background}\n"));
    }

    // v4 `if (imageDescription)` / `if (documentContent)` — JS truthiness, so an
    // EMPTY string appends nothing.
    if let Some(desc) = image_description.filter(|d| !d.is_empty()) {
        context.push_str(&format!(
            "\nVisual Reference (from image analysis):\n{desc}\n\nNote: this visual reference describes the character's PHYSICAL APPEARANCE only. Use it for the physicalDescription field. Do NOT let it bleed into the identity, description, or personality fields — those fields are about facts, behaviour, and self-knowledge respectively, never appearance.\n"
        ));
    }

    if let Some(doc) = document_content.filter(|d| !d.is_empty()) {
        context.push_str(&format!("\nCharacter Reference Document:\n{doc}\n"));
    }

    // v4 `if (existingData)` — JS truthiness on the bag itself.
    let Some(existing) = existing_data.filter(|e| !e.is_null()) else {
        return context;
    };

    let mut fields: Vec<String> = Vec::new();
    if let Some(v) = present_non_blank(existing.get("title")) {
        fields.push(format!("Title: {v}"));
    }
    // v4 `if (existingData.pronouns)` — truthiness, NOT a blank test.
    if let Some(p) = existing.get("pronouns").filter(|p| js_truthy(Some(p))) {
        fields.push(format!(
            "Pronouns: {}/{}/{}",
            p.get("subject")
                .map(to_js_string)
                .unwrap_or_else(|| "undefined".into()),
            p.get("object")
                .map(to_js_string)
                .unwrap_or_else(|| "undefined".into()),
            p.get("possessive")
                .map(to_js_string)
                .unwrap_or_else(|| "undefined".into()),
        ));
    }
    if let Some(aliases) = existing.get("aliases").and_then(Value::as_array) {
        if !aliases.is_empty() {
            fields.push(format!(
                "Aliases: {}",
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
    for (key, label) in [
        ("identity", "Identity"),
        ("description", "Description"),
        ("manifesto", "Manifesto"),
        ("personality", "Personality"),
    ] {
        if let Some(v) = present_non_blank(existing.get(key)) {
            fields.push(format!("{label}: {v}"));
        }
    }
    if let Some(scenarios) = existing.get("scenarios").and_then(Value::as_array) {
        if !scenarios.is_empty() {
            let lines = scenarios
                .iter()
                .map(|s| {
                    format!(
                        "  - {}: {}",
                        s.get("title")
                            .map(to_js_string)
                            .unwrap_or_else(|| "undefined".into()),
                        s.get("content")
                            .map(to_js_string)
                            .unwrap_or_else(|| "undefined".into())
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            fields.push(format!("Scenarios:\n{lines}"));
        }
    }
    // These three carry their value on the NEXT line, not after a space.
    for (key, label) in [
        ("firstMessage", "First Message"),
        ("exampleDialogues", "Example Dialogues"),
        ("systemPrompt", "Default System Prompt"),
    ] {
        if let Some(v) = present_non_blank(existing.get(key)) {
            fields.push(format!("{label}:\n{v}"));
        }
    }

    if !fields.is_empty() {
        context.push_str(&format!(
            "\nExisting Character Information:\n{}\n",
            fields.join("\n")
        ));
    }

    context
}

// ============================================================================
// The runners (v4 `runCharacterWizard` / `runCharacterWizardStreaming` —
// P4.9K2 unit 3)
// ============================================================================
//
// v4 carries the two runners as ~250 duplicated lines. They are factored ONCE
// here ([`run_wizard_core`]) and the places the copies differ are parameters
// with their line pairs recorded:
//
// * the log messages — `[CharacterWizard] Starting` / `Complete` (`:700`,
//   `:940`) vs `Starting (streaming)` / `Complete (streaming)` (`:960`,
//   `:1196`);
// * the events — the streaming twin emits `start`, `field_start`,
//   `field_complete {snippet}`, `field_error {error}`, `done` (`:967`,
//   `:1063-1183`); the non-streaming twin emits nothing and returns
//   `WizardResult {success: true, generated, errors?}` (`:944-948`);
// * the `properties` snippet — `describeGeneratedProperties(props, 'no
//   pronouns found')` (`:1141`; the non-streaming twin computes no snippet);
// * a throw before the field loop — the non-streaming twin THROWS to its
//   route (v4's middleware catch answers `500 Internal server error`); the
//   streaming twin catches it into `done {error, fullContent: {}, errors:
//   {_fatal}}` (`:1203-1211`).
//
// Seams: the completion provider (v4 `createLLMProvider(...).sendMessage` —
// the vision call rides the SAME seam with a base64 attachment), the storage
// backend (`fileStorageManager.downloadFile` — mount-blob keys never touch
// it), the `Db`. The model calls' `llm_logs` rows (`CHARACTER_WIZARD`) are
// written best-effort as v4's are.

/// The `llm_logs.type` every wizard call writes (v4 `type: 'CHARACTER_WIZARD'`).
pub const LOG_TYPE_CHARACTER_WIZARD: &str = "CHARACTER_WIZARD";

/// The tracing target every `[CharacterWizard]` line is emitted under.
pub const WIZARD_LOG_TARGET: &str = "quilltap::character_wizard";

/// v4 `MAX_VISION_IMAGE_SIZE`.
pub const MAX_VISION_IMAGE_SIZE: usize = 5 * 1024 * 1024;

/// v4's vision instruction (byte-exact) — the one user message of the image
/// description call.
pub const IMAGE_DESCRIPTION_INSTRUCTION: &str = "Please describe this image in great detail. Focus on the physical appearance of any person or character shown. Include: face shape, eye color/shape, hair color/style/length, skin tone, body type/build, clothing, pose, and any distinctive features. Be thorough and specific.";

/// v4 `PHYSICAL_DESCRIPTION_PROMPTS` — the six tiers in v4's insertion order
/// (`headAndShoulders` is the shared exported constant; the other five are
/// private to the service). Wire bytes: each reaches a paid model verbatim.
pub const PHYSICAL_DESCRIPTION_PROMPTS: &[(&str, &str)] = &[
    ("headAndShoulders", HEAD_AND_SHOULDERS_PHYSICAL_PROMPT),
    (
        "short",
        r#"Create an extremely concise visual description for image generation, maximum 350 characters.
Focus ONLY on: hair, eyes, skin, body type, and one distinctive feature.
Format: [trait], [trait], [trait]...
No sentences, just comma-separated descriptors.
OUTPUT ONLY THE DESCRIPTION, NO EXPLANATION."#,
    ),
    (
        "medium",
        r#"Create a concise visual description for image generation, maximum 500 characters.
Include: hair color/style, eye color, skin tone, body type, facial features.
Do NOT include clothing, outfits, or accessories — those are handled separately by the wardrobe system. Include the character's natural hair (colour, length, texture) here, but not a styled hairdo — hairstyles are wardrobe items.
Write as a continuous description, no line breaks.
OUTPUT ONLY THE DESCRIPTION, NO EXPLANATION."#,
    ),
    (
        "long",
        r#"Create a detailed visual description for image generation, maximum 750 characters.
Include: complete hair description, eye details, skin, facial structure, body type, posture, any distinctive marks or features.
Do NOT include clothing, outfits, or accessories — those are handled separately by the wardrobe system. Include the character's natural hair (colour, length, texture) here, but not a styled hairdo — hairstyles are wardrobe items.
Write as flowing description suitable for stable diffusion or DALL-E.
OUTPUT ONLY THE DESCRIPTION, NO EXPLANATION."#,
    ),
    (
        "complete",
        r#"Create a comprehensive visual description for image generation, maximum 1000 characters.
Include all physical details: hair (color, length, style, texture), eyes (color, shape, expression), face (shape, features, expression), body (type, height, build), skin (tone, texture, any marks), posture and body language.
Do NOT include clothing, outfits, or accessories — those are handled separately by the wardrobe system. Include the character's natural hair (colour, length, texture) here, but not a styled hairdo — hairstyles are wardrobe items.
Optimized for AI image generation.
OUTPUT ONLY THE DESCRIPTION, NO EXPLANATION."#,
    ),
    (
        "full",
        r#"Write a complete, detailed physical description of this character in markdown format.
Do NOT include clothing, outfits, or accessories — those are handled separately by the wardrobe system. Include the character's natural hair (colour, length, texture) here, but not a styled hairdo — hairstyles are wardrobe items.
Structure with headers:
## Overview
Brief 1-2 sentence summary

## Face & Head
Hair, eyes, face shape, expressions, any facial features

## Body
Build, height, posture, distinguishing physical traits

## Distinctive Features
Unique marks, mannerisms, or visual traits

Be thorough and specific. This will be used as reference for consistent character portrayal."#,
    ),
];

/// v4 `WizardRequest`, post-Zod (the route's `wizardRequestSchema` types every
/// member, so the runner sees concrete values; `existing_data` stays a
/// `Value` for [`build_context_prompt`]'s three-way input).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WizardRequest {
    pub primary_profile_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vision_profile_id: Option<String>,
    pub source_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    pub character_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_data: Option<Value>,
    pub background: String,
    pub fields_to_generate: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character_id: Option<String>,
}

/// v4's `onProgress` callback shape.
pub type OnProgress<'a> = &'a mut (dyn FnMut(Value) + Send);

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}
fn utf16_prefix(s: &str, n: usize) -> String {
    String::from_utf16_lossy(&s.encode_utf16().take(n).collect::<Vec<u16>>())
}
fn str_of(v: &Value, key: &str) -> String {
    v.get(key).map(to_js_string).unwrap_or_default()
}

/// The `[CharacterWizard]` log context as ONE JSON field (the optimizer's
/// idiom — the differential compares the whole bag).
fn ctx(v: Value) -> String {
    serde_json::to_string(&v).unwrap_or_default()
}

fn starting_ctx(user_id: &str, request: &WizardRequest) -> String {
    ctx(json!({
        "userId": user_id,
        "characterName": request.character_name,
        "fieldsToGenerate": request.fields_to_generate,
        "sourceType": request.source_type,
    }))
}

/// v4 `getSnippet(content, maxLength = 100)` (the wizard's version): a string
/// truncated with `...`; an array's first entry as `title: content`; an object
/// with a `shortPrompt` (the physical description) truncated the same way;
/// else `''`.
pub fn get_snippet(content: &Value, max_length: usize) -> String {
    let truncate = |s: &str| {
        if utf16_len(s) > max_length {
            format!("{}...", utf16_prefix(s, max_length))
        } else {
            s.to_string()
        }
    };
    match content {
        Value::String(s) => truncate(s),
        Value::Array(items) if !items.is_empty() => {
            let first = &items[0];
            let preview = if js_truthy(first.get("title")) {
                // `${first.title}: ${first.content ?? ''}`
                let content = match first.get("content") {
                    Some(Value::Null) | None => String::new(),
                    Some(c) => to_js_string(c),
                };
                format!("{}: {content}", str_of(first, "title"))
            } else {
                match first.get("content") {
                    Some(Value::Null) | None => String::new(),
                    Some(c) => to_js_string(c),
                }
            };
            truncate(&preview)
        }
        Value::Object(o) => match o.get("shortPrompt") {
            Some(sp) if js_truthy(Some(sp)) => {
                let sp = to_js_string(sp);
                let mut out = utf16_prefix(&sp, max_length);
                if utf16_len(&sp) > max_length {
                    out.push_str("...");
                }
                out
            }
            _ => String::new(),
        },
        _ => String::new(),
    }
}

/// The per-run call context (v4 threads these ten arguments into every
/// generator).
struct WizardCallCtx<'a, CMP: CompletionProvider> {
    db: &'a Db,
    completion: &'a CMP,
    provider: String,
    base_url: Option<String>,
    model_name: String,
    profile_parameters: Option<Value>,
    user_id: &'a str,
    character_id: Option<&'a str>,
}

/// v4 `generateField` — one completion (`[system contextPrompt, user
/// fieldPrompt]`, `temperature: 0.8`, `maxTokens`), the `No response from
/// model` refusal on an empty answer, the best-effort `CHARACTER_WIZARD` log
/// row, the TRIMMED content.
async fn generate_field<CMP: CompletionProvider>(
    c: &WizardCallCtx<'_, CMP>,
    context_prompt: &str,
    field_prompt: &str,
    max_tokens: i64,
) -> Result<String, String> {
    let messages = vec![
        CompletionMessage::system(context_prompt),
        CompletionMessage::user(field_prompt),
    ];
    let params = CompletionParams {
        messages,
        model: c.model_name.clone(),
        temperature: Some(0.8),
        max_tokens: Some(max_tokens),
        strict_max_tokens: false,
        top_p: None,
        cache_key: None,
        profile_parameters: c.profile_parameters.clone(),
        attachments: Vec::new(),
        request_timeout_ms: None,
    };
    let start_ms = crate::clock::now_unix_ms();
    let response = c
        .completion
        .send_message(&c.provider, c.base_url.as_deref(), &params)
        .await
        .map_err(|e| e.message)?;
    let duration_ms = crate::clock::now_unix_ms() - start_ms;
    if response.content.is_empty() {
        return Err("No response from model".to_string());
    }
    let _ = log_llm_call(
        c.db,
        LogLlmCallParams {
            user_id: c.user_id.to_string(),
            log_type: LOG_TYPE_CHARACTER_WIZARD.to_string(),
            message_id: None,
            chat_id: None,
            character_id: c.character_id.map(str::to_string),
            provider: c.provider.clone(),
            model_name: c.model_name.clone(),
            connection_profile_id: None,
            image_profile_id: None,
            request: LogRequest {
                messages: vec![
                    LogRequestMessage {
                        role: "system".to_string(),
                        content: context_prompt.to_string(),
                        attachments: None,
                    },
                    LogRequestMessage {
                        role: "user".to_string(),
                        content: field_prompt.to_string(),
                        attachments: None,
                    },
                ],
                temperature: Some(0.8),
                max_tokens: Some(max_tokens),
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
    Ok(js_trim(&response.content).to_string())
}

/// v4 `generateImageDescription` — "Reading an image counts as image work:
/// 'Img' stays lit for the whole vision call": the download, the 5 MB ceiling
/// with v4's sentence, the base64 attachment on the one user message,
/// `maxTokens: 1000` / `temperature: 0.7` / the vision profile's parameters,
/// the `No response from vision model` refusal, the log row, the TRIMMED
/// content.
async fn generate_image_description<CMP: CompletionProvider>(
    db: &Db,
    completion: &CMP,
    backend: &dyn StorageBackend,
    image_file: &FileFull,
    vision_profile: &Value,
    user_id: &str,
    character_id: Option<&str>,
) -> Result<String, String> {
    track_activity(
        ActivityKind::Image,
        run_generate_image_description(
            db,
            completion,
            backend,
            image_file,
            vision_profile,
            user_id,
            character_id,
        ),
    )
    .await
}

async fn run_generate_image_description<CMP: CompletionProvider>(
    db: &Db,
    completion: &CMP,
    backend: &dyn StorageBackend,
    image_file: &FileFull,
    vision_profile: &Value,
    user_id: &str,
    character_id: Option<&str>,
) -> Result<String, String> {
    if image_file.storage_key.as_deref().is_none_or(str::is_empty) {
        return Err("Image file has no storage key".to_string());
    }
    let image_buffer = download_file(db, backend, &file_entry_of(image_file))?;
    if image_buffer.len() > MAX_VISION_IMAGE_SIZE {
        let size_mb = to_fixed(image_buffer.len() as f64 / (1024.0 * 1024.0), 1);
        return Err(format!(
            "Image is too large ({size_mb}MB). Vision models have a 5MB limit. Please use a smaller image or resize it before uploading."
        ));
    }
    let base64_data = base64::engine::general_purpose::STANDARD.encode(&image_buffer);
    let attachment = CompletionAttachment {
        id: image_file.id.clone(),
        filename: image_file.original_filename.clone(),
        mime_type: image_file.mime_type.clone(),
        data: base64_data,
    };
    // `visionProfile.baseUrl || undefined` — TRUTHY.
    let provider = str_of(vision_profile, "provider");
    let model_name = str_of(vision_profile, "modelName");
    let base_url = vision_profile
        .get("baseUrl")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let params = CompletionParams {
        messages: vec![CompletionMessage::user(IMAGE_DESCRIPTION_INSTRUCTION)],
        model: model_name.clone(),
        temperature: Some(0.7),
        max_tokens: Some(1000),
        strict_max_tokens: false,
        top_p: None,
        cache_key: None,
        profile_parameters: profile_params_value(vision_profile),
        attachments: vec![attachment],
        request_timeout_ms: None,
    };
    let start_ms = crate::clock::now_unix_ms();
    let response = completion
        .send_message(&provider, base_url.as_deref(), &params)
        .await
        .map_err(|e| e.message)?;
    let duration_ms = crate::clock::now_unix_ms() - start_ms;
    if response.content.is_empty() {
        return Err("No response from vision model".to_string());
    }
    let _ = log_llm_call(
        db,
        LogLlmCallParams {
            user_id: user_id.to_string(),
            log_type: LOG_TYPE_CHARACTER_WIZARD.to_string(),
            message_id: None,
            chat_id: None,
            character_id: character_id.map(str::to_string),
            provider,
            model_name,
            connection_profile_id: None,
            image_profile_id: None,
            request: LogRequest {
                messages: vec![LogRequestMessage {
                    role: "user".to_string(),
                    content: IMAGE_DESCRIPTION_INSTRUCTION.to_string(),
                    attachments: Some(vec![json!({ "id": image_file.id })]),
                }],
                temperature: Some(0.7),
                max_tokens: Some(1000),
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
    Ok(js_trim(&response.content).to_string())
}

/// v4 `generatePhysicalDescriptions` — the six tiers in order, each its own
/// call (`full` 1500 tokens, `complete` 400, the rest 300), the substring caps,
/// the `name: 'AI Generated'` first key.
async fn generate_physical_descriptions<CMP: CompletionProvider>(
    c: &WizardCallCtx<'_, CMP>,
    context_prompt: &str,
) -> Result<Value, String> {
    let mut results = Map::new();
    results.insert("name".into(), Value::String("AI Generated".into()));
    for (level, prompt) in PHYSICAL_DESCRIPTION_PROMPTS {
        let max_tokens = match *level {
            "full" => 1500,
            "complete" => 400,
            _ => 300,
        };
        let content = generate_field(c, context_prompt, prompt, max_tokens).await?;
        let (key, value) = match *level {
            "headAndShoulders" => ("headAndShouldersPrompt", utf16_prefix(&content, 500)),
            "short" => ("shortPrompt", utf16_prefix(&content, 350)),
            "medium" => ("mediumPrompt", utf16_prefix(&content, 500)),
            "long" => ("longPrompt", utf16_prefix(&content, 750)),
            "complete" => ("completePrompt", utf16_prefix(&content, 1000)),
            _ => ("fullDescription", content),
        };
        results.insert(key.into(), Value::String(value));
    }
    Ok(Value::Object(results))
}

/// v4 `generateWardrobeItems` — the shared generation prompt at 2000 tokens,
/// `parseLLMJson`, the shared sanitizer.
async fn generate_wardrobe_items<CMP: CompletionProvider>(
    c: &WizardCallCtx<'_, CMP>,
    context_prompt: &str,
) -> Result<Vec<GeneratedWardrobeItem>, String> {
    let content =
        generate_field(c, context_prompt, &wardrobe_items_generation_prompt(), 2000).await?;
    let items = parse_llm_json(&content).map_err(|e| llm_json_failure_message(&content, &e))?;
    Ok(sanitize_generated_wardrobe_items(&items))
}

/// v4 `generateProperties` — `PROPERTIES_PROMPT` at 300 tokens through
/// `parseGeneratedProperties`.
async fn generate_properties<CMP: CompletionProvider>(
    c: &WizardCallCtx<'_, CMP>,
    context_prompt: &str,
) -> Result<GeneratedProperties, String> {
    let content = generate_field(c, context_prompt, PROPERTIES_PROMPT, 300).await?;
    parse_generated_properties(&content).map_err(|e| llm_json_failure_message(&content, &e))
}

/// v4's `GeneratedProperties` as JSON (`{pronouns, aliases}`; the sanitized
/// pronouns object or `null`).
fn properties_json(props: &GeneratedProperties) -> Value {
    json!({
        "pronouns": props.pronouns.as_ref().map(|p| serde_json::to_value(p).unwrap_or(Value::Null)).unwrap_or(Value::Null),
        "aliases": props.aliases,
    })
}

/// v4 `WizardResult`.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct WizardResult {
    pub success: bool,
    pub generated: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Value>,
}

/// The shared body of both runners. `emit` is the streaming twin's
/// `onProgress` (`None` for the non-streaming twin, which emits nothing);
/// `streaming` selects the twin-specific log wording and snippet. `Err` is
/// a throw BEFORE the field loop (the profile / image / document arms).
async fn run_wizard_core<CMP: CompletionProvider>(
    db: &Db,
    completion: &CMP,
    backend: &dyn StorageBackend,
    request: &WizardRequest,
    user_id: &str,
    mut emit: Option<OnProgress<'_>>,
    streaming: bool,
) -> Result<(Value, Map<String, Value>), String> {
    let db_msg = |e: DbError| e.to_string();

    // Get primary profile
    let pid = request.primary_profile_id.clone();
    let primary_profile = db
        .read_main(move |c| connection_profiles::find_by_id(c, &pid))
        .map_err(db_msg)?
        .filter(|p| {
            p.get("userId")
                .and_then(Value::as_str)
                .is_none_or(|u| u == user_id)
        })
        .ok_or_else(|| "Primary profile not found".to_string())?;

    // Get primary profile API key (resolved for read-order fidelity; the
    // provider seam resolves its own).
    let mut _primary_api_key = String::new();
    if let Some(key_id) = primary_profile
        .get("apiKeyId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        let (key_id, uid) = (key_id.to_string(), user_id.to_string());
        if let Some(key) = db
            .read_main(move |c| api_keys::find_by_id_and_user_id(c, &key_id, &uid))
            .map_err(db_msg)?
        {
            _primary_api_key = key.key_value;
        }
    }

    // Create primary provider — `primaryProfile.baseUrl || undefined` (truthy).
    let call = WizardCallCtx {
        db,
        completion,
        provider: str_of(&primary_profile, "provider"),
        base_url: primary_profile
            .get("baseUrl")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        model_name: str_of(&primary_profile, "modelName"),
        profile_parameters: profile_params_value(&primary_profile),
        user_id,
        character_id: request.character_id.as_deref(),
    };

    // Handle image description if needed
    let mut image_description: Option<String> = None;
    if matches!(request.source_type.as_str(), "upload" | "gallery")
        && request.image_id.as_deref().is_some_and(|s| !s.is_empty())
    {
        let iid = request.image_id.clone().unwrap();
        let image_file = db
            .read_main(move |c| FilesRepository::new(c).find_full_by_id(&iid))
            .map_err(db_msg)?
            .filter(|f| f.user_id == user_id)
            .ok_or_else(|| "Image not found".to_string())?;

        let mut vision_profile = primary_profile.clone();
        if !profile_supports_mime_type(&primary_profile, &image_file.mime_type) {
            let Some(vpid) = request
                .vision_profile_id
                .as_deref()
                .filter(|s| !s.is_empty())
            else {
                return Err("Vision profile required for image analysis".to_string());
            };
            let vpid = vpid.to_string();
            let secondary = db
                .read_main(move |c| connection_profiles::find_by_id(c, &vpid))
                .map_err(db_msg)?
                .filter(|p| {
                    p.get("userId")
                        .and_then(Value::as_str)
                        .is_none_or(|u| u == user_id)
                })
                .ok_or_else(|| "Vision profile not found".to_string())?;
            // (the secondary's api key is resolved by the seam)
            if let Some(key_id) = secondary
                .get("apiKeyId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                let (key_id, uid) = (key_id.to_string(), user_id.to_string());
                let _ = db
                    .read_main(move |c| api_keys::find_by_id_and_user_id(c, &key_id, &uid))
                    .map_err(db_msg)?;
            }
            vision_profile = secondary;
        }

        image_description = Some(
            generate_image_description(
                db,
                completion,
                backend,
                &image_file,
                &vision_profile,
                user_id,
                request.character_id.as_deref(),
            )
            .await?,
        );
    }

    // Handle document content extraction if needed
    let mut document_content: Option<String> = None;
    if request.source_type == "document"
        && request
            .document_id
            .as_deref()
            .is_some_and(|s| !s.is_empty())
    {
        let did = request.document_id.clone().unwrap();
        let document_file = db
            .read_main(move |c| FilesRepository::new(c).find_full_by_id(&did))
            .map_err(db_msg)?
            .filter(|f| f.user_id == user_id)
            .ok_or_else(|| "Document not found".to_string())?;
        let extract = extract_file_content(db, backend, &document_file);
        // `if (!extractResult.success || !extractResult.content)` — an empty
        // extraction is a failure too.
        match extract.content.filter(|c| !c.is_empty()) {
            Some(content) if extract.success => document_content = Some(content),
            _ => {
                return Err(extract
                    .error
                    .filter(|e| !e.is_empty())
                    .unwrap_or_else(|| "Failed to extract document content".to_string()))
            }
        }
    }

    // Generate requested fields
    let mut generated: Map<String, Value> = Map::new();
    let mut errors: Map<String, Value> = Map::new();
    let mut effective_character_name = request.character_name.clone();

    // If 'name' is in the fields to generate, generate it first
    if request.fields_to_generate.iter().any(|f| f == "name") {
        if let Some(e) = emit.as_deref_mut() {
            e(json!({"type": "field_start", "field": "name"}));
        }
        let name_context_prompt = build_context_prompt(
            "",
            &request.background,
            request.existing_data.as_ref(),
            image_description.as_deref(),
            document_content.as_deref(),
        );
        match generate_field(
            &call,
            &name_context_prompt,
            field_prompt("name").expect("the name prompt"),
            100,
        )
        .await
        {
            Ok(generated_name) => {
                generated.insert("name".into(), Value::String(generated_name.clone()));
                effective_character_name = generated_name.clone();
                if let Some(e) = emit.as_deref_mut() {
                    e(json!({
                        "type": "field_complete",
                        "field": "name",
                        "snippet": get_snippet(&Value::String(generated_name), 100),
                    }));
                }
            }
            Err(error_message) => {
                errors.insert("name".into(), Value::String(error_message.clone()));
                if let Some(e) = emit.as_deref_mut() {
                    e(json!({"type": "field_error", "field": "name", "error": error_message}));
                }
                tracing::error!(
                    target: WIZARD_LOG_TARGET,
                    context = %ctx(json!({"error": error_message})),
                    "[CharacterWizard] Failed to generate field: name"
                );
            }
        }
    }

    // Build context prompt with the effective character name
    let context_prompt = build_context_prompt(
        &effective_character_name,
        &request.background,
        request.existing_data.as_ref(),
        image_description.as_deref(),
        document_content.as_deref(),
    );

    // Generate remaining fields (excluding 'name' which was already handled)
    for field in &request.fields_to_generate {
        if field == "name" {
            continue;
        }
        if let Some(e) = emit.as_deref_mut() {
            e(json!({"type": "field_start", "field": field}));
        }
        let outcome: Result<(Value, String), String> = async {
            match field.as_str() {
                "physicalDescription" => {
                    let phys = generate_physical_descriptions(&call, &context_prompt).await?;
                    let snippet = get_snippet(&phys, 100);
                    Ok((phys, snippet))
                }
                "wardrobeItems" => {
                    let items = generate_wardrobe_items(&call, &context_prompt).await?;
                    let snippet = format!("{} wardrobe item(s) generated", items.len());
                    Ok((serde_json::to_value(items).unwrap_or(Value::Null), snippet))
                }
                "properties" => {
                    let props = generate_properties(&call, &context_prompt).await?;
                    let snippet = if streaming {
                        describe_generated_properties(&props, "no pronouns found")
                    } else {
                        String::new()
                    };
                    Ok((properties_json(&props), snippet))
                }
                other => {
                    let field_prompt_text = field_prompt(other)
                        .ok_or_else(|| format!("Unknown wizard field: {other}"))?;
                    let max_tokens = match other {
                        "exampleDialogues" | "systemPrompt" | "firstMessage" => 1000,
                        "scenarios" => 4000,
                        _ => 500,
                    };
                    let raw_content =
                        generate_field(&call, &context_prompt, field_prompt_text, max_tokens)
                            .await?;
                    let field_value = if other == "scenarios" {
                        match parse_llm_json(&raw_content) {
                            Ok(v) => v,
                            Err(_) => {
                                tracing::warn!(
                                    target: WIZARD_LOG_TARGET,
                                    context = %ctx(json!({"rawContent": utf16_prefix(&raw_content, 200)})),
                                    "[CharacterWizard] Failed to parse scenarios JSON, storing as raw string"
                                );
                                Value::String(raw_content)
                            }
                        }
                    } else {
                        Value::String(raw_content)
                    };
                    let snippet = get_snippet(&field_value, 100);
                    Ok((field_value, snippet))
                }
            }
        }
        .await;
        match outcome {
            Ok((value, snippet)) => {
                generated.insert(field.clone(), value);
                if let Some(e) = emit.as_deref_mut() {
                    e(json!({"type": "field_complete", "field": field, "snippet": snippet}));
                }
            }
            Err(error_message) => {
                errors.insert(field.clone(), Value::String(error_message.clone()));
                if let Some(e) = emit.as_deref_mut() {
                    e(json!({"type": "field_error", "field": field, "error": error_message}));
                }
                tracing::error!(
                    target: WIZARD_LOG_TARGET,
                    context = %ctx(json!({"error": error_message})),
                    "[CharacterWizard] Failed to generate field: {field}"
                );
            }
        }
    }

    let complete_ctx = ctx(json!({
        "fieldsGenerated": generated.keys().collect::<Vec<_>>(),
        "fieldsWithErrors": errors.keys().collect::<Vec<_>>(),
    }));
    if streaming {
        tracing::info!(
            target: WIZARD_LOG_TARGET,
            context = %complete_ctx,
            "[CharacterWizard] Complete (streaming)"
        );
    } else {
        tracing::info!(
            target: WIZARD_LOG_TARGET,
            context = %complete_ctx,
            "[CharacterWizard] Complete"
        );
    }
    Ok((Value::Object(generated), errors))
}

/// v4 `runCharacterWizard(request, userId, repos)` — the non-streaming twin.
/// `Err` is v4's throw (the route's generic 500).
pub async fn run_character_wizard<CMP: CompletionProvider>(
    db: &Db,
    completion: &CMP,
    backend: &dyn StorageBackend,
    request: &WizardRequest,
    user_id: &str,
) -> Result<WizardResult, String> {
    tracing::info!(
        target: WIZARD_LOG_TARGET,
        context = %starting_ctx(user_id, request),
        "[CharacterWizard] Starting"
    );
    let (generated, errors) =
        run_wizard_core(db, completion, backend, request, user_id, None, false).await?;
    Ok(WizardResult {
        success: true,
        generated,
        errors: if errors.is_empty() {
            None
        } else {
            Some(Value::Object(errors))
        },
    })
}

/// v4 `runCharacterWizardStreaming(request, userId, repos, onProgress)` — the
/// streaming twin: never fails; a throw before the field loop is the
/// `Streaming generation failed` error line plus `done {error, fullContent:
/// {}, errors: {_fatal}}`.
pub async fn run_character_wizard_streaming<CMP: CompletionProvider>(
    db: &Db,
    completion: &CMP,
    backend: &dyn StorageBackend,
    request: &WizardRequest,
    user_id: &str,
    on_progress: OnProgress<'_>,
) {
    tracing::info!(
        target: WIZARD_LOG_TARGET,
        context = %starting_ctx(user_id, request),
        "[CharacterWizard] Starting (streaming)"
    );
    on_progress(json!({"type": "start"}));
    match run_wizard_core(
        db,
        completion,
        backend,
        request,
        user_id,
        Some(on_progress),
        true,
    )
    .await
    {
        Ok((generated, errors)) => {
            let mut done = json!({"type": "done", "fullContent": generated});
            if !errors.is_empty() {
                done["errors"] = Value::Object(errors);
            }
            on_progress(done);
        }
        Err(error_message) => {
            tracing::error!(
                target: WIZARD_LOG_TARGET,
                context = %ctx(json!({"error": error_message})),
                "[CharacterWizard] Streaming generation failed"
            );
            on_progress(json!({
                "type": "done",
                "error": error_message,
                "fullContent": {},
                "errors": {"_fatal": error_message},
            }));
        }
    }
}

// === P4.82 ===
/// v4's `generateField` as an EXPORT for the
/// `CHARACTER_HEADSHOULDERS_BACKFILL` job handler (v4
/// `lib/background-jobs/handlers/character-headshoulders-backfill.ts` imports
/// it from this very module).
///
/// This is a call-shape adapter, NOT a second implementation: it assembles the
/// private [`WizardCallCtx`] from v4's ten `generateField` arguments and
/// delegates to the one [`generate_field`] the wizard's own five generators
/// use, so the temperature, the `No response from model` refusal, the
/// `CHARACTER_WIZARD` log row and the trailing trim can never diverge between
/// the two callers. The wizard's own path is untouched — its families are the
/// proof.
///
/// `profile_parameters` is `Option<Value>` because v4's tenth argument is
/// optional and the backfill handler does NOT pass it (so the wire carries no
/// profile parameters on that path).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn generate_field_for_caller<CMP: CompletionProvider>(
    db: &Db,
    completion: &CMP,
    provider: &str,
    base_url: Option<&str>,
    model_name: &str,
    context_prompt: &str,
    field_prompt: &str,
    max_tokens: i64,
    user_id: &str,
    character_id: Option<&str>,
    profile_parameters: Option<Value>,
) -> Result<String, String> {
    let call = WizardCallCtx {
        db,
        completion,
        provider: provider.to_string(),
        base_url: base_url.map(str::to_string),
        model_name: model_name.to_string(),
        profile_parameters,
        user_id,
        character_id,
    };
    generate_field(&call, context_prompt, field_prompt, max_tokens).await
}
// === end P4.82 ===
