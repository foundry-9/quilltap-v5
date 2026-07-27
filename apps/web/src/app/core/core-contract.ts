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
  /**
   * The profile the character is handed back to when the operator stops
   * speaking for them (v4 `useImpersonation.ts:115-142`). Present in
   * `types.rs:286` but missing from this mirror until P4.9E3C, so
   * `SelectLLMProfileDialog`'s whole reason for existing was unreachable.
   */
  newConnectionProfileId?: string;
}
export interface ChatSetActiveSpeakerRequest {
  type: 'chatSetActiveSpeaker';
  chatId: string;
  participantId: string | null;
}

// ---------------------------------------------------------------------------
// The Post Office — the in-chat announcement + mail wire contract (§1, written
// verbatim and identically in the P4.9E2A and P4.9E2B work orders; only P4.9E2A
// may add the Rust half, and neither lane may deviate). Field names, casing and
// nesting are FROZEN.
//
// The Whisper dialog is deliberately NOT here: it posts to the ordinary chat-send
// spine with `targetParticipantIds`, which is already on main (v4
// `components/chat/WhisperDialog.tsx:39-47`).
// ---------------------------------------------------------------------------

/** v4 `STAFF_SENDER_ENUM` (`schemas.ts:180`) — exactly these ten, this spelling. */
export type StaffSenderWire =
  | 'lantern'
  | 'aurora'
  | 'librarian'
  | 'concierge'
  | 'prospero'
  | 'host'
  | 'commonplaceBook'
  | 'ariel'
  | 'suparna'
  | 'pascal';

/** v4's discriminated announcer union, tagged on `kind`. */
export type AnnouncerSenderWire =
  | { kind: 'staff'; staffId: StaffSenderWire }
  | { kind: 'character'; characterId: string }
  | { kind: 'custom'; displayName: string };

/**
 * v4 `POST /api/v1/chats/[id]?action=announcement` — post an ad-hoc
 * announcement bubble. 201 on success.
 */
export interface ChatAnnouncementPostRequest {
  type: 'chatAnnouncementPost';
  chatId: string;
  contentMarkdown: string;
  sender: AnnouncerSenderWire;
}

/**
 * v4 `POST /api/v1/chats/[id]?action=announcement-preview` — generate an
 * in-character rewrite. Persists nothing.
 */
export interface ChatAnnouncementPreviewRequest {
  type: 'chatAnnouncementPreview';
  chatId: string;
  seedMarkdown: string;
  characterId: string;
  connectionProfileId: string;
  systemPromptId?: string | null;
}

/**
 * v4 `POST /api/v1/chats/[id]?action=send-mail` — post a letter as one of the
 * operator's player-characters. 201 on success.
 */
export interface ChatSendMailRequest {
  type: 'chatSendMail';
  chatId: string;
  fromCharacterId: string;
  toCharacterId: string;
  bodyMarkdown: string;
  inReplyToPath?: string | null;
}

/**
 * v4 `GET /api/v1/chats/[id]?action=mailbox&characterId=…` — list a
 * player-character's `Mail/` letters, for the Compose Mail "In reply to"
 * dropdown. NOTE: v4 provisions the vault on this read path.
 */
export interface ChatMailboxListRequest {
  type: 'chatMailboxList';
  chatId: string;
  characterId: string;
}

// ---------------------------------------------------------------------------
// The chat cast + avatar overrides (§1, OWNER: lane P4.9E1A) and the two
// chat-admin verbs this lane consumes (§1, OWNER: lane P4.9E3A). Mirrored here
// NAME FOR NAME from those orders' §1 blocks; neither shape may be changed
// unilaterally — a change is a cross-lane escalation.
//
// **The three-valued fields are the load-bearing part.** Where the Rust side
// declares `Option<Option<T>>` (v4's Zod `.nullish()`), the wire has THREE
// states and the client must be deliberate at every call site:
//
//   - key ABSENT       → leave the stored value alone
//   - key present, null → clear the override
//   - key present, value → set it
//
// TypeScript spells that `field?: T | null`, and a client that always sends the
// key defeats the distinction the server went to the trouble of modelling (v4
// `helpers.ts:159-160,180` branch on `!== undefined`). The api layer in
// `chat/chat-cast.api.ts` builds these bodies by conditional spread for exactly
// that reason, and its spec asserts absent-vs-null field by field.
//
// Response bodies are NOT pinned by §1 — each server lane defines its own — so
// these ops read through `CoreClient.dispatchData` (raw `data`) rather than a
// narrowed response variant, the settings/characters-lane precedent.
// ---------------------------------------------------------------------------

/**
 * Add a character to a chat as a participant (v4 `POST …?action=add-participant`,
 * `AddCharacterDialog.tsx:212`).
 *
 * v4's `type: z.literal('CHARACTER')` is carried by the verb NAME, not a field —
 * a deliberate, recorded divergence (E1A tier-2 item 6): the wire has no dead
 * `type` key, so the client must not send one.
 */
export interface ChatAddParticipantRequest {
  type: 'chatAddParticipant';
  chatId: string;
  characterId: string;
  connectionProfileId?: string;
  /** Three-valued (`Option<Option<String>>`). */
  imageProfileId?: string | null;
  displayOrder?: number;
  hasHistoryAccess?: boolean;
  /** Three-valued (`Option<Option<String>>`). */
  joinScenario?: string | null;
  controlledBy?: 'llm' | 'user';
  /** Shaped by the ported `OutfitSelectionSchema` analog (the new-chat form's). */
  outfitSelection?: ChatCreateOutfitSelectionInput;
}

/**
 * Patch one participant (v4 `POST …?action=update-participant`). Four fields are
 * v4-`nullish` and therefore three-valued: `imageProfileId`,
 * `selectedSystemPromptId`, `joinScenario`, `talkativeness`.
 */
export interface ChatUpdateParticipantRequest {
  type: 'chatUpdateParticipant';
  chatId: string;
  participantId: string;
  connectionProfileId?: string;
  /** Three-valued. */
  imageProfileId?: string | null;
  /** Three-valued — an explicit `null` means "use the default prompt". */
  selectedSystemPromptId?: string | null;
  displayOrder?: number;
  isActive?: boolean;
  status?: ParticipantStatusWire;
  controlledBy?: 'llm' | 'user';
  hasHistoryAccess?: boolean;
  /** Three-valued. */
  joinScenario?: string | null;
  /** Three-valued — an explicit `null` clears the per-chat override (v4
   *  `schemas.ts:44-50`), and the value range is 0.1–1.0. */
  talkativeness?: number | null;
}

/** v4's four-state participation status (`schemas.ts:41`). */
export type ParticipantStatusWire = 'active' | 'silent' | 'absent' | 'removed';

/** Soft-remove a participant (v4 `POST …?action=remove-participant`). */
export interface ChatRemoveParticipantRequest {
  type: 'chatRemoveParticipant';
  chatId: string;
  participantId: string;
}

/** Recompile a participant's identity stack (v4 `POST …?action=rebuild-system-prompt`). */
export interface ChatRebuildSystemPromptRequest {
  type: 'chatRebuildSystemPrompt';
  chatId: string;
  participantId: string;
}

/** Read the per-chat avatar overrides (v4 `GET …?action=get-avatars`). */
export interface ChatGetAvatarsRequest {
  type: 'chatGetAvatars';
  chatId: string;
}

/** Pin an image as a character's avatar in this chat (v4 `POST …?action=set-avatar`). */
export interface ChatSetAvatarRequest {
  type: 'chatSetAvatar';
  chatId: string;
  characterId: string;
  imageId: string;
}

/** Clear a character's avatar override (v4 `POST …?action=remove-avatar`). */
export interface ChatRemoveAvatarRequest {
  type: 'chatRemoveAvatar';
  chatId: string;
  characterId: string;
}

/** Toggle per-chat avatar generation (v4 `POST …?action=toggle-avatar-generation`). */
export interface ChatToggleAvatarGenerationRequest {
  type: 'chatToggleAvatarGeneration';
  chatId: string;
}

/**
 * Manual RNG from the composer gutter (v4 `POST …?action=rng`).
 *
 * ⚠ **The request field is `kind`, not `type`.** v4's schema key is `type`, but
 * `type` is the v5 request union's own serde tag, so P4.9E3A's §1 renames it.
 * This rename is the contract (E3A §1 binding note), and it is only the REQUEST
 * key: the `arguments` bag the server echoes back into the persisted TOOL row
 * still carries v4's own `{type, rolls}`, which the client passes through opaque.
 */
export interface ChatRngRequest {
  type: 'chatRng';
  chatId: string;
  /** A die size (integer 2..=1000) or `'flip_coin'` / `'spin_the_bottle'`. */
  kind: number | 'flip_coin' | 'spin_the_bottle';
  /** 1..=100, server-defaulted to 1. */
  rolls?: number;
  /** Preview returns the roll WITHOUT writing a TOOL message. */
  preview?: boolean;
}

/**
 * Replace the chat's disabled-tool sets (v4 `POST …?action=update-tool-settings`).
 * Both arrays are whole-set replacements, not deltas.
 *
 * The consumer is `ChatToolSettingsModal` (`chat/tools/chat-tool-settings-modal.ts`),
 * which lists the inventory through {@link ToolsListRequest}.
 */
export interface ChatUpdateToolSettingsRequest {
  type: 'chatUpdateToolSettings';
  chatId: string;
  disabledTools: string[];
  disabledToolGroups: string[];
}

// ---------------------------------------------------------------------------
// The rest of the chat-admin family (§1, OWNER: lane P4.9E3A — landed
// 2026-07-26). Mirrored here NAME FOR NAME from `api/types.rs:2514-2610`; a
// shape change is a cross-lane escalation, never a unilateral edit.
//
// `chatRng` and `chatUpdateToolSettings` (above) were mirrored when P4.9E3A
// landed; these nine were not, because no screen could reach them yet.
// ---------------------------------------------------------------------------

/**
 * Regenerate a chat's title from its transcript (v4
 * `POST …?action=regenerate-title`).
 *
 * ⚠ **One cheap-LLM call per press, in production.** v4 has no button of this
 * name: the ONLY path to it is ticking "Use automatic naming" in
 * `ChatRenameModal` (`ChatRenameModal.tsx:52`), which is why v5's live verb was
 * unreachable until that dialog landed.
 */
export interface ChatRegenerateTitleRequest {
  type: 'chatRegenerateTitle';
  chatId: string;
}

/** Attach a tag (v4 `POST …?action=add-tag`). */
export interface ChatAddTagRequest {
  type: 'chatAddTag';
  chatId: string;
  tagId: string;
}

/** Detach a tag (v4 `POST …?action=remove-tag`). */
export interface ChatRemoveTagRequest {
  type: 'chatRemoveTag';
  chatId: string;
  tagId: string;
}

/**
 * Re-attribute many messages at once (v4 `POST …?action=bulk-reattribute`).
 *
 * `sourceParticipantId` is v4-`nullable`: an explicit `null` means "the messages
 * with no participant at all" — the operator's own turns — which is what
 * `BulkCharacterReplaceModal`'s `__UNASSIGNED__` sentinel lowers to
 * (`BulkCharacterReplaceModal.tsx:52,75`). It is NOT optional: the key is always
 * sent. `roleFilter` server-defaults to `"both"`.
 */
export interface ChatBulkReattributeRequest {
  type: 'chatBulkReattribute';
  chatId: string;
  sourceParticipantId: string | null;
  targetParticipantId: string;
  roleFilter?: 'ASSISTANT' | 'USER' | 'both';
}

/**
 * Fold another conversation's cast + summary into this chat (v4
 * `POST …?action=merge-conversation`). `chatId` is the merge TARGET.
 */
export interface ChatMergeConversationRequest {
  type: 'chatMergeConversation';
  chatId: string;
  sourceChatId: string;
  characterIds?: string[];
  /** The ported `OutfitSelectionSchema` analog — the new-chat form's shape. */
  outfitSelections?: ChatCreateOutfitSelectionInput[];
}

/**
 * Operator-initiated tool execution (v4 `POST …?action=run-tool`), the
 * `RunToolModal` verb. `toolName` is the tool's **id**, not its display name
 * (`RunToolModal.tsx:139`).
 */
export interface ChatRunToolRequest {
  type: 'chatRunTool';
  chatId: string;
  toolName: string;
  arguments?: Record<string, unknown>;
  characterId?: string;
  private?: boolean;
}

/**
 * Toggle agent mode for this chat (v4 `POST …?action=toggle-agent-mode`).
 *
 * The column is TRI-state and the variant carries all three arms (`Rust
 * Option<Option<bool>>`): key absent ⇒ leave alone, `null` ⇒ clear the per-chat
 * override back to the cascade, `true`/`false` ⇒ pin it. **v4's badge only ever
 * sends `true` or `false`** (`useChatControls.ts:369` computes
 * `agentModeEnabled === null || !agentModeEnabled`), so the ported badge sends
 * only those two arms — a client that sent `null` would be inventing a control
 * v4 does not have.
 */
export interface ChatToggleAgentModeRequest {
  type: 'chatToggleAgentMode';
  chatId: string;
  enabled?: boolean | null;
}

/** Reset and re-queue danger classification (v4 `POST …?action=reclassify-danger`). */
export interface ChatReclassifyDangerRequest {
  type: 'chatReclassifyDanger';
  chatId: string;
}

/** Queue a Scriptorium render with full re-embed (v4 `POST …?action=render-conversation`). */
export interface ChatRenderConversationRequest {
  type: 'chatRenderConversation';
  chatId: string;
}

// ---------------------------------------------------------------------------
// §1 — the chat-dialog wire surface (written verbatim and identically in the
// P4.9E3B and P4.9E3C work orders). P4.9E3B owns the Rust half; this lane
// consumes it. Field names, casing and nesting are FROZEN — a change is a
// cross-lane escalation.
//
// Response bodies are deliberately NOT pinned by §1: each is read through
// `CoreClient.dispatchData` against what E3B's differential proves.
//
// **Two REST edges are part of the same contract**, reached via `apiUrl()`
// rather than dispatch:
//   - `GET /api/v1/tools?chatId=…&includeSchemas=true`
//   - `GET /api/v1/chats/{chatId}?action=export` (byte download,
//     `application/x-ndjson`, `Content-Disposition: attachment`)
// ---------------------------------------------------------------------------

/** The tool inventory (v4 `GET /api/v1/tools`, `app/api/v1/tools/route.ts:429`). */
export interface ToolsListRequest {
  type: 'toolsList';
  chatId?: string;
  includeSchemas?: boolean;
}

/** Chat transcript export as SillyTavern JSONL (v4 `GET …/chats/{id}?action=export`). */
export interface ChatExportRequest {
  type: 'chatExport';
  chatId: string;
}

/** Per-character equipped-outfit summary (v4 `GET …/chats/{id}?action=outfit-summary`). */
export interface ChatOutfitSummaryRequest {
  type: 'chatOutfitSummary';
  chatId: string;
}

/**
 * v4's search-replace scope, a discriminated union
 * (`components/tools/search-replace/types.ts`).
 */
export type SearchReplaceScopeWire =
  | { type: 'chat'; chatId: string }
  | { type: 'character'; characterId: string };

/**
 * Search & replace dry-run (v4 `POST /api/v1/search-replace?action=preview`).
 *
 * ⚠ **Both include flags default TRUE** on the server (v4 prefaults) — an absent
 * key means "included", not "excluded". The client therefore always sends them
 * explicitly rather than relying on the omission meaning what it looks like.
 */
export interface SearchReplacePreviewRequest {
  type: 'searchReplacePreview';
  scope: SearchReplaceScopeWire;
  searchText: string;
  replaceText: string;
  includeMessages?: boolean;
  includeMemories?: boolean;
}

/** Search & replace execute (v4 `POST /api/v1/search-replace?action=execute`). */
export interface SearchReplaceExecuteRequest {
  type: 'searchReplaceExecute';
  scope: SearchReplaceScopeWire;
  searchText: string;
  replaceText: string;
  includeMessages?: boolean;
  includeMemories?: boolean;
}

/**
 * Re-attribute ONE message (v4 `POST /api/v1/messages/{id}?action=reattribute`).
 * Also deletes every memory sourced from the message (v4 `route.ts:342+`).
 */
export interface MessageReattributeRequest {
  type: 'messageReattribute';
  messageId: string;
  newParticipantId: string;
}

/**
 * Group document stores reachable from this chat's user-controlled cast
 * (v4 `GET …/chats/{id}?action=group-stores`, `actions/group-stores.ts:16`).
 *
 * ⚠ **Not part of the §1 freeze** — P4.9E3B added it in its tier-2 audit (item 9)
 * after this contract was written, on finding the picker's read missing. Mirrored
 * here at the round's unification so both sides agree. **It has no client consumer
 * yet:** its only caller would be `LibraryFilePickerModal`, DEFERRED BY NAME by
 * P4.9E3C (see `salon-conversation.ts` and the `m6` row).
 */
export interface ChatGroupStoresRequest {
  type: 'chatGroupStores';
  chatId: string;
}

// ---------------------------------------------------------------------------
// Pascal custom tools — the composer popup's wire contract (§4, OWNER: lane
// P4.6ay). The two verbs are the SPA's path to `GET`/`POST
// /api/v1/chats/{id}/custom-tools`; the response `type` string is lane-AY-owned,
// so the client reads these bodies structurally via `dispatchData` (the
// autonomous-room precedent) rather than pinning a response variant.
// ---------------------------------------------------------------------------

/** The roster GET (`handleList`), resolved FRESH on every popup open — no cache. */
export interface ChatCustomToolsListRequest {
  type: 'chatCustomToolsList';
  chatId: string;
}

/** Run one tool at the operator's behest (`POST ?action=run`). */
export interface ChatCustomToolRunRequest {
  type: 'chatCustomToolRun';
  chatId: string;
  /** The declaration name — what shadowing resolved on. */
  tool: string;
  /** Coerced to the declared types before dispatch (`coerceParamValues`). */
  parameters?: Record<string, number | string | boolean>;
  /** Whisper the run to the operator alone (targets the userId, not a participant). */
  private?: boolean;
  /** Whose perspective to resolve from — set only when a tool has variants. */
  asCharacterId?: string;
}

/**
 * One entry in the popup's roster (v4 custom-tools `route.ts:57` **plus
 * `mountPointId`**, the d68638b4 addition). Deliberately ODDS-FREE — the roll
 * spec and the outcome table are NEVER in the payload.
 */
export interface CustomToolListing {
  /** Identity — what the run call names. Never displayed. */
  name: string;
  /** What to display. Server-resolved (`displayTitle()`), so always present. */
  title: string;
  description: string;
  parameters: Record<string, CustomToolParameterSpec>;
  defaultVisibility: 'public' | 'whisper';
  sourceTier: string;
  /**
   * Whose fact sheet the run will consult, when that is worth saying out loud.
   *
   * Set on a per-character variant that shadows a broader definition — and, since
   * v4 `e8a49597` (mirrored by P4.D24), also on a single-variant row where the
   * server had to fall back to someone other than you: an all-LLM room, or a
   * gate your own character did not pass. **Absent means "runs as you"** — or
   * that there is only one character in the room and nothing to tell apart.
   */
  characterLabel?: string;
  /** Disambiguates a character-labeled variant on the run call. */
  asCharacterId: string;
  definitionPath: string;
  mountName: string;
  /** Which store holds the file — Pascal's Workbench (P4.6bb) edits it by this pair. */
  mountPointId: string;
}

/** A single declared parameter of a custom-tool definition (v4 `CustomToolParameterSpec`). */
export interface CustomToolParameterSpec {
  type: 'number' | 'integer' | 'string' | 'boolean';
  default: number | string | boolean;
  description?: string;
  min?: number;
  max?: number;
}

/** A definition file that failed load-time validation and stayed out of the roster. */
export interface CustomToolLoadError {
  definitionPath: string;
  mountPointId?: string;
  mountName: string;
  tier: string;
  reason: string;
}

/** The roster GET body. */
export interface CustomToolsRosterData {
  tools: CustomToolListing[];
  errors: CustomToolLoadError[];
  /** Tools dropped because the roster hit its cap — surfaced, never silent. */
  droppedForCap?: string[];
}

/**
 * The `?action=run` result summary (the toast's, not the transcript's) — v4
 * builds it inline at `chats/[id]/custom-tools/route.ts:441-447` and gives it no
 * name of its own.
 *
 * RENAMED by P4.6bb (was `CustomToolRunResult`): v4 DOES have a named
 * `CustomToolRunResult` — the FULL run record at
 * `lib/pascal/custom-tools.ts:431-451` — and the Workbench's `customToolPreview`
 * returns exactly that, so the §W1 wire mirror needs the authentic name. This
 * five-field summary is the route's own projection of it, so it takes the
 * qualified name and the real v4 type keeps the plain one.
 */
export interface CustomToolManualRunSummary {
  tool: string;
  value: number;
  state: 'success' | 'partial' | 'failure' | 'info';
  message: string;
  whispered: boolean;
}

/** The `?action=run` body: the posted Pascal message(s) + the run summary. */
export interface CustomToolRunData {
  messages: MessageDto[];
  result: CustomToolManualRunSummary;
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

/**
 * §A (the `c53510c7` state-cascade round) — the chat / group / general state
 * verbs, mirroring lane D10's `CoreRequest` variants name-for-name (the wire
 * contract test pins the tags). Set REPLACES wholesale; reset returns previous.
 *
 * The CHAT tier is the only one that surfaces the inherited cascade beneath its
 * own state; its GET body carries the merged `state` plus the per-tier slices
 * (see {@link ChatStateGetResult}). Group/general edit a single tier directly.
 */
export interface ChatStateGetRequest {
  type: 'chatStateGet';
  chatId: string;
}
export interface ChatStateSetRequest {
  type: 'chatStateSet';
  chatId: string;
  state: Record<string, unknown>;
}
export interface ChatStateResetRequest {
  type: 'chatStateReset';
  chatId: string;
}
export interface GroupStateGetRequest {
  type: 'groupStateGet';
  groupId: string;
}
export interface GroupStateSetRequest {
  type: 'groupStateSet';
  groupId: string;
  state: Record<string, unknown>;
}
export interface GroupStateResetRequest {
  type: 'groupStateReset';
  groupId: string;
}
export interface GeneralStateGetRequest {
  type: 'generalStateGet';
}
export interface GeneralStateSetRequest {
  type: 'generalStateSet';
  state: Record<string, unknown>;
}
export interface GeneralStateResetRequest {
  type: 'generalStateReset';
}

/**
 * How many groups apply to a chat's cascade. `single` merges that one group's
 * state; `ambiguous` (more than one) merges NONE — the UI directs the author to
 * each group's own page (v4 `GroupTier`).
 */
export interface GroupTier {
  status: 'none' | 'single' | 'ambiguous';
  candidates: Array<{ id: string; name: string }>;
  /** Present only when `status === 'single'`. */
  appliedGroupId?: string;
}

/**
 * The `chatStateGet` body — the merged `state` plus the per-tier slices the
 * inherited-layers note reads. `projectState`/`groupState`/`generalState` are
 * OMITTED by the server when their tier has zero keys; `projectId` is omitted
 * when the chat has no project. `state`/`chatState`/`groupTier` always present.
 */
export interface ChatStateGetResult {
  success: boolean;
  state: Record<string, unknown>;
  chatState: Record<string, unknown>;
  projectState?: Record<string, unknown>;
  groupState?: Record<string, unknown>;
  generalState?: Record<string, unknown>;
  groupTier: GroupTier;
  projectId?: string;
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

// ===========================================================================
// The courier + chat-images surface (P4.6ac; p4.6ab implements the server side)
// ---------------------------------------------------------------------------
// Lane B (this file's owner) authors these request variants; lane A pins their
// response envelopes with its route oracle. The SPA reads responses defensively
// (dispatch/dispatchData), so only the request `type`/param NAMES are binding
// here — the response shapes reconcile name-for-name at unification.
// ===========================================================================

/** The album kind (v4 SaveImageDialog `AlbumKind`) — drives the optgroup label. */
export type AlbumKind = 'character' | 'project' | 'document-store' | 'general';

/** Album option for the save-to-album picker (v4 `chatPhotoAlbums`). */
export interface AlbumOption {
  mountPointId: string;
  name: string;
  kind: AlbumKind;
  characterId?: string;
  participantId?: string;
  isUserCharacter?: boolean;
  isDefault?: boolean;
}

/** One chat file (v4 `chatFilesList` row). Read defensively; extra fields ok. */
export interface ChatFileDto {
  id: string;
  filename: string;
  filepath: string;
  mimeType: string;
  sizeBytes?: number;
  createdAt?: string;
}

/** The Courier paste-back: settle the pending manual turn (v4 `?action=resolve-external-turn`). */
export interface MessageResolveExternalTurnRequest {
  type: 'messageResolveExternalTurn';
  chatId: string;
  messageId: string;
  replyContent: string;
}
/** The Courier cancel (v4 `?action=cancel-external-turn`). */
export interface MessageCancelExternalTurnRequest {
  type: 'messageCancelExternalTurn';
  chatId: string;
  messageId: string;
}

/** Save an in-chat image into a store album (v4 `messageSaveImage`). */
export interface MessageSaveImageRequest {
  type: 'messageSaveImage';
  chatId: string;
  messageId: string;
  fileId: string;
  mountPointId: string;
  caption?: string;
}
/** List the save-to-album target albums for a chat (v4 `chatPhotoAlbums`). */
export interface ChatPhotoAlbumsRequest {
  type: 'chatPhotoAlbums';
  chatId: string;
}

/** Record a generated-image tool result on a chat (v4 `chatAddToolResult`). */
export interface ChatAddToolResultRequest {
  type: 'chatAddToolResult';
  chatId: string;
  tool: 'generate_image';
  initiatedBy: string;
  prompt: string;
  /** v4 sends `{id, filename}`; extra fields are tolerated. */
  images: { id: string; filename: string; filepath?: string; mimeType?: string }[];
}

/** List a chat's uploaded/generated files for the gallery (v4 `chatFilesList`). */
export interface ChatFilesListRequest {
  type: 'chatFilesList';
  chatId: string;
}
/** Permanently delete a chat file (v4 `/chat-files/[id]` DELETE). */
export interface ChatFileDeleteRequest {
  type: 'chatFileDelete';
  fileId: string;
}

// ===========================================================================
// The general files family (P4.6af — lane B owns this contract file; p4.6ae
// implements the server side). All JSON verbs ride `POST /api/dispatch`; the
// upload REST leg (`POST /api/v1/files?action=upload`) + the byte GETs are web
// routes. Response bodies are pinned by lane A's route differential, so the SPA
// reads these through {@link CoreClient.dispatchData} (raw `data`); only the
// request `type`/param names are load-bearing here (the settings/characters
// precedent). Names transcribed VERBATIM from the p4.6af Shared contract.
// ===========================================================================

/**
 * One row of `filesList` (v4 `serializeFileEntry`): BOTH `originalFilename` AND
 * `filename` (= originalFilename), `filepath` = `/api/v1/files/{id}`,
 * `folderPath` RESOLVED via `resolveEffectiveFolderPath`, `fileStatus` defaulted
 * `'ok'`. Read defensively — extra fields (`userId`/`width`/`height`) are
 * tolerated; `linkedTo`/`isPlainText` ride list/preview enrichment when present.
 */
export interface FileEntry {
  id: string;
  originalFilename: string;
  filename: string;
  mimeType: string;
  size: number;
  category: string;
  description: string | null;
  projectId: string | null;
  folderPath: string | null;
  filepath: string;
  fileStatus: string;
  createdAt: string;
  updatedAt: string;
  linkedTo?: string[];
  isPlainText?: boolean;
  [key: string]: unknown;
}

/** One folder row (v4 `/files/folders` list). */
export interface FolderEntry {
  id: string;
  path: string;
  name: string;
  parentFolderId: string | null;
  projectId: string | null;
  [key: string]: unknown;
}

/** The delete-associations envelope carried on the 400 `FILE_HAS_ASSOCIATIONS`. */
export interface FileAssociations {
  characters: { id: string; name: string; usage: string }[];
  messages: { chatId: string; chatName: string; messageId: string }[];
}

/** List files (v4 GET `/files?filter=general` / project `?action=list-files`). */
export interface FilesListRequest {
  type: 'filesList';
  projectId?: string;
  folderPath?: string;
  filter?: string;
}

/**
 * Move a legacy file (v4 PUT `/files/{id}`). At least one field required;
 * `projectId` absent keeps, explicit `null` clears (the tri-state — never
 * collapsed). Returns the managed shape (`filename` only, RAW `folderPath`).
 */
export interface FileMoveRequest {
  type: 'fileMove';
  fileId: string;
  folderPath?: string;
  filename?: string;
  projectId?: string | null;
}

/** Promote a file into a project (or back to general) (v4 `?action=promote`). */
export interface FilePromoteRequest {
  type: 'filePromote';
  fileId: string;
  targetProjectId?: string | null;
  folderPath?: string;
}

/**
 * Delete a legacy file (v4 DELETE `/files/{id}`). A bare delete may return the
 * 400 associations envelope (`FILE_HAS_ASSOCIATIONS`); `dissociate: true` then
 * forces it through, severing the links.
 */
export interface FileDeleteRequest {
  type: 'fileDelete';
  fileId: string;
  force?: boolean;
  dissociate?: boolean;
}

/** Batch-generate cached thumbnails (v4 `?action=generate-thumbnails`). */
export interface FilesGenerateThumbnailsRequest {
  type: 'filesGenerateThumbnails';
  fileIds: string[];
  size?: number;
}

/** Stale-file cleanup (v4 `?action=cleanup-stale`). */
export interface FilesCleanupStaleRequest {
  type: 'filesCleanupStale';
  dryRun?: boolean;
}

/**
 * Orphan cleanup (v4 `?action=cleanup-orphans`). The dry run sends only
 * `{dryRun: true}` (no `mode`) and returns the report; the wet run sends
 * `{mode, dryRun: false}` and moves/deletes — hence `mode` is optional.
 */
export interface FilesCleanupOrphansRequest {
  type: 'filesCleanupOrphans';
  mode?: 'move' | 'delete';
  dryRun?: boolean;
}

/** List folders (v4 GET `/files/folders[?projectId=]`, `path` localeCompare ASC). */
export interface FilesFoldersListRequest {
  type: 'filesFoldersList';
  projectId?: string;
}

/** Create a folder (v4 `/files/folders?action=create`; idempotent, parent chain). */
export interface FilesFolderCreateRequest {
  type: 'filesFolderCreate';
  path: string;
  projectId?: string | null;
}

/** Rename a folder (v4 `/files/folders?action=rename`). */
export interface FilesFolderRenameRequest {
  type: 'filesFolderRename';
  path: string;
  newName: string;
  projectId?: string | null;
}

/** Delete an empty folder (v4 `/files/folders?action=delete`; refuses non-empty). */
export interface FilesFolderDeleteRequest {
  type: 'filesFolderDelete';
  path: string;
  projectId?: string | null;
}

/** Filesystem reconciliation sync (v4 `?action=sync`) — STAYS refusal-armed. */
export interface FilesSyncRequest {
  type: 'filesSync';
}

/**
 * The core upload variant behind the REST leg (v4 `?action=upload`). The general
 * Files page has no upload affordance (v4 parity), so this rides only the
 * project/mount paths — carried here for shape completeness (lane A's REST leg).
 */
export interface FileUploadRequest {
  type: 'fileUpload';
  filename: string;
  contentType: string;
  /** base64-encoded bytes. */
  data: string;
  tags?: string[];
  projectId?: string;
  folderPath?: string;
}

/** The general-files request family (folded into {@link CoreRequest}). */
export type FilesFamilyRequest =
  | FilesListRequest
  | FileMoveRequest
  | FilePromoteRequest
  | FileDeleteRequest
  | FilesGenerateThumbnailsRequest
  | FilesCleanupStaleRequest
  | FilesCleanupOrphansRequest
  | FilesFoldersListRequest
  | FilesFolderCreateRequest
  | FilesFolderRenameRequest
  | FilesFolderDeleteRequest
  | FilesSyncRequest
  | FileUploadRequest;

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
  // --- The Post Office, the in-chat announcement + mail surface (§1; P4.9E2A
  //     implements the server side) ---
  // --- The chat cast + avatar overrides (§1; P4.9E1A owns the server half) ---
  | ChatAddParticipantRequest
  | ChatUpdateParticipantRequest
  | ChatRemoveParticipantRequest
  | ChatRebuildSystemPromptRequest
  | ChatGetAvatarsRequest
  | ChatSetAvatarRequest
  | ChatRemoveAvatarRequest
  | ChatToggleAvatarGenerationRequest
  // --- The chat-admin family (§1; P4.9E3A's server half) ---
  | ChatRngRequest
  | ChatUpdateToolSettingsRequest
  | ChatRegenerateTitleRequest
  | ChatAddTagRequest
  | ChatRemoveTagRequest
  | ChatBulkReattributeRequest
  | ChatMergeConversationRequest
  | ChatRunToolRequest
  | ChatToggleAgentModeRequest
  | ChatReclassifyDangerRequest
  | ChatRenderConversationRequest
  // --- The chat-dialog surface (§1; P4.9E3B's server half) ---
  | ToolsListRequest
  | ChatExportRequest
  | ChatOutfitSummaryRequest
  | SearchReplacePreviewRequest
  | SearchReplaceExecuteRequest
  | MessageReattributeRequest
  | ChatGroupStoresRequest
  | ChatAnnouncementPostRequest
  | ChatAnnouncementPreviewRequest
  | ChatSendMailRequest
  | ChatMailboxListRequest
  // --- Pascal custom tools, the composer popup's path (P4.6ay implements the
  //     server side; the two REST legs are GET/POST /chats/{id}/custom-tools) ---
  | ChatCustomToolsListRequest
  | ChatCustomToolRunRequest
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
  | ChatStateGetRequest
  | ChatStateSetRequest
  | ChatStateResetRequest
  | GroupStateGetRequest
  | GroupStateSetRequest
  | GroupStateResetRequest
  | GeneralStateGetRequest
  | GeneralStateSetRequest
  | GeneralStateResetRequest
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
  | ListingSurfaceRequest
  // --- The memory surface (P4.6t; p4.6s implements the server side) ---
  | MemoryRequest
  // --- The courier + chat-images surface (P4.6ac; p4.6ab server side) ---
  | MessageResolveExternalTurnRequest
  | MessageCancelExternalTurnRequest
  | MessageSaveImageRequest
  | ChatPhotoAlbumsRequest
  | ChatAddToolResultRequest
  | ChatFilesListRequest
  | ChatFileDeleteRequest
  // --- The general files family (P4.6af; p4.6ae server side) ---
  | FilesFamilyRequest
  // --- Autonomous rooms (P4.6ad — lane C's own delimited block) ---
  | AutonomousRoomRequest;

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

/**
 * A referenced attachment on a pending courier turn (v4 CourierBubble's
 * `message.pendingExternalAttachments`): the file to download and re-upload into
 * the destination LLM client. Emitted verbatim by `api::salon::project_message`.
 */
export interface PendingExternalAttachment {
  fileId: string;
  filename: string;
  mimeType: string;
  sizeBytes: number;
  downloadUrl: string;
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
  | 'pascal'
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
 * The authoritative account of one Pascal custom-tool roll (v4
 * `lib/schemas/chat.types.ts:342-372`, transcribed field-for-field). NULL on every
 * non-Pascal message; the SPA reads it for the outcome row's header chip
 * (`toolTitle ?? tool`) and — someday — the Workbench (P4.6bb). `.nullable()
 * .optional()` in v4, so it's optional here (the read omits it when NULL, v4
 * `undefined`).
 */
/**
 * §A — the consult record that rides `pascalMeta.llm` and a bench run's
 * result. Shape and key order mirror v4 `lib/schemas/chat.types.ts:378-385`.
 */
export interface LlmConsultRecord {
  ok: boolean;
  output: string;
  prompt: string;
  reason?: string;
  provider?: string;
  model?: string;
}

export interface PascalMeta {
  /** Identity — the declaration name. Always present. */
  tool: string;
  /** The tool's display title at the moment it ran (`displayTitle()`). Absent on
   *  rows posted before this field existed — the UI falls back to `tool`. */
  toolTitle?: string;
  definitionTier: 'character' | 'participant' | 'group' | 'project' | 'global';
  definitionMountId: string;
  params: Record<string, number | string | boolean>;
  /** Which roll shape ran: uniform range, or dice `notation` + `diceRolls`. */
  rollForm: 'range' | 'dice';
  notation?: string;
  /** The untransformed roll. */
  raw: number;
  diceRolls?: number[];
  /** What the outcome table was tested against. */
  value: number;
  state: 'success' | 'partial' | 'failure' | 'info';
  outcomeIndex: number;
  /** The metadata keys the winning outcome consulted (primitives only); absent
   *  when it tested none. */
  metadataTested?: Record<string, number | string | boolean>;
  /**
   * §A — the LLM consult, when the definition declared one: what was asked
   * (`prompt`, fully rendered), whether it was answered (`ok`), and the
   * `output` the table tested — the model's trimmed answer, or the author's
   * `errorMessage` when the consult failed. `reason` is the technical failure
   * cause, recorded for the operator and NEVER spoken in the fiction. Absent
   * on tools with no `llm` block; the optionals are OMITTED when absent, never
   * null.
   *
   * Type-only this round: v4 renders nothing new in the Salon bubble either,
   * so the consult surfaces through the Inspector alone (verified parity).
   */
  llm?: LlmConsultRecord;
  /** A model's reach for the tool vs a user's own Run-Tool. */
  invokedBy: 'llm' | 'user';
  callerParticipantId?: string;
}

/**
 * One persisted message (v4 `handleGet` per-message projection) — MINUS
 * `renderedHtml` (v5 renders markdown client-side; see the rendering service).
 */
export interface MessageDto {
  id: string;
  /**
   * `TOOL` is a real persisted role (v4 `saveToolMessages` — one `role:'TOOL'`
   * row per tool result) that the salon read projects verbatim; the Salon renders
   * it through `qt-tool-message` (P4.17), not the normal message row.
   */
  role: 'USER' | 'ASSISTANT' | 'SYSTEM' | 'TOOL';
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
  /** The Pascal roll record (v4 `.nullable().optional()`); the read omits it when
   *  NULL, so it's typed optional here rather than `| null` like `carinaMeta`. */
  pascalMeta?: PascalMeta | null;
  pendingExternalPrompt: string | null;
  pendingExternalPromptFull: string | null;
  pendingExternalAttachments: PendingExternalAttachment[] | null;
  reasoningContent: string | null;
  reasoningSegments: MessageReasoningSegment[] | null;
  confirmed?: boolean;
  confirmationChecked?: boolean;
  confirmationRevised?: boolean;
  confirmationNotes?: string | null;
  confirmationOriginalContent?: string | null;
  /**
   * Character-initiated `role:'TOOL'` rows folded into this assistant message for
   * embedded rendering (v4 `app/salon/[id]/group-tool-messages.ts` —
   * `groupToolMessagesIntoAssistants`). A pure rendering annotation, never
   * persisted; present only on a shallow clone of a host assistant. See
   * `chat/group-tool-messages.ts`.
   */
  attachedToolMessages?: MessageDto[];
}

/** An enriched participant on the conversation detail (v4 `enrichParticipantDetail`). */
export interface ParticipantDetail {
  id: string;
  type: string;
  displayOrder: number;
  isActive: boolean;
  controlledBy: 'llm' | 'user';
  status: ParticipantStatusWire;
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
  /**
   * The character's NAMED system prompts (v4 `EnrichedCharacterSystemPrompt`,
   * `chat-enrichment.service.ts:296-300`). The server has always projected these
   * (`services/chat_enrichment.rs:246`); the type simply never declared them
   * because nothing consumed them until the participant card grew v4's
   * system-prompt select (P4.9E1B). Absent, not empty, on characters that have
   * none — v4's select renders only when the list is non-empty.
   */
  systemPrompts?: Array<{ id: string; name: string; isDefault?: boolean }>;
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
  /** The per-chat composition-mode flag (v4 `chats.documentEditingMode` —
   *  Enter inserts a newline, Cmd/Ctrl+Enter sends). Baked from
   *  `chat_settings.compositionModeDefault` at creation; the server always
   *  serializes it, but consumers default `?? false`. */
  documentEditingMode?: boolean;
  participants: ParticipantDetail[];
  user: { id: string; name: string; image: string | null };
  messages: MessageDto[];
  projectId: string | null;
  projectName: string | null;
  turnSkippingEnabled: boolean | null;
  /** The two whole-set disable lists (`api/salon.rs:384-387`). */
  disabledTools?: string[];
  disabledToolGroups?: string[];
  agentModeEnabled: boolean;
  resolvedAgentModeEnabled: boolean;
  agentModeSource: string;
  isDangerousChat: boolean | null;
  dangerCategories: string[];
  conciergeOverride: 'OFF' | null;
  /**
   * The chat sidebar's slice of the record (P4.9H1). The first three ARE in the
   * server's projection (`api/salon.rs:303-422`, v4 `handlers/get.ts:528-568`);
   * `timelineMode` and `alertCharactersOfLanternImages` are declared here for the
   * same reason v4 declares them on its `Chat` type (`app/salon/[id]/types.ts`) —
   * the client passes them to the controls — but **v4's route does not project
   * them**, so they arrive `undefined` and the sidebar keeps the operator's
   * in-session choice instead. See `chat/sidebar/chat-section.ts`.
   */
  imageProfileId?: string | null;
  allowCrossCharacterVaultReads?: boolean;
  coreWhisperEnabled?: boolean | null;
  coreWhisperInterval?: number | null;
  timelineMode?: 'realtime' | 'narrative' | null;
  alertCharactersOfLanternImages?: boolean | null;
  /** The per-chat avatar-generation switch (v4 `get.ts:558` DOES project it). */
  avatarGenerationEnabled?: boolean | null;
  offSceneCharacters: OffSceneCharacter[];
  lastTurnParticipantId: string | null;
  activeTypingParticipantId?: string | null;
  impersonatingParticipantIds?: string[];
  /**
   * The chat's blob mount point — the store whose `images/` folder backs
   * relative markdown image refs (v4 `MessageContent` `blobMountPointId`). v4
   * declares this plumbing but never populates it, so the rewrite arm stays
   * dormant until a producer emits it; the field name is binding (Shared
   * contract) so the client-side rewrite has a stable hook.
   */
  blobMountPointId?: string | null;
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
  /**
   * v4 `8bf3cb5f`: when true, a new chat with this character defaults its
   * Starting Outfit to "Let character choose". Vault `properties.json`-backed;
   * the response never sends null (default `false`), but the field stays
   * optional in TS so pre-server-lane responses read defensively (`?? false`).
   */
  canChooseOutfit?: boolean;
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
  /**
   * v4 `8bf3cb5f`: the vault `properties.json` "let this character choose their
   * opening outfit" flag. Response never sends null (default `false`); optional
   * in TS so pre-server-lane responses read defensively (`?? false`).
   */
  canChooseOutfit?: boolean;
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
  /** The composer text-replacement master switch (v4 default true). */
  textReplacementsEnabled?: boolean;
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
  // --- Autonomous rooms (P4.6ad) — one opaque body; the client casts per verb ---
  | { type: 'autonomousRoom'; data: Record<string, unknown> }
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

// ===========================================================================
// Autonomous rooms (P4.6ad — lane C's own delimited block; do not move)
// ===========================================================================

/** v4 room visibility (a per-room override of the user default). */
export type AutonomousRoomVisibility = 'owner_only' | 'household' | 'open';

/** One participant projection in an {@link AutonomousRoomSummary}. */
export interface AutonomousRoomParticipant {
  id: string;
  type: string;
  characterId: string | null;
  status: string;
}

/**
 * One row of `GET /system/autonomous-rooms` — the management-list DTO. Nullable
 * fields arrive `null` (the route coalesces `?? null`); the two counters and
 * `runPausedAccumMs` default `0`, `runDestructiveToolsAllowed` defaults `0`.
 */
export interface AutonomousRoomSummary {
  id: string;
  title: string;
  projectId: string | null;
  projectName: string | null;
  participants: AutonomousRoomParticipant[];
  runState: string | null;
  runStateMessage: string | null;
  currentRunId: string | null;
  runStartedAt: string | null;
  runEndedAt: string | null;
  runPausedAccumMs: number;
  runTurnsConsumed: number;
  runTokensConsumed: number;
  scheduleCron: string | null;
  scheduleNextRunAt: string | null;
  scheduleLastRunAt: string | null;
  scheduleFreshnessWindowMs: number | null;
  budgetMaxTurns: number | null;
  budgetMaxTokens: number | null;
  budgetMaxWallClockMs: number | null;
  budgetEstimatedSpendCapUSD: number | null;
  runDestructiveToolsAllowed: number;
  runVisibility: string | null;
  createdAt: string;
  updatedAt: string;
}

/** The per-room `GET …/autonomous-room` status snapshot (drives the editor modal). */
export interface AutonomousRoomStatusDto {
  chatId: string;
  chatType: string;
  runState: string | null;
  currentRunId: string | null;
  runStateMessage: string | null;
  runStartedAt: string | null;
  runEndedAt: string | null;
  runPausedAccumMs: number;
  runTurnsConsumed: number;
  runTokensConsumed: number;
  scheduleCron: string | null;
  scheduleNextRunAt: string | null;
  scheduleLastRunAt: string | null;
  scheduleFreshnessWindowMs: number | null;
  budgetMaxTurns: number | null;
  budgetMaxTokens: number | null;
  budgetMaxWallClockMs: number | null;
  budgetEstimatedSpendCapUSD: number | null;
  budgetExcludeCacheHits: number;
  runDestructiveToolsAllowed: number;
  runVisibility: string | null;
}

/**
 * The Edit-Enclave modal patch (v4 `updateSettingsSchema`): every cap is nullish
 * so the modal can CLEAR a value (`null`), leave it (omit the key), or set it.
 * `title` only sets; the two booleans are plain optional. Values are in DB units
 * (milliseconds); the modal converts hours/minutes → ms before sending.
 */
export interface AutonomousRoomSettingsPatch {
  title?: string;
  scheduleCron?: string | null;
  scheduleFreshnessWindowMs?: number | null;
  budgetMaxTurns?: number | null;
  budgetMaxTokens?: number | null;
  budgetMaxWallClockMs?: number | null;
  budgetEstimatedSpendCapUSD?: number | null;
  runVisibility?: AutonomousRoomVisibility | null;
  runDestructiveToolsAllowed?: boolean;
  budgetExcludeCacheHits?: boolean;
}

export interface SystemAutonomousRoomsRequest {
  type: 'systemAutonomousRooms';
}
export interface ChatAutonomousRoomStatusRequest {
  type: 'chatAutonomousRoomStatus';
  chatId: string;
}
export interface ChatAutonomousRoomStartRequest {
  type: 'chatAutonomousRoomStart';
  chatId: string;
}
export interface ChatAutonomousRoomPauseRequest {
  type: 'chatAutonomousRoomPause';
  chatId: string;
}
export interface ChatAutonomousRoomStopRequest {
  type: 'chatAutonomousRoomStop';
  chatId: string;
}
export interface ChatAutonomousRoomResumeRequest {
  type: 'chatAutonomousRoomResume';
  chatId: string;
}
export interface ChatAutonomousRoomUpdateSettingsRequest {
  type: 'chatAutonomousRoomUpdateSettings';
  chatId: string;
  settings: AutonomousRoomSettingsPatch;
}

/** The autonomous-rooms request family (folded into {@link CoreRequest}). */
export type AutonomousRoomRequest =
  | SystemAutonomousRoomsRequest
  | ChatAutonomousRoomStatusRequest
  | ChatAutonomousRoomStartRequest
  | ChatAutonomousRoomPauseRequest
  | ChatAutonomousRoomStopRequest
  | ChatAutonomousRoomResumeRequest
  | ChatAutonomousRoomUpdateSettingsRequest;

/** The `{updated, clampedDestructive}` body of an update-settings dispatch. */
export interface AutonomousRoomUpdateResult {
  updated: boolean;
  clampedDestructive: boolean;
}

// ===========================================================================

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
  /** A Pascal custom-tool outcome posted mid-turn (an LLM's `run_custom` reach) —
   *  the full posted Pascal message, surfaced live like `carinaAnswer` (§4). */
  pascalResult?: PostedMessage;

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
/**
 * Generate image(s) from a profile (v4 `?action=generate`). Lane A (P4.6ab)
 * un-refuses this variant; the Shared contract pins the params
 * `{imageProfileId, prompt, chatId?, count?}` and the response
 * `{success, data: [{id, filename, filepath, mimeType}], expandedPrompt}` (read
 * defensively). While the variant is still refusal-armed in a worktree the
 * dialog degrades loudly on the `not_available` envelope.
 */
export interface ImageProfileGenerateRequest {
  type: 'imageProfileGenerate';
  imageProfileId: string;
  prompt: string;
  chatId?: string;
  count?: number;
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

// ===========================================================================
// Memory surface appendix (P4.6t — the Commonplace Book) — LANE B AUTHORS THIS
// ---------------------------------------------------------------------------
// The memory (Commonplace Book) Request variants + read DTOs for the
// per-character Memories tab and the Settings → Memory tab. Lane A (p4.6s) owns
// the server variants + differentials over the NEW `memories-{main,mount}.db`
// fixture; the Shared contract pins the variant `type` names + param names at
// name level, and lane A's oracle pins the exact response envelopes/nullability
// (expect the P4.6p pattern — route reads may omit null nullables while
// create/update echo them AS null). Reads go through `CoreClient.dispatchData`
// (raw `data`), so only the request `type` strings + param names are
// load-bearing on this side; response bodies are read defensively and reconciled
// name-for-name at unification.
// ====================================================================
/** A derived tag row on a memory read (list/get/search enrich `tags` → rows). */
export interface MemoryTagDetail {
  id: string;
  name: string;
  /** Optional presentational color — NOT relied upon (unpinned by lane A). */
  color?: string | null;
}

/**
 * One Commonplace Book memory (v4 `MemoryDto` + derived `tagDetails`). Whether
 * `embedding` rides the JSON echo is pinned by lane A's oracle — this side must
 * NOT rely on it.
 */
export interface MemoryDto {
  id: string;
  userId: string;
  characterId: string;
  content: string;
  summary: string;
  keywords: string[];
  tags: string[];
  importance: number;
  source: 'AUTO' | 'MANUAL';
  chatId: string | null;
  projectId: string | null;
  aboutCharacterId: string | null;
  sourceMessageId: string | null;
  witnessedContext: string | null;
  lastAccessedAt: string | null;
  reinforcementCount: number;
  lastReinforcedAt: string | null;
  relatedMemoryIds: string[];
  reinforcedImportance: number;
  createdAt: string;
  updatedAt: string;
  /** Derived tag rows on list/get/search reads (absent on trimmed shapes). */
  tagDetails?: MemoryTagDetail[];
}

/** A search hit (v4 `memorySearch`): a memory plus its match score. */
export interface MemorySearchHit extends MemoryDto {
  score: number;
  usedEmbedding: boolean;
}

/**
 * The create/edit payload the editor sends (v4 `memory-editor` always sends
 * `source:'MANUAL'`; it does NOT send/edit `tags` or `aboutCharacterId`).
 */
export interface MemoryWriteBag {
  characterId: string;
  content: string;
  summary: string;
  keywords: string[];
  importance: number;
  source: 'MANUAL';
}

// --- Memory list / read / write --------------------------------------------

export type MemorySortBy = 'createdAt' | 'updatedAt' | 'importance';
export type MemorySortOrder = 'asc' | 'desc';

export interface MemoryListRequest {
  type: 'memoryList';
  characterId: string;
  search?: string;
  minImportance?: number;
  source?: 'AUTO' | 'MANUAL';
  sortBy?: MemorySortBy;
  sortOrder?: MemorySortOrder;
  /** Present = paginated; absent = legacy-all (v4's two code paths). */
  limit?: number;
  offset?: number;
}
export interface MemoryGetRequest {
  type: 'memoryGet';
  memoryId: string;
}
export interface MemoryCreateRequest {
  type: 'memoryCreate';
  memory: MemoryWriteBag;
}
export interface MemoryUpdateRequest {
  type: 'memoryUpdate';
  memoryId: string;
  memory: MemoryWriteBag;
}
export interface MemoryDeleteRequest {
  type: 'memoryDelete';
  memoryId: string;
}
export interface MemoryDeleteByChatRequest {
  type: 'memoryDeleteByChat';
  chatId: string;
}
export interface MemorySearchRequest {
  type: 'memorySearch';
  characterId: string;
  query: string;
  limit?: number;
  minImportance?: number;
  minScore?: number;
  source?: 'AUTO' | 'MANUAL';
}
export interface MemoryCountByChatRequest {
  type: 'memoryCountByChat';
  chatId: string;
}
export interface MemoryByMessageRequest {
  type: 'memoryByMessage';
  messageId: string;
}

// --- Housekeeping -----------------------------------------------------------

/** One line of a housekeeping preview/run detail (v4 `HousekeepingDetail`). */
export interface HousekeepingDetail {
  memoryId: string;
  action: 'deleted' | 'merged' | 'kept';
  reason: string;
  summary?: string;
}

/** The preview shape (v4 GET `action=housekeep` → `data.preview`). */
export interface HousekeepingPreview {
  wouldDelete: number;
  wouldMerge: number;
  wouldKeep: number;
  totalBefore: number;
  totalAfter: number;
  details: HousekeepingDetail[];
}

/** The run result (v4 POST `action=housekeep` → `data.result`; `details` only when dryRun). */
export interface HousekeepingResult {
  deleted: number;
  merged: number;
  kept: number;
  totalBefore: number;
  totalAfter: number;
  deletedIds: string[];
  mergedIds: string[];
  details?: HousekeepingDetail[];
}

/** The run options bag (v4 `housekeepingOptionsSchema`, incl. `dryRun`). */
export interface MemoryHousekeepOptions {
  characterId: string;
  maxMemories?: number;
  maxAgeMonths?: number;
  minImportance?: number;
  mergeSimilar?: boolean;
  dryRun?: boolean;
}

/** The instance-wide housekeeping config (v4 defaults injected on Get). */
export interface HousekeepingConfig {
  enabled: boolean;
  perCharacterCap: number;
  perCharacterCapOverrides: Record<string, number>;
  autoMergeSimilarThreshold: number;
  mergeSimilar: boolean;
}

/** One row of the character memory-count roster (memoryCount desc). */
export interface CharacterMemoryCount {
  id: string;
  name: string;
  memoryCount: number;
}

export interface MemoryHousekeepPreviewRequest {
  type: 'memoryHousekeepPreview';
  characterId: string;
  maxMemories?: number;
  maxAgeMonths?: number;
  minImportance?: number;
  mergeSimilar?: boolean;
}
export interface MemoryHousekeepRequest {
  type: 'memoryHousekeep';
  options: MemoryHousekeepOptions;
}
export interface MemoryHousekeepSweepRequest {
  type: 'memoryHousekeepSweep';
}
export interface MemoryHousekeepingConfigGetRequest {
  type: 'memoryHousekeepingConfigGet';
}
export interface MemoryHousekeepingConfigSetRequest {
  type: 'memoryHousekeepingConfigSet';
  /** Merge-patch: send only changed fields. */
  config: Partial<HousekeepingConfig>;
}
export interface MemoryCharacterCountsRequest {
  type: 'memoryCharacterCounts';
}

// --- Embeddings / backfill --------------------------------------------------

/** Per-character embedding coverage (v4 `memoryEmbeddingStatus`). */
export interface MemoryEmbeddingStatus {
  total: number;
  withEmbeddings: number;
  withoutEmbeddings: number;
  percentComplete: number;
  embeddingProfileConfigured: boolean;
  embeddingProfileName: string | null;
}

/** The instance-wide backfill progress (v4 `action=backfill-embeddings` GET). */
export interface BackfillProgress {
  remaining: number;
  inFlight: number;
}

export interface MemoryEmbeddingStatusRequest {
  type: 'memoryEmbeddingStatus';
  characterId: string;
}
export interface MemoryGenerateEmbeddingsRequest {
  type: 'memoryGenerateEmbeddings';
  characterId: string;
  batchSize?: number;
}
export interface MemoryRebuildIndexRequest {
  type: 'memoryRebuildIndex';
  characterId: string;
  confirm: true;
}
export interface MemoryBackfillProgressRequest {
  type: 'memoryBackfillProgress';
}
export interface MemoryBackfillStartRequest {
  type: 'memoryBackfillStart';
  characterId?: string;
  batchSize?: number;
}

// --- Configs (all merge-patch on Set) --------------------------------------

/** The cross-project recall policy (v4 `memoryRecall` instance setting). */
export interface RecallConfig {
  scopePolicy: 'down-weight' | 'exclude';
  expandRelated: boolean;
}

export interface MemoryExtractionLimitsGetRequest {
  type: 'memoryExtractionLimitsGet';
}
export interface MemoryExtractionLimitsSetRequest {
  type: 'memoryExtractionLimitsSet';
  config: Record<string, unknown>;
}
export interface MemoryRecallConfigGetRequest {
  type: 'memoryRecallConfigGet';
}
export interface MemoryRecallConfigSetRequest {
  type: 'memoryRecallConfigSet';
  config: Partial<RecallConfig>;
}
export interface MemoryExtractionConcurrencyGetRequest {
  type: 'memoryExtractionConcurrencyGet';
}
export interface MemoryExtractionConcurrencySetRequest {
  type: 'memoryExtractionConcurrencySet';
  concurrency: number;
}

// --- Regenerate + per-chat --------------------------------------------------

/** The regenerate-all in-flight breakdown (v4 `action=regenerate-all` GET). */
export interface RegenerateAllStatus {
  inFlightFanOut: number;
  inFlightWipes: number;
  inFlightExtractions: number;
  inFlight: number;
}

export interface MemoryRegenerateAllStatusRequest {
  type: 'memoryRegenerateAllStatus';
}
export interface MemoryRegenerateAllRequest {
  type: 'memoryRegenerateAll';
}
export interface ChatQueueMemoriesRequest {
  type: 'chatQueueMemories';
  chatId: string;
}

/**
 * The memory-surface request variants (P4.6t). Folded into {@link CoreRequest}
 * via {@link MemoryRequest}. Variant `type` names + param names are BINDING at
 * name level (identical in p4.6s / p4.6t / p4.6u); the SPA reads response bodies
 * through `dispatchData` and reconciles envelopes at unification.
 */
export type MemoryRequest =
  | MemoryListRequest
  | MemoryGetRequest
  | MemoryCreateRequest
  | MemoryUpdateRequest
  | MemoryDeleteRequest
  | MemoryDeleteByChatRequest
  | MemorySearchRequest
  | MemoryCountByChatRequest
  | MemoryByMessageRequest
  | MemoryHousekeepPreviewRequest
  | MemoryHousekeepRequest
  | MemoryHousekeepSweepRequest
  | MemoryHousekeepingConfigGetRequest
  | MemoryHousekeepingConfigSetRequest
  | MemoryCharacterCountsRequest
  | MemoryEmbeddingStatusRequest
  | MemoryGenerateEmbeddingsRequest
  | MemoryRebuildIndexRequest
  | MemoryBackfillProgressRequest
  | MemoryBackfillStartRequest
  | MemoryExtractionLimitsGetRequest
  | MemoryExtractionLimitsSetRequest
  | MemoryRecallConfigGetRequest
  | MemoryRecallConfigSetRequest
  | MemoryExtractionConcurrencyGetRequest
  | MemoryExtractionConcurrencySetRequest
  | MemoryRegenerateAllStatusRequest
  | MemoryRegenerateAllRequest
  | ChatQueueMemoriesRequest
  // === P4.6x (lane C): the Document Mode client surface ===
  // The chat-scoped document family (lane B's oracle) + the one lane-A variant
  // the SPA consumes (`mountFilesList`). Lane C is the SINGLE AUTHOR of this
  // block per the round contract; lanes A/B do not touch `apps/web/**`. Response
  // bodies are read DEFENSIVELY via `CoreClient.dispatchData` (their `type`
  // strings are lane B's to choose and are not load-bearing here — the P4.6t /
  // settings precedent), so no CoreResponse variants are added for them.
  | ChatActiveDocumentRequest
  | ChatOpenDocumentsRequest
  | ChatRecentDocumentsRequest
  | ChatAccessibleStoresRequest
  | ChatDocumentOpenRequest
  | ChatDocumentCloseRequest
  | ChatDocumentReadRequest
  | ChatDocumentResolveRequest
  | ChatDocumentWriteRequest
  | ChatDocumentRenameRequest
  | ChatDocumentDeleteRequest
  | MountFilesListRequest
  // === P4.6z (lane A): the Scriptorium mount-file actions + system browse ===
  | ScriptoriumRequest
  // === P4.d3 (lane D): the data-retention setting ===
  // Single-author block (lane D); the file owner (lane B) must not edit it.
  // Read defensively via `CoreClient.dispatchData` (the settings precedent — no
  // CoreResponse variant added). The interfaces + DTO live in the P4.d3 block
  // appended at the end of this file.
  | DataRetentionSettingsRequest
  | DataRetentionSettingsUpdateRequest
  // === P4.6ak∥al∥am unification: the text-replacements + story-background
  // verbs (lane A's Rust dispatch; the interfaces live in the P4.6al/P4.6am
  // blocks appended at the end of this file). Folded into the union by the
  // unifier per the round's §2 contract.
  | TextReplacementsListRequest
  | TextReplacementCreateRequest
  | TextReplacementUpdateRequest
  | TextReplacementDeleteRequest
  | TextReplacementsBulkReplaceRequest
  | ChatGetBackgroundRequest
  // P4.6ao/P4.6ap (folded at unification; the block at the end of this file).
  | ChatGetCostRequest
  | ChatRegenerateBackgroundRequest
  // P4.6ar/P4.6as/P4.6at (folded at unification; the block at the end of this
  // file).
  | LlmLogsListRequest
  | SystemImageAestheticsGetRequest
  | SystemImageAestheticsSetRequest
  // P4.6au/P4.6av (folded at unification; the block at the end of this file).
  | SystemHomeRequest
  // P4.6bb — Pascal's Workbench. §W1: the four verbs lane AY implements.
  // §W2: `mountFileRead`/`mountFileWrite` are MIRROR additions of verbs already
  // on main (the editor's file I/O), not new contract.
  | CustomToolsLibraryRequest
  | CustomToolsDestinationsRequest
  | CustomToolPreviewRequest
  | CustomToolAuditRequest
  | MountFileReadRequest
  | MountFileWriteRequest
  // P4.9c (folded at unification; the block at the end of this file).
  | UserProfileGetRequest
  | UserProfileUpdateRequest
  | UserProfileSetAvatarRequest
  | SystemDataDirRequest
  // P4.9a (folded at unification; the block at the end of this file).
  | PhotoGalleryListRequest
  | PhotoGallerySaveRequest
  | PhotoGalleryEntryGetRequest
  | PhotoGalleryEntryRemoveRequest
  // P4.9a2 (folded at the consult-wire round's unification).
  | ImageInfoGetRequest
  // P4.9f1 (folded at the consult-wire round's unification; §1 of that
  // round's Shared contract pins every name and payload below).
  | ChatOutfitGetRequest
  | ChatEquipRequest
  | ChatRegenerateAvatarRequest
  | WardrobeTransferDestinationsRequest
  | WardrobeTransferApplyRequest
  | WardrobeListRequest
  | WardrobeCreateRequest
  | WardrobeItemGetRequest
  | WardrobeUpdateRequest
  | WardrobeDeleteRequest
  | WardrobePreviewAvatarRequest
  // P4.9J4 (folded at the workspace-tabs remainder round's unification): the
  // standalone (chat-less) Document Mode surface — names pinned against
  // `api/types.rs` (the §W.3 wire diff).
  | DocumentStoresRequest
  | DocumentsRecentRequest
  | DocumentOpenRequest
  | DocumentReadRequest
  | DocumentWriteRequest
  | DocumentRenameRequest
  | DocumentDeleteRequest
  // P4.9I1B (folded at the same unification; §B of that round pins every name
  // and payload): the Brahma Console dispatch family.
  | BrahmaConsoleListRequest
  | BrahmaConsoleCreateRequest
  | BrahmaConsoleGetRequest
  | BrahmaConsoleRenameRequest
  | BrahmaConsoleSetModelRequest
  | BrahmaConsoleDeleteRequest
  | BrahmaConsoleMessagesRequest
  | BrahmaConsoleSendRequest
  // P4.9G2 — the Data & System tab's sixteen §1 verbs (P4.9G1 delivers them in
  // `api/types.rs`; this lane mirrors them name-for-name, the unifier diffs the
  // two at unification). The interfaces live in the P4.9G2 block appended at the
  // end of this file. Payloads are the best-guess derivation from v4's route
  // bodies (the G1 survey table is authoritative); the wire diff reconciles.
  | SystemBackupCreateRequest
  | SystemRestorePreviewRequest
  | SystemRestoreExecuteRequest
  | SystemExportEntitiesRequest
  | SystemExportPreviewRequest
  | SystemImportPreviewRequest
  | SystemImportExecuteRequest
  | SystemTasksQueueRequest
  | SystemTasksQueueControlRequest
  | SystemJobConcurrencyGetRequest
  | SystemJobConcurrencySetRequest
  | SystemJobGetRequest
  | SystemJobControlRequest
  | SystemJobDeleteRequest
  | SystemDeleteDataPreviewRequest
  | SystemDeleteDataRequest;
// P4.6u (lane C) — the Salon terminal-pane block.
// Appended by lane C; single-author (lane B, the file owner, must not edit this
// block). The terminal WebSocket + REST protocol types live in
// `app/terminal/terminal-protocol.ts` (pinned from the frozen Rust source
// `quilltap_host::terminal::protocol`, itself a byte-verbatim port of v4
// `lib/schemas/terminal.types.ts`) — they are NOT part of the dispatch envelope
// (the terminal surface is direct REST + WebSocket, not `POST /api/dispatch`).
//
// The pane state the `chatUpdate` bag round-trips (P4.6c full-bag port) and the
// enriched `chatGet` chat echoes back (server defaults: terminalMode "normal",
// dividerPosition 45, rightPaneVerticalSplit 50). `ChatUpdateRequest.chat` is a
// `Record<string, unknown>`, so these ride the WRITE bag without a signature
// change; the READ fields are added to `ChatDetail` via interface
// declaration-merging below (the original `ChatDetail` declaration stays
// untouched — lane B's surface).
// ===========================================================================

/** The Salon terminal-pane state persisted on the chat record (P4.6c round-trip). */
export interface ChatTerminalPaneState {
  terminalMode?: 'normal' | 'split' | 'focus' | null;
  activeTerminalSessionId?: string | null;
  rightPaneVerticalSplit?: number | null;
  dividerPosition?: number | null;
}

/** Merge the pane-state read fields onto `ChatDetail` (the server echoes them). */
export interface ChatDetail extends ChatTerminalPaneState {}

// ===========================================================================
// P4.6x (lane C) — the Document Mode client surface.
//
// Single-author block (lane C owns `apps/web/**`). The wire request `type`
// names mirror the Shared contract; the RESPONSE bodies are read defensively
// through `CoreClient.dispatchData` (see the union note above), so the
// interfaces below type only the fields this vertical reads — extra fields the
// server echoes are ignored, and a defensive extractor tolerates their absence.
//
// The standalone/workspace-tab document surface (`documentStores`,
// `documentsRecent`, `documentOpen`, and the standalone read/write/rename/
// delete variants) was a LOUD tier-3 deferral of that round; it landed with
// P4.9J4 (the workspace-tabs remainder round) — its request types are folded
// in below, after the chat-scoped family.
// ===========================================================================

/** A document's scope triple selector (v4 `DocEditScope`). */
export type DocumentScope = 'project' | 'document_store' | 'general';

/** Query the chat's single active (focused) document (v4 `?action=active-document`). */
export interface ChatActiveDocumentRequest {
  type: 'chatActiveDocument';
  chatId: string;
}

/** List every open document for the chat, oldest-first (v4 `?action=open-documents`). */
export interface ChatOpenDocumentsRequest {
  type: 'chatOpenDocuments';
  chatId: string;
}

/** Recently-opened documents (current-chat rows win the dedupe; server-owned order). */
export interface ChatRecentDocumentsRequest {
  type: 'chatRecentDocuments';
  chatId: string;
}

/** Document stores reachable from this chat; `all` widens to every enabled store. */
export interface ChatAccessibleStoresRequest {
  type: 'chatAccessibleStores';
  chatId: string;
  all?: boolean;
}

/** Open (or create-and-open) a document into the chat's open set. */
export interface ChatDocumentOpenRequest {
  type: 'chatDocumentOpen';
  chatId: string;
  /** Omit for a new blank document (the server mints an "Untitled Document.md"). */
  filePath?: string;
  title?: string;
  /** Default 'project' (server-side). */
  scope?: DocumentScope;
  mountPoint?: string;
  /** Default 'split'. */
  mode?: 'split' | 'focus';
  /** For new blank docs, the folder (relative to scope root) to create inside. */
  targetFolder?: string;
}

/** Close one open document; omit `chatDocumentId` to close the focused document. */
export interface ChatDocumentCloseRequest {
  type: 'chatDocumentClose';
  chatId: string;
  chatDocumentId?: string;
}

/** Read a document's bytes + mtime (never mutates). */
export interface ChatDocumentReadRequest {
  type: 'chatDocumentRead';
  chatId: string;
  filePath: string;
  scope?: DocumentScope;
  mountPoint?: string | null;
}

/** Existence probe for a `qtap://` target — returns `{ exists, kind }`, never bytes. */
export interface ChatDocumentResolveRequest {
  type: 'chatDocumentResolve';
  chatId: string;
  filePath: string;
  scope?: DocumentScope;
  mountPoint?: string | null;
}

/** Write a document; `mtime` guards against clobbering an out-of-band edit (409). */
export interface ChatDocumentWriteRequest {
  type: 'chatDocumentWrite';
  chatId: string;
  filePath: string;
  scope?: DocumentScope;
  mountPoint?: string | null;
  content: string;
  mtime?: number;
  /** Pre-formatted diff → the server posts a single Librarian save announcement. */
  diffContent?: string;
}

/** Rename the focused (or identified) document; the server relocates the file. */
export interface ChatDocumentRenameRequest {
  type: 'chatDocumentRename';
  chatId: string;
  newTitle: string;
  chatDocumentId?: string;
}

/** Delete the focused (or identified) document and its underlying file. */
export interface ChatDocumentDeleteRequest {
  type: 'chatDocumentDelete';
  chatId: string;
  chatDocumentId?: string;
}

/** List a mount point's files + folders (lane A; v4 `GET /mount-points/{id}/files`). */
export interface MountFilesListRequest {
  type: 'mountFilesList';
  mountPointId: string;
}

// ===========================================================================
// P4.9J4 (folded at the workspace-tabs remainder round's unification) — the
// standalone (chat-less) Document Mode request family. Names pinned against
// `api/types.rs` (`DocumentStores`…`DocumentDelete`); the field shapes mirror
// v4's standalone `app/api/v1/documents/route.ts` handlers, which the P4.6w
// differentials pin server-side.
// ===========================================================================

/** The standalone scope enum (v4 `openDocumentSchema` — NO `project`). */
export type StandaloneScope = 'document_store' | 'general';

/** v4 standalone `accessible-stores` — `{ stores, projectLibrary: null }`. */
export interface DocumentStoresRequest {
  type: 'documentStores';
}

/** v4 standalone `recent-documents` — `{ documents }` (project-scope filtered). */
export interface DocumentsRecentRequest {
  type: 'documentsRecent';
}

/** v4 standalone `open-document` — `{ document, content, mtime, isNew }`. */
export interface DocumentOpenRequest {
  type: 'documentOpen';
  /** Omit for a new blank document (the server mints an "Untitled Document.md"). */
  filePath?: string;
  title?: string;
  /** Default 'general' (server-side). */
  scope: StandaloneScope;
  mountPoint?: string;
  /** For new blank docs, the folder (relative to scope root) to create inside. */
  targetFolder?: string;
}

/** v4 standalone `read-document` — `{ content, mtime }`. */
export interface DocumentReadRequest {
  type: 'documentRead';
  filePath: string;
  scope: StandaloneScope;
  mountPoint?: string;
}

/** v4 standalone `write-document` — `{ success, mtime }` (mtime-checked → 409). */
export interface DocumentWriteRequest {
  type: 'documentWrite';
  filePath: string;
  scope: StandaloneScope;
  mountPoint?: string;
  content: string;
  mtime?: number;
}

/** v4 standalone `rename-document` — `{ document }`. */
export interface DocumentRenameRequest {
  type: 'documentRename';
  filePath: string;
  scope: StandaloneScope;
  mountPoint?: string;
  newTitle: string;
}

/** v4 standalone `delete-document` — `{ success }`. */
export interface DocumentDeleteRequest {
  type: 'documentDelete';
  filePath: string;
  scope: StandaloneScope;
  mountPoint?: string;
}

// ===========================================================================
// P4.9I1B (folded at the same unification) — the Brahma Console request
// family (§B of the workspace-tabs remainder round pins every name and
// payload; the P4.9I1A tier-2 differential is the arbiter of the response
// bodies, which the console reads defensively via its own DTOs in
// `brahma/brahma-wire.ts`).
// ===========================================================================

/** `GET /api/v1/brahma-console` — list the operator's Console chats. */
export interface BrahmaConsoleListRequest {
  type: 'brahmaConsoleList';
}

/**
 * `POST /api/v1/brahma-console` — create a Console chat. The dispatch field is
 * `consoleConnectionProfileId` (the DB column); omit it to start on the user's
 * default profile (v4 `route.ts:73-85`).
 */
export interface BrahmaConsoleCreateRequest {
  type: 'brahmaConsoleCreate';
  consoleConnectionProfileId?: string;
}

/** `GET /api/v1/brahma-console/{id}` — the chat detail record. */
export interface BrahmaConsoleGetRequest {
  type: 'brahmaConsoleGet';
  chatId: string;
}

/** `PATCH /api/v1/brahma-console/{id}` — rename (sets `isManuallyRenamed`). */
export interface BrahmaConsoleRenameRequest {
  type: 'brahmaConsoleRename';
  chatId: string;
  title: string;
}

/**
 * `PATCH /api/v1/brahma-console/{id}?action=set-model` — switch the model; the
 * same chat continues. The body field v4 reads is `connectionProfileId`
 * (`[id]/route.ts:100`, `setModelSchema`).
 */
export interface BrahmaConsoleSetModelRequest {
  type: 'brahmaConsoleSetModel';
  chatId: string;
  connectionProfileId: string;
}

/** `DELETE /api/v1/brahma-console/{id}`. */
export interface BrahmaConsoleDeleteRequest {
  type: 'brahmaConsoleDelete';
  chatId: string;
}

/** `GET /api/v1/brahma-console/{id}/messages` — the persisted transcript. */
export interface BrahmaConsoleMessagesRequest {
  type: 'brahmaConsoleMessages';
  chatId: string;
}

/**
 * `POST /api/v1/brahma-console/{id}/messages` — send one message. The dispatch
 * reply is the typed result of the run; the stream frames ride the global
 * Event channel scope-tagged by `chatId` (v4 `BrahmaConsoleSendOptions`).
 */
export interface BrahmaConsoleSendRequest {
  type: 'brahmaConsoleSend';
  chatId: string;
  content: string;
  fileIds?: string[];
}

// --- Response DTOs (read defensively; only the consumed fields are typed) ---

/** A `chat_documents` row as the document family returns it. */
export interface ActiveDocumentRecordDto {
  id: string;
  filePath: string;
  scope: DocumentScope;
  mountPoint?: string | null;
  displayTitle?: string | null;
}

/** The open/rename response envelope body (carries a Librarian announcement). */
export interface OpenDocumentResultDto {
  document: ActiveDocumentRecordDto;
  content?: string;
  mtime?: number;
  /** Posted alongside the open/save/rename/delete (null if the chat was gone). */
  librarianMessage?: MessageDto | null;
}

/** The write response body. */
export interface WriteDocumentResultDto {
  success?: boolean;
  mtime?: number;
  librarianMessage?: MessageDto | null;
}

/** A document store reachable from a chat (v4 `AccessibleStoreOption`). */
export interface AccessibleStoreDto {
  mountPointId: string;
  name: string;
  label: string;
  kind: 'character' | 'document-store';
  mountType: 'filesystem' | 'obsidian' | 'database';
  storeType: 'documents' | 'character';
  characterId?: string;
  /** True when reached via a group membership — bucketed into "Group Files". */
  isGroupStore?: boolean;
}

/** The project's official document store, when provisioned. */
export interface ProjectLibraryTargetDto {
  mountPointId: string;
  name: string;
  mountType: 'filesystem' | 'obsidian' | 'database';
}

/** A recent-document row (server-owned order; `fromCurrentChat` rows win). */
export interface RecentDocumentDto {
  id: string;
  filePath: string;
  scope: DocumentScope;
  mountPoint?: string | null;
  displayTitle?: string | null;
  isActive?: boolean;
  fromCurrentChat?: boolean;
  updatedAt?: string;
}

/**
 * A `doc_mount_files` row from `mountFilesList` (lane A pins the exact
 * serialization; typed permissively here as only these fields are consumed).
 */
export interface DocMountFileDto {
  id: string;
  relativePath: string;
  size?: number | null;
  mimeType?: string | null;
  updatedAt?: string | null;
}

/** The pane-state document read fields the server echoes onto the chat record. */
export interface ChatDocumentPaneState {
  documentMode?: 'normal' | 'split' | 'focus' | null;
}

/** Merge the document-mode read field onto `ChatDetail` (server echoes it). */
export interface ChatDetail extends ChatDocumentPaneState {}

// ===========================================================================
// P4.6z (lane A) — the Scriptorium client surface.
//
// Lane A is the SINGLE AUTHOR of core-contract this round. The mount-point CRUD
// requests + `mountFilesList` already exist above (P4.6p/r/x); this block adds
// the mount-file ACTION verbs the Scriptorium screens dispatch (scan / convert /
// deconvert / file-delete / file-describe / blob-list) and the one new server
// variant, `systemBrowseDirectory` (the DirectoryPicker's host-fs browser).
//
// The action responses ride the server's `mountPoint` / `mountFile` /
// `system` envelopes and are read DEFENSIVELY via `CoreClient.dispatchData`
// (their `type` strings are not load-bearing here — the P4.6x precedent), so no
// `CoreResponse` variants are added.
// ===========================================================================

/** v4 `POST …?action=scan` — the synchronous scan + post-scan embedding enqueue. */
export interface MountScanRequest {
  type: 'mountScan';
  mountPointId: string;
}

/** v4 `POST …?action=convert` (fs→database). REFUSAL-ARMED live (P4.6y). */
export interface MountConvertRequest {
  type: 'mountConvert';
  mountPointId: string;
}

/** v4 `POST …?action=deconvert` (database→fs, to `targetPath`). REFUSAL-ARMED. */
export interface MountDeconvertRequest {
  type: 'mountDeconvert';
  mountPointId: string;
  targetPath: string;
}

/** v4 item-route DELETE — the FileTable's per-file delete (rides `mountFileDelete`). */
export interface MountFileDeleteRequest {
  type: 'mountFileDelete';
  mountPointId: string;
  path: string;
}

/** v4 item-route PATCH — rename and/or describe (descriptions are blob-only). */
export interface MountFileUpdateRequest {
  type: 'mountFileUpdate';
  mountPointId: string;
  path: string;
  description?: string;
  rename?: string;
}

/** v4 `GET …/blobs` (`?folder=` scoped) — the FileTable's lazy blob-detail read. */
export interface MountBlobsListRequest {
  type: 'mountBlobsList';
  mountPointId: string;
  folder?: string;
}

/**
 * v4 `GET /api/v1/system/browse-directory[?path=…]` — the DirectoryPicker's
 * host-filesystem browser (absent/empty `path` → the process home dir).
 */
export interface SystemBrowseDirectoryRequest {
  type: 'systemBrowseDirectory';
  path?: string;
}

/** One directory entry in a {@link BrowseDirectoryResult}. */
export interface BrowseDirectoryEntry {
  name: string;
  path: string;
}

/** The `systemBrowseDirectory` success body (v4 `browse-directory` route). */
export interface BrowseDirectoryResult {
  path: string;
  parent: string | null;
  directories: BrowseDirectoryEntry[];
  /** Present (with an empty `directories`) when the readdir was denied. */
  error?: string;
}

/**
 * The Scriptorium mount-file action + browse requests (P4.6z). Folded into
 * {@link CoreRequest} below.
 */
export type ScriptoriumRequest =
  | MountScanRequest
  | MountConvertRequest
  | MountDeconvertRequest
  | MountFileDeleteRequest
  | MountFileUpdateRequest
  | MountBlobsListRequest
  | SystemBrowseDirectoryRequest;

// ===========================================================================
// P4.d3 (lane D) — the data-retention setting client surface.
//
// Single-author block (lane D). The file owner (lane B) must not edit it. The
// wire request `type` names mirror the Shared contract; the response body
// (`{staleChatDays}`) is read defensively through `CoreClient.dispatchData`, so
// no CoreResponse variant is added (the P4.6x / settings precedent). The union
// members are added inline in the CoreRequest union above with a P4.d3 marker.
// ===========================================================================

/** The instance-wide data-retention settings (v4 `dataRetention`). */
export interface DataRetentionSettingsDto {
  /** Days a chat may sit with no played message before its regenerable working
   *  data is tidied by the nightly sweep (default 30; bounds 1–3650). */
  staleChatDays: number;
}

/** v4 GET `/api/v1/settings/data-retention`. */
export interface DataRetentionSettingsRequest {
  type: 'dataRetentionSettings';
}

/** v4 PUT `/api/v1/settings/data-retention` — merge + validate + echo. */
export interface DataRetentionSettingsUpdateRequest {
  type: 'dataRetentionSettingsUpdate';
  staleChatDays?: number;
}

// ===========================================================================
// --- P4.6al additions ---
// The text-replacement rule surface (lane A implements the Rust dispatch +
// REST edges; these mirror the pinned §1 shapes). Appended-only per the round
// contract; the union members are wired by the unifier (the client api casts
// at the dispatch boundary in-lane). Cross-checked name-for-name against
// types.rs at unification.
// ===========================================================================

/** One text-replacement rule (v4 `text_replacement_rules`). camelCase serde. */
export interface TextReplacementRule {
  id: string;
  fromText: string;
  toText: string;
  caseSensitive: boolean;
  enabled: boolean;
  sortOrder: number;
  createdAt: string;
  updatedAt: string;
}

/** The create/update input body (partial for update). */
export interface TextReplacementRuleInput {
  fromText: string;
  toText: string;
  caseSensitive?: boolean;
  enabled?: boolean;
  sortOrder?: number;
}

/** GET `/api/v1/settings/text-replacements` → `{ rules, count }`. */
export interface TextReplacementsListRequest {
  type: 'textReplacementsList';
}

/** POST `/api/v1/settings/text-replacements` → the created rule (201).
 *  A duplicate `fromText`+`caseSensitive` hits v4's conflict arm. */
export interface TextReplacementCreateRequest extends TextReplacementRuleInput {
  type: 'textReplacementCreate';
}

/** PATCH `/api/v1/settings/text-replacements/:id` → the updated rule. */
export interface TextReplacementUpdateRequest {
  type: 'textReplacementUpdate';
  id: string;
  fromText?: string;
  toText?: string;
  caseSensitive?: boolean;
  enabled?: boolean;
  sortOrder?: number;
}

/** DELETE `/api/v1/settings/text-replacements/:id` → v4's delete envelope. */
export interface TextReplacementDeleteRequest {
  type: 'textReplacementDelete';
  id: string;
}

/** POST `?action=bulk-replace` — transactional full-list replace → `{ rules, count }`. */
export interface TextReplacementsBulkReplaceRequest {
  type: 'textReplacementsBulkReplace';
  rules: TextReplacementRuleInput[];
}

// ===========================================================================
// --- P4.6am additions (lane C) ---
//
// The story-background resolver client surface (dogfood finding #9). The
// dispatch verb `chatGetBackground` and its REST edge (v4 GET
// /api/v1/chats/:id?action=get-background) are lane A's; this block is the TS
// mirror of the Shared-contract §1 shape. The response is read defensively via
// CoreClient.dispatchData, so no CoreResponse variant is added (the P4.d3 /
// settings precedent). The request `type` is bridged with a cast at the call
// site until the unifier folds `ChatGetBackgroundRequest` into the CoreRequest
// union (name-for-name against types.rs). This block appends at the END of the
// file and touches nothing else (§2).
// ===========================================================================

/**
 * v4 GET `/api/v1/chats/:id?action=get-background` (chats `get.ts:182-206`).
 * Every field is null when the chat has no `storyBackgroundImageId` or the
 * referenced file row is missing.
 */
export interface ChatBackgroundDto {
  /** v4's file-path string for the background image (a v4-path — v5 prefers `fileId`). */
  backgroundUrl: string | null;
  /** The background image file id (mapped through the store-backed byte route). */
  fileId: string | null;
  filename: string | null;
  sha256: string | null;
  /** v4's photo link-summary blob (opaque here — the Salon does not consume it). */
  linkSummary: unknown | null;
}

/** The dispatch request for the story-background resolver (§1). */
export interface ChatGetBackgroundRequest {
  type: 'chatGetBackground';
  chatId: string;
}

// ===========================================================================
// --- P4.6ao/P4.6ap unification fold ---
//
// The two P4.6ap-round request interfaces, folded into the CoreRequest union
// at unification per the round's Shared contract (§1/§2) — name-for-name
// against `types.rs` (pinned server-side by `p4_6ao_wire_contract.rs`). The
// api modules (`chat/chat-cost.api.ts`, `screens/salon/story-background.api.ts`)
// re-export them, so consumers are unchanged.
// ===========================================================================

/**
 * The dispatch request for the chat cost breakdown (§1 — v4
 * `GET /api/v1/chats/:id?action=cost`).
 */
export interface ChatGetCostRequest {
  type: 'chatGetCost';
  chatId: string;
  /**
   * Absent means false. v5 never sends it: the detailed arm
   * (`messageBreakdown` / `systemEventBreakdown`) has no v4 client either — a
   * named deferral of the P4.6ap order, not an omission.
   */
  detailed?: boolean;
}

/**
 * The regenerate dispatch request (§2 — v4
 * `POST /api/v1/chats/:id?action=regenerate-background`). The Rust variant and
 * wire name predate the round (`types.rs` P4.6ak block); P4.6ao changed only
 * the dispatch target (refusal → the real enqueue).
 */
export interface ChatRegenerateBackgroundRequest {
  type: 'chatRegenerateBackground';
  chatId: string;
}

// --- P4.6ar/P4.6as/P4.6at unification fold ---
//
// The three request interfaces the round's SPA lanes declared locally, folded
// into the CoreRequest union at unification per the round's Shared contract
// (§1/§2) — name-for-name against `types.rs` (`Request::LlmLogsList` /
// `SystemImageAestheticsGet` / `SystemImageAestheticsSet`, the P4.6ar block).
// The api modules (`chat/llm-logs.api.ts`,
// `screens/settings/images/system-aesthetics.api.ts`) re-export them, so
// consumers are unchanged. The `llmLogGet`/`llmLogDelete` verbs exist
// server-side but no v5 client calls them (v4's Inspector never does either),
// so they deliberately have no union member.
// ===========================================================================

/**
 * The llm-logs list request (§1 — v4 `GET /api/v1/llm-logs`, all filter
 * branches). The Inspector sends only `{chatId, includeMessages: true}` —
 * exactly v4 `useLLMLogs.ts:28`'s query string.
 *
 * `logType` is v4's `?type=` param renamed on the wire — a field literally
 * named `type` would collide with the internally-tagged envelope key; the REST
 * edge maps it. `limit`/`offset` are RAW STRINGS: the Rust handler ports v4's
 * parseInt/Math.min body (including the NaN no-slice quirk) so dispatch and
 * REST agree.
 */
export interface LlmLogsListRequest {
  type: 'llmLogsList';
  messageId?: string;
  chatId?: string;
  characterId?: string;
  logType?: string;
  standalone?: boolean;
  includeMessages?: boolean;
  limit?: string;
  offset?: string;
}

/**
 * The system default-aesthetic read (§2 — v4
 * `GET /api/v1/system/image-aesthetics?kind=lantern|aurora`). Responds
 * `{content}`; `{content: ''}` when the file or the Quilltap General store is
 * absent.
 */
export interface SystemImageAestheticsGetRequest {
  type: 'systemImageAestheticsGet';
  kind: 'lantern' | 'aurora';
}

/**
 * The system default-aesthetic write (§2 — v4
 * `PUT /api/v1/system/image-aesthetics?kind=…`). `content` is optional on the
 * wire (`#[serde(default)]`), but the client always sends it: empty content is
 * meaningful — it DELETES the stored file and restores the fallback (pinned by
 * `image_aesthetics_routes_equivalence`).
 */
export interface SystemImageAestheticsSetRequest {
  type: 'systemImageAestheticsSet';
  kind: 'lantern' | 'aurora';
  content?: string;
}

// ===========================================================================
// P4.6au∥av unification — the `systemHome` verb (lane A's Rust dispatch; lane
// B's `screens/home/home.api.ts` consumes it). Folded into {@link CoreRequest}
// by the unifier per the round's §1 contract. The payload (the v4 `HomeData`
// bag) is read DEFENSIVELY via `CoreClient.dispatchData` (the settings
// precedent — no CoreResponse variant added); its shape lives in
// `screens/home/home.api.ts` and is byte-pinned server-side by
// `home_routes_equivalence`.
// ===========================================================================

/**
 * The home-dashboard read (§1 — v4 `GET /api/v1/system/home` /
 * `lib/services/home-data.service.ts`). No parameters — single-user; the
 * engine supplies the user scope.
 */
export interface SystemHomeRequest {
  type: 'systemHome';
}

// ===========================================================================
// P4.6bb — Pascal's Workbench (the `/custom-tools` vertical).
//
// §W1 of the unit-12 ∥ P4.6bb round contract: the four Workbench dispatch verbs
// and every DTO, mirrored NAME-FOR-NAME from lane AY's Rust unions (the
// `p4_6ar_wire_contract` precedent — the wire diff runs at unification). Each
// DTO is transcribed field-for-field from the SAME v4 lines lane AY transcribed
// from; neither lane invents fields.
//
// §W2: the editor's file I/O rides verbs ALREADY on main — `mountFileRead` and
// `mountFileWrite` are MIRROR ADDITIONS here (from `api/types.rs:1242`/`:1361`),
// not contract changes; `mountFileDelete`/`mountFileUpdate` are mirrored above.
//
// The four response bodies ride the server's envelope and are read DEFENSIVELY
// via `CoreClient.dispatchData` (the P4.6x/P4.6au precedent), so no
// `CoreResponse` variants are added.
// ===========================================================================

/**
 * What a store is attached to. One store can carry several of these
 * (v4 `lib/pascal/workbench.ts:22-28`).
 */
export interface MountAttachment {
  kind: 'general' | 'project' | 'group' | 'character' | 'unattached';
  /** The attached entity's id — absent for `general` and `unattached`. */
  id?: string;
  /** Human-readable badge text: the project/group/character name, etc. */
  label: string;
}

/** A valid definition, as the library lists it (v4 `workbench.ts:31-44`). */
export interface CustomToolLibraryEntry {
  valid: true;
  name: string;
  title: string;
  description: string;
  disabled: boolean;
  defaultVisibility: 'public' | 'whisper';
  rollForm: 'range' | 'dice';
  /** §B — true when the definition declares an `llm` consult block. */
  llm: boolean;
  parameterCount: number;
  outcomeCount: number;
  mountPointId: string;
  mountName: string;
  definitionPath: string;
  attachments: MountAttachment[];
}

/**
 * A broken definition file, listed rather than hidden (v4 `workbench.ts:47-54`).
 * `reason` is the loader's verbatim sentence (`formatDefinitionIssues`) — the
 * library renders it as-is and never re-derives it.
 */
export interface CustomToolLibraryError {
  valid: false;
  definitionPath: string;
  mountPointId: string;
  mountName: string;
  reason: string;
  attachments: MountAttachment[];
}

/** The library GET body (v4 `workbench.ts:56-59`). */
export interface CustomToolLibraryResponse {
  tools: CustomToolLibraryEntry[];
  errors: CustomToolLibraryError[];
}

/** A save target, with the names already on that store's table (v4 `:62-67`). */
export interface DestinationStore {
  mountPointId: string;
  mountName: string;
  /** Names already defined in this store — a duplicate would be a load-time rejection. */
  existingToolNames: string[];
}

/** The destination tree (v4 `workbench.ts:69-77`). */
export interface CustomToolDestinations {
  /** The General store, or null when unprovisioned. */
  general: DestinationStore | null;
  projects: Array<{ projectId: string; projectName: string; stores: DestinationStore[] }>;
  groups: Array<{
    groupId: string;
    groupName: string;
    stores: Array<DestinationStore & { official: boolean }>;
  }>;
  characters: Array<{ characterId: string; characterName: string } & DestinationStore>;
  /** Enabled stores attached to nothing — inert until linked. */
  other: DestinationStore[];
}

/** Resolved parameter values, post-default and post-clamp (v4 `custom-tools.ts:428`). */
export type ResolvedParams = Record<string, number | string | boolean>;

/**
 * The metadata keys the winning outcome tested, and what they held when it was
 * tested (v4 `custom-tools.ts:612`).
 */
export type MetadataTested = Record<string, number | string | boolean>;

/**
 * Everything a run produced (v4 `lib/pascal/custom-tools.ts:431-451`) — the
 * proving bench's preview body. NOT the popup's five-field run summary, which is
 * {@link CustomToolManualRunSummary}.
 */
export interface CustomToolRunResult {
  tool: string;
  params: ResolvedParams;
  rollForm: 'range' | 'dice';
  notation?: string;
  raw: number;
  diceRolls?: number[];
  value: number;
  state: 'success' | 'partial' | 'failure' | 'info';
  outcomeIndex: number;
  /** The rendered outcome message, templates substituted. */
  message: string;
  /** Dice breakdown, or '' for Form A. */
  diceBreakdown: string;
  visibility: 'public' | 'whisper';
  /**
   * The metadata keys the winning outcome tested, and what they held when it
   * was tested. Absent when the winning row consulted no metadata.
   */
  metadataTested?: MetadataTested;
  /**
   * §A — the consult, when the tool declared one. On a BENCH run a scripted
   * oracle carries `provider: 'bench', model: 'simulated'`.
   */
  llm?: LlmConsultRecord;
}

/**
 * The Workbench audit: per-outcome hit counts over many simulated draws
 * (v4 `lib/pascal/custom-tools.ts:962-968`). `runs` is server-fixed at
 * `AUDIT_RUNS = 10_000` (v4 `custom-tools/route.ts:33`) — no `runs` field
 * crosses the wire on the request.
 */
export interface CustomToolAuditResult {
  runs: number;
  outcomes: Array<{ index: number; hits: number; share: number }>;
  valueMin: number;
  valueMax: number;
  valueMean: number;
}

/**
 * The bench's fact sheet (v4 `custom-tools/route.ts:61-64`
 * `MetadataInputSchema`): a hand-typed metadata object passes through VERBATIM;
 * a `{ characterId }` object (exactly one key, a string) hydrates that
 * character's real sheet server-side. The `{characterId}` branch is checked
 * FIRST — it would also satisfy the catch-all record. The SPA sends either form
 * and NEVER resolves the sheet itself.
 */
export type CustomToolMetadataInput = { characterId: string } | Record<string, unknown>;

/** v4 `GET /api/v1/custom-tools` — the whole table, face up. */
export interface CustomToolsLibraryRequest {
  type: 'customToolsLibrary';
}

/** v4 `GET /api/v1/custom-tools?action=destinations` — the save-target tree. */
export interface CustomToolsDestinationsRequest {
  type: 'customToolsDestinations';
}

/**
 * v4 `POST ?action=preview` — dry-run a definition through the one true
 * execution core. Posts nothing, writes nothing. `definition` is the RAW
 * document (v4 `z.unknown()`): the server validates it and returns the
 * `formatDefinitionIssues` sentence on rejection.
 */
export interface CustomToolPreviewRequest {
  type: 'customToolPreview';
  definition: unknown;
  params?: Record<string, unknown> | null;
  private?: boolean;
  metadata?: CustomToolMetadataInput | null;
  /**
   * §B (the `c53510c7` state-cascade round) — the mock merged state
   * (chat → project → group → general) that `$state` references and
   * `{{state.path}}` templates resolve against. Absent/null → `{}`.
   */
  state?: Record<string, unknown> | null;
  /**
   * §B (the `616930db` round) — the bench's oracle for a consulting tool:
   * `{live:true}` spends one real consult, `{output}` scripts the answer,
   * `{fail:true}` scripts the silence. Absent → the seam stays unwired and a
   * consulting tool takes its author's errorMessage path.
   */
  llm?: { live: true } | { output: string } | { fail: true } | null;
}

/** v4 `POST ?action=audit` — deal ten thousand hands and report where they landed. */
export interface CustomToolAuditRequest {
  type: 'customToolAudit';
  definition: unknown;
  params?: Record<string, unknown> | null;
  metadata?: CustomToolMetadataInput | null;
  /**
   * §B (the `c53510c7` round) — the mock merged state, held FIXED across every
   * one of the audit's draws. Absent/null → `{}`.
   */
  state?: Record<string, unknown> | null;
  /**
   * §B — the audit's oracle has NO `live` arm, and that is the SHAPE of the
   * contract, not a guard: ten thousand hands must never mean ten thousand
   * paid consults. Scripted answer or silence only.
   */
  llm?: { output: string } | { fail: true } | null;
}

/**
 * §W2 mirror — v4 `GET /api/v1/mount-points/[id]/files/[...path]` in its JSON
 * envelope form (the `readMountFile` result). Mirrored from the Rust
 * `Request::MountFileRead` (`api/types.rs:1242`); the Workbench editor loads a
 * definition through this rather than the REST JSON leg (refusal-armed in v5).
 */
export interface MountFileReadRequest {
  type: 'mountFileRead';
  mountPointId: string;
  path: string;
  encoding?: string;
  offset?: number;
  limit?: number;
}

/** The `mountFileRead` success body (the v4 `readMountFile` result). */
export interface MountFileReadResult {
  mountPointId: string;
  relativePath: string;
  encoding: string;
  content: string;
  mtime: number;
  sha256: string;
  sizeBytes: number;
  mimeType: string;
  fileType: string;
  totalLines?: number;
  truncated?: boolean;
}

/**
 * §W2 mirror — v4 item-route PUT (the `storeMountFile` ingest, overwrite
 * semantics). Mirrored from the Rust `Request::MountFileWrite`
 * (`api/types.rs:1361`). `expectedMtime` carries the editor's held mtime so a
 * concurrent edit surfaces as a conflict rather than a silent clobber; `force`
 * is the operator's decision to overwrite anyway.
 */
export interface MountFileWriteRequest {
  type: 'mountFileWrite';
  mountPointId: string;
  path: string;
  content: string;
  encoding?: string;
  expectedMtime?: number;
  force?: boolean;
  originalMimeType?: string;
  originalFileName?: string;
}

/** The `mountFileWrite` success body (the v4 item-route PUT result). */
export interface MountFileWriteResult {
  mountPointId: string;
  relativePath: string;
  kind: string;
  fileType: string;
  sha256: string;
  sizeBytes: number;
  mimeType: string;
  mtime: number;
}

// ===========================================================================
// P4.9c — the About + Profile vertical (folded at unification).
//
// §3 of the P4.9a ∥ P4.9c ∥ P4.9b ∥ P4.9d round contract, mirrored
// NAME-FOR-NAME from `api/types.rs`'s `=== P4.9c ===` block (the wire diff ran
// at unification). The response bodies (`{profile}` / the data-dir info bag)
// ride the server's envelope and are read DEFENSIVELY via
// `CoreClient.dispatchData` (the settings/home precedent — no `CoreResponse`
// variants added); their shapes live in `screens/profile/profile.api.ts` and
// are byte-pinned server-side by `profile_routes_equivalence`.
// ===========================================================================

/**
 * v4 `GET /api/v1/user/profile` (default action — the `theme-preference` arms
 * were ported earlier via `theme.service`/chatSettings and are NOT here).
 */
export interface UserProfileGetRequest {
  type: 'userProfileGet';
}

/**
 * v4 `PUT /api/v1/user/profile` — `{name?, email?, image?}`. The Rust side
 * decodes each field as a double-option (absent ≠ null): v4's Zod `.optional()`
 * accepts ABSENT but 400s an explicit `null`, and v5 keeps that verbatim — so
 * the client type admits `null` only to describe the wire faithfully; senders
 * that mean "unchanged" must OMIT the field (see `profile.api.ts`).
 */
export interface UserProfileUpdateRequest {
  type: 'userProfileUpdate';
  name?: string | null;
  email?: string | null;
  image?: string | null;
}

/** v4 `PATCH /api/v1/user/profile?action=set-avatar` — `{imageId}`. */
export interface UserProfileSetAvatarRequest {
  type: 'userProfileSetAvatar';
  imageId?: string | null;
}

/**
 * v4 `GET /api/v1/system/data-dir`. The `POST ?action=open` sibling is
 * REFUSAL-ARMED in v5 (a Tauri shell-open is a named future native nicety).
 */
export interface SystemDataDirRequest {
  type: 'systemDataDir';
}

// ===========================================================================
// P4.9a — the My Photos vertical (folded at unification).
//
// §3 of the `616930db` drift-catch-up + P4.9a-resume round, mirrored
// NAME-FOR-NAME from `api/types.rs`'s `=== P4.9a ===` block (the wire diff ran
// at unification). The response bodies (the gallery list/entry shapes) ride
// the server's envelope and are read DEFENSIVELY via `CoreClient.dispatchData`
// (the settings/home precedent — one `Response::PhotoGallery(Value)` variant
// serves all four verbs server-side); their client shapes live in
// `screens/photos/photos.api.ts` and are byte-pinned server-side by
// `photos_routes_equivalence`. (`photoGallerySave` / `photoGalleryEntryGet`
// had no SPA consumer while the deep gallery modal family was deferred; that
// deferral CLOSED with lane P4.9a2 — see the P4.9a2 block at the end of this
// file.)
// ===========================================================================

/** v4 `GET /api/v1/photos` — the cross-mount `photos/` roll-up. */
export interface PhotoGalleryListRequest {
  type: 'photoGalleryList';
  q?: string;
  tag?: string[];
  limit?: number;
  offset?: number;
}

/**
 * v4 `POST /api/v1/photos` — hard-link an image into the Quilltap Uploads
 * `photos/` album. `fileId` is `unknown` because the server distinguishes
 * absent (`received undefined`) from explicit `null` (`received null`) in its
 * rejection — the Rust side decodes a double option.
 */
export interface PhotoGallerySaveRequest {
  type: 'photoGallerySave';
  fileId?: unknown;
  caption?: string | null;
  tags?: string[];
  chatId?: string | null;
}

/** v4 `GET /api/v1/photos/{id}` — `id` is a `doc_mount_file_links.id`. */
export interface PhotoGalleryEntryGetRequest {
  type: 'photoGalleryEntryGet';
  id: string;
}

/** v4 `DELETE /api/v1/photos/{id}` — link-only removal + last-link file GC. */
export interface PhotoGalleryEntryRemoveRequest {
  type: 'photoGalleryEntryRemove';
  id: string;
}

// ===========================================================================
// P4.9a2 + P4.9f1 — folded at the consult-wire + image-detail + wardrobe
// round's unification, mirrored NAME-FOR-NAME from `api/types.rs` (the wire
// diff ran here). Both lanes shipped these as LOCAL typed modules behind an
// `as unknown as CoreRequest` cast (the `home.api.ts` pattern); the casts are
// gone and the local declarations now re-export from this file.
// ===========================================================================

/** v4 `GET /api/v1/images/{id}` (`route.ts:39-128`) — P4.9a2's only verb. */
export interface ImageInfoGetRequest {
  type: 'imageInfoGet';
  id: string;
}

/**
 * Per-character equipped slots (v4 `EquippedSlotsSchema`,
 * `lib/schemas/wardrobe.types.ts:121-126`). Each slot holds an array of
 * wardrobe item IDs; multiple items per slot represent layering (t-shirt +
 * sweater). Composite items appear as a single ID and are expanded at read
 * time. It lives HERE because it is a wire payload shape;
 * `wardrobe/equipped-slots.ts` re-exports it (the `WardrobeItemDto`
 * precedent) so this file keeps its no-imports rule.
 */
export interface EquippedSlots {
  top: string[];
  bottom: string[];
  footwear: string[];
  accessories: string[];
}

/** v4 `GET /api/v1/chats/{id}?action=outfit` (`outfit.ts` `handleGetOutfit`). */
export interface ChatOutfitGetRequest {
  type: 'chatOutfitGet';
  chatId: string;
}

/**
 * The seven equip modes, in v4's own enum order
 * (`app/api/v1/chats/[id]/actions/outfit.ts:37-79`). v4's comment: "`wear`
 * honors the item's `replace` flag; `replace` force-swaps the slots it
 * covers. `equip` is a deprecated alias for `wear`" — the alias is the
 * easy-to-miscount seventh; it is carried faithfully, never collapsed into
 * `wear`. The server matches `"wear" | "equip" | "replace"` in one arm and
 * branches only on `replace` (`api/chat_outfits.rs:422,441-446`).
 */
export type ChatEquipMode =
  | 'wear'
  | 'replace'
  | 'equip'
  | 'add_to_slot'
  | 'remove_from_slot'
  | 'clear_slot'
  | 'set_all';

/**
 * v4 `POST /api/v1/chats/{id}?action=equip` (`outfit.ts:35-79` schema,
 * `handleEquipSlot:193`). `slot` is required for `add_to_slot` /
 * `remove_from_slot` / `clear_slot`; `itemId` for `wear` / `replace` /
 * `equip` / `add_to_slot`; `slots` for `set_all` (the superRefine arms).
 */
export interface ChatEquipRequest {
  type: 'chatEquip';
  chatId: string;
  characterId: string;
  mode: ChatEquipMode;
  slot?: 'top' | 'bottom' | 'footwear' | 'accessories';
  itemId?: string | null;
  slots?: EquippedSlots;
}

/** v4 `GET /api/v1/wardrobe/transfers` (`transfers/route.ts:236-260`). */
export interface WardrobeTransferDestinationsRequest {
  type: 'wardrobeTransferDestinations';
}

/** v4 `WardrobeTransferDialog.tsx:9` / `transfers/route.ts:47-57`. */
export type WardrobeTransferScope =
  | 'general'
  | 'project'
  | 'group'
  | 'character';

/** v4 `POST /api/v1/wardrobe/transfers` (`transfers/route.ts:47-57`). */
export interface WardrobeTransferApplyRequest {
  type: 'wardrobeTransferApply';
  action: 'move' | 'copy';
  itemId: string;
  sourceCharacterId: string;
  sourceProjectId: string | null;
  destination: { scope: WardrobeTransferScope; id?: string };
}

/** v4 `GET /api/v1/wardrobe` (the global archetype tier). */
export interface WardrobeListRequest {
  type: 'wardrobeList';
}

/** v4 `POST /api/v1/wardrobe` — the `item` bag mirrors `projectWardrobeCreate`. */
export interface WardrobeCreateRequest {
  type: 'wardrobeCreate';
  item: Record<string, unknown>;
}

/**
 * `GET /api/v1/wardrobe/{itemId}` — lane P4.9f1 shipped this BEYOND §1's four
 * archetype verbs (self-flagged in its lane record; additive, accepted at
 * unification). No SPA consumer yet — the type mirrors the wire.
 */
export interface WardrobeItemGetRequest {
  type: 'wardrobeItemGet';
  itemId: string;
}

/** v4 `PUT /api/v1/wardrobe/{itemId}`. */
export interface WardrobeUpdateRequest {
  type: 'wardrobeUpdate';
  itemId: string;
  item: Record<string, unknown>;
}

/** v4 `DELETE /api/v1/wardrobe/{itemId}`. */
export interface WardrobeDeleteRequest {
  type: 'wardrobeDelete';
  itemId: string;
}

/**
 * v4 `POST /api/v1/wardrobe/preview-avatar` — non-persisting (touches neither
 * `avatarOverrides` nor any chat, the route's `:1-15` note). ⚠ The RENDER step
 * answers a loud typed refusal until the `avatar_preview` host wire lands —
 * see the round record; the guard tiers, prompt, and response shape are live.
 */
export interface WardrobePreviewAvatarRequest {
  type: 'wardrobePreviewAvatar';
  characterId: string;
  equippedSlots: EquippedSlots;
  imageProfileId?: string;
}

/**
 * v4 `POST /api/v1/chats/{id}?action=regenerate-avatar` — QUEUES a background
 * job; the fitting-room slots ride as a one-shot `equippedSlots` override.
 */
export interface ChatRegenerateAvatarRequest {
  type: 'chatRegenerateAvatar';
  chatId: string;
  characterId: string;
  /**
   * A one-shot fitting-room override. OPTIONAL, as the server's Zod port has it
   * (`chat_outfits.rs:610` — `EquippedSlotsSchema.optional()`); the wardrobe
   * dialog always sends it, but v4's sidebar camera button posts `{characterId}`
   * alone (`SalonView.tsx:262-266`) and lets the server resolve what is worn.
   */
  equippedSlots?: EquippedSlots;
  imageProfileId?: string;
}

// ===========================================================================
// P4.9G2 — the Data & System tab's dispatch surface (folded at unification).
//
// §1 of the P4.19 ∥ P4.9G1 ∥ P4.9G2 round contract: the sixteen new
// CoreRequest variants P4.9G1 implements in `api/types.rs` and this lane
// (P4.9G2) mirrors NAME-FOR-NAME (the `p4_6ar_wire_contract` precedent — the
// unifier diffs the two by name at unification). Payload fields are the
// best-guess derivation from v4's route bodies (the G1 survey table is
// authoritative); the wire diff reconciles any field-level drift. Response
// bodies ride the server's envelope and are read DEFENSIVELY via
// `CoreClient.dispatchData` (the settings/home precedent — no `CoreResponse`
// variants added); their shapes live in `screens/settings/system/*.api.ts`.
//
// The four WEB-EDGE-ONLY legs (no dispatch verb — reached via `apiUrl()` raw):
//   GET  /api/v1/system/backup/{id}                     (single-use zip stream)
//   POST /api/v1/system/restore?action=upload           (octet-stream → {uploadId})
//   POST /api/v1/system/tools?action=export             (NDJSON .qtap stream)
//   POST /api/v1/system/tools?action=import-preview|import-execute (multipart legs)
// ===========================================================================

/** v4 `POST /api/v1/system/backup` — body `{}`; response `{backupId, manifest}`. */
export interface SystemBackupCreateRequest {
  type: 'systemBackupCreate';
}

/** v4 `POST /api/v1/system/restore?action=preview` — `{uploadId}` → `{preview}`. */
export interface SystemRestorePreviewRequest {
  type: 'systemRestorePreview';
  uploadId: string;
}

/**
 * v4 `POST /api/v1/system/restore` (no action) — `{uploadId, mode}` → `{summary}`.
 * The UI radio `import` maps to the wire `new-account` (v4 `useRestoreData.ts:180`).
 */
export interface SystemRestoreExecuteRequest {
  type: 'systemRestoreExecute';
  uploadId: string;
  mode: 'replace' | 'new-account';
}

/**
 * v4 `GET /api/v1/system/tools?action=export-entities&type=<entityType>` — lists
 * the exportable entities of a type → `{entities, memoryCount}`. `entityType`
 * renames v4's `?type=` query (which would collide with the envelope tag — the
 * `LlmLogsListRequest.logType` precedent).
 */
export interface SystemExportEntitiesRequest {
  type: 'systemExportEntities';
  entityType: string;
}

/**
 * A dry-run of what an export would contain. v4 has no `export-preview` route
 * (the entity listing IS the picker); this speculative dispatch verb is mirrored
 * from §1 by NAME and reconciled at unification — the SPA does not call it (the
 * actual bytes stream from the web-edge `?action=export` leg).
 */
export interface SystemExportPreviewRequest {
  type: 'systemExportPreview';
  entityType: string;
  scope?: 'all' | 'selected';
  selectedIds?: string[];
  includeMemories?: boolean;
}

/**
 * v4 `POST /api/v1/system/tools?action=import-preview` — the JSON leg
 * (`{exportData}`). The multipart leg (a real `File`) is web-edge-only. Response
 * is the server `ImportPreview`.
 */
export interface SystemImportPreviewRequest {
  type: 'systemImportPreview';
  exportData: unknown;
}

/**
 * v4 `POST /api/v1/system/tools?action=import-execute` — the JSON leg
 * (`{exportData, options}`). Response is the server `ImportResult`.
 */
export interface SystemImportExecuteRequest {
  type: 'systemImportExecute';
  exportData: unknown;
  options: {
    selectedIds?: Record<string, string[]>;
    conflictStrategy: string;
    importMemories: boolean;
  };
}

/** v4 `GET /api/v1/system/tools?action=tasks-queue` — stats + rows + processor status. */
export interface SystemTasksQueueRequest {
  type: 'systemTasksQueue';
}

/** v4 `POST /api/v1/system/tools?action=tasks-queue` — `{action}` start/stop. */
export interface SystemTasksQueueControlRequest {
  type: 'systemTasksQueueControl';
  action: 'start' | 'stop';
}

/** v4 `GET /api/v1/system/tools?action=job-concurrency` — `{concurrency}`. */
export interface SystemJobConcurrencyGetRequest {
  type: 'systemJobConcurrencyGet';
}

/**
 * v4 `POST /api/v1/system/tools?action=job-concurrency`. The v4 route body key
 * is `concurrency`; §1 names the DISPATCH field `maxConcurrentJobs` (G1 maps it
 * to v4's `concurrency` at the REST edge). Bounds 1..32, default 4.
 */
export interface SystemJobConcurrencySetRequest {
  type: 'systemJobConcurrencySet';
  maxConcurrentJobs: number;
}

/** v4 `GET /api/v1/system/jobs/{id}` — `{job}` (the full BackgroundJob entity). */
export interface SystemJobGetRequest {
  type: 'systemJobGet';
  jobId: string;
}

/** v4 `POST /api/v1/system/jobs/{id}?action=pause|resume` — `{job}`. */
export interface SystemJobControlRequest {
  type: 'systemJobControl';
  jobId: string;
  action: 'pause' | 'resume';
}

/** v4 `DELETE /api/v1/system/jobs/{id}` — `{success}`. */
export interface SystemJobDeleteRequest {
  type: 'systemJobDelete';
  jobId: string;
}

/** v4 `GET /api/v1/system/tools?action=delete-data-preview` — `{summary}`. */
export interface SystemDeleteDataPreviewRequest {
  type: 'systemDeleteDataPreview';
}

/**
 * v4 `POST /api/v1/system/tools?action=delete-data` — `{confirm}` → `{summary}`.
 * The server gate requires `confirm === 'DELETE_ALL_MY_DATA'` (v4
 * `tools/route.ts:155`); the typed-`DELETE` gate is client-only.
 */
export interface SystemDeleteDataRequest {
  type: 'systemDeleteData';
  confirm: string;
}
