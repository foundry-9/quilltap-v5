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

use serde_json::Value;

use crate::canonicalize::{canonicalize_universal_tools, UniversalTool};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::tools::definitions::universal_tool_by_key;

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
    pub wardrobe_create: bool,
    pub wardrobe_update: bool,
    pub wardrobe_archive: bool,
    pub agent_mode: bool,
    pub document_editing: bool,
    pub ask_carina: bool,
    /// Defaults to `true` (every character surface). The Brahma Console sets false.
    pub include_workspace_tools: bool,
    pub exclude_memory_search: bool,
    pub sql_access: bool,
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
        push_key(&mut universal, "sendMail");
        push_key(&mut universal, "listEmail");
        push_key(&mut universal, "readConversation");
        push_key(&mut universal, "upsertAnnotation");
        push_key(&mut universal, "deleteAnnotation");
    }

    // Unified search — every surface. The Brahma variant drops `memories`.
    push_key(
        &mut universal,
        if options.exclude_memory_search {
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
    if options.wardrobe_create {
        push_key(&mut universal, "wardrobeCreate");
    }
    if options.wardrobe_update {
        push_key(&mut universal, "wardrobeUpdate");
    }
    if options.wardrobe_archive {
        push_key(&mut universal, "wardrobeArchive");
    }
    if options.agent_mode {
        push_key(&mut universal, "submitFinalResponse");
    }

    if options.document_editing {
        // v4's exact document-editing push list (18 doc tools + 3 photo tools).
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
        ] {
            push_key(&mut universal, key);
        }
    }

    // Plugin tools (deferred; a no-plugin instance appends nothing).

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
    /// `None` ⇒ the legacy skip (return EMPTY tools); `Some(list)` ⇒ build + filter.
    pub disabled_tools: Option<&'a [String]>,
    pub disabled_tool_groups: &'a [String],
    pub agent_mode_enabled: bool,
    pub is_multi_character: bool,
    pub help_tools_enabled: bool,
    /// `character.canDressThemselves !== false` (already resolved by the caller).
    pub can_dress_themselves: bool,
    /// `character.canCreateOutfits !== false` (already resolved).
    pub can_create_outfits: bool,
    pub document_editing_enabled: bool,
    pub ask_carina_enabled: bool,
    /// Defaults to `true`; the Brahma Console strips the workspace bundle.
    pub include_workspace_tools: bool,
    pub exclude_memory_search: bool,
    pub sql_access: bool,
    /// Injected `checkModelSupportsTools(provider, model, userId)`.
    pub model_supports_native_tools: bool,
    /// Injected `provider.supportsWebSearch`.
    pub provider_supports_web_search: bool,
}

/// v4 `buildTools` return.
#[derive(Clone, Debug, Default)]
pub struct BuiltTools {
    pub tools: Vec<Value>,
    pub model_supports_native_tools: bool,
    pub use_native_web_search: bool,
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

    // Legacy fallback: `disabledTools === undefined` ⇒ skip tools for this
    // message (returns EMPTY, not all).
    let Some(disabled_tools) = input.disabled_tools else {
        return Ok(BuiltTools {
            tools: Vec::new(),
            model_supports_native_tools,
            use_native_web_search,
        });
    };

    let options = BuildToolsForProviderOptions {
        image_generation: input.image_profile_id.is_some(),
        image_provider_constraints: input.image_provider_constraints.clone(),
        web_search: input.allow_web_search,
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
        wardrobe_create: input.can_create_outfits,
        wardrobe_update: input.can_create_outfits,
        wardrobe_archive: input.can_create_outfits,
        agent_mode: input.agent_mode_enabled,
        document_editing: input.document_editing_enabled,
        ask_carina: input.ask_carina_enabled,
        include_workspace_tools: input.include_workspace_tools,
        exclude_memory_search: input.exclude_memory_search,
        sql_access: input.sql_access,
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
    let tools = format_tools_for_provider(input.provider, &tools);

    Ok(BuiltTools {
        tools,
        model_supports_native_tools,
        use_native_web_search,
    })
}

/// v4's tool-name extraction: `tool.function?.name || tool.name`.
fn tool_name(tool: &Value) -> Option<&str> {
    tool.get("function")
        .and_then(|f| f.get("name"))
        .and_then(Value::as_str)
        .or_else(|| tool.get("name").and_then(Value::as_str))
}

// ===========================================================================
// Destructive-tool filter (v4 lib/tools/destructive-tools.ts + the orchestrator's
// autonomous-room ceiling). Kept here so the tool_build differential can exercise
// it as pure logic alongside the slate.
// ===========================================================================

/// v4 `DESTRUCTIVE_TOOL_NAMES` — vault/file-mutating tools removed from an
/// autonomous room's per-turn slate unless the owner pre-authorized them.
pub const DESTRUCTIVE_TOOL_NAMES: &[&str] = &["doc_delete_file", "doc_delete_folder"];

/// Whether destructive tools are allowed this turn (v4's ceiling logic): the
/// user-level `destructiveToolPolicy` is a CEILING (`'always_refuse'` overrides
/// any per-room grant), and the per-room `runDestructiveToolsAllowed === 1` flag
/// must be set. Only consulted for `chatType === 'autonomous'`.
pub fn destructive_allowed(policy: &str, run_destructive_allowed: bool) -> bool {
    policy != "always_refuse" && run_destructive_allowed
}

/// Remove the destructive tools from a slate (v4's autonomous-room filter). A
/// tool with no name is kept.
pub fn filter_destructive_tools(tools: &mut Vec<Value>) {
    tools.retain(|tool| match tool_name(tool) {
        None => true,
        Some(name) => !DESTRUCTIVE_TOOL_NAMES.contains(&name),
    });
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
            document_editing: true,
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

    #[test]
    fn destructive_filter() {
        assert!(destructive_allowed("opt_in_per_room", true));
        assert!(!destructive_allowed("always_refuse", true));
        assert!(!destructive_allowed("opt_in_per_room", false));
        let opts = BuildToolsForProviderOptions {
            include_workspace_tools: false,
            document_editing: true,
            ..Default::default()
        };
        let mut tools = build_tools_for_provider(&opts);
        assert!(names(&tools).contains(&"doc_delete_file".to_string()));
        filter_destructive_tools(&mut tools);
        assert!(!names(&tools).contains(&"doc_delete_file".to_string()));
        assert!(!names(&tools).contains(&"doc_delete_folder".to_string()));
        assert!(names(&tools).contains(&"doc_read_file".to_string()));
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
