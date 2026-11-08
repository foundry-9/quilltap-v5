/**
 * Committed frame-trace fixture for the SSE stream reducer tests. Authored by
 * hand from the REAL event vocabulary — `crates/quilltap-core/src/services/
 * chat_events.rs` (which serializes byte-identical to v4's SSE frames) and the
 * orchestrator/native-loop orderings the tier-3 differentials exercise.
 *
 * ⚠ P4.D301 (bug 242): until then the single-turn trace had NO depth-0
 * `turnStart` and the chain trace opened at `chainDepth: 1`, though the server
 * sends a depth-0 `turnStart` from inside EVERY `process_message` call
 * (`services/orchestrator.rs`, "turnStart (chainDepth 0) + status" — v4
 * `orchestrator.service.ts:410`, after the `initializing` status): before a
 * first turn, AND right after the chain's own depth-N `turnStart` for each
 * chained turn, because the chain re-enters `process_message`. Every reducer
 * spec that used the old traces folded a shape the server never sends, which
 * is exactly how the pre-fix `inChain` survived unnoticed.
 *
 * Every entry is a {@link WireFrame} exactly as it arrives on `GET /api/events`
 * since P4.D384 (v4 `604a082c4`): a `kind`-tagged Salon frame (contract C3)
 * with its `chatId` scope tag first. The traces were re-expressed from the old
 * untagged bags one kind per frame; `done` frames carry v4's required keys
 * (`outcome`, `messageId`, `usage`, `cacheUsage`, `attachmentResults`,
 * `toolsExecuted`) — v4's parser drops a `done` without them.
 *
 * {@link goldenStream} adds v4's own six golden streams
 * (`__tests__/fixtures/salon-stream/*.legacy.json` through v4's
 * `loadFrameStream`), taken from the stream-frames oracle corpus's section (d)
 * — the frames v4's `useSSEStreaming-golden.test.tsx` replays.
 * The traces cover: content+reasoning interleave, a tool batch spliced at an
 * anchor, a multi-turn chain, a "nothing to add" skip done, a mid-stream error,
 * and interleaved OTHER-chat frames (to prove scope filtering).
 */

import type { ChatStreamFrame, DoneFrame, ScopeEnvelope } from '../core-contract';
import corpusText from '../../../testing/fixtures/stream-frames.oracle.ndjson';

/** A Salon frame on the wire: the scope envelope, then the frame. */
export type WireFrame = ScopeEnvelope & ChatStreamFrame;

/** v4's `doneFrame` test helper: a complete `done`, an ordinary finish unless `fields` says otherwise. */
export function doneFrame(fields: Partial<Omit<DoneFrame, 'kind'>> = {}): DoneFrame {
  return {
    kind: 'done',
    outcome: 'finished',
    messageId: null,
    usage: null,
    cacheUsage: null,
    attachmentResults: null,
    toolsExecuted: false,
    ...fields,
  };
}

/** v4's six golden streams (`GOLDEN_STREAMS`). */
export type GoldenStream =
  | 'plain-reply'
  | 'tool-reply'
  | 'storyteller-concurrent-hand'
  | 'courier-external'
  | 'failures'
  | 'swipe';

/** One golden stream's frames, in order, exactly as v4's `parseFrame` returned them. */
export function goldenStream(name: GoldenStream): ChatStreamFrame[] {
  return (corpusText as unknown as string)
    .split('\n')
    .filter((line) => line.trim() !== '')
    .map((line) => JSON.parse(line) as { section: string; id: string; out: ChatStreamFrame })
    .filter((r) => r.section === 'golden' && r.id.startsWith(`${name}#`))
    .map((r) => r.out);
}

const CHAT = 'chat-1';
const OTHER = 'chat-2';

/**
 * A single turn, in the server's real order: the `initializing` status, the
 * depth-0 `turnStart` naming the responder, the statuses, interleaved
 * reasoning + content, then done with a bubble.
 */
export const singleTurnTrace: WireFrame[] = [
  { chatId: CHAT, kind: 'status', status: { stage: 'initializing', message: 'Loading chat...' } },
  { chatId: CHAT, kind: 'turnStart', participantId: 'p1', characterName: 'Friday', chainDepth: 0 },
  { chatId: CHAT, kind: 'status', status: { stage: 'sending', message: 'Sending to Friday…', characterName: 'Friday', characterId: 'c1' } },
  { chatId: CHAT, kind: 'status', status: { stage: 'streaming', message: 'Friday is composing a reply…' } },
  { chatId: CHAT, kind: 'reasoning', text: 'Let me think' },
  { chatId: CHAT, kind: 'reasoning', text: 'Let me think about cats' },
  { chatId: CHAT, kind: 'content', text: 'Cats ' },
  // An interleaved frame for ANOTHER chat that the consumer must filter out.
  { chatId: OTHER, kind: 'content', text: 'IGNORED — different chat' },
  { chatId: CHAT, kind: 'content', text: 'are ' },
  { chatId: CHAT, kind: 'content', text: 'wonderful.' },
  {
    chatId: CHAT,
    ...doneFrame({
    messageId: 'm1',
    participantId: 'p1',
    usage: { promptTokens: 10, completionTokens: 20, totalTokens: 30 },
    cacheUsage: null,
    toolsExecuted: false,
    turn: { nextSpeakerId: null, reason: 'user_turn', cycleComplete: true, isUsersTurn: true },
    provider: 'ANTHROPIC',
    modelName: 'claude-sonnet-5',
    reasoningContent: 'Let me think about cats',
    reasoningSegments: null,
    }),
  },
];

/** A tool batch spliced at the prose offset where the model paused to call it. */
export const toolBatchTrace: WireFrame[] = [
  { chatId: CHAT, kind: 'content', text: 'Let me look ' },
  { chatId: CHAT, kind: 'toolsDetected', names: ['search'], arguments: [{ query: 'cats' }] },
  { chatId: CHAT, kind: 'toolResult', index: 0, name: 'search', success: true, result: { hits: 3 } },
  { chatId: CHAT, kind: 'content', text: 'into that for you.' },
  { chatId: CHAT, ...doneFrame({ messageId: 'm2', participantId: 'p1', toolsExecuted: true, provider: 'ANTHROPIC', modelName: 'claude-sonnet-5' }) },
];

/**
 * A first turn and a two-character chain behind it. The first turn is the
 * user's send (depth 0 — NOT in a chain: its done is a final bubble); each
 * chained turn opens with the chain's depth-N `turnStart` and then the depth-0
 * one its re-entered `process_message` sends (which must not take the chain
 * back out), content, an intermediate done, `turnComplete`; then chainComplete.
 */
export const multiTurnChainTrace: WireFrame[] = [
  { chatId: CHAT, kind: 'turnStart', participantId: 'p0', characterName: 'Friday', chainDepth: 0 },
  { chatId: CHAT, kind: 'content', text: 'Shall we begin?' },
  { chatId: CHAT, ...doneFrame({ messageId: 'm9', participantId: 'p0', provider: 'ANTHROPIC', modelName: 'claude-sonnet-5', toolsExecuted: false }) },
  { chatId: CHAT, kind: 'turnStart', participantId: 'p1', characterName: 'Ada', chainDepth: 1 },
  { chatId: CHAT, kind: 'turnStart', participantId: 'p1', characterName: 'Ada', chainDepth: 0 },
  { chatId: CHAT, kind: 'content', text: 'Hi, I am Ada.' },
  { chatId: CHAT, ...doneFrame({ messageId: 'm10', participantId: 'p1', provider: 'ANTHROPIC', modelName: 'claude-sonnet-5', toolsExecuted: false }) },
  { chatId: CHAT, kind: 'turnComplete', participantId: 'p1', messageId: 'm10', chainDepth: 1, skipped: false },
  { chatId: CHAT, kind: 'turnStart', participantId: 'p2', characterName: 'Bob', chainDepth: 2 },
  { chatId: CHAT, kind: 'turnStart', participantId: 'p2', characterName: 'Bob', chainDepth: 0 },
  { chatId: CHAT, kind: 'content', text: 'And I am Bob.' },
  { chatId: CHAT, ...doneFrame({ messageId: 'm11', participantId: 'p2', provider: 'ANTHROPIC', modelName: 'claude-sonnet-5', toolsExecuted: false }) },
  { chatId: CHAT, kind: 'turnComplete', participantId: 'p2', messageId: 'm11', chainDepth: 2, skipped: false },
  { chatId: CHAT, kind: 'chainComplete', reason: 'cycle_complete', nextSpeakerId: null, chainDepth: 2 },
];

/** A "nothing to add" turn-pass: a Host announcement then a skip done (no bubble). */
export const skipDoneTrace: WireFrame[] = [
  { chatId: CHAT, kind: 'turnStart', participantId: 'p3', characterName: 'Cyd', chainDepth: 1 },
  { chatId: CHAT, kind: 'turnStart', participantId: 'p3', characterName: 'Cyd', chainDepth: 0 },
  {
    chatId: CHAT,
    kind: 'posted',
    what: 'hostAnnouncement',
    message: { id: 'host-1', type: 'message', systemSender: 'host', content: 'Cyd has nothing to add.' },
  },
  {
    chatId: CHAT,
    ...doneFrame({
      outcome: 'skipped',
      messageId: null,
      participantId: 'p3',
      toolsExecuted: false,
      skippedParticipantId: 'p3',
      provider: 'ANTHROPIC',
      modelName: 'claude-sonnet-5',
    }),
  },
];

/** A mid-stream transport error after partial content. */
export const midStreamErrorTrace: WireFrame[] = [
  { chatId: CHAT, kind: 'status', status: { stage: 'streaming', message: 'Composing…' } },
  { chatId: CHAT, kind: 'content', text: 'Partial answer…' },
  { chatId: CHAT, kind: 'error', message: 'Failed to generate response', errorType: 'stream_error', details: 'boom' },
];

export const FIXTURE_CHAT_ID = CHAT;
export const FIXTURE_OTHER_CHAT_ID = OTHER;
