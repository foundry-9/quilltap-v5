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
  | { type: 'listChats' }
  | ChatSendRequest
  // --- New this round (the P4.4 lane implements the server side) ---
  | { type: 'setup'; passphrase: string }
  | { type: 'storePepper'; passphrase: string }
  | { type: 'changePassphrase'; oldPassphrase: string; newPassphrase: string }
  | ChatCreateRequest;

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

export interface ChatSummaryDto {
  id: string;
  title: string;
  chatType: string;
  messageCount: number;
  lastMessageAt?: string;
  createdAt: string;
  updatedAt: string;
}

export interface ChatSendResultDto {
  messageId: string;
  hasContent: boolean;
  isMultiCharacter: boolean;
  isPaused: boolean;
  userParticipantId?: string;
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
  | { type: 'chats'; data: ChatSummaryDto[] }
  | { type: 'chatSend'; data: ChatSendResultDto }
  | { type: 'setup'; data: SetupDto }
  | { type: 'ack'; data: Record<string, never> }
  | { type: 'chatCreate'; data: ChatCreateDto }
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
