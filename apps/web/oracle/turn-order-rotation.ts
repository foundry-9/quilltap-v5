/**
 * The v4-side recorder behind `src/app/chat/turn-order-rotation.oracle.spec.ts`.
 *
 * Drives v4's REAL `computePredictedTurnOrder` (the drawn-rotation step 4)
 * over the four scenarios its own `turn-order.test.ts` added at `2aca73ad6`,
 * plus v4's REAL `parseCycleOrder` (`lib/chat/turn-manager/cycle-order.ts`) —
 * the client mechanism divergence's other half (P4.D177 §C.2): v5's SPA has
 * no `calculateTurnStateFromHistory`, so `chat/turn-order.ts`'s hand-rolled
 * `parseCycleOrder` twin is checked directly against v4's.
 *
 * Run it from a pinned v4 worktree:
 *
 * ```bash
 * PIN=/tmp/qt-v4-pin-p4d177-78b381a96
 * cp <V5>/apps/web/oracle/turn-order-rotation.ts "$PIN/"
 * cd "$PIN" && npx tsx turn-order-rotation.ts \
 *   > <V5>/apps/web/src/testing/fixtures/turn-order-rotation.oracle.ndjson
 * ```
 *
 * Expect 14 lines.
 */

import { computePredictedTurnOrder } from './lib/chat/turn-manager/turn-order';
import { createInitialTurnState } from './lib/chat/turn-manager/state';
import { parseCycleOrder } from './lib/chat/turn-manager/cycle-order';
import type { TurnState } from './lib/chat/turn-manager/types';
import type { ParticipantData } from './components/chat/ParticipantCard';

function createCharacter(
  id: string,
  name: string,
  talkativeness = 0.5,
  isActive = true,
  controlledBy: 'llm' | 'user' = 'llm',
): ParticipantData {
  return {
    id,
    name,
    controlledBy,
    status: isActive ? 'active' : 'absent',
    character: { talkativeness },
  } as unknown as ParticipantData;
}

function createTurnState(overrides: Partial<TurnState> = {}): TurnState {
  return { ...createInitialTurnState(), ...overrides };
}

const CASES: Array<
  [
    string,
    {
      participants: ParticipantData[];
      turnState: TurnState;
      turnSelectionResult: null;
      isGenerating: boolean;
      respondingParticipantId: string | null;
      userParticipantId: string | null;
    },
  ]
> = [
  [
    'rotation-not-talkativeness',
    {
      participants: [
        createCharacter('alice', 'Alice', 0.9),
        createCharacter('bob', 'Bob', 0.6),
        createCharacter('carol', 'Carol', 0.2),
      ],
      turnState: createTurnState({ cycleOrder: ['carol', 'alice', 'bob'] }),
      turnSelectionResult: null,
      isGenerating: false,
      respondingParticipantId: null,
      userParticipantId: null,
    },
  ],
  [
    'generating-head-rotation-behind',
    {
      participants: [
        createCharacter('alice', 'Alice', 0.9),
        createCharacter('bob', 'Bob', 0.6),
        createCharacter('carol', 'Carol', 0.2),
      ],
      turnState: createTurnState({ cycleOrder: ['carol', 'bob'], lastSpeakerId: 'alice' }),
      turnSelectionResult: null,
      isGenerating: true,
      respondingParticipantId: 'alice',
      userParticipantId: null,
    },
  ],
  [
    'latecomer-behind-dealt-in',
    {
      participants: [
        createCharacter('alice', 'Alice', 0.9),
        createCharacter('bob', 'Bob', 0.6),
        createCharacter('carol', 'Carol', 0.2),
      ],
      turnState: createTurnState({ cycleOrder: ['carol', 'bob'] }),
      turnSelectionResult: null,
      isGenerating: false,
      respondingParticipantId: null,
      userParticipantId: null,
    },
  ],
  [
    'no-rotation-falls-back-to-talkativeness',
    {
      participants: [
        createCharacter('carol', 'Carol', 0.2),
        createCharacter('alice', 'Alice', 0.9),
        createCharacter('bob', 'Bob', 0.6),
      ],
      turnState: createTurnState(),
      turnSelectionResult: null,
      isGenerating: false,
      respondingParticipantId: null,
      userParticipantId: null,
    },
  ],
];

for (const [id, options] of CASES) {
  const result = computePredictedTurnOrder(options);
  console.log(
    JSON.stringify({
      kind: 'rotation',
      id,
      participantIds: JSON.stringify(result.map((e) => e.participantId)),
      positions: JSON.stringify(result.map((e) => e.position)),
      statuses: JSON.stringify(result.map((e) => e.status)),
    }),
  );
}

const PARSE_CASES: Array<[string, string | null | undefined]> = [
  ['absent', undefined],
  ['null', null],
  ['empty-string', ''],
  ['literal-empty-array', '[]'],
  ['bad-json', '{not json'],
  ['non-array-object', '{"a":1}'],
  ['non-array-number', '42'],
  ['strings-only', '["a","b","c"]'],
  ['mixed-drops-non-strings', '["a",1,"b",null,"c"]'],
  ['all-non-strings', '[1,2,3]'],
];
for (const [id, raw] of PARSE_CASES) {
  console.log(JSON.stringify({ kind: 'parse-cycle-order', id, raw: raw === undefined ? null : raw, out: JSON.stringify(parseCycleOrder(raw)) }));
}
