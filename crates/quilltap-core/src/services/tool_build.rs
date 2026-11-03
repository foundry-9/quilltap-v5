//! Tool-slate construction (wave 4 / W4.1g) — port of v4 `buildTools`
//! (`lib/services/chat-message/streaming.service.ts:134–307`) + the **built-in
//! half** of `buildToolsForProvider`
//! (`lib/tools/plugin-tool-builder.ts:299–560`).
//!
//! This assembles the per-turn tool array the model sees: the flag→tool-set
//! construction over the ported [`crate::tools::definitions`] catalog (b.3), the
//! individual disabled-tool filtering, and the canonical (OpenAI/universal)
//! provider shape via the ported [`crate::canonicalize`] machinery.
//!
//! ## Registry-seam inputs (the `getModelContextLimit` precedent)
//!
//! Two provider-capability lookups that v4 performs inline are **injected** here,
//! because they read the async pricing cache / the provider plugin (host-side
//! boundaries, W4.7): `checkModelSupportsTools(provider, model, userId)` → the
//! [`BuildToolsInput::model_supports_native_tools`] boolean, and
//! `provider.supportsWebSearch` → [`BuildToolsInput::provider_supports_web_search`].
//! The differential mocks both, mirroring these inputs.
//!
//! ## Deferrals (documented boundaries)
//!
//! - **The plugin tool registry.** v4's `buildToolsForProvider` appends
//!   `toolRegistry.getConfiguredToolDefinitions(toolConfigs)` (defaults on) and
//!   the disabled-**group** filter consults each plugin's `getToolHierarchy`. The
//!   plugin registry is unported, so for a **no-plugin instance** (the corpus,
//!   and the jest oracle which registers no plugins) this map contributes
//!   nothing: the plugin push is empty, and — because every built-in tool has
//!   **empty** [`ToolSourceMetadata`] — the group patterns never match a built-in
//!   name, so only the individual `disabledTools` ids filter the slate. We still
//!   READ [`crate::db::plugin_config::find_by_user_id`] (the repo IS ported) to
//!   build the `toolConfigs` map faithfully, then document it is unused for the
//!   built-in slate.
//! - **The provider `formatTools` reshape.** v4 hands `canonicalTools` to the
//!   provider plugin's `formatTools` (e.g. Anthropic's tool shape). The provider
//!   registry is unported, so v4's `getProvider(...)===null` fallback path —
//!   "return tools in OpenAI (universal) format" — is exactly what this port
//!   emits (`canonicalize_universal_tools`). The provider manifest (W4.7) wires
//!   the per-provider reshape.
//! - **Image-provider constraint enrichment.** `applyImageConstraintsToTool` is
//!   ported (pure, unit-tested), but its input `getImageProviderConstraints`
//!   reads the provider registry → so it is the injected
//!   [`BuildToolsInput::image_provider_constraints`] (default `None`, i.e. the
//!   base image tool). The corpus/oracle run an empty provider registry, so v4
//!   returns `null` there too.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::canonicalize::{canonicalize_universal_tools, UniversalTool};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::pascal::roster::DiscoveredCustomTool;
use crate::tools::definitions::universal_tool_by_key;
use crate::tools::inventory_tool_flags::{
    enabled_inventory_tool_keys, inventory_tool_flags, InventoryToolFlags,
};
use crate::tools::run_custom::build_run_custom_description;

/// v4 `ImageProviderConstraints` (the two fields `applyImageConstraintsToTool`
/// reads). Provider-registry-sourced (W4.7) → injected.
#[derive(Clone, Debug, Default)]
pub struct ImageProviderConstraints {
    pub prompting_guidance: Option<String>,
    pub prompt_constraint_warning: Option<String>,
}

/// v4 `applyImageConstraintsToTool` — enrich the image-generation tool's
/// description (prompting guidance) + its `prompt` parameter description (length/
/// format warning). Returns the tool unchanged when there is nothing to add.
pub fn apply_image_constraints_to_tool(
    base: UniversalTool,
    constraints: Option<&ImageProviderConstraints>,
) -> UniversalTool {
    let Some(constraints) = constraints else {
        return base;
    };
    let has_guidance = constraints
        .prompting_guidance
        .as_deref()
        .is_some_and(|s| !s.is_empty());
    let has_warning = constraints
        .prompt_constraint_warning
        .as_deref()
        .is_some_and(|s| !s.is_empty());
    if !has_guidance && !has_warning {
        return base;
    }

    let mut tool = base;

    if has_guidance {
        let existing = &tool.function.description;
        tool.function.description = format!(
            "{existing}\n\n**Provider-Specific Prompting Guidance:**\n{}",
            constraints.prompting_guidance.as_deref().unwrap_or("")
        );
    }

    if has_warning {
        // `functionDef.parameters.properties.prompt.description += ' ' + warning`.
        if let Some(props) = tool
            .function
            .parameters
            .get_mut("properties")
            .and_then(Value::as_object_mut)
        {
            let existing = props
                .get("prompt")
                .and_then(|p| p.get("description"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let prompt = props
                .entry("prompt".to_string())
                .or_insert_with(|| Value::Object(Default::default()));
            if let Some(pobj) = prompt.as_object_mut() {
                pobj.insert(
                    "description".to_string(),
                    Value::String(format!(
                        "{existing} {}",
                        constraints
                            .prompt_constraint_warning
                            .as_deref()
                            .unwrap_or("")
                    )),
                );
            }
        }
    }

    tool
}

/// v4 `ToolSourceMetadata` — the plugin/subgroup a tool came from. Built-in tools
/// carry the empty value; the populated variant is a plugin-registry surface
/// (deferred), so it is unused here but kept so [`is_tool_disabled`] mirrors v4.
#[derive(Clone, Debug, Default)]
pub struct ToolSourceMetadata {
    pub plugin_name: Option<String>,
    pub subgroup_id: Option<String>,
}

/// v4 `isToolDisabled` — individual-id + plugin-group-pattern disabling.
/// `request_full_context` is never disabled (a critical safety valve). For a
/// built-in tool `source_metadata` is empty, so only the individual-id branch can
/// match.
pub fn is_tool_disabled(
    tool_id: &str,
    disabled_tools: &[String],
    disabled_tool_groups: &[String],
    source_metadata: &ToolSourceMetadata,
) -> bool {
    if tool_id == "request_full_context" {
        return false;
    }
    if disabled_tools.iter().any(|t| t == tool_id) {
        return true;
    }
    if let Some(plugin_name) = &source_metadata.plugin_name {
        if disabled_tool_groups
            .iter()
            .any(|g| g == &format!("plugin:{plugin_name}"))
        {
            return true;
        }
        if let Some(subgroup_id) = &source_metadata.subgroup_id {
            if disabled_tool_groups
                .iter()
                .any(|g| g == &format!("plugin:{plugin_name}:subgroup:{subgroup_id}"))
            {
                return true;
            }
        }
    }
    false
}

/// v4 `DocToolsMode` (`plugin-tool-builder.ts`, `d1c06cd9d`) — which `doc_*`
/// tools a surface is offered. `Full` is the whole family — reads, writes,
/// document-UI and photo tools (chat surfaces with linked stores, the Brahma
/// Console). `Read` is the five read-only tools alone (`doc_read_file`,
/// `doc_grep`, `doc_list_files`, `doc_read_frontmatter`, `doc_read_heading`)
/// for a surface with no chat row to open documents in and nothing it may
/// write (the Scenario Builder). `Off` (v4's `?? 'off'` default) builds none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocToolsMode {
    #[default]
    Off,
    Read,
    Full,
}

/// v4 `BuildToolsExtras` (`streaming.service.ts`, `d1c06cd9d`) — surface-specific
/// narrowing for [`build_tools`]. "Every field only ever removes tools relative
/// to the positional flags and the profile." `Default` is v4's ABSENT bag.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildToolsExtras {
    /// Only these plugin tools are built (e.g. `["curl"]`); `None` = every
    /// configured plugin tool, and `Some(vec![])` admits none — absent is not
    /// empty. ⚠ v5 builds NO plugin tools (the standing non-port on
    /// [`BuildToolsForProviderOptions`]), so this field has no effect here; it
    /// is carried so a caller states v4's intent, and the admission point in
    /// [`build_tools_for_provider`] names where it would bite.
    pub plugin_tool_allowlist: Option<Vec<String>>,
    /// Build the documents/knowledge-only `search` variant (the Scenario
    /// Builder's). Takes precedence over `exclude_memory_search`.
    pub documents_only_search: bool,
    /// `Some(false)` withholds `search_web` even when the profile allows it.
    /// It can never GRANT search to a profile that forbids it: v4 computes
    /// `!!connectionProfile.allowWebSearch && extras?.webSearch !== false`.
    pub web_search: Option<bool>,
}

/// The `buildToolsForProvider` option set (built-in surface only). Mirrors v4's
/// `BuildToolsOptions`; the plugin-only `includePluginTools` / `toolConfigs` are
/// omitted (deferred). `rng`/`state` are always the workspace defaults (v4 leaves
/// them unset → `!== false` → on when `include_workspace_tools`).
#[derive(Clone, Debug, Default)]
pub struct BuildToolsForProviderOptions {
    pub image_generation: bool,
    pub image_provider_constraints: Option<ImageProviderConstraints>,
    pub web_search: bool,
    pub project_info: bool,
    pub request_full_context: bool,
    pub help_search: bool,
    pub help_settings: bool,
    pub help_navigate: bool,
    pub whisper: bool,
    pub wardrobe_list: bool,
    pub wardrobe_read: bool,
    pub wardrobe_wear: bool,
    pub wardrobe_take_off: bool,
    pub wardrobe_rollback: bool,
    pub wardrobe_history: bool,
    pub wardrobe_create: bool,
    pub wardrobe_update: bool,
    pub wardrobe_archive: bool,
    /// The nine `inventory_*` flags (v4 `BuildToolsOptions extends
    /// InventoryToolFlags`, `e1872c94f`), all set from the character's
    /// `canManageInventory` by [`build_tools`].
    pub inventory_list: bool,
    pub inventory_read: bool,
    pub inventory_equip: bool,
    pub inventory_unequip: bool,
    pub inventory_pick_up: bool,
    pub inventory_create: bool,
    pub inventory_update: bool,
    pub inventory_archive: bool,
    pub inventory_give: bool,
    pub agent_mode: bool,
    /// v4 `docToolsMode` (`d1c06cd9d` — it replaced `documentEditing: boolean`).
    pub doc_tools_mode: DocToolsMode,
    /// v4 `pluginToolAllowlist` — see [`BuildToolsExtras::plugin_tool_allowlist`].
    pub plugin_tool_allowlist: Option<Vec<String>>,
    /// v4 `documentsOnlySearch` — the Scenario Builder `search` variant. Takes
    /// precedence over `exclude_memory_search`.
    pub documents_only_search: bool,
    pub ask_carina: bool,
    /// Defaults to `true` (every character surface). The Brahma Console sets false.
    pub include_workspace_tools: bool,
    pub exclude_memory_search: bool,
    pub sql_access: bool,
    /// The caller's freshly-resolved Pascal roster. `run_custom` is offered ONLY
    /// when this is non-empty — an empty roster would hand the model a tool with
    /// nothing to run, worse than not offering it. The description is composed
    /// per call from exactly this roster (v4 `plugin-tool-builder.ts:413`).
    pub custom_tools: Vec<DiscoveredCustomTool>,
    /// v4 `editCustomTool` (`streaming.service.ts:319-321`): the editor rides on
    /// the same switch as the roster, not on the roster's contents — an empty
    /// table is exactly when a first tool needs writing.
    pub edit_custom_tool: bool,
}

fn push_key(tools: &mut Vec<UniversalTool>, key: &str) {
    if let Some(t) = universal_tool_by_key(key) {
        tools.push(t);
    }
}

/// v4 `buildToolsForProvider` (built-in half). Builds the universal-tool array by
/// flag, canonicalizes it (alpha by name via `localeCompare` + deep key-sort),
/// and returns the canonical (OpenAI/universal) shape as JSON values — the
/// `getProvider(...)===null` fallback path (the provider `formatTools` reshape is
/// the W4.7 deferral). Returns `[]` when nothing is enabled.
pub fn build_tools_for_provider(options: &BuildToolsForProviderOptions) -> Vec<Value> {
    let mut universal: Vec<UniversalTool> = Vec::new();

    if options.image_generation {
        if let Some(base) = universal_tool_by_key("imageGeneration") {
            universal.push(apply_image_constraints_to_tool(
                base,
                options.image_provider_constraints.as_ref(),
            ));
        }
    }
    if options.web_search {
        push_key(&mut universal, "webSearch");
    }
    if options.project_info {
        push_key(&mut universal, "projectInfo");
    }
    if options.request_full_context {
        push_key(&mut universal, "requestFullContext");
    }
    if options.help_search {
        push_key(&mut universal, "helpSearch");
    }
    if options.help_settings {
        push_key(&mut universal, "helpSettings");
    }
    if options.help_navigate {
        push_key(&mut universal, "helpNavigate");
    }

    // The "workspace" bundle — always on for character surfaces, stripped for the
    // character-less Brahma Console. rng/state are unset in v4 (→ `!== false`).
    let include_workspace = options.include_workspace_tools;
    if include_workspace {
        push_key(&mut universal, "rng");
        push_key(&mut universal, "state");
        push_key(&mut universal, "selfInventory");
        // Post Office tools are always available — mail is ungated (any character
        // may write to any character, and a character may always list, read and
        // discard its own post, whatever its systemTransparency). v4's order:
        // send, list, read, discard (`39bc98ffc`/`12c336fad`).
        push_key(&mut universal, "sendMail");
        push_key(&mut universal, "listMail");
        push_key(&mut universal, "readMail");
        push_key(&mut universal, "discardMail");
        push_key(&mut universal, "readConversation");
        push_key(&mut universal, "upsertAnnotation");
        push_key(&mut universal, "deleteAnnotation");
        // The log — always offered to characters; the per-chat tool settings can
        // switch each one off. (P4.D331 — v4 `plugin-tool-builder.ts:507-511`.)
        push_key(&mut universal, "logCreateEntry");
        push_key(&mut universal, "logGrep");
        push_key(&mut universal, "logRead");
    }

    // Unified search — every surface. The Brahma variant drops `memories`; the
    // Scenario Builder variant narrows further to documents/knowledge and takes
    // precedence (v4 `d1c06cd9d`'s three-way pick). All three share the wire
    // name `search`.
    push_key(
        &mut universal,
        if options.documents_only_search {
            "searchScriptoriumScenario"
        } else if options.exclude_memory_search {
            "searchScriptoriumBrahma"
        } else {
            "searchScriptorium"
        },
    );

    if options.sql_access {
        push_key(&mut universal, "runSql");
    }

    if include_workspace {
        push_key(&mut universal, "terminalList");
        push_key(&mut universal, "terminalRead");
    }

    if options.whisper {
        push_key(&mut universal, "whisper");
    }
    if options.ask_carina {
        push_key(&mut universal, "askCarina");
    }
    if options.wardrobe_list {
        push_key(&mut universal, "wardrobeList");
    }
    if options.wardrobe_read {
        push_key(&mut universal, "wardrobeRead");
    }
    if options.wardrobe_wear {
        push_key(&mut universal, "wardrobeWear");
    }
    if options.wardrobe_take_off {
        push_key(&mut universal, "wardrobeTakeOff");
    }
    if options.wardrobe_rollback {
        push_key(&mut universal, "wardrobeRollback");
    }
    if options.wardrobe_history {
        push_key(&mut universal, "wardrobeHistory");
    }
    if options.wardrobe_create {
        push_key(&mut universal, "wardrobeCreate");
    }
    if options.wardrobe_update {
        push_key(&mut universal, "wardrobeUpdate");
    }
    if options.wardrobe_archive {
        push_key(&mut universal, "wardrobeArchive");
    }
    // Inventory tools (gated by canManageInventory), in offer order — after the
    // wardrobe, before `submit_final_response` (v4 `plugin-tool-builder.ts:578-580`).
    let inventory = InventoryToolFlags {
        list: options.inventory_list,
        read: options.inventory_read,
        equip: options.inventory_equip,
        unequip: options.inventory_unequip,
        pick_up: options.inventory_pick_up,
        create: options.inventory_create,
        update: options.inventory_update,
        archive: options.inventory_archive,
        give: options.inventory_give,
    };
    for key in enabled_inventory_tool_keys(&inventory) {
        push_key(&mut universal, key);
    }
    if options.agent_mode {
        push_key(&mut universal, "submitFinalResponse");
    }

    // Document tools (Scriptorium Phase 3.3). `Read` builds the read-only five
    // and nothing else — no writes, no document-UI tools (they need a real chat
    // row), no photo tools. Push order is v4's (wire-visible pre-canonicalization).
    if options.doc_tools_mode == DocToolsMode::Read {
        for key in [
            "docReadFile",
            "docGrep",
            "docListFiles",
            "docReadFrontmatter",
            "docReadHeading",
        ] {
            push_key(&mut universal, key);
        }
    } else if options.doc_tools_mode == DocToolsMode::Full {
        // v4's exact document-editing push list (18 doc tools + 4 photo tools —
        // describe_image joined at a14a1811, bug 92, immediately after
        // attach_image; the order is wire-visible pre-canonicalization).
        // Note: doc_move_folder + the blob doc tools are NOT in this slate even
        // though they exist in the catalog — faithful to v4's push order.
        for key in [
            "docReadFile",
            "docWriteFile",
            "docStrReplace",
            "docInsertText",
            "docGrep",
            "docListFiles",
            "docReadFrontmatter",
            "docUpdateFrontmatter",
            "docReadHeading",
            "docUpdateHeading",
            "docMoveFile",
            "docCopyFile",
            "docDeleteFile",
            "docCreateFolder",
            "docDeleteFolder",
            "docOpenDocument",
            "docCloseDocument",
            "docFocus",
            "keepImage",
            "listImages",
            "attachImage",
            "describeImage",
        ] {
            push_key(&mut universal, key);
        }
    }

    // Add run_custom only when this scene actually has custom tools. An empty
    // roster would offer the model a tool with nothing to run — worse than not
    // offering it. The description is composed here, per call, from the roster
    // the caller just resolved (v4 `plugin-tool-builder.ts:413`).
    if !options.custom_tools.is_empty() {
        if let Some(mut t) = universal_tool_by_key("runCustom") {
            t.function.description = build_run_custom_description(&options.custom_tools);
            universal.push(t);
        }
    }

    // `edit_custom_tool` follows `run_custom` (v4 `plugin-tool-builder.ts:480-482`;
    // the canonicalisation below sorts the slate by name, so push order is moot).
    // It is offered on the roster SWITCH, so an EMPTY roster offers it where
    // `run_custom` is withheld.
    if options.edit_custom_tool {
        push_key(&mut universal, "editCustomTool");
    }

    // Plugin tools (deferred; a no-plugin instance appends nothing). v4 filters
    // `toolRegistry.getConfiguredToolDefinitions(toolConfigs)` by
    // `pluginToolAllowlist` right here (`allowlist ? filter(name ∈ set) : all`,
    // pushed only when non-empty) — `options.plugin_tool_allowlist` is read at
    // this point in v4 and would narrow the plugin push here; with no plugin
    // tools built on v5 (the standing non-port, §R.4(k) of P4.D216) there is
    // nothing for it to admit or refuse. Named, not taken.

    if universal.is_empty() {
        return Vec::new();
    }

    canonicalize_universal_tools(&universal)
        .into_iter()
        .map(|t| serde_json::to_value(t).expect("UniversalTool serializes"))
        .collect()
}

/// The provider `formatTools` reshape (W4.7c) — v4 `buildToolsForProvider`'s
/// step 3-5: hand the canonical (OpenAI/universal) tools to the provider plugin's
/// `formatTools`, reshaping to the provider's native tool definition (Anthropic
/// `input_schema`, Google `parameters`, OpenAI passthrough) via the manifest
/// registry. This CLOSES the W4.1g provider-reshape deferral in the sense that the
/// reshape logic is now ported + differentially verified
/// (`tool_wire_equivalence`).
///
/// **Wired into [`build_tools`]** (Round-3 unification pass): the reshape is the
/// final step of `build_tools`, so the orchestrator spine sends provider-shaped
/// tools at the wire. The `orchestrator_tier3` oracle initializes the real
/// provider registry so v4's `buildToolsForProvider` reshapes the same way. The
/// OPENAI-only `tool_build_equivalence` corpus stays green because OPENAI passes
/// through byte-identically.
pub fn format_tools_for_provider(provider: &str, canonical: &[Value]) -> Vec<Value> {
    crate::model::tool_wire::format_tools_for_provider(
        crate::provider_manifest::Registry::built_in(),
        provider,
        canonical,
    )
}

/// v4 `buildTools` input. The connection-profile fields the flag region reads,
/// the per-turn flags computed in the orchestrator, and the two injected
/// registry-seam capabilities.
pub struct BuildToolsInput<'a> {
    /// `connectionProfile.provider` (documentary — the provider `formatTools`
    /// reshape is deferred, so the emitted shape is provider-independent).
    pub provider: &'a str,
    /// `connectionProfile.useNativeWebSearch`.
    pub use_native_web_search: bool,
    /// `connectionProfile.allowToolUse` (nullable — `Some(false)` short-circuits).
    pub allow_tool_use: Option<bool>,
    /// `connectionProfile.allowWebSearch`.
    pub allow_web_search: bool,
    /// `imageProfileId` (present ⇒ image generation gate on).
    pub image_profile_id: Option<&'a str>,
    /// Injected image-provider constraints (default `None` — base image tool).
    pub image_provider_constraints: Option<ImageProviderConstraints>,
    /// `chat.projectId` (present ⇒ project_info gate on).
    pub project_id: Option<&'a str>,
    /// Compression enabled ⇒ request_full_context gate on.
    pub request_full_context: bool,
    /// The individually disabled tool ids. `None` reads as v4's default `[]`
    /// (`a8214b253` retired the legacy "no tools" skip).
    pub disabled_tools: Option<&'a [String]>,
    pub disabled_tool_groups: &'a [String],
    pub agent_mode_enabled: bool,
    pub is_multi_character: bool,
    pub help_tools_enabled: bool,
    /// `character.canDressThemselves !== false` (already resolved by the caller).
    pub can_dress_themselves: bool,
    /// `character.canCreateOutfits !== false` (already resolved).
    pub can_create_outfits: bool,
    /// `character.canManageInventory !== false` (already resolved) — v4
    /// `buildTools`' positional `canManageInventory` (index 12, `e1872c94f`).
    /// Turns the nine `inventory_*` tools on together.
    pub can_manage_inventory: bool,
    /// v4 `buildTools`' positional `docToolsMode` (index 13; `d1c06cd9d`
    /// replaced the `documentEditingEnabled` boolean there).
    pub doc_tools_mode: DocToolsMode,
    pub ask_carina_enabled: bool,
    /// Defaults to `true`; the Brahma Console strips the workspace bundle.
    pub include_workspace_tools: bool,
    pub exclude_memory_search: bool,
    pub sql_access: bool,
    /// Injected `checkModelSupportsTools(provider, model, userId)`.
    pub model_supports_native_tools: bool,
    /// Injected `provider.supportsWebSearch`.
    pub provider_supports_web_search: bool,
    /// Pascal's custom-tool roster perspective — the RESPONDING character's,
    /// since a character-tier store shadows the farther tiers. `Some` offers
    /// `run_custom`; `None` (the `customTools` setting is off, or the surface has
    /// no roster) leaves it unbuilt. The roster is resolved INSIDE `build_tools`,
    /// at the last possible moment, so a Courier turn / `allowToolUse:false` /
    /// the legacy `disabledTools===undefined` bail all return before a single
    /// mount is listed (v4 `streaming.service.ts:186`).
    pub custom_tool_context: Option<crate::pascal::roster::RosterContext>,
    /// v4 `buildTools`' trailing `extras` (`d1c06cd9d`) — surface-specific
    /// narrowing, applied at construction (never a post-filter).
    /// `BuildToolsExtras::default()` is v4's absent bag.
    pub extras: BuildToolsExtras,
    // === P4.D343 (unit E): the turn's effect ceiling ===
    /// v4 `BuildToolsOptions.effectCeiling`: the turn's effect ceiling (a
    /// storyteller turn, an autonomous room without destructive permission). A
    /// tool whose highest effect is above it is dropped; a computed tool with a
    /// note for that ceiling stays listed with the note appended. The executor's
    /// check is the guarantee; this only spares the model offers it can't use.
    /// Applied LAST, after every other filter.
    pub effect_ceiling: Option<crate::tools::effects::ToolEffect>,
}

/// v4 `buildTools` return.
#[derive(Clone, Debug, Default)]
pub struct BuiltTools {
    pub tools: Vec<Value>,
    pub model_supports_native_tools: bool,
    pub use_native_web_search: bool,
    // === P4.D343 (unit E) ===
    /// Custom tool name → derived effect, for the roster offered this turn
    /// (empty when none) — v4 `BuiltTools.customToolEffects`.
    pub custom_tool_effects: BTreeMap<String, crate::tools::effects::ToolEffect>,
    /// Plugin / MCP tool name → declared effect — v4
    /// `BuiltTools.pluginToolEffects`. Always empty: v5 has no tool-plugin
    /// registry, so v4's `toolRegistry.getToolEffects` and its WARN have no
    /// counterpart (`PLUGIN_TOOL_EFFECTS_EMPTY`, ruling R-2).
    pub plugin_tool_effects: BTreeMap<String, crate::tools::effects::ToolEffect>,
}

/// v4 `buildTools`. Composes the provider gates, the `allowToolUse === false` and
/// `disabledTools === undefined` short-circuits, the (deferred-but-read) plugin
/// config load, the built-in slate, and the individual disabled-tool filter.
pub fn build_tools(db: &Db, user_id: &str, input: &BuildToolsInput) -> Result<BuiltTools, DbError> {
    // Native web search requires BOTH the profile setting AND provider support.
    let use_native_web_search = input.use_native_web_search && input.provider_supports_web_search;
    let model_supports_native_tools = input.model_supports_native_tools;

    // Profile-level tool override — `allowToolUse === false` skips all tools.
    if input.allow_tool_use == Some(false) {
        return Ok(BuiltTools {
            tools: Vec::new(),
            model_supports_native_tools,
            use_native_web_search,
            // v4: every early return returns the (still empty) maps.
            ..Default::default()
        });
    }

    // Fetch the user's plugin tool configs (ported repo). The resulting map is
    // UNUSED for the built-in slate (plugin tools deferred) — read for
    // faithfulness with v4's DB access and to exercise the repo. A read error is
    // swallowed (v4 warns + continues with an empty map).
    let user_id_owned = user_id.to_string();
    let _tool_configs = db
        .read_main(move |c| crate::db::plugin_config::find_by_user_id(c, &user_id_owned))
        .unwrap_or_default();

    // v4 `a8214b253` dropped the legacy `disabledTools === undefined` bail: the
    // options object defaults `disabledTools = []`, so an absent list builds the
    // slate unfiltered (no production caller passes `None`; measured by the
    // re-aimed `tool_build_equivalence` row `disabled_tools_undefined`).
    let disabled_tools: &[String] = input.disabled_tools.unwrap_or(&[]);

    // Pascal's custom pseudo-tools. Resolved ONCE per turn, here at the last
    // possible moment — every early return above skips it entirely. A roster that
    // comes back empty (or a resolve that throws — a broken vault must not take
    // the whole turn down) leaves `run_custom` unbuilt. Deliberately NOT cached:
    // a `.tool.json` edited mid-chat is live on the next call (v4
    // `streaming.service.ts:240`).
    let edit_custom_tool = input.custom_tool_context.is_some();
    let custom_tools = match &input.custom_tool_context {
        Some(ctx) => db
            .read_main(|main| {
                db.read_mount_index(|mount| {
                    Ok(crate::pascal::roster::resolve_custom_tool_roster(
                        ctx,
                        main,
                        mount,
                        // Building the slate for a turn: the gates read the wall
                        // clock here, and the RUN that may follow takes its own
                        // reading. v4 does the same — `resolveCustomToolRoster`
                        // and the handler each call `Date.now()` — so a cannon
                        // that finishes charging between the two is offered and
                        // then rolls as charged, which is the honest order.
                        crate::clock::now_unix_ms(),
                    ))
                })
            })
            .map(|roster| roster.tools.into_iter().map(|(_, t)| t).collect())
            .unwrap_or_default(),
        None => Vec::new(),
    };
    // P4.D343: v4 `customToolEffectMap(customTools)` keyed by `definition.name`.
    let custom_tool_effects = crate::pascal::tool_effect::custom_tool_effect_map(
        custom_tools
            .iter()
            .map(|t: &DiscoveredCustomTool| (t.definition.name.as_str(), &t.definition)),
    );

    // v4 `streaming.service.ts:309` `...inventoryToolFlags(canManageInventory)`.
    let inventory = inventory_tool_flags(Some(input.can_manage_inventory));
    let options = BuildToolsForProviderOptions {
        image_generation: input.image_profile_id.is_some(),
        image_provider_constraints: input.image_provider_constraints.clone(),
        // A surface may narrow web search further (never widen it past the
        // profile): v4 `!!connectionProfile.allowWebSearch && extras?.webSearch
        // !== false`.
        web_search: input.allow_web_search && input.extras.web_search != Some(false),
        project_info: input.project_id.is_some(),
        request_full_context: input.request_full_context,
        help_search: input.help_tools_enabled,
        help_settings: input.help_tools_enabled,
        help_navigate: input.help_tools_enabled,
        whisper: input.is_multi_character,
        wardrobe_list: input.can_dress_themselves,
        wardrobe_read: input.can_dress_themselves,
        wardrobe_wear: input.can_dress_themselves,
        wardrobe_take_off: input.can_dress_themselves,
        wardrobe_rollback: input.can_dress_themselves,
        wardrobe_history: input.can_dress_themselves,
        wardrobe_create: input.can_create_outfits,
        wardrobe_update: input.can_create_outfits,
        wardrobe_archive: input.can_create_outfits,
        inventory_list: inventory.list,
        inventory_read: inventory.read,
        inventory_equip: inventory.equip,
        inventory_unequip: inventory.unequip,
        inventory_pick_up: inventory.pick_up,
        inventory_create: inventory.create,
        inventory_update: inventory.update,
        inventory_archive: inventory.archive,
        inventory_give: inventory.give,
        agent_mode: input.agent_mode_enabled,
        doc_tools_mode: input.doc_tools_mode,
        plugin_tool_allowlist: input.extras.plugin_tool_allowlist.clone(),
        documents_only_search: input.extras.documents_only_search,
        ask_carina: input.ask_carina_enabled,
        include_workspace_tools: input.include_workspace_tools,
        exclude_memory_search: input.exclude_memory_search,
        sql_access: input.sql_access,
        custom_tools,
        edit_custom_tool,
    };
    let mut tools = build_tools_for_provider(&options);

    // Filter disabled tools (individual ids; group patterns only bite plugin
    // tools, which a no-plugin instance never has → built-in tools keep empty
    // source metadata).
    let has_disabled_tools = !disabled_tools.is_empty();
    let has_disabled_groups = !input.disabled_tool_groups.is_empty();
    if has_disabled_tools || has_disabled_groups {
        let source_metadata = ToolSourceMetadata::default();
        tools.retain(|tool| {
            let name = tool_name(tool);
            match name {
                None => true, // keep tools without names
                Some(name) => !is_tool_disabled(
                    name,
                    disabled_tools,
                    input.disabled_tool_groups,
                    &source_metadata,
                ),
            }
        });
    }

    // Provider reshape (v4 `buildToolsForProvider` step 3-5 / `plugin.formatTools`):
    // hand the canonical (universal/OpenAI) slate to the provider manifest's
    // reshape (Anthropic `input_schema`, Google `parameters`, OpenAI passthrough).
    // The disabled/destructive filters above read `function.name`, so the reshape
    // is the LAST step (matching v4). OPENAI/unknown providers pass through
    // byte-identically (proven by `tool_wire_equivalence`), so the OPENAI-only
    // `tool_build_equivalence` corpus stays green.
    let mut tools = format_tools_for_provider(input.provider, &tools);

    // === P4.D343 (unit E): the effect ceiling, LAST (v4 `streaming.service.ts:
    // 476-478` — after the disabled filter, on the provider-shaped list, whose
    // name slot `toolNameOf` reads either way).
    let plugin_tool_effects = BTreeMap::new();
    if let Some(ceiling) = input.effect_ceiling {
        tools = apply_effect_ceiling_to_tools(
            tools,
            ceiling,
            &crate::tools::effects::EffectLookups {
                custom_tool_effects: custom_tool_effects.clone(),
                plugin_tool_effects: BTreeMap::new(),
            },
        );
    }

    Ok(BuiltTools {
        tools,
        model_supports_native_tools,
        use_native_web_search,
        custom_tool_effects,
        plugin_tool_effects,
    })
}

/// v4's PRIVATE `applyEffectCeilingToTools` (`streaming.service.ts:152-185`,
/// P4.D343): per tool, `max_effect_of_tool` within the ceiling → kept; else a
/// computed spec with a note for that ceiling → kept with `\n\n<note>` appended
/// to `function.description` (or the top-level `description`); else dropped. A
/// nameless tool is kept. DEBUG `Effect ceiling applied to tool list` with
/// `dropped` = the dropped names in slate order.
pub fn apply_effect_ceiling_to_tools(
    tools: Vec<Value>,
    ceiling: crate::tools::effects::ToolEffect,
    lookups: &crate::tools::effects::EffectLookups,
) -> Vec<Value> {
    use crate::tools::effects::{is_within_ceiling, max_effect_of_tool, uses_allowed_under};
    let mut kept: Vec<Value> = Vec::with_capacity(tools.len());
    let mut dropped: Vec<String> = Vec::new();
    for mut tool in tools {
        let Some(name) = tool_name(&tool).map(str::to_string) else {
            kept.push(tool);
            continue;
        };
        if is_within_ceiling(max_effect_of_tool(&name, lookups), ceiling) {
            kept.push(tool);
            continue;
        }
        let Some(note) = uses_allowed_under(&name, ceiling) else {
            dropped.push(name);
            continue;
        };
        // v4: `${description ?? ''}\n\n${note}` on `function` when present
        // (`toolObj.function` truthy), else on the top level.
        let slot = match tool.get_mut("function") {
            Some(f) if f.is_object() => f,
            _ => &mut tool,
        };
        if let Some(obj) = slot.as_object_mut() {
            let prior = obj.get("description").and_then(Value::as_str).unwrap_or("");
            let joined = format!("{prior}\n\n{note}");
            obj.insert("description".to_string(), Value::String(joined));
        }
        kept.push(tool);
    }
    tracing::debug!(
        ceiling = ceiling.as_str(),
        kept = kept.len(),
        dropped = ?dropped,
        "Effect ceiling applied to tool list"
    );
    kept
}

/// v4's tool-name extraction: `tool.function?.name || tool.name`.
fn tool_name(tool: &Value) -> Option<&str> {
    tool.get("function")
        .and_then(|f| f.get("name"))
        .and_then(Value::as_str)
        .or_else(|| tool.get("name").and_then(Value::as_str))
}

// ===========================================================================
// The turn's effect ceiling (v4 `turn-tools.service.ts:221-238`, P4.D343).
// Replaces the curated destructive-tool list (v4 deleted
// `lib/tools/destructive-tools.ts`): an autonomous room without destructive
// permission is a `write` ceiling, so every `destroy` tool is withheld.
// ===========================================================================

/// v4 `resolveTurnEffectCeiling(chat, chatSettings, storytellerCeiling)`. A
/// non-autonomous chat takes the storyteller ceiling as is. An autonomous room
/// keeps it when the user-level `destructiveToolPolicy` is not
/// `'always_refuse'` AND the room's `runDestructiveToolsAllowed === 1`;
/// otherwise DEBUG `Autonomous room: destructive tools withheld by effect
/// ceiling` and `lower(storyteller, write)`. The caller passes v4's defaults
/// already applied (`policy ?? 'opt_in_per_room'`, the raw `=== 1`).
pub fn resolve_turn_effect_ceiling(
    chat_type: &str,
    policy: &str,
    allowed_at_room: bool,
    storyteller_ceiling: Option<crate::tools::effects::ToolEffect>,
    chat_id: &str,
) -> Option<crate::tools::effects::ToolEffect> {
    use crate::tools::effects::{lower_tool_effect, ToolEffect};
    if chat_type != "autonomous" {
        return storyteller_ceiling;
    }
    if policy != "always_refuse" && allowed_at_room {
        return storyteller_ceiling;
    }
    tracing::debug!(
        chatId = chat_id,
        policy = policy,
        allowedAtRoom = allowed_at_room,
        "Autonomous room: destructive tools withheld by effect ceiling"
    );
    Some(lower_tool_effect(storyteller_ceiling, ToolEffect::Write))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(tools: &[Value]) -> Vec<String> {
        tools
            .iter()
            .filter_map(|t| tool_name(t).map(String::from))
            .collect()
    }

    fn discovered(name: &str, description: &str) -> DiscoveredCustomTool {
        let raw = serde_json::json!({
            "name": name,
            "description": description,
            "outcomes": [{ "when": true, "message": "done", "state": "success" }],
        });
        DiscoveredCustomTool {
            definition: crate::pascal::custom_tool_types::safe_parse(&raw).unwrap(),
            tier: crate::db::tiered_mount_pool::MountTier::Character,
            mount_point_id: "m".into(),
            mount_name: "M".into(),
            definition_path: "Tools/x.tool.json".into(),
        }
    }

    #[test]
    fn run_custom_offered_only_with_a_non_empty_roster() {
        // Empty roster ⇒ no run_custom (an empty roster would offer a tool with
        // nothing to run — worse than not offering it).
        let empty = BuildToolsForProviderOptions {
            include_workspace_tools: false,
            ..Default::default()
        };
        assert!(!names(&build_tools_for_provider(&empty))
            .iter()
            .any(|n| n == "run_custom"));

        // Non-empty roster ⇒ run_custom present, its description built from the
        // roster (the same builder unit 4 differentials against v4).
        let roster = vec![discovered("unlock", "Try the lock.")];
        let opts = BuildToolsForProviderOptions {
            include_workspace_tools: false,
            custom_tools: roster.clone(),
            ..Default::default()
        };
        let tools = build_tools_for_provider(&opts);
        assert!(names(&tools).iter().any(|n| n == "run_custom"));
        let run_custom = tools
            .iter()
            .find(|t| tool_name(t) == Some("run_custom"))
            .unwrap();
        assert_eq!(
            run_custom["function"]["description"].as_str().unwrap(),
            build_run_custom_description(&roster)
        );
    }

    #[test]
    fn empty_when_no_flags() {
        // include_workspace_tools false + nothing else ⇒ only the always-on search.
        let opts = BuildToolsForProviderOptions {
            include_workspace_tools: false,
            ..Default::default()
        };
        let tools = build_tools_for_provider(&opts);
        assert_eq!(names(&tools), vec!["search".to_string()]);
    }

    #[test]
    fn workspace_bundle_and_search_variant() {
        let opts = BuildToolsForProviderOptions {
            include_workspace_tools: true,
            exclude_memory_search: true,
            ..Default::default()
        };
        let tools = build_tools_for_provider(&opts);
        let n = names(&tools);
        // Workspace bundle present, Brahma search variant, no memory search tool.
        assert!(n.contains(&"self_inventory".to_string()));
        assert!(n.contains(&"read_conversation".to_string()));
        assert!(n.contains(&"terminal_list".to_string()));
        assert!(n.contains(&"send_mail".to_string()));
        assert!(n.contains(&"search".to_string()));
        // P4.D331: the log — always offered on the workspace surfaces.
        for log in ["log_create_entry", "log_grep", "log_read"] {
            assert!(n.contains(&log.to_string()), "{log} missing");
        }
        // Sorted alphabetically by name (canonicalized).
        let mut sorted = n.clone();
        sorted.sort();
        assert_eq!(n, sorted);
    }

    #[test]
    fn wardrobe_and_agent_gates() {
        let opts = BuildToolsForProviderOptions {
            include_workspace_tools: false,
            wardrobe_list: true,
            wardrobe_create: true,
            agent_mode: true,
            ..Default::default()
        };
        let n = names(&build_tools_for_provider(&opts));
        assert!(n.contains(&"wardrobe_list".to_string()));
        assert!(n.contains(&"wardrobe_create".to_string()));
        assert!(n.contains(&"submit_final_response".to_string()));
        assert!(!n.contains(&"wardrobe_read".to_string()));
    }

    #[test]
    fn document_editing_slate() {
        let opts = BuildToolsForProviderOptions {
            include_workspace_tools: false,
            doc_tools_mode: DocToolsMode::Full,
            ..Default::default()
        };
        let n = names(&build_tools_for_provider(&opts));
        assert!(n.contains(&"doc_read_file".to_string()));
        assert!(n.contains(&"doc_delete_file".to_string()));
        assert!(n.contains(&"keep_image".to_string()));
        // Not in v4's push list.
        assert!(!n.contains(&"doc_move_folder".to_string()));
        assert!(!n.contains(&"doc_read_blob".to_string()));
    }

    /// v4 `plugin-tool-builder.scenario.test.ts` (`d1c06cd9d`) case names —
    /// the `'read'` mode builds EXACTLY the read-only five and none of the
    /// writes / document-UI / photo tools; the Scenario Builder `search`
    /// variant offers neither `memories` nor `conversations`. (Case names
    /// only — the byte-exact proof is `tool_build_equivalence` over v4's real
    /// `buildTools`.)
    #[test]
    fn read_mode_builds_the_read_only_five() {
        let opts = BuildToolsForProviderOptions {
            include_workspace_tools: false,
            doc_tools_mode: DocToolsMode::Read,
            ..Default::default()
        };
        let mut n = names(&build_tools_for_provider(&opts));
        n.retain(|t| t != "search");
        n.sort();
        assert_eq!(
            n,
            vec![
                "doc_grep",
                "doc_list_files",
                "doc_read_file",
                "doc_read_frontmatter",
                "doc_read_heading"
            ]
        );
    }

    #[test]
    fn the_scenario_search_variant_offers_no_memories_or_conversations() {
        let opts = BuildToolsForProviderOptions {
            include_workspace_tools: false,
            documents_only_search: true,
            // Precedence: documents-only wins over the Brahma variant.
            exclude_memory_search: true,
            ..Default::default()
        };
        let tools = build_tools_for_provider(&opts);
        let search = tools
            .iter()
            .find(|t| t["function"]["name"] == "search")
            .expect("a search tool is always built");
        let params = serde_json::to_string(&search["function"]["parameters"]).unwrap();
        assert!(!params.contains("memories"), "{params}");
        assert!(!params.contains("conversations"), "{params}");
        assert!(params.contains("knowledge"), "{params}");
    }

    #[test]
    fn is_tool_disabled_rules() {
        let empty = ToolSourceMetadata::default();
        assert!(!is_tool_disabled(
            "request_full_context",
            &["request_full_context".to_string()],
            &[],
            &empty
        ));
        assert!(is_tool_disabled("rng", &["rng".to_string()], &[], &empty));
        // Group patterns never disable a built-in (empty source metadata).
        assert!(!is_tool_disabled(
            "rng",
            &[],
            &["plugin:mcp".to_string()],
            &empty
        ));
    }

    /// P4.D343: the effect ceiling replaces the curated destructive list. An
    /// autonomous room without permission is a `write` ceiling, so EVERY
    /// `destroy` tool goes — the withheld set widens from three to five
    /// (`delete_annotation`, `doc_delete_blob` join) — and the harmless
    /// siblings stay.
    #[test]
    fn write_ceiling_withholds_every_destroy_tool() {
        use crate::tools::effects::{EffectLookups, ToolEffect};
        let opts = BuildToolsForProviderOptions {
            include_workspace_tools: true,
            doc_tools_mode: DocToolsMode::Full,
            ..Default::default()
        };
        let before = names(&build_tools_for_provider(&opts));
        let (kept, lines) = crate::test_support::captured_with(|| {
            apply_effect_ceiling_to_tools(
                build_tools_for_provider(&opts),
                ToolEffect::Write,
                &EffectLookups::default(),
            )
        });
        let after = names(&kept);
        let withheld: Vec<&str> = before
            .iter()
            .filter(|n| !after.contains(n))
            .map(String::as_str)
            .collect();
        // Exactly the `destroy` tools this slate carries (`doc_delete_blob` rides
        // only the blob tools' gate) — `delete_annotation` newly among them.
        let destroys: Vec<&str> = before
            .iter()
            .map(String::as_str)
            .filter(|n| {
                crate::tools::effects::max_effect_of_tool(n, &EffectLookups::default())
                    == ToolEffect::Destroy
            })
            .collect();
        assert_eq!(withheld, destroys);
        assert_eq!(
            withheld,
            [
                "delete_annotation",
                "discard_mail",
                "doc_delete_file",
                "doc_delete_folder"
            ]
        );
        for n in ["send_mail", "list_mail", "read_mail", "doc_read_file"] {
            assert!(after.contains(&n.to_string()), "{n} filtered");
        }
        let hits: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("Effect ceiling applied to tool list"))
            .collect();
        assert_eq!(hits.len(), 1, "{lines:?}");
        assert!(hits[0].starts_with("DEBUG "), "{}", hits[0]);
    }

    /// `resolve_turn_effect_ceiling`: a non-autonomous chat takes the
    /// storyteller ceiling; an autonomous room lowers to `write` (with the
    /// DEBUG) unless the policy allows AND the room is opted in.
    #[test]
    fn resolve_turn_effect_ceiling_matrix() {
        use crate::tools::effects::ToolEffect;
        let (got, lines) = crate::test_support::captured_with(|| {
            resolve_turn_effect_ceiling("salon", "always_refuse", false, None, "c")
        });
        assert_eq!(got, None);
        assert!(lines.is_empty(), "{lines:?}");
        assert_eq!(
            resolve_turn_effect_ceiling("salon", "x", false, Some(ToolEffect::Read), "c"),
            Some(ToolEffect::Read)
        );
        assert_eq!(
            resolve_turn_effect_ceiling("autonomous", "opt_in_per_room", true, None, "c"),
            None
        );
        let (got, lines) = crate::test_support::captured_with(|| {
            resolve_turn_effect_ceiling("autonomous", "opt_in_per_room", false, None, "chat-1")
        });
        assert_eq!(got, Some(ToolEffect::Write));
        let hit = lines
            .iter()
            .find(|l| l.contains("destructive tools withheld by effect ceiling"))
            .expect("the DEBUG");
        assert!(hit.starts_with("DEBUG "), "{hit}");
        assert!(
            hit.contains(" chatId=chat-1 policy=opt_in_per_room allowedAtRoom=false"),
            "{hit}"
        );
        assert_eq!(
            resolve_turn_effect_ceiling(
                "autonomous",
                "always_refuse",
                true,
                Some(ToolEffect::Read),
                "c"
            ),
            Some(ToolEffect::Read)
        );
        assert_eq!(
            resolve_turn_effect_ceiling(
                "autonomous",
                "always_refuse",
                true,
                Some(ToolEffect::Destroy),
                "c"
            ),
            Some(ToolEffect::Write)
        );
    }

    /// A computed tool with a note for the ceiling is kept with the note
    /// appended after a blank line; without a note it is dropped.
    #[test]
    fn computed_tool_notes_under_a_ceiling() {
        use crate::tools::effects::{EffectLookups, ToolEffect};
        let tools = vec![
            serde_json::json!({"type": "function", "function": {"name": "state", "description": "S"}}),
            serde_json::json!({"name": "edit_custom_tool", "description": "E"}),
            serde_json::json!({"type": "function", "function": {"name": "doc_open_document"}}),
            serde_json::json!({"type": "function", "function": {"name": "rng", "description": "R"}}),
            serde_json::json!({"type": "function"}),
        ];
        let kept = apply_effect_ceiling_to_tools(
            tools.clone(),
            ToolEffect::None,
            &EffectLookups::default(),
        );
        assert_eq!(kept.len(), 3);
        assert_eq!(
            kept[0]["description"],
            "E\n\nThis turn allows only `action: \"schema\"`."
        );
        let kept =
            apply_effect_ceiling_to_tools(tools, ToolEffect::Read, &EffectLookups::default());
        assert_eq!(kept.len(), 5);
        assert!(kept[2]["function"]["description"]
            .as_str()
            .unwrap()
            .starts_with("\n\nThis turn allows only opening"));
    }

    /// The Brahma surface (`include_workspace_tools: false`) carries none of the
    /// four Post Office tools (v4 `brahma-console.test.ts`'s `not.toContain`s).
    #[test]
    fn brahma_slate_has_no_mail_tools() {
        let opts = BuildToolsForProviderOptions {
            include_workspace_tools: false,
            ..Default::default()
        };
        let got = names(&build_tools_for_provider(&opts));
        for n in ["send_mail", "list_mail", "read_mail", "discard_mail"] {
            assert!(!got.contains(&n.to_string()), "{n} on the Brahma slate");
        }
        // P4.D331: nor the log ("The Brahma Console does not offer the Log").
        for n in ["log_create_entry", "log_grep", "log_read"] {
            assert!(!got.contains(&n.to_string()), "{n} on the Brahma slate");
        }
    }

    #[test]
    fn apply_image_constraints() {
        let base = universal_tool_by_key("imageGeneration").unwrap();
        let plain = apply_image_constraints_to_tool(base.clone(), None);
        assert_eq!(plain.function.description, base.function.description);

        let enriched = apply_image_constraints_to_tool(
            base.clone(),
            Some(&ImageProviderConstraints {
                prompting_guidance: Some("USE SHORT PROMPTS".to_string()),
                prompt_constraint_warning: Some("Max 200 chars.".to_string()),
            }),
        );
        assert!(enriched
            .function
            .description
            .contains("**Provider-Specific Prompting Guidance:**\nUSE SHORT PROMPTS"));
        let desc = enriched.function.parameters["properties"]["prompt"]["description"]
            .as_str()
            .unwrap();
        assert!(desc.ends_with("Max 200 chars."));
    }
}
