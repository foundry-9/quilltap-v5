/**
 * Oracle case #16 (Wave 2 / B7): weighted next-speaker selection.
 *
 * Drives the REAL selectNextSpeaker from v4's lib/chat/turn-manager/selection.ts.
 * Its only impurity is Math.random() inside pickWeighted; we pin Math.random to
 * a fixed value per case (controlling the input, NOT reimplementing the
 * algorithm) and emit that value so the Rust port — which takes random01 as a
 * parameter — uses the identical draw.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/select-speaker.ts \
 *     > /tmp/oracle-select-speaker.ndjson
 */

import { selectNextSpeaker } from '@/lib/chat/turn-manager/selection';
import type { TurnState, TurnSelectionResult } from '@/lib/chat/turn-manager/types';
import type { ChatParticipantBase, Character } from '@/lib/schemas/types';

type WirePart = {
  id: string;
  type: string;
  status: string;
  characterId: string | null;
  controlledBy: string;
  talkativeness: number | null;
};
// characters: map characterId -> talkativeness (null = character has none).
type WireChars = Record<string, number | null>;

const asParts = (ps: WirePart[]) => ps as unknown as ChatParticipantBase[];
const asChars = (c: WireChars): Map<string, Character> => {
  const m = new Map<string, Character>();
  for (const [cid, talk] of Object.entries(c)) {
    m.set(cid, (talk === null ? {} : { talkativeness: talk }) as unknown as Character);
  }
  return m;
};
const mkState = (queue: string[], spoken: string[], last: string | null): TurnState =>
  ({ spokenSinceUserTurn: spoken, currentTurnParticipantId: null, queue, lastSpeakerId: last } as TurnState);

function withRandom<T>(r: number, fn: () => T): T {
  const orig = Math.random;
  Math.random = () => r;
  try {
    return fn();
  } finally {
    Math.random = orig;
  }
}

type Scenario = {
  id: string;
  participants: WirePart[];
  characters: WireChars;
  queue: string[];
  spoken: string[];
  lastSpeakerId: string | null;
  random01: number;
};

const p = (id: string, type: string, status: string, characterId: string | null, controlledBy: string, talkativeness: number | null): WirePart =>
  ({ id, type, status, characterId, controlledBy, talkativeness });

// Weighted trio: A=0.9, B=0.3, C=0.8 (total 2.0).
const trio: WirePart[] = [
  p('A', 'CHARACTER', 'active', 'ca', 'llm', 0.9),
  p('B', 'CHARACTER', 'active', 'cb', 'llm', 0.3),
  p('C', 'CHARACTER', 'active', 'cc', 'llm', 0.8),
];

const scenarios: Scenario[] = [
  { id: 'queue-wins', participants: trio, characters: {}, queue: ['Z'], spoken: [], lastSpeakerId: null, random01: 0.5 },
  { id: 'no-active', participants: [p('U', 'CHARACTER', 'absent', 'cu', 'user', 0.5)], characters: {}, queue: [], spoken: [], lastSpeakerId: null, random01: 0.5 },
  { id: 'only-character', participants: [p('A', 'CHARACTER', 'active', 'ca', 'llm', 0.9)], characters: {}, queue: [], spoken: [], lastSpeakerId: 'A', random01: 0.5 },
  { id: 'only-character-user', participants: [p('U', 'CHARACTER', 'active', 'cu', 'user', 0.5)], characters: {}, queue: [], spoken: [], lastSpeakerId: null, random01: 0.5 },
  // weighted picks: r chosen to land on A / B / C respectively (rv = r*2.0).
  { id: 'weighted-A', participants: trio, characters: {}, queue: [], spoken: [], lastSpeakerId: null, random01: 0.1 }, // rv 0.2 < 0.9 → A
  { id: 'weighted-B', participants: trio, characters: {}, queue: [], spoken: [], lastSpeakerId: null, random01: 0.5 }, // rv 1.0 → B
  { id: 'weighted-C', participants: trio, characters: {}, queue: [], spoken: [], lastSpeakerId: null, random01: 0.95 }, // rv 1.9 → C
  // last speaker + spoken excluded.
  { id: 'eligible-excludes', participants: trio, characters: {}, queue: [], spoken: ['B'], lastSpeakerId: 'A', random01: 0.1 }, // only C eligible → C
  // user-controlled pick → reason user_turn, id kept.
  {
    id: 'user-pick',
    participants: [p('A', 'CHARACTER', 'active', 'ca', 'llm', 0.5), p('U', 'CHARACTER', 'active', 'cu', 'user', 0.5)],
    characters: {}, queue: [], spoken: [], lastSpeakerId: null, random01: 0.7, // rv 0.7; A0.5,then U → U
  },
  // cycle wrap: both spoken, last = A → eligible empty, newCycle = [B] → B, wrap.
  { id: 'wrap', participants: [p('A', 'CHARACTER', 'active', 'ca', 'llm', 0.9), p('B', 'CHARACTER', 'active', 'cb', 'llm', 0.3)], characters: {}, queue: [], spoken: ['A', 'B'], lastSpeakerId: 'A', random01: 0.5 },
  // total weight zero → equal weights reset to 1.
  { id: 'zero-weights', participants: [p('A', 'CHARACTER', 'active', 'ca', 'llm', 0), p('B', 'CHARACTER', 'active', 'cb', 'llm', 0)], characters: {}, queue: [], spoken: [], lastSpeakerId: null, random01: 0.6 }, // rv 1.2 → B
  // talkativeness fallback: participant null → character value → 0.5 default.
  { id: 'char-fallback', participants: [p('A', 'CHARACTER', 'active', 'ca', 'llm', null), p('B', 'CHARACTER', 'active', 'cb', 'llm', null)], characters: { ca: 0.9 }, queue: [], spoken: [], lastSpeakerId: null, random01: 0.5 }, // A0.9(char), B0.5(default) total1.4; rv0.7<0.9 → A
];

type Row = { kind: 'select'; id: string; scenario: Scenario; out: TurnSelectionResult };
const rows: Row[] = [];

for (const s of scenarios) {
  const result = withRandom(s.random01, () =>
    selectNextSpeaker(asParts(s.participants), asChars(s.characters), mkState(s.queue, s.spoken, s.lastSpeakerId), null),
  );
  rows.push({ kind: 'select', id: s.id, scenario: s, out: result });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
