/**
 * Oracle case (P4.D172): the room's character map — one batched read, every
 * present seat (v4 `d14da3a56`, bug 131).
 *
 * Drives v4's REAL `lib/chat/turn-manager/room-characters.ts` at the
 * `78b381a96` pin over a counting stand-in for `repos.characters.findByIds`,
 * exactly as v4's own `room-characters.test.ts` does. The stub is what makes the
 * CALL COUNT a comparand: "reads the whole room in ONE call, not one per seat"
 * is the point of the commit, and only the count can say it.
 *
 * The bug this closes: four of the six turn paths built their map from
 * `getActiveCharacterParticipants`, which returns LLM seats only, so a seat the
 * human drives never reached the map — its talkativeness fell through to 0.5 and
 * its `archivedAt` was invisible to the seat filter.
 *
 * v4's own talkativeness case is STATISTICAL (300 loud vs 300 quiet). It cannot
 * be a differential row; the `bug131-*` draw rows below make the same point
 * deterministically with pinned draws, and the Rust side mirrors v4's bounds.
 *
 * Concrete-module imports, not the barrel — see `cycle-order.ts`'s note.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/room-characters.ts \
 *     > /tmp/oracle-room-characters.ndjson
 */

import { loadRoomCharacters } from '@/lib/chat/turn-manager/room-characters';
import { cycleCandidates, drawCycleOrder } from '@/lib/chat/turn-manager/cycle-order';
import type { ChatParticipantBase, Character } from '@/lib/schemas/types';

const NOW = '2026-09-09T00:00:00.000Z';

type WirePart = {
  id: string;
  type: string;
  status: string;
  characterId: string | null;
  controlledBy: string;
  talkativeness: number | null;
};
type WireChar = { id: string; name?: string; talkativeness?: number | null; archivedAt?: string | null };

const p = (
  id: string,
  characterId: string | null,
  controlledBy = 'llm',
  status = 'active',
  type = 'CHARACTER',
  talkativeness: number | null = null,
): WirePart => ({ id, type, status, characterId, controlledBy, talkativeness });

const asParts = (ps: WirePart[]) => ps as unknown as ChatParticipantBase[];
const asChar = (c: WireChar) =>
  ({ createdAt: NOW, updatedAt: NOW, name: c.name ?? 'Character ' + c.id, ...c } as unknown as Character);

/** The counting stand-in for the batched read — v4's own `makeRepos`. */
const makeRepos = (rows: WireChar[]) => {
  const calls: string[][] = [];
  const repos = {
    characters: {
      findByIds: async (ids: string[]) => {
        calls.push([...ids]);
        return rows.filter((c) => ids.includes(c.id)).map(asChar);
      },
    },
  };
  return { repos, calls };
};

type LoadCase = {
  id: string;
  participants: WirePart[];
  rows: WireChar[];
  preloaded?: Array<WireChar | null>;
};

const loadCases: LoadCase[] = [
  {
    id: 'includes-user-driven-seats-not-just-llm',
    participants: [p('p-llm', 'char-llm'), p('p-user', 'char-user', 'user')],
    rows: [{ id: 'char-llm' }, { id: 'char-user', talkativeness: 0.9 }],
  },
  {
    id: 'reads-the-whole-room-in-one-call',
    participants: [p('p1', 'char-1'), p('p2', 'char-2'), p('p3', 'char-3'), p('p4', 'char-4', 'user')],
    rows: [{ id: 'char-1' }, { id: 'char-2' }, { id: 'char-3' }, { id: 'char-4' }],
  },
  {
    id: 'asks-only-for-present-seats-carrying-a-character',
    participants: [
      p('p-active', 'char-1'),
      p('p-silent', 'char-2', 'llm', 'silent'),
      p('p-left', 'char-3', 'llm', 'absent'),
      p('p-removed', 'char-5', 'llm', 'removed'),
      p('p-nochar', null),
      p('p-narrator', 'char-4', 'llm', 'active', 'NARRATOR'),
    ],
    rows: [{ id: 'char-1' }, { id: 'char-2' }, { id: 'char-3' }, { id: 'char-4' }, { id: 'char-5' }],
  },
  {
    id: 'dedupes-two-seats-playing-the-same-character',
    participants: [p('p1', 'char-1'), p('p2', 'char-1')],
    rows: [{ id: 'char-1' }],
  },
  {
    id: 'skips-the-read-entirely-when-no-seat-is-present',
    participants: [p('p-left', 'char-1', 'llm', 'absent')],
    rows: [{ id: 'char-1' }],
  },
  {
    id: 'leaves-an-unreadable-character-out-of-the-map',
    participants: [p('p1', 'char-1'), p('p2', 'char-shelved')],
    rows: [{ id: 'char-1' }],
  },
  {
    id: 'prefers-a-preloaded-character-over-the-copy-the-read-returned',
    participants: [p('p1', 'char-1')],
    rows: [{ id: 'char-1', name: 'Stale' }],
    preloaded: [{ id: 'char-1', name: 'Fresh' }],
  },
  {
    id: 'ignores-empty-preloaded-entries',
    participants: [p('p1', 'char-1')],
    rows: [{ id: 'char-1', name: 'Real' }],
    preloaded: [null, null],
  },
  {
    // A preloaded character for a seat the room does NOT hold is still seeded
    // (v4 gates only on `preloaded?.id`), which is why the finalizer's
    // just-spoke seeding is safe even mid-cast-change.
    id: 'a-preloaded-stranger-is-still-seeded',
    participants: [p('p1', 'char-1')],
    rows: [{ id: 'char-1' }],
    preloaded: [{ id: 'char-stranger', name: 'Stranger' }],
  },
  {
    id: 'an-empty-room-reads-nothing',
    participants: [],
    rows: [{ id: 'char-1' }],
  },
];

const rows: unknown[] = [];

for (const c of loadCases) {
  const { repos, calls } = makeRepos(c.rows);
  const map = await loadRoomCharacters(
    repos,
    asParts(c.participants),
    c.preloaded ? { preloaded: c.preloaded.map((x) => (x ? asChar(x) : null)) } : undefined,
  );
  rows.push({
    kind: 'load',
    id: c.id,
    participants: c.participants,
    rows: c.rows,
    preloaded: c.preloaded ?? null,
    // The map, projected to what every consumer reads plus the name the chain
    // decision reads. Key ORDER is not compared (both sides are hash maps); the
    // key SET and each value are.
    out: Object.fromEntries(
      [...map.entries()].map(([id, ch]) => [
        id,
        { name: ch.name, talkativeness: ch.talkativeness ?? null, archivedAt: ch.archivedAt ?? null },
      ]),
    ),
    // The point of the commit: how many times the batched read was called, and
    // with which ids (in v4's insertion order).
    calls,
  });
}

// The two consumption arms — bug 131's actual symptoms, over a map that can see
// the user seat.
type ConsumeCase = { id: string; participants: WirePart[]; rows: WireChar[] };
const consumeCases: ConsumeCase[] = [
  {
    id: 'keeps-a-seat-whose-character-could-not-be-read',
    participants: [p('p1', 'char-1'), p('p2', 'char-shelved')],
    rows: [{ id: 'char-1' }],
  },
  {
    id: 'excludes-a-user-seat-whose-character-is-archived',
    participants: [p('p-llm', 'char-llm'), p('p-user', 'char-user', 'user')],
    rows: [{ id: 'char-llm' }, { id: 'char-user', archivedAt: NOW }],
  },
];
for (const c of consumeCases) {
  const { repos } = makeRepos(c.rows);
  const map = await loadRoomCharacters(repos, asParts(c.participants));
  rows.push({
    kind: 'candidates',
    id: c.id,
    participants: c.participants,
    rows: c.rows,
    out: cycleCandidates(asParts(c.participants), map).map((x) => x.id),
  });
}

// bug 131 deterministically: the SAME room drawn with the SAME pinned draw, once
// with a loud user seat and once with a quiet one. A map built from LLM seats
// alone weights the human at 0.5 either way, so the two rows come out identical;
// only a whole-room map separates them. (v4's own case measures this over 300
// draws; these two rows are the same claim with the RNG held still.)
for (const [id, userTalk, pin] of [
  // The pin is CHOSEN, not arbitrary: at 0.8 the whole-room map gives
  // `p-user` for the loud seat and `p-llm-2` for the quiet one, while an
  // LLM-only map — which weights the human at the 0.5 default whatever their
  // character says — gives `p-user` for BOTH. So the two rows differing IS the
  // fix, and their agreeing is the bug.
  ['bug131-loud-user-seat', 0.98, 0.8],
  ['bug131-quiet-user-seat', 0.02, 0.8],
] as Array<[string, number, number]>) {
  const participants = [
    p('p-llm-1', 'char-llm-1'),
    p('p-llm-2', 'char-llm-2'),
    p('p-user', 'char-user', 'user'),
  ];
  const rowsIn: WireChar[] = [
    { id: 'char-llm-1', talkativeness: 0.98 },
    { id: 'char-llm-2', talkativeness: 0.98 },
    { id: 'char-user', talkativeness: userTalk },
  ];
  const { repos } = makeRepos(rowsIn);
  const map = await loadRoomCharacters(repos, asParts(participants));
  const orig = Math.random;
  const consumed: number[] = [];
  Math.random = () => {
    consumed.push(pin);
    return pin;
  };
  let order: string[];
  try {
    order = drawCycleOrder({ participants: asParts(participants), characters: map });
  } finally {
    Math.random = orig;
  }
  rows.push({ kind: 'draw', id, participants, rows: rowsIn, draws: [pin], out: order, consumedDraws: consumed });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
