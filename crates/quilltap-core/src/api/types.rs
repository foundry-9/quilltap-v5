//! The pure contract types of the Core API boundary (Phase-4 D8): the
//! `Request`/`Response` enums, the scope-tagged `Event` envelope, the DTOs,
//! and the error/readiness vocabulary. No IO here — every transport (axum,
//! CLI, Tauri, uniffi later) marshals these and nothing else.
//!
//! Growth rule (D7): variants are **action-centric** and added when a consumer
//! (an SPA vertical, a CLI subcommand, a test) needs them — never enumerated
//! speculatively. v4's ~124 routes / ~162 action verbs are the *checklist*
//! for eventual coverage, not the wire shape.
//!
//! Wire tagging: `Request` is internally tagged (`{"type": "unlock",
//! "passphrase": "…"}` — the natural dispatch-JSON shape); `Response` is
//! adjacently tagged (`{"type": "chats", "data": […]}`) so list payloads can
//! carry a tag. The HTTP envelope semantics (v4 `lib/api/responses.ts`,
//! the 503/423 readiness statuses) are the transport's marshalling concern
//! (P4.2), not this layer's.

use serde::{Deserialize, Serialize};

use crate::services::chat_events::ChatEvent;
use crate::services::creation_progress::CreationProgressFrame;

// ============================================================================
// Readiness (v4 DbKeyState — lib/startup/dbkey.ts)
// ============================================================================

/// v4's `DbKeyState`, verbatim strings on the wire. `Resolved` and
/// `NeedsVaultStorage` are both **operational** (the pepper is in hand; the
/// latter just recommends storing it in a `.dbkey` file) — v4
/// `startupState.isPepperResolved` treats them identically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PepperState {
    #[serde(rename = "resolved")]
    Resolved,
    #[serde(rename = "needs-setup")]
    NeedsSetup,
    #[serde(rename = "needs-passphrase")]
    NeedsPassphrase,
    #[serde(rename = "needs-vault-storage")]
    NeedsVaultStorage,
}

impl PepperState {
    /// v4 `isPepperResolved`: whether the engine can run (pepper available).
    pub fn is_operational(self) -> bool {
        matches!(self, PepperState::Resolved | PepperState::NeedsVaultStorage)
    }
}

// ============================================================================
// Request / Response
// ============================================================================

/// One variant per user-meaningful operation (api-boundary.md Part 1). The
/// always-available family (health, unlock, instances) works while the vault
/// is locked; everything else is readiness-gated in dispatch (D2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Request {
    /// Liveness + readiness (v4 `GET /health` + `startup-status` essentials).
    Health,
    /// The unlock-family state read (v4 `GET /api/v1/system/unlock`).
    UnlockState,
    /// Unlock a passphrase-protected vault (v4 `?action=unlock`).
    Unlock {
        passphrase: String,
    },
    /// First-run setup (v4 `?action=setup`): mint a pepper, write
    /// `quilltap.dbkey`, provision a fresh encrypted instance (schema + baseline
    /// seed), and assemble. Only valid from `needs-setup`. Returns the pepper
    /// ONCE (shown for the user to save).
    Setup {
        passphrase: String,
    },
    /// Store an env-provided pepper in a `.dbkey` file (v4 `?action=store`): the
    /// `needs-vault-storage` → `resolved` transition. Only valid from
    /// `needs-vault-storage`.
    #[serde(rename_all = "camelCase")]
    StorePepper {
        passphrase: String,
    },
    /// Change the passphrase that wraps the pepper (v4 `?action=change-passphrase`):
    /// re-wrap only, no DB re-encryption. Only valid when unlocked (`resolved`).
    /// Either passphrase may be empty (the no-passphrase sentinel).
    #[serde(rename_all = "camelCase")]
    ChangePassphrase {
        old_passphrase: String,
        new_passphrase: String,
    },
    /// Lock the application (v4 `?action=lock` / auto-lock). Tears the engine
    /// down; the vault returns to `needs-passphrase`.
    Lock,
    /// List registered instances (the v4 launcher's instance registry).
    ListInstances,
    /// List the single user's chats, enriched (v4 `GET /api/v1/chats` →
    /// `handleList` + `enrichChatsForList`). The optional params mirror v4's
    /// query string.
    #[serde(rename_all = "camelCase")]
    ListChats {
        /// v4 `?excludeTagIds=a,b` — drop chats carrying any of these tags.
        #[serde(default)]
        exclude_tag_ids: Vec<String>,
        /// v4 `?limit=N` — cap the post-filter list (applied only when `> 0`).
        #[serde(default)]
        limit: Option<i64>,
        /// v4 `?includeAutonomous=true` — include autonomous rooms regardless of
        /// their `runVisibility`.
        #[serde(default)]
        include_autonomous: bool,
    },
    /// The single-chat GET (v4 `GET /api/v1/chats/{id}` → `handleGet` default
    /// branch): the fully-enriched chat + all messages (minus `renderedHtml`).
    #[serde(rename_all = "camelCase")]
    ChatGet {
        chat_id: String,
    },
    /// The chat-settings read (v4 `GET /api/v1/settings/chat`). Now
    /// default-injects the seed row when none exists (P4.6d).
    ChatSettings,
    /// The chat-settings PUT (v4 `PUT /api/v1/settings/chat`): the partial field
    /// bag folds into the `updateForUser` upsert. Returns the updated row.
    ChatSettingsUpdate {
        settings: serde_json::Value,
    },
    // --- Connection profiles (v4 connection-profiles/route.ts + [id]/) ---
    /// v4 `GET /api/v1/connection-profiles` — the enriched list.
    #[serde(rename_all = "camelCase")]
    ConnectionProfileList {
        /// v4 `?imageCapable=true` — filter to image-generation-capable providers.
        #[serde(default)]
        image_capable: bool,
    },
    /// v4 `POST /api/v1/connection-profiles` (create) — the field bag.
    ConnectionProfileCreate {
        profile: serde_json::Value,
    },
    /// v4 `PUT /api/v1/connection-profiles/[id]` — the field bag.
    #[serde(rename_all = "camelCase")]
    ConnectionProfileUpdate {
        profile_id: String,
        profile: serde_json::Value,
    },
    /// v4 `DELETE /api/v1/connection-profiles/[id]`.
    #[serde(rename_all = "camelCase")]
    ConnectionProfileDelete {
        profile_id: String,
    },
    /// v4 `?action=reorder` — the contract's ordered-id list.
    #[serde(rename_all = "camelCase")]
    ConnectionProfileReorder {
        ordered_ids: Vec<String>,
    },
    /// v4 `?action=reset-sort`.
    ConnectionProfileResetSort,
    /// v4 `?action=test-connection` — the `{provider, apiKeyId?, baseUrl?}` bag.
    ConnectionProfileTest {
        profile: serde_json::Value,
    },
    /// v4 `?action=test-message` — the `{provider, apiKeyId?, baseUrl?, modelName,
    /// parameters?}` bag.
    ConnectionProfileTestMessage {
        profile: serde_json::Value,
    },
    // --- API keys (v4 api-keys/route.ts + [id]/) ---
    /// v4 `GET /api/v1/api-keys` — the masked list.
    ApiKeyList,
    /// v4 `POST /api/v1/api-keys` (create).
    #[serde(rename_all = "camelCase")]
    ApiKeyCreate {
        label: String,
        provider: String,
        api_key: String,
    },
    /// v4 `PUT /api/v1/api-keys/[id]`.
    #[serde(rename_all = "camelCase")]
    ApiKeyUpdate {
        api_key_id: String,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        is_active: Option<bool>,
        #[serde(default)]
        api_key: Option<String>,
    },
    /// v4 `DELETE /api/v1/api-keys/[id]`.
    #[serde(rename_all = "camelCase")]
    ApiKeyDelete {
        api_key_id: String,
    },
    /// v4 `POST /api/v1/api-keys/[id]?action=test`.
    #[serde(rename_all = "camelCase")]
    ApiKeyTest {
        api_key_id: String,
        #[serde(default)]
        base_url: Option<String>,
    },
    // --- Providers + models (v4 providers/route.ts + models/route.ts) ---
    /// v4 `GET /api/v1/providers`.
    ProviderList,
    /// v4 `GET /api/v1/models` (+ `?provider=`) — the cached read.
    #[serde(rename_all = "camelCase")]
    ModelList {
        #[serde(default)]
        provider: Option<String>,
    },
    /// v4 `POST /api/v1/models` — the live fetch + cache.
    #[serde(rename_all = "camelCase")]
    ModelFetch {
        provider: String,
        #[serde(default)]
        api_key_id: Option<String>,
        #[serde(default)]
        base_url: Option<String>,
    },
    /// A turn action (v4 `POST /api/v1/chats/{id}/actions?action=turn` →
    /// `handleTurnAction`): nudge/queue/dequeue/query/skipUserTurn.
    #[serde(rename_all = "camelCase")]
    ChatTurnAction {
        chat_id: String,
        action: String,
        #[serde(default)]
        participant_id: Option<String>,
    },
    /// Edit a message's content (v4 `PUT /api/v1/messages/{id}`).
    #[serde(rename_all = "camelCase")]
    MessageEdit {
        message_id: String,
        content: String,
    },
    /// Delete a message / swipe group (v4 `DELETE /api/v1/messages/{id}`) — the
    /// memory-cascade confirmation protocol.
    #[serde(rename_all = "camelCase")]
    MessageDelete {
        message_id: String,
        /// v4 `?memoryAction=` — DELETE_MEMORIES / KEEP_MEMORIES /
        /// REGENERATE_MEMORIES / ASK_EVERY_TIME.
        #[serde(default)]
        memory_action: Option<String>,
        /// v4 `?skipConfirmation=true`.
        #[serde(default)]
        skip_confirmation: bool,
    },
    /// Swipe a message (v4 `POST /api/v1/messages/{id}?action=swipe`): switch
    /// (`swipeIndex` present) or generate (absent — needs the model driver).
    #[serde(rename_all = "camelCase")]
    MessageSwipe {
        message_id: String,
        #[serde(default)]
        swipe_index: Option<i64>,
    },
    /// The general chat edit (v4 `PUT /api/v1/chats/{id}` → `processChatUpdates`):
    /// the Salon pause/resume + title path. `chat` is the partial field bag
    /// (`updateChatSchema`).
    #[serde(rename_all = "camelCase")]
    ChatUpdate {
        chat_id: String,
        chat: serde_json::Value,
    },
    /// Start impersonating a participant (v4 `POST …?action=impersonate`).
    #[serde(rename_all = "camelCase")]
    ChatImpersonate {
        chat_id: String,
        participant_id: String,
    },
    /// Stop impersonating (v4 `POST …?action=stop-impersonate`); the optional new
    /// connection profile flips the participant back to `controlledBy:'llm'`.
    #[serde(rename_all = "camelCase")]
    ChatStopImpersonate {
        chat_id: String,
        participant_id: String,
        #[serde(default)]
        new_connection_profile_id: Option<String>,
    },
    /// Set the active typing/speaking participant (v4 `POST …?action=set-active-speaker`).
    #[serde(rename_all = "camelCase")]
    ChatSetActiveSpeaker {
        chat_id: String,
        participant_id: String,
    },
    /// Send a chat message and run the full turn (v4
    /// `POST /api/v1/chats/{id}/messages` → `handleSendMessage`). The stream
    /// frames ride the [`Event`] channel (chat-scoped); the dispatch reply is
    /// the typed result of the initial turn. Fields project v4's
    /// `SendMessageOptions`.
    #[serde(rename_all = "camelCase")]
    ChatSend {
        chat_id: String,
        #[serde(default)]
        content: String,
        #[serde(default)]
        continue_mode: bool,
        #[serde(default)]
        responding_participant_id: Option<String>,
        #[serde(default)]
        target_participant_ids: Option<Vec<String>>,
        #[serde(default)]
        speaking_as_participant_id: Option<String>,
        #[serde(default)]
        file_ids: Vec<String>,
        /// v4 continue-mode `nudge` — the human explicitly summoned this character
        /// (withholds the "nothing to add" skip option for this turn).
        #[serde(default)]
        nudge: Option<bool>,
        /// v4 `pendingToolResults` — user-initiated tool results pre-inserted as
        /// TOOL messages before the user message.
        #[serde(default)]
        pending_tool_results: Vec<PendingToolResult>,
    },
    /// Create a chat and run the full seed sequence (v4 `POST /api/v1/chats` →
    /// `handleCreate`). The dispatch payload's create fields (everything but
    /// `type`) are flattened into `request` and handed to the driver, which
    /// deserializes them into a
    /// [`ChatCreateRequest`](crate::services::chat_create::ChatCreateRequest).
    /// Creation-progress frames ride the [`Event`] channel (scope-tagged by
    /// `progressId`); the dispatch reply is the created chat.
    ChatCreate {
        #[serde(flatten)]
        request: serde_json::Map<String, serde_json::Value>,
    },
    // ========================================================================
    // Characters family (P4.6f) — the characters-server surface. Every variant
    // is a differential port of a v4 route handler; response bytes are pinned by
    // the `characters_{reads,mutations}_equivalence` differentials, not the loose
    // `serde_json::Value` payload types. This block is BINDING (identical in the
    // sibling P4.6g SPA order): a name change here changes both orders.
    // ========================================================================
    /// v4 `GET /api/v1/characters` (`handleGet`): the enriched whitelist DTO
    /// list. `npc`/`controlledBy` mirror v4's query-string filters verbatim
    /// (`"true"`/`"false"`, `"user"`/`"llm"`).
    #[serde(rename_all = "camelCase")]
    CharacterList {
        #[serde(default)]
        npc: Option<String>,
        #[serde(default)]
        controlled_by: Option<String>,
    },
    /// v4 `GET /api/v1/characters/[id]` default branch: the detail projection.
    #[serde(rename_all = "camelCase")]
    CharacterGet {
        character_id: String,
    },
    /// v4 `POST /api/v1/characters` (create): the `createCharacterSchema` bag.
    CharacterCreate {
        character: serde_json::Value,
    },
    /// v4 `POST /api/v1/characters?action=quick-create`.
    CharacterQuickCreate {
        name: String,
    },
    /// v4 `PUT /api/v1/characters/[id]` default branch: the `updateCharacterSchema`
    /// bag folded onto the pre-update read.
    #[serde(rename_all = "camelCase")]
    CharacterUpdate {
        character_id: String,
        character: serde_json::Value,
    },
    /// v4 `DELETE /api/v1/characters/[id]` (+ `cascadeChats`/`cascadeImages`).
    #[serde(rename_all = "camelCase")]
    CharacterDelete {
        character_id: String,
        #[serde(default)]
        cascade_chats: bool,
        #[serde(default)]
        cascade_images: bool,
    },
    /// v4 `GET /api/v1/characters/[id]?action=cascade-preview`.
    #[serde(rename_all = "camelCase")]
    CharacterCascadePreview {
        character_id: String,
    },
    /// v4 `POST /api/v1/characters/[id]?action=avatar` (`{imageId: string|null}`).
    #[serde(rename_all = "camelCase")]
    CharacterAvatar {
        character_id: String,
        #[serde(default)]
        image_id: Option<String>,
    },
    /// v4 `POST /api/v1/characters/[id]?action=favorite`.
    #[serde(rename_all = "camelCase")]
    CharacterFavorite {
        character_id: String,
    },
    /// v4 `POST /api/v1/characters/[id]?action=toggle-controlled-by`.
    #[serde(rename_all = "camelCase")]
    CharacterToggleControlledBy {
        character_id: String,
    },
    /// v4 `POST /api/v1/characters/[id]?action=toggle-carina`.
    #[serde(rename_all = "camelCase")]
    CharacterToggleCarina {
        character_id: String,
    },
    /// v4 `POST /api/v1/characters/[id]?action=set-default-partner`.
    #[serde(rename_all = "camelCase")]
    CharacterSetDefaultPartner {
        character_id: String,
        #[serde(default)]
        partner_id: Option<String>,
    },
    /// v4 `POST /api/v1/characters/[id]?action=add-tag`.
    #[serde(rename_all = "camelCase")]
    CharacterAddTag {
        character_id: String,
        tag_id: String,
    },
    /// v4 `POST /api/v1/characters/[id]?action=remove-tag`.
    #[serde(rename_all = "camelCase")]
    CharacterRemoveTag {
        character_id: String,
        tag_id: String,
    },
    /// v4 `GET /api/v1/characters/[id]?action=get-tags`.
    #[serde(rename_all = "camelCase")]
    CharacterGetTags {
        character_id: String,
    },
    /// v4 `GET /api/v1/characters/[id]?action=default-partner`.
    #[serde(rename_all = "camelCase")]
    CharacterDefaultPartner {
        character_id: String,
    },
    /// v4 `GET /api/v1/characters/[id]?action=stats`.
    #[serde(rename_all = "camelCase")]
    CharacterStats {
        character_id: String,
    },
    /// v4 `GET /api/v1/characters/[id]?action=chats`.
    #[serde(rename_all = "camelCase")]
    CharacterChats {
        character_id: String,
        #[serde(default)]
        search: Option<String>,
        #[serde(default)]
        limit: Option<i64>,
        #[serde(default)]
        offset: Option<i64>,
    },
    /// v4 `GET /api/v1/characters/[id]?action=depiction-guidelines`.
    #[serde(rename_all = "camelCase")]
    CharacterDepictionGuidelines {
        character_id: String,
    },
    /// v4 `PUT /api/v1/characters/[id]?action=depiction-guidelines`.
    #[serde(rename_all = "camelCase")]
    CharacterDepictionGuidelinesUpdate {
        character_id: String,
        #[serde(default)]
        content: String,
    },
    // --- Sub-resource: system prompts ---
    #[serde(rename_all = "camelCase")]
    CharacterPromptList {
        character_id: String,
    },
    #[serde(rename_all = "camelCase")]
    CharacterPromptCreate {
        character_id: String,
        name: String,
        content: String,
        #[serde(default)]
        is_default: bool,
    },
    #[serde(rename_all = "camelCase")]
    CharacterPromptUpdate {
        character_id: String,
        prompt_id: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        content: Option<String>,
        #[serde(default)]
        is_default: Option<bool>,
    },
    #[serde(rename_all = "camelCase")]
    CharacterPromptDelete {
        character_id: String,
        prompt_id: String,
    },
    #[serde(rename_all = "camelCase")]
    CharacterPromptSetDefault {
        character_id: String,
        prompt_id: String,
    },
    // --- Sub-resource: scenarios ---
    #[serde(rename_all = "camelCase")]
    CharacterScenarioList {
        character_id: String,
    },
    #[serde(rename_all = "camelCase")]
    CharacterScenarioCreate {
        character_id: String,
        title: String,
        content: String,
    },
    #[serde(rename_all = "camelCase")]
    CharacterScenarioUpdate {
        character_id: String,
        scenario_id: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        content: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    CharacterScenarioDelete {
        character_id: String,
        scenario_id: String,
    },
    // --- Sub-resource: plugin data ---
    #[serde(rename_all = "camelCase")]
    CharacterPluginDataMap {
        character_id: String,
    },
    #[serde(rename_all = "camelCase")]
    CharacterPluginDataUpsert {
        character_id: String,
        plugin_name: String,
        data: serde_json::Value,
    },
    #[serde(rename_all = "camelCase")]
    CharacterPluginDataGet {
        character_id: String,
        plugin_name: String,
    },
    #[serde(rename_all = "camelCase")]
    CharacterPluginDataDelete {
        character_id: String,
        plugin_name: String,
    },
    // --- Sub-resource: wardrobe ---
    #[serde(rename_all = "camelCase")]
    CharacterWardrobeList {
        character_id: String,
    },
    #[serde(rename_all = "camelCase")]
    CharacterWardrobeCreate {
        character_id: String,
        item: serde_json::Value,
    },
    #[serde(rename_all = "camelCase")]
    CharacterWardrobeGet {
        character_id: String,
        item_id: String,
    },
    #[serde(rename_all = "camelCase")]
    CharacterWardrobeUpdate {
        character_id: String,
        item_id: String,
        item: serde_json::Value,
    },
    #[serde(rename_all = "camelCase")]
    CharacterWardrobeDelete {
        character_id: String,
        item_id: String,
    },
    // --- Import / export ---
    /// v4 `GET /api/v1/characters/[id]?action=export` (`format=json|png`).
    #[serde(rename_all = "camelCase")]
    CharacterExport {
        character_id: String,
        #[serde(default)]
        format: Option<String>,
    },
    /// v4 `POST /api/v1/characters?action=import` (JSON body leg; the PNG leg is
    /// the quilltap-web multipart route).
    CharacterImport {
        payload: serde_json::Value,
    },
    // --- Photo gallery (JSON legs; multipart save is the web route) ---
    #[serde(rename_all = "camelCase")]
    CharacterPhotoList {
        character_id: String,
        #[serde(default)]
        limit: Option<i64>,
        #[serde(default)]
        offset: Option<i64>,
    },
    #[serde(rename_all = "camelCase")]
    CharacterPhotoSaveById {
        character_id: String,
        #[serde(default)]
        file_id: Option<String>,
        #[serde(default)]
        link_id: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    CharacterPhotoRemove {
        character_id: String,
        link_id: String,
    },
    // --- Tags (v4 tags/route.ts + tags/[id]/route.ts) ---
    #[serde(rename_all = "camelCase")]
    TagList {
        #[serde(default)]
        search: Option<String>,
    },
    TagCreate {
        name: String,
    },
    #[serde(rename_all = "camelCase")]
    TagGet {
        tag_id: String,
    },
    #[serde(rename_all = "camelCase")]
    TagUpdate {
        tag_id: String,
        tag: serde_json::Value,
    },
    #[serde(rename_all = "camelCase")]
    TagDelete {
        tag_id: String,
    },

    // ========================================================================
    // Groups + Projects family (P4.6k) — the Prospero dispatch backfill. Every
    // variant is a differential port of a v4 route handler; response bytes are
    // pinned by the `groups_routes_equivalence` / `projects_routes_equivalence`
    // differentials, not the loose `serde_json::Value` payload types. This block
    // is BINDING (identical in the sibling P4.6l SPA order): a name change here
    // changes both orders. Optional body fields are the v4 Zod schema fields.
    // ========================================================================
    // --- Groups ---
    /// v4 `GET /api/v1/groups` — createdAt-desc list + `_count.members`.
    GroupList,
    /// v4 `POST /api/v1/groups` (`createGroupSchema`).
    #[serde(rename_all = "camelCase")]
    GroupCreate {
        name: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        color: Option<String>,
        #[serde(default)]
        icon: Option<String>,
    },
    /// v4 `GET /api/v1/groups/[id]` default — detail + `_count.members`.
    #[serde(rename_all = "camelCase")]
    GroupGet {
        group_id: String,
    },
    /// v4 `PUT /api/v1/groups/[id]` (`updateGroupSchema`).
    #[serde(rename_all = "camelCase")]
    GroupUpdate {
        group_id: String,
        group: serde_json::Value,
    },
    /// v4 `DELETE /api/v1/groups/[id]` default.
    #[serde(rename_all = "camelCase")]
    GroupDelete {
        group_id: String,
    },
    /// v4 `GET /api/v1/groups/[id]?action=members`.
    #[serde(rename_all = "camelCase")]
    GroupMembers {
        group_id: String,
    },
    /// v4 `POST /api/v1/groups/[id]?action=addMember`.
    #[serde(rename_all = "camelCase")]
    GroupMemberAdd {
        group_id: String,
        character_id: String,
    },
    /// v4 `DELETE /api/v1/groups/[id]?action=removeMember`.
    #[serde(rename_all = "camelCase")]
    GroupMemberRemove {
        group_id: String,
        character_id: String,
    },
    /// v4 `GET /api/v1/groups/[id]/mount-points`.
    #[serde(rename_all = "camelCase")]
    GroupMountPointList {
        group_id: String,
    },
    /// v4 `POST /api/v1/groups/[id]/mount-points`.
    #[serde(rename_all = "camelCase")]
    GroupMountPointLink {
        group_id: String,
        mount_point_id: String,
    },
    /// v4 `DELETE /api/v1/groups/[id]/mount-points`.
    #[serde(rename_all = "camelCase")]
    GroupMountPointUnlink {
        group_id: String,
        mount_point_id: String,
    },
    /// v4 `GET /api/v1/groups/[id]/scenarios`.
    #[serde(rename_all = "camelCase")]
    GroupScenarioList {
        group_id: String,
    },
    /// v4 `POST /api/v1/groups/[id]/scenarios` (`createScenarioSchema`).
    #[serde(rename_all = "camelCase")]
    GroupScenarioCreate {
        group_id: String,
        scenario: serde_json::Value,
    },
    /// v4 `GET /api/v1/groups/[id]/scenarios/[scenarioPath]`.
    #[serde(rename_all = "camelCase")]
    GroupScenarioGet {
        group_id: String,
        scenario_path: String,
    },
    /// v4 `PUT /api/v1/groups/[id]/scenarios/[scenarioPath]`.
    #[serde(rename_all = "camelCase")]
    GroupScenarioUpdate {
        group_id: String,
        scenario_path: String,
        scenario: serde_json::Value,
    },
    /// v4 `POST /api/v1/groups/[id]/scenarios/[scenarioPath]?action=rename`.
    #[serde(rename_all = "camelCase")]
    GroupScenarioRename {
        group_id: String,
        scenario_path: String,
        new_filename: String,
    },
    /// v4 `DELETE /api/v1/groups/[id]/scenarios/[scenarioPath]`.
    #[serde(rename_all = "camelCase")]
    GroupScenarioDelete {
        group_id: String,
        scenario_path: String,
    },
    /// v4 `GET /api/v1/groups/scenarios?characterIds=` — the participant-union.
    #[serde(rename_all = "camelCase")]
    GroupScenariosUnion {
        character_ids: Vec<String>,
    },
    // --- General (instance-wide "Quilltap General") scenarios (P4.6n) ---
    /// v4 `GET /api/v1/scenarios` — the instance-wide scenario list (race arm:
    /// `mountPointId: null` + empty arrays when unprovisioned).
    ScenarioList,
    /// v4 `POST /api/v1/scenarios` (race arm: 400 when unprovisioned).
    ScenarioCreate {
        scenario: serde_json::Value,
    },
    /// v4 `GET /api/v1/scenarios/[scenarioPath]`.
    #[serde(rename_all = "camelCase")]
    ScenarioGet {
        scenario_path: String,
    },
    /// v4 `PUT /api/v1/scenarios/[scenarioPath]`.
    #[serde(rename_all = "camelCase")]
    ScenarioUpdate {
        scenario_path: String,
        scenario: serde_json::Value,
    },
    /// v4 `POST /api/v1/scenarios/[scenarioPath]?action=rename`.
    #[serde(rename_all = "camelCase")]
    ScenarioRename {
        scenario_path: String,
        new_filename: String,
    },
    /// v4 `DELETE /api/v1/scenarios/[scenarioPath]`.
    #[serde(rename_all = "camelCase")]
    ScenarioDelete {
        scenario_path: String,
    },
    // --- Roleplay templates (P4.6p) ---
    /// v4 `GET /api/v1/roleplay-templates` — a BARE JSON array (built-in-first,
    /// then `name` localeCompare).
    RoleplayTemplateList,
    /// v4 `POST /api/v1/roleplay-templates` (hand-validated body).
    RoleplayTemplateCreate {
        template: serde_json::Value,
    },
    /// v4 `GET /api/v1/roleplay-templates/[id]` (read-time renderingPatterns
    /// auto-gen, non-persisted).
    #[serde(rename_all = "camelCase")]
    RoleplayTemplateGet {
        template_id: String,
    },
    /// v4 `PUT /api/v1/roleplay-templates/[id]` (`updateRoleplayTemplateSchema`).
    #[serde(rename_all = "camelCase")]
    RoleplayTemplateUpdate {
        template_id: String,
        template: serde_json::Value,
    },
    /// v4 `DELETE /api/v1/roleplay-templates/[id]` → `{success, deletedId}`.
    #[serde(rename_all = "camelCase")]
    RoleplayTemplateDelete {
        template_id: String,
    },
    // --- Image profiles (P4.6p) ---
    /// v4 `GET /api/v1/image-profiles` (+ optional `?sortByCharacter=`) →
    /// `{profiles, count}`.
    #[serde(rename_all = "camelCase")]
    ImageProfileList {
        #[serde(default)]
        sort_by_character: Option<String>,
    },
    /// v4 `POST /api/v1/image-profiles` → created profile + `apiKey`.
    ImageProfileCreate {
        profile: serde_json::Value,
    },
    /// v4 `GET /api/v1/image-profiles/[id]` → profile + `apiKey` + enriched tags.
    #[serde(rename_all = "camelCase")]
    ImageProfileGet {
        profile_id: String,
    },
    /// v4 `PUT /api/v1/image-profiles/[id]` (per-field gated) → updated + enrichment.
    #[serde(rename_all = "camelCase")]
    ImageProfileUpdate {
        profile_id: String,
        profile: serde_json::Value,
    },
    /// v4 `DELETE /api/v1/image-profiles/[id]` → `{message}`.
    #[serde(rename_all = "camelCase")]
    ImageProfileDelete {
        profile_id: String,
    },
    /// v4 `GET /api/v1/image-profiles?action=list-providers` → `{providers, count}`.
    ImageProviderList,
    /// v4 `POST /api/v1/image-profiles/[id]?action=generate` — LLM/IO-coupled
    /// (loud refusal arm this round).
    #[serde(rename_all = "camelCase")]
    ImageProfileGenerate {
        profile_id: String,
        #[serde(default)]
        payload: serde_json::Value,
    },
    /// v4 `POST /api/v1/image-profiles?action=validate-key` — live IO (loud refusal
    /// arm this round).
    ImageProfileValidateKey {
        #[serde(default)]
        payload: serde_json::Value,
    },
    /// v4 `GET /api/v1/image-profiles?action=list-models` — live IO (loud refusal
    /// arm this round).
    #[serde(rename_all = "camelCase")]
    ImageProfileListModels {
        #[serde(default)]
        provider: Option<String>,
        #[serde(default)]
        api_key_id: Option<String>,
    },
    // --- Global mount points (P4.6p) ---
    /// v4 `GET /api/v1/mount-points` → `{mountPoints}` (createdAt DESC).
    MountPointList,
    /// v4 `GET /api/v1/mount-points/[id]` → `{mountPoint: {…, embeddedChunkCount,
    /// capabilities}}`.
    #[serde(rename_all = "camelCase")]
    MountPointGet {
        mount_point_id: String,
    },
    /// v4 `POST /api/v1/mount-points` → `{mountPoint, warning?}`.
    #[serde(rename_all = "camelCase")]
    MountPointCreate {
        mount_point: serde_json::Value,
    },
    /// v4 `PATCH /api/v1/mount-points/[id]` → `{mountPoint}` (no count/capabilities).
    #[serde(rename_all = "camelCase")]
    MountPointUpdate {
        mount_point_id: String,
        mount_point: serde_json::Value,
    },
    /// v4 `DELETE /api/v1/mount-points/[id]` → `{message}` (the ordered cascade).
    #[serde(rename_all = "camelCase")]
    MountPointDelete {
        mount_point_id: String,
    },
    // --- Projects ---
    /// v4 `GET /api/v1/projects` — createdAt-desc list + `_count` (BARE envelope).
    ProjectList,
    /// v4 `POST /api/v1/projects` (`createProjectSchema`).
    ProjectCreate {
        project: serde_json::Value,
    },
    /// v4 `GET /api/v1/projects/[id]` default — detail + enriched roster.
    #[serde(rename_all = "camelCase")]
    ProjectGet {
        project_id: String,
    },
    /// v4 `PUT /api/v1/projects/[id]` (`updateProjectSchema`).
    #[serde(rename_all = "camelCase")]
    ProjectUpdate {
        project_id: String,
        project: serde_json::Value,
    },
    /// v4 `DELETE /api/v1/projects/[id]` default.
    #[serde(rename_all = "camelCase")]
    ProjectDelete {
        project_id: String,
    },
    /// v4 `GET /api/v1/projects/[id]?action=list-characters`.
    #[serde(rename_all = "camelCase")]
    ProjectCharacterList {
        project_id: String,
    },
    /// v4 `POST /api/v1/projects/[id]?action=add-character`.
    #[serde(rename_all = "camelCase")]
    ProjectCharacterAdd {
        project_id: String,
        character_id: String,
    },
    /// v4 `DELETE /api/v1/projects/[id]?action=remove-character`.
    #[serde(rename_all = "camelCase")]
    ProjectCharacterRemove {
        project_id: String,
        character_id: String,
    },
    /// v4 `GET /api/v1/projects/[id]?action=list-chats`.
    #[serde(rename_all = "camelCase")]
    ProjectChatList {
        project_id: String,
        #[serde(default)]
        limit: Option<i64>,
        #[serde(default)]
        offset: Option<i64>,
    },
    /// v4 `POST /api/v1/projects/[id]?action=add-chat`.
    #[serde(rename_all = "camelCase")]
    ProjectChatAdd {
        project_id: String,
        chat_id: String,
    },
    /// v4 `DELETE /api/v1/projects/[id]?action=remove-chat`.
    #[serde(rename_all = "camelCase")]
    ProjectChatRemove {
        project_id: String,
        chat_id: String,
    },
    /// v4 `GET /api/v1/projects/[id]?action=list-files`.
    #[serde(rename_all = "camelCase")]
    ProjectFileList {
        project_id: String,
    },
    /// v4 `POST /api/v1/projects/[id]?action=add-file`.
    #[serde(rename_all = "camelCase")]
    ProjectFileAdd {
        project_id: String,
        file_id: String,
    },
    /// v4 `DELETE /api/v1/projects/[id]?action=remove-file`.
    #[serde(rename_all = "camelCase")]
    ProjectFileRemove {
        project_id: String,
        file_id: String,
    },
    /// v4 `GET /api/v1/projects/[id]?action=get-state`.
    #[serde(rename_all = "camelCase")]
    ProjectStateGet {
        project_id: String,
    },
    /// v4 `PUT /api/v1/projects/[id]?action=set-state`.
    #[serde(rename_all = "camelCase")]
    ProjectStateSet {
        project_id: String,
        state: serde_json::Value,
    },
    /// v4 `DELETE /api/v1/projects/[id]?action=reset-state`.
    #[serde(rename_all = "camelCase")]
    ProjectStateReset {
        project_id: String,
    },
    /// v4 `GET /api/v1/projects/[id]?action=get-background`.
    #[serde(rename_all = "camelCase")]
    ProjectBackgroundGet {
        project_id: String,
    },
    /// v4 `GET /api/v1/projects/[id]?action=aesthetic&kind=`.
    #[serde(rename_all = "camelCase")]
    ProjectAestheticGet {
        project_id: String,
        kind: String,
    },
    /// v4 `PUT /api/v1/projects/[id]?action=aesthetic&kind=`.
    #[serde(rename_all = "camelCase")]
    ProjectAestheticSet {
        project_id: String,
        kind: String,
        #[serde(default)]
        content: Option<String>,
    },
    /// v4 `POST /api/v1/projects/[id]?action=update-tool-settings`.
    #[serde(rename_all = "camelCase")]
    ProjectToolSettingsUpdate {
        project_id: String,
        #[serde(default)]
        default_disabled_tools: Vec<String>,
        #[serde(default)]
        default_disabled_tool_groups: Vec<String>,
    },
    /// v4 `GET /api/v1/projects/[id]/mount-points`.
    #[serde(rename_all = "camelCase")]
    ProjectMountPointList {
        project_id: String,
    },
    /// v4 `POST /api/v1/projects/[id]/mount-points`.
    #[serde(rename_all = "camelCase")]
    ProjectMountPointLink {
        project_id: String,
        mount_point_id: String,
    },
    /// v4 `DELETE /api/v1/projects/[id]/mount-points`.
    #[serde(rename_all = "camelCase")]
    ProjectMountPointUnlink {
        project_id: String,
        mount_point_id: String,
    },
    /// v4 `GET /api/v1/projects/[id]/scenarios`.
    #[serde(rename_all = "camelCase")]
    ProjectScenarioList {
        project_id: String,
    },
    /// v4 `POST /api/v1/projects/[id]/scenarios`.
    #[serde(rename_all = "camelCase")]
    ProjectScenarioCreate {
        project_id: String,
        scenario: serde_json::Value,
    },
    /// v4 `GET /api/v1/projects/[id]/scenarios/[scenarioPath]`.
    #[serde(rename_all = "camelCase")]
    ProjectScenarioGet {
        project_id: String,
        scenario_path: String,
    },
    /// v4 `PUT /api/v1/projects/[id]/scenarios/[scenarioPath]`.
    #[serde(rename_all = "camelCase")]
    ProjectScenarioUpdate {
        project_id: String,
        scenario_path: String,
        scenario: serde_json::Value,
    },
    /// v4 `POST /api/v1/projects/[id]/scenarios/[scenarioPath]?action=rename`.
    #[serde(rename_all = "camelCase")]
    ProjectScenarioRename {
        project_id: String,
        scenario_path: String,
        new_filename: String,
    },
    /// v4 `DELETE /api/v1/projects/[id]/scenarios/[scenarioPath]`.
    #[serde(rename_all = "camelCase")]
    ProjectScenarioDelete {
        project_id: String,
        scenario_path: String,
    },
    /// v4 `GET /api/v1/projects/[id]/wardrobe`.
    #[serde(rename_all = "camelCase")]
    ProjectWardrobeList {
        project_id: String,
    },
    /// v4 `POST /api/v1/projects/[id]/wardrobe`.
    #[serde(rename_all = "camelCase")]
    ProjectWardrobeCreate {
        project_id: String,
        item: serde_json::Value,
    },
    /// v4 `GET /api/v1/projects/[id]/wardrobe/[itemId]`.
    #[serde(rename_all = "camelCase")]
    ProjectWardrobeGet {
        project_id: String,
        item_id: String,
    },
    /// v4 `PUT /api/v1/projects/[id]/wardrobe/[itemId]`.
    #[serde(rename_all = "camelCase")]
    ProjectWardrobeUpdate {
        project_id: String,
        item_id: String,
        item: serde_json::Value,
    },
    /// v4 `DELETE /api/v1/projects/[id]/wardrobe/[itemId]`.
    #[serde(rename_all = "camelCase")]
    ProjectWardrobeDelete {
        project_id: String,
        item_id: String,
    },
    // --- Memories (P4.6s) — v4 `memories/route.ts` + `[id]/route.ts` +
    //     `chats/[id]/actions/memories.ts`. See `api::memories`. ---
    /// v4 GET `?characterId=` — the two-path list (paginated when `limit` present,
    /// else legacy-all). `{memories: (…&{tagDetails})[], count, totalCount}`.
    #[serde(rename_all = "camelCase")]
    MemoryList {
        character_id: String,
        #[serde(default)]
        search: Option<String>,
        #[serde(default)]
        min_importance: Option<f64>,
        #[serde(default)]
        source: Option<String>,
        #[serde(default)]
        sort_by: Option<String>,
        #[serde(default)]
        sort_order: Option<String>,
        #[serde(default)]
        limit: Option<i64>,
        #[serde(default)]
        offset: Option<i64>,
    },
    /// v4 GET `/api/v1/memories/[id]` — `{memory}` (+ tagDetails; access-time bump).
    #[serde(rename_all = "camelCase")]
    MemoryGet {
        memory_id: String,
    },
    /// v4 GET `?chatId=` — `{chatId, memoryCount}`.
    #[serde(rename_all = "camelCase")]
    MemoryCountByChat {
        chat_id: String,
    },
    /// v4 GET `?messageId=` — the trimmed `{memoryCount, isSwipeGroup, swipeCount,
    /// memories: [{id, summary, characterId, importance}]}`.
    #[serde(rename_all = "camelCase")]
    MemoryByMessage {
        message_id: String,
    },
    /// v4 GET `?action=character-memory-counts` — `{success, characters: [{id,
    /// name, memoryCount}]}` (memoryCount desc).
    MemoryCharacterCounts,
    /// v4 POST `/api/v1/memories` (no action) — the gate-driven create; 201
    /// `{memory}`. Embedding failure → server-error (no row).
    MemoryCreate {
        memory: serde_json::Value,
    },
    /// v4 PUT `/api/v1/memories/[id]` — `{memory}` (NO re-embed).
    #[serde(rename_all = "camelCase")]
    MemoryUpdate {
        memory_id: String,
        memory: serde_json::Value,
    },
    /// v4 DELETE `/api/v1/memories/[id]` — `{success: true}` (+ unlink scrub).
    #[serde(rename_all = "camelCase")]
    MemoryDelete {
        memory_id: String,
    },
    /// v4 DELETE `/api/v1/memories?chatId=` — `{success, chatId, deletedCount}`.
    #[serde(rename_all = "camelCase")]
    MemoryDeleteByChat {
        chat_id: String,
    },
    /// v4 POST `?action=search` — `{memories: (…&{score, usedEmbedding,
    /// tagDetails})[], count, query, usedEmbedding}` (builtin TF-IDF LIVE).
    MemorySearch {
        search: serde_json::Value,
    },
    /// v4 GET `?action=housekeep&characterId=` — `{success, preview: {…}}`.
    #[serde(rename_all = "camelCase")]
    MemoryHousekeepPreview {
        character_id: String,
        #[serde(default)]
        max_memories: Option<i64>,
        #[serde(default)]
        max_age_months: Option<i64>,
        #[serde(default)]
        min_importance: Option<f64>,
        #[serde(default)]
        merge_similar: Option<bool>,
    },
    /// v4 POST `?action=housekeep` — `{success, dryRun, result: {…, details?}}`.
    MemoryHousekeep {
        options: serde_json::Value,
    },
    /// v4 POST `?action=housekeep-sweep` — `{success, jobId}`.
    MemoryHousekeepSweep,
    /// v4 GET `?action=housekeeping-config` — `{success, settings}`.
    MemoryHousekeepingConfigGet,
    /// v4 POST `?action=housekeeping-config` — merge-patch `{success, settings}`.
    MemoryHousekeepingConfigSet {
        config: serde_json::Value,
    },
    /// v4 GET `?action=recall-config` — `{success, settings}`.
    MemoryRecallConfigGet,
    /// v4 POST `?action=recall-config` — merge-patch `{success, settings}`.
    MemoryRecallConfigSet {
        config: serde_json::Value,
    },
    /// v4 GET `?action=extraction-limits-config` — `{success, settings}`.
    MemoryExtractionLimitsGet,
    /// v4 POST `?action=extraction-limits-config` — merge-patch `{success, settings}`.
    MemoryExtractionLimitsSet {
        config: serde_json::Value,
    },
    /// v4 GET `?action=extraction-concurrency` — `{success, concurrency}`.
    MemoryExtractionConcurrencyGet,
    /// v4 POST `?action=extraction-concurrency` — `{success, concurrency}`.
    MemoryExtractionConcurrencySet {
        concurrency: i64,
    },
}

/// Typed DTO per variant (the uniffi payoff). `Error` carries the one
/// cross-cutting error envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum Response {
    Health(HealthDto),
    UnlockState(UnlockStateDto),
    /// v4 setup response — the minted pepper (shown once) + the save-it message.
    Setup(SetupResultDto),
    /// The empty-body success ack (v4 `successResponse({})`) — `storePepper` /
    /// `changePassphrase`.
    Ack(AckDto),
    Instances(InstancesDto),
    /// v4 `handleList`'s `cleanEnrichedChats` output — the enriched chat list.
    Chats(Vec<crate::services::chat_enrichment::EnrichedChatSummary>),
    ChatSend(ChatSendResultDto),
    /// The v4 `POST /api/v1/chats` 201 body (`{ chat: {...} }`).
    ChatCreate(ChatCreateResultDto),
    /// The single-chat GET / chat PUT body (`{ chat: {...} }`).
    Chat(ChatWrapDto),
    /// v4 `GET /api/v1/settings/chat` body (the raw settings object).
    ChatSettings(serde_json::Value),
    /// v4 `handleTurnAction` body.
    TurnAction(serde_json::Value),
    /// v4 message edit / swipe body (`{ message: {...} }`).
    Message(serde_json::Value),
    /// v4 message-delete body (the confirmation body OR `{ success, memoriesDeleted }`).
    MessageDelete(serde_json::Value),
    /// v4 impersonation-verb body (`{ success, ... }`).
    ChatImpersonation(serde_json::Value),
    // --- Settings surface (P4.6d) ---
    /// v4 connection-profiles list (`{profiles, count}`).
    ConnectionProfiles(serde_json::Value),
    /// v4 connection-profile create/update/get (`{profile}`).
    ConnectionProfile(serde_json::Value),
    /// v4 connection test-connection / test-message body.
    ConnectionTest(serde_json::Value),
    /// v4 api-keys list (`{apiKeys, count}`).
    ApiKeys(serde_json::Value),
    /// v4 api-key create/update/get (`{apiKey}`).
    ApiKey(serde_json::Value),
    /// v4 api-key test body (`{valid, error?}`).
    ApiKeyTest(serde_json::Value),
    /// v4 providers listing (`{providers, count}`).
    Providers(serde_json::Value),
    /// v4 models read/fetch body.
    Models(serde_json::Value),
    // --- Characters surface (P4.6f) ---
    /// A single-object characters-family body (`{character}`, `{data}`, the
    /// action bodies, `{prompts}`/`{scenarios}`/`{wardrobeItems}`/`{pluginData}`,
    /// `{stats,groups}`, `{chats,total}`, `{content}`, the export/import/photo
    /// bodies, …). The exact bytes are pinned by the differentials.
    Character(serde_json::Value),
    /// v4 `GET /api/v1/characters` body (`{characters, count}`).
    Characters(serde_json::Value),
    /// v4 tags list body (`{tags, count}`).
    Tags(serde_json::Value),
    /// v4 single-tag body (`{tag}`).
    Tag(serde_json::Value),
    // --- Groups + Projects surface (P4.6k) ---
    /// A groups-family body (`{groups}`, `{group}`, `{members}`, `{mountPoints}`,
    /// `{link, mountPoint}`, `{mountPointId, scenarios, warnings}`,
    /// `{groupScenarios}`, `{success}`, `{message}`, …). The exact bytes are
    /// pinned by `groups_routes_equivalence`.
    Group(serde_json::Value),
    /// A projects-family body (`{projects}`, `{project}`, `{characters,count}`,
    /// `{chats,pagination}`, `{files,count}`, `{success,state}`,
    /// `{success,previousState}`, `{backgroundUrl,displayMode}`, `{content}`,
    /// `{wardrobeItem}`, …). The exact bytes are pinned by
    /// `projects_routes_equivalence`.
    Project(serde_json::Value),
    /// A general (instance-wide) scenarios-family body (`{mountPointId, scenarios,
    /// warnings}`, `{mountPointId, path, scenarios, warnings}`, `{scenario}`,
    /// `{path, scenarios, warnings}`, `{scenarios, warnings}`). The exact bytes are
    /// pinned by `scenarios_routes_equivalence` (P4.6n).
    Scenario(serde_json::Value),
    // --- Listing surfaces (P4.6p) ---
    /// A roleplay-templates-family body: the LIST is a **bare JSON array** of
    /// templates; create/get/update return the bare template; delete returns
    /// `{success, deletedId}`. The exact bytes are pinned by
    /// `roleplay_templates_routes_equivalence`.
    RoleplayTemplate(serde_json::Value),
    /// An image-profiles-family body (`{profiles, count}`, the bare
    /// profile+`apiKey`(+`tags`), `{message}`, `{providers, count}`, the
    /// `{valid, message, modelCount?}` verdict, …). Pinned by
    /// `image_profiles_routes_equivalence`.
    ImageProfile(serde_json::Value),
    /// A mount-points-family body (`{mountPoints}`, `{mountPoint}`,
    /// `{mountPoint, warning?}`, `{message}`). Pinned by
    /// `mount_points_routes_equivalence`.
    MountPoint(serde_json::Value),
    /// A memories-family body — the list `{memories, count, totalCount}`, the item
    /// `{memory}`, the by-message trimmed shape, the search
    /// `{memories, count, query, usedEmbedding}`, the housekeeping envelopes, the
    /// config get/set `{success, settings|concurrency|...}`, the job-enqueue
    /// `{success, jobId|enqueued|...}`, the per-chat `{success, jobCount, ...}`.
    /// The exact bytes are pinned by `memories_routes_equivalence` (P4.6s).
    Memory(serde_json::Value),
    Error(CoreError),
}

impl Response {
    /// Shorthand for an error response.
    pub fn error(kind: ErrorKind, message: impl Into<String>) -> Response {
        Response::Error(CoreError {
            kind,
            message: message.into(),
            pepper_state: None,
        })
    }

    /// The readiness-gate refusal (D2): dispatch answers this for every
    /// ready-gated variant while the vault is locked; the HTTP transport maps
    /// it to 503/423 with the setup URL.
    pub fn locked(pepper_state: PepperState) -> Response {
        Response::Error(CoreError {
            kind: ErrorKind::Locked,
            message: "The database is locked. Unlock it to continue.".to_string(),
            pepper_state: Some(pepper_state),
        })
    }
}

// ============================================================================
// DTOs
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthDto {
    /// Always `"ok"` if dispatch answered at all.
    pub status: String,
    /// The core/app version string.
    pub version: String,
    /// Whether the engine is assembled and serving (pepper operational).
    pub ready: bool,
    pub pepper_state: PepperState,
}

/// v4 `?action=setup` success body: the pepper is returned ONCE (the user must
/// save it — it is never displayed again).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupResultDto {
    pub pepper: String,
    pub message: String,
}

/// The empty success body (v4 `successResponse({})`). Serializes to `{}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AckDto {}

/// v4 `GET /api/v1/system/unlock` body: `{ state, hasUserPassphrase,
/// autoLockMinutes }` — `autoLockMinutes` only populated when unlocked and
/// the user's auto-lock setting is enabled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockStateDto {
    pub state: PepperState,
    pub has_user_passphrase: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_lock_minutes: Option<f64>,
}

/// One registered instance from the launcher registry (`instances.json`).
/// Never carries the stored passphrase — only whether one is stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceDto {
    pub name: String,
    pub path: String,
    pub is_default: bool,
    pub has_passphrase: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstancesDto {
    pub instances: Vec<InstanceDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_instance: Option<String>,
}

/// A chat list row — a projection of the (differential-verified) chat read
/// shape, not new marshaling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSummaryDto {
    pub id: String,
    pub title: String,
    pub chat_type: String,
    pub message_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// The typed result of a `ChatSend` dispatch — a projection of the spine's
/// [`ProcessMessageResult`](crate::services::message_finalizer::ProcessMessageResult)
/// (the frames themselves ride the [`Event`] channel).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSendResultDto {
    /// The assistant message id the turn minted (or targeted, in continue mode).
    pub message_id: String,
    pub has_content: bool,
    pub is_multi_character: bool,
    pub is_paused: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_participant_id: Option<String>,
}

/// The typed result of a `ChatCreate` dispatch. Serializes to v4's 201 body,
/// `{ "chat": { ...chat, "participants": [EnrichedParticipantSummary] } }`:
/// `chat` is the full hydrated chat row whose own `participants` array has been
/// REPLACED by the enriched participant summaries (the driver merges
/// [`ChatCreateResult`](crate::services::chat_create::ChatCreateResult)'s two
/// halves before constructing this). The SPA's chat-create call and the P4.5 TS
/// contract mirror both consume this shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatCreateResultDto {
    /// The created chat, with `participants` = the enriched summaries.
    pub chat: serde_json::Value,
}

/// The `{ chat: {...} }` wrapper the single-chat GET + chat PUT return.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatWrapDto {
    pub chat: serde_json::Value,
}

/// v4 `pendingToolResultSchema` element — a user-initiated tool result the send
/// route pre-inserts as a TOOL message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingToolResult {
    pub tool: String,
    pub success: bool,
    pub result: String,
    pub prompt: String,
    pub arguments: serde_json::Map<String, serde_json::Value>,
    pub created_at: String,
}

// ============================================================================
// Errors
// ============================================================================

/// The cross-transport error envelope. `kind` follows v4's response-helper
/// vocabulary (`lib/api/responses.ts`); the transport maps kinds to statuses
/// (bad-request → 400, not-found → 404, locked → 423/503 per D2, internal →
/// 500).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreError {
    pub kind: ErrorKind,
    pub message: String,
    /// Populated on readiness refusals so the client can route to the right
    /// unlock/setup screen without a second round-trip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pepper_state: Option<PepperState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorKind {
    BadRequest,
    /// v4 `unauthorized` (HTTP 401): a wrong passphrase on `changePassphrase`.
    Unauthorized,
    /// v4 `forbidden` (HTTP 403): e.g. editing/deleting a built-in roleplay
    /// template.
    Forbidden,
    NotFound,
    /// v4 `conflict` / an explicit `errorResponse(msg, 409)`: a duplicate name
    /// on roleplay-template / image-profile create/update.
    Conflict,
    Locked,
    Internal,
}

// ============================================================================
// Events (D3): one global stream, every event scope-tagged
// ============================================================================

/// The server-push envelope. Scope ids identify what the payload is about so
/// one global stream per client suffices (D3); the payload flattens into the
/// envelope, so a chat frame serializes as v4's SSE frame plus its scope tag.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_id: Option<String>,
    #[serde(flatten)]
    pub payload: EventPayload,
}

/// The event families (phase-4.md "Event families"). P4.0 defines the one
/// vocabulary that already exists — the chat stream frames
/// ([`ChatEvent`], byte-identical to v4's SSE `StreamChunkData`). Creation
/// progress (D6) and the low-vocabulary progress frames join as their
/// producers are ported.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum EventPayload {
    Chat(ChatEvent),
    /// v4's transport-shell error frame (`handleStreamError` →
    /// `encodeErrorEvent`: `{error, errorType, details}`). The ported
    /// `process_message` propagates its error to the caller; the TRANSPORT owns
    /// the frame — the spine driver emits this when the turn errors, exactly
    /// where v4's stream shell does.
    ChatError(ChatErrorPayload),
    /// A chat-creation progress frame (D6, "The Green Room" —
    /// `services::creation_progress`), scope-tagged by `progress_id`. Serializes
    /// to v4's `{kind, …, ts}` frame shape; the [`Event`] envelope adds the
    /// `progressId` tag.
    CreationProgress(CreationProgressFrame),
}

/// v4 `encodeErrorEvent(encoder, error, errorType, details)`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChatErrorPayload {
    /// v4 hardcodes `'Failed to generate response'` at the send-path shell.
    pub error: String,
    #[serde(rename = "errorType")]
    pub error_type: String,
    pub details: String,
}

impl Event {
    /// A chat-scoped stream frame.
    pub fn chat(chat_id: impl Into<String>, frame: ChatEvent) -> Event {
        Event {
            chat_id: Some(chat_id.into()),
            room_id: None,
            progress_id: None,
            payload: EventPayload::Chat(frame),
        }
    }

    /// A chat-scoped transport-shell error frame (v4 `handleStreamError`).
    pub fn chat_error(chat_id: impl Into<String>, payload: ChatErrorPayload) -> Event {
        Event {
            chat_id: Some(chat_id.into()),
            room_id: None,
            progress_id: None,
            payload: EventPayload::ChatError(payload),
        }
    }

    /// A creation-progress frame scope-tagged by `progress_id` (D6). The
    /// transport uses this to replay a `CreationProgressBus` backlog onto a new
    /// `/api/events` stream.
    pub fn creation_progress(
        progress_id: impl Into<String>,
        frame: CreationProgressFrame,
    ) -> Event {
        Event {
            chat_id: None,
            room_id: None,
            progress_id: Some(progress_id.into()),
            payload: EventPayload::CreationProgress(frame),
        }
    }
}
