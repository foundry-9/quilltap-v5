//! v4 `lib/services/ai-import.service.ts` — Summon From Lore (`p4.9k`,
//! P4.9K2). "Multi-call LLM orchestration service that analyzes source material
//! (wiki pages, story documents, character sheets, freeform notes) and
//! generates a complete character in .qtap export format for import via the
//! existing import system. Each LLM call extracts/generates one aspect of the
//! character. The service assembles the final .qtap structure programmatically
//! (UUIDs, timestamps, manifest)."
//!
//! This file's FIRST half is the pure leaf v4 exports — the prompts, the three
//! assembly functions (`assembleWardrobeItems`, `assembleQtapExport`,
//! `restampStructuralFields`) and the two private helpers the runner reads
//! (`shouldRunStep`, `getSnippet`) — diffed against v4's real exports by
//! `ai_import_assembly_equivalence` (tier 1, no fixture: the minted uuids are
//! remapped `<minted-N>` in first-seen order on both sides, the clock is a
//! parameter here and frozen in the oracle). The runner follows in a later
//! unit.
//!
//! ## Input shape
//!
//! The step results are model-produced JSON, read as `serde_json::Value`
//! throughout (the optimizer precedent): v4 destructures them with JS
//! semantics — `basics.scenario ? … : []` is truthiness, `mem.keywords`
//! copied as-is means an ABSENT key is omitted from the export where an
//! explicit `null` is kept, `Math.max(0, Math.min(1, x))` coerces a string —
//! and a typed struct would normalize exactly the edges the differential
//! exists to catch.
//!
//! ## Recorded (v4's own shape, reproduced)
//!
//! `assembleQtapExport` BUILDS a `chats` array when `includeChats` is set and
//! then never attaches it to `data` or the character — the assembled export
//! carries no chat (the comment above the manifest says the import system
//! handles embedded chats; nothing embeds one). Ported as it stands: the build
//! keeps its one observable arm — a `chats` step result without a `messages`
//! array THROWS out of the assembly — and its output is dropped. A candidate
//! upstream filing.

use std::collections::HashMap;

use serde_json::{json, Map, Value};

use crate::api::system_qtap::js_truthy;
use crate::chat_activity::is_character_authored_message;
use crate::clock::iso_from_unix_ms;
use crate::generators::field_semantics::{
    FULL_FIELD_SEMANTICS, PHYSICAL_DESCRIPTION_SEMANTICS, PROMPT_SEMANTICS, PROPERTIES_SEMANTICS,
};
use crate::generators::generated_items::{order_json_leaf_first, wardrobe_items_generation_prompt};
use crate::jsstr::js_trim;
use crate::pascal::js_value::{to_js_string, to_number};

// ============================================================================
// Types
// ============================================================================

/// v4 `AIImportStepName` — the twelve steps, in v4's declaration order.
pub const AI_IMPORT_STEP_NAMES: &[&str] = &[
    "analyzing",
    "character_basics",
    "first_message",
    "system_prompts",
    "physical_descriptions",
    "wardrobe_items",
    "pronouns",
    "memories",
    "chats",
    "assembly",
    "validation",
    "repair",
];

/// The `llm_logs.type` every import call writes (v4 `type: 'AI_IMPORT'`).
pub const LOG_TYPE_AI_IMPORT: &str = "AI_IMPORT";

/// v4 `AIImportRequest`, post the route's hand-rolled read (`route.ts:1196-
/// 1204`): `sourceFileIds || []`, `sourceText || ''`, `includeMemories ??
/// true`, `includeChats ?? false`, `existingResult || undefined`,
/// `regenerateSteps || undefined`.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiImportRequest {
    pub profile_id: String,
    pub source_file_ids: Vec<String>,
    pub source_text: String,
    pub include_memories: bool,
    pub include_chats: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regenerate_steps: Option<Vec<String>>,
}

// ============================================================================
// Constants (wire bytes)
// ============================================================================

/// v4 `SYSTEM_MESSAGE` (byte-exact) — the system message on every import call.
pub const SYSTEM_MESSAGE: &str = "You are a character extraction assistant for Quilltap, a creative writing and roleplay platform. Your job is to analyze source material about a character and extract or generate structured character data. Always respond with ONLY valid JSON — no markdown code fences, no explanations, no extra text.";

/// v4 `SOURCE_ANALYSIS_THRESHOLD` — the source-context length (UTF-16 units)
/// above which the `analyzing` step runs.
pub const SOURCE_ANALYSIS_THRESHOLD: usize = 30000;
/// v4 `MAX_REPAIR_ATTEMPTS`.
pub const MAX_REPAIR_ATTEMPTS: usize = 2;

/// v4 `getAnalyzingPrompt(sourceLength)` (byte-exact).
pub fn get_analyzing_prompt(source_length: usize) -> String {
    format!(
        r#"The following source material is {source_length} characters long. Analyze it and provide a structured summary to guide character extraction.

Respond with JSON:
{{
  "characterName": "the character's name if found",
  "setting": "the world/setting description",
  "keyTraits": ["trait1", "trait2"],
  "relationships": ["relationship1"],
  "physicalDetails": "any physical appearance details found",
  "backgroundSummary": "brief background summary",
  "speechPatterns": "any speech patterns or dialect noted",
  "sourceCoverage": "what the source material covers well vs what needs generation"
}}"#
    )
}

/// v4 `CHARACTER_BASICS_PROMPT` (byte-exact) — `${FULL_FIELD_SEMANTICS}` then
/// the extraction instructions.
pub fn character_basics_prompt() -> String {
    format!(
        r#"{FULL_FIELD_SEMANTICS}

Extract or generate the character's basic information from the source material. Use the vantage-point rule above to decide what belongs in IDENTITY vs DESCRIPTION vs PERSONALITY. Do NOT put the same content under two different fields, and do NOT put physical appearance into DESCRIPTION — physical appearance is generated separately and lives in physicalDescription. Likewise, system prompts, pronouns/aliases, physical descriptions, and wardrobe are each extracted in their own later step — leave that content out of these fields.

Respond with JSON:
{{
  "name": "Character's full name",
  "title": "A short epithet or title (2-5 words, like 'The Wandering Scholar')",
  "identity": "1-2 paragraphs of public-knowledge / outside-view facts only — name, station, occupation, public reputation, signifying outward facts a stranger could plausibly know without having spoken to the character. Never internal motivation, never private mannerisms, never physical appearance. Written about the character from outside, because only others ever read it: 'Ariadne is a research librarian at the Athenaeum.'",
  "description": "1-2 paragraphs of what someone who has interacted with the character would notice — behaviour, mannerisms, frequent verbal patterns, conversational tics. NOT physical appearance (that lives in physicalDescription). NOT the character's private inner monologue. NOT the public-facing reputation that already lives in identity. Write in third person, present tense: 'She finishes other people's sentences and apologises afterwards.'",
  "manifesto": "The basic tenets — the most important facts of the character's existence. The axiomatic core that every other field should remain consistent with. Not a vantage-point field; nobody 'sees' the manifesto, it is the load-bearing truth the character is built on. Short, declarative, foundational. If a fact would be devastating to contradict, it belongs here. Addressed to the character themselves, who is the only one who ever reads it: 'You do not lie to Charlie, not even kindly.'",
  "personality": "1-2 paragraphs of the character's own self-knowledge — inner drivers of speech and behaviour, motivations, beliefs, emotional tendencies, things only the character knows about themselves unless they choose to share them. Never put outward behaviour someone else would observe here, and never put public-facing identity facts. Addressed to the character themselves, whose self-knowledge it is: 'You keep your worry behind your teeth.'",
  "scenario": "1-2 paragraphs setting the default scene for interactions. Present tense, describing environment and relationship context. The scenario is the stage, not the actor — do not restate personality or appearance here, and address no one; the referent is the world: 'The reading room is empty at this hour, rain against the high windows.'"
}}

If the source material clearly provides information for a field, extract and adapt it. If not, generate appropriate content that fits the character."#
    )
}

/// v4 `FIRST_MESSAGE_PROMPT` (byte-exact; the `\\n` sequences are literal
/// backslash-n in the prompt, as v4's template literal escapes them).
pub const FIRST_MESSAGE_PROMPT: &str = r#"Generate a first message and example dialogues for this character based on the source material.

Respond with JSON:
{
  "firstMessage": "An engaging opening message from the character (1-3 paragraphs). Include *actions* and dialogue. This is how the character introduces themselves or sets the scene when first meeting someone.",
  "exampleDialogues": "2-3 example dialogue exchanges showing the character's voice.\nFormat:\n{{char}}: [dialogue and *actions*]\n{{user}}: [response]\n{{char}}: [follow-up]\n\nSeparate exchanges with a blank line."
}"#;

/// v4 `SYSTEM_PROMPTS_PROMPT` (byte-exact).
pub fn system_prompts_prompt() -> String {
    format!(
        r#"{PROMPT_SEMANTICS}

Create system prompts that instruct an AI how to roleplay as this character.

Respond with JSON array:
[
  {{
    "name": "Main",
    "content": "A comprehensive system prompt (300-500 words) covering identity, speech patterns, behaviors, boundaries, and relationship dynamics. Write in second person ('You are...', 'You always...').",
    "isDefault": true
  }}
]

The main prompt should capture the character's essence from the source material. Include specific details about speech patterns, mannerisms, and reactions that make the character unique. If the source material implies distinct interaction modes or model-specific needs, you may add 1-2 additional named prompts (isDefault false) tailored to them."#
    )
}

/// v4 `PHYSICAL_DESCRIPTIONS_PROMPT` (byte-exact).
pub fn physical_descriptions_prompt() -> String {
    format!(
        r#"{PHYSICAL_DESCRIPTION_SEMANTICS}

Generate physical descriptions of this character at varying detail levels for image generation.
Do NOT include clothing, outfits, or accessories — those are handled separately by the wardrobe system. Describe the person as if nothing removable were part of the description; focus only on the character's physical traits. Include the character's natural hair (colour, length, texture) here, but not a styled hairdo — hairstyles are wardrobe items.

Respond with JSON:
{{
  "headAndShouldersPrompt": "Head-and-shoulders portrait description, max 500 chars. ONLY face shape, skin tone, eyes, hair, expression, neckline and visible upper attire. NEVER describe breasts, chest, torso, waist, hips, legs, or anything below the shoulders. No full outfits — only the topmost neckline/collar.",
  "shortPrompt": "Extremely concise visual description, max 350 chars. Comma-separated descriptors: hair, eyes, skin, body type, one distinctive feature. No clothing.",
  "mediumPrompt": "Concise visual description, max 500 chars. Include hair, eyes, skin, body type, facial features. No clothing. Continuous description.",
  "longPrompt": "Detailed visual description, max 750 chars. Complete hair, eye details, skin, facial structure, body type, posture, marks/features. No clothing.",
  "completePrompt": "Comprehensive visual description, max 1000 chars. All physical details optimized for AI image generation. No clothing.",
  "fullDescription": "Complete physical description in markdown format with sections: ## Overview, ## Face & Head, ## Body, ## Distinctive Features. No clothing section."
}}"#
    )
}

/// v4 `WARDROBE_ITEMS_PROMPT` (byte-exact) — the shared generation prompt plus
/// the import's grounding sentence.
pub fn wardrobe_items_prompt() -> String {
    format!(
        "{}\n\nGround every item in the source material where it describes the character's clothing and style; generate plausible items only where the source is silent.",
        wardrobe_items_generation_prompt()
    )
}

/// v4 `PROPERTIES_EXTRACTION_PROMPT` (byte-exact).
pub fn properties_extraction_prompt() -> String {
    format!(
        r#"{PROPERTIES_SEMANTICS}

Determine the character's pronouns and aliases from the source material.

Respond with JSON:
{{
  "pronouns": {{ "subject": "he/she/they/etc", "object": "him/her/them/etc", "possessive": "his/her/their/etc" }},
  "aliases": ["Nickname", "Alternate name"]
}}

Rules:
- If the source material does NOT clearly indicate pronouns, set "pronouns" to JSON null (literally: null). Do not invent placeholders like "unknown", "n/a", or empty strings — return null instead.
- "aliases" lists only nicknames or alternate names the source material actually supports — names other people call the character. Do NOT include the character's primary name, and do NOT include their title/epithet. Use an empty array when there are none."#
    )
}

/// v4 `MEMORIES_PROMPT` (byte-exact).
pub const MEMORIES_PROMPT: &str = r#"Generate memories that this character would have based on the source material. These are key facts, experiences, and knowledge the character should remember.

Respond with JSON array (5-15 memories):
[
  {
    "content": "Detailed memory content (1-3 sentences describing what happened or what the character knows)",
    "summary": "One-sentence distilled version for quick context injection",
    "keywords": ["keyword1", "keyword2", "keyword3"],
    "importance": 0.7
  }
]

Importance scale: 0.0 (trivial) to 1.0 (core identity). Focus on memories that define the character's history, key relationships, formative events, and critical knowledge."#;

/// v4 `CHATS_PROMPT` (byte-exact).
pub const CHATS_PROMPT: &str = r#"Generate an example chat conversation featuring this character to demonstrate their personality and conversational style.

Respond with JSON:
{
  "title": "A descriptive title for this conversation",
  "messages": [
    { "role": "ASSISTANT", "content": "Character's opening message with *actions* and dialogue" },
    { "role": "USER", "content": "User's response" },
    { "role": "ASSISTANT", "content": "Character's reply showing personality" },
    { "role": "USER", "content": "Another user message" },
    { "role": "ASSISTANT", "content": "Character's response demonstrating range" }
  ]
}

Create 5-8 messages showing natural conversation flow with the character's unique voice and mannerisms."#;

/// v4's repair prompt (the `repairPrompt` template literal inside the runner,
/// byte-exact): the validation errors joined by newline, the sections with
/// errors pretty-printed, the section keys joined by `, `.
pub fn repair_prompt(
    errors: &[String],
    sections_to_repair: &Value,
    section_keys: &[String],
) -> String {
    format!(
        r#"The following .qtap export data has validation errors. Fix the issues and return the corrected data as JSON.

Validation errors:
{}

Current data (sections with errors only):
{}

Return ONLY a JSON object with the same section keys ({}), containing the corrected arrays. Do not change the structure, only fix the values that cause validation errors. If a field should be absent rather than null, remove it entirely."#,
        errors.join("\n"),
        serde_json::to_string_pretty(sections_to_repair).unwrap_or_default(),
        section_keys.join(", ")
    )
}

// ============================================================================
// Helpers (v4's private `shouldRunStep` / `getSnippet`)
// ============================================================================

/// v4 `shouldRunStep(step)`: "Not in existingResult, OR explicitly in
/// regenerateSteps". `existing_result` is the request's bag (a `Value` — the
/// `step in request.existingResult` test is key PRESENCE, so a step present
/// with `null` still counts as done).
pub fn should_run_step(
    step: &str,
    existing_result: Option<&Value>,
    regenerate_steps: Option<&[String]>,
) -> bool {
    if regenerate_steps.is_some_and(|steps| steps.iter().any(|s| s == step)) {
        return true;
    }
    if let Some(existing) = existing_result.filter(|v| js_truthy(Some(v))) {
        if existing.as_object().is_some_and(|o| o.contains_key(step)) {
            return false;
        }
    }
    true
}

/// The first `n` UTF-16 units of `s` (JS `s.substring(0, n)`).
fn utf16_prefix(s: &str, n: usize) -> String {
    String::from_utf16_lossy(&s.encode_utf16().take(n).collect::<Vec<u16>>())
}

/// JS `s.length` in UTF-16 units.
fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// v4 `getSnippet(content, maxLength = 100)` (the import's version — a
/// string is truncated with `...`, an object answers its `name` when that is
/// a string, anything else is `''`).
pub fn get_snippet(content: &Value, max_length: usize) -> String {
    match content {
        Value::String(s) => {
            if utf16_len(s) > max_length {
                format!("{}...", utf16_prefix(s, max_length))
            } else {
                s.clone()
            }
        }
        Value::Object(o) => match o.get("name") {
            Some(Value::String(name)) => name.clone(),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

// ============================================================================
// Assembly (v4 `assembleWardrobeItems` / `assembleQtapExport`)
// ============================================================================

/// `Math.max(0, Math.min(1, x))` over a JSON value with JS coercion — a
/// non-numeric answer is `NaN`, which `JSON.stringify` writes as `null`.
fn clamp_importance(v: Option<&Value>) -> Value {
    let n = match v {
        Some(v) => to_number(v),
        None => f64::NAN,
    };
    if n.is_nan() {
        Value::Null
    } else {
        let clamped = n.clamp(0.0, 1.0);
        // `JSON.stringify(1)` is `1`, never `1.0` — an integral double
        // serializes as an integer.
        if clamped.fract() == 0.0 {
            json!(clamped as i64)
        } else {
            json!(clamped)
        }
    }
}

/// `x || null` — a falsy value becomes JSON `null`.
fn or_null(v: Option<&Value>) -> Value {
    if js_truthy(v) {
        v.cloned().unwrap_or(Value::Null)
    } else {
        Value::Null
    }
}

/// v4 `assembleWardrobeItems(generatedItems, characterId, now)` with the id
/// minter injected (`crypto.randomUUID()` in v4): "Turn sanitized generated
/// wardrobe items into export-shaped rows. Composites reference components by
/// title in the generated shape; ids are minted here (leaf-first) so each
/// composite's `componentItemIds` resolves to the ids of its components in the
/// same batch." Two items whose titles fold to the same lowercase string SHARE
/// one id (v4's `Map` keyed by `title.toLowerCase()` — the last mint wins).
pub fn assemble_wardrobe_items_with(
    generated_items: Option<&[Value]>,
    character_id: &str,
    now: &str,
    mint: &mut dyn FnMut() -> String,
) -> Vec<Value> {
    let Some(items) = generated_items.filter(|i| !i.is_empty()) else {
        return Vec::new();
    };
    let ordered = order_json_leaf_first(items);
    let title_of = |item: &Value| to_js_string(item.get("title").unwrap_or(&Value::Null));
    let mut id_by_title: HashMap<String, String> = HashMap::new();
    for item in &ordered {
        id_by_title.insert(title_of(item).to_lowercase(), mint());
    }
    ordered
        .iter()
        .map(|item| {
            let title = title_of(item);
            let component_item_ids: Vec<Value> = item
                .get("components")
                .and_then(Value::as_array)
                .map(|c| {
                    c.iter()
                        .filter_map(|t| {
                            id_by_title
                                .get(&js_trim(&to_js_string(t)).to_lowercase())
                                .map(|id| Value::String(id.clone()))
                        })
                        .collect()
                })
                .unwrap_or_default();
            json!({
                "id": id_by_title.get(&title.to_lowercase()).cloned(),
                "characterId": character_id,
                "title": item.get("title").cloned().unwrap_or(Value::Null),
                "description": or_null(item.get("description")),
                "imagePrompt": or_null(item.get("imagePrompt")),
                "types": item.get("types").cloned().unwrap_or(Value::Null),
                "componentItemIds": component_item_ids,
                "appropriateness": or_null(item.get("appropriateness")),
                "isDefault": item.get("isDefault") == Some(&Value::Bool(true)),
                "replace": item.get("replace") == Some(&Value::Bool(true)),
                "migratedFromClothingRecordId": null,
                "archivedAt": null,
                "createdAt": now,
                "updatedAt": now,
            })
        })
        .collect()
}

/// [`assemble_wardrobe_items_with`] minting v4 uuids.
pub fn assemble_wardrobe_items(
    generated_items: Option<&[Value]>,
    character_id: &str,
    now: &str,
) -> Vec<Value> {
    assemble_wardrobe_items_with(generated_items, character_id, now, &mut || {
        uuid::Uuid::new_v4().to_string()
    })
}

/// A JS array operation over a value that may not be an array: v4 THROWS the
/// TypeError, and the wire carries V8's wording (measured at the pin): a
/// `.map` on `undefined` / `null` names the property read, a `.map` on any
/// other non-array names the SOURCE EXPRESSION (`(stepResults.system_prompts
/// || []).map is not a function`), and a `for…of` over a non-iterable says
/// `<expression> is not iterable`.
fn require_array<'a>(
    v: Option<&'a Value>,
    expression: &str,
    op: ArrayOp,
) -> Result<&'a Vec<Value>, String> {
    match (v, op) {
        (Some(Value::Array(a)), _) => Ok(a),
        (None, ArrayOp::Map) => {
            Err("Cannot read properties of undefined (reading 'map')".to_string())
        }
        (Some(Value::Null), ArrayOp::Map) => {
            Err("Cannot read properties of null (reading 'map')".to_string())
        }
        (_, ArrayOp::Map) => Err(format!("{expression}.map is not a function")),
        (_, ArrayOp::ForOf) => Err(format!("{expression} is not iterable")),
    }
}

#[derive(Clone, Copy)]
enum ArrayOp {
    Map,
    ForOf,
}

/// v4 `assembleQtapExport(stepResults, includeMemories, includeChats,
/// appVersion)` with the clock (`new Date()` / `Date.now()`) and the id
/// minter injected. `Err` is v4's throw (`Character basics with name are
/// required for assembly`, or the TypeError a malformed `chats` step trips).
pub fn assemble_qtap_export_with(
    step_results: &Value,
    include_memories: bool,
    include_chats: bool,
    app_version: &str,
    now_ms: i64,
    mint: &mut dyn FnMut() -> String,
) -> Result<Value, String> {
    let now = iso_from_unix_ms(now_ms);
    let character_id = mint();
    let user_id = mint(); // Placeholder — remapped during import

    let basics = step_results.get("character_basics");
    // `if (!basics?.name)` — JS falsy on the optional chain.
    let name = basics.and_then(|b| b.get("name"));
    if !js_truthy(name) {
        return Err("Character basics with name are required for assembly".to_string());
    }
    let basics = basics.expect("truthy name implies a basics object");
    let name = name.expect("checked").clone();

    // Build character object (v4's literal order).
    let scenarios = if js_truthy(basics.get("scenario")) {
        json!([{
            "id": mint(),
            "title": "Default",
            "content": basics["scenario"],
            "createdAt": now,
            "updatedAt": now,
        }])
    } else {
        json!([])
    };
    let system_prompts: Vec<Value> = match step_results.get("system_prompts") {
        // `(stepResults.system_prompts || []).map(...)`
        Some(v) if js_truthy(Some(v)) => {
            require_array(Some(v), "(stepResults.system_prompts || [])", ArrayOp::Map)?
                .iter()
                .map(|sp| {
                    json!({
                        "id": mint(),
                        "name": sp.get("name").cloned().unwrap_or(Value::Null),
                        "content": sp.get("content").cloned().unwrap_or(Value::Null),
                        "isDefault": sp.get("isDefault").cloned().unwrap_or(Value::Null),
                        "createdAt": now,
                        "updatedAt": now,
                    })
                })
                .collect::<Vec<_>>()
        }
        _ => Vec::new(),
    };
    let physical_description = match step_results.get("physical_descriptions") {
        Some(pd) if js_truthy(Some(pd)) => {
            let cap = |key: &str, n: usize| {
                let v = pd.get(key);
                let s = if js_truthy(v) {
                    to_js_string(v.unwrap())
                } else {
                    String::new()
                };
                Value::String(utf16_prefix(&s, n))
            };
            let mut o = Map::new();
            o.insert("id".into(), Value::String(mint()));
            o.insert("name".into(), Value::String("AI Generated".into()));
            o.insert(
                "headAndShouldersPrompt".into(),
                cap("headAndShouldersPrompt", 500),
            );
            o.insert("shortPrompt".into(), cap("shortPrompt", 350));
            o.insert("mediumPrompt".into(), cap("mediumPrompt", 500));
            o.insert("longPrompt".into(), cap("longPrompt", 750));
            o.insert("completePrompt".into(), cap("completePrompt", 1000));
            // `fullDescription: pd.fullDescription` — an absent key is
            // `undefined`, which JSON.stringify omits.
            if let Some(full) = pd.get("fullDescription") {
                o.insert("fullDescription".into(), full.clone());
            }
            o.insert("createdAt".into(), Value::String(now.clone()));
            o.insert("updatedAt".into(), Value::String(now.clone()));
            Value::Object(o)
        }
        _ => Value::Null,
    };
    let wardrobe_items = assemble_wardrobe_items_with(
        step_results
            .get("wardrobe_items")
            .and_then(Value::as_array)
            .map(Vec::as_slice),
        &character_id,
        &now,
        mint,
    );

    let mut character = Map::new();
    character.insert("id".into(), Value::String(character_id.clone()));
    character.insert("userId".into(), Value::String(user_id.clone()));
    character.insert("name".into(), name);
    character.insert("scenarios".into(), scenarios);
    character.insert("systemPrompts".into(), Value::Array(system_prompts));
    character.insert("avatarUrl".into(), Value::Null);
    character.insert("defaultImageId".into(), Value::Null);
    character.insert("defaultConnectionProfileId".into(), Value::Null);
    character.insert("defaultPartnerId".into(), Value::Null);
    character.insert("defaultRoleplayTemplateId".into(), Value::Null);
    character.insert("defaultImageProfileId".into(), Value::Null);
    character.insert("sillyTavernData".into(), Value::Null);
    character.insert("isFavorite".into(), Value::Bool(false));
    character.insert("npc".into(), Value::Bool(false));
    character.insert("talkativeness".into(), json!(0.5));
    character.insert("controlledBy".into(), Value::String("llm".into()));
    character.insert("defaultAgentModeEnabled".into(), Value::Null);
    character.insert("partnerLinks".into(), json!([]));
    character.insert(
        "aliases".into(),
        match step_results.get("aliases") {
            Some(a) if js_truthy(Some(a)) => a.clone(),
            _ => json!([]),
        },
    );
    character.insert("pronouns".into(), or_null(step_results.get("pronouns")));
    character.insert("tags".into(), json!([]));
    character.insert("avatarOverrides".into(), json!([]));
    character.insert("physicalDescription".into(), physical_description);
    character.insert("wardrobeItems".into(), Value::Array(wardrobe_items));
    character.insert("createdAt".into(), Value::String(now.clone()));
    character.insert("updatedAt".into(), Value::String(now.clone()));

    // Optional text fields are typed as `string` (not nullable) in the qtap
    // export schema, so emitting an explicit null for a field a step omitted
    // (or failed to produce) fails validation and needlessly trips the LLM
    // repair loop. Include each only when there is a usable string.
    let first_message = step_results.get("first_message");
    let optional_text_fields: [(&str, Option<&Value>); 7] = [
        ("title", basics.get("title")),
        ("identity", basics.get("identity")),
        ("description", basics.get("description")),
        ("manifesto", basics.get("manifesto")),
        ("personality", basics.get("personality")),
        (
            "firstMessage",
            first_message.and_then(|f| f.get("firstMessage")),
        ),
        (
            "exampleDialogues",
            first_message.and_then(|f| f.get("exampleDialogues")),
        ),
    ];
    for (key, value) in optional_text_fields {
        if let Some(Value::String(s)) = value {
            if !js_trim(s).is_empty() {
                character.insert(key.to_string(), Value::String(s.clone()));
            }
        }
    }

    // Build memories array
    let mut memories: Vec<Value> = Vec::new();
    if include_memories {
        if let Some(list) = step_results.get("memories").filter(|m| js_truthy(Some(m))) {
            for mem in require_array(Some(list), "stepResults.memories", ArrayOp::ForOf)? {
                let mut o = Map::new();
                o.insert("id".into(), Value::String(mint()));
                o.insert("characterId".into(), Value::String(character_id.clone()));
                // Copied as-is: an ABSENT key is `undefined` (omitted).
                for key in ["content", "summary", "keywords"] {
                    if let Some(v) = mem.get(key) {
                        o.insert(key.into(), v.clone());
                    }
                }
                o.insert("tags".into(), json!([]));
                o.insert("importance".into(), clamp_importance(mem.get("importance")));
                o.insert("source".into(), Value::String("MANUAL".into()));
                o.insert("reinforcementCount".into(), json!(1));
                o.insert("relatedMemoryIds".into(), json!([]));
                o.insert(
                    "reinforcedImportance".into(),
                    clamp_importance(mem.get("importance")),
                );
                o.insert("createdAt".into(), Value::String(now.clone()));
                o.insert("updatedAt".into(), Value::String(now.clone()));
                memories.push(Value::Object(o));
            }
        }
    }

    // Build chats array — built and then never attached (module header): the
    // one observable arm is the throw on a `chats` step without `messages`.
    if include_chats {
        if let Some(chats) = step_results.get("chats").filter(|c| js_truthy(Some(c))) {
            let _chat_id = mint();
            let messages = require_array(
                chats.get("messages"),
                "stepResults.chats.messages",
                ArrayOp::Map,
            )?;
            let chat_messages: Vec<Value> = messages
                .iter()
                .enumerate()
                .map(|(idx, msg)| {
                    let mut o = Map::new();
                    o.insert("type".into(), Value::String("message".into()));
                    o.insert("id".into(), Value::String(mint()));
                    o.insert(
                        "role".into(),
                        msg.get("role").cloned().unwrap_or(Value::Null),
                    );
                    o.insert(
                        "content".into(),
                        msg.get("content").cloned().unwrap_or(Value::Null),
                    );
                    o.insert(
                        "createdAt".into(),
                        Value::String(iso_from_unix_ms(now_ms + idx as i64 * 1000)),
                    );
                    if msg.get("role").and_then(Value::as_str) == Some("ASSISTANT") {
                        o.insert("participantId".into(), Value::String(character_id.clone()));
                    }
                    Value::Object(o)
                })
                .collect();
            // `lastMessageAt` — the one predicate (`chat-activity.ts`); computed
            // for fidelity with v4's evaluation order, then dropped with the chat.
            let _last_message_at = chat_messages
                .iter()
                .rfind(|m| is_character_authored_message(m))
                .and_then(|m| m.get("createdAt").cloned());
        }
    }

    // Determine export type based on what we have
    let mut counts = Map::new();
    counts.insert("characters".into(), json!(1));
    let mut data = Map::new();
    data.insert("characters".into(), json!([Value::Object(character)]));
    if !memories.is_empty() {
        counts.insert("memories".into(), json!(memories.len()));
        data.insert("memories".into(), Value::Array(memories));
    }

    Ok(json!({
        "manifest": {
            "format": "quilltap-export",
            "version": "1.0",
            "exportType": "characters",
            "createdAt": now,
            "appVersion": app_version,
            "settings": {
                "includeMemories": include_memories,
                "scope": "selected",
                "selectedIds": [character_id],
            },
            "counts": Value::Object(counts),
        },
        "data": Value::Object(data),
    }))
}

/// [`assemble_qtap_export_with`] minting v4 uuids.
pub fn assemble_qtap_export(
    step_results: &Value,
    include_memories: bool,
    include_chats: bool,
    app_version: &str,
    now_ms: i64,
) -> Result<Value, String> {
    assemble_qtap_export_with(
        step_results,
        include_memories,
        include_chats,
        app_version,
        now_ms,
        &mut || uuid::Uuid::new_v4().to_string(),
    )
}

// ============================================================================
// Structural normalization (v4 `restampStructuralFields`)
// ============================================================================

/// v4 `UUID_RE` — `/^[0-9a-f]{8}-…-[0-9a-f]{12}$/i` (no version nibble check,
/// unlike Zod's).
fn is_valid_uuid(v: Option<&Value>) -> bool {
    let Some(Value::String(s)) = v else {
        return false;
    };
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    b.iter().enumerate().all(|(i, c)| match i {
        8 | 13 | 18 | 23 => *c == b'-',
        _ => c.is_ascii_hexdigit(),
    })
}

fn is_non_empty_timestamp(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::String(s)) if !js_trim(s).is_empty())
}

/// v4 `restampStructuralFields(data, now)`: "Re-apply the structural
/// scaffolding — ids and createdAt/updatedAt — that the assembler owns on the
/// character's nested collections (systemPrompts, scenarios,
/// physicalDescription, wardrobeItems) and on memories. […] Idempotent: when the
/// export came straight from the assembler (no repair), every field is already
/// present and this is a no-op. Returns the number of fields it had to fix,
/// for logging." MUTATES `data` in place, as v4 does.
pub fn restamp_structural_fields_with(
    data: &mut Value,
    now: &str,
    mint: &mut dyn FnMut() -> String,
) -> i64 {
    let mut fixes = 0i64;
    let ensure_entry_scaffolding =
        |entry: &mut Map<String, Value>, fixes: &mut i64, mint: &mut dyn FnMut() -> String| {
            if !is_valid_uuid(entry.get("id")) {
                entry.insert("id".into(), Value::String(mint()));
                *fixes += 1;
            }
            if !is_non_empty_timestamp(entry.get("createdAt")) {
                entry.insert("createdAt".into(), Value::String(now.to_string()));
                *fixes += 1;
            }
            if !is_non_empty_timestamp(entry.get("updatedAt")) {
                entry.insert("updatedAt".into(), Value::String(now.to_string()));
                *fixes += 1;
            }
        };

    if let Some(characters) = data.get_mut("characters").and_then(Value::as_array_mut) {
        for raw_character in characters.iter_mut() {
            let Some(character) = raw_character.as_object_mut() else {
                continue;
            };
            for key in ["systemPrompts", "scenarios"] {
                if let Some(collection) = character.get_mut(key).and_then(Value::as_array_mut) {
                    for entry in collection.iter_mut() {
                        if let Some(o) = entry.as_object_mut() {
                            ensure_entry_scaffolding(o, &mut fixes, mint);
                        }
                    }
                }
            }
            if let Some(pd) = character
                .get_mut("physicalDescription")
                .and_then(Value::as_object_mut)
            {
                ensure_entry_scaffolding(pd, &mut fixes, mint);
                let blank_name = match pd.get("name") {
                    Some(Value::String(s)) => js_trim(s).is_empty(),
                    _ => true,
                };
                if blank_name {
                    pd.insert("name".into(), Value::String("AI Generated".into()));
                    fixes += 1;
                }
            }
            let character_id_ok = is_valid_uuid(character.get("id"));
            let character_id = character.get("id").cloned();
            if let Some(items) = character
                .get_mut("wardrobeItems")
                .and_then(Value::as_array_mut)
            {
                for raw_item in items.iter_mut() {
                    let Some(item) = raw_item.as_object_mut() else {
                        continue;
                    };
                    ensure_entry_scaffolding(item, &mut fixes, mint);
                    if !is_valid_uuid(item.get("characterId")) && character_id_ok {
                        item.insert("characterId".into(), character_id.clone().unwrap());
                        fixes += 1;
                    }
                    // Repair can rewrite componentItemIds wholesale; keep only
                    // entries that are real uuids so composites never carry
                    // dangling references.
                    if let Some(original) = item.get("componentItemIds").cloned() {
                        let filtered: Vec<Value> = original
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter(|v| is_valid_uuid(Some(v)))
                                    .cloned()
                                    .collect()
                            })
                            .unwrap_or_default();
                        let changed = match original.as_array() {
                            Some(a) => filtered.len() != a.len(),
                            None => true,
                        };
                        if changed {
                            item.insert("componentItemIds".into(), Value::Array(filtered));
                            fixes += 1;
                        }
                    }
                }
            }
        }
    }

    if let Some(memories) = data.get_mut("memories").and_then(Value::as_array_mut) {
        for raw_memory in memories.iter_mut() {
            if let Some(o) = raw_memory.as_object_mut() {
                ensure_entry_scaffolding(o, &mut fixes, mint);
            }
        }
    }

    fixes
}

/// [`restamp_structural_fields_with`] minting v4 uuids.
pub fn restamp_structural_fields(data: &mut Value, now: &str) -> i64 {
    restamp_structural_fields_with(data, now, &mut || uuid::Uuid::new_v4().to_string())
}
