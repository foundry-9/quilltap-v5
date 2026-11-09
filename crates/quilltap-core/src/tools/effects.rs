//! Tool effects — the one resolver (v4 `lib/tools/effect-types.ts` +
//! `lib/tools/effects.ts`, P4.D343 at `236c04e35`).
//!
//! Every tool declares what it does to the world: `none` (nothing beyond its
//! own return value), `read` (reads, changes nothing), `write` (changes stored
//! state) or `destroy` (a write that cannot be undone from inside Quilltap —
//! deletion). Anything undeclared counts as `write`.
//!
//! v4 declares each built-in tool's effect beside its definition
//! (`xxxToolEffect`) and collects them into `BUILT_IN_TOOL_EFFECTS`. v5 keeps
//! ONE table here — a `match` on the tool's `function.name` (ruling R-13), never
//! a field on [`crate::tools::definitions::ToolDef`] (the generated catalog is
//! P4.D350's `CATALOG`). The catalog-coverage unit test below fails when a
//! catalog name has no entry, so declaring an effect stays part of adding a
//! tool, as v4's `tool-definitions-snapshot.test.ts` makes it.
//!
//! Four tools carry a COMPUTED effect that depends on the call's raw input:
//! `state`, `edit_custom_tool`, `doc_open_document` and `run_custom`. Each
//! validates the input with its tool's own schema first; input that fails
//! validation counts as [`UNDECLARED_TOOL_EFFECT`] — never the resolver (v4
//! `computedToolEffect`). The `run_custom` resolver lives here rather than in
//! `tools/run_custom.rs` (it reads only `input.tool` and the custom-tool map).
//!
//! Consumers: the executor's ceiling check (`tools/executor.rs`, the real
//! guarantee), `build_tools`' tool-list filter, the autonomous-room destructive
//! gate (`resolve_turn_effect_ceiling`), and the storyteller bid tier.
//!
//! v4 `8becfc6ce` deleted `isComputedTool`; v5 writes no twin of it.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

/// What a tool call does to the world (v4 `ToolEffect`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ToolEffect {
    None,
    Read,
    Write,
    Destroy,
}

/// v4 `TOOL_EFFECTS`, in rank order.
pub const TOOL_EFFECTS: [ToolEffect; 4] = [
    ToolEffect::None,
    ToolEffect::Read,
    ToolEffect::Write,
    ToolEffect::Destroy,
];

impl ToolEffect {
    /// The wire string (`'none' | 'read' | 'write' | 'destroy'`).
    pub fn as_str(self) -> &'static str {
        match self {
            ToolEffect::None => "none",
            ToolEffect::Read => "read",
            ToolEffect::Write => "write",
            ToolEffect::Destroy => "destroy",
        }
    }

    /// v4 `TOOL_EFFECT_RANK` (`none: 0, read: 1, write: 2, destroy: 3`).
    pub fn rank(self) -> u8 {
        match self {
            ToolEffect::None => 0,
            ToolEffect::Read => 1,
            ToolEffect::Write => 2,
            ToolEffect::Destroy => 3,
        }
    }

    /// Parse a wire string; `None` for anything else.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<ToolEffect> {
        match s {
            "none" => Some(ToolEffect::None),
            "read" => Some(ToolEffect::Read),
            "write" => Some(ToolEffect::Write),
            "destroy" => Some(ToolEffect::Destroy),
            _ => None,
        }
    }
}

impl serde::Serialize for ToolEffect {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

/// What an undeclared tool, or a computed effect whose input fails validation,
/// counts as (v4 `UNDECLARED_TOOL_EFFECT`).
pub const UNDECLARED_TOOL_EFFECT: ToolEffect = ToolEffect::Write;

/// Everything an effect lookup may need beyond the call itself (v4
/// `ToolEffectContext` / `ToolEffectLookups`). v4's `Map`s become sorted maps;
/// order is never observed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectLookups {
    /// Custom tool name → derived effect, for the caller's roster.
    pub custom_tool_effects: BTreeMap<String, ToolEffect>,
    /// Plugin / MCP tool name → declared effect. v5 has no tool-plugin
    /// registry, so this is always empty in production
    /// (`PLUGIN_TOOL_EFFECTS_EMPTY`, ruling R-2); the parameter is carried so
    /// the signatures match v4's and a future registry slots in.
    pub plugin_tool_effects: BTreeMap<String, ToolEffect>,
}

/// The four computed effects (v4 `computedToolEffect(...)` specs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputedEffect {
    /// `state`: `fetch` reads; `set` and `delete` write.
    State,
    /// `edit_custom_tool`: `action: "schema"` is none; no `content` reads; else write.
    EditCustomTool,
    /// `doc_open_document`: a `uri` or `path` reads; a blank document writes.
    DocOpenDocument,
    /// `run_custom`: whatever the named custom tool does.
    RunCustom,
}

impl ComputedEffect {
    /// The highest effect any valid input can have. All four are `write`.
    pub fn max_effect(self) -> ToolEffect {
        ToolEffect::Write
    }

    /// The per-ceiling description note (v4's `notes[ceiling]`).
    fn note(self, ceiling: ToolEffect) -> Option<&'static str> {
        match (self, ceiling) {
            (ComputedEffect::State, ToolEffect::Read) => Some(
                "This turn allows only `operation: \"fetch\"`; `set` and `delete` will be refused.",
            ),
            (ComputedEffect::EditCustomTool, ToolEffect::None) => {
                Some("This turn allows only `action: \"schema\"`.")
            }
            (ComputedEffect::EditCustomTool, ToolEffect::Read) => Some(
                "This turn allows only `action: \"schema\"` and validation (no `content`); saving will be refused.",
            ),
            (ComputedEffect::DocOpenDocument, ToolEffect::Read) => Some(
                "This turn allows only opening an existing file (pass `path` or `uri`); creating a blank document will be refused.",
            ),
            (ComputedEffect::RunCustom, ToolEffect::Read) => Some(
                "This turn allows only the custom tools marked \"read\" in the list above; a tool with effects will be refused.",
            ),
            _ => None,
        }
    }

    /// v4 `usesAllowedUnder(ceiling)`: `null` when the ceiling allows every use
    /// (`rank[ceiling] >= rank[maxEffect]`), else the note for that ceiling (or
    /// `null` when there is none — the tool is then dropped by the list filter).
    pub fn uses_allowed_under(self, ceiling: ToolEffect) -> Option<&'static str> {
        if ceiling.rank() >= self.max_effect().rank() {
            return None;
        }
        self.note(ceiling)
    }

    /// The effect of one call: the tool's own schema first (a failed parse is
    /// [`UNDECLARED_TOOL_EFFECT`]), then the resolver. v4 parses
    /// `input ?? {}`, so a JSON `null` input parses as `{}`.
    pub fn resolve(self, input: &Value, lookups: &EffectLookups) -> ToolEffect {
        let empty = Value::Object(Map::new());
        let input = if input.is_null() { &empty } else { input };
        let resolved = match self {
            ComputedEffect::State => crate::tools::state::state_tool_effect(input),
            ComputedEffect::EditCustomTool => {
                crate::tools::edit_custom_tool::edit_custom_tool_effect(input)
            }
            ComputedEffect::DocOpenDocument => {
                crate::tools::doc_edit::document_ui::doc_open_document_effect(input)
            }
            ComputedEffect::RunCustom => run_custom_tool_effect(input, lookups),
        };
        resolved.unwrap_or(UNDECLARED_TOOL_EFFECT)
    }
}

/// One built-in tool's declared spec (v4 `ToolEffectSpec`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolEffectSpec {
    Declared(ToolEffect),
    Computed(ComputedEffect),
}

/// v4 `BUILT_IN_TOOL_EFFECTS` (`effects.ts:107-182`) — 78 rows: 3 none, 37
/// read, 29 write, 5 destroy, 4 computed. Order is unobservable; the SET and
/// the VALUES are what `tool_effects_equivalence` pins. `None` when the tool
/// declares no effect (v4 `toolEffectSpecOf` — an own-property lookup).
pub fn tool_effect_spec_of(name: &str) -> Option<ToolEffectSpec> {
    use ToolEffect::*;
    use ToolEffectSpec::{Computed, Declared};
    Some(match name {
        "ask_carina" => Declared(Read),
        "attach_image" => Declared(Write),
        "delete_annotation" => Declared(Destroy),
        "describe_image" => Declared(Read),
        "discard_mail" => Declared(Destroy),
        "doc_close_document" => Declared(Read),
        "doc_copy_file" => Declared(Write),
        "doc_create_folder" => Declared(Write),
        "doc_delete_blob" => Declared(Destroy),
        "doc_delete_file" => Declared(Destroy),
        "doc_delete_folder" => Declared(Destroy),
        "doc_focus" => Declared(Read),
        "doc_grep" => Declared(Read),
        "doc_insert_text" => Declared(Write),
        "doc_list_blobs" => Declared(Read),
        "doc_list_files" => Declared(Read),
        "doc_move_file" => Declared(Write),
        "doc_move_folder" => Declared(Write),
        "doc_open_document" => Computed(ComputedEffect::DocOpenDocument),
        "doc_read_blob" => Declared(Read),
        "doc_read_file" => Declared(Read),
        "doc_read_frontmatter" => Declared(Read),
        "doc_read_heading" => Declared(Read),
        "doc_str_replace" => Declared(Write),
        "doc_update_frontmatter" => Declared(Write),
        "doc_update_heading" => Declared(Write),
        "doc_write_blob" => Declared(Write),
        "doc_write_file" => Declared(Write),
        "edit_custom_tool" => Computed(ComputedEffect::EditCustomTool),
        "generate_image" => Declared(Write),
        "help_navigate" => Declared(Read),
        "help_search" => Declared(Read),
        "help_settings" => Declared(Read),
        "inventory_archive" => Declared(Write),
        "inventory_create" => Declared(Write),
        "inventory_equip" => Declared(Write),
        "inventory_give" => Declared(Write),
        "inventory_list" => Declared(Read),
        "inventory_pick_up" => Declared(Write),
        "inventory_read" => Declared(Read),
        "inventory_unequip" => Declared(Write),
        "inventory_update" => Declared(Write),
        "keep_image" => Declared(Write),
        "list_images" => Declared(Read),
        "list_mail" => Declared(Read),
        "log_create_entry" => Declared(Write),
        "log_grep" => Declared(Read),
        "log_read" => Declared(Read),
        "project_info" => Declared(Read),
        "read_conversation" => Declared(Read),
        "read_mail" => Declared(Read),
        "request_full_context" => Declared(Read),
        "rng" => Declared(None),
        "run_custom" => Computed(ComputedEffect::RunCustom),
        "run_sql" => Declared(Read),
        // === P4.D374 === the four Brahma lookups (v4 `c539c82e0`, `effects.ts:164-171`).
        "lookup_character" => Declared(Read),
        "lookup_memories" => Declared(Read),
        "lookup_messages" => Declared(Read),
        "read_messages" => Declared(Read),
        // === /P4.D374 ===
        "search" => Declared(Read),
        "search_web" => Declared(Read),
        "self_inventory" => Declared(Read),
        "send_mail" => Declared(Write),
        "state" => Computed(ComputedEffect::State),
        "submit_final_response" => Declared(None),
        "terminal_list" => Declared(Read),
        "terminal_read" => Declared(Read),
        "upsert_annotation" => Declared(Write),
        "wardrobe_archive" => Declared(Write),
        "wardrobe_create" => Declared(Write),
        "wardrobe_history" => Declared(Read),
        "wardrobe_list" => Declared(Read),
        "wardrobe_read" => Declared(Read),
        "wardrobe_rollback" => Declared(Write),
        "wardrobe_take_off" => Declared(Write),
        "wardrobe_update" => Declared(Write),
        "wardrobe_wear" => Declared(Write),
        "whisper" => Declared(None),
        _ => return Option::None,
    })
}

/// Every key of v4's `BUILT_IN_TOOL_EFFECTS`, sorted (code-unit order). The
/// table is the `match` above; this list is the enumeration the oracle compares
/// and the unit tests walk.
pub const BUILT_IN_TOOL_EFFECT_NAMES: [&str; 78] = [
    "ask_carina",
    "attach_image",
    "delete_annotation",
    "describe_image",
    "discard_mail",
    "doc_close_document",
    "doc_copy_file",
    "doc_create_folder",
    "doc_delete_blob",
    "doc_delete_file",
    "doc_delete_folder",
    "doc_focus",
    "doc_grep",
    "doc_insert_text",
    "doc_list_blobs",
    "doc_list_files",
    "doc_move_file",
    "doc_move_folder",
    "doc_open_document",
    "doc_read_blob",
    "doc_read_file",
    "doc_read_frontmatter",
    "doc_read_heading",
    "doc_str_replace",
    "doc_update_frontmatter",
    "doc_update_heading",
    "doc_write_blob",
    "doc_write_file",
    "edit_custom_tool",
    "generate_image",
    "help_navigate",
    "help_search",
    "help_settings",
    "inventory_archive",
    "inventory_create",
    "inventory_equip",
    "inventory_give",
    "inventory_list",
    "inventory_pick_up",
    "inventory_read",
    "inventory_unequip",
    "inventory_update",
    "keep_image",
    "list_images",
    "list_mail",
    "log_create_entry",
    "log_grep",
    "log_read",
    "lookup_character",
    "lookup_memories",
    "lookup_messages",
    "project_info",
    "read_conversation",
    "read_mail",
    "read_messages",
    "request_full_context",
    "rng",
    "run_custom",
    "run_sql",
    "search",
    "search_web",
    "self_inventory",
    "send_mail",
    "state",
    "submit_final_response",
    "terminal_list",
    "terminal_read",
    "upsert_annotation",
    "wardrobe_archive",
    "wardrobe_create",
    "wardrobe_history",
    "wardrobe_list",
    "wardrobe_read",
    "wardrobe_rollback",
    "wardrobe_take_off",
    "wardrobe_update",
    "wardrobe_wear",
    "whisper",
];

/// v4 `runCustomToolEffect`'s resolver: `lookups.customToolEffects.get(tool)
/// ?? 'write'`, after `runCustomToolInputSchema` (a failed parse → `None`).
fn run_custom_tool_effect(input: &Value, lookups: &EffectLookups) -> Option<ToolEffect> {
    let parsed = crate::tools::run_custom::validate_run_custom_input(input)?;
    Some(
        lookups
            .custom_tool_effects
            .get(&parsed.tool)
            .copied()
            .unwrap_or(ToolEffect::Write),
    )
}

/// The effect of one call (v4 `effectOfToolCall`). A computed built-in effect
/// sees the call's input (validated by the tool's own schema; invalid input
/// counts as `write`); a name absent from the table reads
/// `plugin_tool_effects`, then `write`.
pub fn effect_of_tool_call(name: &str, input: &Value, lookups: &EffectLookups) -> ToolEffect {
    match tool_effect_spec_of(name) {
        Some(ToolEffectSpec::Declared(e)) => e,
        Some(ToolEffectSpec::Computed(c)) => c.resolve(input, lookups),
        None => lookups
            .plugin_tool_effects
            .get(name)
            .copied()
            .unwrap_or(UNDECLARED_TOOL_EFFECT),
    }
}

/// The highest effect any call of this tool can have (v4 `maxEffectOfTool`).
pub fn max_effect_of_tool(name: &str, lookups: &EffectLookups) -> ToolEffect {
    match tool_effect_spec_of(name) {
        Some(ToolEffectSpec::Declared(e)) => e,
        Some(ToolEffectSpec::Computed(c)) => c.max_effect(),
        None => lookups
            .plugin_tool_effects
            .get(name)
            .copied()
            .unwrap_or(UNDECLARED_TOOL_EFFECT),
    }
}

/// The highest of a set of effects; `none` for an empty set (v4
/// `highestToolEffect`).
pub fn highest_tool_effect<I: IntoIterator<Item = ToolEffect>>(effects: I) -> ToolEffect {
    let mut highest = ToolEffect::None;
    for effect in effects {
        if effect.rank() > highest.rank() {
            highest = effect;
        }
    }
    highest
}

/// The higher of two effects; a tie keeps `a` (v4 `maxToolEffect`).
pub fn max_tool_effect(a: ToolEffect, b: ToolEffect) -> ToolEffect {
    if a.rank() >= b.rank() {
        a
    } else {
        b
    }
}

/// True when `effect` is within `ceiling` (v4 `isWithinCeiling`).
pub fn is_within_ceiling(effect: ToolEffect, ceiling: ToolEffect) -> bool {
    effect.rank() <= ceiling.rank()
}

/// The lower of a ceiling (unset = no ceiling) and another ceiling (v4
/// `lowerToolEffect`); a tie keeps the ceiling.
pub fn lower_tool_effect(ceiling: Option<ToolEffect>, other: ToolEffect) -> ToolEffect {
    match ceiling {
        None => other,
        Some(c) if c.rank() <= other.rank() => c,
        Some(_) => other,
    }
}

/// The computed spec's note for this ceiling, or `None` (v4
/// `spec.usesAllowedUnder(ceiling)` on a computed spec; a declared tool has no
/// notes).
pub fn uses_allowed_under(name: &str, ceiling: ToolEffect) -> Option<&'static str> {
    match tool_effect_spec_of(name) {
        Some(ToolEffectSpec::Computed(c)) => c.uses_allowed_under(ceiling),
        _ => None,
    }
}

/// The tool result's text when a call is refused for exceeding the turn's
/// ceiling (v4 `describeCeilingRefusal`).
pub fn describe_ceiling_refusal(name: &str, effect: ToolEffect, ceiling: ToolEffect) -> String {
    format!(
        "Not run: `{name}` is a {} call, and this turn allows only {}. Nothing was changed. Carry on without it.",
        effect.as_str(),
        ceiling_phrase(ceiling)
    )
}

fn ceiling_phrase(ceiling: ToolEffect) -> &'static str {
    match ceiling {
        ToolEffect::None => "tools with no effect (no lookups, no changes)",
        ToolEffect::Read => "lookups (read-only tools)",
        ToolEffect::Write => "lookups and changes, not deletions",
        ToolEffect::Destroy => "any tool",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeSet;

    #[test]
    fn table_has_v4s_74_rows_in_v4s_split() {
        let mut counts = [0usize; 5];
        for name in BUILT_IN_TOOL_EFFECT_NAMES {
            match tool_effect_spec_of(name).expect(name) {
                ToolEffectSpec::Declared(e) => counts[e.rank() as usize] += 1,
                ToolEffectSpec::Computed(_) => counts[4] += 1,
            }
        }
        assert_eq!(BUILT_IN_TOOL_EFFECT_NAMES.len(), 78);
        assert_eq!(counts, [3, 37, 29, 5, 4]);
        let sorted: Vec<&str> = {
            let mut v = BUILT_IN_TOOL_EFFECT_NAMES.to_vec();
            v.sort_unstable();
            v.dedup();
            v
        };
        assert_eq!(sorted, BUILT_IN_TOOL_EFFECT_NAMES.to_vec());
        assert_eq!(tool_effect_spec_of("not_a_tool"), None);
    }

    /// Correction 2: every distinct catalog `function.name` is an effect key,
    /// and every effect key is a catalog name — so a P4.D350 `CATALOG` regen
    /// cannot silently add an undeclared tool.
    #[test]
    fn every_catalog_name_declares_an_effect() {
        let catalog: BTreeSet<&str> = crate::tools::definitions::TOOL_DEFINITIONS
            .iter()
            .map(|d| d.name)
            .collect();
        let table: BTreeSet<&str> = BUILT_IN_TOOL_EFFECT_NAMES.iter().copied().collect();
        assert_eq!(catalog, table);
    }

    #[test]
    fn the_four_notes_and_phrases() {
        assert_eq!(
            uses_allowed_under("state", ToolEffect::Read),
            Some(
                "This turn allows only `operation: \"fetch\"`; `set` and `delete` will be refused."
            )
        );
        assert_eq!(uses_allowed_under("state", ToolEffect::None), None);
        assert_eq!(uses_allowed_under("state", ToolEffect::Write), None);
        assert_eq!(
            uses_allowed_under("edit_custom_tool", ToolEffect::None),
            Some("This turn allows only `action: \"schema\"`.")
        );
        assert!(uses_allowed_under("doc_open_document", ToolEffect::Read).is_some());
        assert!(uses_allowed_under("run_custom", ToolEffect::Read).is_some());
        assert_eq!(uses_allowed_under("rng", ToolEffect::None), None);
        assert_eq!(
            describe_ceiling_refusal("doc_write_file", ToolEffect::Write, ToolEffect::Read),
            "Not run: `doc_write_file` is a write call, and this turn allows only lookups (read-only tools). Nothing was changed. Carry on without it."
        );
        assert!(
            describe_ceiling_refusal("x", ToolEffect::Read, ToolEffect::None)
                .contains("tools with no effect (no lookups, no changes)")
        );
        assert!(
            describe_ceiling_refusal("x", ToolEffect::Destroy, ToolEffect::Write)
                .contains("lookups and changes, not deletions")
        );
        assert!(
            describe_ceiling_refusal("x", ToolEffect::Destroy, ToolEffect::Destroy)
                .contains("any tool")
        );
    }

    #[test]
    fn computed_effects_validate_first() {
        let l = EffectLookups::default();
        assert_eq!(
            effect_of_tool_call("state", &json!({"operation": "fetch"}), &l),
            ToolEffect::Read
        );
        assert_eq!(
            effect_of_tool_call("state", &json!({"operation": "set"}), &l),
            ToolEffect::Write
        );
        assert_eq!(
            effect_of_tool_call("state", &json!({"operation": "peek"}), &l),
            ToolEffect::Write
        );
        assert_eq!(
            effect_of_tool_call("edit_custom_tool", &json!({"action": "schema"}), &l),
            ToolEffect::None
        );
        assert_eq!(
            effect_of_tool_call(
                "edit_custom_tool",
                &json!({"mount_point": "a", "file": "b"}),
                &l
            ),
            ToolEffect::Read
        );
        assert_eq!(
            effect_of_tool_call("doc_open_document", &json!({}), &l),
            ToolEffect::Write
        );
        assert_eq!(
            effect_of_tool_call("doc_open_document", &Value::Null, &l),
            ToolEffect::Write
        );
        assert_eq!(
            effect_of_tool_call("doc_open_document", &json!({"path": "a.md"}), &l),
            ToolEffect::Read
        );
        let mut l2 = EffectLookups::default();
        l2.custom_tool_effects
            .insert("coin".into(), ToolEffect::Read);
        assert_eq!(
            effect_of_tool_call("run_custom", &json!({"tool": "coin"}), &l2),
            ToolEffect::Read
        );
        assert_eq!(
            effect_of_tool_call("run_custom", &json!({"tool": "other"}), &l2),
            ToolEffect::Write
        );
        assert_eq!(
            effect_of_tool_call("mcp_thing", &json!({}), &l2),
            ToolEffect::Write
        );
        l2.plugin_tool_effects
            .insert("mcp_thing".into(), ToolEffect::Read);
        assert_eq!(
            effect_of_tool_call("mcp_thing", &json!({}), &l2),
            ToolEffect::Read
        );
        assert_eq!(max_effect_of_tool("state", &l2), ToolEffect::Write);
    }

    #[test]
    fn rank_helpers() {
        assert!(is_within_ceiling(ToolEffect::Read, ToolEffect::Read));
        assert!(!is_within_ceiling(ToolEffect::Write, ToolEffect::Read));
        assert_eq!(
            lower_tool_effect(None, ToolEffect::Write),
            ToolEffect::Write
        );
        assert_eq!(
            lower_tool_effect(Some(ToolEffect::Read), ToolEffect::Write),
            ToolEffect::Read
        );
        assert_eq!(
            lower_tool_effect(Some(ToolEffect::Destroy), ToolEffect::Write),
            ToolEffect::Write
        );
        assert_eq!(highest_tool_effect([]), ToolEffect::None);
        assert_eq!(
            highest_tool_effect([ToolEffect::Read, ToolEffect::Destroy, ToolEffect::None]),
            ToolEffect::Destroy
        );
        assert_eq!(
            max_tool_effect(ToolEffect::None, ToolEffect::Read),
            ToolEffect::Read
        );
    }
}
