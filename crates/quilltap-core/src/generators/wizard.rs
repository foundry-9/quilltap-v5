//! v4 `lib/services/character-wizard.service.ts` — the AI Wizard (`p4.9k`,
//! P4.9K2). This file carries the PURE half: [`build_context_prompt`], the
//! context every wizard field-generation call opens with. The prompt CONSTANTS
//! live in the generated [`super::wizard_prompts`].
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

use serde_json::Value;

use crate::jsstr::js_trim;
use crate::pascal::js_value::to_js_string;

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
    if let Some(p) = existing.get("pronouns").filter(|p| !p.is_null()) {
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
