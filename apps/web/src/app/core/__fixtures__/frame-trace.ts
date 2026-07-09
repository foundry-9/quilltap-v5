/**
 * Committed frame-trace fixture for the SSE stream reducer tests. Authored by
 * hand from the REAL event vocabulary — `crates/quilltap-core/src/services/
 * chat_events.rs` (which serializes byte-identical to v4's SSE frames) and the
 * orchestrator/native-loop orderings the tier-3 differentials exercise.
 *
 * Every entry is a `ScopedEvent` exactly as it arrives on `GET /api/events`:
 * a bare single-key (or flat-multi-key) chat frame plus its `chatId` scope tag.
 * The traces cover: content+reasoning interleave, a tool batch spliced at an
 * anchor, a multi-turn chain, a "nothing to add" skip done, a mid-stream error,
 * and interleaved OTHER-chat frames (to prove scope filtering).
 */

import type { ScopedEvent } from '../core-contract';

const CHAT = 'chat-1';
const OTHER = 'chat-2';

/** A single turn: status → interleaved reasoning + content → done with a bubble. */
export const singleTurnTrace: ScopedEvent[] = [
  { chatId: CHAT, status: { stage: 'sending', message: 'Sending to Friday…', characterName: 'Friday', characterId: 'c1' } },
  { chatId: CHAT, status: { stage: 'streaming', message: 'Friday is composing a reply…' } },
  { chatId: CHAT, reasoning: 'Let me think' },
  { chatId: CHAT, reasoning: 'Let me think about cats' },
  { chatId: CHAT, content: 'Cats ' },
  // An interleaved frame for ANOTHER chat that the consumer must filter out.
  { chatId: OTHER, content: 'IGNORED — different chat' },
  { chatId: CHAT, content: 'are ' },
  { chatId: CHAT, content: 'wonderful.' },
  {
    chatId: CHAT,
    done: true,
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
  },
];

/** A tool batch spliced at the prose offset where the model paused to call it. */
export const toolBatchTrace: ScopedEvent[] = [
  { chatId: CHAT, content: 'Let me look ' },
  { chatId: CHAT, toolsDetected: 1, toolNames: ['search'], toolArguments: [{ query: 'cats' }] },
  { chatId: CHAT, toolResult: { index: 0, name: 'search', success: true, result: { hits: 3 } } },
  { chatId: CHAT, content: 'into that for you.' },
  { chatId: CHAT, done: true, messageId: 'm2', participantId: 'p1', toolsExecuted: true, provider: 'ANTHROPIC', modelName: 'claude-sonnet-5' },
];

/** A two-character chain: turnStart / content / intermediate done / turnComplete ×2, then chainComplete. */
export const multiTurnChainTrace: ScopedEvent[] = [
  { chatId: CHAT, turnStart: true, participantId: 'p1', characterName: 'Ada', chainDepth: 1 },
  { chatId: CHAT, content: 'Hi, I am Ada.' },
  { chatId: CHAT, done: true, messageId: 'm10', participantId: 'p1', provider: 'ANTHROPIC', modelName: 'claude-sonnet-5', toolsExecuted: false },
  { chatId: CHAT, turnComplete: true, participantId: 'p1', messageId: 'm10', chainDepth: 1, skipped: false },
  { chatId: CHAT, turnStart: true, participantId: 'p2', characterName: 'Bob', chainDepth: 2 },
  { chatId: CHAT, content: 'And I am Bob.' },
  { chatId: CHAT, done: true, messageId: 'm11', participantId: 'p2', provider: 'ANTHROPIC', modelName: 'claude-sonnet-5', toolsExecuted: false },
  { chatId: CHAT, turnComplete: true, participantId: 'p2', messageId: 'm11', chainDepth: 2, skipped: false },
  { chatId: CHAT, chainComplete: true, reason: 'cycle_complete', nextSpeakerId: null, chainDepth: 2 },
];

/** A "nothing to add" turn-pass: a Host announcement then a skip done (no bubble). */
export const skipDoneTrace: ScopedEvent[] = [
  { chatId: CHAT, turnStart: true, participantId: 'p3', characterName: 'Cyd', chainDepth: 1 },
  {
    chatId: CHAT,
    hostAnnouncement: { id: 'host-1', type: 'message', systemSender: 'host', content: 'Cyd has nothing to add.' },
  },
  {
    chatId: CHAT,
    done: true,
    messageId: null,
    participantId: 'p3',
    toolsExecuted: false,
    skipped: true,
    skippedParticipantId: 'p3',
    provider: 'ANTHROPIC',
    modelName: 'claude-sonnet-5',
  },
];

/** A mid-stream transport error after partial content. */
export const midStreamErrorTrace: ScopedEvent[] = [
  { chatId: CHAT, status: { stage: 'streaming', message: 'Composing…' } },
  { chatId: CHAT, content: 'Partial answer…' },
  { chatId: CHAT, error: 'Failed to generate response', errorType: 'stream_error', details: 'boom' },
];

export const FIXTURE_CHAT_ID = CHAT;
export const FIXTURE_OTHER_CHAT_ID = OTHER;
