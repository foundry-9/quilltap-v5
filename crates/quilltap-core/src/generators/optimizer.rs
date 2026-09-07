//! v4 `lib/services/character-optimizer.service.ts` — Aurora's "Refine from
//! Memories" (`p4.9k`, P4.9K1). **Ported from the POST-bug-119 shape** (v4
//! `15573c3a1`, 2026-09-02, drift-ledger §3): the pre-fix inline `.filter` on a
//! `parseLLMJson<OptimizerSuggestion[]>` cast never existed in v5 and is never
//! transcribed here.
//!
//! The first half of this file is the PURE half — the types, the coercions, and
//! every prompt builder. Each prompt string IS wire bytes: it reaches a paid
//! model verbatim, so all of it is diffed against v4's real exports by
//! `character_optimizer_prompts_equivalence`. The second half (P4.9K1 unit 4)
//! is the runner — `run_character_optimizer` — diffed end-to-end by
//! `character_optimizer_tier3_equivalence`.
//!
//! ## Input shape
//!
//! Character / wardrobe / memory / analysis inputs are read as
//! `serde_json::Value`, the shape v5's repos and the LLM parse already hand
//! around (the `services::file_fallback` precedent). That is deliberate: v4
//! interpolates these fields into prose with JS semantics — `x || '(empty)'` is
//! JS truthiness, `${x}` is JS `ToString`, and an ABSENT key renders
//! `undefined` where an explicit `null` renders `null`. Modelling them as typed
//! Rust structs would silently normalize exactly the edges the differential
//! exists to catch. [`crate::api::system_qtap::js_truthy`] and
//! [`crate::pascal::js_value::to_js_string`] are those two semantics.

use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;
use serde_json::{json, Value};

use crate::api::system_qtap::js_truthy;
use crate::cheap_llm::{build_character_cache_key, profile_params_value};
use crate::clock::{iso_from_unix_ms, iso_to_ms};
use crate::db::database_store::write_database_document;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::memories_read::{
    find_by_character_about_character, search_by_content_about_character,
};
use crate::db::runtime::Db;
use crate::db::vector_store::CharacterVectorStore;
use crate::db::{
    api_keys, characters_read, connection_profiles, embedding_profiles, wardrobe_read, DbError,
};
use crate::generators::field_semantics::{
    FIELD_SEMANTICS_PREAMBLE, FULL_FIELD_SEMANTICS, PROPERTIES_SEMANTICS, WARDROBE_SEMANTICS,
};
use crate::generators::generated_items::sanitize_generated_wardrobe_items;
use crate::generators::llm_json::{
    escape_control_chars_in_strings, parse_llm_json, repair_truncated_json, strip_code_fences,
    LlmJsonError,
};
use crate::jsnum::to_fixed;
use crate::jsstr::{js_trim, js_trim_end, utf16_len};
use crate::memory_weighting::{calculate_effective_weight, MemoryInputs, DEFAULT_WEIGHTING_CONFIG};
use crate::model::completion::{
    CompletionMessage, CompletionParams, CompletionProvider, CompletionResponse,
};
use crate::model::embedding::{EmbeddingPriority, EmbeddingProvider};
use crate::pascal::js_value::to_js_string;
use crate::services::llm_logging::{
    log_llm_call, LogContext, LogLlmCallParams, LogRequest, LogRequestMessage, LogResponse,
};

// ============================================================================
// Constants (wire bytes)
// ============================================================================

/// v4 `SYSTEM_MESSAGE` (byte-exact) — the system message on every optimizer
/// completion.
pub const SYSTEM_MESSAGE: &str = r#"You are a character analysis assistant for Quilltap, a creative writing and roleplay platform. Your job is to analyze a character's accumulated memories and identify behavioral patterns that should be reflected in their configuration.

Key concepts:
- Characters can have MULTIPLE named scenarios. A scenario is a setting for a chat — it describes the environment, circumstances, and context in which an interaction takes place. Scenarios set the stage but do not fundamentally change the character's personality, voice, or behavior. Think of them as different locations or situations where the character might be encountered.
- Characters can have MULTIPLE named system prompts. Each system prompt provides different instructions for how the AI should roleplay the character, potentially for different contexts or styles of interaction.
- Characters have a WARDROBE of slot-typed clothing/accessory items (top, bottom, footwear, accessories, hair) that is separate from their physical description — the physical description covers only the person, nothing removable. The "hair" slot holds a hairstyle or hairdo (braided, permed, an updo, a wig) — the styling, not the hair itself; natural hair colour, length, and texture stay in the physical description.
- Characters have structured PROPERTIES: pronouns (read-only for you) and aliases (nicknames others actually call the character).

Always respond with ONLY valid JSON — no markdown code fences, no explanations, no extra text."#;

/// v4 `MIN_REINFORCED_MEMORIES`.
pub const MIN_REINFORCED_MEMORIES: i64 = 2;
/// v4 `MAX_MEMORIES_FOR_ANALYSIS` — also `optimizeStreamSchema`'s `maxMemories`
/// default.
pub const MAX_MEMORIES_FOR_ANALYSIS: i64 = 30;
/// v4 `MIN_SIGNIFICANCE_THRESHOLD`.
pub const MIN_SIGNIFICANCE_THRESHOLD: f64 = 0.3;

/// v4 `SUGGESTION_SCHEMA_PREAMBLE` (byte-exact) — appended to six of the seven
/// suggestion prompts.
pub const SUGGESTION_SCHEMA_PREAMBLE: &str = r#"Each suggestion object in the JSON array must follow this schema:
{
  "field": "identity|description|manifesto|personality|exampleDialogues|talkativeness|scenarios|systemPrompt|physicalDescription|wardrobeItems|aliases",
  "subId": "ID of the existing scenario, system prompt, or wardrobe item being updated (only when refining an existing item); for a physicalDescription suggestion, the sub-field key being refined — one of fullDescription, headAndShouldersPrompt, shortPrompt, mediumPrompt, longPrompt, completePrompt",
  "subName": "Human-readable name of the existing sub-item, or the label of the physical sub-field (only when subId is set)",
  "name": "Name for a NEW system prompt or wardrobe item (only when no subId is provided)",
  "currentValue": "The current text of the field/item being changed",
  "proposedValue": "The complete new text for the field/item",
  "rationale": "Why this change is suggested, referencing specific behavioral patterns",
  "significance": 0.5,
  "memoryExcerpts": ["Memory excerpt 1", "Memory excerpt 2"]
}

Rules that apply to every suggestion:
- Assign a significance score: 0.3+ = noticeable shift, 0.6+ = fundamental behavioral change.
- Include 1-3 memory excerpts that support the suggestion.
- Only propose changes that are meaningfully different from the current value.
- Preserve the character's existing voice and style while incorporating the behavioral patterns.
- Keep each field's form of address exactly as its definition states, even when the current value gets it wrong elsewhere: manifesto, personality, and system prompts speak TO the character ("You keep your worry behind your teeth"); identity and description speak ABOUT the character from outside ("She finishes other people's sentences"); physical-description sub-fields are bare noun phrases ("auburn hair cut short; grey eyes"). Never flip a field from one form to another while rewording it.
- Do NOT propose brand-new scenarios. Existing scenarios may be refined, but creating new scenarios is out of scope.
- Scenarios describe "where and when" (setting, environment, circumstances). They should not alter the character's personality, voice, or core behavior unless the environment itself demands it."#;

// ============================================================================
// Helpers
// ============================================================================

/// JS `${value}` for a field that may be ABSENT. A missing key renders
/// `undefined`; an explicit `null` renders `null` — `to_js_string` alone cannot
/// tell those apart, and v4's templates can produce either.
fn js_interp(v: Option<&Value>) -> String {
    match v {
        None => "undefined".to_string(),
        Some(x) => to_js_string(x),
    }
}

/// JS `x || '<fallback>'` — the falsy test is JS truthiness, and the truthy
/// branch stringifies whatever it found.
fn js_or(v: Option<&Value>, fallback: &str) -> String {
    if js_truthy(v) {
        to_js_string(v.unwrap())
    } else {
        fallback.to_string()
    }
}

fn arr(v: Option<&Value>) -> Option<&Vec<Value>> {
    v.and_then(Value::as_array)
}

/// v4 `JSON.stringify(analysis, null, 2)` — the two-space pretty form embedded
/// in six prompts. `serde_json`'s pretty printer is the same shape; the key
/// ORDER is the parsed analysis's own (the `preserve_order` rule), which is why
/// the analysis travels as a `Value` rather than a struct.
fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| "null".to_string())
}

/// v4 `coerceSuggestionText` — "Defensively coerce any value into a renderable
/// string. The LLM sometimes 'structures' content that contains `{{user}}` /
/// `{{char}}` template placeholders into a JSON object like
/// `{user: "...", char: "..."}` instead of leaving the literal string alone —
/// rendering that object as a React child crashes the modal."
pub fn coerce_suggestion_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.clone(),
        None | Some(Value::Null) => String::new(),
        Some(v @ Value::Number(_)) | Some(v @ Value::Bool(_)) => to_js_string(v),
        // v4's `try { JSON.stringify(value) } catch { String(value) }`. A
        // `serde_json::Value` cannot hold a cycle or a BigInt, so the catch is
        // unreachable here — recorded, not simulated.
        Some(other) => serde_json::to_string(other).unwrap_or_else(|_| to_js_string(other)),
    }
}

/// v4 `coerceSuggestionArray` — **bug 119's fix, ported from the post-fix
/// shape.** "Defensively coerce a sub-step's parsed JSON into an array of
/// suggestions. The prompts ask for a bare JSON array, but a model periodically
/// answers with a wrapper object (`{"suggestions": [...]}`) or, when it has
/// exactly one amendment to offer, with a single bare suggestion object. Both
/// parse cleanly, so the parse guard never fires — and the array operations
/// that follow then threw a TypeError that aborted the whole optimization run,
/// discarding every suggestion the earlier sub-steps had already produced."
///
/// **The key list is ORDERED and load-bearing** — the FIRST array-valued
/// property wins, so a payload carrying both `suggestions` and `items` takes
/// `suggestions`. Swapping the order reddens the differential's order arm.
pub fn coerce_suggestion_array(value: &Value) -> Vec<Value> {
    if let Value::Array(a) = value {
        return a.clone();
    }
    // v4's `!value || typeof value !== 'object'`: a falsy value or a non-object.
    // `null` fails both halves; an array already returned above.
    let Some(record) = value.as_object() else {
        return Vec::new();
    };
    // A wrapper object: take the first plausibly-named array property.
    for key in ["suggestions", "items", "results", "data", "amendments"] {
        if let Some(Value::Array(a)) = record.get(key) {
            return a.clone();
        }
    }
    // A lone suggestion object, un-arrayed. `field` is the shape's fingerprint.
    if matches!(record.get("field"), Some(Value::String(_))) {
        return vec![value.clone()];
    }
    Vec::new()
}

// ============================================================================
// Prompt builders (wire bytes)
// ============================================================================

/// v4 `buildCharacterContext` — "Build character context string from character
/// data. `wardrobeItems` is the character's current wardrobe (optional —
/// omitted in some tests); it is shown so suggestions can account for what the
/// character already owns and wears."
pub fn build_character_context(character: &Value, wardrobe_items: Option<&[Value]>) -> String {
    let pronoun_text = match character.get("pronouns") {
        Some(p) if js_truthy(Some(p)) => format!(
            "{}/{}/{}",
            js_interp(p.get("subject")),
            js_interp(p.get("object")),
            js_interp(p.get("possessive"))
        ),
        _ => "(not set)".to_string(),
    };

    let aliases = arr(character.get("aliases"));
    let alias_text = match aliases {
        Some(a) if !a.is_empty() => a
            .iter()
            .map(|v| match v {
                Value::Null => String::new(),
                other => to_js_string(other),
            })
            .collect::<Vec<_>>()
            .join(", "),
        _ => "(none)".to_string(),
    };

    let scenarios = arr(character.get("scenarios"));
    let scenario_text = match scenarios {
        Some(s) if !s.is_empty() => s
            .iter()
            .map(|sc| {
                format!(
                    "  - {}: {}",
                    js_interp(sc.get("title")),
                    js_interp(sc.get("content"))
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => "(empty)".to_string(),
    };

    let mut parts: Vec<String> = vec![
        format!("=== Character: {} ===", js_interp(character.get("name"))),
        String::new(),
        format!(
            "Title (private label; read-only): {}",
            js_or(character.get("title"), "(empty)")
        ),
        format!("Pronouns (read-only): {pronoun_text}"),
        format!("Aliases: {alias_text}"),
        String::new(),
        "Identity:".to_string(),
        js_or(character.get("identity"), "(empty)"),
        String::new(),
        "Description:".to_string(),
        js_or(character.get("description"), "(empty)"),
        String::new(),
        "Manifesto:".to_string(),
        js_or(character.get("manifesto"), "(empty)"),
        String::new(),
        "Personality:".to_string(),
        js_or(character.get("personality"), "(empty)"),
        String::new(),
        "Scenarios:".to_string(),
        scenario_text,
        String::new(),
        "First Message:".to_string(),
        js_or(character.get("firstMessage"), "(empty)"),
        String::new(),
        "Example Dialogues:".to_string(),
        js_or(character.get("exampleDialogues"), "(empty)"),
        String::new(),
        format!(
            "Talkativeness: {}",
            js_interp(character.get("talkativeness"))
        ),
    ];

    if let Some(prompts) = arr(character.get("systemPrompts")) {
        if !prompts.is_empty() {
            parts.push(String::new());
            parts.push("=== System Prompts ===".to_string());
            for sp in prompts {
                parts.push(format!(
                    "[System Prompt: \"{}\" (ID: {})]",
                    js_interp(sp.get("name")),
                    js_interp(sp.get("id"))
                ));
                parts.push(js_interp(sp.get("content")));
                parts.push(String::new());
            }
        }
    }

    if js_truthy(character.get("physicalDescription")) {
        let pd = character.get("physicalDescription").unwrap();
        parts.push("=== Physical Description ===".to_string());
        parts.push(format!(
            "[Physical Description: \"{}\" (ID: {})]",
            js_interp(pd.get("name")),
            js_interp(pd.get("id"))
        ));
        parts.push(format!(
            "Head & Shoulders: {}",
            js_or(pd.get("headAndShouldersPrompt"), "(empty)")
        ));
        parts.push(format!(
            "Short: {}",
            js_or(pd.get("shortPrompt"), "(empty)")
        ));
        parts.push(format!(
            "Medium: {}",
            js_or(pd.get("mediumPrompt"), "(empty)")
        ));
        parts.push(format!("Long: {}", js_or(pd.get("longPrompt"), "(empty)")));
        parts.push(format!(
            "Complete: {}",
            js_or(pd.get("completePrompt"), "(empty)")
        ));
        parts.push(format!(
            "Full: {}",
            js_or(pd.get("fullDescription"), "(empty)")
        ));
        parts.push(String::new());
    }

    if let Some(items) = wardrobe_items {
        if !items.is_empty() {
            parts.push("=== Wardrobe ===".to_string());
            for item in items {
                // v4 `if (item.archivedAt) continue;` — JS truthiness, so an
                // explicit null archivedAt does NOT skip the item.
                if js_truthy(item.get("archivedAt")) {
                    continue;
                }
                let mut flags: Vec<String> = Vec::new();
                if js_truthy(item.get("isDefault")) {
                    flags.push("default".to_string());
                }
                if let Some(components) = arr(item.get("componentItemIds")) {
                    if !components.is_empty() {
                        flags.push(format!("composite of {} item(s)", components.len()));
                    }
                }
                let flag_text = if flags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", flags.join("; "))
                };
                parts.push(format!(
                    "[Wardrobe Item: \"{}\" (ID: {})]{}",
                    js_interp(item.get("title")),
                    js_interp(item.get("id")),
                    flag_text
                ));
                parts.push(format!(
                    "  Slots: {}",
                    match arr(item.get("types")) {
                        Some(t) => t
                            .iter()
                            .map(|v| match v {
                                Value::Null => String::new(),
                                other => to_js_string(other),
                            })
                            .collect::<Vec<_>>()
                            .join(", "),
                        // `undefined.join` would THROW in v4; every real row
                        // carries `types`. Recorded: the fixture never omits it.
                        None => "undefined".to_string(),
                    }
                ));
                if js_truthy(item.get("appropriateness")) {
                    parts.push(format!(
                        "  Appropriateness: {}",
                        js_interp(item.get("appropriateness"))
                    ));
                }
                parts.push(format!(
                    "  Description: {}",
                    js_or(item.get("description"), "(empty)")
                ));
            }
            parts.push(String::new());
        }
    }

    parts.join("\n")
}

/// v4 `buildMemoryContext` — "Build memory context string from ranked
/// memories". Each entry is v4's `{ memory }` wrapper.
pub fn build_memory_context(memories: &[Value]) -> String {
    let mut parts: Vec<String> = vec![format!(
        "=== Reinforced Memories (top {}) ===",
        memories.len()
    )];
    for (i, entry) in memories.iter().enumerate() {
        let memory = entry.get("memory");
        parts.push(format!(
            "[Memory #{}] (reinforced {} times): {}",
            i + 1,
            js_interp(memory.and_then(|m| m.get("reinforcementCount"))),
            js_interp(memory.and_then(|m| m.get("content")))
        ));
    }
    parts.join("\n")
}

/// v4 `getAnalysisPrompt` (byte-exact).
pub fn get_analysis_prompt() -> String {
    format!(
        r#"{FULL_FIELD_SEMANTICS}

Analyze this character's configuration alongside their most-reinforced memories. Identify 3-8 behavioral patterns that are established in the memories but not fully captured in the character's current configuration.

For every pattern you identify, decide which of the three editable fields (IDENTITY, DESCRIPTION, PERSONALITY) it is evidence for, using the vantage-point rule above. Patterns that demonstrate behavior visible to interlocutors → DESCRIPTION. Patterns that reveal the character's self-knowledge or inner drivers → PERSONALITY. Public-knowledge facts strangers could know on sight → IDENTITY. Patterns that don't fit any of these (e.g. environment) belong to scenarios and should still be surfaced.

Look for:
- Speech habits and verbal patterns (DESCRIPTION)
- Emotional tendencies and inner drivers (PERSONALITY)
- Relationship dynamics — outward (DESCRIPTION) vs. inward attitude (PERSONALITY)
- Behavioural quirks or consistent actions (DESCRIPTION)
- Self-knowledge, motivations, beliefs the character privately holds (PERSONALITY)
- Public-facing facts: station, occupation, reputation that strangers know on sight (IDENTITY)
- Recurring settings or environments that might warrant refining an existing scenario (remember: a scenario describes the setting/environment of a chat, not a change in the character's personality)
- Concrete physical/appearance details the memories establish — scars, hair, height (these inform the physical description, not behaviour)
- Habitual garments, outfits, or accessories the memories establish — a signature coat, a locket always worn (these inform the WARDROBE, never the physical description)
- Nicknames or alternate names other characters repeatedly use for this character (these inform the ALIASES property)

Respond with JSON:
{{
  "behavioralPatterns": [
    {{
      "pattern": "Brief description of the behavioral pattern",
      "evidence": "Specific examples from the memories that demonstrate this pattern",
      "frequency": "How often this appears across the memories"
    }}
  ],
  "summary": "A 2-3 sentence overview of how the character has evolved through their interactions, highlighting the gap between their current configuration and their demonstrated behavior."
}}"#
    )
}

/// v4 `getGeneralFieldsSuggestionsPrompt` (byte-exact).
pub fn get_general_fields_suggestions_prompt(analysis: &Value) -> String {
    format!(
        r#"{FIELD_SEMANTICS_PREAMBLE}

Based on the behavioral analysis below and the character's current configuration, propose targeted modifications to the character's GENERAL fields only:

  - identity (public-knowledge / outside-view facts only — name, station, occupation, reputation)
  - description (behavior, mannerisms, verbal patterns visible to interlocutors)
  - manifesto (the basic tenets, the axiomatic core; not a vantage-point field, it is the load-bearing truth the character is built on)
  - personality (the character's own self-knowledge; inner drivers of speech and behavior)
  - exampleDialogues
  - talkativeness (a number between 0.1 and 1.0)

The vantage-point rule is strict:
- A suggestion for IDENTITY may only contain facts a stranger could plausibly know without having spoken to the character. Never put internal motivation, private mannerisms, or self-knowledge here.
- A suggestion for DESCRIPTION must reflect things someone who has interacted with the character would notice — not the character's own internal monologue, and not surface-level public reputation.
- A suggestion for MANIFESTO should be rare and high-stakes — propose manifesto changes only when the memory contradicts a basic tenet, not for tonal or stylistic improvements. Manifesto edits reverberate across every other field.
- A suggestion for PERSONALITY must reflect the character's own self-knowledge and inner drivers. Never put outward behavior someone else would observe here, and never put public-facing identity facts.
- Do NOT propose the same content under two different fields. Pick the one whose vantage point matches.
- Do NOT suggest edits to title, scenarios, system prompts, the physical description, the wardrobe, or aliases in this response — those are out of scope for this pass (title is never editable here; the rest are each handled by their own dedicated passes).

If you see nothing worth changing in the general fields, respond with an empty JSON array.

=== Behavioural Analysis ===
{analysis_json}

{SUGGESTION_SCHEMA_PREAMBLE}

Respond with a JSON array of suggestion objects."#,
        analysis_json = pretty(analysis)
    )
}

/// v4 `getScenarioSuggestionPrompt` (byte-exact).
pub fn get_scenario_suggestion_prompt(analysis: &Value, scenario: &Value) -> String {
    let id = js_interp(scenario.get("id"));
    format!(
        r#"Focus solely on the following scenario. Decide whether its content should be refined to better reflect the demonstrated behavior below. A scenario describes the environment, circumstances, and context of a chat — it is the stage, not the actor. Refinements should sharpen the setting (place, circumstance, atmosphere, starting situation), not rewrite the character's personality.

=== Scenario Under Review ===
ID: {id}
Title: {title}
Current content:
{content}

=== Behavioural Analysis ===
{analysis_json}

Produce at most ONE suggestion. If the current scenario is already an appropriate setting for the patterns observed, respond with an empty JSON array.

{SUGGESTION_SCHEMA_PREAMBLE}

Additional rules specific to scenario refinement:
- Set field="scenarios" and subId="{id}".
- currentValue must be the existing scenario content verbatim.
- proposedValue must be a complete replacement for the scenario content.

Respond with a JSON array of at most one suggestion."#,
        title = js_interp(scenario.get("title")),
        content = js_or(scenario.get("content"), "(empty)"),
        analysis_json = pretty(analysis)
    )
}

/// v4 `getSystemPromptSuggestionPrompt` (byte-exact).
pub fn get_system_prompt_suggestion_prompt(analysis: &Value, prompt: &Value) -> String {
    let id = js_interp(prompt.get("id"));
    format!(
        r#"Focus solely on the following system prompt. Decide whether its text should be refined to better reflect the demonstrated behavior below, while preserving the interaction style the prompt is clearly trying to achieve.

=== System Prompt Under Review ===
ID: {id}
Name: {name}
Is default variant: {is_default}
Current content:
{content}

=== Behavioural Analysis ===
{analysis_json}

Produce at most ONE suggestion. If the current prompt already captures the patterns you would want to reinforce, respond with an empty JSON array.

{SUGGESTION_SCHEMA_PREAMBLE}

Additional rules specific to system-prompt refinement:
- Set field="systemPrompt" and subId="{id}".
- currentValue must be the existing prompt content verbatim.
- proposedValue must be a complete replacement for the prompt content.
- Do NOT change the prompt's evident interaction style (e.g. a "terse" prompt should stay terse); only sharpen its articulation of the character.

Respond with a JSON array of at most one suggestion."#,
        name = js_interp(prompt.get("name")),
        is_default = if js_truthy(prompt.get("isDefault")) {
            "yes"
        } else {
            "no"
        },
        content = js_or(prompt.get("content"), "(empty)"),
        analysis_json = pretty(analysis)
    )
}

/// v4 `getPhysicalDescriptionSuggestionPrompt` (byte-exact). `physical` is
/// v4's `Character['physicalDescription'] | null`.
pub fn get_physical_description_suggestion_prompt(
    analysis: &Value,
    physical: Option<&Value>,
) -> String {
    // v4 `const pd = physical ?? null` then `pd ? […] : '(…)'` — a JS TRUTHY
    // test, so an explicit `null` and an absent value both take the fallback.
    let current = match physical {
        Some(pd) if js_truthy(Some(pd)) => [
            format!("Name: {}", js_interp(pd.get("name"))),
            format!(
                "fullDescription: {}",
                js_or(pd.get("fullDescription"), "(empty)")
            ),
            format!(
                "headAndShouldersPrompt: {}",
                js_or(pd.get("headAndShouldersPrompt"), "(empty)")
            ),
            format!("shortPrompt: {}", js_or(pd.get("shortPrompt"), "(empty)")),
            format!("mediumPrompt: {}", js_or(pd.get("mediumPrompt"), "(empty)")),
            format!("longPrompt: {}", js_or(pd.get("longPrompt"), "(empty)")),
            format!(
                "completePrompt: {}",
                js_or(pd.get("completePrompt"), "(empty)")
            ),
        ]
        .join("\n"),
        _ => "(this character has no physical description yet)".to_string(),
    };

    format!(
        r#"Focus solely on the character's PHYSICAL DESCRIPTION — their appearance. Decide whether any of its sub-fields should be refined to reflect concrete appearance details established in the memories. This is about how the character LOOKS, not how they behave; ignore behavioural patterns unless they imply a visible, physical trait (a habitual posture, a recurring article of dress, an acquired scar).

The physical description has these sub-fields:
- fullDescription — prose appearance description (the physical-description document)
- headAndShouldersPrompt — a tight head-and-shoulders portrait prompt for avatars: face, hair, expression, neckline and visible upper attire ONLY; never breasts, torso, waist, hips, legs, or any anatomy below the shoulders
- shortPrompt — a brief image-generation prompt (a few words / phrases)
- mediumPrompt — a moderately detailed image-generation prompt
- longPrompt — a detailed image-generation prompt
- completePrompt — the most complete image-generation prompt

=== Current Physical Description ===
{current}

=== Behavioural Analysis ===
{analysis_json}

Produce at most ONE suggestion per sub-field, and only for sub-fields the memories genuinely speak to. If the memories reveal nothing about the character's appearance, respond with an empty JSON array — this is the common case.

{SUGGESTION_SCHEMA_PREAMBLE}

Additional rules specific to physical-description refinement:
- Set field="physicalDescription".
- Set subId to the exact sub-field key being changed: one of fullDescription, headAndShouldersPrompt, shortPrompt, mediumPrompt, longPrompt, completePrompt.
- Set subName to a human label for that sub-field (e.g. "Full Description", "Head & Shoulders", "Short Prompt", "Medium Prompt", "Long Prompt", "Complete Prompt").
- currentValue must be the existing text of that sub-field (empty string if it has none).
- proposedValue must be the complete new text for that sub-field.
- Keep the image prompts (headAndShoulders/short/medium/long/complete) in the comma-or-phrase style image models expect; keep fullDescription in prose.
- For headAndShouldersPrompt specifically: describe ONLY what a head-and-shoulders crop shows (face, hair, expression, neckline, visible upper attire). Never describe breasts, torso, waist, hips, legs, or any anatomy below the shoulders.

Respond with a JSON array of suggestion objects (may be empty)."#,
        analysis_json = pretty(analysis)
    )
}

/// v4 `getWardrobeSuggestionPrompt` (byte-exact).
pub fn get_wardrobe_suggestion_prompt(analysis: &Value, wardrobe_items: &[Value]) -> String {
    // v4 `wardrobeItems.filter((item) => !item.archivedAt)` — JS truthiness.
    let active_count = wardrobe_items
        .iter()
        .filter(|item| !js_truthy(item.get("archivedAt")))
        .count();
    format!(
        r#"{WARDROBE_SEMANTICS}

Focus solely on the character's WARDROBE (shown in the character context above). Decide whether the memories establish anything about what the character habitually wears that the wardrobe does not yet capture. This is about removable things — clothing, outfits, accessories, and hairstyles. A bodily feature (scar, tattoo, fur) belongs to the physical description and must NEVER become a wardrobe item — with one deliberate exception: a hairSTYLE goes in the wardrobe's "hair" slot, while the hair's natural colour, length, and texture stay in the physical description.

Two kinds of suggestions are allowed:

1. REFINE an existing item's description. Set field="wardrobeItems" and subId to the item's ID (from the character context), subName to its title, currentValue to its existing description verbatim, and proposedValue to the complete new description.
2. ADD a new item the memories genuinely establish — a signature garment or accessory that appears repeatedly. Set field="wardrobeItems", omit subId, set name to the item's title, currentValue to the empty string, proposedValue to a one-or-two-sentence human-readable summary of the item, and include a "wardrobeItem" object:
   "wardrobeItem": {{
     "title": "Short descriptive name",
     "description": "A sentence or two describing the item's appearance",
     "imagePrompt": "Terse literal visual cue for image generation (optional)",
     "types": ["top"],
     "appropriateness": "casual, everyday",
     "isDefault": false
   }}
   Valid slot types: "top", "bottom", "footwear", "accessories", "hair"; a single garment may cover several slots (a dress is ["top","bottom"]; a braided updo is ["hair"]).

The character currently has {active_count} wardrobe item(s). Only propose what the memories genuinely support; if they reveal nothing about the character's clothing or accessories, respond with an empty JSON array — this is the common case. Never propose deleting items.

=== Behavioural Analysis ===
{analysis_json}

{SUGGESTION_SCHEMA_PREAMBLE}

Respond with a JSON array of suggestion objects (may be empty)."#,
        analysis_json = pretty(analysis)
    )
}

/// v4 `getPropertiesSuggestionPrompt` (byte-exact).
pub fn get_properties_suggestion_prompt(analysis: &Value, character: &Value) -> String {
    // v4 `character.aliases ?? []` — NULLISH, not falsy.
    let aliases: Vec<Value> = match character.get("aliases") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a.clone(),
        // A non-array, non-nullish `aliases` survives `??` and then `.length` /
        // `.join` are read off it. No real row can be here; recorded.
        Some(_) => Vec::new(),
    };
    let alias_line = if !aliases.is_empty() {
        aliases
            .iter()
            .map(|v| match v {
                Value::Null => String::new(),
                other => to_js_string(other),
            })
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        "(none)".to_string()
    };
    format!(
        r#"{PROPERTIES_SEMANTICS}

Focus solely on the character's ALIASES. Current aliases: {alias_line}.

Decide whether the memories establish nicknames or alternate names that other characters repeatedly use for this character and that are missing from the alias list. Propose at most 3 additions — one suggestion per alias:
- Set field="aliases".
- currentValue is the current alias list joined with commas (or the empty string when there are none).
- proposedValue is the single new alias to add, exactly as others use it.
- Do NOT propose removing or renaming existing aliases, do NOT propose the character's primary name or their title/epithet, and do NOT propose one-off forms of address that only appeared once.
- Pronouns are read-only context for you — never propose pronoun changes.

If the memories establish no new aliases, respond with an empty JSON array — this is the common case.

=== Behavioural Analysis ===
{analysis_json}

{SUGGESTION_SCHEMA_PREAMBLE}

Respond with a JSON array of suggestion objects (may be empty)."#,
        analysis_json = pretty(analysis)
    )
}

/// v4 `getNewSystemPromptsSuggestionPrompt` (byte-exact).
pub fn get_new_system_prompts_suggestion_prompt(analysis: &Value) -> String {
    format!(
        r#"Review the character's existing system prompts (shown in the character context). Propose any NEW system prompts that are warranted by the behavioral patterns below but aren't already covered by the existing set. Do NOT propose edits to existing prompts here — this pass handles additions only. Do NOT propose new scenarios. If no new system prompt is warranted, respond with an empty JSON array.

=== Behavioural Analysis ===
{analysis_json}

{SUGGESTION_SCHEMA_PREAMBLE}

Additional rules specific to this pass:
- For each new system prompt: field="systemPrompt", omit subId, include a "name" field with a short descriptive label, and put the complete prompt text in proposedValue. currentValue should be the empty string.
- Be conservative: only propose a new prompt if there is a clear interaction style the existing set does not cover.

Respond with a JSON array of suggestion objects (may be empty)."#,
        analysis_json = pretty(analysis)
    )
}

// ============================================================================
// The runner (v4 `runCharacterOptimizer` — P4.9K1 unit 4)
// ============================================================================
//
// The stateful half of the same v4 file: the memory pipeline (search → date
// filter → rank → reinforcement filter → limit), the analysis call, the
// per-sub-step passes with bug 119's containment, the suggestions-file writer.
// The pure half above supplies every prompt; this half supplies the order,
// the seams, and the events.
//
// ## Seams (mocked identically on both sides of the tier-3 differential)
//
// * The model boundary — a [`CompletionProvider`] (v4 `createLLMProvider(...)
//   .sendMessage`), canned by the exact call on both sides.
// * The embedding boundary — an [`EmbeddingProvider`] (v4
//   `generateEmbeddingForUser`), canned by the exact query text.
// * The clock — `now_ms` is a parameter (v4's `Date` is frozen by the oracle):
//   it feeds the memory-weight decay, the suggestions-file stamp, and nothing
//   else that is compared (the `Date.now()` durations reach only `llm_logs`).
// * Progress — `on_progress` is v4's `onProgress` callback; the engine hands
//   in a closure that publishes through the K0
//   [`GeneratorProgressEmitter`](crate::services::generator_progress) AND
//   remembers the last event for the `{ terminal }` dispatch payload.
//
// ## Log lines
//
// v4's `[CharacterOptimizer] …` lines are emitted with the SAME message bytes
// and their whole context bag as ONE `context=<json>` field (target
// `quilltap::character_optimizer`), which is how the differential compares
// them against the oracle's captured `logger` calls — the two bug-119
// containment lines among them (§R.12).
//
// ## Recorded divergences
//
// * A JSON parse failure's `error` text is V8's `JSON.parse` wording in v4.
//   [`v8_json_parse_message`] reproduces the measured "unexpected token" and
//   "unexpected end" arms (the shapes a prose answer produces); every other
//   arm (a malformed token INSIDE a value, an unterminated string, trailing
//   garbage after a valid value) falls back to serde's own message.
// * `memory.createdAt` that fails to parse ranks as JS `NaN` (dropped by the
//   weight threshold) — modelled by dropping the row; the repos never mint
//   such a value.

/// The `llm_logs.type` for both optimizer calls (v4 `type: 'CHARACTER_OPTIMIZER'`).
pub const LOG_TYPE_CHARACTER_OPTIMIZER: &str = "CHARACTER_OPTIMIZER";

/// The tracing target every `[CharacterOptimizer]` line is emitted under.
pub const OPTIMIZER_LOG_TARGET: &str = "quilltap::character_optimizer";

/// v4 `OptimizerOutputMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptimizerOutputMode {
    Apply,
    SuggestionsFile,
}

impl OptimizerOutputMode {
    /// The wire / schema spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            OptimizerOutputMode::Apply => "apply",
            OptimizerOutputMode::SuggestionsFile => "suggestions-file",
        }
    }
}

/// v4 `OptimizerOptions`, POST-defaults (the route's `optimizeStreamSchema`
/// materializes every default, so the runner sees concrete values).
#[derive(Clone, Debug, PartialEq)]
pub struct OptimizerOptions {
    pub max_memories: i64,
    pub search_query: String,
    pub use_semantic_search: bool,
    pub since_date: Option<String>,
    pub before_date: Option<String>,
    pub output_mode: OptimizerOutputMode,
}

impl Default for OptimizerOptions {
    /// The schema defaults: `maxMemories` 30, `searchQuery` `''`,
    /// `useSemanticSearch` true, both dates `null`, `outputMode` `'apply'`.
    fn default() -> Self {
        OptimizerOptions {
            max_memories: MAX_MEMORIES_FOR_ANALYSIS,
            search_query: String::new(),
            use_semantic_search: true,
            since_date: None,
            before_date: None,
            output_mode: OptimizerOutputMode::Apply,
        }
    }
}

/// v4's `onProgress` callback shape.
pub type OnProgress<'a> = &'a mut (dyn FnMut(Value) + Send);

// ---------------------------------------------------------------------------
// JS-shaped helpers
// ---------------------------------------------------------------------------

/// The first `n` UTF-16 units of `s` (JS `s.substring(0, n)`).
fn utf16_prefix(s: &str, n: usize) -> String {
    String::from_utf16_lossy(&s.encode_utf16().take(n).collect::<Vec<u16>>())
}

/// The last `n` UTF-16 units of `s` (JS `s.slice(-n)`).
fn utf16_tail(s: &str, n: usize) -> String {
    let len = utf16_len(s);
    crate::jsstr::utf16_slice_from(s, len.saturating_sub(n))
}

/// JS `typeof` over a parsed JSON value (`null` is `'object'`; an array is
/// excluded by the caller, which branches on `Array.isArray` first).
fn js_typeof(v: &Value) -> &'static str {
    match v {
        Value::Null | Value::Object(_) | Value::Array(_) => "object",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
    }
}

/// V8's `JSON.parse` failure wording for the arms this port reproduces
/// (measured on Node 24.13.1 at the `f699da6f6` pin, `json-parser.cc`'s
/// `GetErrorMessageWithEllipses` rule): whitespace-only input →
/// `Unexpected end of JSON input`; a first non-whitespace character that
/// cannot start a JSON value → `Unexpected token '<c>', <context> is not valid
/// JSON`, where the context is the whole source when it is shorter than 21
/// UTF-16 units, else a 10-unit window on the side(s) of the position with
/// `...` marking the elided side. A `t`/`f`/`n` start is scanned against its
/// keyword as V8's `ScanLiteral` does — the first mismatching unit is the
/// token, at its own position (`no json here` → `'o'`); a source ending
/// inside the keyword is `Unexpected end of JSON input`; a whole keyword
/// followed by anything but whitespace is `Unexpected non-whitespace
/// character after JSON at position N (line L column C)`. `None` for every
/// other shape (a failure INSIDE a value that starts legally) — the caller
/// falls back to serde's message, a RECORDED divergence.
pub fn v8_json_parse_message(text: &str) -> Option<String> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let is_ws = |u: u16| matches!(u, 0x20 | 0x09 | 0x0a | 0x0d);
    let Some(pos) = units.iter().position(|u| !is_ws(*u)) else {
        return Some("Unexpected end of JSON input".to_string());
    };
    let mut pos = pos;
    let c = units[pos];
    // A literal start is scanned against its keyword (V8 `ScanLiteral`): the
    // first mismatching unit is the reported token at ITS position; a source
    // that ends inside the keyword is `Unexpected end of JSON input`; a whole
    // keyword followed by a non-whitespace unit is the after-JSON arm.
    let keyword = match c {
        0x74 => Some("true"),
        0x66 => Some("false"),
        0x6e => Some("null"),
        _ => None,
    };
    if let Some(keyword) = keyword {
        let kw: Vec<u16> = keyword.encode_utf16().collect();
        let mut mismatch = None;
        for (j, k) in kw.iter().enumerate().skip(1) {
            match units.get(pos + j) {
                None => return Some("Unexpected end of JSON input".to_string()),
                Some(u) if u != k => {
                    mismatch = Some(pos + j);
                    break;
                }
                Some(_) => {}
            }
        }
        match mismatch {
            Some(at) => pos = at,
            None => {
                let end = pos + kw.len();
                let Some(off) = units[end..].iter().position(|u| !is_ws(*u)) else {
                    return None; // a valid literal — no failure to word
                };
                let at = end + off;
                let line = 1 + units[..at].iter().filter(|u| **u == 0x0a).count();
                let line_start = units[..at]
                    .iter()
                    .rposition(|u| *u == 0x0a)
                    .map_or(0, |i| i + 1);
                let column = at - line_start + 1;
                return Some(format!(
                    "Unexpected non-whitespace character after JSON at position {at} (line {line} column {column})"
                ));
            }
        }
    } else {
        let starts_value = matches!(c, 0x7b | 0x5b | 0x22 | 0x2d) // { [ " -
            || (0x30..=0x39).contains(&c);
        if starts_value {
            return None;
        }
    }
    const K: usize = 10;
    let len = units.len();
    let sub = |a: usize, b: usize| String::from_utf16_lossy(&units[a..b]);
    let token = sub(pos, pos + 1);
    let message = if len < 2 * K + 1 {
        format!(
            "Unexpected token '{token}', \"{}\" is not valid JSON",
            sub(0, len)
        )
    } else if pos < K {
        format!(
            "Unexpected token '{token}', \"{}\"... is not valid JSON",
            sub(0, pos + K)
        )
    } else if pos >= len - K {
        format!(
            "Unexpected token '{token}', ...\"{}\" is not valid JSON",
            sub(pos - K, len)
        )
    } else {
        format!(
            "Unexpected token '{token}', ...\"{}\"... is not valid JSON",
            sub(pos - K, pos + K)
        )
    };
    Some(message)
}

/// The message v4's `parseLLMJson` throws for `raw`: the LAST parse in its
/// chain is over the repaired text, and that `SyntaxError` is what propagates
/// — so the V8 wording is computed over the same repaired bytes.
pub fn llm_json_failure_message(raw: &str, err: &LlmJsonError) -> String {
    let repaired = repair_truncated_json(&escape_control_chars_in_strings(&strip_code_fences(raw)));
    v8_json_parse_message(&repaired).unwrap_or_else(|| err.message.clone())
}

/// `new Date(`${d}T00:00:00.000Z`).getTime()` — `None` is JS `NaN`.
fn day_boundary_ms(d: &str) -> Option<i64> {
    iso_to_ms(&format!("{d}T00:00:00.000Z"))
}

/// `new Date(m.createdAt).getTime()` — `None` is JS `NaN`.
fn created_at_ms(m: &Value) -> Option<i64> {
    m.get("createdAt")
        .and_then(Value::as_str)
        .and_then(iso_to_ms)
}

/// The weighting inputs of a memory row (`None` when `createdAt` or
/// `importance` is unusable — the JS `NaN` weight, dropped by the threshold).
fn memory_inputs(m: &Value) -> Option<MemoryInputs> {
    let ms = |key: &str| {
        m.get(key)
            .and_then(Value::as_str)
            .and_then(iso_to_ms)
            .map(|x| x as f64)
    };
    Some(MemoryInputs {
        importance: m.get("importance").and_then(Value::as_f64)?,
        reinforced_importance: m.get("reinforcedImportance").and_then(Value::as_f64),
        created_at_ms: created_at_ms(m)? as f64,
        last_reinforced_at_ms: ms("lastReinforcedAt"),
        last_accessed_at_ms: ms("lastAccessedAt"),
        reinforcement_count: m.get("reinforcementCount").and_then(Value::as_u64),
        graph_degree: m
            .get("relatedMemoryIds")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
        kind_episodic: m.get("kind").and_then(Value::as_str) == Some("episodic"),
        occurred_at_ms: crate::episodic::event_time_ms(m.get("occurredAt").and_then(Value::as_str)),
    })
}

/// v4 `rankMemoriesByWeight(memories)` with the default config: weight every
/// row at `now_ms`, drop those under `minWeightThreshold`, STABLE-sort
/// descending by effective weight.
pub fn rank_memories_by_weight(memories: Vec<Value>, now_ms: i64) -> Vec<Value> {
    let mut weighted: Vec<(Value, f64)> = memories
        .into_iter()
        .filter_map(|m| {
            let inputs = memory_inputs(&m)?;
            let w = calculate_effective_weight(&inputs, &DEFAULT_WEIGHTING_CONFIG, now_ms as f64)
                .effective_weight;
            Some((m, w))
        })
        .filter(|(_, w)| *w >= DEFAULT_WEIGHTING_CONFIG.min_weight_threshold)
        .collect();
    weighted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    weighted.into_iter().map(|(m, _)| m).collect()
}

/// `memory.reinforcementCount >= MIN_REINFORCED_MEMORIES` (JS: a missing or
/// null count compares false).
fn is_reinforced(m: &Value) -> bool {
    m.get("reinforcementCount")
        .and_then(Value::as_f64)
        .is_some_and(|n| n >= MIN_REINFORCED_MEMORIES as f64)
}

fn ctx(v: Value) -> String {
    serde_json::to_string(&v).unwrap_or_default()
}

fn str_of(v: &Value, key: &str) -> String {
    v.get(key).map(to_js_string).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// The model call (v4 `callOptimizerLLM`)
// ---------------------------------------------------------------------------

/// Everything a sub-step call needs that does not change between sub-steps.
struct CallCtx<'a, CMP: CompletionProvider> {
    completion: &'a CMP,
    provider: String,
    base_url: Option<String>,
    model_name: String,
    profile_id: String,
    profile_parameters: Option<Value>,
    character_id: &'a str,
    user_id: &'a str,
    character_context: String,
    memory_context: String,
}

/// v4 `callOptimizerLLM` — the two-message request; `Err` carries the thrown
/// message (a provider failure, or `No response from model` for an empty
/// answer). `Ok` is the TRIMMED content plus the measured duration.
async fn call_optimizer_llm<CMP: CompletionProvider>(
    c: &CallCtx<'_, CMP>,
    instruction: &str,
    temperature: f64,
    max_tokens: i64,
) -> Result<(String, i64), String> {
    let messages = vec![
        CompletionMessage::system(SYSTEM_MESSAGE),
        CompletionMessage::user(format!(
            "{}\n\n---\n\n{}\n\n---\n\n{}",
            c.character_context, c.memory_context, instruction
        )),
    ];
    let params = CompletionParams {
        messages,
        model: c.model_name.clone(),
        temperature: Some(temperature),
        max_tokens: Some(max_tokens),
        strict_max_tokens: false,
        top_p: None,
        cache_key: build_character_cache_key(Some(c.character_id)),
        profile_parameters: c.profile_parameters.clone(),
        attachments: Vec::new(),
        request_timeout_ms: None,
    };
    let start_ms = crate::clock::now_unix_ms();
    let response: CompletionResponse = c
        .completion
        .send_message(&c.provider, c.base_url.as_deref(), &params)
        .await
        .map_err(|e| e.message)?;
    let duration_ms = crate::clock::now_unix_ms() - start_ms;
    // v4 `if (!response?.content)` — JS falsy: an empty answer is a throw.
    if response.content.is_empty() {
        return Err("No response from model".to_string());
    }
    Ok((js_trim(&response.content).to_string(), duration_ms))
}

/// The `llm_logs` row both optimizer calls write (v4 `logLLMCall({... type:
/// 'CHARACTER_OPTIMIZER' ...})`), best-effort. v4 chains a `.catch` that
/// warns `Failed to log analysis LLM call` / `Failed to log sub-step LLM
/// call`; v5's `log_llm_call` never throws (it swallows and answers `None`),
/// so those two warn lines have no arm to fire from here — recorded, not
/// simulated.
async fn log_optimizer_call<CMP: CompletionProvider>(
    db: &Db,
    c: &CallCtx<'_, CMP>,
    user_placeholder: &str,
    temperature: f64,
    max_tokens: i64,
    raw: &str,
    duration_ms: i64,
) {
    let _ = log_llm_call(
        db,
        LogLlmCallParams {
            user_id: c.user_id.to_string(),
            log_type: LOG_TYPE_CHARACTER_OPTIMIZER.to_string(),
            message_id: None,
            chat_id: None,
            character_id: Some(c.character_id.to_string()),
            provider: c.provider.clone(),
            model_name: c.model_name.clone(),
            connection_profile_id: Some(c.profile_id.clone()),
            image_profile_id: None,
            request: LogRequest {
                messages: vec![
                    LogRequestMessage {
                        role: "system".to_string(),
                        content: SYSTEM_MESSAGE.to_string(),
                        attachments: None,
                    },
                    LogRequestMessage {
                        role: "user".to_string(),
                        content: user_placeholder.to_string(),
                        attachments: None,
                    },
                ],
                temperature: Some(temperature),
                max_tokens: Some(max_tokens),
                tools: None,
            },
            response: LogResponse {
                content: utf16_prefix(raw, 500),
                error: None,
                finish_reason: None,
                tool_calls: None,
            },
            usage: None,
            cache_usage: None,
            raw_provider_usage: None,
            request_hashes: None,
            duration_ms: Some(duration_ms as f64),
        },
        &LogContext::none(),
    )
    .await;
}

// ---------------------------------------------------------------------------
// The sub-step pass (v4 `runSubStepCore` / `runSubStep`)
// ---------------------------------------------------------------------------

/// The per-run sub-step state v4 closes over.
struct SubStepState {
    index: usize,
    total: usize,
    all_suggestions: Vec<Value>,
}

/// v4's `{...s, id, currentValue, proposedValue, rationale, memoryExcerpts,
/// wardrobeItem}` — a spread keeps an existing key's POSITION with the new
/// value, appends the keys `s` lacked in literal order, and a key whose new
/// value is `undefined` is omitted by `JSON.stringify` (the `wardrobeItem:
/// undefined` arm).
fn finish_suggestion(s: &serde_json::Map<String, Value>, id: String) -> Value {
    let excerpts = match s.get("memoryExcerpts") {
        Some(Value::Array(items)) => Value::Array(
            items
                .iter()
                .map(|x| Value::String(coerce_suggestion_text(Some(x))))
                .collect(),
        ),
        _ => Value::Array(Vec::new()),
    };
    // A brand-new wardrobe item must carry a valid structured payload;
    // sanitize it (slot types, coerced flags) and let the filter below drop
    // the suggestion if nothing survives.
    let is_new_wardrobe_item = s.get("field").and_then(Value::as_str) == Some("wardrobeItems")
        && !js_truthy(s.get("subId"));
    let wardrobe_item: Option<Value> = if is_new_wardrobe_item && js_truthy(s.get("wardrobeItem")) {
        sanitize_generated_wardrobe_items(&Value::Array(vec![s["wardrobeItem"].clone()]))
            .into_iter()
            .next()
            .and_then(|item| serde_json::to_value(item).ok())
    } else {
        None
    };
    let overrides: [(&str, Option<Value>); 6] = [
        ("id", Some(Value::String(id))),
        (
            "currentValue",
            Some(Value::String(coerce_suggestion_text(s.get("currentValue")))),
        ),
        (
            "proposedValue",
            Some(Value::String(coerce_suggestion_text(
                s.get("proposedValue"),
            ))),
        ),
        (
            "rationale",
            Some(Value::String(coerce_suggestion_text(s.get("rationale")))),
        ),
        ("memoryExcerpts", Some(excerpts)),
        ("wardrobeItem", wardrobe_item),
    ];
    let mut out = serde_json::Map::new();
    for (k, v) in s {
        match overrides.iter().find(|(ok, _)| ok == k) {
            Some((_, Some(nv))) => {
                out.insert(k.clone(), nv.clone());
            }
            Some((_, None)) => {} // `undefined` — omitted by JSON.stringify
            None => {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    for (k, v) in overrides {
        if !s.contains_key(k) {
            if let Some(v) = v {
                out.insert(k.to_string(), v);
            }
        }
    }
    Value::Object(out)
}

/// v4 `runSubStepCore`: one focused pass — the `substep_start` frame, the
/// model call (a failure is CONTAINED: the warn + an empty `substep_complete`),
/// the parse with bug 119's `coerceSuggestionArray` (a non-array answer is the
/// coerced warn; unparseable JSON is the skipping warn), the significance
/// filter + the defensive text coercions + the new-wardrobe-item sanitize,
/// the `llm_logs` row, the `substep_complete` frame.
///
/// `Err` is v4's "unexpected throw" — every fallible step above is caught
/// INSIDE this function exactly as v4 catches it, so today no arm reaches the
/// outer containment; the shape is kept because bug 119 is the reason it
/// exists, and [`contain_sub_step_outcome`] is pinned with a synthetic
/// failure.
#[allow(clippy::too_many_arguments)]
async fn run_sub_step_core<CMP: CompletionProvider>(
    db: &Db,
    c: &CallCtx<'_, CMP>,
    state: &mut SubStepState,
    kind: &str,
    label: &str,
    instruction: &str,
    on_progress: OnProgress<'_>,
) -> Result<(), String> {
    state.index += 1;
    let sub_step = json!({
        "kind": kind,
        "label": label,
        "index": state.index,
        "total": state.total,
    });
    on_progress(json!({"type": "substep_start", "step": "generating", "subStep": sub_step}));

    let (raw, sub_step_duration_ms) = match call_optimizer_llm(c, instruction, 0.7, 6000).await {
        Ok(r) => r,
        Err(error) => {
            tracing::warn!(
                target: OPTIMIZER_LOG_TARGET,
                context = %ctx(json!({"characterId": c.character_id, "subStep": label, "error": error})),
                "[CharacterOptimizer] Sub-step LLM call failed; continuing"
            );
            on_progress(json!({
                "type": "substep_complete",
                "step": "generating",
                "subStep": sub_step,
                "partialSuggestions": [],
            }));
            return Ok(());
        }
    };

    let parsed: Vec<Value> = match parse_llm_json(&raw) {
        Ok(raw_parsed) => {
            let parsed = coerce_suggestion_array(&raw_parsed);
            if !raw_parsed.is_array() {
                tracing::warn!(
                    target: OPTIMIZER_LOG_TARGET,
                    context = %ctx(json!({
                        "characterId": c.character_id,
                        "subStep": label,
                        "parsedType": js_typeof(&raw_parsed),
                        "recovered": parsed.len(),
                    })),
                    "[CharacterOptimizer] Sub-step answered with a non-array; coerced"
                );
            }
            parsed
        }
        Err(parse_error) => {
            tracing::warn!(
                target: OPTIMIZER_LOG_TARGET,
                context = %ctx(json!({
                    "characterId": c.character_id,
                    "subStep": label,
                    "rawTail": utf16_tail(&raw, 200),
                    "error": llm_json_failure_message(&raw, &parse_error),
                })),
                "[CharacterOptimizer] Sub-step produced unparseable JSON; skipping"
            );
            Vec::new()
        }
    };

    // `.filter((s) => s && typeof s.significance === 'number' && s.significance
    // >= MIN_SIGNIFICANCE_THRESHOLD)` — only an object can carry a numeric
    // `significance`; then the spread, then the new-wardrobe-item drop.
    let filtered: Vec<Value> = parsed
        .iter()
        .filter_map(|s| {
            let obj = s.as_object()?;
            let significance = obj.get("significance")?.as_f64()?;
            if significance < MIN_SIGNIFICANCE_THRESHOLD {
                return None;
            }
            Some(finish_suggestion(obj, uuid::Uuid::new_v4().to_string()))
        })
        .filter(|s| {
            !(s.get("field").and_then(Value::as_str) == Some("wardrobeItems")
                && !js_truthy(s.get("subId"))
                && !js_truthy(s.get("wardrobeItem")))
        })
        .collect();

    state.all_suggestions.extend(filtered.iter().cloned());

    log_optimizer_call(
        db,
        c,
        &format!("[character context + memory context + {label} instruction]"),
        0.7,
        6000,
        &raw,
        sub_step_duration_ms,
    )
    .await;

    on_progress(json!({
        "type": "substep_complete",
        "step": "generating",
        "subStep": sub_step,
        "partialSuggestions": filtered,
    }));
    Ok(())
}

/// v4 `runSubStep`'s catch — bug 119's containment: "One misbehaving sub-step
/// must never cost the run. Every sub-step is a self-contained pass whose only
/// output is appended to `allSuggestions`, so an unexpected throw is logged
/// and skipped rather than aborting the whole optimization and discarding the
/// suggestions already gathered." The `substep_complete` it emits carries NO
/// `subStep` (v4's literal omits it).
pub fn contain_sub_step_outcome(
    character_id: &str,
    label: &str,
    outcome: Result<(), String>,
    on_progress: OnProgress<'_>,
) {
    if let Err(error) = outcome {
        tracing::error!(
            target: OPTIMIZER_LOG_TARGET,
            context = %ctx(json!({"characterId": character_id, "subStep": label})),
            error = %error,
            "[CharacterOptimizer] Sub-step failed unexpectedly; continuing"
        );
        on_progress(json!({
            "type": "substep_complete",
            "step": "generating",
            "partialSuggestions": [],
        }));
    }
}

// ---------------------------------------------------------------------------
// The suggestions-file writer
// ---------------------------------------------------------------------------

const SUGGESTIONS_FOLDER: &str = "Suggestions";

/// v4 `yamlString` — "Quote-safe single-line YAML string. Any multi-line input
/// is collapsed (the rendered body contains the full text anyway; this is just
/// the frontmatter summary)."
fn yaml_string(value: &str) -> String {
    static NEWLINES: OnceLock<Regex> = OnceLock::new();
    let re = NEWLINES.get_or_init(|| Regex::new(r"[\r\n]+").expect("static regex"));
    let single = re.replace_all(value, " ");
    format!("\"{}\"", js_trim(&single).replace('"', "\\\""))
}

/// v4 `fenceOrEmpty`.
fn fence_or_empty(value: &str) -> String {
    if value.is_empty() || js_trim(value).is_empty() {
        "_(empty)_".to_string()
    } else {
        format!("```\n{value}\n```")
    }
}

/// v4 `describeSuggestion` — the `####` heading of one suggestion.
fn describe_suggestion(s: &Value) -> String {
    let field = s.get("field").and_then(Value::as_str).unwrap_or("");
    // `a ?? b ?? c ?? ''` — the first non-nullish, interpolated.
    let nullish_chain = |keys: &[&str]| -> String {
        keys.iter()
            .find_map(|k| s.get(*k).filter(|v| !v.is_null()).map(to_js_string))
            .unwrap_or_default()
    };
    match field {
        "scenarios" => js_trim_end(&format!(
            "Scenario: {}",
            nullish_chain(&["subName", "title", "subId"])
        ))
        .to_string(),
        "systemPrompt" => {
            if js_truthy(s.get("subId")) {
                format!("System prompt: {}", nullish_chain(&["subName", "subId"]))
            } else if js_truthy(s.get("name")) {
                format!("New system prompt: {}", str_of(s, "name"))
            } else {
                "New system prompt".to_string()
            }
        }
        "physicalDescription" => {
            if js_truthy(s.get("subName")) {
                format!("Physical description: {}", str_of(s, "subName"))
            } else {
                "Physical description".to_string()
            }
        }
        "wardrobeItems" => {
            if js_truthy(s.get("subId")) {
                format!("Wardrobe item: {}", nullish_chain(&["subName", "subId"]))
            } else if js_truthy(s.get("name")) {
                format!("New wardrobe item: {}", str_of(s, "name"))
            } else {
                "New wardrobe item".to_string()
            }
        }
        "aliases" => {
            if js_truthy(s.get("proposedValue")) {
                format!("New alias: {}", str_of(s, "proposedValue"))
            } else {
                "New alias".to_string()
            }
        }
        "identity" => "Identity".to_string(),
        "description" => "Description".to_string(),
        "manifesto" => "Manifesto".to_string(),
        "personality" => "Personality".to_string(),
        "exampleDialogues" => "Example dialogues".to_string(),
        "talkativeness" => "Talkativeness".to_string(),
        _ => js_interp(s.get("field")),
    }
}

/// v4 `groupSuggestionsForReport` — the eight buckets in v4's heading order,
/// empty buckets omitted.
fn group_suggestions_for_report(suggestions: &[Value]) -> Vec<(&'static str, Vec<&Value>)> {
    let mut general = Vec::new();
    let mut scenario_updates = Vec::new();
    let mut prompt_updates = Vec::new();
    let mut prompt_new = Vec::new();
    let mut physical = Vec::new();
    let mut wardrobe = Vec::new();
    let mut aliases = Vec::new();
    let mut other = Vec::new();
    for s in suggestions {
        match s.get("field").and_then(Value::as_str) {
            // New scenarios are no longer proposed; every scenario suggestion is a refinement.
            Some("scenarios") => scenario_updates.push(s),
            Some("systemPrompt") => {
                if js_truthy(s.get("subId")) {
                    prompt_updates.push(s)
                } else {
                    prompt_new.push(s)
                }
            }
            Some("physicalDescription") => physical.push(s),
            Some("wardrobeItems") => wardrobe.push(s),
            Some("aliases") => aliases.push(s),
            Some(
                "identity" | "description" | "manifesto" | "personality" | "exampleDialogues"
                | "talkativeness",
            ) => general.push(s),
            _ => other.push(s),
        }
    }
    let mut groups = Vec::new();
    for (heading, items) in [
        ("General Fields", general),
        ("Scenario Refinements", scenario_updates),
        ("Physical Description", physical),
        ("Wardrobe", wardrobe),
        ("Aliases", aliases),
        ("System Prompt Refinements", prompt_updates),
        ("Proposed New System Prompts", prompt_new),
        ("Other", other),
    ] {
        if !items.is_empty() {
            groups.push((heading, items));
        }
    }
    groups
}

/// v4 `renderSuggestionsMarkdown` (byte-exact).
pub fn render_suggestions_markdown(
    character: &Value,
    analysis: &Value,
    suggestions: &[Value],
    memory_count: usize,
    model_name: &str,
    generated_at: &str,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    let character_name = str_of(character, "name");
    lines.push("---".to_string());
    lines.push("type: character-suggestions".to_string());
    lines.push(format!("generatedAt: {generated_at}"));
    lines.push(format!("characterId: {}", str_of(character, "id")));
    lines.push(format!("characterName: {}", yaml_string(&character_name)));
    lines.push(format!("model: {}", yaml_string(model_name)));
    lines.push(format!("memoryCount: {memory_count}"));
    lines.push(format!("suggestionCount: {}", suggestions.len()));
    lines.push("---".to_string());
    lines.push(String::new());
    lines.push(format!(
        "# Refinement Suggestions — {}",
        utf16_prefix(generated_at, 10)
    ));
    lines.push(String::new());
    lines.push(format!(
        "The automata have consulted {memory_count} memoir{} from {character_name}'s Commonplace Book and offer the following proposals for the consideration of author and character alike. Nothing herein has been applied — treat this as an itinerary of possible refinements, to be debated, amended, rejected, or commissioned at your leisure.",
        if memory_count == 1 { "" } else { "s" }
    ));
    lines.push(String::new());
    lines.push("## Summary".to_string());
    lines.push(String::new());
    lines.push(js_or(analysis.get("summary"), "_(no summary provided)_"));
    lines.push(String::new());

    let patterns = analysis
        .get("behavioralPatterns")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if !patterns.is_empty() {
        lines.push("## Behavioural Patterns Observed".to_string());
        lines.push(String::new());
        for (i, bp) in patterns.iter().enumerate() {
            lines.push(format!("### {}. {}", i + 1, js_interp(bp.get("pattern"))));
            lines.push(String::new());
            lines.push(format!("**Evidence:** {}", js_interp(bp.get("evidence"))));
            lines.push(String::new());
            lines.push(format!("**Frequency:** {}", js_interp(bp.get("frequency"))));
            lines.push(String::new());
        }
    }

    lines.push("## Proposed Changes".to_string());
    lines.push(String::new());
    if suggestions.is_empty() {
        lines.push("_No changes of sufficient significance were proposed._".to_string());
        lines.push(String::new());
    } else {
        for (heading, items) in group_suggestions_for_report(suggestions) {
            lines.push(format!("### {heading}"));
            lines.push(String::new());
            for s in items {
                lines.push(format!("#### {}", describe_suggestion(s)));
                lines.push(String::new());
                lines.push(format!(
                    "- **Significance:** {}",
                    to_fixed(
                        s.get("significance").and_then(Value::as_f64).unwrap_or(0.0),
                        2
                    )
                ));
                if js_truthy(s.get("rationale")) {
                    lines.push(format!("- **Rationale:** {}", str_of(s, "rationale")));
                }
                lines.push(String::new());
                lines.push("**Current:**".to_string());
                lines.push(String::new());
                lines.push(fence_or_empty(&str_of(s, "currentValue")));
                lines.push(String::new());
                lines.push("**Proposed:**".to_string());
                lines.push(String::new());
                lines.push(fence_or_empty(&str_of(s, "proposedValue")));
                lines.push(String::new());
                if let Some(excerpts) = s.get("memoryExcerpts").and_then(Value::as_array) {
                    if !excerpts.is_empty() {
                        lines.push("**Supporting memoirs:**".to_string());
                        lines.push(String::new());
                        for excerpt in excerpts {
                            lines
                                .push(format!("> {}", to_js_string(excerpt).replace('\n', "\n> ")));
                            lines.push(String::new());
                        }
                    }
                }
            }
        }
    }

    lines.push("---".to_string());
    lines.push(String::new());
    lines.push(
        "_Generated by Quilltap's Character Optimizer in suggestions-file mode. Discuss at leisure; apply only what rings true._"
            .to_string(),
    );
    lines.push(String::new());

    lines.join("\n")
}

/// v4 `writeSuggestionsFileToVault`: `Suggestions/refinement-<stamp>.md`
/// through the database-store writer (the chunk pass rides along), then the
/// info line. `Err` is the store's own message (v4's throw).
#[allow(clippy::too_many_arguments)]
async fn write_suggestions_file_to_vault(
    db: &Db,
    mount_point_id: &str,
    character: &Value,
    analysis: &Value,
    suggestions: &[Value],
    memory_count: usize,
    model_name: &str,
    now_ms: i64,
) -> Result<String, String> {
    let stamp_iso = iso_from_unix_ms(now_ms);
    // `.replace(/[:]/g, '').replace(/\..+$/, '').replace('T', '-')`
    let stamp_file = {
        let no_colons = stamp_iso.replace(':', "");
        let no_fraction = match no_colons.find('.') {
            Some(i) => no_colons[..i].to_string(),
            None => no_colons,
        };
        no_fraction.replacen('T', "-", 1)
    };
    let relative_path = format!("{SUGGESTIONS_FOLDER}/refinement-{stamp_file}.md");
    let content = render_suggestions_markdown(
        character,
        analysis,
        suggestions,
        memory_count,
        model_name,
        &stamp_iso,
    );

    let mid = mount_point_id.to_string();
    let rel = relative_path.clone();
    db.write(move |writers| {
        let mount = writers
            .mount_index()
            .ok_or_else(|| DbError::Internal("no mount-index database".into()))?
            .connection();
        write_database_document(mount, &mid, &rel, &content)
            .map(|_| ())
            .map_err(|e| DbError::Internal(e.to_string()))
    })
    .await
    .map_err(|e| e.to_string())?;

    tracing::info!(
        target: OPTIMIZER_LOG_TARGET,
        context = %ctx(json!({
            "characterId": str_of(character, "id"),
            "mountPointId": mount_point_id,
            "relativePath": relative_path,
            "suggestionCount": suggestions.len(),
        })),
        "[CharacterOptimizer] Wrote suggestions file to vault"
    );
    Ok(relative_path)
}

// ---------------------------------------------------------------------------
// The memory pipeline's semantic arm
// ---------------------------------------------------------------------------

/// The body of v4's `try { … }` around the semantic search: `Ok(None)` when
/// embedding is unavailable (v4 falls through to text search silently),
/// `Ok(Some(rows))` on a completed semantic pass, `Err(message)` for any
/// failure inside the block (v4's catch → the fallback warn).
async fn semantic_candidates<EMB: EmbeddingProvider>(
    db: &Db,
    embedding: &EMB,
    character_id: &str,
    user_id: &str,
    search_query: &str,
) -> Result<Option<Vec<Value>>, String> {
    let uid = user_id.to_string();
    let available = db
        .read_main(move |c| embedding_profiles::find_default(c, &uid))
        .map_err(|e| e.to_string())?
        .is_some();
    if !available {
        return Ok(None);
    }
    let embedding_result = embedding
        .generate_embedding_for_user(search_query, user_id, None, EmbeddingPriority::Background)
        .await
        .map_err(|e| e.message)?;
    let cid = character_id.to_string();
    let store = db
        .read_main(move |c| CharacterVectorStore::load(c, &cid))
        .map_err(|e| e.to_string())?;
    let matched: HashSet<String> = store
        .search(&embedding_result.embedding, 500)
        .into_iter()
        .map(|r| r.id)
        .collect();
    let cid = character_id.to_string();
    let about_self = db
        .read_main(move |c| find_by_character_about_character(c, &cid, &cid))
        .map_err(|e| e.to_string())?;
    Ok(Some(
        about_self
            .into_iter()
            .filter(|m| {
                m.get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| matched.contains(id))
            })
            .collect(),
    ))
}

// ---------------------------------------------------------------------------
// The runner
// ---------------------------------------------------------------------------

/// v4 `runCharacterOptimizer(characterId, connectionProfileId, userId, repos,
/// onProgress, options)`: never fails — every throw inside v4's `try` becomes
/// the `Optimization failed` error line plus the `error` frame.
#[allow(clippy::too_many_arguments)]
pub async fn run_character_optimizer<CMP: CompletionProvider, EMB: EmbeddingProvider>(
    db: &Db,
    completion: &CMP,
    embedding: &EMB,
    character_id: &str,
    connection_profile_id: &str,
    user_id: &str,
    on_progress: OnProgress<'_>,
    options: &OptimizerOptions,
    now_ms: i64,
) {
    let search_query = js_trim(&options.search_query).to_string();
    tracing::info!(
        target: OPTIMIZER_LOG_TARGET,
        context = %ctx(json!({
            "userId": user_id,
            "characterId": character_id,
            "connectionProfileId": connection_profile_id,
            "maxMemories": options.max_memories,
            "searchQuery": if search_query.is_empty() { "(none)" } else { search_query.as_str() },
            "useSemanticSearch": options.use_semantic_search,
            "sinceDate": options.since_date,
            "beforeDate": options.before_date,
        })),
        "[CharacterOptimizer] Starting character optimization"
    );

    on_progress(json!({"type": "start"}));

    let outcome = run_optimizer_inner(
        db,
        completion,
        embedding,
        character_id,
        connection_profile_id,
        user_id,
        on_progress,
        options,
        &search_query,
        now_ms,
    )
    .await;

    if let Err(error_message) = outcome {
        tracing::error!(
            target: OPTIMIZER_LOG_TARGET,
            context = %ctx(json!({
                "characterId": character_id,
                "userId": user_id,
                "error": error_message,
            })),
            "[CharacterOptimizer] Optimization failed"
        );
        on_progress(json!({"type": "error", "error": error_message}));
    }
}

/// The body of v4's `try` — `Err` is the thrown error's `.message`.
#[allow(clippy::too_many_arguments)]
async fn run_optimizer_inner<CMP: CompletionProvider, EMB: EmbeddingProvider>(
    db: &Db,
    completion: &CMP,
    embedding: &EMB,
    character_id: &str,
    connection_profile_id: &str,
    user_id: &str,
    on_progress: OnProgress<'_>,
    options: &OptimizerOptions,
    search_query: &str,
    now_ms: i64,
) -> Result<(), String> {
    let db_msg = |e: DbError| e.to_string();

    // Step 1: Load character and memories
    on_progress(json!({"type": "step_start", "step": "loading"}));

    let cid = character_id.to_string();
    let character = db
        .read_main(|main| {
            db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &cid))
        })
        .map_err(db_msg)?
        .filter(|c| {
            c.get("userId")
                .and_then(Value::as_str)
                .is_none_or(|owner| owner == user_id)
        })
        .ok_or_else(|| "Character not found".to_string())?;

    // Memory retrieval pipeline: search → date filter → rank → reinforcement
    // filter → limit.
    //
    // The optimizer only learns from memories ABOUT the character
    // (self-references: aboutCharacterId === characterId). Inter-character
    // memories the character holds about other participants would skew
    // behavioral-pattern analysis toward those others' habits. Legacy
    // null-aboutCharacterId rows are excluded by design — the
    // post-attribution-overhaul pipeline collapses self-references to
    // characterId, so the null pile is genuinely unattributed and not a
    // fallback for "self".
    let text_search = |q: &str| {
        let (cid, q) = (character_id.to_string(), q.to_string());
        db.read_main(move |c| search_by_content_about_character(c, &cid, &cid, &q))
            .map_err(db_msg)
    };
    let mut candidate_memories: Vec<Value> = if !search_query.is_empty() {
        if options.use_semantic_search {
            // Try semantic search first, fall back to text search
            let mut used_semantic = false;
            let mut rows = Vec::new();
            match semantic_candidates(db, embedding, character_id, user_id, search_query).await {
                Ok(Some(found)) => {
                    rows = found;
                    used_semantic = true;
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(
                        target: OPTIMIZER_LOG_TARGET,
                        context = %ctx(json!({"characterId": character_id, "error": error})),
                        "[CharacterOptimizer] Semantic search failed, falling back to text search"
                    );
                }
            }
            if !used_semantic {
                rows = text_search(search_query)?;
            }
            rows
        } else {
            // Text search only
            text_search(search_query)?
        }
    } else {
        // No search query — load all about-self memories
        let cid = character_id.to_string();
        db.read_main(move |c| find_by_character_about_character(c, &cid, &cid))
            .map_err(db_msg)?
    };

    // Apply date filters (`if (sinceDate)` — JS truthy, so `''` is no filter;
    // an unparsable day is `NaN`, against which every comparison is false).
    if let Some(since) = options.since_date.as_deref().filter(|s| !s.is_empty()) {
        let since_ms = day_boundary_ms(since);
        candidate_memories.retain(|m| match (created_at_ms(m), since_ms) {
            (Some(created), Some(since)) => created >= since,
            _ => false,
        });
    }
    if let Some(before) = options.before_date.as_deref().filter(|s| !s.is_empty()) {
        let before_ms = day_boundary_ms(before);
        candidate_memories.retain(|m| match (created_at_ms(m), before_ms) {
            (Some(created), Some(before)) => created < before,
            _ => false,
        });
    }

    // Rank by weight and filter by reinforcement
    let ranked = rank_memories_by_weight(candidate_memories, now_ms);
    let reinforced: Vec<Value> = ranked.into_iter().filter(is_reinforced).collect();
    let filtered_count = reinforced.len();
    let qualifying_memories: Vec<Value> = reinforced
        .into_iter()
        .take(options.max_memories.max(0) as usize)
        .collect();

    on_progress(json!({
        "type": "step_complete",
        "step": "loading",
        "memoryCount": qualifying_memories.len(),
        "filteredCount": filtered_count,
    }));

    // Check if we have enough memories
    if (qualifying_memories.len() as i64) < MIN_REINFORCED_MEMORIES {
        tracing::info!(
            target: OPTIMIZER_LOG_TARGET,
            context = %ctx(json!({
                "characterId": character_id,
                "found": qualifying_memories.len(),
                "required": MIN_REINFORCED_MEMORIES,
            })),
            "[CharacterOptimizer] Not enough reinforced memories for analysis"
        );
        on_progress(json!({
            "type": "done",
            "analysis": {
                "behavioralPatterns": [],
                "summary": "Not enough reinforced memories to analyze.",
            },
            "suggestions": [],
        }));
        return Ok(());
    }

    // Step 2: Perform analysis
    on_progress(json!({"type": "step_start", "step": "analyzing"}));

    let pid = connection_profile_id.to_string();
    let profile = db
        .read_main(move |c| connection_profiles::find_by_id(c, &pid))
        .map_err(db_msg)?
        .filter(|p| {
            p.get("userId")
                .and_then(Value::as_str)
                .is_none_or(|owner| owner == user_id)
        })
        .ok_or_else(|| "Connection profile not found".to_string())?;

    // Get API key — v4 `if (profile.apiKeyId)` → `findApiKeyByIdAndUserId` →
    // `key_value`, else `''`. Resolved for read-order fidelity; the provider
    // seam resolves its own (the external-prompt precedent).
    let mut _api_key = String::new();
    if let Some(key_id) = profile
        .get("apiKeyId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        let (key_id, uid) = (key_id.to_string(), user_id.to_string());
        if let Some(key) = db
            .read_main(move |c| api_keys::find_by_id_and_user_id(c, &key_id, &uid))
            .map_err(db_msg)?
        {
            _api_key = key.key_value;
        }
    }

    // (v4 ensures the plugin system is initialized here — the v5 provider is
    // the assembled seam.)

    // Create LLM provider — `profile.baseUrl || undefined`: TRUTHY, so an
    // empty-string baseUrl is dropped (unlike the external prompt's `??`).
    let provider = str_of(&profile, "provider");
    let model_name = str_of(&profile, "modelName");
    let base_url = profile
        .get("baseUrl")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    // Build context strings (wardrobe rides along so every pass can see what
    // the character already owns and wears)
    let cid = character_id.to_string();
    let wardrobe_items: Vec<Value> = db
        .read_main(|main| {
            db.read_mount_index(|mount| {
                let docs = DocMountDocumentsRepository::new(mount);
                wardrobe_read::find_by_character_id(main, &docs, &cid, false)
            })
        })
        .map_err(db_msg)?;
    let character_context = build_character_context(&character, Some(&wardrobe_items));
    let wrapped: Vec<Value> = qualifying_memories
        .iter()
        .map(|m| json!({"memory": m}))
        .collect();
    let memory_context = build_memory_context(&wrapped);

    let call_ctx = CallCtx {
        completion,
        provider,
        base_url,
        model_name: model_name.clone(),
        profile_id: str_of(&profile, "id"),
        profile_parameters: profile_params_value(&profile),
        character_id,
        user_id,
        character_context,
        memory_context,
    };

    // Call LLM for analysis
    let (analysis_raw, analysis_duration_ms) =
        call_optimizer_llm(&call_ctx, &get_analysis_prompt(), 0.5, 8000).await?;

    let analysis: Value = match parse_llm_json(&analysis_raw) {
        Ok(v) => v,
        Err(parse_error) => {
            let error = llm_json_failure_message(&analysis_raw, &parse_error);
            tracing::error!(
                target: OPTIMIZER_LOG_TARGET,
                context = %ctx(json!({
                    "characterId": character_id,
                    "rawLength": utf16_len(&analysis_raw),
                    "rawTail": utf16_tail(&analysis_raw, 200),
                    "error": error,
                })),
                "[CharacterOptimizer] Failed to parse analysis JSON"
            );
            return Err(error);
        }
    };

    // Log the LLM call
    log_optimizer_call(
        db,
        &call_ctx,
        "[character context + memory context + analysis instruction]",
        0.5,
        8000,
        &analysis_raw,
        analysis_duration_ms,
    )
    .await;

    on_progress(json!({
        "type": "step_complete",
        "step": "analyzing",
        "analysis": analysis,
    }));

    // Step 3: Generate suggestions, one focused pass per sub-step. Each pass
    // runs the same character+memory context through the LLM but with a
    // prompt that constrains it to a single concern (general fields, a
    // specific scenario, a specific system prompt, or proposing new items),
    // so per-item patterns don't get averaged out across siblings.
    on_progress(json!({"type": "step_start", "step": "generating"}));

    let existing_scenarios: Vec<Value> = character
        .get("scenarios")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let existing_prompts: Vec<Value> = character
        .get("systemPrompts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    // (kind, label, instruction) in v4's order.
    let mut sub_steps: Vec<(&str, String, String)> = vec![(
        "general",
        "General fields".to_string(),
        get_general_fields_suggestions_prompt(&analysis),
    )];
    for scenario in &existing_scenarios {
        sub_steps.push((
            "scenario",
            format!("Scenario: {}", js_interp(scenario.get("title"))),
            get_scenario_suggestion_prompt(&analysis, scenario),
        ));
    }
    for prompt in &existing_prompts {
        sub_steps.push((
            "systemPrompt",
            format!("System prompt: {}", js_interp(prompt.get("name"))),
            get_system_prompt_suggestion_prompt(&analysis, prompt),
        ));
    }
    sub_steps.push((
        "physicalDescription",
        "Physical description".to_string(),
        get_physical_description_suggestion_prompt(
            &analysis,
            // `character.physicalDescription ?? null`
            character
                .get("physicalDescription")
                .filter(|v| !v.is_null()),
        ),
    ));
    sub_steps.push((
        "wardrobe",
        "Wardrobe".to_string(),
        get_wardrobe_suggestion_prompt(&analysis, &wardrobe_items),
    ));
    sub_steps.push((
        "properties",
        "Aliases".to_string(),
        get_properties_suggestion_prompt(&analysis, &character),
    ));
    sub_steps.push((
        "newSystemPrompts",
        "Proposed new system prompts".to_string(),
        get_new_system_prompts_suggestion_prompt(&analysis),
    ));

    let mut state = SubStepState {
        index: 0,
        total: sub_steps.len(),
        all_suggestions: Vec::new(),
    };
    for (kind, label, instruction) in &sub_steps {
        let outcome = run_sub_step_core(
            db,
            &call_ctx,
            &mut state,
            kind,
            label,
            instruction,
            on_progress,
        )
        .await;
        contain_sub_step_outcome(character_id, label, outcome, on_progress);
    }

    let suggestions = state.all_suggestions;

    on_progress(json!({
        "type": "step_complete",
        "step": "generating",
        "suggestions": suggestions,
    }));

    // Optional: write the aggregated suggestions into the character's vault
    // as a markdown document so the user (or the character, in-chat) can
    // review and discuss them without applying anything to the live config.
    let mut suggestions_file_path: Option<String> = None;
    if options.output_mode == OptimizerOutputMode::SuggestionsFile {
        let mount_point_id = character
            .get("characterDocumentMountPointId")
            .filter(|v| js_truthy(Some(v)))
            .map(to_js_string)
            .ok_or_else(|| {
                "Suggestions-file mode requires the character to be linked to a document-store vault."
                    .to_string()
            })?;
        let path = write_suggestions_file_to_vault(
            db,
            &mount_point_id,
            &character,
            &analysis,
            &suggestions,
            qualifying_memories.len(),
            &model_name,
            now_ms,
        )
        .await?;
        on_progress(json!({
            "type": "suggestions_file_written",
            "suggestionsFilePath": path,
        }));
        suggestions_file_path = Some(path);
    }

    // Done
    let mut complete_ctx = json!({
        "characterId": character_id,
        "characterName": str_of(&character, "name"),
        "patternCount": analysis
            .get("behavioralPatterns")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
        "suggestionCount": suggestions.len(),
        "outputMode": options.output_mode.as_str(),
    });
    if let Some(p) = &suggestions_file_path {
        complete_ctx["suggestionsFilePath"] = Value::String(p.clone());
    }
    tracing::info!(
        target: OPTIMIZER_LOG_TARGET,
        context = %ctx(complete_ctx),
        "[CharacterOptimizer] Character optimization complete"
    );

    let mut done = json!({
        "type": "done",
        "analysis": analysis,
        "suggestions": suggestions,
    });
    if let Some(p) = suggestions_file_path {
        done["suggestionsFilePath"] = Value::String(p);
    }
    on_progress(done);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// v4's `coerceSuggestionText` is NOT exported, so no tier-1 oracle can
    /// drive it; the values below were MEASURED on Node 24.13.1 at the
    /// `f699da6f6` pin (`String(0.5)`, `JSON.stringify({user:"x",char:"y"})`,
    /// `String(true)`, `JSON.stringify([1,"a",null])`) — the §3 unification
    /// review's pin for a `pub fn` that had no caller and no test.
    #[test]
    fn coerce_suggestion_text_matches_node() {
        assert_eq!(coerce_suggestion_text(Some(&json!("as is"))), "as is");
        assert_eq!(coerce_suggestion_text(None), "");
        assert_eq!(coerce_suggestion_text(Some(&Value::Null)), "");
        assert_eq!(coerce_suggestion_text(Some(&json!(0.5))), "0.5");
        assert_eq!(coerce_suggestion_text(Some(&json!(true))), "true");
        assert_eq!(
            coerce_suggestion_text(Some(&json!({"user": "x", "char": "y"}))),
            r#"{"user":"x","char":"y"}"#
        );
        assert_eq!(
            coerce_suggestion_text(Some(&json!([1, "a", null]))),
            r#"[1,"a",null]"#
        );
    }

    // --- the runner's helpers (P4.9K1 unit 4) ---

    /// `v8_json_parse_message` against the table MEASURED on Node 24.13.1 at
    /// the `f699da6f6` pin (`JSON.parse` of each input): the short / start /
    /// end / surround context forms and the empty-input arm, plus the
    /// `None` fall-through for inputs that START legally.
    #[test]
    fn v8_json_parse_message_matches_the_measured_table() {
        let table: &[(&str, Option<&str>)] = &[
            (
                "The character is fine as she is.",
                Some("Unexpected token 'T', \"The charac\"... is not valid JSON"),
            ),
            (
                "I have no",
                Some("Unexpected token 'I', \"I have no\" is not valid JSON"),
            ),
            (
                "abc",
                Some("Unexpected token 'a', \"abc\" is not valid JSON"),
            ),
            ("A", Some("Unexpected token 'A', \"A\" is not valid JSON")),
            ("", Some("Unexpected end of JSON input")),
            ("   \n", Some("Unexpected end of JSON input")),
            (
                "Sure! Here is the JSON you asked for: []",
                Some("Unexpected token 'S', \"Sure! Here\"... is not valid JSON"),
            ),
            (
                "\n\n\t  Sure thing, here it is: {}",
                Some("Unexpected token 'S', \"\n\n\t  Sure thing\"... is not valid JSON"),
            ),
            (
                "            The character",
                Some("Unexpected token 'T', ...\"          The charac\"... is not valid JSON"),
            ),
            (
                "xxxxxxxxxxxxxxxxxxxxx",
                Some("Unexpected token 'x', \"xxxxxxxxxx\"... is not valid JSON"),
            ),
            (
                "xxxxxxxxxxxxxxxxxxxx",
                Some("Unexpected token 'x', \"xxxxxxxxxxxxxxxxxxxx\" is not valid JSON"),
            ),
            (".5", Some("Unexpected token '.', \".5\" is not valid JSON")),
            // Legal starts — a failure INSIDE the value is the recorded
            // fall-through to serde's wording.
            ("{\"a\":1,}", None),
            ("[1,2", None),
            ("\"unterminated", None),
            ("-", None),
            // Literal starts — V8 scans the keyword (measured on Node 24.13.1).
            ("tru", Some("Unexpected end of JSON input")),
            ("n", Some("Unexpected end of JSON input")),
            ("nul", Some("Unexpected end of JSON input")),
            ("fals", Some("Unexpected end of JSON input")),
            ("null", None),
            (
                "no json here",
                Some("Unexpected token 'o', \"no json here\" is not valid JSON"),
            ),
            ("nope", Some("Unexpected token 'o', \"nope\" is not valid JSON")),
            (
                "nonsense that is quite long indeed",
                Some("Unexpected token 'o', \"nonsense th\"... is not valid JSON"),
            ),
            (
                "    nah, not json at all here",
                Some("Unexpected token 'a', \"    nah, not js\"... is not valid JSON"),
            ),
            (
                "null x",
                Some("Unexpected non-whitespace character after JSON at position 5 (line 1 column 6)"),
            ),
            (
                "truex",
                Some("Unexpected non-whitespace character after JSON at position 4 (line 1 column 5)"),
            ),
            (
                "false!",
                Some("Unexpected non-whitespace character after JSON at position 5 (line 1 column 6)"),
            ),
            (
                "\nnull x",
                Some("Unexpected non-whitespace character after JSON at position 6 (line 2 column 6)"),
            ),
            (
                "\n\n  true  !",
                Some("Unexpected non-whitespace character after JSON at position 10 (line 3 column 9)"),
            ),
            (
                "null\n\nx",
                Some("Unexpected non-whitespace character after JSON at position 6 (line 3 column 1)"),
            ),
            ("12ab", None),
        ];
        for (input, want) in table {
            assert_eq!(
                v8_json_parse_message(input).as_deref(),
                *want,
                "input {input:?}"
            );
        }
    }

    /// Bug 119's outer containment (`runSubStep`'s catch): a failed core pass
    /// logs the error line and emits a `substep_complete` WITHOUT a `subStep`
    /// (v4's literal omits it); a succeeded pass emits nothing here.
    #[test]
    fn contain_sub_step_outcome_logs_and_emits_only_on_failure() {
        let mut events: Vec<Value> = Vec::new();
        let lines = crate::test_support::captured(|| {
            contain_sub_step_outcome("c-1", "General fields", Ok(()), &mut |e| events.push(e));
            contain_sub_step_outcome(
                "c-1",
                "Scenario: Harbor",
                Err("boom".to_string()),
                &mut |e| events.push(e),
            );
        });
        assert_eq!(
            events,
            vec![
                json!({"type": "substep_complete", "step": "generating", "partialSuggestions": []})
            ]
        );
        let failed: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("[CharacterOptimizer] Sub-step failed unexpectedly; continuing"))
            .collect();
        assert_eq!(failed.len(), 1, "exactly one containment line: {lines:#?}");
        assert!(
            failed[0].starts_with("ERROR ")
                && failed[0]
                    .contains(r#"context={"characterId":"c-1","subStep":"Scenario: Harbor"}"#)
                && failed[0].contains("error=boom"),
            "{}",
            failed[0]
        );
    }

    /// The JS spread's key discipline: an existing key keeps its POSITION with
    /// the new value, keys the model omitted append in literal order, and a
    /// `wardrobeItem` that resolves to `undefined` is OMITTED even when the
    /// model sent one.
    #[test]
    fn finish_suggestion_keeps_spread_key_order() {
        let s = json!({
            "field": "description",
            "rationale": null,
            "currentValue": {"user": "x"},
            "significance": 0.7,
            "wardrobeItem": {"title": "ignored on a non-wardrobe field"},
        });
        let out = finish_suggestion(s.as_object().unwrap(), "ID".to_string());
        assert_eq!(
            serde_json::to_string(&out).unwrap(),
            r#"{"field":"description","rationale":"","currentValue":"{\"user\":\"x\"}","significance":0.7,"id":"ID","proposedValue":"","memoryExcerpts":[]}"#
        );
        // A new wardrobe item: sanitized and appended LAST.
        let s = json!({
            "field": "wardrobeItems",
            "significance": 0.5,
            "wardrobeItem": {"title": " Coat ", "types": ["top", "bogus"], "isDefault": 1},
        });
        let out = finish_suggestion(s.as_object().unwrap(), "ID".to_string());
        let keys: Vec<&String> = out.as_object().unwrap().keys().collect();
        assert_eq!(
            keys,
            [
                "field",
                "significance",
                "wardrobeItem",
                "id",
                "currentValue",
                "proposedValue",
                "rationale",
                "memoryExcerpts"
            ]
        );
        assert_eq!(out["wardrobeItem"]["title"], "Coat");
        assert_eq!(out["wardrobeItem"]["types"], json!(["top"]));
    }

    /// `rankMemoriesByWeight` — the threshold drop, the STABLE descending sort,
    /// and the `NaN`-weight drop for an unparsable `createdAt`.
    #[test]
    fn rank_memories_by_weight_orders_and_drops() {
        let now = 1_772_446_830_000_i64;
        let day = |d: i64| format!("2026-03-0{d}T00:00:00.000Z");
        let mem = |id: &str, importance: f64, created: &str| json!({"id": id, "importance": importance, "createdAt": created, "relatedMemoryIds": []});
        let ranked = rank_memories_by_weight(
            vec![
                mem("low", 0.04, &day(1)), // under the 0.05 floor → dropped
                mem("a", 0.5, &day(1)),
                mem("b", 0.9, &day(1)),
                mem("a2", 0.5, &day(1)), // ties with `a` → stays AFTER it
                mem("nan", 0.9, "not a date"), // JS NaN → dropped
            ],
            now,
        );
        let ids: Vec<&str> = ranked.iter().map(|m| m["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["b", "a", "a2"]);
    }
}
