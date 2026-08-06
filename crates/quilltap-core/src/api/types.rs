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
        // ── P4.9E1A: the bag's participant families (additive, all defaulted) ──
        // v4's `chatUpdateRequestSchema` carries these as SIBLINGS of `chat`, and
        // `processChatUpdates` runs them in this order after the `chat` bag. They
        // are `serde_json::Value` because they are the SAME field bags the
        // `chatUpdateParticipant` / `chatAddParticipant` verbs carry — the handler
        // coerces them through the one shared parser so the two entrances cannot
        // drift (v4 calls the identical `helpers.ts` functions from both).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        update_participant: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        add_participant: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remove_participant_id: Option<String>,
        // ── end P4.9E1A ──
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
    /// (still a loud refusal arm; the P4.6ab tier-2 un-refusal is OPEN). Params
    /// follow the P4.6ab/ac/ad Shared contract (`{imageProfileId, prompt,
    /// chatId?, count?}`) so the SPA's generate dialog reaches the refusal
    /// envelope rather than a parse error (reconciled at unification).
    #[serde(rename_all = "camelCase")]
    ImageProfileGenerate {
        image_profile_id: String,
        prompt: String,
        #[serde(default)]
        chat_id: Option<String>,
        #[serde(default)]
        count: Option<i64>,
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
    // --- The chat / group / general state tiers (P4.d10 §A, v4 `f48f34dc`) ---
    /// v4 `GET /api/v1/chats/[id]?action=get-state` — the merged four-tier
    /// cascade under the participants-union group scope. Body:
    /// `{ success, state, chatState, projectState?, groupState?,
    /// generalState?, groupTier, projectId? }` — the three inherited tiers
    /// OMITTED when empty; `projectId` omitted when the chat has none.
    #[serde(rename_all = "camelCase")]
    ChatStateGet {
        chat_id: String,
    },
    /// v4 `PUT /api/v1/chats/[id]?action=set-state` — REPLACES the chat's own
    /// state wholesale. Body `{ success, state }`.
    #[serde(rename_all = "camelCase")]
    ChatStateSet {
        chat_id: String,
        state: serde_json::Value,
    },
    /// v4 `DELETE /api/v1/chats/[id]?action=reset-state`. Body
    /// `{ success, previousState }`.
    #[serde(rename_all = "camelCase")]
    ChatStateReset {
        chat_id: String,
    },
    /// v4 `GET /api/v1/groups/[id]?action=get-state` — the group's OWN state
    /// only, NO cascade (the merge happens on the chat route). Body
    /// `{ success, state }`.
    #[serde(rename_all = "camelCase")]
    GroupStateGet {
        group_id: String,
    },
    /// v4 `PUT /api/v1/groups/[id]?action=set-state`.
    #[serde(rename_all = "camelCase")]
    GroupStateSet {
        group_id: String,
        state: serde_json::Value,
    },
    /// v4 `DELETE /api/v1/groups/[id]?action=reset-state`.
    #[serde(rename_all = "camelCase")]
    GroupStateReset {
        group_id: String,
    },
    /// v4 `GET /api/v1/settings/general-state` — the instance-wide bottom tier.
    #[serde(rename_all = "camelCase")]
    GeneralStateGet {},
    /// v4 `PUT /api/v1/settings/general-state` — replaces wholesale
    /// (`stateBodySchema`: `state` must be a JSON object).
    #[serde(rename_all = "camelCase")]
    GeneralStateSet {
        state: serde_json::Value,
    },
    /// v4 `DELETE /api/v1/settings/general-state` — resets to `{}`. Body
    /// `{ success, previousState }`.
    #[serde(rename_all = "camelCase")]
    GeneralStateReset {},
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
    /// v4 GET `?action=backfill-embeddings` — `{success, progress: {remaining,
    /// inFlight}}`.
    MemoryBackfillProgress,
    /// v4 GET `?action=regenerate-all` — `{success, inFlightFanOut, inFlightWipes,
    /// inFlightExtractions, inFlight}`.
    MemoryRegenerateAllStatus,
    /// v4 POST `?action=regenerate-all` — `{success, jobId, isNew, cleared,
    /// message}`.
    MemoryRegenerateAll,
    /// v4 GET `?action=embeddings&characterId=` — `{total, withEmbeddings,
    /// withoutEmbeddings, percentComplete, embeddingProfileConfigured,
    /// embeddingProfileName}`.
    #[serde(rename_all = "camelCase")]
    MemoryEmbeddingStatus {
        character_id: String,
    },
    /// v4 POST `?action=backfill-embeddings` — `{success, enqueued, remaining,
    /// message}`.
    #[serde(rename_all = "camelCase")]
    MemoryBackfillStart {
        #[serde(default)]
        character_id: Option<String>,
        #[serde(default)]
        batch_size: Option<i64>,
    },
    /// v4 POST `?action=embeddings` — deferred (the `generateMissingEmbeddings`
    /// service is unported); recognized-but-refused.
    #[serde(rename_all = "camelCase")]
    MemoryGenerateEmbeddings {
        character_id: String,
        #[serde(default)]
        batch_size: Option<i64>,
    },
    /// v4 PUT `?action=embeddings` — deferred (the `rebuildVectorIndex` service is
    /// unported); recognized-but-refused.
    #[serde(rename_all = "camelCase")]
    MemoryRebuildIndex {
        character_id: String,
        confirm: bool,
    },
    /// v4 per-chat `?action=queue-memories` — queue one per-turn
    /// `MEMORY_EXTRACTION` job per turn in the chat (P4.6BM). v4's legacy
    /// `characterId`/`characterName`/`messagePairs` request fields are accepted
    /// by its Zod schema and ignored by the handler, so `chat_id` is the whole
    /// live surface.
    #[serde(rename_all = "camelCase")]
    ChatQueueMemories {
        chat_id: String,
    },
    // === P4.6v: mount files (lane A, append-only) ===
    /// v4 `GET /api/v1/mount-points/[id]/files` → `{ files, folders }` (the full
    /// `DocMountFileLinkWithContent` set + the merged folder path set). Lane C's
    /// DocumentPicker consumes this.
    #[serde(rename_all = "camelCase")]
    MountFilesList {
        mount_point_id: String,
    },
    /// v4 `GET /api/v1/mount-points/[id]/files/[...path]` JSON envelope form →
    /// the `readMountFile` result (`{mountPointId, relativePath, encoding,
    /// content, mtime, sha256, sizeBytes, mimeType, fileType, totalLines?,
    /// truncated?}`).
    #[serde(rename_all = "camelCase")]
    MountFileRead {
        mount_point_id: String,
        path: String,
        #[serde(default)]
        encoding: Option<String>,
        #[serde(default)]
        offset: Option<i64>,
        #[serde(default)]
        limit: Option<i64>,
    },
    /// v4 `POST /api/v1/mount-points/[id]?action=scan` → `{ scanResult,
    /// embeddingJobsEnqueued }` (the synchronous scan + the post-scan embedding
    /// enqueue).
    #[serde(rename_all = "camelCase")]
    MountScan {
        mount_point_id: String,
    },
    /// v4 `POST ?action=reindex` → `{mountPointId, mountName, processed,
    /// succeeded, failed, skipped, errors}` (synchronous in-request re-chunk).
    #[serde(rename_all = "camelCase")]
    MountReindex {
        mount_point_id: String,
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        force: Option<bool>,
    },
    /// v4 `POST ?action=embed` → `{mountPointId, mountName, jobs, queued,
    /// skipped}` (the scoped EMBEDDING_GENERATE enqueue).
    #[serde(rename_all = "camelCase")]
    MountEmbed {
        mount_point_id: String,
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        force: Option<bool>,
    },
    /// v4 collection-route `?action=semantic-search` → `{results, count, query,
    /// embeddingModel, embeddingDimensions}`. `search` carries the v4
    /// `semanticSearchSchema` fields verbatim (`query`, `mountPointIds?`,
    /// `projectId?`, `pathPrefix?`, `top?`, `threshold?`), parsed in-handler.
    #[serde(rename_all = "camelCase")]
    MountSemanticSearch {
        #[serde(flatten)]
        search: serde_json::Value,
    },
    /// v4 `?action=move-file` → the v4 `FileOpResult` (strategy + sha pair).
    #[serde(rename_all = "camelCase")]
    MountFileMove {
        mount_point_id: String,
        source_path: String,
        dest_mount_point_id: String,
        dest_path: String,
    },
    /// v4 `?action=copy-file` → the v4 `FileOpResult`.
    #[serde(rename_all = "camelCase")]
    MountFileCopy {
        mount_point_id: String,
        source_path: String,
        dest_mount_point_id: String,
        dest_path: String,
        #[serde(default)]
        force: Option<bool>,
    },
    /// v4 `?action=link-file` → the v4 `FileOpResult` (true hard link; cross-
    /// storage/device → `UNSUPPORTED`).
    #[serde(rename_all = "camelCase")]
    MountFileLink {
        mount_point_id: String,
        source_path: String,
        dest_mount_point_id: String,
        dest_path: String,
    },
    /// v4 `?action=delete-file` → `{deleted, mountPointId, path}` (the item
    /// route's DELETE rides the same variant at the web edge, 404-ing
    /// `deleted:false`).
    #[serde(rename_all = "camelCase")]
    MountFileDelete {
        mount_point_id: String,
        path: String,
    },
    /// v4 `?action=delete-folder` → `{mountPointId, path}` (`CONFLICT` when
    /// non-empty).
    #[serde(rename_all = "camelCase")]
    MountFolderDelete {
        mount_point_id: String,
        path: String,
    },
    /// v4 `?action=move-folder` → `{mountPointId, fromPath, toPath}`
    /// (same-mount only).
    #[serde(rename_all = "camelCase")]
    MountFolderMove {
        mount_point_id: String,
        from_path: String,
        to_path: String,
    },
    /// v4 item-route PATCH (rename and/or describe; ≥1 required; rename runs
    /// FIRST; descriptions are blob-only) → `{mountPointId, relativePath,
    /// renamed, descriptionUpdated, fileType?}`.
    #[serde(rename_all = "camelCase")]
    MountFileUpdate {
        mount_point_id: String,
        path: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        rename: Option<String>,
    },
    /// v4 `POST .../folders` (`isPathSafe`-guarded) → `{path}`.
    #[serde(rename_all = "camelCase")]
    MountFolderCreate {
        mount_point_id: String,
        path: String,
    },
    /// v4 item-route PUT (the `storeMountFile` ingest, overwrite semantics) →
    /// `{mountPointId, relativePath, kind, fileType, sha256, sizeBytes,
    /// mimeType, mtime}`. The multipart PUT leg maps onto this at the web edge
    /// (base64 content + originalMimeType/originalFileName riders).
    #[serde(rename_all = "camelCase")]
    MountFileWrite {
        mount_point_id: String,
        path: String,
        content: String,
        #[serde(default)]
        encoding: Option<String>,
        #[serde(default)]
        expected_mtime: Option<i64>,
        #[serde(default)]
        force: Option<bool>,
        #[serde(default)]
        original_mime_type: Option<String>,
        #[serde(default)]
        original_file_name: Option<String>,
    },
    /// v4 `?action=write-file` — the BYTE-PRESERVING `file_ops::writeFile`
    /// (behavior-keyed apart from the transcoding PUT; the multipart action leg
    /// maps here) → `{sha256, sizeBytes, destPath, mountPointId}`.
    #[serde(rename_all = "camelCase")]
    MountFileWriteRaw {
        mount_point_id: String,
        path: String,
        /// Raw bytes, base64.
        data: String,
        #[serde(default)]
        force: Option<bool>,
    },
    /// v4 `POST .../blobs` (multipart at the web edge; 201) → `{document}` for
    /// a native-text upload, else `{blob}` (the full joined view).
    #[serde(rename_all = "camelCase")]
    MountBlobUpload {
        mount_point_id: String,
        path: String,
        #[serde(default)]
        description: Option<String>,
        /// File bytes, base64 (decoded from the multipart part at the edge).
        data: String,
        #[serde(default)]
        original_mime_type: Option<String>,
        #[serde(default)]
        original_file_name: Option<String>,
    },
    /// v4 `GET .../blobs` (`?folder=` scoped) → `{blobs}`.
    #[serde(rename_all = "camelCase")]
    MountBlobsList {
        mount_point_id: String,
        #[serde(default)]
        folder: Option<String>,
    },
    /// v4 blob-item DELETE (documents-table fallback preserved) → `{success}`.
    #[serde(rename_all = "camelCase")]
    MountBlobDelete {
        mount_point_id: String,
        path: String,
    },
    /// v4 blob-item PATCH (description; blob rows only) → `{blob}`.
    #[serde(rename_all = "camelCase")]
    MountBlobUpdate {
        mount_point_id: String,
        path: String,
        #[serde(default)]
        description: Option<String>,
    },
    /// v4 `?action=convert` — REFUSAL-ARMED (loud, typed; the guards run live,
    /// the conversion machinery is a named P4.6y deferral).
    #[serde(rename_all = "camelCase")]
    MountConvert {
        mount_point_id: String,
    },
    /// v4 `?action=deconvert` — REFUSAL-ARMED (see `MountConvert`).
    #[serde(rename_all = "camelCase")]
    MountDeconvert {
        mount_point_id: String,
        target_path: String,
    },
    // === P4.6w: documents ===
    // Chat-scoped Document Mode (all take `chatId`). See `api::documents`. The
    // schema-bag variants flatten the remaining fields into `body` (the exact
    // shapes are the v4 Zod schemas, parsed inside the handler).
    /// v4 `active-document` — `{ document | null }` (earliest-opened active).
    #[serde(rename_all = "camelCase")]
    ChatActiveDocument {
        chat_id: String,
    },
    /// v4 `open-documents` — `{ documents }` (oldest-first).
    #[serde(rename_all = "camelCase")]
    ChatOpenDocuments {
        chat_id: String,
    },
    /// v4 `recent-documents` — `{ documents }` (current-chat rows win the dedupe).
    #[serde(rename_all = "camelCase")]
    ChatRecentDocuments {
        chat_id: String,
    },
    /// v4 `accessible-stores` — `{ stores, projectLibrary }` (`all` = look-everywhere).
    #[serde(rename_all = "camelCase")]
    ChatAccessibleStores {
        chat_id: String,
        #[serde(default)]
        all: bool,
    },
    /// v4 `open-document` — `{ document, content, mtime, librarianMessage }`.
    #[serde(rename_all = "camelCase")]
    ChatDocumentOpen {
        chat_id: String,
        #[serde(flatten)]
        body: serde_json::Value,
    },
    /// v4 `close-document` — `{ success }`.
    #[serde(rename_all = "camelCase")]
    ChatDocumentClose {
        chat_id: String,
        #[serde(default)]
        chat_document_id: Option<String>,
    },
    /// v4 `read-document` — `{ content, mtime }`.
    #[serde(rename_all = "camelCase")]
    ChatDocumentRead {
        chat_id: String,
        #[serde(flatten)]
        body: serde_json::Value,
    },
    /// v4 `resolve-document` — `{ exists, kind }` (never bytes).
    #[serde(rename_all = "camelCase")]
    ChatDocumentResolve {
        chat_id: String,
        #[serde(flatten)]
        body: serde_json::Value,
    },
    /// v4 `write-document` — `{ success, mtime, librarianMessage }`.
    #[serde(rename_all = "camelCase")]
    ChatDocumentWrite {
        chat_id: String,
        #[serde(flatten)]
        body: serde_json::Value,
    },
    /// v4 `rename-document` — `{ document, librarianMessage }`.
    #[serde(rename_all = "camelCase")]
    ChatDocumentRename {
        chat_id: String,
        #[serde(flatten)]
        body: serde_json::Value,
    },
    /// v4 `delete-document` — `{ success, librarianMessage }`.
    #[serde(rename_all = "camelCase")]
    ChatDocumentDelete {
        chat_id: String,
        #[serde(default)]
        chat_document_id: Option<String>,
    },
    // Standalone (chat-less) Document Mode — no chat rows in the response, no
    // Librarian; scope restricted to `document_store`/`general`.
    /// v4 standalone `accessible-stores` — `{ stores, projectLibrary: null }`.
    DocumentStores,
    /// v4 standalone `recent-documents` — `{ documents }` (project-scope filtered).
    DocumentsRecent,
    /// v4 standalone `open-document` — `{ document, content, mtime, isNew }`.
    DocumentOpen {
        #[serde(flatten)]
        body: serde_json::Value,
    },
    /// v4 standalone `read-document` — `{ content, mtime }`.
    DocumentRead {
        #[serde(flatten)]
        body: serde_json::Value,
    },
    /// v4 standalone `write-document` — `{ success, mtime }`.
    DocumentWrite {
        #[serde(flatten)]
        body: serde_json::Value,
    },
    /// v4 standalone `rename-document` — `{ document }`.
    DocumentRename {
        #[serde(flatten)]
        body: serde_json::Value,
    },
    /// v4 standalone `delete-document` — `{ success }`.
    DocumentDelete {
        #[serde(flatten)]
        body: serde_json::Value,
    },
    // === end P4.6w ===
    // === P4.6z: system ===
    /// v4 `GET /api/v1/system/browse-directory[?path=…]` — the DirectoryPicker's
    /// host-filesystem browser (absent/empty `path` → the process home dir). No
    /// DB access; rides `/api/dispatch`, no new REST route.
    #[serde(rename_all = "camelCase")]
    SystemBrowseDirectory {
        #[serde(default)]
        path: Option<String>,
    },
    // === end P4.6z ===
    // === P4.6au: the home dashboard ===
    /// v4 `GET /api/v1/system/home` — the home-dashboard payload (recent chats,
    /// projects, characters, the greeting name, the "continue last" chat id).
    /// No parameters — single-user; the engine supplies the user scope.
    SystemHome,
    // === end P4.6au ===
    // === P4.6ab: courier + chat images (lane A, append-only) ===
    /// v4 `POST /chats/[id]/messages/[messageId]?action=resolve-external-turn`
    /// (`{replyContent}`) → `{resolved, messageId, participantId}`. The settle
    /// re-enters the cheap-LLM triggers when a connection profile resolves (the
    /// dispatch arm rides the courier-resolve driver seam).
    #[serde(rename_all = "camelCase")]
    MessageResolveExternalTurn {
        chat_id: String,
        message_id: String,
        reply_content: String,
    },
    /// v4 `?action=cancel-external-turn` → `{cancelled, messageId}`.
    #[serde(rename_all = "camelCase")]
    MessageCancelExternalTurn {
        chat_id: String,
        message_id: String,
    },
    /// v4 `?action=save-image` (`{fileId, mountPointId, caption?, tags?}`) →
    /// `{saved, mountPoint, relativePath, linkId, keptAt, fileId, sha256}`.
    #[serde(rename_all = "camelCase")]
    MessageSaveImage {
        chat_id: String,
        message_id: String,
        #[serde(flatten)]
        body: serde_json::Value,
    },
    /// v4 `GET /chats/[id]?action=photo-albums` → `{albums}`.
    #[serde(rename_all = "camelCase")]
    ChatPhotoAlbums {
        chat_id: String,
    },
    /// v4 `POST /chats/[id]?action=add-tool-result`
    /// (`{tool, initiatedBy, prompt?, result?, images?}`) → `{success, message}`.
    #[serde(rename_all = "camelCase")]
    ChatAddToolResult {
        chat_id: String,
        #[serde(flatten)]
        body: serde_json::Value,
    },
    /// v4 `GET /api/v1/chats/[id]/files` → `{files}`.
    #[serde(rename_all = "camelCase")]
    ChatFilesList {
        chat_id: String,
    },
    /// v4 `DELETE /api/v1/chat-files/[id]` → `{success}`.
    #[serde(rename_all = "camelCase")]
    ChatFileDelete {
        file_id: String,
    },
    /// The web-edge base64 CoreRequest for the multipart upload leg (v4
    /// `POST /api/v1/chats/[id]/files`). `resolution`/`conflicting_file_id` drive the
    /// project-file conflict resolution.
    #[serde(rename_all = "camelCase")]
    ChatFileUpload {
        chat_id: String,
        filename: String,
        content_type: String,
        /// base64 of the file bytes.
        data: String,
        #[serde(default)]
        resolution: Option<String>,
        #[serde(default)]
        conflicting_file_id: Option<String>,
    },
    // === end P4.6ab ===
    // === P4.6ad: autonomous rooms (lane C, append-only) ===
    /// v4 `GET /api/v1/system/autonomous-rooms` — every `chatType==='autonomous'`
    /// chat owned by the user, sorted running-first. `{rooms: Room[]}`.
    SystemAutonomousRooms,
    /// v4 `GET /api/v1/chats/{id}/autonomous-room` — the run-status snapshot.
    #[serde(rename_all = "camelCase")]
    ChatAutonomousRoomStatus {
        chat_id: String,
    },
    /// v4 `POST …?action=start` — manual run start → `{runId, jobId}`.
    #[serde(rename_all = "camelCase")]
    ChatAutonomousRoomStart {
        chat_id: String,
    },
    /// v4 `POST …?action=pause` → `{paused: true}`.
    #[serde(rename_all = "camelCase")]
    ChatAutonomousRoomPause {
        chat_id: String,
    },
    /// v4 `POST …?action=stop` → `{stopped: true}`.
    #[serde(rename_all = "camelCase")]
    ChatAutonomousRoomStop {
        chat_id: String,
    },
    /// v4 `POST …?action=resume` → `{runId, jobId}`.
    #[serde(rename_all = "camelCase")]
    ChatAutonomousRoomResume {
        chat_id: String,
    },
    /// v4 `POST …?action=update-settings` — the Edit Enclave modal patch (v4
    /// `updateSettingsSchema`: every cap `.nullish()` so the modal can CLEAR;
    /// ms units) → `{updated: true, clampedDestructive}`.
    #[serde(rename_all = "camelCase")]
    ChatAutonomousRoomUpdateSettings {
        chat_id: String,
        #[serde(default)]
        settings: serde_json::Value,
    },
    // === end P4.6ad ===
    // === P4.d3 === (data-retention setting — the db-size-reduction drift)
    /// v4 GET `/api/v1/settings/data-retention` — `{staleChatDays}` (default 30).
    DataRetentionSettings,
    /// v4 PUT `/api/v1/settings/data-retention` — merge `{...current, ...body}`,
    /// validate `staleChatDays` (int 1..3650), echo the parsed settings
    /// (`validationError` on failure). The raw value is carried so the handler
    /// validates the body itself (v4 `safeParse`), matching v4's rejection of
    /// out-of-range OR wrong-typed input; absent → keep current.
    DataRetentionSettingsUpdate {
        #[serde(default, rename = "staleChatDays")]
        stale_chat_days: Option<serde_json::Value>,
    },
    // === end P4.d3 ===
    // === P4.6ae === (the general files family — lane A, append-only)
    /// v4 GET `/api/v1/files` — `{files: FileEntry[]}` (`serializeFileEntry`),
    /// `createdAt` DESC, `filter='general'` → projectId null.
    FilesList {
        #[serde(default, rename = "projectId")]
        project_id: Option<String>,
        #[serde(default, rename = "folderPath")]
        folder_path: Option<String>,
        #[serde(default)]
        filter: Option<String>,
    },
    /// v4 `POST /api/v1/files/[id]?action=move` — `{data: ManagedFile}`. `projectId`
    /// is a TRI-STATE (absent keeps / null clears / value sets), decoded via
    /// [`double_option`].
    FileMove {
        #[serde(rename = "fileId")]
        file_id: String,
        #[serde(default, rename = "folderPath")]
        folder_path: Option<String>,
        #[serde(default)]
        filename: Option<String>,
        #[serde(default, rename = "projectId", deserialize_with = "double_option")]
        project_id: Option<Option<String>>,
    },
    /// v4 `POST /api/v1/files/[id]?action=promote` — `{data: ManagedFile}`.
    /// `targetProjectId ?? null` (null/absent both clear to general).
    FilePromote {
        #[serde(rename = "fileId")]
        file_id: String,
        #[serde(default, rename = "targetProjectId")]
        target_project_id: Option<String>,
        #[serde(default, rename = "folderPath")]
        folder_path: Option<String>,
    },
    /// v4 `DELETE /api/v1/files/[id]` — `{success:true}` | the
    /// `FILE_HAS_ASSOCIATIONS` 400. `dissociate` is refusal-armed this lane.
    FileDelete {
        #[serde(rename = "fileId")]
        file_id: String,
        #[serde(default)]
        force: bool,
        #[serde(default)]
        dissociate: bool,
    },
    /// v4 GET `/api/v1/files/folders` — `{folders, count}` (`path` localeCompare ASC).
    FilesFoldersList {
        #[serde(default, rename = "projectId")]
        project_id: Option<String>,
    },
    /// v4 `POST /api/v1/files/folders?action=create` — `{folder, alreadyExists,
    /// message}` (idempotent; auto-creates the parent chain).
    FilesFolderCreate {
        path: String,
        #[serde(default, rename = "projectId")]
        project_id: Option<String>,
    },
    /// v4 `POST /api/v1/files/folders?action=rename` — `{oldPath, newPath,
    /// foldersUpdated, filesUpdated}`.
    FilesFolderRename {
        path: String,
        #[serde(rename = "newName")]
        new_name: String,
        #[serde(default, rename = "projectId")]
        project_id: Option<String>,
    },
    /// v4 `POST /api/v1/files/folders?action=delete` — `{message, path}` (two
    /// DISTINCT non-empty 400s).
    FilesFolderDelete {
        path: String,
        #[serde(default, rename = "projectId")]
        project_id: Option<String>,
    },
    /// v4 `POST /api/v1/files?action=sync` — STAYS refusal-armed (the unported
    /// `lib/file-storage/reconciliation` subsystem).
    FilesSync,
    // === end P4.6ae ===
    // === P4.6ah === (files write + maintenance — lane A, append-only)
    /// v4 `POST /api/v1/files?action=upload` (`saveFileEntry`) core variant. `tags`
    /// are the resolved tagIds v4's `tags.map(t => t.tagId)` produces (BOTH the
    /// file's `linkedTo` AND its `tags` column). → `{data: FileEntry}` (201 create /
    /// 200 overwrite; the status is derived at the web edge).
    #[serde(rename_all = "camelCase")]
    FileUpload {
        filename: String,
        content_type: String,
        /// base64 of the file bytes.
        data: String,
        #[serde(default)]
        tags: Option<Vec<String>>,
        #[serde(default)]
        project_id: Option<String>,
        #[serde(default)]
        folder_path: Option<String>,
    },
    /// v4 `POST /api/v1/chats/[id]/files?action=link` — link an existing library
    /// file to the chat → `{file}`.
    #[serde(rename_all = "camelCase")]
    ChatFileLink {
        chat_id: String,
        file_id: String,
    },
    /// v4 `POST /api/v1/files?action=generate-thumbnails` → `{total, generated,
    /// cached, errors}`.
    #[serde(rename_all = "camelCase")]
    FilesGenerateThumbnails {
        file_ids: Vec<String>,
        #[serde(default)]
        size: Option<i64>,
    },
    /// v4 `POST /api/v1/files?action=cleanup-stale` (`dryRun` default true) →
    /// `{total, stale, deleted, dryRun, staleFiles, errors?}`.
    #[serde(rename_all = "camelCase")]
    FilesCleanupStale {
        #[serde(default)]
        dry_run: Option<bool>,
    },
    /// v4 `POST /api/v1/files?action=cleanup-orphans` (`mode: 'move'|'delete'`,
    /// `dryRun` default true) → v4's dry/wet shapes.
    #[serde(rename_all = "camelCase")]
    FilesCleanupOrphans {
        mode: String,
        #[serde(default)]
        dry_run: Option<bool>,
    },
    // === end P4.6ah ===
    // === P4.6ak: text-replacements + get-background (lane A, append-only) ===
    /// v4 `GET /api/v1/settings/text-replacements` → `{ rules, count }`.
    TextReplacementsList,
    /// v4 `POST /api/v1/settings/text-replacements` (create) — the input bag
    /// (`{fromText, toText, caseSensitive?, enabled?, sortOrder?}`) → `{ rule }`
    /// (201; duplicate `(fromText, caseSensitive)` → the 409 conflict arm).
    TextReplacementCreate {
        #[serde(flatten)]
        body: serde_json::Value,
    },
    /// v4 `PATCH /api/v1/settings/text-replacements/[id]` — the partial patch bag
    /// → `{ rule }` (404 when the id is unknown; 409 on a colliding pair).
    #[serde(rename_all = "camelCase")]
    TextReplacementUpdate {
        id: String,
        #[serde(flatten)]
        body: serde_json::Value,
    },
    /// v4 `DELETE /api/v1/settings/text-replacements/[id]` → 204 (404 when the id
    /// is unknown).
    #[serde(rename_all = "camelCase")]
    TextReplacementDelete {
        id: String,
    },
    /// v4 `POST /api/v1/settings/text-replacements?action=bulk-replace`
    /// (`{rules: input[]}`) → `{ rules, count }` (the transactional full-list
    /// replace).
    TextReplacementsBulkReplace {
        #[serde(flatten)]
        body: serde_json::Value,
    },
    /// v4 `GET /api/v1/chats/[id]?action=get-background` → `{ backgroundUrl,
    /// fileId, filename, sha256, linkSummary }` (all-null when the chat has no
    /// `storyBackgroundImageId` or the file row is missing).
    #[serde(rename_all = "camelCase")]
    ChatGetBackground {
        chat_id: String,
    },
    /// v4 `POST /api/v1/chats/[id]?action=regenerate-background` (the
    /// story-background GENERATION subsystem — `handlers/post.ts:77` /
    /// `handleRegenerateBackground`). A tier-3 deferral (image-profile prompt
    /// build + the 30s poll loop); the dispatch answers a loud typed
    /// `not_available` so the SPA gets a recognized refusal, not an
    /// unknown-action fallback.
    #[serde(rename_all = "camelCase")]
    ChatRegenerateBackground {
        chat_id: String,
    },
    // === end P4.6ak ===
    // === P4.6ao: the token/cost read (lane A, append-only) ===
    /// v4 `GET /api/v1/chats/[id]?action=cost[&detailed=true]` → the cost
    /// breakdown, RAW (v4 answers `NextResponse.json(breakdown)`, NOT the
    /// successResponse envelope). `detailed` absent means false — v4's route
    /// computes it as `searchParams.get('detailed') === 'true'`, so any other
    /// value is likewise false. Missing chat → `notFound('Chat')`.
    #[serde(rename_all = "camelCase")]
    ChatGetCost {
        chat_id: String,
        #[serde(default)]
        detailed: Option<bool>,
    },
    // === end P4.6ao ===
    // === P4.6ay: Pascal's custom-tools route (the composer popup's surface) ===
    /// v4 `GET /api/v1/chats/[id]/custom-tools` → `{tools, errors,
    /// droppedForCap?}` — the merged-per-perspective roster. Missing chat →
    /// `notFound('Chat')`.
    #[serde(rename_all = "camelCase")]
    ChatCustomToolsList {
        chat_id: String,
    },
    /// v4 `POST /api/v1/chats/[id]/custom-tools?action=run` → `{messages, result}`.
    /// A manual (operator) run: `invokedBy:'user'`, the whisper target the
    /// operator's `userId` (not a participant id). All four 400 arms + the
    /// post-Prospero-and-400 path live in the handler.
    #[serde(rename_all = "camelCase")]
    ChatCustomToolRun {
        chat_id: String,
        tool: String,
        #[serde(default)]
        parameters: Option<serde_json::Map<String, serde_json::Value>>,
        #[serde(default)]
        private: Option<bool>,
        #[serde(default)]
        as_character_id: Option<String>,
    },
    /// v4 `GET /api/v1/custom-tools` → the Workbench library: every definition
    /// in every enabled store, valid or broken, with attachment badges.
    CustomToolsLibrary,
    /// v4 `GET /api/v1/custom-tools?action=destinations` → the save-target list.
    CustomToolsDestinations,
    /// v4 `POST /api/v1/custom-tools?action=preview` → one deal through the
    /// shared execution core. Posts nothing, writes nothing. `metadata` is v4's
    /// `MetadataInputSchema` union: a plain object passes through verbatim, a
    /// `{ characterId }` object hydrates that character's sheet server-side.
    #[serde(rename_all = "camelCase")]
    CustomToolPreview {
        definition: serde_json::Value,
        #[serde(default)]
        params: Option<serde_json::Value>,
        #[serde(default)]
        private: Option<bool>,
        #[serde(default)]
        metadata: Option<serde_json::Value>,
        /// §B (P4.d10): mock merged state for `$state` refs and
        /// `{{state.path}}` templates. Absent/null → `{}`.
        #[serde(default)]
        state: Option<serde_json::Value>,
        /// §B: which oracle answers the bench — the real one (`{live:true}`), a
        /// scripted answer (`{output}`), or a scripted failure (`{fail:true}`).
        /// Absent leaves the seam unwired, so the consult fails soft into the
        /// author's `errorMessage` path — which is the one path every such table
        /// must survive anyway.
        #[serde(default)]
        llm: Option<serde_json::Value>,
    },
    /// v4 `POST /api/v1/custom-tools?action=audit` → `AUDIT_RUNS` deals,
    /// reported by where they landed. The run count is server-fixed and never
    /// crosses the wire.
    #[serde(rename_all = "camelCase")]
    CustomToolAudit {
        definition: serde_json::Value,
        #[serde(default)]
        params: Option<serde_json::Value>,
        #[serde(default)]
        metadata: Option<serde_json::Value>,
        /// §B (P4.d10): mock merged state for `$state` refs, held fixed across
        /// every draw. Absent/null → `{}`.
        #[serde(default)]
        state: Option<serde_json::Value>,
        /// §B: the fixed consult every draw shares. **No `live` arm** — an audit
        /// is ten thousand hands, and the point of the bench is that it never
        /// spends ten thousand LLM calls. v4 enforces that by SHAPE (the audit
        /// body's union simply lacks the arm), not by a runtime check, so the
        /// same is true here.
        #[serde(default)]
        llm: Option<serde_json::Value>,
    },
    // === end P4.6ay ===
    // === P4.6ar: the llm-logs read surface + the system aesthetics pair (lane A) ===
    /// v4 `GET /api/v1/llm-logs` (all filter branches).
    #[serde(rename_all = "camelCase")]
    LlmLogsList {
        #[serde(default)]
        message_id: Option<String>,
        #[serde(default)]
        chat_id: Option<String>,
        #[serde(default)]
        character_id: Option<String>,
        /// v4's `type` query param. Wire name `logType` — a field literally named
        /// `type` would collide with the internally-tagged envelope key. The REST
        /// edge maps `?type=` onto it. NOT validated (v4 casts).
        #[serde(default)]
        log_type: Option<String>,
        #[serde(default)]
        standalone: bool,
        #[serde(default)]
        include_messages: bool,
        /// Raw query strings; the HANDLER ports v4's parseInt/Math.min body
        /// (including the NaN no-slice quirk) so dispatch and REST agree.
        #[serde(default)]
        limit: Option<String>,
        #[serde(default)]
        offset: Option<String>,
    },
    /// v4 `GET /api/v1/llm-logs/[id]`.
    #[serde(rename_all = "camelCase")]
    LlmLogGet {
        id: String,
    },
    /// v4 `DELETE /api/v1/llm-logs/[id]`.
    #[serde(rename_all = "camelCase")]
    LlmLogDelete {
        id: String,
    },
    /// v4 `GET /api/v1/system/image-aesthetics?kind=lantern|aurora`.
    #[serde(rename_all = "camelCase")]
    SystemImageAestheticsGet {
        kind: String,
    },
    /// v4 `PUT /api/v1/system/image-aesthetics?kind=…`; empty or absent content
    /// deletes the file (restores fallback).
    #[serde(rename_all = "camelCase")]
    SystemImageAestheticsSet {
        kind: String,
        #[serde(default)]
        content: Option<String>,
    },
    // === end P4.6ar ===
    // === P4.9c: the user-profile + data-dir surface (lane C, append-only) ===
    /// v4 `GET /api/v1/user/profile` — the DEFAULT action only. v4's route is
    /// `?action=` multiplexed and also serves `theme-preference`; those arms are
    /// ALREADY ported (`theme.service` over chatSettings) and deliberately NOT
    /// re-ported here.
    UserProfileGet,
    /// v4 `PUT /api/v1/user/profile` (default action) — `{name?, email?, image?}`.
    ///
    /// The three fields are double-options because v4 validates them with Zod
    /// (`name: string.min(1).max(200).optional()`, `email: z.email().optional()`,
    /// `image: z.url().optional().or(z.literal(''))`) and `.optional()` accepts
    /// ABSENT but rejects `null`. Collapsing null into absent here would silently
    /// "fix" a real v4 behavior — v4's own profile form sends `null` for a
    /// cleared field and gets a 400 back (see `api::user_profile`).
    #[serde(rename_all = "camelCase")]
    UserProfileUpdate {
        #[serde(default, deserialize_with = "double_option")]
        name: Option<Option<serde_json::Value>>,
        #[serde(default, deserialize_with = "double_option")]
        email: Option<Option<serde_json::Value>>,
        #[serde(default, deserialize_with = "double_option")]
        image: Option<Option<serde_json::Value>>,
    },
    /// v4 `PATCH /api/v1/user/profile?action=set-avatar` — `{imageId}`, a
    /// REQUIRED but nullable key. A non-null id must resolve to a file whose
    /// `category` is `IMAGE` or `AVATAR`; the stored value is the file API
    /// pointer `/api/v1/files/{imageId}`, and `null` clears the avatar.
    #[serde(rename_all = "camelCase")]
    UserProfileSetAvatar {
        #[serde(default, deserialize_with = "double_option")]
        image_id: Option<Option<serde_json::Value>>,
    },
    /// v4 `GET /api/v1/system/data-dir` — where this instance keeps its data.
    /// The `POST ?action=open` sibling is REFUSAL-ARMED in v5 (a Tauri
    /// shell-open is a named future native nicety).
    SystemDataDir,
    // === end P4.9c ===
    // === P4.9a: the user photo gallery (lane A, append-only) ===
    /// v4 `GET /api/v1/photos` — the deduped cross-mount `photos/` roll-up.
    ///
    /// `tag` is named for v4's REPEATED query parameter (`?tag=a&tag=b`), which
    /// v4's route collects with `searchParams.getAll('tag')` and hands the
    /// service as `tags`. Keeping the wire name at the parameter spelling means
    /// the REST edge is a pure rename-free lift.
    ///
    /// `limit`/`offset` are `f64` because v4's route runs them through
    /// `Number(...)` BEFORE Zod sees them, so a non-numeric string arrives as
    /// `NaN` and a fractional string survives as a fraction — both of which
    /// v4's `z.number().int()` then rejects with its own message. An `i64` here
    /// would silently repair inputs v4 refuses.
    #[serde(rename_all = "camelCase")]
    PhotoGalleryList {
        #[serde(default)]
        q: Option<String>,
        #[serde(default)]
        tag: Option<Vec<String>>,
        #[serde(default)]
        limit: Option<f64>,
        #[serde(default)]
        offset: Option<f64>,
    },
    /// v4 `POST /api/v1/photos` — hard-link an image-v2 `FileEntry` into the
    /// Quilltap Uploads mount's `photos/` folder. A second save of the same
    /// bytes is v4's 400 (`already saved`).
    ///
    /// `caption`/`chatId` are `z.string().nullable().optional()` in v4 — both
    /// absent and explicit `null` are accepted and mean the same thing, so a
    /// plain `Option` is exact here (unlike the P4.9c profile fields).
    ///
    /// `fileId` is a DOUBLE option because v4's Zod distinguishes the two
    /// falsy shapes in its message: an absent key reports `received undefined`,
    /// an explicit `null` reports `received null`. A plain `Value` would decode
    /// both to `Value::Null` and collapse the split (pinned by
    /// `save_absent_field`).
    #[serde(rename_all = "camelCase")]
    PhotoGallerySave {
        #[serde(default, deserialize_with = "double_option")]
        file_id: Option<Option<serde_json::Value>>,
        #[serde(default)]
        caption: Option<String>,
        #[serde(default)]
        tags: Option<Vec<String>>,
        #[serde(default)]
        chat_id: Option<String>,
    },
    /// v4 `GET /api/v1/photos/{id}` — one entry; `id` is a
    /// `doc_mount_file_links.id`.
    #[serde(rename_all = "camelCase")]
    PhotoGalleryEntryGet {
        id: String,
    },
    /// v4 `DELETE /api/v1/photos/{id}` — link-only removal; the file row and
    /// blob are GC'd only when this was the last hard link to the bytes.
    #[serde(rename_all = "camelCase")]
    PhotoGalleryEntryRemove {
        id: String,
    },
    /// v4 `GET /api/v1/images/{id}` (`app/api/v1/images/[id]/route.ts:39-128`)
    /// — the image-info read the deep detail modals hang off (P4.9a2): the file
    /// row's metadata plus the DERIVED `tags[].tagType`, the
    /// `characterGalleryLinks` reverse index (which character photo albums hold
    /// a copy of these bytes), and the `_count` usage pair.
    #[serde(rename_all = "camelCase")]
    ImageInfoGet {
        id: String,
    },
    // === end P4.9a ===
    // === P4.9f1: the wardrobe server surface (lane F1, append-only) ===
    /// v4 `GET /api/v1/chats/{id}?action=outfit` — the full equipped-outfit
    /// state (`{equippedOutfit}`).
    #[serde(rename_all = "camelCase")]
    ChatOutfitGet {
        chat_id: String,
    },
    /// v4 `POST /api/v1/chats/{id}?action=equip` — all SEVEN modes (`wear`,
    /// `replace`, the deprecated `equip` alias, `add_to_slot`,
    /// `remove_from_slot`, `clear_slot`, `set_all`). The v4 route body
    /// (`{characterId, mode, slot?, itemId?, slots?}`) rides FLATTENED and raw
    /// so the core's Zod-faithful parse sees exactly what v4's did (the P4.6w
    /// documents idiom).
    #[serde(rename_all = "camelCase")]
    ChatEquip {
        chat_id: String,
        #[serde(flatten)]
        body: serde_json::Value,
    },
    /// v4 `POST /api/v1/chats/{id}?action=regenerate-avatar` — queues the
    /// avatar job; body `{characterId, imageProfileId?, equippedSlots?}`
    /// flattened raw.
    #[serde(rename_all = "camelCase")]
    ChatRegenerateAvatar {
        chat_id: String,
        #[serde(flatten)]
        body: serde_json::Value,
    },
    /// v4 `GET /api/v1/wardrobe/transfers` — the destination options.
    WardrobeTransferDestinations,
    /// v4 `POST /api/v1/wardrobe/transfers` — move/copy one item between
    /// tiers; the v4 body (`{action, itemId, sourceCharacterId,
    /// sourceProjectId?, destination}`) rides flattened raw.
    WardrobeTransferApply {
        #[serde(flatten)]
        body: serde_json::Value,
    },
    /// v4 `GET /api/v1/wardrobe` — the GLOBAL archetype tier listing.
    WardrobeList,
    /// v4 `POST /api/v1/wardrobe` — create a shared archetype (201).
    WardrobeCreate {
        item: serde_json::Value,
    },
    /// v4 `GET /api/v1/wardrobe/{itemId}` — one archetype (an ADDITION beyond
    /// the §1-pinned four: v4 ships the single-item GET on the same route
    /// file, and the REST edge would otherwise 405 where v4 answers 200).
    #[serde(rename_all = "camelCase")]
    WardrobeItemGet {
        item_id: String,
    },
    /// v4 `PUT /api/v1/wardrobe/{itemId}` — patch a shared archetype.
    #[serde(rename_all = "camelCase")]
    WardrobeUpdate {
        item_id: String,
        item: serde_json::Value,
    },
    /// v4 `DELETE /api/v1/wardrobe/{itemId}` — delete (equipped references
    /// cleaned warn-and-proceed first).
    #[serde(rename_all = "camelCase")]
    WardrobeDelete {
        item_id: String,
    },
    /// v4 `POST /api/v1/wardrobe/preview-avatar` — the SYNCHRONOUS one-off
    /// avatar render against an arbitrary slot snapshot; body `{characterId,
    /// equippedSlots, imageProfileId?}` flattened raw.
    WardrobePreviewAvatar {
        #[serde(flatten)]
        body: serde_json::Value,
    },
    /// v4 `POST /api/v1/wardrobe/analyze-image` — REFUSAL-ARMED (tier 3, §4:
    /// a vision-LLM call with no Rust port this round).
    WardrobeAnalyzeImage {
        #[serde(flatten)]
        body: serde_json::Value,
    },
    // === end P4.9f1 ===

    // === P4.9I1A: the Brahma Console dedicated dispatch family ===
    /// v4 `GET /api/v1/brahma-console` — the user's brahma chats.
    BrahmaConsoleList,
    /// v4 `POST /api/v1/brahma-console` — create a brahma chat (optional starting
    /// profile, else the user's default).
    #[serde(rename_all = "camelCase")]
    BrahmaConsoleCreate {
        #[serde(default)]
        console_connection_profile_id: Option<String>,
    },
    /// v4 `GET /api/v1/brahma-console/{id}` — chat detail.
    #[serde(rename_all = "camelCase")]
    BrahmaConsoleGet {
        chat_id: String,
    },
    /// v4 `PATCH /api/v1/brahma-console/{id}` (no action) — rename.
    #[serde(rename_all = "camelCase")]
    BrahmaConsoleRename {
        chat_id: String,
        title: String,
    },
    /// v4 `PATCH /api/v1/brahma-console/{id}?action=set-model` — switch profile.
    #[serde(rename_all = "camelCase")]
    BrahmaConsoleSetModel {
        chat_id: String,
        connection_profile_id: String,
    },
    /// v4 `DELETE /api/v1/brahma-console/{id}` — delete.
    #[serde(rename_all = "camelCase")]
    BrahmaConsoleDelete {
        chat_id: String,
    },
    /// v4 `GET /api/v1/brahma-console/{id}/messages` — the transcript.
    #[serde(rename_all = "camelCase")]
    BrahmaConsoleMessages {
        chat_id: String,
    },
    /// v4 `POST /api/v1/brahma-console/{id}/messages` — send + stream (the
    /// orchestrator; frames ride the Event channel, chat-scoped).
    #[serde(rename_all = "camelCase")]
    BrahmaConsoleSend {
        chat_id: String,
        content: String,
        #[serde(default)]
        file_ids: Vec<String>,
    },
    // === end P4.9I1A ===
    // === P4.d13: the recall-replay verb (episodic recall §3, append-only) ===
    /// v4 `POST /api/v1/chats/[id]?action=recall-replay` — replay a turn's
    /// memory recall (old path vs new path candidate tables). The three body
    /// params ride as RAW JSON values so the handler reproduces v4's
    /// silent-undefined coercion (`typeof x === 'number' && Number.isInteger`)
    /// byte-for-byte — a garbage-typed param falls back to its default rather
    /// than failing the parse.
    #[serde(rename_all = "camelCase")]
    ChatRecallReplay {
        chat_id: String,
        #[serde(default)]
        turn_index: Option<serde_json::Value>,
        #[serde(default)]
        character_id: Option<serde_json::Value>,
        #[serde(default)]
        limit: Option<serde_json::Value>,
    },
    // === end P4.d13 ===

    // === P4.9G1: the Data & System server surface (16 verbs; §1 contract) ===
    // Backup / restore (backup CREATE + the download/upload legs are web-edge
    // only — see quilltap-web routes). Restore preview/execute + backup create
    // ride dispatch.
    /// v4 `POST /api/v1/system/backup` — stage the full-graph zip, register it in
    /// the temp store, return `{success, backupId, manifest}`. `compact`
    /// (`7189a968`, the C2 contract): default false; only JSON literal `true`
    /// engages the compact projection (embeddings + the six derived embedding
    /// collections left behind, rebuilt after restoring).
    #[serde(rename_all = "camelCase")]
    SystemBackupCreate {
        #[serde(default)]
        compact: bool,
    },
    /// v4 `POST /api/v1/system/restore?action=preview` `{uploadId}`.
    #[serde(rename_all = "camelCase")]
    SystemRestorePreview {
        upload_id: String,
    },
    /// v4 `POST /api/v1/system/restore` `{uploadId, mode:'replace'|'new-account'}`.
    #[serde(rename_all = "camelCase")]
    SystemRestoreExecute {
        upload_id: String,
        mode: String,
    },
    // === P4.37: The Almanack (system report) ===
    // The web-edge action strings keep v4's frozen `capabilities-report-*`
    // names; the verbs use the feature's real one. §1 of the work order — the
    // SPA's `core-contract.ts` mirrors these name-for-name.
    /// v4 `POST /api/v1/system/tools?action=capabilities-report-generate`
    /// (`{progressId?}`) → `{success, reportId, filename, storageKey, size,
    /// content}`. Blocking; `reportId` is the FILES-ROW id (v4's 404 fix).
    /// `progressId` is optional — absent means an untracked run.
    #[serde(rename_all = "camelCase")]
    SystemAlmanackGenerate {
        #[serde(default)]
        progress_id: Option<String>,
    },
    /// v4 `GET ?action=capabilities-report-list` → `{reports: [...]}`, newest
    /// first.
    SystemAlmanackList,
    /// v4 `GET ?action=capabilities-report-get&reportId=…` →
    /// `{reportId, filename, content, size}`. The `&download=true` leg is
    /// WEB-EDGE-ONLY (same action, no separate verb).
    #[serde(rename_all = "camelCase")]
    SystemAlmanackGet {
        report_id: String,
    },
    /// v4 `POST ?action=capabilities-report-delete` (`{reportId}`) →
    /// `{success: true}`.
    #[serde(rename_all = "camelCase")]
    SystemAlmanackDelete {
        report_id: String,
    },
    // === end P4.37 ===
    /// v4 `GET /api/v1/system/tools?action=export-entities&type=`.
    #[serde(rename_all = "camelCase")]
    SystemExportEntities {
        entity_type: String,
    },
    /// v4 `GET /api/v1/system/tools?action=export-preview`.
    #[serde(rename_all = "camelCase")]
    SystemExportPreview {
        entity_type: String,
        #[serde(default)]
        scope: Option<String>,
        #[serde(default)]
        selected_ids: Vec<String>,
        #[serde(default)]
        include_memories: bool,
    },
    /// v4 `POST /api/v1/system/tools?action=import-preview` (JSON `{exportData}`;
    /// the multipart leg is web-edge only).
    #[serde(rename_all = "camelCase")]
    SystemImportPreview {
        export_data: serde_json::Value,
    },
    /// v4 `POST /api/v1/system/tools?action=import-execute` (JSON leg).
    #[serde(rename_all = "camelCase")]
    SystemImportExecute {
        export_data: serde_json::Value,
        options: serde_json::Value,
    },
    /// v4 `GET /api/v1/system/tools?action=tasks-queue`.
    SystemTasksQueue,
    /// v4 `POST /api/v1/system/tools?action=tasks-queue` `{action:'start'|'stop'}`.
    #[serde(rename_all = "camelCase")]
    SystemTasksQueueControl {
        action: String,
    },
    /// v4 `GET /api/v1/system/tools?action=job-concurrency`.
    SystemJobConcurrencyGet,
    /// v4 `POST /api/v1/system/tools?action=job-concurrency` (Zod 1..32).
    #[serde(rename_all = "camelCase")]
    SystemJobConcurrencySet {
        max_concurrent_jobs: i64,
    },
    /// v4 `GET /api/v1/system/jobs/[id]`.
    #[serde(rename_all = "camelCase")]
    SystemJobGet {
        job_id: String,
    },
    /// v4 `POST /api/v1/system/jobs/[id]?action=pause|resume`.
    #[serde(rename_all = "camelCase")]
    SystemJobControl {
        job_id: String,
        action: String,
    },
    /// v4 `DELETE /api/v1/system/jobs/[id]`.
    #[serde(rename_all = "camelCase")]
    SystemJobDelete {
        job_id: String,
    },
    /// v4 `GET /api/v1/system/tools?action=delete-data-preview`.
    SystemDeleteDataPreview,
    /// v4 `POST /api/v1/system/tools?action=delete-data` `{confirm}`.
    #[serde(rename_all = "camelCase")]
    SystemDeleteData {
        confirm: String,
    },
    // === end P4.9G1 ===
    // === P4.9E2A: the in-chat Post Office / Announcer surface (§1, frozen) ===
    /// v4 `POST /api/v1/chats/[id]?action=announcement` — post an ad-hoc
    /// announcement bubble. 201 on success.
    #[serde(rename_all = "camelCase")]
    ChatAnnouncementPost {
        chat_id: String,
        content_markdown: String,
        sender: AnnouncerSenderWire,
        /// P4.D37 (v4 `a163862c`) — the whisper audience: CHAT PARTICIPANT ids
        /// (not character ids). Omitted / null posts publicly, as it always has;
        /// every id is re-verified against the chat's current participants
        /// server-side and a dangling one is a 400.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_participant_ids: Option<Vec<String>>,
    },
    /// v4 `POST /api/v1/chats/[id]?action=announcement-preview` — generate an
    /// in-character rewrite. Persists nothing.
    #[serde(rename_all = "camelCase")]
    ChatAnnouncementPreview {
        chat_id: String,
        seed_markdown: String,
        character_id: String,
        connection_profile_id: String,
        system_prompt_id: Option<String>,
        /// P4.D37 (v4 `a163862c`) — the audience the operator has chosen for the
        /// eventual post, so the character's rewrite can be addressed to the right
        /// people (and told the remark is private). Same shape as the post
        /// action's field; the preview does NOT refuse dangling ids.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_participant_ids: Option<Vec<String>>,
    },
    /// v4 `POST /api/v1/chats/[id]?action=send-mail` — post a letter as one of the
    /// operator's player-characters. 201 on success.
    #[serde(rename_all = "camelCase")]
    ChatSendMail {
        chat_id: String,
        from_character_id: String,
        to_character_id: String,
        body_markdown: String,
        in_reply_to_path: Option<String>,
    },
    /// v4 `GET /api/v1/chats/[id]?action=mailbox&characterId=…` — list a
    /// player-character's `Mail/` letters, for the Compose Mail "In reply to"
    /// dropdown. NOTE: v4 provisions the vault on this read path.
    #[serde(rename_all = "camelCase")]
    ChatMailboxList {
        chat_id: String,
        character_id: String,
    },
    // === end P4.9E2A ===
    // === P4.9E1A: the chat cast + avatar-override verbs ===
    /// Add a character to a chat as a participant (v4 `POST …?action=add-participant`).
    /// v4's `type: z.literal('CHARACTER')` is carried by the variant name, not a field.
    #[serde(rename_all = "camelCase")]
    ChatAddParticipant {
        chat_id: String,
        character_id: String,
        #[serde(default)]
        connection_profile_id: Option<String>,
        #[serde(default)]
        image_profile_id: Option<Option<String>>,
        #[serde(default)]
        display_order: Option<i64>,
        #[serde(default)]
        has_history_access: Option<bool>,
        #[serde(default)]
        join_scenario: Option<Option<String>>,
        #[serde(default)]
        controlled_by: Option<String>,
        #[serde(default)]
        outfit_selection: Option<serde_json::Value>,
    },
    /// Patch one participant (v4 `POST …?action=update-participant`). Every
    /// `Option<Option<T>>` field is v4-`nullish`: absent ≠ explicit null.
    #[serde(rename_all = "camelCase")]
    ChatUpdateParticipant {
        chat_id: String,
        participant_id: String,
        #[serde(default)]
        connection_profile_id: Option<String>,
        #[serde(default)]
        image_profile_id: Option<Option<String>>,
        #[serde(default)]
        selected_system_prompt_id: Option<Option<String>>,
        #[serde(default)]
        display_order: Option<i64>,
        #[serde(default)]
        is_active: Option<bool>,
        #[serde(default)]
        status: Option<String>,
        #[serde(default)]
        controlled_by: Option<String>,
        #[serde(default)]
        has_history_access: Option<bool>,
        #[serde(default)]
        join_scenario: Option<Option<String>>,
        #[serde(default)]
        talkativeness: Option<Option<f64>>,
    },
    /// Soft-remove a participant (v4 `POST …?action=remove-participant`).
    #[serde(rename_all = "camelCase")]
    ChatRemoveParticipant {
        chat_id: String,
        participant_id: String,
    },
    /// Recompile a participant's identity stack (v4 `POST …?action=rebuild-system-prompt`).
    #[serde(rename_all = "camelCase")]
    ChatRebuildSystemPrompt {
        chat_id: String,
        participant_id: String,
    },
    /// Read the per-chat avatar overrides (v4 `GET …?action=get-avatars`).
    #[serde(rename_all = "camelCase")]
    ChatGetAvatars {
        chat_id: String,
    },
    /// Pin an image as a character's avatar in this chat (v4 `POST …?action=set-avatar`).
    #[serde(rename_all = "camelCase")]
    ChatSetAvatar {
        chat_id: String,
        character_id: String,
        image_id: String,
    },
    /// Clear a character's avatar override (v4 `POST …?action=remove-avatar`).
    #[serde(rename_all = "camelCase")]
    ChatRemoveAvatar {
        chat_id: String,
        character_id: String,
    },
    /// Toggle per-chat avatar generation (v4 `POST …?action=toggle-avatar-generation`).
    #[serde(rename_all = "camelCase")]
    ChatToggleAvatarGeneration {
        chat_id: String,
    },
    // === end P4.9E1A ===
    // === P4.9E3A: the chat-admin + tools verbs (§1, frozen) ===
    /// Regenerate a chat's title from its transcript (v4 `POST …?action=regenerate-title`).
    #[serde(rename_all = "camelCase")]
    ChatRegenerateTitle {
        chat_id: String,
    },
    /// Attach a tag (v4 `POST …?action=add-tag`).
    #[serde(rename_all = "camelCase")]
    ChatAddTag {
        chat_id: String,
        tag_id: String,
    },
    /// Detach a tag (v4 `POST …?action=remove-tag`).
    #[serde(rename_all = "camelCase")]
    ChatRemoveTag {
        chat_id: String,
        tag_id: String,
    },
    /// Re-attribute many messages at once (v4 `POST …?action=bulk-reattribute`).
    /// `source_participant_id` is v4-`nullable` (explicit null = unattributed);
    /// `role_filter` defaults to `"both"`.
    #[serde(rename_all = "camelCase")]
    ChatBulkReattribute {
        chat_id: String,
        source_participant_id: Option<String>,
        target_participant_id: String,
        #[serde(default)]
        role_filter: Option<String>,
    },
    /// Fold another conversation's cast + summary into this chat (v4
    /// `POST …?action=merge-conversation`). `chat_id` is the merge TARGET.
    #[serde(rename_all = "camelCase")]
    ChatMergeConversation {
        chat_id: String,
        source_chat_id: String,
        #[serde(default)]
        character_ids: Option<Vec<String>>,
        #[serde(default)]
        outfit_selections: Option<Vec<serde_json::Value>>,
    },
    /// Replace the chat's disabled-tool sets (v4 `POST …?action=update-tool-settings`).
    /// The service ALSO sets `forceToolsOnNextMessage = true`; the response echoes
    /// only the two arrays.
    #[serde(rename_all = "camelCase")]
    ChatUpdateToolSettings {
        chat_id: String,
        disabled_tools: Vec<String>,
        disabled_tool_groups: Vec<String>,
    },
    /// Operator-initiated tool execution (v4 `POST …?action=run-tool`).
    #[serde(rename_all = "camelCase")]
    ChatRunTool {
        chat_id: String,
        tool_name: String,
        #[serde(default)]
        arguments: Option<serde_json::Value>,
        #[serde(default)]
        character_id: Option<String>,
        #[serde(default)]
        private: Option<bool>,
    },
    /// Manual RNG from the composer gutter (v4 `POST …?action=rng`). `kind` is
    /// either a die size (integer 2..=1000) or `"flip_coin"` / `"spin_the_bottle"`;
    /// `rolls` defaults to 1 (1..=100); `preview` returns without writing a message.
    #[serde(rename_all = "camelCase")]
    ChatRng {
        chat_id: String,
        kind: serde_json::Value,
        #[serde(default)]
        rolls: Option<u32>,
        #[serde(default)]
        preview: Option<bool>,
    },
    /// Toggle agent mode for this chat (v4 `POST …?action=toggle-agent-mode`).
    #[serde(rename_all = "camelCase")]
    ChatToggleAgentMode {
        chat_id: String,
        /// v4's `toggleAgentModeSchema` body is `{ enabled: boolean | null }`
        /// and the column is tri-state: absent leaves it alone, `null` clears
        /// the per-chat override back to the cascade, `true`/`false` pins it.
        /// (Widened at this round's unification — P4.9E3A shipped the SERVICE
        /// taking the full tri-state but §1 was frozen across three orders, so
        /// the variant could only express the absent arm.)
        #[serde(default)]
        enabled: Option<Option<bool>>,
    },
    /// Reset and re-queue danger classification (v4 `POST …?action=reclassify-danger`).
    #[serde(rename_all = "camelCase")]
    ChatReclassifyDanger {
        chat_id: String,
    },
    /// Queue a Scriptorium render with full re-embed (v4 `POST …?action=render-conversation`).
    #[serde(rename_all = "camelCase")]
    ChatRenderConversation {
        chat_id: String,
    },
    // === end P4.9E3A ===
    // === P4.9E3B: the chat-dialog server remainder (§1, frozen) ===
    /// The tool inventory (v4 `GET /api/v1/tools`, app/api/v1/tools/route.ts:429).
    #[serde(rename_all = "camelCase")]
    ToolsList {
        #[serde(default)]
        chat_id: Option<String>,
        #[serde(default)]
        include_schemas: Option<bool>,
    },
    /// Chat transcript export as SillyTavern JSONL (v4 `GET …/chats/{id}?action=export`).
    #[serde(rename_all = "camelCase")]
    ChatExport {
        chat_id: String,
    },
    /// The readable Markdown transcript (v4 `GET …/chats/{id}?action=export-markdown`,
    /// P4.d28 / v4 `b3ee00f1`).
    #[serde(rename_all = "camelCase")]
    ChatExportMarkdown {
        chat_id: String,
    },
    /// Per-character equipped-outfit summary (v4 `GET …/chats/{id}?action=outfit-summary`).
    #[serde(rename_all = "camelCase")]
    ChatOutfitSummary {
        chat_id: String,
    },
    /// Search & replace dry-run (v4 `POST /api/v1/search-replace?action=preview`).
    /// `scope` is v4's discriminated union: {"type":"chat","chatId":…} |
    /// {"type":"character","characterId":…}. The two include flags default TRUE
    /// (v4 prefaults) — absent means true, not false.
    #[serde(rename_all = "camelCase")]
    SearchReplacePreview {
        scope: serde_json::Value,
        search_text: String,
        replace_text: String,
        #[serde(default)]
        include_messages: Option<bool>,
        #[serde(default)]
        include_memories: Option<bool>,
    },
    /// Search & replace execute (v4 `POST /api/v1/search-replace?action=execute`).
    #[serde(rename_all = "camelCase")]
    SearchReplaceExecute {
        scope: serde_json::Value,
        search_text: String,
        replace_text: String,
        #[serde(default)]
        include_messages: Option<bool>,
        #[serde(default)]
        include_memories: Option<bool>,
    },
    /// Re-attribute ONE message (v4 `POST /api/v1/messages/{id}?action=reattribute`).
    /// Also deletes every memory sourced from the message (v4 route.ts:342+).
    #[serde(rename_all = "camelCase")]
    MessageReattribute {
        message_id: String,
        new_participant_id: String,
    },
    /// Group document stores reachable from this chat's user-controlled cast
    /// (v4 `GET …/chats/{id}?action=group-stores`, actions/group-stores.ts:16).
    /// NOT part of the §1 freeze — added by the order's tier-2 audit (item 9),
    /// which found the picker's read missing; recorded for the unifier + E3C.
    #[serde(rename_all = "camelCase")]
    ChatGroupStores {
        chat_id: String,
    },
    // === end P4.9E3B ===
    // === P4.9E4A: the library-picker server remainder (append-only) ===
    /// v4 `POST /api/v1/chats/{id}/files?action=attach-mount-file`
    /// (`handleAttachMountFile`) — attach a document-store file to the chat by
    /// posting the Librarian's attachment announcement (which IS the attachment
    /// record). → `{file: {…, type: "mountFile"}, announcement: {id, createdAt}}`.
    ///
    /// Both string fields are `#[serde(default)]` because v4's validation is
    /// hand-rolled, not Zod: a missing OR non-string field is falsy and answers
    /// `badRequest`, so the web edge coerces either to `""` and the handler's
    /// empty check covers both arms byte-for-byte.
    #[serde(rename_all = "camelCase")]
    ChatAttachMountFile {
        chat_id: String,
        #[serde(default)]
        mount_point_id: String,
        #[serde(default)]
        relative_path: String,
    },
    // === end P4.9E4A ===
    // === P4.9P: the global-search endpoint (append-only) ===
    /// v4 `GET /api/v1/ui/search?q=…&types=…&limit=…&offset=…` — the global
    /// SearchBar/SearchDialog endpoint. All four params ride as RAW strings:
    /// the HANDLER ports v4's trim/parseInt/Math.min-max body (including the
    /// NaN empty-page quirk) so dispatch and REST agree byte-for-byte.
    #[serde(rename_all = "camelCase")]
    UiSearch {
        #[serde(default)]
        q: Option<String>,
        /// CSV of `chats|characters|tags|memories|messages`; unknown entries are
        /// filtered out and an all-unknown list falls back to ALL types (v4's
        /// `parsedTypes.length > 0` gate).
        #[serde(default)]
        types: Option<String>,
        #[serde(default)]
        limit: Option<String>,
        #[serde(default)]
        offset: Option<String>,
    },
    // === end P4.9P ===
    // === P4.D50: the Taboo list (v4 `7df7de8e`) — append-only ===
    /// v4 `GET /api/v1/settings/taboo` — the instance-wide forbidden-phrase
    /// list, `{phrases: string[]}` (empty when never written).
    TabooSettings,
    /// v4 `PUT /api/v1/settings/taboo` — merge `{...current, ...body}`,
    /// `TabooSettingsSchema.safeParse` the merge, persist the NORMALIZED list,
    /// and echo what was stored.
    ///
    /// The raw value rides through so the handler runs the Zod-faithful parse
    /// itself (the `DataRetentionSettingsUpdate` precedent): a partial body —
    /// `{}` in particular — must leave the stored list untouched, and a
    /// present-but-invalid `phrases` must 400 rather than being coerced.
    ///
    /// `Option<Option<_>>` is deliberate: `None` is the key ABSENT (the merge
    /// keeps the stored list), `Some(None)` is an explicit `null`, which is NOT
    /// `undefined` — Zod's `.default([])` does not fire for it, so v4 answers
    /// `validationError`. A plain `Option` would collapse the two.
    TabooSettingsUpdate {
        #[serde(default, deserialize_with = "double_option")]
        phrases: Option<Option<serde_json::Value>>,
    },
    // === end P4.D50 ===
}

// === P4.9E2A: the announcer sender union (§1, frozen) ===

/// v4's `insertAnnouncementSchema.sender` discriminated union (`schemas.ts:193`),
/// tagged on `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// (The per-variant `rename_all` is what puts `staffId` / `characterId` /
/// `displayName` on the wire: serde's enum-level `rename_all` renames VARIANTS
/// only, so the §1 casing needs it stated per arm.)
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AnnouncerSenderWire {
    #[serde(rename_all = "camelCase")]
    Staff { staff_id: StaffSenderWire },
    #[serde(rename_all = "camelCase")]
    Character { character_id: String },
    #[serde(rename_all = "camelCase")]
    Custom { display_name: String },
}

/// v4 `STAFF_SENDER_ENUM` (`schemas.ts:180`) — exactly these ten, this spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StaffSenderWire {
    Lantern,
    Aurora,
    Librarian,
    Concierge,
    Prospero,
    Host,
    CommonplaceBook,
    Ariel,
    Suparna,
    Pascal,
}

// === end P4.9E2A ===

/// serde double-option: `#[serde(default, deserialize_with = "double_option")]` on
/// an `Option<Option<T>>` field decodes an ABSENT field to `None`, an explicit
/// `null` to `Some(None)`, and a value to `Some(Some(v))` — the null-vs-absent
/// distinction serde's default `Option` collapses. Used for the `fileMove`
/// projectId tri-state.
fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Deserialize::deserialize(de).map(Some)
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
    /// v4 `GET .../custom-tools` body (`{tools, errors, droppedForCap?}`).
    CustomToolsList(serde_json::Value),
    /// v4 `POST .../custom-tools?action=run` body (`{messages, result}`).
    CustomToolRun(serde_json::Value),
    /// v4 `GET /api/v1/custom-tools` body (`{tools, errors}` — the Workbench
    /// library).
    CustomToolsLibrary(serde_json::Value),
    /// v4 `GET /api/v1/custom-tools?action=destinations` body.
    CustomToolsDestinations(serde_json::Value),
    /// v4 `POST /api/v1/custom-tools?action=preview` body (a `CustomToolRunResult`).
    CustomToolPreview(serde_json::Value),
    /// v4 `POST /api/v1/custom-tools?action=audit` body (a `CustomToolAuditResult`).
    CustomToolAudit(serde_json::Value),
    /// v4 message edit / swipe body (`{ message: {...} }`).
    Message(serde_json::Value),
    /// v4 brahma-console CRUD bodies (`{ chats }` / `{ chat }` / `{ messages }` /
    /// `{ message }`) — the P4.9I1A dedicated dispatch family. Send's reply is the
    /// separate `BrahmaConsoleSend` variant.
    BrahmaConsole(serde_json::Value),
    /// v4 brahma-console send reply (`{ messageId }`) — the orchestrator's typed
    /// dispatch result; the stream frames ride the Event channel (the `ChatSend`
    /// architecture).
    BrahmaConsoleSend(serde_json::Value),
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
    /// A state-tier body (P4.d10 §A, v4 `f48f34dc`): the chat merged-cascade
    /// GET (`{success, state, chatState, projectState?, groupState?,
    /// generalState?, groupTier, projectId?}`), the group/general own-state
    /// GETs (`{success, state}`), the sets (`{success, state}`), and the
    /// resets (`{success, previousState}`). The exact bytes are pinned by
    /// `state_routes_equivalence`.
    State(serde_json::Value),
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
    /// P4.d13: the recall-replay result (v4 `successResponse(result)` — the
    /// full old/new candidate-table object, passed through verbatim).
    RecallReplay(serde_json::Value),
    /// A mount-files-family body (P4.6v): the list `{files, folders}`, the read
    /// envelope (`readMountFile` result), and the file-op mutation results. Pinned
    /// by `mount_read_equivalence` (+ later mount-ops differentials).
    MountFile(serde_json::Value),
    // === P4.6w: documents ===
    /// A Document Mode body (chat-scoped + standalone): the active/open/recent
    /// document lists, `{stores, projectLibrary}`, the open
    /// `{document, content, mtime, librarianMessage}`, the read `{content, mtime}`,
    /// the resolve `{exists, kind}`, the write/rename/delete envelopes. The exact
    /// bytes are pinned by `documents_routes_equivalence` (P4.6w).
    Document(serde_json::Value),
    // === end P4.6w ===
    // === P4.6z: system ===
    /// The `systemBrowseDirectory` success body (`{path, parent, directories,
    /// error?}`). Pinned by `browse_directory_equivalence`.
    System(serde_json::Value),
    // === end P4.6z ===
    // === P4.6au: the home dashboard ===
    /// The `systemHome` success body — v4's `HomeData` (`{displayName,
    /// lastChatId, recentChats, projects, characters}`; the REST edge emits it
    /// RAW, v4's `successResponse(data)`). The exact bytes, key order included,
    /// are pinned by `home_routes_equivalence` (P4.6au).
    SystemHome(serde_json::Value),
    // === end P4.6au ===
    // === P4.9c: the user-profile + data-dir surface (lane C, append-only) ===
    /// A user-profile body — the `{profile: {…}}` envelope v4's GET / PUT /
    /// PATCH all answer with (`route.ts:92`, `:213`, `:292`). The REST edge
    /// emits it RAW. Pinned by `profile_routes_equivalence`.
    UserProfile(serde_json::Value),
    /// The `GET /api/v1/system/data-dir` payload (v4's `DataDirInfo`, emitted
    /// RAW by `successResponse(response)`). Pinned by
    /// `profile_routes_equivalence`'s data-dir case over `services::data_dir`.
    SystemDataDir(serde_json::Value),
    // === end P4.9c ===
    // === P4.9a: the user photo gallery (lane A, append-only) ===
    /// A user-photo-gallery body: the list `{entries, total, hasMore}`, one
    /// entry object, the save receipt `{linkId, mountPointId, …}`, the
    /// delete `{deleted, fileGC}`, or the image-info `{data: {…}}` envelope
    /// (P4.9a2 `ImageInfoGet`). Every one is emitted RAW by v4's
    /// `successResponse`/`created`, so the REST edge unwraps this variant and
    /// writes the value straight out. Pinned by `photos_routes_equivalence`.
    PhotoGallery(serde_json::Value),
    // === end P4.9a ===
    // === P4.6ab: courier + chat images ===
    /// A courier/chat-images body — the resolve/cancel envelopes, the save-image
    /// `{saved, …}`, `{albums}`, the add-tool-result `{success, message}`, the
    /// chat-files list `{files}` / delete `{success}` / upload
    /// `{file}` | `{duplicate, …}`. Pinned by `courier_images_routes_equivalence`.
    ChatMedia(serde_json::Value),
    // === end P4.6ab ===
    // === P4.6ad: autonomous rooms ===
    /// An autonomous-rooms-family body: the listing `{rooms}`, the status
    /// snapshot, the run-control envelopes (`{runId, jobId}`, `{paused: true}`,
    /// `{stopped: true}`, `{updated: true, clampedDestructive}`). The exact bytes
    /// are pinned by `autonomous_rooms_routes_equivalence` (P4.6ad).
    AutonomousRoom(serde_json::Value),
    // === end P4.6ad ===
    // === P4.d3 === (data-retention setting)
    /// The data-retention settings body (`{staleChatDays}`) — v4's
    /// `successResponse(settings)`. Pinned by `settings_routes_equivalence`.
    DataRetention(serde_json::Value),
    // === end P4.d3 ===
    // === P4.6ae === (the general files family — lane A, append-only)
    /// A general files-family body: the listing `{files}`, move/promote `{data}`,
    /// delete `{success}`, the folders `{folders,count}` / create / rename / delete
    /// envelopes, and the upload `{data}`. Pinned by `files_routes_equivalence`.
    Files(serde_json::Value),
    // === end P4.6ae ===
    // === P4.6ak === (text-replacements + get-background — lane A, append-only)
    /// A text-replacement-rules body: the list/bulk `{rules, count}`, the item
    /// `{rule}`, and `null` for the delete's 204 (the REST edge maps null → an
    /// empty 204). Pinned by `text_replacements_routes_equivalence`.
    TextReplacement(serde_json::Value),
    /// The `chatGetBackground` body (`{backgroundUrl, fileId, filename, sha256,
    /// linkSummary}` — all-null when unset/missing). Pinned by the same
    /// differential.
    ChatBackground(serde_json::Value),
    // === end P4.6ak ===
    // === P4.6ao === (the token/cost read — lane A, append-only)
    /// The `chatGetCost` body — v4's breakdown object VERBATIM: the five keys
    /// `{totalTokens, promptTokens, completionTokens, estimatedCostUSD,
    /// priceSource}`, plus `{messageBreakdown, systemEventBreakdown}` on the
    /// detailed arm. Pinned by `cost_background_routes_equivalence`.
    ChatCost(serde_json::Value),
    // === end P4.6ao ===
    // === P4.6ar === (the llm-logs read surface + system aesthetics — lane A)
    /// An llm-logs-family body: the list `{logs, count, total, limit, offset}`
    /// (`limit`/`offset` are JS numbers, so a garbage param serializes as `null`),
    /// the item GET's RAW log object (v4 `successResponse(log)` — no wrapper key),
    /// and the delete's `{success, deletedId}`. Pinned by
    /// `llm_logs_routes_equivalence`.
    LlmLog(serde_json::Value),
    /// The `uiSearch` success body — v4's `SearchResponse` (`{results,
    /// totalCount, query, types, hasMore, countsByType}`, `successResponse`'s
    /// RAW payload). Pinned by `ui_search_equivalence` (P4.9P).
    UiSearch(serde_json::Value),
    /// The system default-aesthetic body, byte-identical to the project pair:
    /// get → `{content}`; set → `{success: true}`. Pinned by
    /// `image_aesthetics_routes_equivalence`.
    SystemAesthetic(serde_json::Value),
    // === end P4.6ar ===
    // === P4.9f1: the wardrobe server surface (lane F1, append-only) ===
    /// A wardrobe-family body — the archetype tier (`{wardrobeItems}`,
    /// `{wardrobeItem}`, `{success}`), the transfers pair (`{destinations}`,
    /// `{wardrobeItem, action}`), and the avatar preview (`{fileId, url,
    /// mimeType, prompt}`). The exact bytes are pinned by
    /// `wardrobe_routes_equivalence`.
    Wardrobe(serde_json::Value),
    /// A chat-outfit body — `{equippedOutfit}`, `{equippedSlots}`, and the
    /// regenerate-avatar `{message, queued}`. Pinned by
    /// `wardrobe_routes_equivalence`.
    ChatOutfit(serde_json::Value),
    // === end P4.9f1 ===
    // === P4.9E2A: the in-chat Post Office / Announcer surface (§1, frozen) ===
    /// A Post Office / Announcer body — `{success, message}` (announcement
    /// post), `{success, proposedMarkdown}` (preview), `{success, path}`
    /// (send-mail), `{letters: [{path, from, sentAt}]}` (mailbox). Pinned by
    /// `post_office_routes_equivalence`.
    ///
    /// v4 answers **201** on the two `created`-style arms; the dispatch
    /// boundary carries no per-verb success status (every success is 200 at the
    /// HTTP edge — the standing `ChatCreate` precedent), so the BODY is what
    /// §1 freezes.
    ChatPostOffice(serde_json::Value),
    // === end P4.9E2A ===
    // === P4.9E1A: the chat cast + avatar-override verbs ===
    /// The raw body of every P4.9E1A verb (v4's action handlers each return a
    /// differently-shaped literal — `{participant, chat}`, `{success, chat}`,
    /// `{ok, chat}`, `{data}`, `{avatarGenerationEnabled}` — so the boundary
    /// carries them as-is rather than inventing a union).
    ChatCast(serde_json::Value),
    // === end P4.9E1A ===
    // === P4.9E3A: the chat-admin + tools verbs ===
    /// A chat-admin / tools body — the tag pair's `{success, tag}` / `{success}`,
    /// bulk re-attribution's `{success, messagesUpdated, memoriesDeleted}`, the
    /// merge's `{success, merge, chat}`, tool settings' `{success, data}`,
    /// run-tool's and rng's `{success, message, result}` (and rng's preview
    /// `{success, preview, result}`), agent mode's and the two enqueueing verbs'
    /// `{success, data}`, and regenerate-title's `{success, title}`. The exact
    /// bytes are pinned by `chat_admin_routes_equivalence`.
    ///
    /// v4 answers **201** on `add-tag`; the dispatch boundary carries no per-verb
    /// success status (every success is 200 at the HTTP edge — the standing
    /// `ChatCreate` precedent), so the BODY is what §1 freezes.
    ChatAdmin(serde_json::Value),
    // === end P4.9E3A ===
    // === P4.9E3B: the chat-dialog server remainder ===
    /// The tool inventory body — v4 `successResponse({ tools, count })`.
    ToolsInventory(serde_json::Value),
    /// The chat export payload: the JSONL bytes plus the download filename.
    /// The `Content-Type: application/x-ndjson` + `Content-Disposition` headers
    /// belong at the quilltap-web edge (the `characters_routes.rs:373` precedent).
    #[serde(rename_all = "camelCase")]
    ChatExportPayload {
        filename: String,
        jsonl: String,
    },
    /// The Markdown transcript payload: the document bytes plus the download
    /// filename. `Content-Type: text/markdown; charset=utf-8`, the RFC 5987
    /// `Content-Disposition`, and v4's `Cache-Control: no-store` are the web
    /// edge's (P4.d28).
    #[serde(rename_all = "camelCase")]
    ChatMarkdownTranscriptPayload {
        filename: String,
        markdown: String,
    },
    /// A chat-dialog verb body (outfit-summary's `{summary}`, search-replace's
    /// raw un-enveloped preview/result, reattribute's
    /// `{success, message, memoriesDeleted}`) — each of v4's handlers returns a
    /// differently-shaped literal, so the boundary carries them as-is (the
    /// `ChatAdmin` precedent).
    ChatDialog(serde_json::Value),
    // === end P4.9E3B ===
    // === P4.D50: the Taboo list (v4 `7df7de8e`) — append-only ===
    /// The Taboo settings body (`{phrases}`) — v4's `successResponse(settings)`.
    /// The PUT arm echoes the NORMALIZED list the setter stored, never the raw
    /// submission, so the client cache can never disagree with the database.
    /// Pinned by `settings_routes_equivalence`.
    Taboo(serde_json::Value),
    // === end P4.D50 ===
    Error(CoreError),
}

impl Response {
    /// Shorthand for an error response.
    pub fn error(kind: ErrorKind, message: impl Into<String>) -> Response {
        Response::Error(CoreError {
            kind,
            message: message.into(),
            pepper_state: None,
            code: None,
            associations: None,
            entity: None,
        })
    }

    /// v4's store-unavailable refusal (P4.23): [`ErrorKind::Unavailable`] with
    /// the entity carry, so the transport can answer the deliberate contextful
    /// 503 body. `message` is v4's fixed error string for the entity (the
    /// dispatch envelope's own `message` mirrors the wire body's `error`).
    pub fn store_unavailable(label: &str, id: &str) -> Response {
        Response::Error(CoreError {
            kind: ErrorKind::Unavailable,
            message: unavailable_error_string(label),
            pepper_state: None,
            code: None,
            associations: None,
            entity: Some(Box::new(UnavailableEntity {
                label: label.to_string(),
                id: id.to_string(),
            })),
        })
    }

    /// An error response carrying v4's file-op `code` (P4.6y — the
    /// `{ error, code }` body; see [`CoreError::code`]).
    pub fn error_coded(
        kind: ErrorKind,
        message: impl Into<String>,
        code: impl Into<String>,
    ) -> Response {
        Response::Error(CoreError {
            kind,
            message: message.into(),
            pepper_state: None,
            code: Some(code.into()),
            associations: None,
            entity: None,
        })
    }

    /// v4's `FILE_HAS_ASSOCIATIONS` refusal (P4.6ah `fileDelete`): a `{error,
    /// code, associations}` body carrying the itemized `getFileAssociations`
    /// result. Only the un-forced linked-file delete emits it.
    pub fn error_with_associations(
        kind: ErrorKind,
        message: impl Into<String>,
        code: impl Into<String>,
        associations: FileAssociations,
    ) -> Response {
        Response::Error(CoreError {
            kind,
            message: message.into(),
            pepper_state: None,
            code: Some(code.into()),
            associations: Some(associations),
            entity: None,
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
            code: None,
            associations: None,
            entity: None,
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
    /// v4's `{ error, code }` file-op body (P4.6y): the `FileOpError` ∪
    /// `DatabaseStoreError` code union (`SOURCE_NOT_FOUND | DEST_EXISTS |
    /// MOUNT_NOT_FOUND | INVALID_PATH | UNSUPPORTED | VERIFY_FAILED | CONFLICT |
    /// NOT_FOUND | NOT_EMPTY | INVALID`) plus `EMBEDDING_FAILED` on the
    /// semantic-search embed refusal. The SPA error translation keys on this
    /// first, HTTP status second. Absent everywhere else.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// v4's `FILE_HAS_ASSOCIATIONS` itemized payload (P4.6ah `fileDelete`):
    /// the `getFileAssociations` result — present ONLY on the un-forced
    /// linked-file delete refusal, absent everywhere else.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub associations: Option<FileAssociations>,
    /// The store-unavailable entity carry (P4.23): present ONLY on
    /// [`ErrorKind::Unavailable`], absent everywhere else. Lets the transport
    /// answer v4's deliberate contextful 503 body
    /// (`{"error": "<…> unavailable", "<entity>Id": <id>}`) without parsing
    /// the message — see [`CoreError::unavailable_wire_body`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<Box<UnavailableEntity>>,
}

/// The broken store's entity, carried on [`ErrorKind::Unavailable`] errors.
/// `label` is the lowercase singular entity label (`"project"` / `"group"` /
/// `"character"`); `id` the entity id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnavailableEntity {
    pub label: String,
    pub id: String,
}

/// Map a [`crate::db::DbError`] surfacing at an api terminal arm to its
/// `Response` (P4.23): the structured store-unavailable refusal answers v4's
/// deliberate contextful 503; everything else stays the plain internal error
/// (`internal(e)`'s exact behavior).
pub(crate) fn db_error_response(e: crate::db::DbError) -> Response {
    match e {
        crate::db::DbError::StoreUnavailable {
            entity_label, id, ..
        } => Response::store_unavailable(entity_label, &id),
        other => Response::error(ErrorKind::Internal, other.to_string()),
    }
}

/// v4's fixed error string for a broken store, per entity
/// (`lib/api/middleware/context.ts:176-205`): the character vault has its own
/// wording; the store-backed entities share the document-store one.
pub fn unavailable_error_string(label: &str) -> String {
    match label {
        "character" => "Character vault unavailable".to_string(),
        _ => {
            let (head, tail) = label.split_at(1);
            format!("{}{} document store unavailable", head.to_uppercase(), tail)
        }
    }
}

impl CoreError {
    /// v4's exact store-unavailable 503 body — `{"error": <fixed string>,
    /// "<label>Id": <id>}`, in that key order (`preserve_order` makes insertion
    /// order the wire order). `None` unless this error carries the entity.
    pub fn unavailable_wire_body(&self) -> Option<serde_json::Value> {
        let entity = self.entity.as_ref()?;
        let mut body = serde_json::Map::new();
        body.insert(
            "error".to_string(),
            serde_json::Value::String(unavailable_error_string(&entity.label)),
        );
        body.insert(
            format!("{}Id", entity.label),
            serde_json::Value::String(entity.id.clone()),
        );
        Some(serde_json::Value::Object(body))
    }
}

/// v4 `getFileAssociations` result (`lib/files/get-file-associations.ts`) — the
/// itemized `FILE_HAS_ASSOCIATIONS` payload the delete refusal carries and the
/// SPA delete dialog consumes (P4.6ah). Serializes as `{characters, messages}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileAssociations {
    pub characters: Vec<FileAssocCharacter>,
    pub messages: Vec<FileAssocMessage>,
}

/// A character that uses the file as its default or an override avatar image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileAssocCharacter {
    pub id: String,
    pub name: String,
    /// `'default'` | `'override'` (default wins the dedup when a character uses
    /// the file both ways).
    pub usage: String,
}

/// A message whose attachments reference the file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileAssocMessage {
    pub chat_id: String,
    pub chat_name: String,
    pub message_id: String,
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
    /// An explicit `errorResponse(msg, 422)`: the request was well-formed and
    /// the definition valid, but the table would not deal — Pascal's Workbench
    /// preview/audit refusing a `CustomToolRunError`, or a character's vault
    /// being unreadable when the bench asked for their fact sheet. Distinct from
    /// `BadRequest` on purpose: the author's INPUT was fine, so the SPA renders
    /// the reason rather than pointing at the form.
    Unprocessable,
    Locked,
    /// A store-backed entity's document store / character vault is broken
    /// (P4.23): v4's middleware answers a deliberate, contextful **503** —
    /// `{"error": "<…> unavailable", "<entity>Id": <id>}` — "so callers and
    /// logs can tell store degradation apart from a generic crash"
    /// (`context.ts:176-205`). A SEPARATE kind from [`ErrorKind::Locked`]
    /// (also 503) on purpose: "vault locked" must stay distinguishable from
    /// "store broken".
    Unavailable,
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

#[cfg(test)]
mod tests {
    use super::Request;

    /// P4.D50 §3-review pin: the `TabooSettingsUpdate.phrases` tri-state must
    /// survive SERDE deserialization — the dispatch JSON leg, which the route
    /// differential cannot see (the web edge hand-constructs the variant).
    /// Without `deserialize_with = "double_option"`, `{"phrases": null}`
    /// silently collapsed to key-absent (keeping the stored list) where v4's
    /// PUT answers `validationError` — Zod's `.default([])` fires only for
    /// `undefined`, never for `null`.
    #[test]
    fn taboo_update_phrases_tristate_survives_serde() {
        let absent: Request = serde_json::from_str(r#"{"type":"tabooSettingsUpdate"}"#).unwrap();
        assert!(matches!(
            absent,
            Request::TabooSettingsUpdate { phrases: None }
        ));

        let null: Request =
            serde_json::from_str(r#"{"type":"tabooSettingsUpdate","phrases":null}"#).unwrap();
        assert!(matches!(
            null,
            Request::TabooSettingsUpdate {
                phrases: Some(None)
            }
        ));

        let value: Request =
            serde_json::from_str(r#"{"type":"tabooSettingsUpdate","phrases":["x"]}"#).unwrap();
        match value {
            Request::TabooSettingsUpdate {
                phrases: Some(Some(v)),
            } => assert_eq!(v, serde_json::json!(["x"])),
            other => panic!("expected Some(Some(_)), got {other:?}"),
        }
    }
}
