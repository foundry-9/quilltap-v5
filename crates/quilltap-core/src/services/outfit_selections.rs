//! Per-character outfit application for chat creation / participant add — v4's
//! `lib/wardrobe/apply-outfit-selections.ts` + the `chooseLLMOutfit` cheap-LLM
//! task (`lib/memory/cheap-llm-tasks/outfit-selection.ts`).
//!
//! [`apply_outfit_selections`] resolves each character's `OutfitSelection` to a
//! concrete equipped-slots record and persists it via
//! [`ChatOutfitsRepository::set_equipped_outfit`]. Modes: `default` (the
//! wardrobe items marked default), `manual` (the passed slot assignments),
//! `none` (undressed), `previous_chat` (copy from `source_chat_id`, else
//! default), and `llm_choose` (ask a cheap LLM, else default). An outfit failure
//! is swallowed by the create handler's try/catch — never fatal.
//!
//! The `6bf88959` progress narration: only the slow `llm_choose` path narrates —
//! a `wardrobe-start` frame before the call and a `wardrobe-result` frame (the
//! decided four-slot preview) afterwards; the fallback path resolves the panel
//! with a `log` warning + a `wardrobe-result`.
//!
//! ## Documented seam — the malformed-JSON parse path
//!
//! v4's outfit parser calls `JSON.parse` (which THROWS on malformed JSON →
//! `executeCheapLLMTask` catches → task failure → the caller falls back to
//! default). The ported [`CheapLlmTaskExecutor`] takes an infallible parser, so a
//! provider SUCCESS is always a task success; the parser here returns empty slots
//! on unparseable content (a valid-but-non-object JSON → empty slots is v4-faithful,
//! but a genuinely malformed JSON diverges — empty-outfit success vs v4's
//! default-fallback). The corpus keeps canned responses valid JSON and drives the
//! default-fallback branch via a provider FAILURE instead (identical on both
//! sides), so the divergent path is never exercised.

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::cheap_llm::CheapLlmSelection;
use crate::clock::iso_to_ms;
use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::wardrobe_read::find_by_character_id;
use crate::db::{characters_read, connection_profiles, DbError};
use crate::memory_tasks::strip_code_fences;
use crate::model::completion::{CompletionMessage, CompletionProvider, CompletionRole};
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::creation_progress::{
    CreationProgressEmitter, LogLevel, OutfitPreviewEntry, OutfitPreviewSlots,
};
use crate::services::image_job_common::build_cheap_llm_selection;
use crate::tools::wardrobe_shared::resolve_equipped_outfit_leaf_values;
use crate::wardrobe::{Slots, WARDROBE_SLOT_TYPES};

/// v4 `OUTFIT_SELECTION_PROMPT` (byte-exact).
const OUTFIT_SELECTION_PROMPT: &str = "You are a wardrobe assistant for a roleplay character. Your job is to choose what a character should wear at the start of a scene, based on:
- The character's available wardrobe items
- The scenario/setting description
- The character's personality

Choose items that are contextually appropriate. For example, formal wear for a business meeting, casual clothes for relaxing at home, or era-appropriate costume for a historical setting.

You MUST respond with ONLY a JSON object mapping slot names to ARRAYS of wardrobe item IDs. Valid slots are: \"top\", \"bottom\", \"footwear\", \"accessories\". Use an empty array [] for any slot you want to leave empty.

You may put multiple items in the same slot to layer them (e.g. a t-shirt under a sweater); list them inner-to-outer.

If the available wardrobe contains a composite item (its description mentions it bundles other items, or its title implies an outfit set), you may pick that composite directly — equipping it places it in all the slots it covers.

Example response:
{\"top\": [\"uuid-tshirt\", \"uuid-sweater\"], \"bottom\": [\"uuid-jeans\"], \"footwear\": [\"uuid-boots\"], \"accessories\": []}

Do not include any other text, explanation, or markdown formatting. Just the JSON object.";

/// One character's outfit selection (v4 `OutfitSelection`).
#[derive(Clone, Debug)]
pub struct OutfitSelection {
    pub character_id: String,
    pub mode: String,
    /// The `manual` mode's slot assignments; ignored otherwise.
    pub slots: Option<Slots>,
}

/// The context the LLM path needs (v4 `OutfitSelectionContext`, minus the
/// progress emitter which is passed separately).
#[derive(Clone, Debug, Default)]
pub struct OutfitContext<'a> {
    pub user_id: &'a str,
    pub scenario_text: Option<&'a str>,
    /// The chat settings' `cheapLLMSettings` sub-object (v4 `buildCheapLLMConfig`
    /// source). `None` → the `DEFAULT_CHEAP_LLM_CONFIG` defaults.
    pub cheap_settings: Option<&'a Value>,
    /// The continuation source chat for `previous_chat` mode.
    pub source_chat_id: Option<&'a str>,
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

/// A `Slots` → the `{top,bottom,footwear,accessories}` `Value` that
/// `set_equipped_outfit` stores.
fn slots_to_value(slots: &Slots) -> Value {
    json!({
        "top": slots.top,
        "bottom": slots.bottom,
        "footwear": slots.footwear,
        "accessories": slots.accessories,
    })
}

/// v4 `resolveDefaultOutfit`: the character's wardrobe items marked default,
/// oldest-`createdAt` first, each id pushed into every slot its `types` cover.
pub fn resolve_default_outfit(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Result<Slots, DbError> {
    let docs = DocMountDocumentsRepository::new(mount);
    let items = find_by_character_id(main, &docs, character_id, false)?;
    let mut defaults: Vec<Value> = items
        .into_iter()
        .filter(|i| i.get("isDefault").and_then(Value::as_bool) == Some(true))
        .collect();
    if defaults.is_empty() {
        return Ok(Slots::default());
    }
    // Deterministic: oldest default first; a missing/unparseable createdAt sorts
    // to the end (v4 `Number.POSITIVE_INFINITY`).
    defaults.sort_by(|a, b| {
        let at = s(a, "createdAt")
            .and_then(|c| iso_to_ms(&c))
            .unwrap_or(i64::MAX);
        let bt = s(b, "createdAt")
            .and_then(|c| iso_to_ms(&c))
            .unwrap_or(i64::MAX);
        at.cmp(&bt)
    });

    let mut slots = Slots::default();
    for item in &defaults {
        let Some(id) = item.get("id").and_then(Value::as_str) else {
            continue;
        };
        if let Some(types) = item.get("types").and_then(Value::as_array) {
            for t in types {
                match t.as_str() {
                    Some("top") => slots.top.push(id.to_string()),
                    Some("bottom") => slots.bottom.push(id.to_string()),
                    Some("footwear") => slots.footwear.push(id.to_string()),
                    Some("accessories") => slots.accessories.push(id.to_string()),
                    _ => {}
                }
            }
        }
    }
    Ok(slots)
}

/// v4 `chooseLLMOutfit`'s response parser (over the ported executor's infallible
/// contract — see the module-header seam). Validates each candidate id against
/// the wardrobe (must exist AND cover the slot), tolerating a legacy
/// single-id/null shape.
fn parse_outfit_response(content: &str, items: &[Value]) -> Slots {
    let clean = strip_code_fences(content);
    let parsed: Value = serde_json::from_str(&clean).unwrap_or(Value::Null);
    if !parsed.is_object() {
        return Slots::default();
    }
    // valid item ids + which slots each covers.
    let mut valid: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut item_slots: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();
    for it in items {
        if let Some(id) = it.get("id").and_then(Value::as_str) {
            valid.insert(id);
            let types: Vec<&str> = it
                .get("types")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            item_slots.insert(id, types);
        }
    }

    let mut result = Slots::default();
    for slot in WARDROBE_SLOT_TYPES {
        let raw = parsed.get(slot);
        // array → as-is; null/absent → []; scalar → [scalar].
        let candidates: Vec<&Value> = match raw {
            Some(Value::Array(a)) => a.iter().collect(),
            None | Some(Value::Null) => Vec::new(),
            Some(other) => vec![other],
        };
        let target = match slot {
            "top" => &mut result.top,
            "bottom" => &mut result.bottom,
            "footwear" => &mut result.footwear,
            "accessories" => &mut result.accessories,
            _ => continue,
        };
        for cand in candidates {
            let Some(id) = cand.as_str() else { continue };
            if !valid.contains(id) {
                continue;
            }
            let covers = item_slots
                .get(id)
                .map(|v| v.contains(&slot))
                .unwrap_or(false);
            if !covers {
                continue;
            }
            if !target.iter().any(|x| x == id) {
                target.push(id.to_string());
            }
        }
    }
    result
}

/// Build the two `chooseLLMOutfit` messages (system prompt + the character/
/// wardrobe/scenario user message), byte-exact to v4.
fn build_outfit_messages(
    character: &Value,
    wardrobe_items: &[Value],
    scenario_text: Option<&str>,
) -> Vec<CompletionMessage> {
    let character_name = s(character, "name").unwrap_or_default();
    let note = |label: &str, field: &str| -> String {
        match s(character, field) {
            Some(v) if !v.trim().is_empty() => format!("\n{label}\n{v}"),
            _ => String::new(),
        }
    };
    let manifesto_note = note("Character Manifesto (foundational tenets):", "manifesto");
    let description_note = note(
        "Character Description (behaviour and mannerisms):",
        "description",
    );
    let personality_note = note("Character Personality (internal drivers):", "personality");
    let scenario_note = match scenario_text {
        Some(sc) if !sc.is_empty() => format!("\nScenario: {sc}"),
        _ => "\nScenario: (general conversation, no specific setting)".to_string(),
    };

    let wardrobe_section = wardrobe_items
        .iter()
        .map(|item| {
            let id = s(item, "id").unwrap_or_default();
            let title = s(item, "title").unwrap_or_default();
            let types = item
                .get("types")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let appropriateness = match s(item, "appropriateness") {
                Some(a) if !a.is_empty() => format!(" [appropriate for: {a}]"),
                _ => String::new(),
            };
            let desc = match s(item, "description") {
                Some(d) if !d.is_empty() => format!(" — {d}"),
                _ => String::new(),
            };
            let component_count = item
                .get("componentItemIds")
                .and_then(Value::as_array)
                .map(|a| a.len())
                .unwrap_or(0);
            let composite_marker = if component_count > 0 {
                let plural = if component_count == 1 { "" } else { "s" };
                format!(" [composite — bundles {component_count} other item{plural}]")
            } else {
                String::new()
            };
            format!(
                "  - ID: {id} | \"{title}\"{composite_marker} (covers: {types}){appropriateness}{desc}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let user_content = format!(
        "Character: {character_name}{manifesto_note}{description_note}{personality_note}{scenario_note}\n\nAvailable Wardrobe Items:\n{wardrobe_section}\n\nChoose what {character_name} should wear for this scene:"
    );

    vec![
        CompletionMessage {
            role: CompletionRole::System,
            content: OUTFIT_SELECTION_PROMPT.to_string(),
        },
        CompletionMessage {
            role: CompletionRole::User,
            content: user_content,
        },
    ]
}

/// v4 `toOutfitPreviewSlots` over the resolved leaf items: each item →
/// `{id, title, isComposite}`.
fn to_outfit_preview_slots(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    slots: &Slots,
) -> OutfitPreviewSlots {
    let docs = DocMountDocumentsRepository::new(mount);
    let leaf = resolve_equipped_outfit_leaf_values(main, &docs, character_id, slots, &[])
        .unwrap_or_default();
    let map = |items: &[Value]| -> Vec<OutfitPreviewEntry> {
        items
            .iter()
            .map(|i| OutfitPreviewEntry {
                id: s(i, "id").unwrap_or_default(),
                title: s(i, "title").unwrap_or_default(),
                is_composite: i
                    .get("componentItemIds")
                    .and_then(Value::as_array)
                    .map(|a| !a.is_empty())
                    .unwrap_or(false),
            })
            .collect()
    };
    OutfitPreviewSlots {
        top: map(&leaf.top),
        bottom: map(&leaf.bottom),
        footwear: map(&leaf.footwear),
        accessories: map(&leaf.accessories),
    }
}

/// v4 `applyOutfitSelections`: resolve + persist each character's outfit. `main`
/// is the writer connection (reads + `set_equipped_outfit`); `docs` the
/// mount-index store; `completion`/`executor` the cheap-LLM boundary for
/// `llm_choose`. An outfit failure never propagates (v4's create-handler
/// try/catch); per-selection read errors surface as `Err` for that try/catch.
#[allow(clippy::too_many_arguments)]
pub async fn apply_outfit_selections<C: CompletionProvider>(
    main: &Connection,
    mount: &Connection,
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    chat_id: &str,
    selections: &[OutfitSelection],
    ctx: &OutfitContext<'_>,
    emitter: &CreationProgressEmitter,
) -> Result<(), DbError> {
    let outfits = ChatOutfitsRepository::new(main);
    for selection in selections {
        let character_id = selection.character_id.as_str();
        match selection.mode.as_str() {
            "default" => {
                let slots = resolve_default_outfit(main, mount, character_id)?;
                outfits.set_equipped_outfit(chat_id, character_id, &slots_to_value(&slots))?;
            }
            "manual" => {
                let slots = selection.slots.clone().unwrap_or_default();
                outfits.set_equipped_outfit(chat_id, character_id, &slots_to_value(&slots))?;
            }
            "none" => {
                outfits.set_equipped_outfit(
                    chat_id,
                    character_id,
                    &slots_to_value(&Slots::default()),
                )?;
            }
            "previous_chat" => {
                let mut applied = false;
                if let Some(source) = ctx.source_chat_id {
                    // v4 wraps the read in try/catch → fall back to default.
                    if let Ok(Some(prev)) =
                        outfits.get_equipped_outfit_for_character(source, character_id)
                    {
                        outfits.set_equipped_outfit(chat_id, character_id, &prev)?;
                        applied = true;
                    }
                }
                if !applied {
                    let slots = resolve_default_outfit(main, mount, character_id)?;
                    outfits.set_equipped_outfit(chat_id, character_id, &slots_to_value(&slots))?;
                }
            }
            "llm_choose" => {
                apply_llm_choose(
                    main,
                    mount,
                    completion,
                    executor,
                    &outfits,
                    chat_id,
                    character_id,
                    ctx,
                    emitter,
                )
                .await?;
            }
            _ => {
                // Unknown mode — v4 logs and skips (no write).
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn apply_llm_choose<C: CompletionProvider>(
    main: &Connection,
    mount: &Connection,
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    outfits: &ChatOutfitsRepository<'_>,
    chat_id: &str,
    character_id: &str,
    ctx: &OutfitContext<'_>,
    emitter: &CreationProgressEmitter,
) -> Result<(), DbError> {
    let mut applied = false;
    let mut consulted = false;
    let mut consulted_name = String::new();

    // v4 wraps the whole attempt in try/catch → fall to default; a read error
    // here propagates to the create handler's try/catch (as v4's throws do).
    let docs = DocMountDocumentsRepository::new(mount);
    let character = characters_read::find_by_id(main, mount, character_id)?;
    let wardrobe_items = find_by_character_id(main, &docs, character_id, false)?;

    if let Some(character) = character.as_ref() {
        if !wardrobe_items.is_empty() {
            let all_profiles = connection_profiles::find_all(main)?;
            let selection: Option<CheapLlmSelection> =
                build_cheap_llm_selection(&all_profiles, ctx.cheap_settings);

            if let Some(selection) = selection {
                let character_name = s(character, "name").unwrap_or_default();
                consulted = true;
                consulted_name = character_name.clone();
                emitter.wardrobe_start(character_id, &character_name);

                let messages = build_outfit_messages(character, &wardrobe_items, ctx.scenario_text);
                let items_for_parse = wardrobe_items.clone();
                let result = executor
                    .execute(
                        completion,
                        &selection,
                        messages,
                        move |content| parse_outfit_response(content, &items_for_parse),
                        None,
                        None,
                        Some(character_id),
                        Some("outfit-selection"),
                    )
                    .await;

                if result.success {
                    if let Some(slots) = result.result {
                        outfits.set_equipped_outfit(
                            chat_id,
                            character_id,
                            &slots_to_value(&slots),
                        )?;
                        applied = true;
                        // Publish the decided outfit for the dialog.
                        let preview = to_outfit_preview_slots(main, mount, character_id, &slots);
                        emitter.wardrobe_result(character_id, &character_name, preview);
                    }
                }
            }
        }
    }

    if !applied {
        let slots = resolve_default_outfit(main, mount, character_id)?;
        outfits.set_equipped_outfit(chat_id, character_id, &slots_to_value(&slots))?;
        if consulted {
            emitter.log(
                format!("{consulted_name} settled on their usual attire."),
                Some(LogLevel::Warn),
            );
            let preview = to_outfit_preview_slots(main, mount, character_id, &slots);
            emitter.wardrobe_result(character_id, &consulted_name, preview);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(id: &str, types: &[&str], is_default: bool) -> Value {
        json!({ "id": id, "title": id, "types": types, "isDefault": is_default })
    }

    #[test]
    fn parse_validates_ids_and_slot_coverage() {
        let items = vec![
            item("shirt", &["top"], false),
            item("jeans", &["bottom"], false),
            item("dress", &["top", "bottom"], false),
        ];
        // Valid ids in the right slots; a bogus id + wrong-slot id dropped.
        let content = r#"{"top":["shirt","dress","nope","jeans"],"bottom":["dress"],"footwear":[],"accessories":[]}"#;
        let slots = parse_outfit_response(content, &items);
        assert_eq!(slots.top, vec!["shirt", "dress"]); // jeans (bottom-only) dropped from top
        assert_eq!(slots.bottom, vec!["dress"]);
        assert!(slots.footwear.is_empty());
    }

    #[test]
    fn parse_tolerates_scalar_and_non_object() {
        let items = vec![item("shirt", &["top"], false)];
        // Legacy single-id shape.
        let s1 = parse_outfit_response(r#"{"top":"shirt"}"#, &items);
        assert_eq!(s1.top, vec!["shirt"]);
        // Non-object → empty.
        let s2 = parse_outfit_response("[1,2,3]", &items);
        assert!(s2.top.is_empty() && s2.bottom.is_empty());
    }

    #[test]
    fn message_layout_is_byte_exact() {
        let character = json!({
            "name": "Aria",
            "manifesto": "Honor.",
            "description": "",
            "personality": "Bold."
        });
        let items = vec![
            json!({ "id": "a1", "title": "Cloak", "types": ["top"], "componentItemIds": [] }),
            json!({ "id": "a2", "title": "Set", "types": ["top", "bottom"], "componentItemIds": ["x", "y"], "description": "a bundle" }),
        ];
        let msgs = build_outfit_messages(&character, &items, Some("A keep."));
        assert_eq!(msgs[0].content, OUTFIT_SELECTION_PROMPT);
        let u = &msgs[1].content;
        assert!(
            u.starts_with("Character: Aria\nCharacter Manifesto (foundational tenets):\nHonor.")
        );
        // Empty description note omitted; personality note present.
        assert!(!u.contains("Character Description (behaviour and mannerisms):"));
        assert!(u.contains("\nCharacter Personality (internal drivers):\nBold."));
        assert!(u.contains("\nScenario: A keep."));
        assert!(u.contains("  - ID: a1 | \"Cloak\" (covers: top)"));
        assert!(u.contains("  - ID: a2 | \"Set\" [composite — bundles 2 other items] (covers: top, bottom) — a bundle"));
        assert!(u.ends_with("Choose what Aria should wear for this scene:"));
    }
}
