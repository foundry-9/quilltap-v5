/**
 * Oracle case #17 (Wave 3 / B8): context-summary gating cadence.
 *
 * Drives the REAL pure functions from v4's lib/chat/context-summary.ts:
 *   evaluateSummarizationGate, calculateInterchangeCount,
 *   shouldCheckTitleAtInterchange, partitionMessagesIntoTurns.
 * All four are side-effect-free; no injection needed.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/context-summary.ts \
 *     > /tmp/oracle-context-summary.ndjson
 */

import {
  evaluateSummarizationGate,
  calculateInterchangeCount,
  shouldCheckTitleAtInterchange,
  partitionMessagesIntoTurns,
} from '@/lib/chat/context-summary';
import type { ChatEvent } from '@/lib/schemas/types';

type Row =
  | { kind: 'gate'; id: string; currentTurn: number; lastFoldedTurn: number; lastFullRebuildTurn: number; out: string }
  | { kind: 'interchange'; id: string; messages: Array<{ role?: string; type?: string; systemSender?: string | null }>; chatType?: string; out: number }
  | { kind: 'title'; id: string; current: number; lastChecked: number; chatType?: string; out: boolean }
  | { kind: 'partition'; id: string; messages: Array<Record<string, unknown>>; chatType?: string; out: Array<{ turnNumber: number; ids: string[] }> };

const rows: Row[] = [];

// ---- evaluateSummarizationGate -------------------------------------------
const gates: Array<[string, number, number, number]> = [
  ['below-floor-0', 0, 0, 0],
  ['below-floor-5', 5, 0, 0],
  ['at-floor-10', 10, 0, 0],
  ['just-over-fold', 11, 0, 0],
  ['fold-12-1', 12, 1, 0],
  ['skip-12-2', 12, 2, 0],
  ['hard-60', 60, 0, 0],
  ['hard-boundary-61', 61, 5, 11],
  ['fold-not-hard', 60, 40, 20],
  ['skip-recent-fold', 60, 55, 20],
  ['fold-30', 30, 15, 25],
  ['skip-22', 22, 15, 10],
];
for (const [id, c, lf, lr] of gates) {
  rows.push({ kind: 'gate', id, currentTurn: c, lastFoldedTurn: lf, lastFullRebuildTurn: lr, out: evaluateSummarizationGate({ currentTurn: c, lastFoldedTurn: lf, lastFullRebuildTurn: lr }) });
}

// ---- calculateInterchangeCount -------------------------------------------
const mU = { role: 'USER', type: 'message' };
const mA = { role: 'ASSISTANT', type: 'message' };
const mWhisper = { role: 'ASSISTANT', type: 'message', systemSender: 'librarian' };
const interchanges: Array<[string, Array<{ role?: string; type?: string; systemSender?: string | null }>, string | undefined]> = [
  ['empty', [], undefined],
  ['one-pair', [mU, mA], undefined],
  ['two-user-one-assistant', [mU, mU, mA], undefined],
  ['one-user-two-assistant', [mU, mA, mA], undefined],
  ['whisper-excluded', [mU, mWhisper, mA], undefined],
  ['nonmessage-type-excluded', [mU, { role: 'ASSISTANT', type: 'context-summary' }, mA], undefined],
  ['lowercase-role', [{ role: 'user', type: 'message' }, { role: 'assistant', type: 'message' }], undefined],
  ['empty-type-is-message', [{ role: 'USER', type: '' }, { role: 'ASSISTANT', type: '' }], undefined],
  ['undefined-role-skipped', [{ type: 'message' }, mU, mA], undefined],
  ['autonomous-counts-assistant', [mA, mA, mA], 'autonomous'],
  ['autonomous-whisper-excluded', [mA, mWhisper, mA, mWhisper, mA], 'autonomous'],
  ['autonomous-no-user-floor', [mU, mA, mA], 'autonomous'],
  ['empty-systemSender-counts', [mU, { role: 'ASSISTANT', type: 'message', systemSender: '' }], undefined],
];
for (const [id, messages, chatType] of interchanges) {
  rows.push({ kind: 'interchange', id, messages, chatType, out: calculateInterchangeCount(messages, chatType) });
}

// ---- shouldCheckTitleAtInterchange ---------------------------------------
const titles: Array<[string, number, number, string | undefined]> = [
  ['reg-below-min', 1, 0, undefined],
  ['reg-checkpoint-2', 2, 0, undefined],
  ['reg-already-2', 2, 2, undefined],
  ['reg-gap-4', 4, 3, undefined],
  ['reg-checkpoint-5', 5, 3, undefined],
  ['reg-12-after-10', 12, 10, undefined],
  ['reg-20-after-10', 20, 10, undefined],
  ['reg-25-after-19', 25, 19, undefined],
  ['reg-jump-23', 23, 8, undefined],
  ['help-checkpoint-1', 1, 0, 'help'],
  ['help-already-1', 1, 1, 'help'],
  ['help-brahma-1', 1, 0, 'brahma'],
  ['autonomous-as-regular', 15, 5, 'autonomous'],
];
for (const [id, current, lastChecked, chatType] of titles) {
  rows.push({ kind: 'title', id, current, lastChecked, chatType, out: shouldCheckTitleAtInterchange(current, lastChecked, chatType) });
}

// ---- partitionMessagesIntoTurns ------------------------------------------
const ev = (id: string, role: string, type = 'message', systemSender: string | null = null): Record<string, unknown> =>
  ({ id, role, type, content: `c-${id}`, systemSender });
const partitions: Array<[string, Array<Record<string, unknown>>, string | undefined]> = [
  ['empty', [], undefined],
  ['two-turns', [ev('u1', 'USER'), ev('a1', 'ASSISTANT'), ev('u2', 'USER'), ev('a2', 'ASSISTANT')], undefined],
  ['leading-greeting', [ev('a0', 'ASSISTANT'), ev('u1', 'USER'), ev('a1', 'ASSISTANT')], undefined],
  ['staff-whisper-excluded', [ev('u1', 'USER'), ev('w1', 'ASSISTANT', 'message', 'librarian'), ev('a1', 'ASSISTANT')], undefined],
  ['nonmessage-skipped', [ev('u1', 'USER'), ev('cs', 'ASSISTANT', 'context-summary'), ev('a1', 'ASSISTANT')], undefined],
  ['assistant-only-no-flush', [ev('a1', 'ASSISTANT'), ev('a2', 'ASSISTANT')], undefined],
  ['autonomous-each-assistant', [ev('a1', 'ASSISTANT'), ev('a2', 'ASSISTANT'), ev('a3', 'ASSISTANT')], 'autonomous'],
  ['autonomous-whisper-excluded', [ev('a1', 'ASSISTANT'), ev('w1', 'ASSISTANT', 'message', 'librarian'), ev('a2', 'ASSISTANT')], 'autonomous'],
];
for (const [id, messages, chatType] of partitions) {
  const turns = partitionMessagesIntoTurns(messages as unknown as ChatEvent[], chatType);
  rows.push({ kind: 'partition', id, messages, chatType, out: turns.map(t => ({ turnNumber: t.turnNumber, ids: t.ids })) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
