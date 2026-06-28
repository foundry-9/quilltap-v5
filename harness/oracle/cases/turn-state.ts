/**
 * Oracle case #13 (Wave 2 / B5): the turn-state machine.
 *
 * Drives REAL pure functions from v4's lib/chat/turn-manager/queue.ts,
 * state.ts, and utils.ts (getQueuePosition): the queue ops, history-derived
 * state, the after-message/after-skip update, and the spoken-this-cycle wrap.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/turn-state.ts \
 *     > /tmp/oracle-turn-state.ndjson
 */

import {
  addToQueue,
  removeFromQueue,
  popFromQueue,
  nudgeParticipant,
  resetCycleForUserSkip,
} from '@/lib/chat/turn-manager/queue';
import {
  createInitialTurnState,
  calculateTurnStateFromHistory,
  updateTurnStateAfterMessage,
  computeSpokenThisCycleAfterMessage,
  computeSpokenThisCycleAfterSkip,
} from '@/lib/chat/turn-manager/state';
import { getQueuePosition } from '@/lib/chat/turn-manager/utils';
import type { TurnState } from '@/lib/chat/turn-manager/types';
import type { ChatParticipantBase, ChatEvent, MessageEvent } from '@/lib/schemas/types';

type WireState = {
  spokenSinceUserTurn: string[];
  currentTurnParticipantId: string | null;
  queue: string[];
  lastSpeakerId: string | null;
};
const st = (
  spokenSinceUserTurn: string[],
  currentTurnParticipantId: string | null,
  queue: string[],
  lastSpeakerId: string | null,
): TurnState => ({ spokenSinceUserTurn, currentTurnParticipantId, queue, lastSpeakerId } as TurnState);
const wire = (s: TurnState): WireState => ({
  spokenSinceUserTurn: s.spokenSinceUserTurn,
  currentTurnParticipantId: s.currentTurnParticipantId,
  queue: s.queue,
  lastSpeakerId: s.lastSpeakerId,
});

// Message view: { type?, role, participantId?, targetParticipantIds? }
type WireMsg = { type?: string; role: string; participantId?: string | null; targetParticipantIds?: string[] };
const asMsg = (m: WireMsg) => m as unknown as MessageEvent;
const asEvent = (m: WireMsg) => m as unknown as ChatEvent;

// Participant view: { id, type, status, characterId }
type WirePart = { id: string; type: string; status: string; characterId: string | null };
const asParts = (ps: WirePart[]) => ps as unknown as ChatParticipantBase[];

type Row =
  | { kind: 'queueOp'; id: string; op: string; state: WireState; arg: string | null; out: WireState; popped?: string | null }
  | { kind: 'queuePos'; id: string; state: WireState; participantId: string; out: number }
  | { kind: 'calc'; id: string; messages: WireMsg[]; spokenJson: string | null; out: WireState }
  | { kind: 'update'; id: string; state: WireState; message: WireMsg; out: WireState }
  | { kind: 'computeMsg'; id: string; message: WireMsg; participants: WirePart[]; currentJson: string | null; out: string | null }
  | { kind: 'computeSkip'; id: string; skippedId: string; participants: WirePart[]; currentJson: string | null; out: string | null };

const rows: Row[] = [];

// ---- queue ops -------------------------------------------------------------
const baseQ = st(['A'], null, ['X', 'Y'], 'Z');
rows.push({ kind: 'queueOp', id: 'add-new', op: 'add', state: wire(baseQ), arg: 'W', out: wire(addToQueue(baseQ, 'W')) });
rows.push({ kind: 'queueOp', id: 'add-dup', op: 'add', state: wire(baseQ), arg: 'X', out: wire(addToQueue(baseQ, 'X')) });
rows.push({ kind: 'queueOp', id: 'remove-present', op: 'remove', state: wire(baseQ), arg: 'X', out: wire(removeFromQueue(baseQ, 'X')) });
rows.push({ kind: 'queueOp', id: 'remove-absent', op: 'remove', state: wire(baseQ), arg: 'Q', out: wire(removeFromQueue(baseQ, 'Q')) });
rows.push({ kind: 'queueOp', id: 'nudge-absent', op: 'nudge', state: wire(baseQ), arg: 'W', out: wire(nudgeParticipant(baseQ, 'W')) });
rows.push({ kind: 'queueOp', id: 'nudge-present', op: 'nudge', state: wire(baseQ), arg: 'Y', out: wire(nudgeParticipant(baseQ, 'Y')) });
rows.push({ kind: 'queueOp', id: 'reset-skip', op: 'resetSkip', state: wire(st(['A', 'B'], 'C', ['X'], 'Z')), arg: null, out: wire(resetCycleForUserSkip(st(['A', 'B'], 'C', ['X'], 'Z'))) });
const popNonEmpty = popFromQueue(baseQ);
rows.push({ kind: 'queueOp', id: 'pop-nonempty', op: 'pop', state: wire(baseQ), arg: null, out: wire(popNonEmpty.state), popped: popNonEmpty.participantId });
const emptyQ = st([], null, [], null);
const popEmpty = popFromQueue(emptyQ);
rows.push({ kind: 'queueOp', id: 'pop-empty', op: 'pop', state: wire(emptyQ), arg: null, out: wire(popEmpty.state), popped: popEmpty.participantId });

// ---- getQueuePosition ------------------------------------------------------
rows.push({ kind: 'queuePos', id: 'pos-first', state: wire(baseQ), participantId: 'X', out: getQueuePosition(baseQ, 'X') });
rows.push({ kind: 'queuePos', id: 'pos-second', state: wire(baseQ), participantId: 'Y', out: getQueuePosition(baseQ, 'Y') });
rows.push({ kind: 'queuePos', id: 'pos-absent', state: wire(baseQ), participantId: 'Q', out: getQueuePosition(baseQ, 'Q') });

// ---- calculateTurnStateFromHistory ----------------------------------------
const msgs1: WireMsg[] = [
  { role: 'USER', participantId: 'u1' },
  { role: 'ASSISTANT', participantId: 'c1' },
  { role: 'ASSISTANT', participantId: 'c2', targetParticipantIds: ['u1'] }, // whisper, skipped
];
rows.push({ kind: 'calc', id: 'last-from-history', messages: msgs1, spokenJson: '["c1"]', out: wire(calculateTurnStateFromHistory({ messages: msgs1 as unknown as MessageEvent[], participants: [], userParticipantId: null, spokenThisCycleParticipantIds: '["c1"]' })) });
rows.push({ kind: 'calc', id: 'empty', messages: [], spokenJson: null, out: wire(calculateTurnStateFromHistory({ messages: [], participants: [], userParticipantId: null })) });
rows.push({ kind: 'calc', id: 'bad-json', messages: msgs1, spokenJson: 'not json', out: wire(calculateTurnStateFromHistory({ messages: msgs1 as unknown as MessageEvent[], participants: [], userParticipantId: null, spokenThisCycleParticipantIds: 'not json' })) });
const msgsNoPid: WireMsg[] = [{ role: 'ASSISTANT', participantId: null }, { role: 'SYSTEM', participantId: 'sys' }];
rows.push({ kind: 'calc', id: 'no-eligible', messages: msgsNoPid, spokenJson: '["x","y",123]', out: wire(calculateTurnStateFromHistory({ messages: msgsNoPid as unknown as MessageEvent[], participants: [], userParticipantId: null, spokenThisCycleParticipantIds: '["x","y",123]' })) });

// ---- updateTurnStateAfterMessage ------------------------------------------
const upBase = st(['c1'], 'c2', ['c2', 'c3'], 'c1');
rows.push({ kind: 'update', id: 'normal', state: wire(upBase), message: { role: 'ASSISTANT', participantId: 'c2' }, out: wire(updateTurnStateAfterMessage(upBase, asMsg({ role: 'ASSISTANT', participantId: 'c2' }), null)) });
rows.push({ kind: 'update', id: 'dup-spoken', state: wire(upBase), message: { role: 'ASSISTANT', participantId: 'c1' }, out: wire(updateTurnStateAfterMessage(upBase, asMsg({ role: 'ASSISTANT', participantId: 'c1' }), null)) });
rows.push({ kind: 'update', id: 'whisper-noop', state: wire(upBase), message: { role: 'ASSISTANT', participantId: 'c3', targetParticipantIds: ['c1'] }, out: wire(updateTurnStateAfterMessage(upBase, asMsg({ role: 'ASSISTANT', participantId: 'c3', targetParticipantIds: ['c1'] }), null)) });
rows.push({ kind: 'update', id: 'wrong-role', state: wire(upBase), message: { role: 'SYSTEM', participantId: 'c3' }, out: wire(updateTurnStateAfterMessage(upBase, asMsg({ role: 'SYSTEM', participantId: 'c3' }), null)) });
rows.push({ kind: 'update', id: 'no-pid', state: wire(upBase), message: { role: 'USER', participantId: null }, out: wire(updateTurnStateAfterMessage(upBase, asMsg({ role: 'USER', participantId: null }), null)) });

// ---- computeSpokenThisCycleAfterMessage -----------------------------------
const parts3: WirePart[] = [
  { id: 'c1', type: 'CHARACTER', status: 'active', characterId: 'char-1' },
  { id: 'c2', type: 'CHARACTER', status: 'active', characterId: 'char-2' },
  { id: 'u1', type: 'CHARACTER', status: 'active', characterId: 'char-u' },
];
const msg = (role: string, participantId: string | null, target?: string[]): WireMsg => ({ type: 'message', role, participantId, targetParticipantIds: target });
rows.push({ kind: 'computeMsg', id: 'append', message: msg('ASSISTANT', 'c1'), participants: parts3, currentJson: null, out: computeSpokenThisCycleAfterMessage(asEvent(msg('ASSISTANT', 'c1')), asParts(parts3), null) });
rows.push({ kind: 'computeMsg', id: 'already-no-wrap', message: msg('ASSISTANT', 'c1'), participants: parts3, currentJson: '["c1"]', out: computeSpokenThisCycleAfterMessage(asEvent(msg('ASSISTANT', 'c1')), asParts(parts3), '["c1"]') });
rows.push({ kind: 'computeMsg', id: 'wrap', message: msg('ASSISTANT', 'u1'), participants: parts3, currentJson: '["c1","c2"]', out: computeSpokenThisCycleAfterMessage(asEvent(msg('ASSISTANT', 'u1')), asParts(parts3), '["c1","c2"]') });
rows.push({ kind: 'computeMsg', id: 'not-message-type', message: { type: 'status', role: 'ASSISTANT', participantId: 'c1' }, participants: parts3, currentJson: null, out: computeSpokenThisCycleAfterMessage(asEvent({ type: 'status', role: 'ASSISTANT', participantId: 'c1' }), asParts(parts3), null) });
rows.push({ kind: 'computeMsg', id: 'whisper', message: msg('ASSISTANT', 'c1', ['c2']), participants: parts3, currentJson: null, out: computeSpokenThisCycleAfterMessage(asEvent(msg('ASSISTANT', 'c1', ['c2'])), asParts(parts3), null) });
rows.push({ kind: 'computeMsg', id: 'no-pid', message: msg('USER', null), participants: parts3, currentJson: null, out: computeSpokenThisCycleAfterMessage(asEvent(msg('USER', null)), asParts(parts3), null) });

// ---- computeSpokenThisCycleAfterSkip --------------------------------------
rows.push({ kind: 'computeSkip', id: 'append', skippedId: 'u1', participants: parts3, currentJson: '["c1"]', out: computeSpokenThisCycleAfterSkip('u1', asParts(parts3), '["c1"]') });
rows.push({ kind: 'computeSkip', id: 'wrap', skippedId: 'u1', participants: parts3, currentJson: '["c1","c2"]', out: computeSpokenThisCycleAfterSkip('u1', asParts(parts3), '["c1","c2"]') });
rows.push({ kind: 'computeSkip', id: 'already-no-wrap', skippedId: 'c1', participants: parts3, currentJson: '["c1"]', out: computeSpokenThisCycleAfterSkip('c1', asParts(parts3), '["c1"]') });

// reference createInitialTurnState directly via the 'calc' empty case above.
void createInitialTurnState;

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
