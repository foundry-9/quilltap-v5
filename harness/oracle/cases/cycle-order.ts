/**
 * Oracle case (P4.D172): the cycle rotation — drawn once, then followed.
 *
 * Drives v4's REAL `lib/chat/turn-manager/{cycle-order,state,queue,turn-order,
 * selection}.ts` at the `78b381a96` pin. Every case name in v4's own
 * `__tests__/unit/lib/chat/turn-manager/cycle-order.test.ts` appears here as a
 * corpus row, plus the arms that file leaves to statistics.
 *
 * `Math.random` is pinned to an ORDERED ARRAY per row (`drawCycleOrder` calls it
 * once per remaining candidate) and the draws actually CONSUMED are emitted, so
 * the Rust port's replay is compared on the count as well as the values. v4's own
 * weighting cases are statistical (400 draws, `p1First > 240`) and are not
 * transcribable as differential rows; the pinned-sequence rows below cover the
 * same ground deterministically, and a Rust-only statistical test mirrors v4's
 * bounds.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/cycle-order.ts \
 *     > /tmp/oracle-cycle-order.ndjson
 */

// Imported from the CONCRETE modules rather than the `turn-manager` barrel:
// `state.ts` imports `cycle-order.ts` and both are re-exported by `index.ts`, and
// under tsx's ESM loader that cycle makes the barrel report
// `does not provide an export named 'calculateTurnStateFromHistory'`. v4's own
// jest suite (CJS) does not see it. `select-speaker.ts` imports the same way.
import {
  drawCycleOrder,
  pickFromCycleOrder,
  resolveCycleOrder,
  parseCycleOrder,
  cycleCandidates,
} from '@/lib/chat/turn-manager/cycle-order';
import {
  computeCycleOrderAfterMessage,
  computeCycleOrderAfterSkip,
  createInitialTurnState,
  calculateTurnStateFromHistory,
  updateTurnStateAfterMessage,
} from '@/lib/chat/turn-manager/state';
import { selectNextSpeaker } from '@/lib/chat/turn-manager/selection';
import { resetCycleForUserSkip } from '@/lib/chat/turn-manager/queue';
import { computePredictedTurnOrder } from '@/lib/chat/turn-manager/turn-order';
import { getSelectionExplanation } from '@/lib/chat/turn-manager/utils';
import type { TurnState } from '@/lib/chat/turn-manager/types';
import type { ChatParticipantBase, Character, MessageEvent } from '@/lib/schemas/types';

// --------------------------------------------------------------------------
// Wire shapes (mirrored by the Rust side's serde structs)
// --------------------------------------------------------------------------

type WirePart = {
  id: string;
  type: string;
  status: string;
  characterId: string | null;
  controlledBy: string;
  talkativeness: number | null;
};
/** characterId -> the two fields the rotation reads. */
type WireChar = { talkativeness?: number | null; archivedAt?: string | null };
type WireChars = Record<string, WireChar>;

const asParts = (ps: WirePart[]) => ps as unknown as ChatParticipantBase[];

const asChars = (c: WireChars): Map<string, Character> => {
  const m = new Map<string, Character>();
  for (const [cid, spec] of Object.entries(c)) {
    const built: Record<string, unknown> = { id: cid, name: `Character ${cid}` };
    if (spec.talkativeness !== undefined && spec.talkativeness !== null) {
      built.talkativeness = spec.talkativeness;
    }
    if (spec.archivedAt !== undefined) built.archivedAt = spec.archivedAt;
    m.set(cid, built as unknown as Character);
  }
  return m;
};

const p = (
  id: string,
  characterId: string | null,
  controlledBy = 'llm',
  status = 'active',
  talkativeness: number | null = null,
  type = 'CHARACTER',
): WirePart => ({ id, type, status, characterId, controlledBy, talkativeness });

const mkState = (o: Partial<TurnState> = {}): TurnState =>
  ({ ...createInitialTurnState(), ...o } as TurnState);

/**
 * Pins `Math.random` to an ordered sequence and reports what was consumed.
 * The last value REPEATS once the array is spent, so a one-element array is the
 * old scalar pin (see `select-speaker.ts`).
 */
function withRandom<T>(draws: number[], fn: () => T): { out: T; consumed: number[] } {
  const orig = Math.random;
  const consumed: number[] = [];
  let i = 0;
  Math.random = () => {
    const v = draws.length === 0 ? 0 : draws[Math.min(i, draws.length - 1)];
    i += 1;
    consumed.push(v);
    return v;
  };
  try {
    return { out: fn(), consumed };
  } finally {
    Math.random = orig;
  }
}

const rows: unknown[] = [];
const emit = (row: unknown) => rows.push(row);

// The three-seat room v4's own suite builds, with every character at 0.5.
const ROOM: WirePart[] = [p('p1', 'char-1'), p('p2', 'char-2'), p('p3', 'char-3')];
const ROOM_CHARS: WireChars = {
  'char-1': { talkativeness: 0.5 },
  'char-2': { talkativeness: 0.5 },
  'char-3': { talkativeness: 0.5 },
};

// --------------------------------------------------------------------------
// parseCycleOrder
// --------------------------------------------------------------------------

for (const [id, json] of [
  ['parse-null', null],
  ['parse-undefined', undefined],
  ['parse-empty-string', ''],
  ['parse-ok', '["a","b"]'],
  ['parse-not-json', '{not json'],
  ['parse-not-array', '"scalar"'],
  ['parse-object', '{"a":1}'],
  ['parse-mixed-members', '[1,"a",null,{"x":1},"b"]'],
  ['parse-empty-array', '[]'],
] as Array<[string, string | null | undefined]>) {
  emit({ kind: 'parse', id, json: json ?? null, out: parseCycleOrder(json) });
}

// --------------------------------------------------------------------------
// cycleCandidates — "is the present, unarchived character seats"
// --------------------------------------------------------------------------

const candidateCases: Array<{ id: string; participants: WirePart[]; characters: WireChars }> = [
  { id: 'candidates-plain-room', participants: ROOM, characters: ROOM_CHARS },
  {
    id: 'candidates-drops-absent-and-removed-and-user-typed',
    participants: [
      p('p1', 'char-1'),
      p('p2', 'char-2', 'llm', 'absent'),
      p('p3', 'char-3', 'llm', 'removed'),
      p('p4', 'char-4', 'llm', 'silent'),
      p('p5', null),
      p('p6', 'char-6', 'llm', 'active', null, 'USER'),
    ],
    characters: ROOM_CHARS,
  },
  {
    // A USER-DRIVEN seat is IN the rotation (the bug-131 invariant).
    id: 'candidates-keeps-a-user-driven-seat',
    participants: [p('p1', 'char-1'), p('p2', 'char-2', 'user')],
    characters: ROOM_CHARS,
  },
  {
    id: 'candidates-drops-a-known-archived-character',
    participants: ROOM,
    characters: { ...ROOM_CHARS, 'char-2': { talkativeness: 0.5, archivedAt: '2026-08-10T00:00:00.000Z' } },
  },
  {
    // "keeps a seat whose character is not in the map" — the optional chain.
    id: 'candidates-keeps-an-unknown-character',
    participants: ROOM,
    characters: { 'char-1': { talkativeness: 0.5 } },
  },
  {
    // An EMPTY-STRING archivedAt is JS-falsy: no tombstone.
    id: 'candidates-empty-archived-at-is-not-archived',
    participants: ROOM,
    characters: { ...ROOM_CHARS, 'char-2': { talkativeness: 0.5, archivedAt: '' } },
  },
];
for (const c of candidateCases) {
  emit({
    kind: 'candidates',
    id: c.id,
    participants: c.participants,
    characters: c.characters,
    out: cycleCandidates(asParts(c.participants), asChars(c.characters)).map((x) => x.id),
  });
}

// --------------------------------------------------------------------------
// drawCycleOrder
// --------------------------------------------------------------------------

const drawCases: Array<{
  id: string;
  participants: WirePart[];
  characters: WireChars;
  excludeFirst?: string | null;
  draws: number[];
}> = [
  { id: 'draw-every-candidate-exactly-once', participants: ROOM, characters: ROOM_CHARS, draws: [0.0] },
  { id: 'draw-sequence-walks-the-shrinking-pool', participants: ROOM, characters: ROOM_CHARS, draws: [0.9, 0.9, 0.9] },
  { id: 'draw-mixed-sequence', participants: ROOM, characters: ROOM_CHARS, draws: [0.5, 0.1, 0.99] },
  { id: 'draw-keeps-the-previous-speaker-out-of-first', participants: ROOM, characters: ROOM_CHARS, excludeFirst: 'p1', draws: [0.0, 0.0, 0.0] },
  { id: 'draw-seats-the-only-candidate-even-when-they-just-spoke', participants: [p('p1', 'char-1')], characters: ROOM_CHARS, excludeFirst: 'p1', draws: [0.0] },
  {
    id: 'draw-weights-by-talkativeness-override-wins',
    participants: [p('p1', 'char-1', 'llm', 'active', 0.9), p('p2', 'char-2'), p('p3', 'char-3')],
    characters: { 'char-1': { talkativeness: 0.1 }, 'char-2': { talkativeness: 0.5 }, 'char-3': { talkativeness: 0.5 } },
    draws: [0.4, 0.5, 0.5],
  },
  {
    id: 'draw-character-talkativeness-then-default',
    participants: [p('p1', 'char-1'), p('p2', 'char-unknown')],
    characters: { 'char-1': { talkativeness: 0.9 } },
    draws: [0.7, 0.0],
  },
  { id: 'draw-leaves-archived-out', participants: ROOM, characters: { ...ROOM_CHARS, 'char-2': { talkativeness: 0.5, archivedAt: '2026-08-10T00:00:00.000Z' } }, draws: [0.0, 0.0] },
  { id: 'draw-keeps-a-seat-whose-character-is-not-in-the-map', participants: ROOM, characters: { 'char-1': { talkativeness: 0.5 } }, draws: [0.99, 0.99, 0.99] },
  { id: 'draw-empty-room-is-empty', participants: [], characters: {}, draws: [0.5] },
  { id: 'draw-user-driven-seat-carries-its-talkativeness', participants: [p('p1', 'char-1'), p('p2', 'char-2', 'user')], characters: { 'char-1': { talkativeness: 0.1 }, 'char-2': { talkativeness: 0.9 } }, draws: [0.5, 0.0] },
  { id: 'draw-all-silent-equal-weights', participants: [p('p1', 'char-1', 'llm', 'active', 0), p('p2', 'char-2', 'llm', 'active', 0)], characters: {}, draws: [0.6, 0.0] },
  { id: 'draw-exclude-first-ignored-once-the-head-is-placed', participants: ROOM, characters: ROOM_CHARS, excludeFirst: 'p3', draws: [0.99, 0.99, 0.99] },
];
for (const c of drawCases) {
  const { out, consumed } = withRandom(c.draws, () =>
    drawCycleOrder({
      participants: asParts(c.participants),
      characters: asChars(c.characters),
      excludeFirst: c.excludeFirst ?? null,
    }),
  );
  emit({ kind: 'draw', id: c.id, participants: c.participants, characters: c.characters, excludeFirst: c.excludeFirst ?? null, draws: c.draws, out, consumedDraws: consumed });
}

// --------------------------------------------------------------------------
// pickFromCycleOrder
// --------------------------------------------------------------------------

const pickCases: Array<{
  id: string;
  order: string[] | null;
  participants: WirePart[];
  characters: WireChars;
  spoken: string[];
  lastSpeakerId: string | null;
}> = [
  { id: 'pick-first-who-can-still-speak', order: ['p1', 'p2', 'p3'], participants: ROOM, characters: ROOM_CHARS, spoken: [], lastSpeakerId: null },
  { id: 'pick-skips-already-spoken', order: ['p1', 'p2', 'p3'], participants: ROOM, characters: ROOM_CHARS, spoken: ['p1'], lastSpeakerId: null },
  { id: 'pick-skips-the-last-speaker', order: ['p1', 'p2', 'p3'], participants: ROOM, characters: ROOM_CHARS, spoken: [], lastSpeakerId: 'p1' },
  { id: 'pick-skips-a-departed-seat', order: ['gone', 'p2'], participants: ROOM, characters: ROOM_CHARS, spoken: [], lastSpeakerId: null },
  { id: 'pick-skips-an-archived-seat', order: ['p2', 'p3'], participants: ROOM, characters: { ...ROOM_CHARS, 'char-2': { talkativeness: 0.5, archivedAt: '2026-08-10T00:00:00.000Z' } }, spoken: [], lastSpeakerId: null },
  { id: 'pick-exhausted-order-is-null', order: ['p1', 'p2', 'p3'], participants: ROOM, characters: ROOM_CHARS, spoken: ['p1', 'p2', 'p3'], lastSpeakerId: null },
  { id: 'pick-empty-order-is-null', order: [], participants: ROOM, characters: ROOM_CHARS, spoken: [], lastSpeakerId: null },
  { id: 'pick-missing-order-is-null', order: null, participants: ROOM, characters: ROOM_CHARS, spoken: [], lastSpeakerId: null },
  { id: 'pick-a-user-driven-seat-is-usable', order: ['p2'], participants: [p('p1', 'char-1'), p('p2', 'char-2', 'user')], characters: ROOM_CHARS, spoken: [], lastSpeakerId: null },
];
for (const c of pickCases) {
  emit({
    kind: 'pick-from',
    id: c.id,
    order: c.order,
    participants: c.participants,
    characters: c.characters,
    spoken: c.spoken,
    lastSpeakerId: c.lastSpeakerId,
    out: pickFromCycleOrder(
      c.order ?? undefined,
      asParts(c.participants),
      asChars(c.characters),
      { spokenSinceUserTurn: c.spoken, lastSpeakerId: c.lastSpeakerId } as TurnState,
    ),
  });
}

// --------------------------------------------------------------------------
// resolveCycleOrder — the single writer
// --------------------------------------------------------------------------

const resolveCases: Array<{
  id: string;
  participants: WirePart[];
  characters: WireChars;
  stored: string[];
  spoken: string[];
  lastSpeakerId: string | null;
  draws: number[];
  failWrite?: boolean;
}> = [
  { id: 'resolve-draws-and-persists-when-nothing-usable', participants: ROOM, characters: ROOM_CHARS, stored: [], spoken: [], lastSpeakerId: null, draws: [0.0, 0.0, 0.0] },
  { id: 'resolve-returns-a-usable-stored-rotation-untouched', participants: ROOM, characters: ROOM_CHARS, stored: ['p1', 'p2', 'p3'], spoken: [], lastSpeakerId: null, draws: [0.0] },
  { id: 'resolve-seats-a-mid-cycle-arrival-at-the-back', participants: ROOM, characters: ROOM_CHARS, stored: ['p1', 'p2'], spoken: [], lastSpeakerId: null, draws: [0.0] },
  { id: 'resolve-a-latecomer-who-already-spoke-is-not-appended', participants: ROOM, characters: ROOM_CHARS, stored: ['p1', 'p2'], spoken: ['p3'], lastSpeakerId: null, draws: [0.0] },
  { id: 'resolve-stores-nothing-for-a-one-character-room', participants: [p('p1', 'char-1')], characters: ROOM_CHARS, stored: [], spoken: [], lastSpeakerId: null, draws: [0.0] },
  { id: 'resolve-stores-nothing-for-an-empty-room', participants: [], characters: {}, stored: [], spoken: [], lastSpeakerId: null, draws: [0.0] },
  { id: 'resolve-still-returns-the-rotation-when-the-write-fails', participants: ROOM, characters: ROOM_CHARS, stored: [], spoken: [], lastSpeakerId: null, draws: [0.0, 0.0, 0.0], failWrite: true },
  { id: 'resolve-redraws-a-spent-rotation-holding-back-the-last-speaker', participants: ROOM, characters: ROOM_CHARS, stored: ['p1', 'p2', 'p3'], spoken: ['p1', 'p2', 'p3'], lastSpeakerId: 'p3', draws: [0.0, 0.0, 0.0] },
  { id: 'resolve-a-stale-stored-order-redraws', participants: ROOM, characters: ROOM_CHARS, stored: ['gone-1', 'gone-2'], spoken: [], lastSpeakerId: null, draws: [0.5, 0.5, 0.5] },
  { id: 'resolve-a-two-seat-room-with-one-archived-stores-nothing', participants: ROOM, characters: { 'char-1': { talkativeness: 0.5 }, 'char-2': { talkativeness: 0.5, archivedAt: '2026-08-10T00:00:00.000Z' }, 'char-3': { talkativeness: 0.5, archivedAt: '2026-08-10T00:00:00.000Z' } }, stored: [], spoken: [], lastSpeakerId: null, draws: [0.0] },
];
for (const c of resolveCases) {
  const writes: Array<{ chatId: string; cycleOrderParticipantIds: string }> = [];
  const repos = {
    chats: {
      update: async (id: string, data: { cycleOrderParticipantIds: string }) => {
        if (c.failWrite) throw new Error('write refused by the corpus');
        writes.push({ chatId: id, cycleOrderParticipantIds: data.cycleOrderParticipantIds });
        return undefined;
      },
    },
  };
  // resolveCycleOrder is async; the draws happen inside it, so pin around the
  // await rather than around the call.
  const orig = Math.random;
  const consumed: number[] = [];
  let i = 0;
  Math.random = () => {
    const v = c.draws.length === 0 ? 0 : c.draws[Math.min(i, c.draws.length - 1)];
    i += 1;
    consumed.push(v);
    return v;
  };
  let out: string[];
  try {
    out = await resolveCycleOrder(
      repos,
      { id: 'chat-1', participants: asParts(c.participants) },
      asChars(c.characters),
      mkState({ cycleOrder: c.stored, spokenSinceUserTurn: c.spoken, lastSpeakerId: c.lastSpeakerId }),
    );
  } finally {
    Math.random = orig;
  }
  emit({
    kind: 'resolve',
    id: c.id,
    participants: c.participants,
    characters: c.characters,
    stored: c.stored,
    spoken: c.spoken,
    lastSpeakerId: c.lastSpeakerId,
    draws: c.draws,
    failWrite: c.failWrite === true,
    out,
    writes,
    consumedDraws: consumed,
  });
}

// --------------------------------------------------------------------------
// consumption — computeCycleOrderAfter{Message,Skip}
// --------------------------------------------------------------------------

const mkMessage = (
  role: string,
  participantId: string | null,
  extra: Record<string, unknown> = {},
) => ({ type: 'message', id: 'm1', role, content: 'x', attachments: [], createdAt: now(), participantId, ...extra });
function now() {
  return '2026-09-09T00:00:00.000Z';
}

const consumeCases: Array<{ id: string; message: Record<string, unknown>; json: string | null }> = [
  { id: 'consume-strikes-the-speaker', message: mkMessage('ASSISTANT', 'p1'), json: '["p1","p2","p3"]' },
  { id: 'consume-user-message-counts', message: mkMessage('USER', 'p2'), json: '["p1","p2","p3"]' },
  { id: 'consume-empties-on-the-last', message: mkMessage('ASSISTANT', 'p3'), json: '["p3"]' },
  { id: 'consume-already-out-is-a-noop', message: mkMessage('ASSISTANT', 'p9'), json: '["p1","p2"]' },
  { id: 'consume-not-a-message-type', message: { ...mkMessage('ASSISTANT', 'p1'), type: 'announcement' }, json: '["p1"]' },
  { id: 'consume-system-role', message: mkMessage('SYSTEM', 'p1'), json: '["p1"]' },
  { id: 'consume-no-participant-id', message: mkMessage('ASSISTANT', null), json: '["p1"]' },
  { id: 'consume-whisper', message: mkMessage('ASSISTANT', 'p1', { targetParticipantIds: ['p2'] }), json: '["p1"]' },
  { id: 'consume-empty-whisper-target-still-counts', message: mkMessage('ASSISTANT', 'p1', { targetParticipantIds: [] }), json: '["p1","p2"]' },
  { id: 'consume-null-json', message: mkMessage('ASSISTANT', 'p1'), json: null },
  { id: 'consume-bad-json', message: mkMessage('ASSISTANT', 'p1'), json: '{not json' },
];
for (const c of consumeCases) {
  emit({
    kind: 'after-message',
    id: c.id,
    message: c.message,
    json: c.json,
    out: computeCycleOrderAfterMessage(c.message as unknown as MessageEvent, c.json),
  });
}

for (const [id, pid, json] of [
  ['skip-takes-the-seat-out', 'p2', '["p1","p2","p3"]'],
  ['skip-already-out-is-a-noop', 'p9', '["p1","p2"]'],
  ['skip-empties-on-the-last', 'p1', '["p1"]'],
  ['skip-null-json', 'p1', null],
] as Array<[string, string, string | null]>) {
  emit({ kind: 'after-skip', id, participantId: pid, json, out: computeCycleOrderAfterSkip(pid, json) });
}

// --------------------------------------------------------------------------
// state: calculateTurnStateFromHistory / updateTurnStateAfterMessage / reset
// --------------------------------------------------------------------------

emit({ kind: 'initial-state', id: 'initial-state', out: createInitialTurnState() });

for (const [id, cycleJson] of [
  ['history-reads-the-rotation-off-the-row', '["p2","p3","p1"]'],
  ['history-missing-rotation-is-none', null],
  ['history-bad-rotation-is-none', '{not json'],
  ['history-empty-rotation', '[]'],
] as Array<[string, string | null]>) {
  emit({
    kind: 'history',
    id,
    cycleOrderParticipantIds: cycleJson,
    out: calculateTurnStateFromHistory({
      messages: [],
      participants: asParts(ROOM),
      userParticipantId: null,
      spokenThisCycleParticipantIds: '[]',
      cycleOrderParticipantIds: cycleJson ?? undefined,
    }),
  });
}

for (const [id, role, pid, extra] of [
  ['update-strikes-the-speaker', 'ASSISTANT', 'p2', {}],
  ['update-whisper-leaves-it', 'ASSISTANT', 'p2', { targetParticipantIds: ['p1'] }],
  ['update-no-participant-leaves-it', 'ASSISTANT', null, {}],
] as Array<[string, string, string | null, Record<string, unknown>]>) {
  emit({
    kind: 'update-after-message',
    id,
    role,
    participantId: pid,
    extra,
    state: mkState({ cycleOrder: ['p1', 'p2', 'p3'], queue: ['p2'], spokenSinceUserTurn: [] }),
    out: updateTurnStateAfterMessage(
      mkState({ cycleOrder: ['p1', 'p2', 'p3'], queue: ['p2'], spokenSinceUserTurn: [] }),
      mkMessage(role, pid, extra) as unknown as MessageEvent,
      null,
    ),
  });
}

emit({
  kind: 'reset-cycle-for-user-skip',
  id: 'reset-drops-the-rotation',
  state: mkState({ cycleOrder: ['p1', 'p2'], spokenSinceUserTurn: ['p3'], queue: ['p1'], lastSpeakerId: 'p3' }),
  out: resetCycleForUserSkip(
    mkState({ cycleOrder: ['p1', 'p2'], spokenSinceUserTurn: ['p3'], queue: ['p1'], lastSpeakerId: 'p3' }),
  ),
});

// --------------------------------------------------------------------------
// turn-order: the rotation-rank comparator (step 4)
// --------------------------------------------------------------------------

/** v4's `ParticipantData` (components/chat/ParticipantCard.tsx) — the sidebar shape. */
type WireTOPart = {
  id: string;
  type: 'CHARACTER';
  controlledBy?: 'llm' | 'user';
  displayOrder: number;
  isActive: boolean;
  status?: string;
  character?: { id: string; name: string; talkativeness: number } | null;
};
const tp = (id: string, talkativeness: number, controlledBy: 'llm' | 'user' = 'llm', status = 'active'): WireTOPart => ({
  id,
  type: 'CHARACTER',
  controlledBy,
  displayOrder: 0,
  isActive: true,
  status,
  character: { id: 'char-' + id, name: 'Character ' + id, talkativeness },
});

type TurnOrderCase = {
  id: string;
  participants: WireTOPart[];
  queue: string[];
  spoken: string[];
  lastSpeakerId: string | null;
  cycleOrder: string[];
  nextSpeakerId: string | null;
  userParticipantId: string | null;
};
const TO_ROOM: WireTOPart[] = [tp('p1', 0.1), tp('p2', 0.9), tp('p3', 0.5)];
const turnOrderCases: TurnOrderCase[] = [
  // The rotation decides the order, NOT talkativeness (p2 is loudest and goes last).
  { id: 'turn-order-follows-the-rotation', participants: TO_ROOM, queue: [], spoken: [], lastSpeakerId: null, cycleOrder: ['p1', 'p3', 'p2'], nextSpeakerId: null, userParticipantId: null },
  // No rotation on file → the old talkativeness-descending guess survives.
  { id: 'turn-order-falls-back-to-talkativeness-with-no-rotation', participants: TO_ROOM, queue: [], spoken: [], lastSpeakerId: null, cycleOrder: [], nextSpeakerId: null, userParticipantId: null },
  // A seat the current cycle never dealt in sorts AFTER everyone in the rotation.
  { id: 'turn-order-a-latecomer-sorts-after-the-rotation', participants: TO_ROOM, queue: [], spoken: [], lastSpeakerId: null, cycleOrder: ['p3'], nextSpeakerId: null, userParticipantId: null },
  // Two latecomers keep the talkativeness tiebreak between themselves.
  { id: 'turn-order-two-latecomers-keep-the-talkativeness-tiebreak', participants: TO_ROOM, queue: [], spoken: [], lastSpeakerId: null, cycleOrder: ['p3'], nextSpeakerId: 'p3', userParticipantId: null },
  // The user's own seat is positioned in step 5, not by rotation rank.
  { id: 'turn-order-the-user-seat-keeps-its-own-slot', participants: [tp('p1', 0.1), tp('pu', 0.9, 'user'), tp('p3', 0.5)], queue: [], spoken: [], lastSpeakerId: null, cycleOrder: ['pu', 'p3', 'p1'], nextSpeakerId: null, userParticipantId: 'pu' },
  // The queue still jumps the line ahead of the rotation.
  { id: 'turn-order-the-queue-jumps-the-rotation', participants: TO_ROOM, queue: ['p2'], spoken: [], lastSpeakerId: null, cycleOrder: ['p1', 'p3', 'p2'], nextSpeakerId: null, userParticipantId: null },
  // Already-spoken and the last speaker drop out of the still-to-come list.
  { id: 'turn-order-spoken-and-last-drop-out', participants: TO_ROOM, queue: [], spoken: ['p1'], lastSpeakerId: 'p3', cycleOrder: ['p1', 'p3', 'p2'], nextSpeakerId: null, userParticipantId: null },
  // An inactive seat still tails with no position.
  { id: 'turn-order-inactive-tails-without-a-position', participants: [tp('p1', 0.1), tp('p2', 0.9, 'llm', 'absent'), tp('p3', 0.5)], queue: [], spoken: [], lastSpeakerId: null, cycleOrder: ['p3', 'p1'], nextSpeakerId: null, userParticipantId: null },
];
for (const c of turnOrderCases) {
  emit({
    kind: 'turn-order',
    id: c.id,
    participants: c.participants,
    queue: c.queue,
    spoken: c.spoken,
    lastSpeakerId: c.lastSpeakerId,
    cycleOrder: c.cycleOrder,
    nextSpeakerId: c.nextSpeakerId,
    userParticipantId: c.userParticipantId,
    out: computePredictedTurnOrder({
      participants: c.participants as never,
      turnState: mkState({ queue: c.queue, spokenSinceUserTurn: c.spoken, lastSpeakerId: c.lastSpeakerId, cycleOrder: c.cycleOrder }),
      turnSelectionResult: c.nextSpeakerId ? ({ nextSpeakerId: c.nextSpeakerId } as never) : null,
      isGenerating: false,
      respondingParticipantId: undefined,
      userParticipantId: c.userParticipantId,
    }),
  });
}

// --------------------------------------------------------------------------
// selection follows the rotation (step 2)
// --------------------------------------------------------------------------

const selectCases: Array<{
  id: string;
  participants: WirePart[];
  characters: WireChars;
  queue: string[];
  spoken: string[];
  lastSpeakerId: string | null;
  cycleOrder: string[];
  draws: number[];
  impersonating?: string[];
}> = [
  { id: 'select-reports-the-rotation-as-the-reason', participants: ROOM, characters: ROOM_CHARS, queue: [], spoken: [], lastSpeakerId: null, cycleOrder: ['p2', 'p3', 'p1'], draws: [0.5] },
  { id: 'select-walks-the-rotation-step-2', participants: ROOM, characters: ROOM_CHARS, queue: [], spoken: ['p2'], lastSpeakerId: 'p2', cycleOrder: ['p2', 'p3', 'p1'], draws: [0.5] },
  { id: 'select-walks-the-rotation-step-3', participants: ROOM, characters: ROOM_CHARS, queue: [], spoken: ['p2', 'p3'], lastSpeakerId: 'p3', cycleOrder: ['p2', 'p3', 'p1'], draws: [0.5] },
  { id: 'select-queue-jumps-ahead-of-the-rotation', participants: ROOM, characters: ROOM_CHARS, queue: ['p3'], spoken: [], lastSpeakerId: null, cycleOrder: ['p1', 'p2', 'p3'], draws: [0.5] },
  { id: 'select-falls-back-to-the-weighted-pick-with-no-rotation', participants: ROOM, characters: ROOM_CHARS, queue: [], spoken: [], lastSpeakerId: null, cycleOrder: [], draws: [0.5] },
  { id: 'select-falls-back-when-the-rotation-is-spent', participants: ROOM, characters: ROOM_CHARS, queue: [], spoken: ['p1', 'p2', 'p3'], lastSpeakerId: 'p3', cycleOrder: ['p1', 'p2', 'p3'], draws: [0.5] },
  { id: 'select-pauses-when-the-rotation-reaches-a-user-driven-seat', participants: [p('p1', 'char-1'), p('p2', 'char-2', 'user'), p('p3', 'char-3')], characters: ROOM_CHARS, queue: [], spoken: [], lastSpeakerId: null, cycleOrder: ['p2', 'p1', 'p3'], draws: [0.5] },
  { id: 'select-pauses-on-an-impersonated-seat-in-the-rotation', participants: ROOM, characters: ROOM_CHARS, queue: [], spoken: [], lastSpeakerId: null, cycleOrder: ['p2', 'p1', 'p3'], draws: [0.5], impersonating: ['p2'] },
  { id: 'select-a-rotation-naming-only-stale-seats-falls-back', participants: ROOM, characters: ROOM_CHARS, queue: [], spoken: [], lastSpeakerId: null, cycleOrder: ['gone-1', 'gone-2'], draws: [0.1] },
  { id: 'select-only-character-beats-the-rotation', participants: [p('p1', 'char-1')], characters: ROOM_CHARS, queue: [], spoken: [], lastSpeakerId: null, cycleOrder: ['p1'], draws: [0.5] },
  { id: 'select-rotation-skips-an-archived-seat', participants: ROOM, characters: { ...ROOM_CHARS, 'char-2': { talkativeness: 0.5, archivedAt: '2026-08-10T00:00:00.000Z' } }, queue: [], spoken: [], lastSpeakerId: null, cycleOrder: ['p2', 'p3', 'p1'], draws: [0.5] },
];
for (const c of selectCases) {
  const { out, consumed } = withRandom(c.draws, () =>
    selectNextSpeaker(
      asParts(c.participants),
      asChars(c.characters),
      mkState({ queue: c.queue, spokenSinceUserTurn: c.spoken, lastSpeakerId: c.lastSpeakerId, cycleOrder: c.cycleOrder }),
      null,
      c.impersonating,
    ),
  );
  emit({
    kind: 'select',
    id: c.id,
    participants: c.participants,
    characters: c.characters,
    queue: c.queue,
    spoken: c.spoken,
    lastSpeakerId: c.lastSpeakerId,
    cycleOrder: c.cycleOrder,
    draws: c.draws,
    impersonating: c.impersonating ?? null,
    out,
    explanation: getSelectionExplanation(out),
    consumedDraws: consumed,
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
