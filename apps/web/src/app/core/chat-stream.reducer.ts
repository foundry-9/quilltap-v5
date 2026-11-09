/**
 * The SSE stream reducer — a pure fold of chat-scoped {@link ChatStreamFrame}s
 * into per-chat streaming view state. This is the v5 port of v4's client hooks
 * `app/salon/[id]/hooks/useSSEStreaming.ts` + `useMessageStreaming.ts`.
 *
 * ## P4.D384 — v4 `604a082c4`: one frame, one kind
 * A Salon frame is now a member of the `kind`-tagged union, read only through
 * `parseFrameValue` (`./stream-frames`), and {@link reduceChatFrame} is v4's
 * dispatch: `frame.seat ? handleSeatFrame : handleFrame`, each an exhaustive
 * `switch (frame.kind)` ending in `assertNever`. The FLOOR frames (the raised
 * hand and the Storyteller board) are handled the same in both switches — which
 * is v4 `9eab8d2da`'s fix for the seat-tagged `handAnswered` the old
 * presence-based fold swallowed behind its concurrent early return. The old
 * multi-key-per-frame branches retire ("no ordering of tests decides what a
 * frame means"). v4's `604a082c4` "concurrent empty bubble" fix (React's
 * updater race reading a seat's text back out of state) is CONVERGED BY
 * CONSTRUCTION here: this fold reads its own `concurrentStreams` in a pure
 * pass, so a seat's `done` always sees its finished text.
 *
 * {@link reduceLegacyChatFrame} keeps the pre-`604a082c4` presence-based fold
 * for the Brahma console, whose stream v4 still renders as the legacy bag
 * (`legacyFrameEncoder`) and reads with its own client (ruling P-2).
 *
 * The reducer is deliberately PURE (`(state, frame) => state`) and side-effect
 * free — v4's hook interleaves React state, toasts, scroll, and `fetchChat()`;
 * those are the Salon vertical's concern (P4.6). Here we only compute what the
 * bubble needs to render.
 *
 * ## Frame union semantics (from v4)
 *  - `content` — INCREMENTAL append to the in-progress bubble.
 *  - `reasoning` — CUMULATIVE live-replace (each frame carries the full text).
 *  - `reasoningSegments` / `reasoningContent` — positioned blocks / full text on `done`.
 *  - `toolsDetected` — push a tool batch tagged with the prose offset (`content.length`).
 *  - `toolResult` — mark the matching call on the MOST RECENT batch, carrying
 *    the frame's sibling `error` sentence onto it.
 *  - `status` — the live status stage.
 *  - `carinaAnswer` / `hostAnnouncement` — insert a posted message, deduped by id.
 *  - `confirmationResult` — patch a bubble's confirmation state (+ revised content).
 *  - `done` — finalize the turn (skip / empty-response / pending-external / normal).
 *  - `turnStart` / `turnComplete` / `chainComplete` — the multi-character chain.
 *  - transport error frame `{ error, errorType, details }` — record + halt.
 *
 * ## The v5 structural difference (handle it explicitly)
 * v4 reads frames from the POST response body. v5 rides the GLOBAL `/api/events`
 * stream (scope-tagged by `chatId`) while `dispatch(chatSend)` resolves only when
 * the turn COMPLETES. The consumer therefore subscribes+filters by `chatId`
 * BEFORE dispatching, runs the dispatch promise concurrently, and reconciles the
 * final result at the end — the reducer folds the frames independently of that
 * promise. See {@link ChatStreamState.finalDone} for the terminal payload the
 * consumer reconciles against the dispatch reply.
 */

import type {
  AttachmentResults,
  ChatStreamFrame,
  DoneFrame,
  DoneOutcome,
  FrameOf,
  LegacyStreamBag,
  ReasoningSegment,
  ResponseStatus,
  NextSpeakerInfo,
  PostedMessage,
  RouteAttempt,
  StorytellerBid,
  StorytellerRound,
} from './core-contract';
import { assertNever, doneLeavesNoReply } from './stream-frames';

// === P4.D344: Storyteller mode ===
/**
 * Storyteller frames the Salon's board reads off a send's stream (v4
 * `useSSEStreaming.ts:17-20`). The reducer only CARRIES the latest one; the
 * board's cache write is the vertical's (`reportStreamTransitions`).
 */
export type StorytellerStreamEvent =
  | { kind: 'round'; round: StorytellerRound }
  | { kind: 'bid'; roundId: string; bid: StorytellerBid }
  | { kind: 'open'; roundId: string };

/** One live bubble for a concurrent ("all at once") storyteller turn (v4 `useSSEStreaming.ts:23-26`). */
export interface ConcurrentStream {
  participantId: string;
  content: string;
}
// === end P4.D344 ===

/** One in-progress tool call within a batch. */
export interface PendingToolCall {
  id: string;
  name: string;
  status: 'pending' | 'success' | 'error';
  result?: unknown;
  arguments?: Record<string, unknown>;
  /**
   * The failing tool's human-readable sentence, carried RAW (the executor's own
   * `Error: ` prefix intact) from the frame's `error` field — a SIBLING of
   * `result`, since `result` is null on failure. Kept unresolved here so the
   * reducer stays pure and the render site resolves it, mirroring v4's own
   * separation (v4 `d9c98cf2`, Bug 84 — the field had no reader in either app
   * until then). See `chat/tool-result-error.ts`.
   */
  errorText?: string;
}

/**
 * A batch of tool calls plus the offset into the streamed prose where they fired
 * — lets the bubble splice each batch back into the live text at that point (the
 * live analogue of the persisted `anchorOffset`).
 */
export interface StreamingToolBatch {
  offset: number;
  calls: PendingToolCall[];
}

/** A message surfaced during the stream. */
export type StreamMessage =
  | {
      kind: 'assistant';
      id: string;
      content: string;
      participantId: string | null;
      provider: string | null;
      modelName: string | null;
      /** `null` emitted when nothing failed (P4.D177 §C.1). */
      routeTrail: RouteAttempt[] | null;
      isSilentMessage?: boolean;
      reasoningContent: string | null;
      reasoningSegments: ReasoningSegment[] | null;
      /** True when emitted from an intermediate (chained) done rather than the final one. */
      intermediate: boolean;
      // Confirmation patch (applied when a confirmationResult arrives for this id).
      confirmed?: boolean | null;
      confirmationChecked?: boolean;
      confirmationRevised?: boolean;
      confirmationNotes?: string | null;
      /**
       * Never set by the live patch: v4's `applyConfirmationResult` carries
       * confirmed/revised/notes/content only — the pre-revision text arrives
       * with the post-turn canonical refetch. Declared so the stream→bubble
       * mapper carries the whole five-field family rather than silently
       * dropping this one (P4.D132; the badge reads it).
       */
      confirmationOriginalContent?: string | null;
    }
  | { kind: 'carina'; id: string; message: PostedMessage }
  | { kind: 'host'; id: string; message: PostedMessage }
  | { kind: 'pascal'; id: string; message: PostedMessage };

/** The terminal `done` info the consumer reconciles against the dispatch reply. */
export interface FinalDoneInfo {
  /**
   * How the turn closed (v4 `done.outcome`, P4.D384). The four booleans below
   * are DERIVED from it in ONE place ({@link outcomeFlags}, v4's
   * `legacyOutcomeFlags`) so the Salon's existing readers do not churn.
   */
  outcome: DoneOutcome;
  messageId: string | null;
  provider: string | null;
  modelName: string | null;
  turn: NextSpeakerInfo | null;
  skipped: boolean;
  skippedParticipantId: string | null;
  /** v4 `done.yielded`: the character yielded the floor to the raised hand. */
  yielded: boolean;
  emptyResponse: boolean;
  emptyResponseReason: string | null;
  pendingExternalTurn: boolean;
  toolsExecuted: boolean;
  /**
   * v4 `done.stopped`: the operator hard-stopped the turn before any prose
   * streamed — reset without a bubble or a toast, as `skipped` does.
   */
  stopped: boolean;
  /**
   * The provider plugin's attachment ledger, carried STRAIGHT off the frame
   * (bug 94). The reducer neither reads nor reshapes it: the render site raises
   * v4's warning from `failed`, and it can only do that if the pure fold hands
   * the object through. Absent on the frame → `null`, and the SAME object
   * reference is preserved so a consumer can use identity as the once-per-done
   * key (the Courier's `pendingExternalTurn` patch spreads it forward unchanged).
   */
  attachmentResults: AttachmentResults | null;
}

/** A raised hand being put to a character (v4 `handAsked` frame, `9eab8d2da`) — see {@link ChatStreamState.handAsked}. */
export interface HandAsked {
  participantId: string;
  characterName: string;
}

/** A raised hand's answer (v4 `handAnswered` frame) — see {@link ChatStreamState.handAnswered}. */
export interface HandAnswered {
  participantId: string | null;
  characterName: string | null;
  yielded: boolean;
}

/** The folded per-chat streaming state. */
export interface ChatStreamState {
  /** Content has begun streaming for the current in-progress turn. */
  streaming: boolean;
  /** Awaiting the first content chunk of the current turn. */
  waitingForResponse: boolean;
  /** Accumulated content of the in-progress bubble (v4 `fullContent`). */
  content: string;
  /** Cumulative live reasoning (replace, not append). */
  reasoning: string;
  /** Tool batches for the in-progress bubble, tagged with prose offsets. */
  toolBatches: StreamingToolBatch[];
  /** The live status stage (v4 `responseStatus`). */
  status: ResponseStatus | null;
  /** Whether we are inside a multi-character chain. */
  inChain: boolean;
  /** The currently responding participant (across chained turns). */
  respondingParticipantId: string | null;
  /** Messages surfaced during the stream (assistant bubbles + carina/host), deduped by id. */
  messages: StreamMessage[];
  /** A recorded transport / stream error (v4 throws; we halt and record). */
  error: string | null;
  /**
   * The error frame's persistence flags (P4.D301, v4 bug 241 — the send shell's
   * `userMessagePersisted` / `pendingToolResultsPersisted`), each copied ONLY
   * when the frame carried a real boolean (v4 `useSSEStreaming.ts:695-698`), so
   * an absent key means "the server did not say". `null` until an error frame.
   * The Salon reads it when the dispatch rejection carries no `submission`.
   */
  errorSubmission: { userMessagePersisted?: boolean; pendingToolResultsPersisted?: boolean } | null;
  /** The terminal chain has completed (chainComplete, or a non-chained final done). */
  finished: boolean;
  /**
   * The `chainComplete` frame's stated reason, or null when the chain ended
   * without one (a non-chained final `done`). v4 defaults it at the callback
   * boundary (`useSSEStreaming.ts:687` — `data.reason || 'no_next_speaker'`);
   * the reducer stays literal and the vertical applies v4's default.
   */
  chainReason: string | null;
  /**
   * The `chainComplete` frame's `paused` flag (v4 bug 123, `:690` —
   * `data.paused === true`, so absent and `false` are the same answer). The
   * fold must CARRY it: the pause announcement is raised by the vertical after
   * the reconcile, long after this frame has been folded away.
   */
  chainPaused: boolean;
  /**
   * The `chainComplete` frame's `heldUserTurn` flag (v4 bug 137 — `data
   * .heldUserTurn === true`, so absent and `false` are one answer). Carried
   * for the same reason as {@link chainPaused}: the notice it raises is a side
   * effect the vertical performs after the reconcile, long after this frame has
   * been folded away.
   */
  chainHeldUserTurn: boolean;
  /**
   * The latest `handAnswered` frame (v4 `useSSEStreaming.ts:734-742`): the
   * character asked about the operator's raised hand has answered it. CARRIED
   * on the state for the same reason as {@link chainPaused}: the fold must
   * carry it, the toast and the lowering are raised by the vertical.
   */
  handAnswered: HandAnswered | null;
  /** Bumped per `handAnswered` frame so two answers in one chain each report. */
  handAnsweredSeq: number;
  /**
   * The latest `handAsked` frame (v4 `9eab8d2da`): the raised hand is being put
   * to this character. CARRIED like {@link handAnswered} — the hand status the
   * composer's chip shows is the vertical's.
   */
  handAsked: HandAsked | null;
  /** Bumped per `handAsked` frame. */
  handAskedSeq: number;
  // === P4.D344: Storyteller mode ===
  /**
   * The latest storyteller board frame (`storytellerRound` / `storytellerBid`
   * / `roundOpen`) — CARRIED, like {@link handAnswered}: the fold is pure, the
   * board's cache write is the vertical's.
   */
  storyteller: StorytellerStreamEvent | null;
  /** Bumped per storyteller frame so every bid in a burst reaches the board. */
  storytellerSeq: number;
  /**
   * The "all at once" turns still streaming, one bubble per participant (v4
   * `concurrentStreams`). A concurrent frame leaves the shared streaming slot
   * above alone.
   */
  concurrentStreams: ConcurrentStream[];
  // === end P4.D344 ===
  /** The terminal `done` payload for dispatch-reply reconciliation. */
  finalDone: FinalDoneInfo | null;
  // --- internal bookkeeping ---
  /** Monotonic batch counter so tool-call ids stay unique across batches. */
  toolBatchSeq: number;
}

/** A fresh streaming state (call at the start of every send / continue turn). */
export function initialChatStreamState(): ChatStreamState {
  return {
    streaming: false,
    waitingForResponse: false,
    content: '',
    reasoning: '',
    toolBatches: [],
    status: null,
    inChain: false,
    respondingParticipantId: null,
    messages: [],
    error: null,
    errorSubmission: null,
    finished: false,
    chainReason: null,
    chainPaused: false,
    chainHeldUserTurn: false,
    handAnswered: null,
    handAnsweredSeq: 0,
    handAsked: null,
    handAskedSeq: 0,
    storyteller: null,
    storytellerSeq: 0,
    concurrentStreams: [],
    finalDone: null,
    toolBatchSeq: 0,
  };
}

/**
 * Fold ONE parsed frame into the state, returning a new state (pure) — v4's
 * `handleLine`: a frame is a concurrent seat's or the stream's own, and then
 * exactly one kind.
 */
export function reduceChatFrame(prev: ChatStreamState, frame: ChatStreamFrame): ChatStreamState {
  return frame.seat ? reduceSeat(prev, frame, frame.seat.participantId) : reduceMain(prev, frame);
}

type FloorFrame = FrameOf<'handAsked' | 'handAnswered' | 'storytellerRound' | 'storytellerBid' | 'roundOpen'>;

/**
 * The raised hand and the Storyteller board: the same whoever's seat sent them
 * (v4 `applyFloorFrame`). Carried, never toasted here — the hand status and the
 * board's cache write are the vertical's (`reportStreamTransitions`).
 */
function applyFloorFrame(s: ChatStreamState, frame: FloorFrame): ChatStreamState {
  switch (frame.kind) {
    case 'handAsked':
      return {
        ...s,
        handAsked: { participantId: frame.participantId, characterName: frame.characterName },
        handAskedSeq: s.handAskedSeq + 1,
      };
    case 'handAnswered':
      return {
        ...s,
        handAnswered: {
          participantId: frame.participantId,
          characterName: frame.characterName,
          yielded: frame.yielded,
        },
        handAnsweredSeq: s.handAnsweredSeq + 1,
      };
    // === P4.D344: the board frames — handed to the board, nothing else folds
    // from them. The carried shapes are the ones the board reads (unchanged). ===
    case 'storytellerRound':
      return carryStoryteller(s, { kind: 'round', round: frame.round });
    case 'storytellerBid':
      return carryStoryteller(s, { kind: 'bid', roundId: frame.roundId, bid: frame.bid });
    case 'roundOpen':
      return carryStoryteller(s, { kind: 'open', roundId: frame.roundId });
    // === end P4.D344 ===
    default:
      return assertNever(frame);
  }
}

/**
 * Storyteller "all at once": a frame from one of several concurrent turns
 * (v4 `handleSeatFrame`). Each seat gets its own bubble; the shared streaming
 * slot is left alone.
 */
function reduceSeat(s: ChatStreamState, frame: ChatStreamFrame, pid: string): ChatStreamState {
  const has = s.concurrentStreams.some((c) => c.participantId === pid);
  switch (frame.kind) {
    case 'handAsked':
    case 'handAnswered':
    case 'storytellerRound':
    case 'storytellerBid':
    case 'roundOpen':
      return applyFloorFrame(s, frame);
    case 'turnStart':
      return {
        ...s,
        waitingForResponse: false,
        concurrentStreams: has ? s.concurrentStreams : [...s.concurrentStreams, { participantId: pid, content: '' }],
      };
    case 'content': {
      if (!frame.text) return s;
      const text = frame.text;
      return {
        ...s,
        concurrentStreams: has
          ? s.concurrentStreams.map((c) => (c.participantId === pid ? { ...c, content: c.content + text } : c))
          : [...s.concurrentStreams, { participantId: pid, content: text }],
      };
    }
    case 'done': {
      const finished = s.concurrentStreams.find((c) => c.participantId === pid)?.content ?? '';
      let messages = s.messages;
      if (frame.messageId && !doneLeavesNoReply(frame.outcome)) {
        messages = dedupePush(messages, assistantFromDone(frame, finished, pid, true));
      }
      return {
        ...s,
        messages,
        concurrentStreams: s.concurrentStreams.filter((c) => c.participantId !== pid),
      };
    }
    case 'posted':
      return { ...s, messages: dedupePush(s.messages, postedMessage(frame)) };
    case 'error':
      // v4 toasts it and reads on; v5 records it for the run's one error toast
      // (recorded at P4.D344 — kept). The seat's turn is OVER, though: v5
      // contains a concurrent seat's failure to that seat and the round reads
      // on (`CONCURRENT_TURN_FAILURE_CONTAINED`), so its half-written bubble
      // goes with it — no `done` will ever come to settle it. (v4's round
      // rejects whole there, so v4 never reaches this case.) The shared
      // streaming slot is left alone: the other seats and the tail stream on.
      return {
        ...s,
        error: frame.message,
        concurrentStreams: s.concurrentStreams.filter((c) => c.participantId !== pid),
      };
    // A concurrent turn's status, thinking, tools and chain bookkeeping have no
    // bubble of their own; the round's own frames carry the rest.
    case 'reasoning':
    case 'status':
    case 'fileProcessing':
    case 'toolsDetected':
    case 'toolResult':
    case 'turnComplete':
    case 'chainComplete':
    case 'pendingExternalTurn':
    case 'confirmationResult':
    case 'swipeDone':
      return s;
    default:
      return assertNever(frame);
  }
}

/** The stream's own frames (v4 `handleFrame`), mapped onto the pure fold. */
function reduceMain(s: ChatStreamState, frame: ChatStreamFrame): ChatStreamState {
  switch (frame.kind) {
    case 'handAsked':
    case 'handAnswered':
    case 'storytellerRound':
    case 'storytellerBid':
    case 'roundOpen':
      return applyFloorFrame(s, frame);

    case 'status':
      return { ...s, status: frame.status };

    // content — incremental append; flip streaming on the first chunk
    case 'content': {
      if (!frame.text) return s;
      return {
        ...s,
        content: s.content + frame.text,
        waitingForResponse: s.streaming ? s.waitingForResponse : false,
        streaming: true,
      };
    }

    // reasoning — cumulative, so REPLACE the buffer
    case 'reasoning':
      return { ...s, reasoning: frame.text };

    // the turn failed after the stream opened — record and halt (v4 throws)
    case 'error': {
      const message = frame.details ? `${frame.message}: ${frame.details}` : frame.message;
      const errorSubmission: NonNullable<ChatStreamState['errorSubmission']> = {};
      if (typeof frame.userMessagePersisted === 'boolean') {
        errorSubmission.userMessagePersisted = frame.userMessagePersisted;
      }
      if (typeof frame.pendingToolResultsPersisted === 'boolean') {
        errorSubmission.pendingToolResultsPersisted = frame.pendingToolResultsPersisted;
      }
      return { ...s, error: message, errorSubmission, status: null, streaming: false, waitingForResponse: false };
    }

    // tool detection — push a batch tagged with the current prose offset
    case 'toolsDetected': {
      const seq = s.toolBatchSeq;
      const calls: PendingToolCall[] = frame.names.map((name, idx) => ({
        id: `tool-${seq}-${idx}`,
        name,
        status: 'pending',
        arguments: frame.arguments[idx],
      }));
      return {
        ...s,
        toolBatchSeq: seq + 1,
        toolBatches: [...s.toolBatches, { offset: s.content.length, calls }],
      };
    }

    // tool result (FLAT) — mark the matching call on the MOST RECENT batch
    case 'toolResult':
      return {
        ...s,
        toolBatches: applyToolResult(s.toolBatches, {
          index: frame.index,
          name: frame.name,
          success: frame.success,
          result: frame.result,
          error: frame.error,
        }),
      };

    // a Carina answer, a Host announcement or a Pascal outcome surfaced
    // mid-turn — inserted at once, deduped by id
    case 'posted':
      return { ...s, messages: dedupePush(s.messages, postedMessage(frame)) };

    // an answer-confirmation result (FLAT) — patch the bubble (and, on a
    // revision, its content)
    case 'confirmationResult':
      return {
        ...s,
        messages: applyConfirmation(s.messages, {
          messageId: frame.messageId,
          confirmed: frame.confirmed,
          revised: frame.revised,
          notes: frame.notes,
          ...(frame.content !== undefined ? { content: frame.content } : {}),
        }),
      };

    case 'done':
      return reduceDone(s, frame);

    // the Courier parked a placeholder for a manual / clipboard turn; the
    // accompanying `done` (with `pendingExternalTurn: true`) closes the turn
    case 'pendingExternalTurn':
      return { ...s, finalDone: { ...(s.finalDone ?? emptyDone()), pendingExternalTurn: true } };

    // turn start (a character is about to respond) — reset the buffer. Only a
    // CHAINED turn (depth > 0) enters a chain: the server also sends a depth-0
    // turnStart before every first turn so the bubble shows the right
    // responder, and that one must leave the turn's done on the ordinary path.
    // v4 never CLEARS `inChain` on depth 0, it only declines to set it (bug
    // 242, `b045dd986`).
    case 'turnStart':
      return {
        ...s,
        inChain: frame.chainDepth > 0 ? true : s.inChain,
        content: '',
        reasoning: '',
        toolBatches: [],
        streaming: false,
        waitingForResponse: true,
        respondingParticipantId: frame.participantId,
      };

    // turn complete (a chained character finished) — reset without ending the chain
    case 'turnComplete':
      return { ...s, content: '', reasoning: '', streaming: false, waitingForResponse: false };

    // chain complete (all chained turns done) — terminal
    case 'chainComplete':
      return {
        ...s,
        inChain: false,
        content: '',
        reasoning: '',
        streaming: false,
        waitingForResponse: false,
        respondingParticipantId: null,
        finished: true,
        // v4 builds the onChainComplete event here. v5's fold is pure and the
        // announcement is a side effect, so the fields it needs ride the state
        // to the vertical instead of a callback.
        chainReason: frame.reason,
        chainPaused: frame.paused === true,
        chainHeldUserTurn: frame.heldUserTurn === true,
      };

    // Not frames a Salon send carries.
    case 'fileProcessing':
    case 'swipeDone':
      return s;

    default:
      return assertNever(frame);
  }
}

/** The bubble a posted frame surfaces (`what` → the bubble kind). */
function postedMessage(frame: FrameOf<'posted'>): StreamMessage {
  const kind = frame.what === 'carinaAnswer' ? 'carina' : frame.what === 'hostAnnouncement' ? 'host' : 'pascal';
  return { kind, id: frame.message.id, message: frame.message };
}

/** v4 `assistantMessageFromDone` — the assistant bubble a `done` settles. */
function assistantFromDone(
  frame: DoneFrame,
  content: string,
  participantId: string | null,
  intermediate: boolean,
): Extract<StreamMessage, { kind: 'assistant' }> {
  return {
    kind: 'assistant',
    id: frame.messageId as string,
    content,
    participantId,
    provider: frame.provider || null,
    modelName: frame.modelName || null,
    routeTrail: frame.routeTrail ?? null,
    isSilentMessage: frame.isSilentMessage || undefined,
    reasoningContent: frame.reasoningContent ?? null,
    reasoningSegments: frame.reasoningSegments ?? null,
    intermediate,
  };
}

function carryStoryteller(s: ChatStreamState, event: StorytellerStreamEvent): ChatStreamState {
  return { ...s, storyteller: event, storytellerSeq: s.storytellerSeq + 1 };
}

/** Fold a whole parsed-frame sequence into a final state (test/util convenience). */
export function foldChatFrames(
  frames: ChatStreamFrame[],
  initial: ChatStreamState = initialChatStreamState(),
): ChatStreamState {
  return frames.reduce(reduceChatFrame, initial);
}

// ---------------------------------------------------------------------------
// done handling
// ---------------------------------------------------------------------------

/**
 * The four legacy booleans a `done.outcome` stands for — v4's
 * `legacyOutcomeFlags` (`stream-frames.ts:515-524`): a bare yield is also a
 * skip. The ONE place {@link FinalDoneInfo}'s flags are derived.
 */
export function outcomeFlags(
  outcome: DoneOutcome,
): Pick<FinalDoneInfo, 'skipped' | 'yielded' | 'stopped' | 'emptyResponse'> {
  return {
    skipped: outcome === 'skipped' || outcome === 'yielded',
    yielded: outcome === 'yielded',
    stopped: outcome === 'stopped',
    emptyResponse: outcome === 'empty',
  };
}

function reduceDone(prev: ChatStreamState, frame: DoneFrame): ChatStreamState {
  const finalDone: FinalDoneInfo = {
    outcome: frame.outcome,
    messageId: frame.messageId,
    provider: frame.provider ?? null,
    modelName: frame.modelName ?? null,
    turn: frame.turn ?? null,
    skippedParticipantId: frame.skippedParticipantId ?? null,
    emptyResponseReason: frame.emptyResponseReason ?? null,
    pendingExternalTurn: frame.pendingExternalTurn ?? false,
    toolsExecuted: frame.toolsExecuted,
    attachmentResults: frame.attachmentResults,
    ...outcomeFlags(frame.outcome),
  };
  let s: ChatStreamState = { ...prev, finalDone };

  // A pass, a bare yield or a hard stop before any prose: the Host note (if
  // any) already surfaced via a posted frame. Reset without a bubble.
  if (doneLeavesNoReply(frame.outcome)) {
    return { ...s, content: '', reasoning: '', toolBatches: [], streaming: false };
  }

  // Empty response: no bubble; the send path also stops waiting.
  if (frame.outcome === 'empty') {
    return { ...s, content: '', reasoning: '', toolBatches: [], streaming: false, waitingForResponse: false };
  }

  // The Courier: this closes a parked placeholder turn — no bubble; the
  // placeholder arrives with the refetch.
  if (frame.pendingExternalTurn) {
    return {
      ...s,
      content: '',
      reasoning: '',
      toolBatches: [],
      streaming: false,
      waitingForResponse: false,
      respondingParticipantId: null,
    };
  }

  // Normal completion — emit the assistant bubble. Intermediate (chained) dones
  // skip an empty bubble (v4 `if (!fullContent) return`); the final done emits
  // whatever content there is against the server messageId.
  const content = prev.content;
  const emitBubble = prev.inChain ? content.trim().length > 0 : true;
  if (emitBubble && frame.messageId) {
    const assistant: StreamMessage = {
      ...assistantFromDone(frame, content, frame.participantId ?? prev.respondingParticipantId ?? null, prev.inChain),
      provider: frame.provider ?? null,
      modelName: frame.modelName ?? null,
    };
    s = { ...s, messages: dedupePush(prev.messages, assistant) };
  }

  return { ...s, content: '', reasoning: '', toolBatches: [], streaming: false };
}

// ---------------------------------------------------------------------------
// === P4.D384 === the legacy bag fold (the Brahma console)
// ---------------------------------------------------------------------------

/**
 * The LEGACY fold (P4.D384 ruling P-2) — the pre-`604a082c4` presence-based
 * body, unchanged but for its name: the Brahma console's stream is still the
 * untagged bag (v4 `brahma-console/orchestrator.service.ts:125` —
 * `legacyFrameEncoder`; read by v4's own `useBrahmaConsoleStreaming.ts`, whose
 * role this plays). Each independent `if` branch matches v4's old
 * `readSSEStream`, so a single bag with multiple keys applies each in order.
 * Never fed a Salon frame.
 */
export function reduceLegacyChatFrame(prev: ChatStreamState, frame: LegacyStreamBag): ChatStreamState {
  let s = prev;

  // === P4.D344: storyteller board frames — handed to the board, nothing else
  // folds from them (v4 `useSSEStreaming.ts:705-717`, three early returns) ===
  if (frame.storytellerRound) {
    return carryStoryteller(s, { kind: 'round', round: frame.storytellerRound });
  }
  if (frame.storytellerBid) {
    return carryStoryteller(s, { kind: 'bid', ...frame.storytellerBid });
  }
  if (frame.roundOpen && frame.roundId) {
    return carryStoryteller(s, { kind: 'open', roundId: frame.roundId });
  }
  // Storyteller "all at once": a frame from one of several concurrent turns.
  // Each participant gets its own bubble; the shared streaming slot is left
  // alone (v4 `:722-760`).
  if (frame.concurrent && frame.participantId) {
    return reduceLegacyConcurrent(s, frame, frame.participantId);
  }
  // === end P4.D344 ===

  // status
  if (frame.status) {
    s = { ...s, status: frame.status };
  }

  // content — incremental append; flip streaming on the first chunk
  if (frame.content) {
    const content = s.content + frame.content;
    s = {
      ...s,
      content,
      waitingForResponse: s.streaming ? s.waitingForResponse : false,
      streaming: true,
    };
  }

  // reasoning — cumulative, so REPLACE the buffer
  if (typeof frame.reasoning === 'string') {
    s = { ...s, reasoning: frame.reasoning };
  }

  // transport / stream error — record and halt (v4 throws out of the loop)
  if (frame.error) {
    const message = frame.details ? `${frame.error}: ${frame.details}` : frame.error;
    const errorSubmission: NonNullable<ChatStreamState['errorSubmission']> = {};
    if (typeof frame.userMessagePersisted === 'boolean') {
      errorSubmission.userMessagePersisted = frame.userMessagePersisted;
    }
    if (typeof frame.pendingToolResultsPersisted === 'boolean') {
      errorSubmission.pendingToolResultsPersisted = frame.pendingToolResultsPersisted;
    }
    s = { ...s, error: message, errorSubmission, status: null, streaming: false, waitingForResponse: false };
  }

  // tool detection — push a batch tagged with the current prose offset
  if (frame.toolsDetected) {
    const names = frame.toolNames ?? [];
    const args = frame.toolArguments ?? [];
    const seq = s.toolBatchSeq;
    const calls: PendingToolCall[] = names.map((name, idx) => ({
      id: `tool-${seq}-${idx}`,
      name,
      status: 'pending',
      arguments: args[idx],
    }));
    s = {
      ...s,
      toolBatchSeq: seq + 1,
      toolBatches: [...s.toolBatches, { offset: s.content.length, calls }],
    };
  }

  // tool result — mark the matching call on the MOST RECENT batch
  if (frame.toolResult) {
    s = { ...s, toolBatches: applyToolResult(s.toolBatches, frame.toolResult) };
  }

  // a Carina reference answer surfaced mid-turn — insert, deduped by id
  if (frame.carinaAnswer) {
    s = { ...s, messages: dedupePush(s.messages, { kind: 'carina', id: frame.carinaAnswer.id, message: frame.carinaAnswer }) };
  }

  // a Host announcement (turn-pass note) surfaced mid-turn — insert, deduped by id
  if (frame.hostAnnouncement) {
    s = { ...s, messages: dedupePush(s.messages, { kind: 'host', id: frame.hostAnnouncement.id, message: frame.hostAnnouncement }) };
  }

  // the raised hand has been answered — carried, not toasted here (R-5)
  if (frame.handAnswered) {
    s = {
      ...s,
      handAnswered: {
        participantId: frame.participantId ?? null,
        characterName: frame.characterName ?? null,
        yielded: frame.yielded === true,
      },
      handAnsweredSeq: s.handAnsweredSeq + 1,
    };
  }

  // a Pascal custom-tool outcome (an LLM's run_custom reach) surfaced mid-turn —
  // insert, deduped by id, and render live like carina/host (§4).
  if (frame.pascalResult) {
    s = { ...s, messages: dedupePush(s.messages, { kind: 'pascal', id: frame.pascalResult.id, message: frame.pascalResult }) };
  }

  // an answer-confirmation result — patch the bubble (and, on a revision, its content)
  if (frame.confirmationResult) {
    s = { ...s, messages: applyConfirmation(s.messages, frame.confirmationResult) };
  }

  // completion
  if (frame.done) {
    s = reduceLegacyDone(s, frame);
  }

  // the Courier parked a placeholder for a manual / clipboard turn
  if (frame.pendingExternalTurn && !frame.done) {
    s = { ...s, finalDone: { ...(s.finalDone ?? emptyDone()), pendingExternalTurn: true } };
  }

  // turn start (a character is about to respond) — reset the buffer. Only a
  // CHAINED turn (depth > 0) enters a chain: the server also sends a depth-0
  // turnStart before every first turn (and inside every chained turn's
  // re-entered process_message) so the bubble shows the right responder, and
  // that one must leave the turn's done on the ordinary path. v4 never CLEARS
  // `inChain` on depth 0, it only declines to set it (bug 242, `b045dd986`,
  // `useSSEStreaming.ts:756-762`).
  if (frame.turnStart) {
    s = {
      ...s,
      inChain: (frame.chainDepth ?? 0) > 0 ? true : s.inChain,
      content: '',
      reasoning: '',
      toolBatches: [],
      streaming: false,
      waitingForResponse: true,
      respondingParticipantId: frame.participantId ?? s.respondingParticipantId,
    };
  }

  // turn complete (a chained character finished) — reset without ending the chain
  if (frame.turnComplete) {
    s = { ...s, content: '', reasoning: '', streaming: false, waitingForResponse: false };
  }

  // chain complete (all chained turns done) — terminal
  if (frame.chainComplete) {
    s = {
      ...s,
      inChain: false,
      content: '',
      reasoning: '',
      streaming: false,
      waitingForResponse: false,
      respondingParticipantId: null,
      finished: true,
      // v4 `useSSEStreaming.ts:686-691` builds the onChainComplete event here.
      // v5's fold is pure and the announcement is a side effect, so the two
      // fields it needs ride the state to the vertical instead of a callback.
      chainReason: frame.reason ?? null,
      chainPaused: frame.paused === true,
      chainHeldUserTurn: frame.heldUserTurn === true,
    };
  }

  return s;
}

function reduceLegacyConcurrent(s: ChatStreamState, frame: LegacyStreamBag, pid: string): ChatStreamState {
  const has = s.concurrentStreams.some((c) => c.participantId === pid);
  if (frame.turnStart) {
    return {
      ...s,
      waitingForResponse: false,
      concurrentStreams: has ? s.concurrentStreams : [...s.concurrentStreams, { participantId: pid, content: '' }],
    };
  }
  if (frame.content) {
    const text = frame.content;
    return {
      ...s,
      concurrentStreams: has
        ? s.concurrentStreams.map((c) => (c.participantId === pid ? { ...c, content: c.content + text } : c))
        : [...s.concurrentStreams, { participantId: pid, content: text }],
    };
  }
  if (frame.done) {
    const finished = s.concurrentStreams.find((c) => c.participantId === pid)?.content ?? '';
    let messages = s.messages;
    if (frame.messageId && !frame.skipped && !frame.stopped) {
      messages = dedupePush(messages, {
        kind: 'assistant',
        id: frame.messageId,
        content: finished,
        participantId: pid,
        provider: frame.provider || null,
        modelName: frame.modelName || null,
        routeTrail: frame.routeTrail ?? null,
        isSilentMessage: frame.isSilentMessage || undefined,
        reasoningContent: frame.reasoningContent ?? null,
        reasoningSegments: frame.reasoningSegments ?? null,
        intermediate: true,
      });
    }
    return {
      ...s,
      messages,
      concurrentStreams: s.concurrentStreams.filter((c) => c.participantId !== pid),
    };
  }
  const posted = frame.carinaAnswer
    ? ({ kind: 'carina', message: frame.carinaAnswer } as const)
    : frame.hostAnnouncement
      ? ({ kind: 'host', message: frame.hostAnnouncement } as const)
      : frame.pascalResult
        ? ({ kind: 'pascal', message: frame.pascalResult } as const)
        : null;
  if (posted) {
    return { ...s, messages: dedupePush(s.messages, { kind: posted.kind, id: posted.message.id, message: posted.message }) };
  }
  if (frame.error) {
    // v4 toasts it and reads on; v5 records it for the run's one error toast.
    return { ...s, error: frame.error };
  }
  return s;
}

function reduceLegacyDone(prev: ChatStreamState, frame: LegacyStreamBag): ChatStreamState {
  const finalDone: FinalDoneInfo = {
    outcome: legacyOutcomeOf(frame),
    messageId: frame.messageId ?? null,
    provider: frame.provider ?? null,
    modelName: frame.modelName ?? null,
    turn: frame.turn ?? null,
    skipped: frame.skipped ?? false,
    skippedParticipantId: frame.skippedParticipantId ?? null,
    yielded: frame.yielded ?? false,
    emptyResponse: frame.emptyResponse ?? false,
    emptyResponseReason: frame.emptyResponseReason ?? null,
    pendingExternalTurn: frame.pendingExternalTurn ?? false,
    toolsExecuted: frame.toolsExecuted ?? false,
    stopped: frame.stopped ?? false,
    attachmentResults: frame.attachmentResults ?? null,
  };
  let s: ChatStreamState = { ...prev, finalDone };

  // "Nothing to add" turn-pass: the Host note already surfaced via
  // hostAnnouncement. Reset without appending a bubble.
  // A hard stop before any prose is the same silent reset (v4 `done.skipped || done.stopped`).
  if (frame.skipped || frame.stopped) {
    return { ...s, content: '', reasoning: '', toolBatches: [], streaming: false };
  }

  // Empty response: no bubble; the send path also stops waiting.
  if (frame.emptyResponse) {
    return {
      ...s,
      content: '',
      reasoning: '',
      toolBatches: [],
      streaming: false,
      waitingForResponse: false,
    };
  }

  // Courier placeholder: the pendingExternalTurn frame already ran; don't emit a bubble.
  if (frame.pendingExternalTurn) {
    return {
      ...s,
      content: '',
      reasoning: '',
      toolBatches: [],
      streaming: false,
      waitingForResponse: false,
      respondingParticipantId: null,
    };
  }

  // Normal completion — emit the assistant bubble. Intermediate (chained) dones
  // skip an empty bubble (v4 `if (!fullContent) return`); the final done emits
  // whatever content there is against the server messageId.
  const content = prev.content;
  const emitBubble = prev.inChain ? content.trim().length > 0 : true;
  if (emitBubble && frame.messageId) {
    const assistant: StreamMessage = {
      kind: 'assistant',
      id: frame.messageId,
      content,
      participantId: frame.participantId ?? prev.respondingParticipantId ?? null,
      provider: frame.provider ?? null,
      modelName: frame.modelName ?? null,
      routeTrail: frame.routeTrail ?? null,
      isSilentMessage: frame.isSilentMessage || undefined,
      reasoningContent: frame.reasoningContent ?? null,
      reasoningSegments: frame.reasoningSegments ?? null,
      intermediate: prev.inChain,
    };
    s = { ...s, messages: dedupePush(prev.messages, assistant) };
  }

  return {
    ...s,
    content: '',
    reasoning: '',
    toolBatches: [],
    streaming: false,
  };
}

/** The outcome a legacy bag's flags stand for (the inverse of {@link outcomeFlags}). */
function legacyOutcomeOf(frame: LegacyStreamBag): DoneOutcome {
  if (frame.stopped) return 'stopped';
  if (frame.yielded) return 'yielded';
  if (frame.skipped) return 'skipped';
  if (frame.emptyResponse) return 'empty';
  return 'finished';
}
// === end P4.D384 ===

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

function applyToolResult(
  batches: StreamingToolBatch[],
  result: NonNullable<LegacyStreamBag['toolResult']>,
): StreamingToolBatch[] {
  if (batches.length === 0) {
    return batches;
  }
  const lastIdx = batches.length - 1;
  const last = batches[lastIdx];
  const { index, name, success } = result;
  const calls = last.calls.map((tc, idx) =>
    (index !== undefined && idx === index) || (index === undefined && tc.name === name)
      ? {
          ...tc,
          status: success ? ('success' as const) : ('error' as const),
          result: result.result,
          // The sibling `error`, not `result.error` — see `PendingToolCall`.
          errorText: result.error,
        }
      : tc,
  );
  const next = batches.slice();
  next[lastIdx] = { ...last, calls };
  return next;
}

function dedupePush(messages: StreamMessage[], message: StreamMessage): StreamMessage[] {
  return messages.some((m) => m.id === message.id) ? messages : [...messages, message];
}

function applyConfirmation(
  messages: StreamMessage[],
  result: NonNullable<LegacyStreamBag['confirmationResult']>,
): StreamMessage[] {
  return messages.map((m) =>
    m.kind === 'assistant' && m.id === result.messageId
      ? {
          ...m,
          confirmed: result.confirmed,
          confirmationChecked: true,
          confirmationRevised: result.revised,
          confirmationNotes: result.notes,
          ...(typeof result.content === 'string' ? { content: result.content } : {}),
        }
      : m,
  );
}

function emptyDone(): FinalDoneInfo {
  return {
    outcome: 'finished',
    messageId: null,
    provider: null,
    modelName: null,
    turn: null,
    skipped: false,
    skippedParticipantId: null,
    yielded: false,
    emptyResponse: false,
    emptyResponseReason: null,
    pendingExternalTurn: false,
    toolsExecuted: false,
    stopped: false,
    attachmentResults: null,
  };
}
