//! v4 `lib/services/character-optimizer.service.ts` — Aurora's "Refine from
//! Memories" (`p4.9k`, P4.9K1). **Ported from the POST-bug-119 shape** (v4
//! `15573c3a1`, 2026-09-02, drift-ledger §3): the pre-fix inline `.filter` on a
//! `parseLLMJson<OptimizerSuggestion[]>` cast never existed in v5 and is never
//! transcribed here.
//!
//! This file carries the PURE half — the types, the coercions, and every prompt
//! builder. Each prompt string IS wire bytes: it reaches a paid model verbatim,
//! so all of it is diffed against v4's real exports by
//! `character_optimizer_prompts_equivalence`.
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

use serde_json::Value;

use crate::api::system_qtap::js_truthy;
use crate::generators::field_semantics::{
    FIELD_SEMANTICS_PREAMBLE, FULL_FIELD_SEMANTICS, PROPERTIES_SEMANTICS, WARDROBE_SEMANTICS,
};
use crate::pascal::js_value::to_js_string;

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
}
