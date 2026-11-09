/**
 * Oracle case (tier 1, P4.D347): the pure heart of bugs 263 / 264 / 265 at
 * v4 `bdd09b75b` — driving v4's REAL code at the pin:
 *
 * - `lib/tokens/token-counter.ts` — `countMessageTokens` (content + ROLE tokens
 *   + 4) and `countMessagesTokens` (Σ + 3; 0 for an empty array): the one
 *   per-message rule the history budget and the pre-send check share (bug 263).
 * - `lib/chat/context/message-selector.ts` — `selectableMessageTokens` (the row
 *   price: `countMessageTokens` + the `[name] ` prefix) and
 *   `selectRecentMessages` (the newest-first tail fit; the force-include keeps
 *   `id` at the pin).
 * - `lib/chat/context/turn-history.ts` — `packTurnHistory` (the clamp + the
 *   "getting long" advice), `buildLeadingSystemBlocks` (blocks 1–3), and
 *   `turnHistoryRows` with a CAST, which runs the private `fromWindow` (bug
 *   264 — the multi-character history cut to the compression window: by the
 *   start row's id, else by index on equal lengths, else kept whole with a
 *   WARN). The cast here has every seat on full history access and no
 *   whispers, so the rows' ids are exactly `fromWindow`'s — attribution is
 *   `build_context_tier3`'s subject, not this family's; only ids (and the
 *   WARN) are compared.
 * - `lib/chat/context/turn-compression.ts` — `verbatimWindowStart` (bug 265:
 *   counted by `toVisibleConversationMessage`) and `effectiveWindowSize`.
 *
 * Provider rates (R-11, measured at the pin — `token-counter.ts`
 * `CHARS_PER_TOKEN`): the default (no provider) and every provider but GOOGLE
 * is 3.5 chars / token; GOOGLE is 3.8. No provider runs at 4. Each row carries
 * its `cpt` so the Rust side passes the same rate.
 *
 * Pure — runs under `npx tsx`. Emits NDJSON `{kind, id, input, out}`.
 *
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   npx tsx $V5W/harness/oracle/cases/turn-history.ts > /tmp/oracle-turn-history.ndjson
 */

import { logger } from '@/lib/logger';
import { countMessageTokens, countMessagesTokens } from '@/lib/tokens/token-counter';
import { selectRecentMessages, selectableMessageTokens } from '@/lib/chat/context/message-selector';
import {
  buildLeadingSystemBlocks,
  packTurnHistory,
  turnHistoryRows,
} from '@/lib/chat/context/turn-history';
import { effectiveWindowSize, verbatimWindowStart } from '@/lib/chat/context/turn-compression';

type Provider = 'GOOGLE' | undefined;
const CPT: Record<string, number> = { default: 3.5, GOOGLE: 3.8 };
const cptOf = (p: Provider) => CPT[p ?? 'default'];
const PROVIDERS: Provider[] = [undefined, 'GOOGLE'];

// Capture the module logger's WARNs (the `fromWindow` line) — the same object
// `turn-history.ts` imports.
const warns: Array<{ message: string; context: unknown }> = [];
// eslint-disable-next-line @typescript-eslint/no-explicit-any
(logger as any).warn = (message: string, context?: unknown) => {
  warns.push({ message, context });
};
// Everything else stays off stdout (the NDJSON): `effectiveWindowSize`'s INFO
// (unchanged across `bdd09b75b`; v5 carries no twin — P4.D347 records it).
// eslint-disable-next-line @typescript-eslint/no-explicit-any
(logger as any).info = () => {};
// eslint-disable-next-line @typescript-eslint/no-explicit-any
(logger as any).debug = () => {};

const out: string[] = [];
const emit = (kind: string, id: string, input: unknown, value: unknown) =>
  out.push(JSON.stringify({ kind, id, input, out: value }));

// ---- countMessageTokens / countMessagesTokens ----
const counts: Array<[string, Array<{ role: string; content: string }>]> = [
  ['empty', []],
  ['one-user', [{ role: 'user', content: 'Hello there, friend.' }]],
  ['roles', [
    { role: 'system', content: 'You are Friday.' },
    { role: 'USER', content: 'When she is not scolding Vergil' },
    { role: 'ASSISTANT', content: 'Abigail documents everything.' },
  ]],
  ['empty-content', [{ role: 'user', content: '' }]],
  ['astral', [{ role: 'assistant', content: '🎩 A soirée with Zöe.' }]],
];
for (const p of PROVIDERS) {
  for (const [id, msgs] of counts) {
    emit('count_messages', `${id}|${p ?? 'default'}`, { messages: msgs, cpt: cptOf(p) }, countMessagesTokens(msgs, p));
    for (const [i, m] of msgs.entries()) {
      emit('count_message', `${id}#${i}|${p ?? 'default'}`, { ...m, cpt: cptOf(p) }, countMessageTokens(m, p));
    }
  }
}

// ---- selectableMessageTokens / selectRecentMessages ----
type Sel = { role: string; content: string; id?: string; thoughtSignature?: string; name?: string; participantId?: string };
const history: Sel[] = [
  { role: 'USER', content: 'The ledger is open on the desk.', id: 'm1' },
  { role: 'ASSISTANT', content: 'Abigail will absolutely make that a formal finding.', id: 'm2', name: 'Friday', participantId: 'p1' },
  { role: 'user', content: 'You have seen how she documents.', id: 'm3', name: 'Vergil', participantId: 'p2', thoughtSignature: 'sig-3' },
  { role: 'ASSISTANT', content: 'Indeed, every line of it, in triplicate and in ink.', id: 'm4', name: '' },
  { role: 'USER', content: 'When she is not scolding Vergil', id: 'm5' },
];
for (const p of PROVIDERS) {
  for (const m of history) {
    emit('select_tokens', `${m.id}|${p ?? 'default'}`, { msg: m, cpt: cptOf(p) }, selectableMessageTokens(m, p as never));
  }
  for (const max of [0, 10, 25, 40, 60, 1000]) {
    emit('select', `max${max}|${p ?? 'default'}`, { messages: history, maxTokens: max, cpt: cptOf(p) },
      selectRecentMessages(history, max, p as never));
  }
}
emit('select', 'empty', { messages: [], maxTokens: 100, cpt: 3.5 }, selectRecentMessages([], 100, undefined as never));
// The force-include: nothing fits, the last row comes back WITH its id.
emit('select', 'force-include-keeps-id', { messages: [history[4]], maxTokens: 1, cpt: 3.5 },
  selectRecentMessages([history[4]], 1, undefined as never));

// ---- packTurnHistory: max(0, min(remaining, recentMessagesBudget)) ----
const long: Sel[] = Array.from({ length: 24 }, (_, i) => ({
  role: i % 2 ? 'ASSISTANT' : 'USER',
  content: `Line ${i} — `.padEnd(80, 'x'),
  id: `l${i}`,
}));
const packs: Array<[string, Sel[], number, number, number]> = [
  ['fits', history, 1000, 2000, 100000],
  ['remaining-binds', history, 30, 2000, 100000],
  ['budget-binds', history, 1000, 30, 100000],
  ['exhausted', history, -50, 2000, 100000],
  ['long-advice', long, 200, 2000, 100000],
  ['long-near-limit', long, 200, 2000, 900],
];
for (const [id, rows, remainingTokens, recentMessagesBudget, totalLimit] of packs) {
  const warnings: string[] = [];
  const packed = packTurnHistory(rows as never, { provider: undefined as never, remainingTokens, recentMessagesBudget, totalLimit, warnings });
  emit('pack', id, { rows, remainingTokens, recentMessagesBudget, totalLimit, cpt: 3.5 }, { ...packed, warnings });
}

// ---- buildLeadingSystemBlocks ----
for (const [id, block] of [['two-blocks', null], ['three-blocks', '## Conversation Context\n\nA summary.']] as const) {
  const blocks = buildLeadingSystemBlocks({ systemPrompt: 'You are Friday, a careful clerk.', characterName: 'Friday', compressedHistoryBlock: block });
  emit('leading_blocks', id, { systemPrompt: 'You are Friday, a careful clerk.', characterName: 'Friday', compressedHistoryBlock: block, cpt: 3.5 },
    { blocks: blocks.map(b => ({ role: b.role, content: b.content })), tokens: countMessagesTokens(blocks, undefined) });
}

// ---- verbatimWindowStart (bug 265) ----
type Raw = { role: string; content: string; type?: string; systemKind?: string; systemSender?: string };
const windowRows: Raw[] = [
  { role: 'USER', content: 'one' },
  { role: 'ASSISTANT', content: 'two, a perfectly ordinary reply' },
  { role: 'USER', content: 'summary row', type: 'context-summary' },
  { role: 'ASSISTANT', content: 'The Host handed Leilani a note.', systemSender: 'host', systemKind: 'inform' },
  { role: 'ASSISTANT', content: 'The Host begs leave to introduce a person spoken of.', systemSender: 'host', systemKind: 'off-scene-characters' },
  { role: 'ASSISTANT', content: 'The Host observes that one member of the previous company did not make the journey.', systemSender: 'host', systemKind: 'off-scene-characters' },
  { role: 'USER', content: '' },
  { role: 'ASSISTANT', content: '[Tool call made]\n{"toolName":"x"}' },
  { role: 'USER', content: 'three' },
  { role: 'ASSISTANT', content: 'four, another ordinary reply here' },
];
for (const n of [0, 1, 2, 3, 4, 5, 6, 10]) {
  emit('window_start', `all|${n}`, { messages: windowRows, windowCount: n }, verbatimWindowStart(windowRows, n));
}
// The inform record inside the window rides along but never counts.
const informInWindow: Raw[] = [
  { role: 'USER', content: 'a' },
  { role: 'ASSISTANT', content: 'b, an ordinary reply' },
  { role: 'ASSISTANT', content: 'The Host handed Leilani a note.', systemSender: 'host', systemKind: 'inform' },
  { role: 'USER', content: 'c' },
];
emit('window_start', 'inform-in-window|2', { messages: informInWindow, windowCount: 2 }, verbatimWindowStart(informInWindow, 2));
emit('window_start', 'empty|3', { messages: [], windowCount: 3 }, verbatimWindowStart([], 3));
// === P4.D372 (v4 `db6190b44`) === an ordinary fact counted at the baseline
// too (a seat line is a USER row; a Host fact an ASSISTANT row with text);
// the ONE count move is a machine fact whose body is only tool markup —
// `stripToolArtifacts` emptied it at the baseline, and the fact arm now keeps
// it (framed, before the strip).
const deusWindow: Array<Raw & { isEstablishedFact?: boolean }> = [
  { role: 'USER', content: 'a' },
  { role: 'ASSISTANT', content: '[Tool call made]', systemSender: 'host', systemKind: 'established-fact' },
  { role: 'USER', content: 'The bridge fell.', isEstablishedFact: true },
  { role: 'ASSISTANT', content: 'The river rose above the mill.', systemSender: 'host', systemKind: 'established-fact' },
  { role: 'ASSISTANT', content: 'b, an ordinary reply' },
];
for (const n of [1, 2, 3, 4, 5]) {
  emit('window_start', `deus|${n}`, { messages: deusWindow, windowCount: n }, verbatimWindowStart(deusWindow, n));
}
// === /P4.D372 ===

// ---- effectiveWindowSize ----
const eff: Array<[string, number | undefined, number, number | undefined]> = [
  ['standard', 5, 20, undefined],
  ['cache-at-count', 5, 20, 20],
  ['cache-behind', 5, 20, 14],
  ['cache-ahead', 5, 20, 25],
  ['zero-standard', 0, 20, undefined],
  ['zero-standard-behind', 0, 20, 17],
  ['ten', 10, 3, 1],
];
for (const [id, standard, visible, cached] of eff) {
  emit('effective_window', id, { standard: standard ?? null, visible, cached: cached ?? null }, effectiveWindowSize(standard, visible, cached));
}

// ---- fromWindow through turnHistoryRows with a cast (bug 264) ----
const participants = [
  { id: 'p1', type: 'CHARACTER', characterId: 'c1', controlledBy: 'llm', status: 'active', hasHistoryAccess: true, createdAt: '2026-01-01T00:00:00.000Z' },
  { id: 'p2', type: 'CHARACTER', characterId: 'c2', controlledBy: 'llm', status: 'active', hasHistoryAccess: true, createdAt: '2026-01-01T00:00:00.000Z' },
];
const characters = new Map([
  ['c1', { id: 'c1', name: 'Friday' }],
  ['c2', { id: 'c2', name: 'Vergil' }],
]);
type Mwp = { role: string; content: string; id?: string; participantId?: string };
const mwp = (ids: Array<string | undefined>): Mwp[] =>
  ids.map((id, i) => ({ role: i % 2 ? 'ASSISTANT' : 'USER', content: `row ${i}`, ...(id ? { id } : {}), participantId: i % 2 ? 'p1' : 'p2' }));
const existing = (ids: Array<string | undefined>) =>
  ids.map((id, i) => ({ role: i % 2 ? 'assistant' : 'user', content: `row ${i}`, ...(id ? { id } : {}) }));
const castCases: Array<[string, Mwp[], ReturnType<typeof existing>, number]> = [
  ['no-cut', mwp(['a', 'b', 'c', 'd']), existing(['a', 'b', 'c', 'd']), 0],
  ['by-id', mwp(['a', 'b', 'c', 'd']), existing(['a', 'b', 'c', 'd']), 2],
  // The cast has one more row (a row the plain history dropped): the id finds it.
  ['by-id-unequal', mwp(['a', 'x', 'b', 'c', 'd']), existing(['a', 'b', 'c', 'd']), 2],
  ['shuffled-ids', mwp(['d', 'c', 'b', 'a', 'e']), existing(['a', 'b', 'c', 'd']), 1],
  ['equal-no-id', mwp([undefined, undefined, undefined, undefined]), existing([undefined, undefined, undefined, undefined]), 1],
  ['unequal-no-id', mwp([undefined, undefined, undefined, undefined, undefined]), existing([undefined, undefined, undefined, undefined]), 2],
  ['start-row-id-missing', mwp(['x', 'y', 'z']), existing(['a', 'b', 'c']), 1],
  ['past-the-end', mwp(['a', 'b']), existing(['a', 'b']), 2],
];
for (const [id, rows, existingMessages, windowStart] of castCases) {
  warns.length = 0;
  const got = turnHistoryRows({
    existingMessages,
    cast: {
      respondingParticipant: participants[0] as never,
      allParticipants: participants as never,
      participantCharacters: characters as never,
      messagesWithParticipants: rows as never,
    },
    windowStart,
    summaryAnchorIds: [],
  });
  emit('from_window', id, { rows, existingMessages, windowStart }, {
    ids: got.map(m => m.id ?? null),
    contents: got.map(m => m.content),
    warns: warns.map(w => ({ message: w.message, context: w.context })),
  });
}

for (const line of out) process.stdout.write(line + '\n');
