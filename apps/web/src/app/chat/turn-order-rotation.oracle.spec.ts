/**
 * The corpus differential for the drawn-rotation half of `turn-order.ts`
 * against v4's REAL `lib/chat/turn-manager/{turn-order,cycle-order}.ts` at
 * the `78b381a96` pin — recorded by `apps/web/oracle/turn-order-rotation.ts`
 * (P4.D177 §C.2). `computePredictedTurnOrder`'s hand-transcribed cases in
 * `turn-order.spec.ts` already cover the four scenarios by value; this file
 * is the byte-for-byte cross-check against v4's own execution, plus
 * `parseCycleOrder` — the client mechanism divergence's other half, checked
 * directly since v5 never calls v4's `calculateTurnStateFromHistory`.
 */

import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/turn-order-rotation.oracle.ndjson';
import { computePredictedTurnOrder, createInitialTurnState, parseCycleOrder, type TurnOrderParticipant, type TurnState } from './turn-order';

interface RotationRow {
  kind: 'rotation';
  id: string;
  participantIds: string;
  positions: string;
  statuses: string;
}
interface ParseRow {
  kind: 'parse-cycle-order';
  id: string;
  raw: string | null;
  out: string;
}
type Row = RotationRow | ParseRow;

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

const of = <T extends Row['kind']>(kind: T) => ROWS.filter((r): r is Extract<Row, { kind: T }> => r.kind === kind);

function createCharacter(id: string, talkativeness = 0.5): TurnOrderParticipant {
  return { id, controlledBy: 'llm', status: 'active', character: { talkativeness } };
}
function createTurnState(overrides: Partial<TurnState> = {}): TurnState {
  return { ...createInitialTurnState(), ...overrides };
}

const SCENARIOS: Record<string, ReturnType<typeof computePredictedTurnOrder>> = {};
{
  const alice09 = createCharacter('alice', 0.9);
  const bob06 = createCharacter('bob', 0.6);
  const carol02 = createCharacter('carol', 0.2);

  SCENARIOS['rotation-not-talkativeness'] = computePredictedTurnOrder({
    participants: [alice09, bob06, carol02],
    turnState: createTurnState({ cycleOrder: ['carol', 'alice', 'bob'] }),
    turnSelectionResult: null,
    isGenerating: false,
    respondingParticipantId: null,
    userParticipantId: null,
  });
  SCENARIOS['generating-head-rotation-behind'] = computePredictedTurnOrder({
    participants: [alice09, bob06, carol02],
    turnState: createTurnState({ cycleOrder: ['carol', 'bob'], lastSpeakerId: 'alice' }),
    turnSelectionResult: null,
    isGenerating: true,
    respondingParticipantId: 'alice',
    userParticipantId: null,
  });
  SCENARIOS['latecomer-behind-dealt-in'] = computePredictedTurnOrder({
    participants: [alice09, bob06, carol02],
    turnState: createTurnState({ cycleOrder: ['carol', 'bob'] }),
    turnSelectionResult: null,
    isGenerating: false,
    respondingParticipantId: null,
    userParticipantId: null,
  });
  SCENARIOS['no-rotation-falls-back-to-talkativeness'] = computePredictedTurnOrder({
    participants: [createCharacter('carol', 0.2), createCharacter('alice', 0.9), createCharacter('bob', 0.6)],
    turnState: createTurnState(),
    turnSelectionResult: null,
    isGenerating: false,
    respondingParticipantId: null,
    userParticipantId: null,
  });
}

describe('turn-order-rotation agrees with v4 row for row', () => {
  it('carries the whole recorded corpus', () => {
    expect(ROWS).toHaveLength(14);
    expect(of('rotation')).toHaveLength(4);
    expect(of('parse-cycle-order')).toHaveLength(10);
  });

  it.each(of('rotation'))('rotation — $id', (row) => {
    const result = SCENARIOS[row.id];
    expect(JSON.stringify(result.map((e) => e.participantId))).toBe(row.participantIds);
    expect(JSON.stringify(result.map((e) => e.position))).toBe(row.positions);
    expect(JSON.stringify(result.map((e) => e.status))).toBe(row.statuses);
  });

  it.each(of('parse-cycle-order'))('parseCycleOrder — $id', (row) => {
    expect(JSON.stringify(parseCycleOrder(row.raw))).toBe(row.out);
  });
});
