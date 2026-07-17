/**
 * The SSE stream reducer — a pure fold of chat-scoped `ChatStreamFrame`s into
 * per-chat streaming view state. This is the v5 port of v4's client hooks
 * `app/salon/[id]/hooks/useSSEStreaming.ts` + `useMessageStreaming.ts`; the
 * union semantics below are lifted from `readSSEStream`.
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
 *  - `toolResult` — mark the matching call on the MOST RECENT batch.
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
  ChatStreamFrame,
  ReasoningSegment,
  ResponseStatus,
  NextSpeakerInfo,
  PostedMessage,
} from './core-contract';

/** One in-progress tool call within a batch. */
export interface PendingToolCall {
  id: string;
  name: string;
  status: 'pending' | 'success' | 'error';
  result?: unknown;
  arguments?: Record<string, unknown>;
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
    }
  | { kind: 'carina'; id: string; message: PostedMessage }
  | { kind: 'host'; id: string; message: PostedMessage }
  | { kind: 'pascal'; id: string; message: PostedMessage };

/** The terminal `done` info the consumer reconciles against the dispatch reply. */
export interface FinalDoneInfo {
  messageId: string | null;
  provider: string | null;
  modelName: string | null;
  turn: NextSpeakerInfo | null;
  skipped: boolean;
  skippedParticipantId: string | null;
  emptyResponse: boolean;
  emptyResponseReason: string | null;
  pendingExternalTurn: boolean;
  toolsExecuted: boolean;
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
  /** The terminal chain has completed (chainComplete, or a non-chained final done). */
  finished: boolean;
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
    finished: false,
    finalDone: null,
    toolBatchSeq: 0,
  };
}

/**
 * Fold ONE frame into the state, returning a new state (pure). Mirrors the body
 * of v4's `readSSEStream` line loop — each independent `if` branch below matches
 * v4's, so a single frame with multiple keys applies each in v4's order.
 */
export function reduceChatFrame(prev: ChatStreamState, frame: ChatStreamFrame): ChatStreamState {
  let s = prev;

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
    s = { ...s, error: message, status: null, streaming: false, waitingForResponse: false };
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
    s = reduceDone(s, frame);
  }

  // the Courier parked a placeholder for a manual / clipboard turn
  if (frame.pendingExternalTurn && !frame.done) {
    s = { ...s, finalDone: { ...(s.finalDone ?? emptyDone()), pendingExternalTurn: true } };
  }

  // turn start (a chained character is about to respond) — reset the buffer
  if (frame.turnStart) {
    s = {
      ...s,
      inChain: true,
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
    };
  }

  return s;
}

/** Fold a whole frame sequence into a final state (test/util convenience). */
export function foldChatFrames(
  frames: ChatStreamFrame[],
  initial: ChatStreamState = initialChatStreamState(),
): ChatStreamState {
  return frames.reduce(reduceChatFrame, initial);
}

// ---------------------------------------------------------------------------
// done handling
// ---------------------------------------------------------------------------

function reduceDone(prev: ChatStreamState, frame: ChatStreamFrame): ChatStreamState {
  const finalDone: FinalDoneInfo = {
    messageId: frame.messageId ?? null,
    provider: frame.provider ?? null,
    modelName: frame.modelName ?? null,
    turn: frame.turn ?? null,
    skipped: frame.skipped ?? false,
    skippedParticipantId: frame.skippedParticipantId ?? null,
    emptyResponse: frame.emptyResponse ?? false,
    emptyResponseReason: frame.emptyResponseReason ?? null,
    pendingExternalTurn: frame.pendingExternalTurn ?? false,
    toolsExecuted: frame.toolsExecuted ?? false,
  };
  let s: ChatStreamState = { ...prev, finalDone };

  // "Nothing to add" turn-pass: the Host note already surfaced via
  // hostAnnouncement. Reset without appending a bubble.
  if (frame.skipped) {
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

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

function applyToolResult(
  batches: StreamingToolBatch[],
  result: NonNullable<ChatStreamFrame['toolResult']>,
): StreamingToolBatch[] {
  if (batches.length === 0) {
    return batches;
  }
  const lastIdx = batches.length - 1;
  const last = batches[lastIdx];
  const { index, name, success } = result;
  const calls = last.calls.map((tc, idx) =>
    (index !== undefined && idx === index) || (index === undefined && tc.name === name)
      ? { ...tc, status: success ? ('success' as const) : ('error' as const), result: result.result }
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
  result: NonNullable<ChatStreamFrame['confirmationResult']>,
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
    messageId: null,
    provider: null,
    modelName: null,
    turn: null,
    skipped: false,
    skippedParticipantId: null,
    emptyResponse: false,
    emptyResponseReason: null,
    pendingExternalTurn: false,
    toolsExecuted: false,
  };
}
