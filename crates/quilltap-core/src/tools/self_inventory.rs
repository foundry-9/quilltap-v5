//! The `self_inventory` tool handler — v4 `lib/tools/handlers/self-inventory-*`
//! (W4.1d batch 1, the largest tool handler).
//!
//! Assembles the introspection report a character can request about itself in a
//! chat: its vault files, vault access, memory/conversation stats, the assembled
//! system prompt, last-turn LLM usage, Carina reachability, the Quilltap
//! environment, and the surrounding context (chat/project/groups/characters/files).
//! Each section is wrapped so a single failing lookup yields an "unavailable"
//! marker rather than sinking the whole report — v4's per-section try/catch.
//!
//! v4 splits this across `helpers.ts` (shared primitives), `builders.ts` (the
//! GATHER half), `formatters.ts` (the RENDER half) and the orchestrator; the port
//! keeps them one module, in that order (types → env seam → gather → render →
//! orchestration).
//!
//! ## Faithful v4 shapes
//!
//! * Every section object serializes **byte-exact** to v4 — typed structs in
//!   schema field order (never a sorted `serde_json::Value`), JS number rendering
//!   ([`crate::db::js_number_to_json`] for the integer-valued floats), and the
//!   `Number.prototype.toLocaleString('en-US')` thousands grouping for the
//!   formatter's `formatNumber`. The `.localeCompare` sorts use the ported
//!   [`crate::collation::locale_compare`] where v4 sorts by `localeCompare`
//!   (vault-file `relativePath`, group name, member/participant name, Carina name,
//!   context-character name, context-file `filePath`); `.sort` is stable in JS, so
//!   ties keep input order (the DB read order the readers preserve).
//! * The dispatcher's `structuredResult` does NOT surface `result.carina` (carina
//!   appears only inside `formattedText`); [`SelfInventoryOutput`] exposes `carina`
//!   for the formatter but the dispatcher must reproduce that asymmetry.
//!
//! ## The environment seam (the ONLY host boundary)
//!
//! The `quilltap` section's runtime-mode / client-shell / release-notes / changelog
//! reads and `isMountIndexDegraded()` are **host-environment** sources, not enclave
//! or service state. They are modeled as an injected [`SelfInventoryEnv`] fed the
//! same values v4's oracle run produces:
//!
//! * `runtime_mode` + `client_shell` — cover `quilltap.version` fully.
//! * `release_notes` / `changelog` — the port supports them via the env seam, but
//!   the DIFFERENTIAL corpus requests only `quilltap.version` (release notes /
//!   changelog are file-system reads with no self-inventory-specific logic — a
//!   documented deferral; the formatter path for them is unit-tested, not
//!   differential-verified).
//! * `mount_index_degraded` — injected `false` (fixtures have a healthy mount
//!   index).
//! * `model_context_limit` — the lastTurn section's `getModelContextLimit` needs
//!   the plugin/pricing **registry** (a registry seam, as everywhere in v5); the
//!   env carries the registry rows so the port calls the ported
//!   [`crate::model_context::get_model_context_limit`] with the SAME data v4's
//!   registry holds.

use serde::Serialize;
use serde_json::Value;

use crate::collation::locale_compare;
use crate::db::runtime::Db;
use crate::db::{js_number_to_json, DbError};
use crate::folder_utils::{is_automatic_image_path, is_image_file_name, is_os_cruft_name};
use crate::format_bytes::format_bytes;
use crate::jsnum::to_fixed;
use crate::model_context::{get_model_context_limit, ModelInfo, PricingRow};
use crate::services::knowledge_injector::qtap_uri::{
    format_doc_store_uri, format_scoped_uri, format_self_uri, ScopedAuthority,
};
use crate::services::tool_execution::LoadedMemoriesContext;

const HIGH_IMPORTANCE_THRESHOLD: f64 = 0.7;

// ===========================================================================
// The environment seam
// ===========================================================================

/// A client-shell descriptor — v4 `SelfInventoryClientShell`
/// (`{ type:'electron', shellVersion } | { type:'browser' } | { type:'unknown' }`).
#[derive(Clone, Debug)]
pub enum ClientShell {
    Electron { shell_version: String },
    Browser,
    Unknown,
}

/// The injected host-environment values the `quilltap` section + the mount-index
/// degraded check + the model-context registry need. The harness feeds these the
/// same values v4's oracle run produces.
#[derive(Clone)]
pub struct SelfInventoryEnv {
    /// v4 `packageJson.version` — the Quilltap version string surfaced at the
    /// output top level (`quilltapVersion`) AND inside the `quilltap` section. A
    /// host-provided value (the v5 crate version differs from v4's package
    /// version), so it is injected here and the corpus feeds v4's value.
    pub version: String,
    /// v4 `resolveRuntimeMode()` — one of the `SelfInventoryRuntimeMode` strings.
    pub runtime_mode: String,
    /// v4 `resolveClientShell()`.
    pub client_shell: ClientShell,
    /// v4 `isMountIndexDegraded()`.
    pub mount_index_degraded: bool,
    /// The release-notes body + version, when requested (`quilltap.releaseNotes`).
    /// `None` reproduces v4's "no release notes found". Kept out of the corpus.
    pub release_notes: Option<(String, String)>,
    /// The changelog body, when requested (`quilltap.changelog`). Kept out of the
    /// corpus.
    pub changelog: Option<String>,
    /// The plugin registry model-info rows for `get_model_context_limit`.
    pub model_info: Vec<ModelInfo>,
    /// The fallback-pricing rows for `get_model_context_limit`.
    pub fallback_pricing: Vec<PricingRow>,
    /// `getDefaultContextWindow(provider)` for `get_model_context_limit`.
    pub registry_default_context: i64,
}

impl SelfInventoryEnv {
    fn model_context_limit(&self, provider: &str, model_name: &str) -> i64 {
        get_model_context_limit(
            provider,
            model_name,
            &self.model_info,
            &self.fallback_pricing,
            self.registry_default_context,
        )
    }
}

// ===========================================================================
// Output types (v4 `SelfInventoryToolOutput` + section shapes, key order = v4)
// ===========================================================================

/// A single vault file (v4 `SelfInventoryVaultFile`).
#[derive(Clone, Debug, Serialize)]
pub struct VaultFile {
    #[serde(rename = "mountPointName")]
    pub mount_point_name: String,
    #[serde(rename = "relativePath")]
    pub relative_path: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "fileType")]
    pub file_type: String,
    #[serde(rename = "fileSizeBytes")]
    pub file_size_bytes: Value,
    #[serde(rename = "lastModified")]
    pub last_modified: String,
    pub uri: String,
}

/// v4 `SelfInventoryVaultIncludedParts` / `SelfInventoryContextIncludedParts` share
/// the `{ character, groups }` shape for vault + vaultAccess.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct VaultIncludedParts {
    pub character: bool,
    pub groups: bool,
}

/// v4 `SelfInventoryVaultCharacterSection` (available / unavailable union, tagged by
/// the `available` boolean; serde `untagged` reproduces the flat JSON).
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum VaultCharacterSection {
    Available {
        available: AvailTrue,
        #[serde(rename = "mountPointName")]
        mount_point_name: String,
        #[serde(rename = "mountPointId")]
        mount_point_id: String,
        #[serde(rename = "fileCount")]
        file_count: i64,
        files: Vec<VaultFile>,
    },
    Unavailable {
        available: AvailFalse,
        reason: String,
        message: String,
    },
}

/// One group's vault listing (v4 `SelfInventoryVaultGroup`).
#[derive(Clone, Debug, Serialize)]
pub struct VaultGroup {
    #[serde(rename = "groupId")]
    pub group_id: String,
    #[serde(rename = "groupName")]
    pub group_name: String,
    #[serde(rename = "mountPointId")]
    pub mount_point_id: String,
    #[serde(rename = "mountPointName")]
    pub mount_point_name: String,
    #[serde(rename = "fileCount")]
    pub file_count: i64,
    pub files: Vec<VaultFile>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum VaultGroupsSection {
    Available {
        available: AvailTrue,
        groups: Vec<VaultGroup>,
    },
    Unavailable {
        available: AvailFalse,
        reason: String,
        message: String,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct VaultSection {
    #[serde(rename = "includedParts")]
    pub included_parts: VaultIncludedParts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<VaultCharacterSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<VaultGroupsSection>,
}

/// v4 `SelfInventoryVaultAccessParticipant`.
#[derive(Clone, Debug, Serialize)]
pub struct VaultAccessParticipant {
    #[serde(rename = "participantId")]
    pub participant_id: String,
    #[serde(rename = "characterId")]
    pub character_id: String,
    #[serde(rename = "characterName")]
    pub character_name: String,
    #[serde(rename = "controlledBy")]
    pub controlled_by: String,
    pub status: String,
    #[serde(rename = "isSelf")]
    pub is_self: bool,
    pub access: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum VaultAccessCharacterSection {
    Available {
        available: AvailTrue,
        #[serde(rename = "mountPointName")]
        mount_point_name: String,
        #[serde(rename = "sharedVaultsEnabled")]
        shared_vaults_enabled: bool,
        participants: Vec<VaultAccessParticipant>,
    },
    Unavailable {
        available: AvailFalse,
        #[serde(rename = "sharedVaultsEnabled")]
        shared_vaults_enabled: bool,
        message: String,
    },
}

/// v4 `SelfInventoryGroupVaultMember`.
#[derive(Clone, Debug, Serialize)]
pub struct GroupVaultMember {
    #[serde(rename = "characterId")]
    pub character_id: String,
    #[serde(rename = "characterName")]
    pub character_name: String,
    #[serde(rename = "isSelf")]
    pub is_self: bool,
    pub access: String,
}

/// v4 `SelfInventoryGroupVaultAccess`.
#[derive(Clone, Debug, Serialize)]
pub struct GroupVaultAccess {
    #[serde(rename = "groupId")]
    pub group_id: String,
    #[serde(rename = "groupName")]
    pub group_name: String,
    pub members: Vec<GroupVaultMember>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum VaultAccessGroupsSection {
    Available {
        available: AvailTrue,
        groups: Vec<GroupVaultAccess>,
    },
    Unavailable {
        available: AvailFalse,
        reason: String,
        message: String,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct VaultAccessSection {
    #[serde(rename = "includedParts")]
    pub included_parts: VaultIncludedParts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<VaultAccessCharacterSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<VaultAccessGroupsSection>,
}

/// v4 `SelfInventoryMemorySection`. `available` is a plain bool here (v4 always
/// emits `true` on the happy path; the handler's `.catch` produces `false`).
#[derive(Clone, Debug, Serialize)]
pub struct MemorySection {
    pub available: bool,
    #[serde(rename = "totalCount")]
    pub total_count: i64,
    #[serde(rename = "highImportanceCount")]
    pub high_importance_count: i64,
    #[serde(rename = "highImportancePercent")]
    pub high_importance_percent: Value,
    pub threshold: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// v4 `SelfInventorySemanticMemoryItem`.
#[derive(Clone, Debug, Serialize)]
pub struct SemanticMemoryItem {
    pub summary: String,
    pub importance: Value,
    pub score: Value,
    #[serde(rename = "effectiveWeight")]
    pub effective_weight: Value,
}

/// v4 `SelfInventoryInterCharacterMemoryItem`.
#[derive(Clone, Debug, Serialize)]
pub struct InterCharacterMemoryItem {
    #[serde(rename = "aboutCharacterName")]
    pub about_character_name: String,
    pub summary: String,
    pub importance: Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum LoadedMemoriesSection {
    Available {
        available: AvailTrue,
        #[serde(rename = "semanticMemories")]
        semantic_memories: Vec<SemanticMemoryItem>,
        #[serde(rename = "interCharacterMemories")]
        inter_character_memories: Vec<InterCharacterMemoryItem>,
        recap: Option<String>,
    },
    Unavailable {
        available: AvailFalse,
        message: String,
    },
}

/// v4 `SelfInventoryChatSection`.
#[derive(Clone, Debug, Serialize)]
pub struct ChatSection {
    pub available: bool,
    #[serde(rename = "chatCount")]
    pub chat_count: i64,
    #[serde(rename = "earliestCreatedAt")]
    pub earliest_created_at: Option<String>,
    #[serde(rename = "latestActivityAt")]
    pub latest_activity_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// v4 `SelfInventoryPromptSection`.
#[derive(Clone, Debug, Serialize)]
pub struct PromptSection {
    pub available: bool,
    #[serde(rename = "systemPrompt")]
    pub system_prompt: Option<String>,
    #[serde(rename = "characterCount")]
    pub character_count: i64,
    #[serde(rename = "approxTokens")]
    pub approx_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// v4 `SelfInventoryLastTurnSection`.
#[derive(Clone, Debug, Serialize)]
pub struct LastTurnSection {
    pub available: bool,
    pub source: Option<String>,
    pub provider: Option<String>,
    #[serde(rename = "modelName")]
    pub model_name: Option<String>,
    #[serde(rename = "promptTokens")]
    pub prompt_tokens: Option<i64>,
    #[serde(rename = "completionTokens")]
    pub completion_tokens: Option<i64>,
    #[serde(rename = "totalTokens")]
    pub total_tokens: Option<i64>,
    #[serde(rename = "contextWindow")]
    pub context_window: Option<i64>,
    #[serde(rename = "utilizationPercent")]
    pub utilization_percent: Option<Value>,
    #[serde(rename = "loggedAt")]
    pub logged_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// v4 `SelfInventoryCarinaReachable`.
#[derive(Clone, Debug, Serialize)]
pub struct CarinaReachable {
    pub name: String,
    #[serde(rename = "isAnswerer")]
    pub is_answerer: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum CarinaSection {
    Available {
        available: AvailTrue,
        #[serde(rename = "selfEnabled")]
        self_enabled: bool,
        reachable: Vec<CarinaReachable>,
    },
    Unavailable {
        available: AvailFalse,
        message: String,
    },
}

/// v4 `SelfInventoryQuilltapIncludedParts`.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct QuilltapIncludedParts {
    pub version: bool,
    #[serde(rename = "releaseNotes")]
    pub release_notes: bool,
    pub changelog: bool,
}

/// v4 `SelfInventoryClientShell` serialized shape.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum ClientShellJson {
    Electron {
        #[serde(rename = "type")]
        kind: ElectronTag,
        #[serde(rename = "shellVersion")]
        shell_version: String,
    },
    Browser {
        #[serde(rename = "type")]
        kind: BrowserTag,
    },
    Unknown {
        #[serde(rename = "type")]
        kind: UnknownTag,
    },
}

/// v4 `SelfInventoryQuilltapSection`.
#[derive(Clone, Debug, Serialize)]
pub struct QuilltapSection {
    pub available: bool,
    #[serde(rename = "includedParts")]
    pub included_parts: QuilltapIncludedParts,
    pub version: String,
    #[serde(rename = "runtimeMode")]
    pub runtime_mode: String,
    #[serde(rename = "clientShell")]
    pub client_shell: ClientShellJson,
    #[serde(rename = "releaseNotes")]
    pub release_notes: Option<String>,
    #[serde(rename = "releaseNotesVersion")]
    pub release_notes_version: Option<String>,
    pub changelog: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// v4 `SelfInventoryContextMount`.
#[derive(Clone, Debug, Serialize)]
pub struct ContextMount {
    #[serde(rename = "mountPointId")]
    pub mount_point_id: String,
    pub name: String,
}

/// v4 `SelfInventoryContextIncludedParts`.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct ContextIncludedParts {
    pub chat: bool,
    pub project: bool,
    pub groups: bool,
    pub characters: bool,
    pub files: bool,
}

/// v4 `SelfInventoryContextChat`.
#[derive(Clone, Debug, Serialize)]
pub struct ContextChat {
    pub available: bool,
    #[serde(rename = "chatId")]
    pub chat_id: String,
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum ContextProject {
    Present {
        available: AvailTrue,
        present: PresentTrue,
        id: String,
        name: String,
        #[serde(rename = "mountPoints")]
        mount_points: Vec<ContextMount>,
    },
    Absent {
        available: AvailTrue,
        present: PresentFalse,
    },
    Unavailable {
        available: AvailFalse,
        message: String,
    },
}

/// v4 `SelfInventoryContextGroup`.
#[derive(Clone, Debug, Serialize)]
pub struct ContextGroup {
    pub id: String,
    pub name: String,
    #[serde(rename = "mountPoints")]
    pub mount_points: Vec<ContextMount>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum ContextGroups {
    Available {
        available: AvailTrue,
        groups: Vec<ContextGroup>,
    },
    Unavailable {
        available: AvailFalse,
        message: String,
    },
}

/// v4 `SelfInventoryContextCharacter`.
#[derive(Clone, Debug, Serialize)]
pub struct ContextCharacter {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub identity: Option<String>,
    #[serde(rename = "isUserPersona")]
    pub is_user_persona: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum ContextCharacters {
    Available {
        available: AvailTrue,
        characters: Vec<ContextCharacter>,
    },
    Unavailable {
        available: AvailFalse,
        message: String,
    },
}

/// v4 `SelfInventoryContextFile`.
#[derive(Clone, Debug, Serialize)]
pub struct ContextFile {
    pub scope: String,
    #[serde(rename = "mountPoint")]
    pub mount_point: Option<String>,
    #[serde(rename = "filePath")]
    pub file_path: String,
    #[serde(rename = "displayTitle")]
    pub display_title: Option<String>,
    pub uri: String,
    #[serde(rename = "howToReach")]
    pub how_to_reach: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum ContextFiles {
    Available {
        available: AvailTrue,
        files: Vec<ContextFile>,
    },
    Unavailable {
        available: AvailFalse,
        message: String,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct ContextSection {
    #[serde(rename = "includedParts")]
    pub included_parts: ContextIncludedParts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat: Option<ContextChat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<ContextProject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<ContextGroups>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub characters: Option<ContextCharacters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<ContextFiles>,
}

/// The full tool output (v4 `SelfInventoryToolOutput`). Section fields serialize in
/// v4's declaration order; `error` trails.
#[derive(Clone, Debug, Serialize)]
pub struct SelfInventoryOutput {
    pub success: bool,
    #[serde(rename = "quilltapVersion")]
    pub quilltap_version: String,
    #[serde(rename = "characterId")]
    pub character_id: String,
    #[serde(rename = "characterName")]
    pub character_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault: Option<VaultSection>,
    #[serde(rename = "vaultAccess", skip_serializing_if = "Option::is_none")]
    pub vault_access: Option<VaultAccessSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<MemorySection>,
    #[serde(rename = "loadedMemories", skip_serializing_if = "Option::is_none")]
    pub loaded_memories: Option<LoadedMemoriesSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chats: Option<ChatSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<PromptSection>,
    #[serde(rename = "lastTurn", skip_serializing_if = "Option::is_none")]
    pub last_turn: Option<LastTurnSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carina: Option<CarinaSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quilltap: Option<QuilltapSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<ContextSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Zero-sized literal tags so serde emits the constant discriminants v4 carries.
// ---------------------------------------------------------------------------

macro_rules! literal_tag {
    ($name:ident, bool, $val:literal) => {
        #[derive(Clone, Copy, Debug)]
        pub struct $name;
        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_bool($val)
            }
        }
    };
    ($name:ident, str, $val:literal) => {
        #[derive(Clone, Copy, Debug)]
        pub struct $name;
        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str($val)
            }
        }
    };
}

literal_tag!(AvailTrue, bool, true);
literal_tag!(AvailFalse, bool, false);
literal_tag!(PresentTrue, bool, true);
literal_tag!(PresentFalse, bool, false);
literal_tag!(ElectronTag, str, "electron");
literal_tag!(BrowserTag, str, "browser");
literal_tag!(UnknownTag, str, "unknown");

// ===========================================================================
// Low-level number/date helpers (v4 `helpers.ts`)
// ===========================================================================

/// v4 `roundPercent(n) = Math.round(n * 10) / 10`, returned as a JS number
/// ([`js_number_to_json`] so `40.0` renders `40`, `33.3` stays `33.3`).
fn round_percent(n: f64) -> Value {
    // Math.round: half rounds toward +Infinity. `(n*10)` for the domains here is
    // finite and modest; `f64::round` rounds half away from zero, which differs
    // only for exact .5 of a negative — percentages are non-negative here.
    let rounded = (n * 10.0).round() / 10.0;
    js_number_to_json(rounded)
}

/// v4 `formatNumber(n) = n.toLocaleString('en-US')` — group thousands with `,`.
/// All call sites pass a non-negative integer.
fn format_number(n: i64) -> String {
    let neg = n < 0;
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    let len = digits.len();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    if neg {
        format!("-{out}")
    } else {
        out
    }
}

/// v4 `formatDate(iso) = iso.slice(0, 10)` — the leading `YYYY-MM-DD`. JS `slice`
/// is over UTF-16 code units; ISO timestamps are ASCII, so a byte slice suffices
/// (guarding a shorter string).
fn format_date(iso: &str) -> String {
    iso.chars().take(10).collect()
}

/// v4 `helpers.ts::keepVaultFile` — OS cruft always dropped; auto-generated images
/// dropped unless opted in (document-store `character-avatars`/`story-backgrounds`,
/// plus — for a character vault — a top-level `images/` image).
fn keep_vault_file(
    relative_path: &str,
    include_automatic_images: bool,
    treat_images_folder_as_generated: bool,
) -> bool {
    let segments: Vec<&str> = relative_path.split('/').collect();
    let basename = segments.last().copied().unwrap_or("");
    if is_os_cruft_name(basename) {
        return false;
    }
    if include_automatic_images {
        return true;
    }
    if is_automatic_image_path(relative_path) {
        return false;
    }
    if treat_images_folder_as_generated
        && segments.first() == Some(&"images")
        && is_image_file_name(basename)
    {
        return false;
    }
    true
}

/// v4 `helpers.ts::mapVaultFiles` — filter, map to [`VaultFile`], sort by
/// `relativePath.localeCompare`. `make_uri` is the per-store URI builder.
fn map_vault_files(
    rows: &[crate::db::doc_mount_files::VaultFileRow],
    mount_point_name: &str,
    include_automatic_images: bool,
    treat_images_folder_as_generated: bool,
    make_uri: impl Fn(&str) -> String,
) -> Vec<VaultFile> {
    let mut files: Vec<VaultFile> = rows
        .iter()
        .filter(|row| {
            keep_vault_file(
                &row.relative_path,
                include_automatic_images,
                treat_images_folder_as_generated,
            )
        })
        .map(|row| VaultFile {
            mount_point_name: mount_point_name.to_string(),
            relative_path: row.relative_path.clone(),
            file_name: row.file_name.clone(),
            file_type: row.file_type.clone(),
            file_size_bytes: js_number_to_json(row.file_size_bytes),
            last_modified: row.last_modified.clone(),
            uri: make_uri(&row.relative_path),
        })
        .collect();
    // JS `.sort` is stable; `localeCompare` on relativePath.
    files.sort_by(|a, b| locale_compare(&a.relative_path, &b.relative_path));
    files
}

// ===========================================================================
// Small JSON accessors on the vault-overlaid character / chat Values
// ===========================================================================

fn json_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn json_bool(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn json_str_array(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// `Date.parse(iso)` finite? — v4 uses this to skip un-parseable timestamps in the
/// chats section. Reuse the ported strict ISO parser (`None` = not finite).
fn parse_iso_ms(iso: &str) -> Option<i64> {
    crate::clock::iso_to_ms(iso)
}

/// v4 `isParticipantPresent(status)` — active or silent.
fn is_participant_present(status: &str) -> bool {
    matches!(status, "active" | "silent")
}

// ===========================================================================
// GATHER — group resolution (shared by vault.groups / vaultAccess.groups /
// context.groups). v4 `builders.ts::resolveMyGroups`.
// ===========================================================================

struct ResolvedGroup {
    group_id: String,
    group_name: String,
    /// Official store + linked stores, de-duplicated, first-seen order.
    mount_point_ids: Vec<String>,
}

/// Resolve the groups the character belongs to + each group's document stores
/// (v4 `resolveMyGroups`): membership → `groups.findById` (skip on error/absent),
/// official mount + `groupDocMountLinks.findByGroupId`, then sort by group name.
fn resolve_my_groups(db: &Db, character_id: &str) -> Result<Vec<ResolvedGroup>, DbError> {
    let memberships: Vec<String> = db.read_mount_index(|mount| {
        crate::db::group_character_members::GroupCharacterMembersRepository::new(mount)
            .find_group_ids_by_character_id(character_id)
    })?;

    let mut resolved: Vec<ResolvedGroup> = Vec::new();
    for group_id in memberships {
        // v4 wraps `groups.findById` in try/catch (unavailable store → skip).
        let group = db.read_main(|main| {
            db.read_mount_index(|mount| {
                crate::db::groups::GroupsRepository::new(main, mount)
                    .find_by_id(&group_id)
                    .map_err(overlay_to_db_err)
            })
        });
        let group = match group {
            Ok(Some(g)) => g,
            Ok(None) => continue,
            Err(_) => continue, // store unavailable — skip, not fatal
        };

        // Official store + linked stores, deduped (first-seen order).
        let mut ids: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        if let Some(official) = json_str(&group, "officialMountPointId") {
            if seen.insert(official.clone()) {
                ids.push(official);
            }
        }
        let links = db.read_mount_index(|mount| {
            crate::db::group_doc_mount_links::GroupDocMountLinksRepository::new(mount)
                .find_by_group_id(&group_id)
        })?;
        for l in links {
            if seen.insert(l.clone()) {
                ids.push(l);
            }
        }

        resolved.push(ResolvedGroup {
            group_id: json_str(&group, "id").unwrap_or_default(),
            group_name: json_str(&group, "name").unwrap_or_default(),
            mount_point_ids: ids,
        });
    }

    // JS `.sort` (stable) by group name localeCompare.
    resolved.sort_by(|a, b| locale_compare(&a.group_name, &b.group_name));
    Ok(resolved)
}

/// A mount-point `(id, name)` fetch off the mount-index db.
fn find_mount(db: &Db, id: &str) -> Result<Option<(String, String)>, DbError> {
    db.read_mount_index(|mount| {
        crate::db::doc_mount_points::DocMountPointsRepository::new(mount).find_id_and_name_by_id(id)
    })
}

/// Fetch the vault-file rows for a mount point.
fn find_vault_rows(
    db: &Db,
    mount_point_id: &str,
) -> Result<Vec<crate::db::doc_mount_files::VaultFileRow>, DbError> {
    db.read_mount_index(|mount| {
        crate::db::doc_mount_files::DocMountFilesRepository::new(mount)
            .find_vault_files_by_mount_point_id(mount_point_id)
    })
}

// ===========================================================================
// GATHER — vault section (v4 buildVaultCharacterSection / buildVaultGroupsSection)
// ===========================================================================

fn build_vault_character_section(
    db: &Db,
    env: &SelfInventoryEnv,
    character: &Value,
    include_automatic_images: bool,
) -> Result<VaultCharacterSection, DbError> {
    if env.mount_index_degraded {
        return Ok(VaultCharacterSection::Unavailable {
            available: AvailFalse,
            reason: "mount_index_degraded".to_string(),
            message: "Mount index database is in degraded mode.".to_string(),
        });
    }

    let Some(mount_point_id) =
        json_str(character, "characterDocumentMountPointId").filter(|s| !s.is_empty())
    else {
        return Ok(VaultCharacterSection::Unavailable {
            available: AvailFalse,
            reason: "no_vault".to_string(),
            message: "Character has no vault mount point.".to_string(),
        });
    };

    let Some((mp_id, mp_name)) = find_mount(db, &mount_point_id)? else {
        return Ok(VaultCharacterSection::Unavailable {
            available: AvailFalse,
            reason: "no_vault".to_string(),
            message: format!("Mount point {mount_point_id} not found."),
        });
    };

    let rows = find_vault_rows(db, &mount_point_id)?;
    // The character's own vault → the stable, readable `self` form.
    let files = map_vault_files(&rows, &mp_name, include_automatic_images, true, |rel| {
        format_self_uri(rel, None, None)
    });

    Ok(VaultCharacterSection::Available {
        available: AvailTrue,
        mount_point_name: mp_name,
        mount_point_id: mp_id,
        file_count: files.len() as i64,
        files,
    })
}

fn build_vault_groups_section(
    db: &Db,
    env: &SelfInventoryEnv,
    character_id: &str,
    include_automatic_images: bool,
) -> Result<VaultGroupsSection, DbError> {
    if env.mount_index_degraded {
        return Ok(VaultGroupsSection::Unavailable {
            available: AvailFalse,
            reason: "mount_index_degraded".to_string(),
            message: "Mount index database is in degraded mode.".to_string(),
        });
    }

    let groups = resolve_my_groups(db, character_id)?;
    if groups.is_empty() {
        return Ok(VaultGroupsSection::Unavailable {
            available: AvailFalse,
            reason: "no_groups".to_string(),
            message: "You are not a member of any groups.".to_string(),
        });
    }

    let mut out: Vec<VaultGroup> = Vec::new();
    for group in &groups {
        for mount_point_id in &group.mount_point_ids {
            let Some((mp_id, mp_name)) = find_mount(db, mount_point_id)? else {
                continue;
            };
            let rows = find_vault_rows(db, mount_point_id)?;
            // A group store, not the acting character's own vault → address by name.
            let mp_name_for_uri = mp_name.clone();
            let mp_id_for_uri = mp_id.clone();
            let files = map_vault_files(&rows, &mp_name, include_automatic_images, false, |rel| {
                format_doc_store_uri(&mp_name_for_uri, &mp_id_for_uri, rel, false, None, None)
            });
            out.push(VaultGroup {
                group_id: group.group_id.clone(),
                group_name: group.group_name.clone(),
                mount_point_id: mp_id,
                mount_point_name: mp_name,
                file_count: files.len() as i64,
                files,
            });
        }
    }

    Ok(VaultGroupsSection::Available {
        available: AvailTrue,
        groups: out,
    })
}

// ===========================================================================
// GATHER — vaultAccess section
// ===========================================================================

fn build_vault_access_groups_section(
    db: &Db,
    character_id: &str,
) -> Result<VaultAccessGroupsSection, DbError> {
    let groups = resolve_my_groups(db, character_id)?;
    if groups.is_empty() {
        return Ok(VaultAccessGroupsSection::Unavailable {
            available: AvailFalse,
            reason: "no_groups".to_string(),
            message: "You are not a member of any groups.".to_string(),
        });
    }

    let mut out: Vec<GroupVaultAccess> = Vec::new();
    for group in &groups {
        let member_ids = db.read_mount_index(|mount| {
            crate::db::group_character_members::GroupCharacterMembersRepository::new(mount)
                .find_character_ids_by_group_id(&group.group_id)
        })?;

        let mut members: Vec<GroupVaultMember> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for member_id in member_ids {
            if !seen.insert(member_id.clone()) {
                continue;
            }
            // v4 wraps `characters.findById` in try/catch (broken vault → Unknown).
            let mut character_name = "Unknown".to_string();
            if let Ok(Some(c)) = db.read_main(|main| {
                db.read_mount_index(|mount| {
                    crate::db::characters_read::find_by_id(main, mount, &member_id)
                })
            }) {
                if let Some(n) = json_str(&c, "name") {
                    character_name = n;
                }
            }
            members.push(GroupVaultMember {
                is_self: member_id == character_id,
                character_id: member_id,
                character_name,
                access: "read_write".to_string(),
            });
        }

        // Sort: self first, then name localeCompare (stable).
        members.sort_by(|a, b| {
            if a.is_self != b.is_self {
                return if a.is_self {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }
            locale_compare(&a.character_name, &b.character_name)
        });

        out.push(GroupVaultAccess {
            group_id: group.group_id.clone(),
            group_name: group.group_name.clone(),
            members,
        });
    }

    Ok(VaultAccessGroupsSection::Available {
        available: AvailTrue,
        groups: out,
    })
}

fn build_vault_access_character_section(
    db: &Db,
    character: &Value,
    chat_id: &str,
) -> Result<VaultAccessCharacterSection, DbError> {
    let chat = db.read_main(|main| crate::db::chats_read::find_by_id(main, chat_id))?;
    let shared_vaults_enabled = chat
        .as_ref()
        .map(|c| json_bool(c, "allowCrossCharacterVaultReads"))
        .unwrap_or(false);

    let character_id = json_str(character, "id").unwrap_or_default();
    let mount_point_id =
        json_str(character, "characterDocumentMountPointId").filter(|s| !s.is_empty());
    if mount_point_id.is_none() {
        return Ok(VaultAccessCharacterSection::Unavailable {
            available: AvailFalse,
            shared_vaults_enabled,
            message: "Character has no vault mount point.".to_string(),
        });
    }
    let Some(chat) = chat else {
        return Ok(VaultAccessCharacterSection::Unavailable {
            available: AvailFalse,
            shared_vaults_enabled,
            message: "Chat not found.".to_string(),
        });
    };
    let mount_point_id = mount_point_id.unwrap();

    let Some((_mp_id, mp_name)) = find_mount(db, &mount_point_id)? else {
        return Ok(VaultAccessCharacterSection::Unavailable {
            available: AvailFalse,
            shared_vaults_enabled,
            message: format!("Mount point {mount_point_id} not found."),
        });
    };

    let mut participants: Vec<VaultAccessParticipant> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(ps) = chat.get("participants").and_then(Value::as_array) {
        for p in ps {
            if json_str(p, "type").as_deref() != Some("CHARACTER") {
                continue;
            }
            let Some(cid) = json_str(p, "characterId").filter(|s| !s.is_empty()) else {
                continue;
            };
            let status = json_str(p, "status").unwrap_or_default();
            if !is_participant_present(&status) {
                continue;
            }
            if !seen.insert(cid.clone()) {
                continue;
            }

            let is_self = cid == character_id;
            let controlled_by = json_str(p, "controlledBy").unwrap_or_default();
            let is_user = controlled_by == "user";

            let access: Option<&str> = if is_self || is_user {
                Some("read_write")
            } else if shared_vaults_enabled {
                Some("read_only")
            } else {
                None
            };
            let Some(access) = access else { continue };

            let mut character_name = "Unknown".to_string();
            if let Ok(Some(c)) = db.read_main(|main| {
                db.read_mount_index(|mount| {
                    crate::db::characters_read::find_by_id(main, mount, &cid)
                })
            }) {
                if let Some(n) = json_str(&c, "name") {
                    character_name = n;
                }
            }

            participants.push(VaultAccessParticipant {
                participant_id: json_str(p, "id").unwrap_or_default(),
                character_id: cid,
                character_name,
                controlled_by,
                status,
                is_self,
                access: access.to_string(),
            });
        }
    }

    // Sort: self first, then user persona, then name localeCompare.
    participants.sort_by(|a, b| {
        if a.is_self != b.is_self {
            return if a.is_self {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        let a_user = if a.controlled_by == "user" { 0 } else { 1 };
        let b_user = if b.controlled_by == "user" { 0 } else { 1 };
        if a_user != b_user {
            return a_user.cmp(&b_user);
        }
        locale_compare(&a.character_name, &b.character_name)
    });

    Ok(VaultAccessCharacterSection::Available {
        available: AvailTrue,
        mount_point_name: mp_name,
        shared_vaults_enabled,
        participants,
    })
}

// ===========================================================================
// GATHER — memory / loadedMemories / chats
// ===========================================================================

fn build_memory_section(db: &Db, character_id: &str) -> Result<MemorySection, DbError> {
    let total_count =
        db.read_main(|main| crate::db::memories_read::count_by_character_id(main, character_id))?;
    let high = db.read_main(|main| {
        crate::db::memories_read::find_by_importance(main, character_id, HIGH_IMPORTANCE_THRESHOLD)
    })?;
    let high_count = high.len() as i64;
    let percent = if total_count == 0 {
        js_number_to_json(0.0)
    } else {
        round_percent((high_count as f64 / total_count as f64) * 100.0)
    };

    Ok(MemorySection {
        available: true,
        total_count,
        high_importance_count: high_count,
        high_importance_percent: percent,
        threshold: js_number_to_json(HIGH_IMPORTANCE_THRESHOLD),
        message: None,
    })
}

fn build_loaded_memories_section(loaded: Option<&LoadedMemoriesContext>) -> LoadedMemoriesSection {
    let Some(loaded) = loaded else {
        return LoadedMemoriesSection::Unavailable {
            available: AvailFalse,
            message:
                "No loaded-memory data available for this turn (tool was invoked outside of a prompt-building flow)."
                    .to_string(),
        };
    };
    LoadedMemoriesSection::Available {
        available: AvailTrue,
        semantic_memories: loaded
            .semantic
            .iter()
            .map(|m| SemanticMemoryItem {
                summary: m.summary.clone(),
                importance: js_number_to_json(m.importance),
                score: js_number_to_json(m.score),
                effective_weight: js_number_to_json(m.effective_weight),
            })
            .collect(),
        inter_character_memories: loaded
            .inter_character
            .iter()
            .map(|m| InterCharacterMemoryItem {
                about_character_name: m.about_character_name.clone(),
                summary: m.summary.clone(),
                importance: js_number_to_json(m.importance),
            })
            .collect(),
        recap: loaded.recap.clone(),
    }
}

fn build_chats_section(db: &Db, character_id: &str) -> Result<ChatSection, DbError> {
    let chats =
        db.read_main(|main| crate::db::chats_read::find_by_character_id(main, character_id))?;
    if chats.is_empty() {
        return Ok(ChatSection {
            available: true,
            chat_count: 0,
            earliest_created_at: None,
            latest_activity_at: None,
            message: None,
        });
    }

    let mut earliest_ms = i64::MAX;
    let mut latest_ms = i64::MIN;
    let mut earliest_iso: Option<String> = None;
    let mut latest_iso: Option<String> = None;

    for chat in &chats {
        if let Some(created) = json_str(chat, "createdAt") {
            if let Some(ms) = parse_iso_ms(&created) {
                if ms < earliest_ms {
                    earliest_ms = ms;
                    earliest_iso = Some(created);
                }
            }
        }

        // activityIso = lastMessageAt ?? updatedAt ?? createdAt
        let activity_iso = json_str(chat, "lastMessageAt")
            .or_else(|| json_str(chat, "updatedAt"))
            .or_else(|| json_str(chat, "createdAt"));
        if let Some(activity) = activity_iso {
            if let Some(ms) = parse_iso_ms(&activity) {
                if ms > latest_ms {
                    latest_ms = ms;
                    latest_iso = Some(activity);
                }
            }
        }
    }

    Ok(ChatSection {
        available: true,
        chat_count: chats.len() as i64,
        earliest_created_at: earliest_iso,
        latest_activity_at: latest_iso,
        message: None,
    })
}

// ===========================================================================
// GATHER — prompt section (v4 buildPromptSection + resolveRespondingContext)
// ===========================================================================

/// Build the [`crate::system_prompt::Character`] the prompt builder reads off a
/// vault-overlaid character Value — the full field set the identity stack consumes
/// (pronouns / physicalDescription / scenarios / systemPrompts / exampleDialogues),
/// unlike the abbreviated `orchestrator::to_sys_char` (whose corpus carries none of
/// those). Field names match the overlay's output keys.
fn to_sys_char(c: &Value) -> crate::system_prompt::Character {
    use crate::system_prompt::{Character, Pronouns, ScenarioEntry, SystemPromptEntry};

    let pronouns = c
        .get("pronouns")
        .filter(|v| v.is_object())
        .map(|p| Pronouns {
            subject: json_str(p, "subject").unwrap_or_default(),
            object: json_str(p, "object").unwrap_or_default(),
            possessive: json_str(p, "possessive").unwrap_or_default(),
        });

    let physical_description = c
        .get("physicalDescription")
        .filter(|v| v.is_object())
        .map(|pd| crate::system_prompt::PhysicalDescription {
            name: json_str(pd, "name").unwrap_or_default(),
            usage_context: json_str(pd, "usageContext"),
            short_prompt: json_str(pd, "shortPrompt"),
            medium_prompt: json_str(pd, "mediumPrompt"),
            long_prompt: json_str(pd, "longPrompt"),
            complete_prompt: json_str(pd, "completePrompt"),
            full_description: json_str(pd, "fullDescription"),
        });

    let scenarios = c
        .get("scenarios")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|s| ScenarioEntry {
                    content: json_str(s, "content").unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();

    let system_prompts = c
        .get("systemPrompts")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|s| SystemPromptEntry {
                    id: json_str(s, "id").unwrap_or_default(),
                    content: json_str(s, "content").unwrap_or_default(),
                    is_default: json_bool(s, "isDefault"),
                })
                .collect()
        })
        .unwrap_or_default();

    Character {
        name: json_str(c, "name").unwrap_or_default(),
        title: json_str(c, "title"),
        identity: json_str(c, "identity"),
        description: json_str(c, "description"),
        manifesto: json_str(c, "manifesto"),
        personality: json_str(c, "personality"),
        aliases: json_str_array(c, "aliases"),
        pronouns,
        physical_description,
        example_dialogues: json_str(c, "exampleDialogues"),
        scenarios,
        system_prompts,
    }
}

/// v4 `resolveRespondingContext` — the responding participant + userCharacter for
/// the prompt builder. Returns `None` when no matching participant exists.
struct RespondingContext {
    selected_system_prompt_id: Option<String>,
    user_character: Option<crate::system_prompt::UserCharacter>,
}

fn resolve_responding_context(
    db: &Db,
    chat: &Value,
    character_id: &str,
    calling_participant_id: Option<&str>,
) -> Result<Option<RespondingContext>, DbError> {
    let participants = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    // respondingParticipant precedence: by callingParticipantId → the
    // non-user CHARACTER for this character → any CHARACTER for this character.
    let responding = participants
        .iter()
        .find(|p| json_str(p, "id").as_deref() == calling_participant_id)
        .or_else(|| {
            participants.iter().find(|p| {
                json_str(p, "type").as_deref() == Some("CHARACTER")
                    && json_str(p, "characterId").as_deref() == Some(character_id)
                    && json_str(p, "controlledBy").as_deref() != Some("user")
            })
        })
        .or_else(|| {
            participants.iter().find(|p| {
                json_str(p, "type").as_deref() == Some("CHARACTER")
                    && json_str(p, "characterId").as_deref() == Some(character_id)
            })
        });

    let Some(responding) = responding else {
        return Ok(None);
    };
    let selected_system_prompt_id = json_str(responding, "selectedSystemPromptId");

    // userCharacter: a CHARACTER participant controlled by the user.
    let mut user_character = None;
    if let Some(up) = participants.iter().find(|p| {
        json_str(p, "type").as_deref() == Some("CHARACTER")
            && json_str(p, "controlledBy").as_deref() == Some("user")
    }) {
        if let Some(ucid) = json_str(up, "characterId").filter(|s| !s.is_empty()) {
            if let Some(uc) = db.read_main(|main| {
                db.read_mount_index(|mount| {
                    crate::db::characters_read::find_by_id(main, mount, &ucid)
                })
            })? {
                user_character = Some(crate::system_prompt::UserCharacter {
                    name: json_str(&uc, "name").unwrap_or_default(),
                    description: json_str(&uc, "description").unwrap_or_default(),
                });
            }
        }
    }

    Ok(Some(RespondingContext {
        selected_system_prompt_id,
        user_character,
    }))
}

fn build_prompt_section(
    db: &Db,
    character: &Value,
    chat_id: &str,
    character_id: &str,
    calling_participant_id: Option<&str>,
) -> Result<PromptSection, DbError> {
    let Some(chat) = db.read_main(|main| crate::db::chats_read::find_by_id(main, chat_id))? else {
        return Ok(PromptSection {
            available: false,
            system_prompt: None,
            character_count: 0,
            approx_tokens: None,
            message: Some("Chat not found.".to_string()),
        });
    };

    let Some(resolved) =
        resolve_responding_context(db, &chat, character_id, calling_participant_id)?
    else {
        return Ok(PromptSection {
            available: false,
            system_prompt: None,
            character_count: 0,
            approx_tokens: None,
            message: Some(
                "Responding participant not found for character in this chat.".to_string(),
            ),
        });
    };

    // roleplayTemplate: `chat.roleplayTemplateId` → `roleplayTemplates.findById`'s
    // `systemPrompt` (string only). v4 wraps in try/catch.
    let roleplay_template: Option<String> = match json_str(&chat, "roleplayTemplateId") {
        Some(rt_id) => db
            .read_main(|main| crate::db::roleplay_templates::find_system_prompt_by_id(main, &rt_id))
            .unwrap_or(None),
        None => None,
    };

    // buildSystemPrompt. `timestampConfig` is set to `None` here (the corpus chat
    // carries no timestampConfig, so v4's `chat.timestampConfig ?? null` is null and
    // the timestamp template path never fires); now_ms/local_offset therefore unused.
    let sys = to_sys_char(character);
    // Deliberately omits `taboo_phrases`: this is introspection, not a live turn,
    // and the instance-settings read isn't worth threading through for a
    // reporting path. The reconstructed prompt is therefore missing the Taboo
    // section a real turn would carry — a known, accepted fidelity gap
    // (v4 `lib/tools/handlers/self-inventory/builders.ts`, `7df7de8e`).
    let system_prompt = crate::system_prompt::build_system_prompt(
        &crate::system_prompt::BuildSystemPromptOptions {
            character: &sys,
            user_character: resolved.user_character.as_ref(),
            roleplay_template: roleplay_template.as_deref(),
            tool_instructions: None,
            selected_system_prompt_id: resolved.selected_system_prompt_id.as_deref(),
            timestamp_config: None,
            is_initial_message: false,
            timezone: None,
            scenario_text: json_str(&chat, "scenarioText").as_deref(),
            precompiled_identity_stack: None,
            taboo_phrases: None,
            now_ms: 0,
            local_offset_minutes: 0,
        },
    )
    .map_err(|e| DbError::Key(format!("buildSystemPrompt: invalid timezone {}", e.0)))?;

    // characterCount = systemPrompt.length (UTF-16); approxTokens = round(count/4).
    let character_count = crate::jsstr::utf16_len(&system_prompt) as i64;
    let approx_tokens = (character_count as f64 / 4.0).round() as i64;

    Ok(PromptSection {
        available: true,
        system_prompt: Some(system_prompt),
        character_count,
        approx_tokens: Some(approx_tokens),
        message: None,
    })
}

// ===========================================================================
// GATHER — lastTurn section (v4 buildLastTurnSection)
// ===========================================================================

fn build_last_turn_section(
    db: &Db,
    env: &SelfInventoryEnv,
    character: &Value,
    chat_id: &str,
    character_id: &str,
    calling_participant_id: Option<&str>,
) -> Result<LastTurnSection, DbError> {
    let last_log = db
        .read_llm_logs(|logs| {
            crate::db::llm_logs::LLMLogsRepository::new(logs).find_last_by_chat_id(chat_id)
        })
        .unwrap_or(None);

    if let Some(log) = last_log {
        let (prompt_tokens, completion_tokens, total_tokens) = match &log.usage {
            Some(u) => (
                Some(u.prompt_tokens),
                Some(u.completion_tokens),
                Some(u.total_tokens),
            ),
            None => (None, None, None),
        };
        // getModelContextLimit never throws; a 0 result is falsy in the util path,
        // but the function always returns a positive default — kept as-is.
        let context_window = env.model_context_limit(&log.provider, &log.model_name);
        let utilization = match total_tokens {
            Some(t) if context_window > 0 => {
                Some(round_percent((t as f64 / context_window as f64) * 100.0))
            }
            _ => None,
        };
        return Ok(LastTurnSection {
            available: true,
            source: Some("llm_log".to_string()),
            provider: Some(log.provider),
            model_name: Some(log.model_name),
            prompt_tokens,
            completion_tokens,
            total_tokens,
            context_window: Some(context_window),
            utilization_percent: utilization,
            logged_at: Some(log.created_at),
            message: None,
        });
    }

    // Fallback: resolve the profile that would run.
    let Some(chat) = db.read_main(|main| crate::db::chats_read::find_by_id(main, chat_id))? else {
        return Ok(last_turn_unavailable("Chat not found."));
    };
    let participants = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let participant = participants
        .iter()
        .find(|p| json_str(p, "id").as_deref() == calling_participant_id)
        .or_else(|| {
            participants.iter().find(|p| {
                json_str(p, "type").as_deref() == Some("CHARACTER")
                    && json_str(p, "characterId").as_deref() == Some(character_id)
            })
        });
    let Some(participant) = participant else {
        return Ok(last_turn_unavailable(
            "No LLM log and responding participant not found.",
        ));
    };

    let profile_id = crate::services::participant_resolver::connection::resolve_connection_profile(
        participant,
        character,
        None,
    );
    // v4 catches a throw from resolveConnectionProfile; `None` is v4's throw path.
    let Some(profile_id) = profile_id else {
        return Ok(last_turn_unavailable(&format!(
            "No LLM log and profile resolution failed: {}",
            "No connection profile configured for character"
        )));
    };
    let Some(profile) =
        db.read_main(|main| crate::db::connection_profiles::find_by_id(main, &profile_id))?
    else {
        return Ok(last_turn_unavailable(&format!(
            "No LLM log and connection profile {profile_id} not found."
        )));
    };

    let provider = json_str(&profile, "provider").unwrap_or_default();
    let model_name = json_str(&profile, "modelName").unwrap_or_default();
    // profile.maxContext ?? getModelContextLimit(...)
    let context_window = profile
        .get("maxContext")
        .and_then(Value::as_f64)
        .map(|f| f as i64)
        .unwrap_or_else(|| env.model_context_limit(&provider, &model_name));

    Ok(LastTurnSection {
        available: true,
        source: Some("profile_fallback".to_string()),
        provider: Some(provider),
        model_name: Some(model_name),
        prompt_tokens: Some(0),
        completion_tokens: Some(0),
        total_tokens: Some(0),
        context_window: Some(context_window),
        utilization_percent: Some(js_number_to_json(0.0)),
        logged_at: None,
        message: None,
    })
}

fn last_turn_unavailable(message: &str) -> LastTurnSection {
    LastTurnSection {
        available: false,
        source: None,
        provider: None,
        model_name: None,
        prompt_tokens: None,
        completion_tokens: None,
        total_tokens: None,
        context_window: None,
        utilization_percent: None,
        logged_at: None,
        message: Some(message.to_string()),
    }
}

// ===========================================================================
// GATHER — carina section (v4 buildCarinaSection)
// ===========================================================================

fn build_carina_section(db: &Db, character_id: &str) -> Result<CarinaSection, DbError> {
    let raw = db.read_main(crate::db::characters_read::find_all_raw)?;

    let self_enabled = raw
        .iter()
        .any(|c| json_str(c, "id").as_deref() == Some(character_id) && json_bool(c, "canBeCarina"));

    let mut reachable: Vec<CarinaReachable> = raw
        .iter()
        .filter(|c| json_str(c, "id").as_deref() != Some(character_id))
        .filter(|c| self_enabled || json_bool(c, "canBeCarina"))
        .map(|c| CarinaReachable {
            name: json_str(c, "name").unwrap_or_default(),
            is_answerer: json_bool(c, "canBeCarina"),
        })
        .collect();
    reachable.sort_by(|a, b| locale_compare(&a.name, &b.name));

    Ok(CarinaSection::Available {
        available: AvailTrue,
        self_enabled,
        reachable,
    })
}

// ===========================================================================
// GATHER — quilltap section (v4 buildQuilltapSection, host-env seam)
// ===========================================================================

fn build_quilltap_section(env: &SelfInventoryEnv, parts: QuilltapIncludedParts) -> QuilltapSection {
    let client_shell = match &env.client_shell {
        ClientShell::Electron { shell_version } => ClientShellJson::Electron {
            kind: ElectronTag,
            shell_version: shell_version.clone(),
        },
        ClientShell::Browser => ClientShellJson::Browser { kind: BrowserTag },
        ClientShell::Unknown => ClientShellJson::Unknown { kind: UnknownTag },
    };

    let (release_notes, release_notes_version) = if parts.release_notes {
        match &env.release_notes {
            Some((body, ver)) => (Some(body.clone()), Some(ver.clone())),
            None => (None, None),
        }
    } else {
        (None, None)
    };

    let changelog = if parts.changelog {
        env.changelog.clone()
    } else {
        None
    };

    QuilltapSection {
        available: true,
        included_parts: parts,
        version: env.version.clone(),
        runtime_mode: env.runtime_mode.clone(),
        client_shell,
        release_notes,
        release_notes_version,
        changelog,
        message: None,
    }
}

// ===========================================================================
// GATHER — context section (v4 buildContextSection + sub-builders)
// ===========================================================================

fn resolve_mount_names(db: &Db, ids: &[String]) -> Result<Vec<ContextMount>, DbError> {
    let mut mounts = Vec::new();
    for id in ids {
        if let Some((mp_id, mp_name)) = find_mount(db, id)? {
            mounts.push(ContextMount {
                mount_point_id: mp_id,
                name: mp_name,
            });
        }
    }
    Ok(mounts)
}

fn build_context_project(db: &Db, project_id: Option<&str>) -> Result<ContextProject, DbError> {
    let Some(project_id) = project_id.filter(|s| !s.is_empty()) else {
        return Ok(ContextProject::Absent {
            available: AvailTrue,
            present: PresentFalse,
        });
    };

    let project = db.read_main(|main| {
        db.read_mount_index(|mount| {
            crate::db::projects::ProjectsRepository::new(main, mount)
                .find_by_id(project_id)
                .map_err(overlay_to_db_err)
        })
    });
    let project = match project {
        Ok(Some(p)) => p,
        _ => {
            return Ok(ContextProject::Absent {
                available: AvailTrue,
                present: PresentFalse,
            })
        }
    };

    let mut ids: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(official) = json_str(&project, "officialMountPointId") {
        if seen.insert(official.clone()) {
            ids.push(official);
        }
    }
    // v4 wraps projectDocMountLinks in try/catch (mount-index unavailable → skip).
    if let Ok(links) = db.read_mount_index(|mount| {
        crate::db::project_doc_mount_links::ProjectDocMountLinksRepository::new(mount)
            .find_by_project_id(project_id)
    }) {
        for l in links {
            if seen.insert(l.clone()) {
                ids.push(l);
            }
        }
    }

    let mount_points = resolve_mount_names(db, &ids)?;
    Ok(ContextProject::Present {
        available: AvailTrue,
        present: PresentTrue,
        id: json_str(&project, "id").unwrap_or_default(),
        name: json_str(&project, "name").unwrap_or_default(),
        mount_points,
    })
}

fn build_context_groups(db: &Db, character_id: &str) -> Result<ContextGroups, DbError> {
    let groups = resolve_my_groups(db, character_id)?;
    let mut out = Vec::new();
    for group in &groups {
        let mount_points = resolve_mount_names(db, &group.mount_point_ids)?;
        out.push(ContextGroup {
            id: group.group_id.clone(),
            name: group.group_name.clone(),
            mount_points,
        });
    }
    Ok(ContextGroups::Available {
        available: AvailTrue,
        groups: out,
    })
}

fn build_context_characters(
    db: &Db,
    self_character: &Value,
    chat: Option<&Value>,
) -> Result<ContextCharacters, DbError> {
    let Some(chat) = chat else {
        return Ok(ContextCharacters::Unavailable {
            available: AvailFalse,
            message: "Chat not found.".to_string(),
        });
    };
    let self_id = json_str(self_character, "id").unwrap_or_default();

    let mut out: Vec<ContextCharacter> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(ps) = chat.get("participants").and_then(Value::as_array) {
        for p in ps {
            if json_str(p, "type").as_deref() != Some("CHARACTER") {
                continue;
            }
            let Some(cid) = json_str(p, "characterId").filter(|s| !s.is_empty()) else {
                continue;
            };
            let status = json_str(p, "status").unwrap_or_default();
            if !is_participant_present(&status) {
                continue;
            }
            if cid == self_id {
                continue; // others present, not yourself
            }
            if !seen.insert(cid.clone()) {
                continue;
            }

            // v4 wraps characters.findById in try/catch (unavailable → skip).
            let c = db.read_main(|main| {
                db.read_mount_index(|mount| {
                    crate::db::characters_read::find_by_id(main, mount, &cid)
                })
            });
            let c = match c {
                Ok(Some(c)) => c,
                Ok(None) => continue,
                Err(_) => continue,
            };

            out.push(ContextCharacter {
                id: json_str(&c, "id").unwrap_or_default(),
                name: json_str(&c, "name").unwrap_or_default(),
                aliases: json_str_array(&c, "aliases"),
                identity: json_str(&c, "identity"),
                is_user_persona: json_str(p, "controlledBy").as_deref() == Some("user"),
            });
        }
    }

    out.sort_by(|a, b| locale_compare(&a.name, &b.name));
    Ok(ContextCharacters::Available {
        available: AvailTrue,
        characters: out,
    })
}

/// v4 `buildContextFileUri` — the qtap:// URI for a chat-context document from the
/// stored triple.
fn build_context_file_uri(scope: &str, mount_point: Option<&str>, file_path: &str) -> String {
    if scope == "project" {
        return format_scoped_uri(ScopedAuthority::Project, file_path, None, None);
    }
    if scope == "general" {
        return format_scoped_uri(ScopedAuthority::General, file_path, None, None);
    }
    match mount_point {
        None => format_self_uri(file_path, None, None),
        Some(mp) if mp.to_lowercase() == "self" => format_self_uri(file_path, None, None),
        Some(mp) => format_doc_store_uri(mp, "", file_path, false, None, None),
    }
}

fn build_context_files(db: &Db, chat_id: &str) -> Result<ContextFiles, DbError> {
    let docs = db.read_main(|main| {
        crate::db::chat_documents::ChatDocumentsRepository::new(main).find_by_chat_id(chat_id)
    })?;
    let mut files: Vec<ContextFile> = docs
        .into_iter()
        .map(|d| {
            let uri = build_context_file_uri(&d.scope, d.mount_point.as_deref(), &d.file_path);
            let how_to_reach = format!("doc_read_file({{ uri: \"{uri}\" }})");
            ContextFile {
                scope: d.scope,
                mount_point: d.mount_point,
                file_path: d.file_path,
                display_title: d.display_title,
                uri,
                how_to_reach,
            }
        })
        .collect();
    files.sort_by(|a, b| locale_compare(&a.file_path, &b.file_path));
    Ok(ContextFiles::Available {
        available: AvailTrue,
        files,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_context_section(
    db: &Db,
    character: &Value,
    chat_id: &str,
    character_id: &str,
    project_id: Option<&str>,
    parts: ContextIncludedParts,
) -> Result<ContextSection, DbError> {
    let chat = db.read_main(|main| crate::db::chats_read::find_by_id(main, chat_id))?;

    let mut out = ContextSection {
        included_parts: parts,
        chat: None,
        project: None,
        groups: None,
        characters: None,
        files: None,
    };

    if parts.chat {
        out.chat = Some(match &chat {
            Some(c) => ContextChat {
                available: true,
                chat_id: json_str(c, "id").unwrap_or_default(),
                title: json_str(c, "title"),
                message: None,
            },
            None => ContextChat {
                available: false,
                chat_id: chat_id.to_string(),
                title: None,
                message: Some("Chat not found.".to_string()),
            },
        });
    }

    if parts.project {
        // context.projectId ?? chat?.projectId ?? null
        let pid = project_id
            .map(str::to_string)
            .or_else(|| chat.as_ref().and_then(|c| json_str(c, "projectId")));
        out.project = Some(build_context_project(db, pid.as_deref())?);
    }

    if parts.groups {
        out.groups = Some(build_context_groups(db, character_id)?);
    }

    if parts.characters {
        out.characters = Some(build_context_characters(db, character, chat.as_ref())?);
    }

    if parts.files {
        out.files = Some(build_context_files(db, chat_id)?);
    }

    Ok(out)
}

// ===========================================================================
// resolve*IncludedParts (v4 builders.ts)
// ===========================================================================

/// v4 `resolveRequestedSections`: the `sections` arg → a set, or all sections.
fn resolve_requested_sections(args: &Value) -> Option<Vec<String>> {
    args.get("sections")
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        })
}

fn has_section(requested: &Option<Vec<String>>, name: &str) -> bool {
    match requested {
        None => true, // all sections requested
        Some(list) => list.iter().any(|s| s == name),
    }
}

fn resolve_vault_parts(req: &Option<Vec<String>>) -> Option<VaultIncludedParts> {
    let all = has_section(req, "vault");
    let character = all || has_section(req, "vault.character");
    let groups = all || has_section(req, "vault.groups");
    if !character && !groups {
        return None;
    }
    Some(VaultIncludedParts { character, groups })
}

fn resolve_vault_access_parts(req: &Option<Vec<String>>) -> Option<VaultIncludedParts> {
    let all = has_section(req, "vaultAccess");
    let character = all || has_section(req, "vaultAccess.character");
    let groups = all || has_section(req, "vaultAccess.groups");
    if !character && !groups {
        return None;
    }
    Some(VaultIncludedParts { character, groups })
}

fn resolve_quilltap_parts(req: &Option<Vec<String>>) -> Option<QuilltapIncludedParts> {
    let all = has_section(req, "quilltap");
    let version = all || has_section(req, "quilltap.version");
    let release_notes = all || has_section(req, "quilltap.releaseNotes");
    let changelog = all || has_section(req, "quilltap.changelog");
    if !version && !release_notes && !changelog {
        return None;
    }
    Some(QuilltapIncludedParts {
        version,
        release_notes,
        changelog,
    })
}

fn resolve_context_parts(req: &Option<Vec<String>>) -> Option<ContextIncludedParts> {
    let all = has_section(req, "context");
    let chat = all || has_section(req, "context.chat");
    let project = all || has_section(req, "context.project");
    let groups = all || has_section(req, "context.groups");
    let characters = all || has_section(req, "context.characters");
    let files = all || has_section(req, "context.files");
    if !chat && !project && !groups && !characters && !files {
        return None;
    }
    Some(ContextIncludedParts {
        chat,
        project,
        groups,
        characters,
        files,
    })
}

// ===========================================================================
// Orchestration (v4 executeSelfInventoryTool)
// ===========================================================================

/// Execute the self-inventory tool (v4 `executeSelfInventoryTool`). Reads only; a
/// per-section failure produces an "unavailable" marker rather than propagating.
/// `args` is the raw tool-call arguments (`{ sections?, includeAutomaticImages? }`).
#[allow(clippy::too_many_arguments)]
pub async fn execute_self_inventory(
    db: &Db,
    env: &SelfInventoryEnv,
    _user_id: &str,
    chat_id: &str,
    character_id: Option<&str>,
    project_id: Option<&str>,
    calling_participant_id: Option<&str>,
    loaded_memories: Option<&LoadedMemoriesContext>,
    args: &Value,
) -> SelfInventoryOutput {
    let requested = resolve_requested_sections(args);

    let Some(character_id) = character_id else {
        return SelfInventoryOutput {
            success: false,
            quilltap_version: env.version.clone(),
            character_id: String::new(),
            character_name: String::new(),
            error: Some("self_inventory requires a character context".to_string()),
            ..empty_sections()
        };
    };

    let character = db.read_main(|main| {
        db.read_mount_index(|mount| {
            crate::db::characters_read::find_by_id(main, mount, character_id)
        })
    });
    let character = match character {
        Ok(Some(c)) => c,
        _ => {
            return SelfInventoryOutput {
                success: false,
                quilltap_version: env.version.clone(),
                character_id: character_id.to_string(),
                character_name: String::new(),
                error: Some(format!("Character {character_id} not found")),
                ..empty_sections()
            };
        }
    };

    let character_name = json_str(&character, "name").unwrap_or_default();
    let resolved_id = json_str(&character, "id").unwrap_or_else(|| character_id.to_string());

    let include_automatic_images = args
        .get("includeAutomaticImages")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut result = SelfInventoryOutput {
        success: true,
        quilltap_version: env.version.clone(),
        character_id: resolved_id.clone(),
        character_name,
        error: None,
        ..empty_sections()
    };

    // vault
    if let Some(parts) = resolve_vault_parts(&requested) {
        let mut section = VaultSection {
            included_parts: parts,
            character: None,
            groups: None,
        };
        if parts.character {
            section.character = Some(
                build_vault_character_section(db, env, &character, include_automatic_images)
                    .unwrap_or_else(|e| VaultCharacterSection::Unavailable {
                        available: AvailFalse,
                        reason: "error".to_string(),
                        message: db_err_message(&e),
                    }),
            );
        }
        if parts.groups {
            section.groups = Some(
                build_vault_groups_section(db, env, &resolved_id, include_automatic_images)
                    .unwrap_or_else(|e| VaultGroupsSection::Unavailable {
                        available: AvailFalse,
                        reason: "error".to_string(),
                        message: db_err_message(&e),
                    }),
            );
        }
        result.vault = Some(section);
    }

    // vaultAccess
    if let Some(parts) = resolve_vault_access_parts(&requested) {
        let mut section = VaultAccessSection {
            included_parts: parts,
            character: None,
            groups: None,
        };
        if parts.character {
            section.character = Some(
                build_vault_access_character_section(db, &character, chat_id).unwrap_or_else(|e| {
                    VaultAccessCharacterSection::Unavailable {
                        available: AvailFalse,
                        shared_vaults_enabled: false,
                        message: db_err_message(&e),
                    }
                }),
            );
        }
        if parts.groups {
            section.groups = Some(
                build_vault_access_groups_section(db, &resolved_id).unwrap_or_else(|e| {
                    VaultAccessGroupsSection::Unavailable {
                        available: AvailFalse,
                        reason: "error".to_string(),
                        message: db_err_message(&e),
                    }
                }),
            );
        }
        result.vault_access = Some(section);
    }

    // memory
    if has_section(&requested, "memory") {
        result.memory =
            Some(
                build_memory_section(db, &resolved_id).unwrap_or_else(|e| MemorySection {
                    available: false,
                    total_count: 0,
                    high_importance_count: 0,
                    high_importance_percent: js_number_to_json(0.0),
                    threshold: js_number_to_json(HIGH_IMPORTANCE_THRESHOLD),
                    message: Some(db_err_message(&e)),
                }),
            );
    }

    // loadedMemories
    if has_section(&requested, "loadedMemories") {
        result.loaded_memories = Some(build_loaded_memories_section(loaded_memories));
    }

    // chats
    if has_section(&requested, "chats") {
        result.chats =
            Some(
                build_chats_section(db, &resolved_id).unwrap_or_else(|e| ChatSection {
                    available: false,
                    chat_count: 0,
                    earliest_created_at: None,
                    latest_activity_at: None,
                    message: Some(db_err_message(&e)),
                }),
            );
    }

    // prompt
    if has_section(&requested, "prompt") {
        result.prompt = Some(
            build_prompt_section(
                db,
                &character,
                chat_id,
                &resolved_id,
                calling_participant_id,
            )
            .unwrap_or_else(|e| PromptSection {
                available: false,
                system_prompt: None,
                character_count: 0,
                approx_tokens: None,
                message: Some(db_err_message(&e)),
            }),
        );
    }

    // lastTurn
    if has_section(&requested, "lastTurn") {
        result.last_turn = Some(
            build_last_turn_section(
                db,
                env,
                &character,
                chat_id,
                &resolved_id,
                calling_participant_id,
            )
            .unwrap_or_else(|e| last_turn_unavailable(&db_err_message(&e))),
        );
    }

    // carina
    if has_section(&requested, "carina") {
        result.carina = Some(build_carina_section(db, &resolved_id).unwrap_or_else(|e| {
            CarinaSection::Unavailable {
                available: AvailFalse,
                message: db_err_message(&e),
            }
        }));
    }

    // quilltap
    if let Some(parts) = resolve_quilltap_parts(&requested) {
        result.quilltap = Some(build_quilltap_section(env, parts));
    }

    // context
    if let Some(parts) = resolve_context_parts(&requested) {
        result.context = Some(
            build_context_section(db, &character, chat_id, &resolved_id, project_id, parts)
                .unwrap_or(ContextSection {
                    included_parts: parts,
                    chat: None,
                    project: None,
                    groups: None,
                    characters: None,
                    files: None,
                }),
        );
    }

    result
}

/// Error → v4 `getErrorMessage(err)` (an `Error`'s `.message`). The port surfaces
/// the `DbError` display.
fn db_err_message(e: &DbError) -> String {
    e.to_string()
}

/// Collapse a store-backed `OverlayError` to `DbError` so it flows through the
/// `read_*` closures (a vault-unavailable overlay error is a plain `DbError::Key`
/// the outer skip/`.catch` handles).
fn overlay_to_db_err(e: crate::db::document_store_overlay::OverlayError) -> DbError {
    use crate::db::document_store_overlay::OverlayError;
    match e {
        OverlayError::Db(db_err) => db_err,
        other => DbError::Key(format!("store overlay: {other}")),
    }
}

/// All section fields defaulted to `None` (a struct-update base).
fn empty_sections() -> SelfInventoryOutput {
    SelfInventoryOutput {
        success: false,
        quilltap_version: String::new(),
        character_id: String::new(),
        character_name: String::new(),
        vault: None,
        vault_access: None,
        memory: None,
        loaded_memories: None,
        chats: None,
        prompt: None,
        last_turn: None,
        carina: None,
        quilltap: None,
        context: None,
        error: None,
    }
}

// ===========================================================================
// RENDER — the markdown report (v4 formatters.ts)
// ===========================================================================

/// Render a JSON number as a JS template literal would (`${n}`): an integer-valued
/// number prints without a fractional part, else its shortest decimal. Our source
/// values are produced by [`js_number_to_json`] / [`round_percent`], which already
/// collapse integer-valued floats, so a `Value::Number` prints via serde's
/// shortest round-trip (matching V8 for the modest percent/threshold domain here).
fn render_num(v: &Value) -> String {
    match v {
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

/// `x.toFixed(2)` on a JS number Value (used by the loaded-memories detail lines).
fn to_fixed2(v: &Value) -> String {
    let f = v.as_f64().unwrap_or(0.0);
    to_fixed(f, 2)
}

fn format_vault_file_line(f: &VaultFile) -> String {
    format!(
        "- {}  [{}, {}, modified {}]",
        f.relative_path,
        f.file_type,
        format_bytes(f.file_size_bytes.as_f64().unwrap_or(0.0)),
        format_date(&f.last_modified)
    )
}

fn format_vault_character(section: &VaultCharacterSection) -> String {
    match section {
        VaultCharacterSection::Unavailable { message, .. } => {
            format!("## Character Vault\nUnavailable — {message}")
        }
        VaultCharacterSection::Available {
            mount_point_name,
            file_count,
            files,
            ..
        } => {
            let header = format!(
                "## Character Vault\nMount point: {mount_point_name} ({file_count} file{})",
                if *file_count == 1 { "" } else { "s" }
            );
            if files.is_empty() {
                return format!("{header}\n(no files)");
            }
            let lines: Vec<String> = files.iter().map(format_vault_file_line).collect();
            let footer = format!(
                "(To read one of these files: doc_read_file({{ uri: \"qtap://self/<relativePath>\" }}) — the reserved authority 'self' always addresses your own vault, whatever its name. The triple form doc_read_file(scope='document_store', mount_point='self', path='<relativePath>') still works, as do the name '{mount_point_name}' and its ID.)"
            );
            format!("{header}\n{}\n{footer}", lines.join("\n"))
        }
    }
}

fn format_vault_groups(section: &VaultGroupsSection) -> String {
    match section {
        VaultGroupsSection::Unavailable { message, .. } => {
            format!("## Group Vaults\nUnavailable — {message}")
        }
        VaultGroupsSection::Available { groups, .. } => {
            if groups.is_empty() {
                return "## Group Vaults\n(no group vaults)".to_string();
            }
            let blocks: Vec<String> = groups
                .iter()
                .map(|g| {
                    let head = format!(
                        "### {} — {} ({} file{})",
                        g.group_name,
                        g.mount_point_name,
                        g.file_count,
                        if g.file_count == 1 { "" } else { "s" }
                    );
                    if g.files.is_empty() {
                        return format!("{head}\n(no files)");
                    }
                    let lines: Vec<String> = g.files.iter().map(format_vault_file_line).collect();
                    let footer = format!(
                        "(To read a file: doc_read_file({{ uri: \"qtap://{}/<relativePath>\" }}) — or doc_read_file(scope='document_store', mount_point='{}', path='<relativePath>'))",
                        g.mount_point_name, g.mount_point_name
                    );
                    format!("{head}\n{}\n{footer}", lines.join("\n"))
                })
                .collect();
            let mut parts = vec!["## Group Vaults".to_string()];
            parts.extend(blocks);
            parts.join("\n\n")
        }
    }
}

fn format_vault_section(section: &VaultSection) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(c) = &section.character {
        parts.push(format_vault_character(c));
    }
    if let Some(g) = &section.groups {
        parts.push(format_vault_groups(g));
    }
    parts.join("\n\n")
}

fn format_vault_access_participant(p: &VaultAccessParticipant) -> String {
    let mut tags: Vec<&str> = Vec::new();
    if p.is_self {
        tags.push("self");
    }
    if p.controlled_by == "user" {
        tags.push("user persona");
    }
    if p.status == "silent" {
        tags.push("silent");
    }
    let tag_suffix = if tags.is_empty() {
        String::new()
    } else {
        format!(" ({})", tags.join(", "))
    };
    format!("- {}{tag_suffix}", p.character_name)
}

fn format_vault_access_character(section: &VaultAccessCharacterSection) -> String {
    match section {
        VaultAccessCharacterSection::Unavailable { message, .. } => {
            format!("## Vault Access — Character (this chat)\nUnavailable — {message}")
        }
        VaultAccessCharacterSection::Available {
            mount_point_name,
            shared_vaults_enabled,
            participants,
            ..
        } => {
            let toggle_line = if *shared_vaults_enabled {
                "Shared Vaults: ON — other present characters can read this vault."
            } else {
                "Shared Vaults: OFF — only the owner and user persona can access this vault via chat tools."
            };
            let rw: Vec<&VaultAccessParticipant> = participants
                .iter()
                .filter(|p| p.access == "read_write")
                .collect();
            let ro: Vec<&VaultAccessParticipant> = participants
                .iter()
                .filter(|p| p.access == "read_only")
                .collect();
            let rw_block = if rw.is_empty() {
                "(none)".to_string()
            } else {
                rw.iter()
                    .map(|p| format_vault_access_participant(p))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            let ro_block = if ro.is_empty() {
                "(none)".to_string()
            } else {
                ro.iter()
                    .map(|p| format_vault_access_participant(p))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            [
                "## Vault Access — Character (this chat)".to_string(),
                format!("Mount point: {mount_point_name}"),
                toggle_line.to_string(),
                "Read/Write:".to_string(),
                rw_block,
                "Read-only:".to_string(),
                ro_block,
            ]
            .join("\n")
        }
    }
}

fn format_vault_access_groups(section: &VaultAccessGroupsSection) -> String {
    match section {
        VaultAccessGroupsSection::Unavailable { message, .. } => {
            format!("## Vault Access — Groups\nUnavailable — {message}")
        }
        VaultAccessGroupsSection::Available { groups, .. } => {
            if groups.is_empty() {
                return "## Vault Access — Groups\n(no groups)".to_string();
            }
            let blocks: Vec<String> = groups
                .iter()
                .map(|g| {
                    let head = format!("### {}", g.group_name);
                    if g.members.is_empty() {
                        return format!("{head}\n(no members)");
                    }
                    let lines: Vec<String> = g
                        .members
                        .iter()
                        .map(|m| {
                            format!(
                                "- {}{} — read/write",
                                m.character_name,
                                if m.is_self { " (self)" } else { "" }
                            )
                        })
                        .collect();
                    format!(
                        "{head}\nAll members can read and write this group's vault, in any chat:\n{}",
                        lines.join("\n")
                    )
                })
                .collect();
            let mut parts = vec!["## Vault Access — Groups".to_string()];
            parts.extend(blocks);
            parts.join("\n\n")
        }
    }
}

fn format_vault_access_section(section: &VaultAccessSection) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(c) = &section.character {
        parts.push(format_vault_access_character(c));
    }
    if let Some(g) = &section.groups {
        parts.push(format_vault_access_groups(g));
    }
    parts.join("\n\n")
}

fn format_loaded_memories_section(section: &LoadedMemoriesSection) -> String {
    match section {
        LoadedMemoriesSection::Unavailable { message, .. } => {
            format!("## Memories Loaded This Turn\nUnavailable — {message}")
        }
        LoadedMemoriesSection::Available {
            semantic_memories,
            inter_character_memories,
            recap,
            ..
        } => {
            let mut parts = vec!["## Memories Loaded This Turn".to_string()];
            if let Some(r) = recap {
                if !r.is_empty() {
                    parts.push("### Memory Recap".to_string());
                    parts.push(r.clone());
                }
            }
            if !semantic_memories.is_empty() {
                parts.push(format!(
                    "### Relevant Memories ({})",
                    semantic_memories.len()
                ));
                for m in semantic_memories {
                    parts.push(format!(
                        "- [importance {}, score {}, weight {}] {}",
                        to_fixed2(&m.importance),
                        to_fixed2(&m.score),
                        to_fixed2(&m.effective_weight),
                        m.summary
                    ));
                }
            } else {
                parts.push("### Relevant Memories\n(none loaded this turn)".to_string());
            }
            if !inter_character_memories.is_empty() {
                parts.push(format!(
                    "### Memories About Other Characters ({})",
                    inter_character_memories.len()
                ));
                for m in inter_character_memories {
                    parts.push(format!(
                        "- About {} [importance {}]: {}",
                        m.about_character_name,
                        to_fixed2(&m.importance),
                        m.summary
                    ));
                }
            }
            parts.join("\n")
        }
    }
}

fn format_memory_section(section: &MemorySection) -> String {
    if !section.available {
        return format!(
            "## Memory Stats\nUnavailable — {}",
            section.message.as_deref().unwrap_or("unknown error")
        );
    }
    [
        "## Memory Stats".to_string(),
        format!("Total memories: {}", format_number(section.total_count)),
        format!(
            "High-importance (>= {}): {} ({}%)",
            render_num(&section.threshold),
            format_number(section.high_importance_count),
            render_num(&section.high_importance_percent)
        ),
    ]
    .join("\n")
}

fn format_chats_section(section: &ChatSection) -> String {
    if !section.available {
        return format!(
            "## Conversation Stats\nUnavailable — {}",
            section.message.as_deref().unwrap_or("unknown error")
        );
    }
    if section.chat_count == 0 {
        return "## Conversation Stats\nChats: 0\n(no conversations yet)".to_string();
    }
    [
        "## Conversation Stats".to_string(),
        format!("Chats: {}", format_number(section.chat_count)),
        format!(
            "Earliest created: {}",
            section
                .earliest_created_at
                .as_deref()
                .unwrap_or("(unknown)")
        ),
        format!(
            "Most recent activity: {}",
            section.latest_activity_at.as_deref().unwrap_or("(unknown)")
        ),
    ]
    .join("\n")
}

fn format_prompt_section(section: &PromptSection) -> String {
    if !section.available {
        return format!(
            "## Assembled System Prompt\nUnavailable — {}",
            section.message.as_deref().unwrap_or("unknown error")
        );
    }
    [
        "## Assembled System Prompt".to_string(),
        format!(
            "{} chars, ~{} tokens",
            format_number(section.character_count),
            format_number(section.approx_tokens.unwrap_or(0))
        ),
        "(Excludes per-turn tool instructions, memory blocks, conversation history, and wardrobe/status notifications.)".to_string(),
        String::new(),
        "---".to_string(),
        section.system_prompt.clone().unwrap_or_default(),
        "---".to_string(),
    ]
    .join("\n")
}

fn format_last_turn_section(section: &LastTurnSection) -> String {
    if !section.available {
        return format!(
            "## Last-Turn LLM Usage\nUnavailable — {}",
            section.message.as_deref().unwrap_or("unknown error")
        );
    }
    let source_label = if section.source.as_deref() == Some("llm_log") {
        format!(
            "llm_log (logged {})",
            section.logged_at.as_deref().unwrap_or("unknown")
        )
    } else {
        "profile_fallback (no LLM call recorded yet for this chat)".to_string()
    };
    let prompt_tokens = section.prompt_tokens.unwrap_or(0);
    let completion_tokens = section.completion_tokens.unwrap_or(0);
    let total_tokens = section.total_tokens.unwrap_or(0);
    let token_line = format!(
        "Tokens: {} prompt + {} completion = {} total",
        format_number(prompt_tokens),
        format_number(completion_tokens),
        format_number(total_tokens)
    );
    // `section.contextWindow ? ... : ...` — a falsy (0 or null) window prints unknown.
    let window_line = match section.context_window {
        Some(w) if w != 0 => format!(
            "Context window: {} (utilization: {}%)",
            format_number(w),
            section
                .utilization_percent
                .as_ref()
                .map(render_num)
                .unwrap_or_else(|| "0".to_string())
        ),
        _ => "Context window: (unknown)".to_string(),
    };
    [
        "## Last-Turn LLM Usage".to_string(),
        format!("Source: {source_label}"),
        format!(
            "Provider: {} / {}",
            section.provider.as_deref().unwrap_or("(unknown)"),
            section.model_name.as_deref().unwrap_or("(unknown)")
        ),
        token_line,
        window_line,
    ]
    .join("\n")
}

fn format_carina_section(section: &CarinaSection) -> String {
    match section {
        CarinaSection::Unavailable { message, .. } => {
            format!("## Carina\nUnavailable — {message}")
        }
        CarinaSection::Available {
            self_enabled,
            reachable,
            ..
        } => {
            let self_line = if *self_enabled {
                "You ARE a Carina answerer — others can put quick questions to you with @YourName: (public) or @YourName? (whisper), and the ask_carina tool can route to you. Because you are an answerer, a Carina line opens to ANY character (a line opens when either side is an answerer), so you can reach everyone listed below. Queries reach the other party in isolation (no chat history), and the reply renders under the answerer's own avatar."
            } else {
                "You are NOT a Carina answerer — you cannot be addressed with @-queries or reached via the ask_carina tool. You can still reach the Carina answerers listed below (a line opens when either side is an answerer)."
            };
            let reach_block = if reachable.is_empty() {
                if *self_enabled {
                    "Characters you can reach via Carina: (none — there are no other characters)"
                        .to_string()
                } else {
                    "Carina answerers you can reach: (none)".to_string()
                }
            } else if *self_enabled {
                let mut lines = vec![format!(
                    "Characters you can reach via Carina ({}):",
                    reachable.len()
                )];
                for r in reachable {
                    lines.push(format!(
                        "- {}{}",
                        r.name,
                        if r.is_answerer {
                            " (also a Carina answerer)"
                        } else {
                            ""
                        }
                    ));
                }
                lines.join("\n")
            } else {
                let mut lines = vec![format!(
                    "Carina answerers you can reach ({}):",
                    reachable.len()
                )];
                for r in reachable {
                    lines.push(format!("- {}", r.name));
                }
                lines.join("\n")
            };
            [
                "## Carina".to_string(),
                self_line.to_string(),
                String::new(),
                reach_block,
            ]
            .join("\n")
        }
    }
}

fn runtime_mode_label(mode: &str) -> &str {
    match mode {
        "local-dev" => "Local (development)",
        "local-production" => "Local (production)",
        "docker" => "Docker",
        "vm" => "VM (Lima/WSL2)",
        "electron" => "Electron desktop app",
        "electron-docker" => "Electron + Docker",
        "electron-vm" => "Electron + VM",
        // v4 `RUNTIME_MODE_LABELS[mode] ?? mode` — unknown falls back to the raw.
        other => other,
    }
}

fn format_quilltap_section(section: &QuilltapSection) -> String {
    if !section.available {
        return format!(
            "## Quilltap\nUnavailable — {}",
            section.message.as_deref().unwrap_or("unknown error")
        );
    }
    let mut parts = vec!["## Quilltap".to_string()];
    if section.included_parts.version {
        parts.push(format!("Version: {}", section.version));
        parts.push(format!(
            "Runtime: {}",
            runtime_mode_label(&section.runtime_mode)
        ));
        match &section.client_shell {
            ClientShellJson::Electron { shell_version, .. } => {
                parts.push(format!("Client: Electron shell v{shell_version}"));
            }
            ClientShellJson::Browser { .. } => parts.push("Client: Web browser".to_string()),
            ClientShellJson::Unknown { .. } => parts.push("Client: (unknown)".to_string()),
        }
    }
    if section.included_parts.release_notes {
        match &section.release_notes {
            Some(rn) => parts.extend([
                String::new(),
                format!(
                    "### Release Notes (v{})",
                    section.release_notes_version.as_deref().unwrap_or("")
                ),
                rn.clone(),
            ]),
            None => parts.extend([
                String::new(),
                "### Release Notes".to_string(),
                "(no release notes found for this version)".to_string(),
            ]),
        }
    }
    if section.included_parts.changelog {
        match &section.changelog {
            Some(cl) => parts.extend([String::new(), "### Changelog".to_string(), cl.clone()]),
            None => parts.extend([
                String::new(),
                "### Changelog".to_string(),
                "(changelog not available)".to_string(),
            ]),
        }
    }
    parts.join("\n")
}

fn format_context_chat(chat: &ContextChat) -> String {
    if !chat.available {
        return format!(
            "### This Chat\nUnavailable — {}",
            chat.message.as_deref().unwrap_or("unknown error")
        );
    }
    [
        "### This Chat".to_string(),
        format!("- Name: {}", chat.title.as_deref().unwrap_or("(untitled)")),
        format!("- ID: {}", chat.chat_id),
    ]
    .join("\n")
}

fn format_context_project(project: &ContextProject) -> String {
    match project {
        ContextProject::Unavailable { message, .. } => {
            format!("### Project\nUnavailable — {message}")
        }
        ContextProject::Absent { .. } => {
            "### Project\n(this chat is not part of a project)".to_string()
        }
        ContextProject::Present {
            id,
            name,
            mount_points,
            ..
        } => {
            let stores = if mount_points.is_empty() {
                "(none)".to_string()
            } else {
                mount_points
                    .iter()
                    .map(|m| m.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            [
                "### Project".to_string(),
                format!("- Name: {name}"),
                format!("- ID: {id}"),
                format!("- Linked stores: {stores}"),
            ]
            .join("\n")
        }
    }
}

fn format_context_groups(groups: &ContextGroups) -> String {
    match groups {
        ContextGroups::Unavailable { message, .. } => {
            format!("### Your Groups\nUnavailable — {message}")
        }
        ContextGroups::Available { groups, .. } => {
            if groups.is_empty() {
                return "### Your Groups\n(you are not a member of any groups)".to_string();
            }
            let mut lines = vec!["### Your Groups".to_string()];
            for g in groups {
                let stores = if g.mount_points.is_empty() {
                    "(none)".to_string()
                } else {
                    g.mount_points
                        .iter()
                        .map(|m| m.name.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                lines.push(format!(
                    "- {} (id: {}) — linked stores: {}",
                    g.name, g.id, stores
                ));
            }
            lines.join("\n")
        }
    }
}

fn format_context_characters(characters: &ContextCharacters) -> String {
    match characters {
        ContextCharacters::Unavailable { message, .. } => {
            format!("### Characters Present\nUnavailable — {message}")
        }
        ContextCharacters::Available { characters, .. } => {
            if characters.is_empty() {
                return "### Characters Present\n(no other characters are present in this chat)"
                    .to_string();
            }
            let mut lines = vec!["### Characters Present".to_string()];
            for c in characters {
                let persona_tag = if c.is_user_persona {
                    " (user persona)"
                } else {
                    ""
                };
                let alias_tag = if c.aliases.is_empty() {
                    String::new()
                } else {
                    format!(" [aka {}]", c.aliases.join(", "))
                };
                let identity_line = match &c.identity {
                    Some(id) if !id.is_empty() => format!("\n    Identity: {id}"),
                    _ => String::new(),
                };
                lines.push(format!(
                    "- {}{persona_tag}{alias_tag} (id: {}){identity_line}",
                    c.name, c.id
                ));
            }
            lines.join("\n")
        }
    }
}

fn format_context_files(files: &ContextFiles) -> String {
    match files {
        ContextFiles::Unavailable { message, .. } => {
            format!("### Attached Files\nUnavailable — {message}")
        }
        ContextFiles::Available { files, .. } => {
            if files.is_empty() {
                return "### Attached Files\n(no files are attached to this chat)".to_string();
            }
            let mut lines = vec!["### Attached Files".to_string()];
            for f in files {
                let title = match &f.display_title {
                    Some(t) if !t.is_empty() => format!(" \"{t}\""),
                    _ => String::new(),
                };
                let mount_tag = match &f.mount_point {
                    Some(mp) if !mp.is_empty() => format!(", mount_point={mp}"),
                    _ => String::new(),
                };
                lines.push(format!(
                    "- {}{title} [scope={}{mount_tag}]\n    Reach it: {}",
                    f.file_path, f.scope, f.how_to_reach
                ));
            }
            lines.join("\n")
        }
    }
}

fn format_context_section(section: &ContextSection) -> String {
    let mut parts = vec!["## Context".to_string()];
    if let Some(c) = &section.chat {
        parts.push(String::new());
        parts.push(format_context_chat(c));
    }
    if let Some(p) = &section.project {
        parts.push(String::new());
        parts.push(format_context_project(p));
    }
    if let Some(g) = &section.groups {
        parts.push(String::new());
        parts.push(format_context_groups(g));
    }
    if let Some(ch) = &section.characters {
        parts.push(String::new());
        parts.push(format_context_characters(ch));
    }
    if let Some(f) = &section.files {
        parts.push(String::new());
        parts.push(format_context_files(f));
    }
    parts.join("\n")
}

/// Render the full self-inventory report (v4 `formatSelfInventoryResults`).
pub fn format_self_inventory(output: &SelfInventoryOutput) -> String {
    if !output.success {
        return format!(
            "You are running on Quilltap v{}.\n\nSelf-Inventory Error: {}",
            output.quilltap_version,
            output.error.as_deref().unwrap_or("Unknown error")
        );
    }

    let mut lines = vec![
        format!("You are running on Quilltap v{}.", output.quilltap_version),
        String::new(),
        "# Self-Inventory Report".to_string(),
        format!(
            "Character: {} (id: {})",
            output.character_name, output.character_id
        ),
    ];

    if let Some(v) = &output.vault {
        lines.push(String::new());
        lines.push(format_vault_section(v));
    }
    if let Some(va) = &output.vault_access {
        lines.push(String::new());
        lines.push(format_vault_access_section(va));
    }
    if let Some(m) = &output.memory {
        lines.push(String::new());
        lines.push(format_memory_section(m));
    }
    if let Some(lm) = &output.loaded_memories {
        lines.push(String::new());
        lines.push(format_loaded_memories_section(lm));
    }
    if let Some(c) = &output.chats {
        lines.push(String::new());
        lines.push(format_chats_section(c));
    }
    if let Some(p) = &output.prompt {
        lines.push(String::new());
        lines.push(format_prompt_section(p));
    }
    if let Some(lt) = &output.last_turn {
        lines.push(String::new());
        lines.push(format_last_turn_section(lt));
    }
    if let Some(ca) = &output.carina {
        lines.push(String::new());
        lines.push(format_carina_section(ca));
    }
    if let Some(q) = &output.quilltap {
        lines.push(String::new());
        lines.push(format_quilltap_section(q));
    }
    if let Some(ctx) = &output.context {
        lines.push(String::new());
        lines.push(format_context_section(ctx));
    }

    lines.join("\n")
}
