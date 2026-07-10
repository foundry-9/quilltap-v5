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
export type PepperState =
  | 'resolved'
  | 'needs-setup'
  | 'needs-passphrase'
  | 'needs-vault-storage';

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
 * The chat-creation request. **Provisional (P4.5-owned):** P4.5 does not consume
 * this — the first Salon vertical (P4.6) finalises the body/DTO from the v4 POST
 * body. Shape kept loose on purpose; the P4.4 lane implements the server variant.
 */
export interface ChatCreateRequest {
  type: 'chatCreate';
  title?: string;
  chatType?: string;
  participantCharacterIds?: string[];
  [key: string]: unknown;
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
  | ChatSetActiveSpeakerRequest;

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

/** Provisional chat-create DTO (see [`ChatCreateRequest`]). */
export interface ChatCreateDto {
  id: string;
  [key: string]: unknown;
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

/**
 * Creation-progress frame fields (D6). **P4.5 does not consume these this
 * round** — declared so the envelope type is complete and the P4.6 verticals can
 * fold them in. Field names are faithful to v4's `creation-progress.ts` shapes.
 */
export interface CreationProgressFrame {
  level?: 'log' | 'info' | 'warn' | 'error' | 'status';
  message?: string;
}
