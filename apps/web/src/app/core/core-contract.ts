/**
 * The Core API wire contract — hand-written TS discriminated unions mirroring
 * the Rust source of truth (`crates/quilltap-core/src/api/types.rs` +
 * `crates/quilltap-core/src/services/chat_events.rs`).
 *
 * Kept in ONE module so the unification pass can diff it against the Rust enums
 * at a glance. No codegen this round — the surface is small and stable.
 *
 * Wire tagging (must match the Rust serde attributes exactly):
 *  - `Request` is INTERNALLY tagged, camelCase: `{ "type": "unlock", "passphrase": "…" }`.
 *  - `Response` is ADJACENTLY tagged: `{ "type": "chats", "data": [...] }`.
 *  - The error envelope is `{ "type": "error", "data": { kind, message, pepperState? } }`.
 *  - `Event` flattens its payload beside the scope tags: a chat frame is
 *    `{ "chatId": "…", "content": "…" }` (the frame is a bare single-key object).
 */

// ===========================================================================
// Readiness (v4 DbKeyState — Rust `PepperState`)
// ===========================================================================

/**
 * v4's `DbKeyState`, verbatim wire strings. `resolved` and `needs-vault-storage`
 * are both operational (the pepper is in hand); `needs-setup` / `needs-passphrase`
 * are not. `loading` is a CLIENT-only sentinel while the first `health` is in
 * flight — never sent by the server.
 */
export type PepperState = 'resolved' | 'needs-setup' | 'needs-passphrase' | 'needs-vault-storage';

/** Client-side readiness including the pre-first-response `loading` sentinel. */
export type ReadinessState = PepperState | 'loading';

/** v4 `isPepperResolved`: whether the engine can serve (pepper available). */
export function isOperational(state: PepperState): boolean {
  return state === 'resolved' || state === 'needs-vault-storage';
}

// ===========================================================================
// Request
// ===========================================================================

/** Send-a-message options — projects the Rust `Request::ChatSend` fields. */
export interface ChatSendRequest {
  type: 'chatSend';
  chatId: string;
  content?: string;
  continueMode?: boolean;
  respondingParticipantId?: string | null;
  targetParticipantIds?: string[] | null;
  speakingAsParticipantId?: string | null;
  fileIds?: string[];
  /** Client-computed skip-eligibility summon (v4 `sendMessageSchema.nudge`). */
  nudge?: boolean;
  /** Pre-computed tool results to thread (v4 `sendMessageSchema.pendingToolResults`). */
  pendingToolResults?: unknown[];
}

/** List chats with the v4 `handleList` query knobs (all optional). */
export interface ListChatsRequest {
  type: 'listChats';
  excludeTagIds?: string[];
  limit?: number;
  includeAutonomous?: boolean;
}

/** Fetch one enriched conversation (v4 `handleGet`). */
export interface ChatGetRequest {
  type: 'chatGet';
  chatId: string;
}

/** A partial chat update (v4 PUT `/chats/:id`). */
export interface ChatUpdateRequest {
  type: 'chatUpdate';
  chatId: string;
  chat: Record<string, unknown>;
}

/** Turn-management actions (v4 `handleTurnAction`). */
export interface ChatTurnActionRequest {
  type: 'chatTurnAction';
  chatId: string;
  action: 'nudge' | 'queue' | 'dequeue' | 'query' | 'skipUserTurn';
  participantId?: string;
}

/** Edit a message's content (v4 PUT `/messages/:id`). */
export interface MessageEditRequest {
  type: 'messageEdit';
  messageId: string;
  content: string;
}

/** Delete a message, optionally handling the memory cascade (v4 DELETE `/messages/:id`). */
export interface MessageDeleteRequest {
  type: 'messageDelete';
  messageId: string;
  /** The v4 memory-cascade choice. */
  memoryAction?: 'KEEP_MEMORIES' | 'DELETE_MEMORIES' | 'REGENERATE_MEMORIES';
  /** Skip the confirmation round-trip once the user has chosen. */
  skipConfirmation?: boolean;
}

/** Switch / generate a swipe variant (v4 `messageSwipe`). */
export interface MessageSwipeRequest {
  type: 'messageSwipe';
  messageId: string;
  /** Omit to generate a NEW variant; provide to switch to an existing one. */
  swipeIndex?: number;
}

/** Impersonation + Speaking-As controls (v4 `actions/participants.ts`). */
export interface ChatImpersonateRequest {
  type: 'chatImpersonate';
  chatId: string;
  participantId: string;
}
export interface ChatStopImpersonateRequest {
  type: 'chatStopImpersonate';
  chatId: string;
  participantId: string;
}
export interface ChatSetActiveSpeakerRequest {
  type: 'chatSetActiveSpeaker';
  chatId: string;
  participantId: string | null;
}

/**
 * One participant in the flattened chat-create body (v4 `createChatSchema`
 * participant). `connectionProfileId` rides only LLM-controlled entries;
 * `controlledBy` is `'user'` for the human's in-place persona.
 */
export interface ChatCreateParticipantInput {
  type: 'CHARACTER';
  characterId: string;
  connectionProfileId?: string;
  selectedSystemPromptId?: string;
  controlledBy?: 'llm' | 'user';
}

/** The starting-outfit mode per character (v4 `OutfitSelectionMode`). */
export type OutfitSelectionMode = 'default' | 'manual' | 'llm_choose' | 'none' | 'previous_chat';

/** One character's starting-outfit choice (v4 `OutfitSelection`). */
export interface ChatCreateOutfitSelectionInput {
  characterId: string;
  mode: OutfitSelectionMode;
  /** Present only for `manual` (the wardrobe-composer deferral). */
  slots?: Record<string, unknown>;
}

/**
 * The chat-creation request (re-pinned P4.6q from v4's flattened `POST
 * /api/v1/chats` body and the live `services/chat_create::ChatCreateRequest`).
 * The dispatch flattens every field beside `type` into the server's create
 * driver. Scenario source precedence is `scenarioId` > `projectScenarioPath` >
 * (`groupScenarioPath` + `groupScenarioGroupId`) > `generalScenarioPath`; free
 * `scenario` notes ride independently and layer beneath the resolved preset.
 * `timestampConfig` is omitted when its `mode` is `NONE`; `avatarGenerationEnabled`
 * is only ever sent `true`. The autonomous-room fields are carried for shape (the
 * P4.6q form defers autonomous mode) — their hours→ms / minutes→ms conversions
 * live at submit.
 */
export interface ChatCreateRequest {
  type: 'chatCreate';
  title: string;
  participants: ChatCreateParticipantInput[];
  imageProfileId?: string;
  scenario?: string;
  scenarioId?: string;
  projectScenarioPath?: string;
  groupScenarioPath?: string;
  groupScenarioGroupId?: string;
  generalScenarioPath?: string;
  timestampConfig?: TimestampConfig;
  projectId?: string;
  avatarGenerationEnabled?: boolean;
  outfitSelections?: ChatCreateOutfitSelectionInput[];
  continuationFromChatId?: string;
  progressId?: string;
  // Autonomous-room fields (deferred this round; kept for shape parity).
  chatType?: string;
  scheduleCron?: string;
  scheduleFreshnessWindowMs?: number;
  budgetMaxTurns?: number;
  budgetMaxTokens?: number;
  budgetMaxWallClockMs?: number;
  budgetEstimatedSpendCapUSD?: number;
  runVisibility?: 'owner_only' | 'household' | 'open';
  runDestructiveToolsAllowed?: boolean;
  budgetExcludeCacheHits?: boolean;
}

// ---------------------------------------------------------------------------
// Settings surface (P4.6d implements the server side; see its Shared contract)
// ---------------------------------------------------------------------------

/** PUT the chat-settings row (v4 `settings/chat` PUT) — a partial merge. */
export interface ChatSettingsUpdateRequest {
  type: 'chatSettingsUpdate';
  settings: Record<string, unknown>;
}

/** Create / update / delete / reorder a connection profile (v4 `connection-profiles`). */
export interface ConnectionProfileCreateRequest {
  type: 'connectionProfileCreate';
  profile: Record<string, unknown>;
}
export interface ConnectionProfileUpdateRequest {
  type: 'connectionProfileUpdate';
  profileId: string;
  profile: Record<string, unknown>;
}
export interface ConnectionProfileDeleteRequest {
  type: 'connectionProfileDelete';
  profileId: string;
}
/** Persist a new profile order (v4 `?action=reorder`) — ids in display order. */
export interface ConnectionProfileReorderRequest {
  type: 'connectionProfileReorder';
  orderedIds: string[];
}
/** The provider actions (v4 `?action=test-connection` / `test-message`). */
export interface ConnectionProfileTestRequest {
  type: 'connectionProfileTest';
  profile: Record<string, unknown>;
}
export interface ConnectionProfileTestMessageRequest {
  type: 'connectionProfileTestMessage';
  profile: Record<string, unknown>;
}

/** API-key CRUD + test (v4 `api-keys`). */
export interface ApiKeyCreateRequest {
  type: 'apiKeyCreate';
  label: string;
  provider: string;
  apiKey: string;
}
export interface ApiKeyUpdateRequest {
  type: 'apiKeyUpdate';
  apiKeyId: string;
  label?: string;
  isActive?: boolean;
  apiKey?: string;
}
export interface ApiKeyDeleteRequest {
  type: 'apiKeyDelete';
  apiKeyId: string;
}
export interface ApiKeyTestRequest {
  type: 'apiKeyTest';
  apiKeyId: string;
}

/** The registry listing + models read/fetch (v4 `providers` / `models`). */
export interface ModelListRequest {
  type: 'modelList';
  provider?: string;
}
export interface ModelFetchRequest {
  type: 'modelFetch';
  provider: string;
  apiKeyId?: string;
  baseUrl?: string;
}

// ---------------------------------------------------------------------------
// Characters surface (P4.6f implements the server side; see its Shared contract)
//
// Serde names transcribed VERBATIM from the p4.6f Shared contract. The response
// bodies are pinned by lane A's differentials, not by a Rust `Response` type, so
// the SPA reads these ops through {@link CoreClient.dispatchData} (raw `data`)
// rather than a narrowed response variant — the response `type` string is not
// load-bearing here (the settings-lane precedent for unpinned response types).
// ---------------------------------------------------------------------------

/** List characters with the v4 `npc` / `controlledBy` filters (v4 GET `/characters`). */
export interface CharacterListRequest {
  type: 'characterList';
  npc?: 'true' | 'false';
  controlledBy?: 'user' | 'llm';
}

/** The detail projection (v4 GET `/characters/:id`). */
export interface CharacterGetRequest {
  type: 'characterGet';
  characterId: string;
}

/** Create a character from the full form bag (v4 POST `/characters`). */
export interface CharacterCreateRequest {
  type: 'characterCreate';
  character: Record<string, unknown>;
}

/** Quick-create by name only (v4 POST `?action=quick-create`). */
export interface CharacterQuickCreateRequest {
  type: 'characterQuickCreate';
  name: string;
}

/** Update a character (v4 PUT `/characters/:id`) — the whole form bag merged. */
export interface CharacterUpdateRequest {
  type: 'characterUpdate';
  characterId: string;
  character: Record<string, unknown>;
}

/** Delete with the cascade flags (v4 DELETE `/characters/:id`). */
export interface CharacterDeleteRequest {
  type: 'characterDelete';
  characterId: string;
  cascadeChats?: boolean;
  cascadeImages?: boolean;
}

/** The pre-delete impact preview (v4 `?action=cascade-preview`). */
export interface CharacterCascadePreviewRequest {
  type: 'characterCascadePreview';
  characterId: string;
}

/** Set / clear the avatar (v4 `?action=avatar`) — `imageId: null` clears. */
export interface CharacterAvatarRequest {
  type: 'characterAvatar';
  characterId: string;
  imageId: string | null;
}

/** The thin toggle verbs (v4 `?action=favorite|toggle-controlled-by|toggle-carina`). */
export interface CharacterFavoriteRequest {
  type: 'characterFavorite';
  characterId: string;
}
export interface CharacterToggleControlledByRequest {
  type: 'characterToggleControlledBy';
  characterId: string;
}
export interface CharacterToggleCarinaRequest {
  type: 'characterToggleCarina';
  characterId: string;
}

/** Set / clear the default partner (v4 `?action=set-default-partner`). */
export interface CharacterSetDefaultPartnerRequest {
  type: 'characterSetDefaultPartner';
  characterId: string;
  partnerId: string | null;
}

/** Add / remove a tag on the character (v4 `?action=add-tag|remove-tag`). */
export interface CharacterAddTagRequest {
  type: 'characterAddTag';
  characterId: string;
  tagId: string;
}
export interface CharacterRemoveTagRequest {
  type: 'characterRemoveTag';
  characterId: string;
  tagId: string;
}

/** Resolve the character's own tag details (v4 `?action=get-tags`). */
export interface CharacterGetTagsRequest {
  type: 'characterGetTags';
  characterId: string;
}

/** The header stat line (v4 `?action=stats`). */
export interface CharacterStatsRequest {
  type: 'characterStats';
  characterId: string;
}

/** The per-character conversation list (v4 `?action=chats`). */
export interface CharacterChatsRequest {
  type: 'characterChats';
  characterId: string;
  search?: string;
  limit?: number;
  offset?: number;
}

/** Read `defaultPartnerId` (v4 `?action=default-partner`). */
export interface CharacterDefaultPartnerRequest {
  type: 'characterDefaultPartner';
  characterId: string;
}

/** The Ariel Clause depiction-guidelines file (v4 `?action=depiction-guidelines`). */
export interface CharacterDepictionGuidelinesRequest {
  type: 'characterDepictionGuidelines';
  characterId: string;
}
export interface CharacterDepictionGuidelinesUpdateRequest {
  type: 'characterDepictionGuidelinesUpdate';
  characterId: string;
  content: string;
}

/** System-prompt sub-resource CRUD (v4 `/characters/:id/prompts`). */
export interface CharacterPromptListRequest {
  type: 'characterPromptList';
  characterId: string;
}
export interface CharacterPromptCreateRequest {
  type: 'characterPromptCreate';
  characterId: string;
  name: string;
  content: string;
  isDefault?: boolean;
}
export interface CharacterPromptUpdateRequest {
  type: 'characterPromptUpdate';
  characterId: string;
  promptId: string;
  name?: string;
  content?: string;
  isDefault?: boolean;
}
export interface CharacterPromptDeleteRequest {
  type: 'characterPromptDelete';
  characterId: string;
  promptId: string;
}
export interface CharacterPromptSetDefaultRequest {
  type: 'characterPromptSetDefault';
  characterId: string;
  promptId: string;
}

/** Scenario sub-resource CRUD (v4 `/characters/:id/scenarios`). */
export interface CharacterScenarioListRequest {
  type: 'characterScenarioList';
  characterId: string;
}
export interface CharacterScenarioCreateRequest {
  type: 'characterScenarioCreate';
  characterId: string;
  title: string;
  content: string;
}
export interface CharacterScenarioUpdateRequest {
  type: 'characterScenarioUpdate';
  characterId: string;
  scenarioId: string;
  title?: string;
  content?: string;
}
export interface CharacterScenarioDeleteRequest {
  type: 'characterScenarioDelete';
  characterId: string;
  scenarioId: string;
}

/** Plugin-data sub-resource (v4 `/characters/:id/plugin-data`). */
export interface CharacterPluginDataMapRequest {
  type: 'characterPluginDataMap';
  characterId: string;
}
export interface CharacterPluginDataUpsertRequest {
  type: 'characterPluginDataUpsert';
  characterId: string;
  pluginName: string;
  data: unknown;
}
export interface CharacterPluginDataGetRequest {
  type: 'characterPluginDataGet';
  characterId: string;
  pluginName: string;
}
export interface CharacterPluginDataDeleteRequest {
  type: 'characterPluginDataDelete';
  characterId: string;
  pluginName: string;
}

/** Wardrobe sub-resource (v4 `/characters/:id/wardrobe`) — the dialog is deferred,
 *  but the read/CRUD contract is transcribed for completeness. */
export interface CharacterWardrobeListRequest {
  type: 'characterWardrobeList';
  characterId: string;
}
export interface CharacterWardrobeCreateRequest {
  type: 'characterWardrobeCreate';
  characterId: string;
  item: Record<string, unknown>;
}
export interface CharacterWardrobeGetRequest {
  type: 'characterWardrobeGet';
  characterId: string;
  itemId: string;
}
export interface CharacterWardrobeUpdateRequest {
  type: 'characterWardrobeUpdate';
  characterId: string;
  itemId: string;
  item: Record<string, unknown>;
}
export interface CharacterWardrobeDeleteRequest {
  type: 'characterWardrobeDelete';
  characterId: string;
  itemId: string;
}

/** SillyTavern export / import (v4 `?action=export` / `?action=import`). */
export interface CharacterExportRequest {
  type: 'characterExport';
  characterId: string;
  format: 'json';
}
export interface CharacterImportRequest {
  type: 'characterImport';
  payload: unknown;
}

/** Photo gallery reads + JSON saves (the multipart upload leg is a web route). */
export interface CharacterPhotoListRequest {
  type: 'characterPhotoList';
  characterId: string;
  limit?: number;
  offset?: number;
}
export interface CharacterPhotoSaveByIdRequest {
  type: 'characterPhotoSaveById';
  characterId: string;
  fileId?: string;
  linkId?: string;
}
export interface CharacterPhotoRemoveRequest {
  type: 'characterPhotoRemove';
  characterId: string;
  linkId: string;
}

/** Tags CRUD (v4 `/tags`). */
export interface TagListRequest {
  type: 'tagList';
  search?: string;
}
export interface TagCreateRequest {
  type: 'tagCreate';
  name: string;
}
export interface TagGetRequest {
  type: 'tagGet';
  tagId: string;
}
export interface TagUpdateRequest {
  type: 'tagUpdate';
  tagId: string;
  name?: string;
  visualStyle?: string;
  quickHide?: boolean;
}
export interface TagDeleteRequest {
  type: 'tagDelete';
  tagId: string;
}

// ---------------------------------------------------------------------------
// Groups + Projects surface (P4.6k implements the server side; see its Shared
// contract). Serde names transcribed VERBATIM from the p4.6k/l Shared contract.
// The response bodies are pinned by lane A's differentials, not by a Rust
// `Response` type, so the SPA reads these ops through {@link CoreClient.dispatchData}
// (raw `data`) rather than a narrowed response variant.
// ---------------------------------------------------------------------------

/** List groups with `_count.members` (v4 GET `/groups`, createdAt desc). */
export interface GroupListRequest {
  type: 'groupList';
}

/** Create a group (v4 POST `/groups`). */
export interface GroupCreateRequest {
  type: 'groupCreate';
  name: string;
  description?: string | null;
  color?: string | null;
  icon?: string | null;
}

/** The group detail projection (v4 GET `/groups/:id`). */
export interface GroupGetRequest {
  type: 'groupGet';
  groupId: string;
}

/** The group patch bag (v4 PUT `/groups/:id` body, `updateGroupSchema`). */
export interface GroupUpdatePatch {
  name?: string;
  description?: string | null;
  color?: string | null;
  icon?: string | null;
}

/**
 * Update a group (v4 PUT `/groups/:id`). The patch rides a nested `group` bag —
 * the shape lane A pinned and differential-proved (reconciled at unification;
 * the flat form the order sketched was never live).
 */
export interface GroupUpdateRequest {
  type: 'groupUpdate';
  groupId: string;
  group: GroupUpdatePatch;
}

/** Delete a group (v4 DELETE `/groups/:id`) — immediate, no confirm. */
export interface GroupDeleteRequest {
  type: 'groupDelete';
  groupId: string;
}

/** The group's members (v4 `?action=members`). */
export interface GroupMembersRequest {
  type: 'groupMembers';
  groupId: string;
}

/** Add / remove a member character (v4 `?action=addMember|removeMember`). */
export interface GroupMemberAddRequest {
  type: 'groupMemberAdd';
  groupId: string;
  characterId: string;
}
export interface GroupMemberRemoveRequest {
  type: 'groupMemberRemove';
  groupId: string;
  characterId: string;
}

/** The document stores linked to a group (v4 GET `/groups/:id/mount-points`). */
export interface GroupMountPointListRequest {
  type: 'groupMountPointList';
  groupId: string;
}
export interface GroupMountPointLinkRequest {
  type: 'groupMountPointLink';
  groupId: string;
  mountPointId: string;
}
export interface GroupMountPointUnlinkRequest {
  type: 'groupMountPointUnlink';
  groupId: string;
  mountPointId: string;
}

/**
 * The scenario body bag — re-pinned 2026-07-11 from v4's Zod schemas
 * (`createScenarioSchema` / `updateScenarioSchema`), identical across the
 * group / project / general families (supersedes the earlier
 * `{name, content, isDefault}` sketch).
 *
 * `filename` vs `name` is a four-vantage-grade distinction, never collapsed:
 * `filename` is the on-disk name without `.md` (set at create, changed only via
 * rename); `name` is the display title stored in the file's frontmatter. Create
 * requires `filename` + `body`; every optional is truly absent (no Zod default).
 */
export interface ScenarioCreateBag {
  filename: string;
  name?: string;
  description?: string;
  isDefault?: boolean;
  body: string;
}

/** The scenario update bag: `body` required; NO `filename` (the path rides the variant's `scenarioPath`). */
export interface ScenarioUpdateBag {
  name?: string;
  description?: string;
  isDefault?: boolean;
  body: string;
}

/** Group scenarios (v4 `/groups/:id/scenarios`), mirror of project + general. */
export interface GroupScenarioListRequest {
  type: 'groupScenarioList';
  groupId: string;
}
export interface GroupScenarioCreateRequest {
  type: 'groupScenarioCreate';
  groupId: string;
  scenario: ScenarioCreateBag;
}
export interface GroupScenarioGetRequest {
  type: 'groupScenarioGet';
  groupId: string;
  scenarioPath: string;
}
export interface GroupScenarioUpdateRequest {
  type: 'groupScenarioUpdate';
  groupId: string;
  scenarioPath: string;
  scenario: ScenarioUpdateBag;
}
export interface GroupScenarioRenameRequest {
  type: 'groupScenarioRename';
  groupId: string;
  scenarioPath: string;
  newFilename: string;
}
export interface GroupScenarioDeleteRequest {
  type: 'groupScenarioDelete';
  groupId: string;
  scenarioPath: string;
}

/** The New-Chat participant-union scenarios (v4 GET `/groups/scenarios?characterIds=`). */
export interface GroupScenariosUnionRequest {
  type: 'groupScenariosUnion';
  characterIds: string[];
}

/** List projects with `_count` (v4 GET `/projects`, createdAt desc). */
export interface ProjectListRequest {
  type: 'projectList';
}

/**
 * Create a project (v4 POST `/projects`). The body rides a nested `project`
 * bag — the shape lane A pinned and differential-proved (reconciled at
 * unification).
 */
export interface ProjectCreateRequest {
  type: 'projectCreate';
  project: {
    name: string;
    description?: string;
  };
}

/** The project detail projection (v4 GET `/projects/:id`). */
export interface ProjectGetRequest {
  type: 'projectGet';
  projectId: string;
}

/** The project patch bag (v4 PUT `/projects/:id` body, `updateProjectSchema`). */
export interface ProjectUpdatePatch {
  name?: string;
  description?: string | null;
  instructions?: string | null;
  allowAnyCharacter?: boolean;
  characterRoster?: string[];
  color?: string | null;
  icon?: string | null;
  defaultAgentModeEnabled?: boolean | null;
  defaultAvatarGenerationEnabled?: boolean | null;
  defaultImageProfileId?: string | null;
  defaultRoleplayTemplateId?: string | null;
  defaultAlertCharactersOfLanternImages?: boolean | null;
  answerConfirmationOverride?: 'ON' | 'OFF' | null;
  backgroundDisplayMode?: 'latest_chat' | 'project' | 'static' | 'theme';
}

/**
 * Update a project (v4 PUT `/projects/:id`). The partial rides a nested
 * `project` bag — the shape lane A pinned and differential-proved (reconciled
 * at unification).
 */
export interface ProjectUpdateRequest {
  type: 'projectUpdate';
  projectId: string;
  project: ProjectUpdatePatch;
}

/** Delete a project (v4 DELETE `/projects/:id`) — chats/files disassociated. */
export interface ProjectDeleteRequest {
  type: 'projectDelete';
  projectId: string;
}

/** The project roster (v4 `?action=roster`) + roster mutations. */
export interface ProjectCharacterListRequest {
  type: 'projectCharacterList';
  projectId: string;
}
export interface ProjectCharacterAddRequest {
  type: 'projectCharacterAdd';
  projectId: string;
  characterId: string;
}
export interface ProjectCharacterRemoveRequest {
  type: 'projectCharacterRemove';
  projectId: string;
  characterId: string;
}

/** The project chats page (v4 `?action=chats`, limit/offset) + chat mutations. */
export interface ProjectChatListRequest {
  type: 'projectChatList';
  projectId: string;
  limit?: number;
  offset?: number;
}
export interface ProjectChatAddRequest {
  type: 'projectChatAdd';
  projectId: string;
  chatId: string;
}
export interface ProjectChatRemoveRequest {
  type: 'projectChatRemove';
  projectId: string;
  chatId: string;
}

/** The project files list (v4 `?action=files`, two-branch DTO) + file mutations. */
export interface ProjectFileListRequest {
  type: 'projectFileList';
  projectId: string;
}
export interface ProjectFileAddRequest {
  type: 'projectFileAdd';
  projectId: string;
  fileId: string;
}
export interface ProjectFileRemoveRequest {
  type: 'projectFileRemove';
  projectId: string;
  fileId: string;
}

/** Project state (v4 `?action=state`) — set REPLACES wholesale; reset returns previous. */
export interface ProjectStateGetRequest {
  type: 'projectStateGet';
  projectId: string;
}
export interface ProjectStateSetRequest {
  type: 'projectStateSet';
  projectId: string;
  state: Record<string, unknown>;
}
export interface ProjectStateResetRequest {
  type: 'projectStateReset';
  projectId: string;
}

/** The story-background URL resolution by display mode (v4 `?action=background`). */
export interface ProjectBackgroundGetRequest {
  type: 'projectBackgroundGet';
  projectId: string;
}

/** The Prospero aesthetic files (v4 `?action=aesthetic`) — `lantern|aurora` only. */
export interface ProjectAestheticGetRequest {
  type: 'projectAestheticGet';
  projectId: string;
  kind: 'lantern' | 'aurora';
}
export interface ProjectAestheticSetRequest {
  type: 'projectAestheticSet';
  projectId: string;
  kind: 'lantern' | 'aurora';
  content?: string;
}

/** Default tool settings (v4 `?action=update-tool-settings`) — the SPA row is deferred. */
export interface ProjectToolSettingsUpdateRequest {
  type: 'projectToolSettingsUpdate';
  projectId: string;
  defaultDisabledTools: string[];
  defaultDisabledToolGroups: string[];
}

/** The document stores linked to a project (v4 GET `/projects/:id/mount-points`). */
export interface ProjectMountPointListRequest {
  type: 'projectMountPointList';
  projectId: string;
}
export interface ProjectMountPointLinkRequest {
  type: 'projectMountPointLink';
  projectId: string;
  mountPointId: string;
}
export interface ProjectMountPointUnlinkRequest {
  type: 'projectMountPointUnlink';
  projectId: string;
  mountPointId: string;
}

/** Project scenarios (v4 `/projects/:id/scenarios`), mirror of groups + general. */
export interface ProjectScenarioListRequest {
  type: 'projectScenarioList';
  projectId: string;
}
export interface ProjectScenarioCreateRequest {
  type: 'projectScenarioCreate';
  projectId: string;
  scenario: ScenarioCreateBag;
}
export interface ProjectScenarioGetRequest {
  type: 'projectScenarioGet';
  projectId: string;
  scenarioPath: string;
}
export interface ProjectScenarioUpdateRequest {
  type: 'projectScenarioUpdate';
  projectId: string;
  scenarioPath: string;
  scenario: ScenarioUpdateBag;
}
export interface ProjectScenarioRenameRequest {
  type: 'projectScenarioRename';
  projectId: string;
  scenarioPath: string;
  newFilename: string;
}
export interface ProjectScenarioDeleteRequest {
  type: 'projectScenarioDelete';
  projectId: string;
  scenarioPath: string;
}

/**
 * General (instance-wide "Quilltap General" mount) scenarios (v4 `/scenarios`).
 * Same bag + verbs as the project/group families, minus the scope id — the
 * mount is resolved server-side (a null `mountPointId` means unprovisioned).
 */
export interface ScenarioListRequest {
  type: 'scenarioList';
}
export interface ScenarioCreateRequest {
  type: 'scenarioCreate';
  scenario: ScenarioCreateBag;
}
export interface ScenarioGetRequest {
  type: 'scenarioGet';
  scenarioPath: string;
}
export interface ScenarioUpdateRequest {
  type: 'scenarioUpdate';
  scenarioPath: string;
  scenario: ScenarioUpdateBag;
}
export interface ScenarioRenameRequest {
  type: 'scenarioRename';
  scenarioPath: string;
  newFilename: string;
}
export interface ScenarioDeleteRequest {
  type: 'scenarioDelete';
  scenarioPath: string;
}

/** Project wardrobe (v4 `/projects/:id/wardrobe`) — reuses the character wardrobe machinery. */
export interface ProjectWardrobeListRequest {
  type: 'projectWardrobeList';
  projectId: string;
}
export interface ProjectWardrobeCreateRequest {
  type: 'projectWardrobeCreate';
  projectId: string;
  item: Record<string, unknown>;
}
export interface ProjectWardrobeGetRequest {
  type: 'projectWardrobeGet';
  projectId: string;
  itemId: string;
}
export interface ProjectWardrobeUpdateRequest {
  type: 'projectWardrobeUpdate';
  projectId: string;
  itemId: string;
  item: Record<string, unknown>;
}
export interface ProjectWardrobeDeleteRequest {
  type: 'projectWardrobeDelete';
  projectId: string;
  itemId: string;
}

/** The internally-tagged request union (one variant per user-meaningful op). */
export type CoreRequest =
  | { type: 'health' }
  | { type: 'unlockState' }
  | { type: 'unlock'; passphrase: string }
  | { type: 'lock' }
  | { type: 'listInstances' }
  | ListChatsRequest
  | ChatSendRequest
  | { type: 'setup'; passphrase: string }
  | { type: 'storePepper'; passphrase: string }
  | { type: 'changePassphrase'; oldPassphrase: string; newPassphrase: string }
  | ChatCreateRequest
  // --- The Salon conversation surface (P4.6a implements the server side) ---
  | ChatGetRequest
  | { type: 'chatSettings' }
  | ChatUpdateRequest
  | ChatTurnActionRequest
  | MessageEditRequest
  | MessageDeleteRequest
  | MessageSwipeRequest
  | ChatImpersonateRequest
  | ChatStopImpersonateRequest
  | ChatSetActiveSpeakerRequest
  // --- The Settings surface (P4.6d implements the server side) ---
  | ChatSettingsUpdateRequest
  | { type: 'connectionProfileList' }
  | ConnectionProfileCreateRequest
  | ConnectionProfileUpdateRequest
  | ConnectionProfileDeleteRequest
  | ConnectionProfileReorderRequest
  | { type: 'connectionProfileResetSort' }
  | ConnectionProfileTestRequest
  | ConnectionProfileTestMessageRequest
  | { type: 'apiKeyList' }
  | ApiKeyCreateRequest
  | ApiKeyUpdateRequest
  | ApiKeyDeleteRequest
  | ApiKeyTestRequest
  | { type: 'providerList' }
  | ModelListRequest
  | ModelFetchRequest
  // --- The Characters surface (P4.6f implements the server side) ---
  | CharacterListRequest
  | CharacterGetRequest
  | CharacterCreateRequest
  | CharacterQuickCreateRequest
  | CharacterUpdateRequest
  | CharacterDeleteRequest
  | CharacterCascadePreviewRequest
  | CharacterAvatarRequest
  | CharacterFavoriteRequest
  | CharacterToggleControlledByRequest
  | CharacterToggleCarinaRequest
  | CharacterSetDefaultPartnerRequest
  | CharacterAddTagRequest
  | CharacterRemoveTagRequest
  | CharacterGetTagsRequest
  | CharacterStatsRequest
  | CharacterChatsRequest
  | CharacterDefaultPartnerRequest
  | CharacterDepictionGuidelinesRequest
  | CharacterDepictionGuidelinesUpdateRequest
  | CharacterPromptListRequest
  | CharacterPromptCreateRequest
  | CharacterPromptUpdateRequest
  | CharacterPromptDeleteRequest
  | CharacterPromptSetDefaultRequest
  | CharacterScenarioListRequest
  | CharacterScenarioCreateRequest
  | CharacterScenarioUpdateRequest
  | CharacterScenarioDeleteRequest
  | CharacterPluginDataMapRequest
  | CharacterPluginDataUpsertRequest
  | CharacterPluginDataGetRequest
  | CharacterPluginDataDeleteRequest
  | CharacterWardrobeListRequest
  | CharacterWardrobeCreateRequest
  | CharacterWardrobeGetRequest
  | CharacterWardrobeUpdateRequest
  | CharacterWardrobeDeleteRequest
  | CharacterExportRequest
  | CharacterImportRequest
  | CharacterPhotoListRequest
  | CharacterPhotoSaveByIdRequest
  | CharacterPhotoRemoveRequest
  | TagListRequest
  | TagCreateRequest
  | TagGetRequest
  | TagUpdateRequest
  | TagDeleteRequest
  // --- The Groups + Projects surface (P4.6k implements the server side) ---
  | GroupListRequest
  | GroupCreateRequest
  | GroupGetRequest
  | GroupUpdateRequest
  | GroupDeleteRequest
  | GroupMembersRequest
  | GroupMemberAddRequest
  | GroupMemberRemoveRequest
  | GroupMountPointListRequest
  | GroupMountPointLinkRequest
  | GroupMountPointUnlinkRequest
  | GroupScenarioListRequest
  | GroupScenarioCreateRequest
  | GroupScenarioGetRequest
  | GroupScenarioUpdateRequest
  | GroupScenarioRenameRequest
  | GroupScenarioDeleteRequest
  | GroupScenariosUnionRequest
  | ProjectListRequest
  | ProjectCreateRequest
  | ProjectGetRequest
  | ProjectUpdateRequest
  | ProjectDeleteRequest
  | ProjectCharacterListRequest
  | ProjectCharacterAddRequest
  | ProjectCharacterRemoveRequest
  | ProjectChatListRequest
  | ProjectChatAddRequest
  | ProjectChatRemoveRequest
  | ProjectFileListRequest
  | ProjectFileAddRequest
  | ProjectFileRemoveRequest
  | ProjectStateGetRequest
  | ProjectStateSetRequest
  | ProjectStateResetRequest
  | ProjectBackgroundGetRequest
  | ProjectAestheticGetRequest
  | ProjectAestheticSetRequest
  | ProjectToolSettingsUpdateRequest
  | ProjectMountPointListRequest
  | ProjectMountPointLinkRequest
  | ProjectMountPointUnlinkRequest
  | ProjectScenarioListRequest
  | ProjectScenarioCreateRequest
  | ProjectScenarioGetRequest
  | ProjectScenarioUpdateRequest
  | ProjectScenarioRenameRequest
  | ProjectScenarioDeleteRequest
  | ProjectWardrobeListRequest
  | ProjectWardrobeCreateRequest
  | ProjectWardrobeGetRequest
  | ProjectWardrobeUpdateRequest
  | ProjectWardrobeDeleteRequest
  | ScenarioListRequest
  | ScenarioCreateRequest
  | ScenarioGetRequest
  | ScenarioUpdateRequest
  | ScenarioRenameRequest
  | ScenarioDeleteRequest
  // --- The listing surfaces (P4.6p/q/r Shared contract; byte-identical appendix) ---
  | ListingSurfaceRequest;

export type RequestType = CoreRequest['type'];

// ===========================================================================
// DTOs
// ===========================================================================

export interface HealthDto {
  /** Always `"ok"` if dispatch answered at all. */
  status: string;
  version: string;
  ready: boolean;
  pepperState: PepperState;
}

export interface UnlockStateDto {
  state: PepperState;
  hasUserPassphrase: boolean;
  /** Only populated when unlocked and the user's auto-lock setting is enabled. */
  autoLockMinutes?: number;
}

export interface InstanceDto {
  name: string;
  path: string;
  isDefault: boolean;
  hasPassphrase: boolean;
}

export interface InstancesDto {
  instances: InstanceDto[];
  defaultInstance?: string;
}

/** The pre-enrichment summary (P4.5 foundation shape; retained for reference). */
export interface ChatSummaryDto {
  id: string;
  title: string;
  chatType: string;
  messageCount: number;
  lastMessageAt?: string;
  createdAt: string;
  updatedAt: string;
}

// ---------------------------------------------------------------------------
// Enriched list (v4 `EnrichedChatSummary` — `handleList` / `cleanEnrichedChats`)
// ---------------------------------------------------------------------------

export interface EnrichedImage {
  id: string;
  filepath: string;
  url: string | null;
}

/** A list-card character summary (v4 `EnrichedCharacterSummary`). */
export interface EnrichedCharacterSummary {
  id: string;
  name: string;
  title: string | null;
  avatarUrl: string | null;
  defaultImageId: string | null;
  defaultImage: EnrichedImage | null;
  talkativeness: number;
  /** Tag IDs (strings), resolved to styles client-side. */
  tags: string[];
}

export interface EnrichedParticipantSummary {
  id: string;
  type: 'CHARACTER';
  displayOrder: number;
  isActive: boolean;
  status: string;
  removedAt?: string | null;
  character: EnrichedCharacterSummary | null;
}

/** A chat tag reference (v4 `EnrichedTag`). */
export interface EnrichedTag {
  tag: { id: string; name: string };
}

export interface EnrichedProject {
  id: string;
  name: string;
  color: string | null;
}

export interface EnrichedStoryBackground {
  id: string;
  filepath: string;
}

/** One row in the Salon list (v4 `EnrichedChatSummary`, minus `_allTagIds`). */
export interface EnrichedChatSummary {
  id: string;
  title: string;
  contextSummary: string | null;
  createdAt: string;
  updatedAt: string;
  lastMessageAt: string | null;
  participants: EnrichedParticipantSummary[];
  tags: EnrichedTag[];
  project: EnrichedProject | null;
  storyBackground: EnrichedStoryBackground | null;
  isDangerousChat: boolean;
  conciergeOverride: 'OFF' | null;
  chatType: 'salon' | 'help' | 'autonomous' | 'brahma';
  scriptoriumStatus: 'none' | 'rendered' | 'embedded';
  _count: { messages: number; memories: number };
}

export interface ChatSendResultDto {
  messageId: string;
  hasContent: boolean;
  isMultiCharacter: boolean;
  isPaused: boolean;
  userParticipantId?: string;
}

// ---------------------------------------------------------------------------
// Conversation detail (v4 `handleGet` → `{ chat }`)
// ---------------------------------------------------------------------------

/** A reasoning block anchored into the content (v4 `ReasoningSegment`). */
export interface MessageReasoningSegment {
  anchorOffset: number;
  content: string;
  seq: number;
}

export interface MessageAttachment {
  id: string;
  filename: string;
  filepath: string;
  mimeType: string;
  sha256?: string;
}

/** v4 systemSender union (the personified Staff). */
export type SystemSender =
  | 'lantern'
  | 'aurora'
  | 'librarian'
  | 'concierge'
  | 'prospero'
  | 'host'
  | 'commonplaceBook'
  | 'ariel'
  | 'carina'
  | 'suparna'
  | null;

export interface HostEvent {
  participantId?: string | null;
  toStatus?: string | null;
  introducedCharacterIds?: string[] | null;
}

export interface CustomAnnouncer {
  kind: 'character' | 'custom';
  characterId?: string | null;
  displayName?: string | null;
}

export interface CarinaMeta {
  answererId: string;
  question: string;
}

/**
 * One persisted message (v4 `handleGet` per-message projection) — MINUS
 * `renderedHtml` (v5 renders markdown client-side; see the rendering service).
 */
export interface MessageDto {
  id: string;
  role: 'USER' | 'ASSISTANT' | 'SYSTEM';
  content: string;
  tokenCount: number | null;
  promptTokens: number | null;
  completionTokens: number | null;
  createdAt: string;
  swipeGroupId: string | null;
  swipeIndex: number | null;
  participantId: string | null;
  attachments: MessageAttachment[];
  provider: string | null;
  modelName: string | null;
  targetParticipantIds: string[] | null;
  isSilentMessage: boolean | null;
  systemSender: SystemSender;
  systemKind: string | null;
  hostEvent: HostEvent | null;
  customAnnouncer: CustomAnnouncer | null;
  carinaMeta: CarinaMeta | null;
  pendingExternalPrompt: string | null;
  pendingExternalPromptFull: string | null;
  pendingExternalAttachments: unknown[] | null;
  reasoningContent: string | null;
  reasoningSegments: MessageReasoningSegment[] | null;
  confirmed?: boolean;
  confirmationChecked?: boolean;
  confirmationRevised?: boolean;
  confirmationNotes?: string | null;
  confirmationOriginalContent?: string | null;
}

/** An enriched participant on the conversation detail (v4 `enrichParticipantDetail`). */
export interface ParticipantDetail {
  id: string;
  type: string;
  displayOrder: number;
  isActive: boolean;
  controlledBy: 'llm' | 'user';
  status: 'active' | 'silent' | 'absent' | 'removed';
  removedAt?: string | null;
  character: DetailCharacter | null;
  connectionProfile: { id: string; name: string; provider: string; modelName: string } | null;
  imageProfile: { id: string; name: string; provider: string; modelName: string } | null;
  selectedSystemPromptId?: string | null;
  talkativeness?: number | null;
  createdAt: string;
  updatedAt: string;
}

export interface DetailCharacter {
  id: string;
  name: string;
  title: string | null;
  avatarUrl: string | null;
  defaultImageId: string | null;
  defaultImage: EnrichedImage | null;
  talkativeness?: number;
}

export interface OffSceneCharacter {
  id: string;
  name: string;
  title: string | null;
  avatarUrl: string | null;
}

/** The `{ chat }` body of v4 `handleGet` (the fields the read path consumes). */
export interface ChatDetail {
  id: string;
  title: string;
  contextSummary: string | null;
  roleplayTemplateId: string | null;
  chatType: 'salon' | 'autonomous' | 'help' | 'brahma';
  createdAt: string;
  updatedAt: string;
  isPaused: boolean;
  isManuallyRenamed: boolean;
  participants: ParticipantDetail[];
  user: { id: string; name: string; image: string | null };
  messages: MessageDto[];
  projectId: string | null;
  projectName: string | null;
  turnSkippingEnabled: boolean | null;
  agentModeEnabled: boolean;
  resolvedAgentModeEnabled: boolean;
  agentModeSource: string;
  isDangerousChat: boolean | null;
  dangerCategories: string[];
  conciergeOverride: 'OFF' | null;
  offSceneCharacters: OffSceneCharacter[];
  lastTurnParticipantId: string | null;
  activeTypingParticipantId?: string | null;
  impersonatingParticipantIds?: string[];
}

// ---------------------------------------------------------------------------
// Characters DTOs (v4 `app/aurora/[id]/view/types.ts` + the handler projections)
// ---------------------------------------------------------------------------

/** The subject/object/possessive pronoun triple (v4 `pronouns`). */
export interface Pronouns {
  subject: string;
  object: string;
  possessive: string;
}

/** v4 `TimestampConfig` (kept loose — the SPA round-trips it opaquely). */
export type TimestampConfig = Record<string, unknown>;

/** One system prompt on a character (v4 `CharacterSystemPrompt`). */
export interface CharacterSystemPrompt {
  id: string;
  name: string;
  content: string;
  isDefault: boolean;
  createdAt: string;
  updatedAt: string;
}

/** One scenario on a character (v4 `CharacterScenario`). */
export interface CharacterScenario {
  id: string;
  title: string;
  content: string;
  createdAt: string;
  updatedAt: string;
}

/** The physical-description packet (v4 `CharacterPhysicalDescription`). */
export interface CharacterPhysicalDescription {
  id?: string;
  name?: string | null;
  usageContext?: string | null;
  headAndShouldersPrompt?: string | null;
  shortPrompt?: string | null;
  mediumPrompt?: string | null;
  longPrompt?: string | null;
  completePrompt?: string | null;
  fullDescription?: string | null;
  createdAt?: string;
  updatedAt?: string;
}

/**
 * One row of the character LIST DTO (v4 `handlers/get.ts:58-92` — the
 * hand-assembled whitelist). The `description` runs through `processTemplate`
 * for the card preview; the toggles read `isFavorite` / `controlledBy` /
 * `canBeCarina`.
 */
export interface CharacterListItem {
  id: string;
  name: string;
  title: string | null;
  description: string | null;
  defaultImageId: string | null;
  defaultImage: EnrichedImage | null;
  isFavorite: boolean;
  controlledBy: 'llm' | 'user';
  canBeCarina: boolean;
  defaultConnectionProfileId: string | null;
  defaultPartnerId: string | null;
  defaultPartnerName: string | null;
  defaultTimestampConfig: TimestampConfig | null;
  defaultScenarioId: string | null;
  defaultSystemPromptId: string | null;
  defaultImageProfileId: string | null;
  npc: boolean;
  createdAt: string;
  tags: string[];
  updatedAt: string;
  systemPrompts: Array<{ id: string; name: string; isDefault: boolean }>;
  scenarios: Array<{ id: string; title: string; content: string }>;
  _count: { chats: number };
}

/**
 * The character DETAIL projection (v4 `[id]/handlers/get.ts` — the full row
 * spread + `defaultImage` + `_count`). The four vantage points
 * (identity / description / manifesto / personality) are DISTINCT — never
 * collapse them.
 */
export interface CharacterDetail {
  id: string;
  name: string;
  title: string | null;
  identity: string | null;
  description: string | null;
  manifesto: string | null;
  personality: string | null;
  scenarios: CharacterScenario[];
  firstMessage: string | null;
  exampleDialogues: string | null;
  systemPrompt?: string | null;
  systemPrompts: CharacterSystemPrompt[];
  physicalDescription: CharacterPhysicalDescription | null;
  avatarUrl?: string | null;
  defaultImageId: string | null;
  defaultImage: EnrichedImage | null;
  defaultConnectionProfileId: string | null;
  controlledBy: 'llm' | 'user';
  isFavorite: boolean;
  canBeCarina: boolean | null;
  npc: boolean;
  defaultAgentModeEnabled: boolean | null;
  defaultHelpToolsEnabled: boolean | null;
  canDressThemselves: boolean | null;
  canCreateOutfits: boolean | null;
  defaultTimestampConfig: TimestampConfig | null;
  defaultScenarioId: string | null;
  defaultSystemPromptId: string | null;
  defaultPartnerId: string | null;
  defaultImageProfileId: string | null;
  aliases: string[];
  pronouns: Pronouns | null;
  characterDocumentMountPointId: string | null;
  tags: string[];
  createdAt: string;
  updatedAt: string;
  _count?: { chats: number };
  [key: string]: unknown;
}

/** The character's own tag details (v4 `?action=get-tags`). */
export interface CharacterTagDetail {
  id: string;
  name: string;
  visualStyle: string | null;
}

/** The header stat line (v4 `?action=stats` → `stats`). */
export interface CharacterStats {
  memories: number;
  conversations: number;
  wardrobeItems: number;
  photos: number;
  scenarios: number;
  knowledge: number;
  core: number;
  characterFiles: number;
  characterFilesTotal: number;
}

/** A group badge on the header (v4 `?action=stats` → `groups`). */
export interface CharacterGroupBadge {
  id: string;
  name: string;
  description: string | null;
  color: string | null;
  icon: string | null;
}

/** One exclusive chat in the delete-cascade preview (v4 `cascade-preview`). */
export interface CascadePreviewChat {
  id: string;
  title: string;
  messageCount: number;
  lastMessageAt: string | null;
}

/** The delete-cascade impact preview (v4 `?action=cascade-preview`). */
export interface CascadePreview {
  characterId: string;
  characterName: string;
  exclusiveChats: CascadePreviewChat[];
  exclusiveCharacterImageCount: number;
  exclusiveChatImageCount: number;
  totalExclusiveImageCount: number;
  memoryCount: number;
}

/** One recent-message preview in a per-character chat card (v4 `action=chats`). */
export interface CharacterChatMessagePreview {
  id: string;
  role: string;
  content: string;
  createdAt: string;
}

/** One conversation in the character Conversations tab (v4 `action=chats`). */
export interface CharacterChatSummary {
  id: string;
  title: string | null;
  createdAt: string;
  updatedAt: string;
  lastMessageAt: string | null;
  character: { id: string; name: string };
  project: { id: string; name: string } | null;
  storyBackground: { id: string; filepath: string } | null;
  /** Up to three most-recent messages, recent-first (v4 `slice(0, 3)`). */
  messages: CharacterChatMessagePreview[];
  tags: Array<{ tag: { id: string; name: string } }>;
  isDangerousChat: boolean;
  _count: { messages: number; memories: number };
  scriptoriumStatus: 'none' | 'rendered' | 'embedded';
}

/** The `action=chats` page (v4 `{ chats, total }`). */
export interface CharacterChatsResult {
  chats: CharacterChatSummary[];
  total: number;
}

/** One row of the tags list (v4 `/tags`). */
export interface TagDto {
  id: string;
  name: string;
  visualStyle?: string | null;
  quickHide?: boolean;
  [key: string]: unknown;
}

/** A photo-gallery entry (v4 `listCharacterGallery` → the pinned P4.6i
 *  `{ entries, total, hasMore }` envelope). `linkId` is the vault
 *  `doc_mount_file_links.id` — it is also what `characterPhotoRemove` takes and
 *  what `characterAvatar {imageId}` stores for vault photos. */
export interface CharacterPhoto {
  linkId: string;
  mountPointId: string;
  relativePath: string;
  fileName: string;
  blobUrl: string;
  mimeType: string;
  sha256: string;
  fileSizeBytes: number;
  keptAt: string;
  caption: string | null;
  tags: string[];
  linkSummary?: unknown;
  [key: string]: unknown;
}

/** The finalized gallery-entry alias (Shared contract `CharacterGalleryEntry`). */
export type CharacterGalleryEntry = CharacterPhoto;

/** A connection profile as the character screens consume it (id + name + model). */
export interface CharacterConnectionProfile {
  id: string;
  name: string;
  provider?: string;
  modelName?: string;
}

// ---------------------------------------------------------------------------
// Chat settings (v4 GET `/api/v1/settings/chat`)
// ---------------------------------------------------------------------------

export interface ChatSettingsDto {
  avatarDisplayMode: 'ALWAYS' | 'GROUP_ONLY' | 'NEVER';
  avatarDisplayStyle: 'CIRCULAR' | 'RECTANGULAR';
  tokenDisplaySettings?: {
    showPerMessageTokens: boolean;
    showPerMessageCost: boolean;
    showChatTotals: boolean;
    showSystemEvents: boolean;
  };
  thinkingDisplay?: { defaultVisible: boolean; defaultCollapsed: boolean };
  dangerousContentSettings?: {
    mode: 'OFF' | 'DETECT_ONLY' | 'AUTO_ROUTE';
    displayMode: 'SHOW' | 'BLUR' | 'COLLAPSE';
    showWarningBadges: boolean;
  };
  autoScrollOnResponseComplete?: boolean;
  [key: string]: unknown;
}

// ---------------------------------------------------------------------------
// Settings DTOs (v4 providers / connection-profiles / api-keys / models bodies)
// ---------------------------------------------------------------------------

/** One provider row (v4 GET `/providers`). `icon`/`optionsSchema` are `null` on
 *  the v5 manifest — documented absent fields, kept for shape faithfulness. */
export interface ProviderInfo {
  id: string;
  name: string;
  displayName: string;
  description: string;
  abbreviation: string;
  colors?: { bg: string; text: string; icon: string };
  icon?: string | null;
  type: 'llm' | 'search' | string;
  capabilities: {
    chat: boolean;
    imageGeneration: boolean;
    embeddings: boolean;
    webSearch: boolean;
    toolUse?: boolean;
  };
  configRequirements: {
    requiresApiKey: boolean;
    requiresBaseUrl: boolean;
    apiKeyLabel?: string;
    baseUrlLabel?: string;
    baseUrlDefault?: string;
    baseUrlPlaceholder?: string;
  };
  optionsSchema?: unknown | null;
}

/** The masked API-key projection (v4 `api-keys` list — never the plaintext key). */
export interface ApiKeyDto {
  id: string;
  provider: string;
  label: string;
  isActive: boolean;
  lastUsed: string | null;
  createdAt: string;
  updatedAt: string;
  keyPreview: string;
}

/** An auto-association surfaced by `apiKeyCreate` (v4 `ProfileAssociation`). */
export interface ProfileAssociation {
  profileId: string;
  profileName: string;
}

/** The `apiKey` reference joined onto a connection profile (v4 `enrichWithApiKey`). */
export interface ProfileApiKeyRef {
  id: string;
  label: string;
  provider: string;
  isActive: boolean;
}

export interface ProfileTag {
  id: string;
  name: string;
}

/** One enriched connection profile (v4 `connection-profiles` list/detail). */
export interface ConnectionProfileDto {
  id: string;
  name: string;
  transport?: 'api' | 'courier';
  courierDeltaMode?: boolean;
  provider: string;
  apiKeyId?: string | null;
  baseUrl?: string | null;
  modelName: string;
  parameters: Record<string, unknown>;
  isDefault: boolean;
  isCheap?: boolean;
  isDangerousCompatible?: boolean;
  allowWebSearch?: boolean;
  useNativeWebSearch?: boolean;
  allowToolUse?: boolean;
  pseudoToolMode?: 'auto' | 'native' | 'simple-json' | 'text-block';
  supportsImageUpload?: boolean;
  modelClass?: string | null;
  maxContext?: number | null;
  sortIndex?: number;
  apiKey?: ProfileApiKeyRef | null;
  tags?: ProfileTag[];
}

/** Per-model info returned alongside a models fetch (v4 `ModelInfo`). */
export interface ModelInfo {
  id: string;
  displayName?: string;
  deprecated?: boolean;
  experimental?: boolean;
  maxOutputTokens?: number;
  contextWindow?: number;
}

/** The models read/fetch body (v4 POST `/models`). */
export interface ModelsDto {
  models: string[];
  modelsWithInfo?: ModelInfo[];
}

// ---------------------------------------------------------------------------
// Message-mutation / turn-action results
// ---------------------------------------------------------------------------

/** v4 `messageDelete` — either the confirmation prompt or the applied result. */
export type MessageDeleteDto =
  | {
      requiresConfirmation: true;
      memoryCount: number;
      messageIds: string[];
      isSwipeGroup: boolean;
    }
  | { success: boolean; memoriesDeleted?: number };

export interface TurnActionDto {
  [key: string]: unknown;
}

/** The one-time setup response — `data.pepper` is shown once, then never again. */
export interface SetupDto {
  pepper: string;
  message: string;
}

/**
 * The chat-create echo (v4's 201 body / `ChatCreateResultDto`): the created
 * chat under `chat`, whose `participants` array is the enriched participant
 * summaries. The New-Chat submit reads `data.chat.id` to navigate.
 */
export interface ChatCreateDto {
  chat: { id: string; participants?: unknown[]; [key: string]: unknown };
}

// ---------------------------------------------------------------------------
// Groups + Projects DTOs (v4 `app/aurora/types.ts` + `app/prospero` projections)
// ---------------------------------------------------------------------------

/** One group row (v4 `GroupRowSchema` projection + the list `_count.members`). */
export interface GroupSummary {
  id: string;
  name: string;
  description: string | null;
  color: string | null;
  icon: string | null;
  officialMountPointId: string | null;
  state?: Record<string, unknown> | null;
  createdAt: string;
  updatedAt: string;
  /** Present on the list read (`_count.members`); the card reads `memberCount`. */
  _count?: { members: number };
}

/** A member / add-picker character (v4 `GroupMember` — null-filtered `{id, name}`). */
export interface GroupMemberSummary {
  id: string;
  name: string;
}

/**
 * A linked document store (v4 `DocumentStore`). `groupMountPointList` /
 * `projectMountPointList` return the LINKED stores; fields beyond `id`/`name`/
 * `mountType` are read defensively (the raw mount-point row may omit computed
 * `fileCount`/`totalSizeBytes`/`enabled`).
 */
export interface DocumentStoreSummary {
  id: string;
  name: string;
  description?: string | null;
  mountType: string;
  fileCount?: number;
  totalSizeBytes?: number;
  enabled?: boolean;
  [key: string]: unknown;
}

/** One project list row (v4 list projection + `_count: {chats, files, characters}`). */
export interface ProjectSummary {
  id: string;
  name: string;
  description: string | null;
  color: string | null;
  icon: string | null;
  createdAt: string;
  updatedAt: string;
  _count?: { chats: number; files: number; characters: number };
  [key: string]: unknown;
}

/** One enriched roster character on a project (v4 `?action=roster` projection). */
export interface ProjectRosterCharacter {
  id: string;
  name: string;
  defaultImageId: string | null;
  defaultImage: EnrichedImage | null;
  tags: string[];
  chatCount: number;
}

/**
 * The project DETAIL projection (v4 `actions/project-crud.ts` GET). The full
 * project row + enriched roster + `_count`. Kept loose (index signature) — the
 * per-field save handlers read/write individual keys.
 */
export interface ProjectDetail {
  id: string;
  name: string;
  description: string | null;
  instructions: string | null;
  color: string | null;
  icon: string | null;
  allowAnyCharacter: boolean;
  characterRoster: string[];
  roster?: ProjectRosterCharacter[];
  defaultAgentModeEnabled: boolean | null;
  defaultAvatarGenerationEnabled: boolean | null;
  defaultImageProfileId: string | null;
  defaultRoleplayTemplateId: string | null;
  defaultAlertCharactersOfLanternImages: boolean | null;
  answerConfirmationOverride: 'ON' | 'OFF' | null;
  backgroundDisplayMode: 'latest_chat' | 'project' | 'static' | 'theme';
  state: Record<string, unknown> | null;
  createdAt: string;
  updatedAt: string;
  _count?: { chats: number; files: number; characters: number };
  [key: string]: unknown;
}

/**
 * One project file row (v4 `?action=files` legacy-file-shaped DTO). Store-backed
 * rows add `mountPointId`/`relativePath`; both branches share this shape.
 */
export interface ProjectFileDto {
  id: string;
  fileName: string;
  mimeType: string;
  fileSizeBytes: number;
  category: string;
  filepath?: string | null;
  thumbnailUrl?: string | null;
  createdAt: string;
  updatedAt: string;
  folderPath?: string | null;
  mountPointId?: string;
  relativePath?: string;
  [key: string]: unknown;
}

/**
 * One scenario file in a group/project/general `Scenarios/` folder (v4 scenario
 * DTO — re-pinned 2026-07-11). `filename`/`name` are the four-vantage-grade
 * distinction, never collapsed (see {@link ScenarioCreateBag}); `rawIsDefault`
 * is the file's own frontmatter flag before the multi-default reconciliation
 * that `isDefault` reflects.
 */
export interface ScenarioDto {
  path: string;
  filename: string;
  name: string;
  description?: string;
  isDefault: boolean;
  rawIsDefault: boolean;
  body: string;
  lastModified: string;
  createdAt: string;
  updatedAt: string;
}

/**
 * The scenario list envelope (v4 `{mountPointId, scenarios, warnings}`). For the
 * general family `mountPointId` is `null` when the "Quilltap General" mount has
 * not been provisioned yet (empty arrays alongside).
 */
export interface ScenarioListDto {
  mountPointId: string | null;
  scenarios: ScenarioDto[];
  warnings: string[];
}

/** The four wardrobe coverage slots (v4 `WardrobeItemTypeEnum`). */
export type WardrobeSlotType = 'top' | 'bottom' | 'footwear' | 'accessories';

/**
 * One project (or character) wardrobe item (v4 `WardrobeItemSchema`). The
 * project tier of the tri-tier wardrobe model — wearable by every character in
 * the project's chats. `imagePrompt` is the terse visual cue preferred over
 * `title` in image prompts; a non-empty `componentItemIds` marks a composite;
 * `replace` clears the item's designated slots on wear instead of layering.
 */
export interface WardrobeItemDto {
  id: string;
  characterId?: string | null;
  title: string;
  description?: string | null;
  imagePrompt?: string | null;
  types: WardrobeSlotType[];
  componentItemIds?: string[];
  appropriateness?: string | null;
  isDefault?: boolean;
  replace?: boolean;
  archivedAt?: string | null;
  createdAt: string;
  updatedAt: string;
}

/** The story-background resolution (v4 `?action=background`). */
export interface ProjectBackgroundDto {
  url: string | null;
  sourceChatId?: string | null;
}

// ===========================================================================
// Errors
// ===========================================================================

/**
 * The cross-transport error kind. The known set follows v4's response-helper
 * vocabulary; `unauthorized` is forward-declared for the P4.4 change-passphrase
 * mapping, and the `(string & {})` tail keeps the client lenient toward any kind
 * the server adds later (the P4.4 lane owns `ErrorKind` extension).
 */
export type ErrorKind =
  | 'bad-request'
  | 'not-found'
  | 'locked'
  | 'internal'
  | 'unauthorized'
  // eslint-disable-next-line @typescript-eslint/ban-types
  | (string & {});

export interface CoreError {
  kind: ErrorKind;
  message: string;
  /** Present on readiness refusals so the router can redirect without a re-fetch. */
  pepperState?: PepperState;
}

// ===========================================================================
// Response
// ===========================================================================

export type CoreResponse =
  | { type: 'health'; data: HealthDto }
  | { type: 'unlockState'; data: UnlockStateDto }
  | { type: 'instances'; data: InstancesDto }
  | { type: 'chats'; data: EnrichedChatSummary[] }
  | { type: 'chatSend'; data: ChatSendResultDto }
  | { type: 'setup'; data: SetupDto }
  | { type: 'ack'; data: Record<string, never> }
  | { type: 'chatCreate'; data: ChatCreateDto }
  // --- The Salon conversation surface ---
  | { type: 'chat'; data: { chat: ChatDetail } }
  | { type: 'chatSettings'; data: ChatSettingsDto }
  | { type: 'turnAction'; data: TurnActionDto }
  | { type: 'message'; data: { message: MessageDto } }
  | { type: 'messageDelete'; data: MessageDeleteDto }
  // --- The Settings surface (P4.6d implements the server side) ---
  | { type: 'connectionProfiles'; data: { profiles: ConnectionProfileDto[]; count: number } }
  | { type: 'connectionProfile'; data: { profile: ConnectionProfileDto } }
  | { type: 'connectionTest'; data: { valid: boolean; error?: string } }
  // The test-message response type is not pinned by name in the Shared contract
  // (only its `{success,message?,error?}` body is). `connectionTestMessage` is
  // this lane's best guess; the SPA reads it defensively via `dispatch`, so the
  // exact type string is not load-bearing — reconcile at unification.
  | { type: 'connectionTestMessage'; data: { success: boolean; message?: string; error?: string } }
  | { type: 'apiKeys'; data: { apiKeys: ApiKeyDto[]; count: number } }
  | { type: 'apiKey'; data: { apiKey: ApiKeyDto & { associations?: ProfileAssociation[] } } }
  | { type: 'providers'; data: { providers: ProviderInfo[]; count: number } }
  | { type: 'models'; data: ModelsDto }
  | { type: 'error'; data: CoreError };

export type ResponseType = CoreResponse['type'];

/** Narrow a response to a specific variant, or throw its error message. */
export function expectResponse<T extends ResponseType>(
  resp: CoreResponse,
  type: T,
): Extract<CoreResponse, { type: T }> {
  if (resp.type === 'error') {
    throw new CoreDispatchError(resp.data);
  }
  if (resp.type !== type) {
    throw new Error(`Expected a "${type}" response but got "${resp.type}"`);
  }
  return resp as Extract<CoreResponse, { type: T }>;
}

/** A thrown wrapper around a `{ type: "error" }` response envelope. */
export class CoreDispatchError extends Error {
  readonly kind: ErrorKind;
  readonly pepperState?: PepperState;
  constructor(error: CoreError) {
    super(error.message);
    this.name = 'CoreDispatchError';
    this.kind = error.kind;
    this.pepperState = error.pepperState;
  }
}

// ===========================================================================
// Events (D3): one global stream, every event scope-tagged
// ===========================================================================

/**
 * A reasoning segment on the `done` frame (v4 `ReasoningSegment` — anchor offset
 * into the content, the block text, the sequence).
 */
export interface ReasoningSegment {
  anchorOffset: number;
  content: string;
  seq: number;
}

/**
 * One parsed chat stream frame. The wire frames are BARE single-key (or
 * flat-multi-key) objects — never a tagged wrapper — so, exactly as v4's client
 * does, we model them as one flat interface of optional fields and branch on
 * presence. Field names mirror the Rust `ChatEvent` serialization byte-for-byte.
 */
export interface ChatStreamFrame {
  // content / reasoning
  content?: string;
  /** Cumulative live "thinking" — replace, don't append. */
  reasoning?: string;
  /** Full reasoning text on the done frame. */
  reasoningContent?: string | null;
  /** Positioned reasoning blocks on the done frame. */
  reasoningSegments?: ReasoningSegment[] | null;

  // status
  status?: ResponseStatus;

  // transport-shell error frame (v4 `handleStreamError`: {error, errorType, details})
  error?: string;
  errorType?: string;
  details?: string;

  // done family
  done?: boolean;
  messageId?: string | null;
  participantId?: string;
  provider?: string | null;
  modelName?: string | null;
  isSilentMessage?: boolean;
  emptyResponse?: boolean;
  emptyResponseReason?: string;
  toolsExecuted?: boolean;
  skipped?: boolean;
  skippedParticipantId?: string | null;
  pendingExternalTurn?: boolean;
  usage?: TokenUsage | null;
  cacheUsage?: CacheUsage | null;
  turn?: NextSpeakerInfo | null;

  // tool frames
  toolsDetected?: number;
  toolNames?: string[];
  toolArguments?: Array<Record<string, unknown>>;
  toolResult?: ToolResultFrame;

  // turn / chain frames (flat boolean-flag objects)
  turnStart?: boolean;
  turnComplete?: boolean;
  chainComplete?: boolean;
  characterName?: string;
  chainDepth?: number;
  nextSpeakerId?: string | null;
  reason?: string;

  // mid-turn posted messages (full MessageEvent objects)
  carinaAnswer?: PostedMessage;
  hostAnnouncement?: PostedMessage;

  // answer confirmation
  confirmationResult?: ConfirmationResult;
}

export interface ResponseStatus {
  stage: string;
  message: string;
  toolName?: string;
  characterName?: string;
  characterId?: string;
}

export interface TokenUsage {
  promptTokens?: number;
  completionTokens?: number;
  totalTokens?: number;
}

export interface CacheUsage {
  cacheCreationInputTokens?: number;
  cacheReadInputTokens?: number;
}

export interface NextSpeakerInfo {
  nextSpeakerId: string | null;
  reason: string;
  cycleComplete: boolean;
  isUsersTurn: boolean;
}

export interface ToolResultFrame {
  index?: number;
  name: string;
  success: boolean;
  result?: unknown;
  error?: string;
}

export interface ConfirmationResult {
  messageId: string;
  confirmed: boolean | null;
  revised: boolean;
  notes: string | null;
  content?: string;
}

/** A full posted message object (carina / host) — shape owned by the writer. */
export type PostedMessage = Record<string, unknown> & { id: string };

/**
 * The scope-tagged event envelope (D3). One global stream carries every event;
 * the scope ids say what the payload is about. The chat payload flattens into
 * the envelope, so a chat frame arrives as `{ chatId, ...ChatStreamFrame }`.
 */
export type ScopedEvent = {
  chatId?: string;
  roomId?: string;
  progressId?: string;
} & ChatStreamFrame &
  CreationProgressFrame;

/** One resolved garment in a Green-Room slot preview (Rust `OutfitPreviewEntry`). */
export interface OutfitPreviewEntry {
  id: string;
  title: string;
  isComposite: boolean;
}

/** The decided four-slot outfit rendered read-only in the Green Room (Rust `OutfitPreviewSlots`). */
export interface OutfitPreviewSlots {
  top: OutfitPreviewEntry[];
  bottom: OutfitPreviewEntry[];
  footwear: OutfitPreviewEntry[];
  accessories: OutfitPreviewEntry[];
}

/**
 * Creation-progress frame fields (D6 — the Green Room), re-pinned P4.6q FROM
 * `crates/quilltap-core/src/services/creation_progress.rs`
 * (`CreationProgressFrame`). The Rust side is a `kind`-tagged kebab-case union;
 * its fields fold FLAT into the {@link ScopedEvent} envelope (like
 * {@link ChatStreamFrame}), so a frame arrives as `{ progressId, kind, ... }`.
 * The Green-Room reducer narrows on `kind`. `level` rides only `log` frames;
 * `characterId`/`characterName`/`slots` ride the two `wardrobe-*` frames.
 */
export interface CreationProgressFrame {
  kind?: 'status' | 'log' | 'wardrobe-start' | 'wardrobe-result' | 'done' | 'error';
  message?: string;
  level?: 'info' | 'warn' | 'error';
  characterId?: string;
  characterName?: string;
  slots?: OutfitPreviewSlots;
  ts?: number;
}

// ===========================================================================
// Listing-surface appendix (P4.6p/q/r) — BINDING, BYTE-IDENTICAL in lanes B & C
// ---------------------------------------------------------------------------
// Roleplay templates, image profiles, and global mount points. Lane A (p4.6p)
// owns the server variants + differentials; the SPA lanes append this identical
// block so each worktree compiles (the unifier keeps ONE copy). Request `type`
// names + DTO shapes are pinned by the p4.6p Shared contract and the live Rust
// DTOs (`db/roleplay_templates.rs`, `db/image_profiles.rs`,
// `db/doc_mount_points.rs`). Reads go through `CoreClient.dispatchData` (raw
// `data`), so only the request `type` strings are load-bearing on this side.
// ===========================================================================

// --- Roleplay templates ----------------------------------------------------

/** Styling add-ons on a template delimiter (Rust `DelimiterAddOns`). */
export interface TemplateDelimiterAddOns {
  bold: boolean;
  italic: boolean;
  reverse: boolean;
  underline: string;
  border: string;
  font: string;
}

/**
 * One roleplay-template delimiter (Rust `TemplateDelimiter`, kind-discriminated:
 * `wrap` | `linePrefix` | `tagPrefix`). `narrationDelimiters` on the template
 * itself is a `StringOrPair`; these are the styled inline/line markers.
 */
export type TemplateDelimiter =
  | {
      kind: 'wrap';
      name: string;
      buttonName: string;
      style: string;
      hideDelimiter?: boolean;
      addOns?: TemplateDelimiterAddOns;
      /** `open…close` around an inline span — a string or an `[open, close]` pair. */
      delimiters: string | [string, string];
    }
  | {
      kind: 'linePrefix';
      name: string;
      buttonName: string;
      style: string;
      hideDelimiter?: boolean;
      addOns?: TemplateDelimiterAddOns;
      marker: string;
    }
  | {
      kind: 'tagPrefix';
      name: string;
      buttonName: string;
      style: string;
      hideDelimiter?: boolean;
      addOns?: TemplateDelimiterAddOns;
      open: string;
      close: string;
      tokenPattern?: string;
    };

/** One read-time rendering pattern (Rust `RenderingPattern`, auto-gen, non-persisted). */
export interface RenderingPattern {
  pattern: string;
  className: string;
  flags?: string;
  scope?: string;
  hideDelimiters?: boolean;
}

/** Dialogue-detection config on a roleplay template (Rust `DialogueDetection`). */
export interface DialogueDetection {
  openingChars: string[];
  closingChars: string[];
  className: string;
}

/** The narration delimiter: a single string, or an `[open, close]` pair. */
export type NarrationDelimiters = string | [string, string];

/** One roleplay template (v4 `RoleplayTemplateDto`; `userId: null` = built-in). */
export interface RoleplayTemplateDto {
  id: string;
  userId: string | null;
  name: string;
  /** Omitted on route reads when null; the create/update echoes carry `null`. */
  description?: string | null;
  systemPrompt: string;
  isBuiltIn: boolean;
  tags: string[];
  delimiters: TemplateDelimiter[];
  renderingPatterns: RenderingPattern[];
  /** Omitted on route reads when null; the create/update echoes carry `null`. */
  dialogueDetection?: DialogueDetection | null;
  narrationDelimiters: NarrationDelimiters;
  createdAt: string;
  updatedAt: string;
}

/** The create bag (v4 `createRoleplayTemplateSchema`). */
export interface RoleplayTemplateCreateBag {
  name: string;
  description?: string | null;
  systemPrompt: string;
  narrationDelimiters: NarrationDelimiters;
  delimiters?: TemplateDelimiter[];
  renderingPatterns?: RenderingPattern[];
  dialogueDetection?: DialogueDetection | null;
}

/** The update bag (v4 `updateRoleplayTemplateSchema`, all-optional). */
export interface RoleplayTemplateUpdateBag {
  name?: string;
  description?: string | null;
  systemPrompt?: string;
  tags?: string[];
  delimiters?: TemplateDelimiter[];
  renderingPatterns?: RenderingPattern[];
  dialogueDetection?: DialogueDetection | null;
  narrationDelimiters?: NarrationDelimiters;
}

export interface RoleplayTemplateListRequest {
  type: 'roleplayTemplateList';
}
export interface RoleplayTemplateCreateRequest {
  type: 'roleplayTemplateCreate';
  template: RoleplayTemplateCreateBag;
}
export interface RoleplayTemplateGetRequest {
  type: 'roleplayTemplateGet';
  templateId: string;
}
export interface RoleplayTemplateUpdateRequest {
  type: 'roleplayTemplateUpdate';
  templateId: string;
  template: RoleplayTemplateUpdateBag;
}
export interface RoleplayTemplateDeleteRequest {
  type: 'roleplayTemplateDelete';
  templateId: string;
}

// --- Image profiles ---------------------------------------------------------

/** The API-key summary attached to an image profile (v4 `apiKey` enrichment). */
export interface ImageProfileApiKeyRef {
  id: string;
  label: string;
  provider: string;
  isActive: boolean;
}

/** An enriched image-profile tag (v4 get/list `tags` → `[{tagId, tag}]`). */
export interface ImageProfileTagRef {
  tagId: string;
  tag: string;
}

/**
 * One image-generation profile (v4 `ImageProfileDto` + `apiKey` summary). The
 * get/list reads enrich `tags` to {@link ImageProfileTagRef}; the
 * `sortByCharacter` list additionally carries `matchingTags` + `matchingTagCount`.
 */
export interface ImageProfileDto {
  id: string;
  userId: string;
  name: string;
  provider: string;
  apiKeyId?: string | null;
  baseUrl?: string | null;
  modelName: string;
  parameters: Record<string, unknown>;
  isDefault: boolean;
  isDangerousCompatible: boolean;
  tags: ImageProfileTagRef[];
  createdAt: string;
  updatedAt: string;
  apiKey: ImageProfileApiKeyRef | null;
  matchingTags?: ImageProfileTagRef[];
  matchingTagCount?: number;
}

/** One image provider descriptor (v4 `imageProviderList`). */
export interface ImageProviderInfo {
  value: string;
  label: string;
  defaultModels: string[];
  apiKeyProvider: string;
  legacyNames: string[];
}

/** The create bag (v4's hand-validated create body; `apiKeyId || null` /
 * `baseUrl || null` route coercions make explicit `null` ≡ absent). */
export interface ImageProfileCreateBag {
  name: string;
  provider: string;
  apiKeyId?: string | null;
  baseUrl?: string | null;
  modelName: string;
  parameters?: Record<string, unknown>;
  isDefault?: boolean;
  isDangerousCompatible?: boolean;
}

/** The update bag (v4 `updateImageProfileSchema`; explicit `apiKeyId: null` clears). */
export interface ImageProfileUpdateBag {
  name?: string;
  provider?: string;
  apiKeyId?: string | null;
  baseUrl?: string | null;
  modelName?: string;
  parameters?: Record<string, unknown>;
  isDefault?: boolean;
  isDangerousCompatible?: boolean;
}

export interface ImageProfileListRequest {
  type: 'imageProfileList';
  /** Bubble a character's matching profiles to the top (v4 `?sortByCharacter=`). */
  sortByCharacter?: string;
  /** Sent by v4's picker but READ BY NEITHER SERVER (v4 route.ts reads only
   * `sortByCharacter`; the Rust variant ignores unknown fields) — kept for
   * wire parity with v4's client. */
  sortByUserCharacter?: string;
}
export interface ImageProfileCreateRequest {
  type: 'imageProfileCreate';
  profile: ImageProfileCreateBag;
}
export interface ImageProfileGetRequest {
  type: 'imageProfileGet';
  profileId: string;
}
export interface ImageProfileUpdateRequest {
  type: 'imageProfileUpdate';
  profileId: string;
  profile: ImageProfileUpdateBag;
}
export interface ImageProfileDeleteRequest {
  type: 'imageProfileDelete';
  profileId: string;
}
export interface ImageProviderListRequest {
  type: 'imageProviderList';
}
/** Refusal-armed image-profile action verbs (loud; shapes mirror the Rust variants). */
export interface ImageProfileGenerateRequest {
  type: 'imageProfileGenerate';
  profileId: string;
  /** v4 `generateImageSchema` body (prompt, count, size, …) — opaque until live. */
  payload?: Record<string, unknown>;
}
export interface ImageProfileValidateKeyRequest {
  type: 'imageProfileValidateKey';
  /** v4 `?action=validate-key` body — opaque until live. */
  payload?: Record<string, unknown>;
}
export interface ImageProfileListModelsRequest {
  type: 'imageProfileListModels';
  provider?: string;
  apiKeyId?: string;
}

// --- Global mount points ----------------------------------------------------

/** Per-mount capability flags (v4 mount-point GET `capabilities`). */
export interface MountPointCapabilities {
  canWrite: boolean;
  canDelete: boolean;
  canCreateFolder: boolean;
  canMoveIn: boolean;
  canMoveOut: boolean;
  canConvert: boolean;
}

/**
 * One global document mount point (v4 `DocMountPointSchema` + `embeddedChunkCount`;
 * the GET read adds {@link MountPointCapabilities}).
 */
export interface DocMountPointDto {
  id: string;
  userId: string;
  name: string;
  basePath: string;
  mountType: string;
  storeType: string;
  includePatterns: string[];
  excludePatterns: string[];
  enabled: boolean;
  lastScannedAt: string | null;
  scanStatus: string;
  lastScanError: string | null;
  conversionStatus: string;
  conversionError: string | null;
  fileCount: number;
  chunkCount: number;
  totalSizeBytes: number;
  createdAt: string;
  updatedAt: string;
  embeddedChunkCount: number;
  capabilities?: MountPointCapabilities;
}

/** The create bag (v4 `createMountPointSchema`). */
export interface MountPointCreateBag {
  name: string;
  basePath?: string;
  mountType: string;
  storeType?: string;
  includePatterns?: string[];
  excludePatterns?: string[];
  enabled?: boolean;
}

/** The update bag (v4 `updateMountPointSchema`, PATCH semantics). */
export interface MountPointUpdateBag {
  name?: string;
  basePath?: string;
  mountType?: string;
  storeType?: string;
  includePatterns?: string[];
  excludePatterns?: string[];
  enabled?: boolean;
}

export interface MountPointListRequest {
  type: 'mountPointList';
}
export interface MountPointGetRequest {
  type: 'mountPointGet';
  mountPointId: string;
}
export interface MountPointCreateRequest {
  type: 'mountPointCreate';
  mountPoint: MountPointCreateBag;
}
export interface MountPointUpdateRequest {
  type: 'mountPointUpdate';
  mountPointId: string;
  mountPoint: MountPointUpdateBag;
}
export interface MountPointDeleteRequest {
  type: 'mountPointDelete';
  mountPointId: string;
}

/**
 * The listing-surface request variants (P4.6p/q/r). Folded into {@link CoreRequest}
 * via {@link ListingSurfaceRequest} — BINDING, byte-identical in lanes B & C.
 */
export type ListingSurfaceRequest =
  | RoleplayTemplateListRequest
  | RoleplayTemplateCreateRequest
  | RoleplayTemplateGetRequest
  | RoleplayTemplateUpdateRequest
  | RoleplayTemplateDeleteRequest
  | ImageProfileListRequest
  | ImageProfileCreateRequest
  | ImageProfileGetRequest
  | ImageProfileUpdateRequest
  | ImageProfileDeleteRequest
  | ImageProviderListRequest
  | ImageProfileGenerateRequest
  | ImageProfileValidateKeyRequest
  | ImageProfileListModelsRequest
  | MountPointListRequest
  | MountPointGetRequest
  | MountPointCreateRequest
  | MountPointUpdateRequest
  | MountPointDeleteRequest;
