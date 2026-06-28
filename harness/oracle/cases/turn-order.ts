/**
 * Oracle case #15 (Wave 2 / B6b): predicted turn order (display).
 *
 * Drives the REAL computePredictedTurnOrder from v4's
 * lib/chat/turn-manager/turn-order.ts.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/turn-order.ts \
 *     > /tmp/oracle-turn-order.ndjson
 */

import { computePredictedTurnOrder } from '@/lib/chat/turn-manager/turn-order';
import type { TurnState, TurnSelectionResult } from '@/lib/chat/turn-manager/types';

// ParticipantData-shaped (frontend type) — only id/status/controlledBy/character
// .talkativeness are read; cast a partial through unknown.
type WirePart = { id: string; status: string; controlledBy: string; talkativeness: number | null };
const mkPart = (w: WirePart) =>
  ({
    id: w.id,
    status: w.status,
    controlledBy: w.controlledBy,
    character: w.talkativeness === null ? undefined : { talkativeness: w.talkativeness },
  } as unknown);

const mkState = (queue: string[], spoken: string[], last: string | null): TurnState =>
  ({ spokenSinceUserTurn: spoken, currentTurnParticipantId: null, queue, lastSpeakerId: last } as TurnState);

type WireEntry = { participantId: string; position: number | null; status: string };

type Scenario = {
  id: string;
  participants: WirePart[];
  queue: string[];
  spoken: string[];
  lastSpeakerId: string | null;
  nextSpeakerId: string | null;
  isGenerating: boolean;
  respondingParticipantId: string | null;
  userParticipantId: string | null;
};

const p = (id: string, status: string, controlledBy: string, talkativeness: number | null): WirePart => ({ id, status, controlledBy, talkativeness });

const full: WirePart[] = [
  p('A', 'active', 'llm', 0.9),
  p('B', 'active', 'llm', 0.3),
  p('C', 'active', 'llm', 0.6),
  p('U', 'active', 'user', null),
  p('E', 'silent', 'llm', 0.7),
  p('D', 'absent', 'llm', 0.5),
];

const scenarios: Scenario[] = [
  {
    id: 'full',
    participants: full,
    queue: [],
    spoken: [],
    lastSpeakerId: 'B',
    nextSpeakerId: null,
    isGenerating: true,
    respondingParticipantId: 'A',
    userParticipantId: 'U',
  },
  {
    id: 'next-speaker',
    participants: full,
    queue: [],
    spoken: [],
    lastSpeakerId: null,
    nextSpeakerId: 'C',
    isGenerating: false,
    respondingParticipantId: null,
    userParticipantId: 'U',
  },
  {
    id: 'queue-dedup-generating',
    participants: full,
    queue: ['A', 'C'], // A is also generating → dedup keeps it as 'generating'
    spoken: [],
    lastSpeakerId: null,
    nextSpeakerId: null,
    isGenerating: true,
    respondingParticipantId: 'A',
    userParticipantId: null,
  },
  {
    id: 'talkativeness-tie',
    participants: [p('X', 'active', 'llm', 0.5), p('Y', 'active', 'llm', 0.5), p('Z', 'active', 'llm', 0.5)],
    queue: [],
    spoken: [],
    lastSpeakerId: null,
    nextSpeakerId: null,
    isGenerating: false,
    respondingParticipantId: null,
    userParticipantId: null,
  },
  {
    id: 'spoken-fallthrough',
    participants: full,
    queue: [],
    spoken: ['A', 'C', 'E'], // these become 'spoken', B eligible
    lastSpeakerId: null,
    nextSpeakerId: null,
    isGenerating: false,
    respondingParticipantId: null,
    userParticipantId: 'U',
  },
  {
    id: 'inactive-mix',
    participants: [p('A', 'active', 'llm', 0.5), p('Ab', 'absent', 'llm', 0.5), p('Rm', 'removed', 'llm', 0.5)],
    queue: [],
    spoken: [],
    lastSpeakerId: null,
    nextSpeakerId: null,
    isGenerating: false,
    respondingParticipantId: null,
    userParticipantId: null,
  },
  {
    id: 'empty',
    participants: [],
    queue: [],
    spoken: [],
    lastSpeakerId: null,
    nextSpeakerId: null,
    isGenerating: false,
    respondingParticipantId: null,
    userParticipantId: null,
  },
  {
    id: 'unknown-id-ignored',
    participants: full,
    queue: ['GHOST'], // not a participant → ignored
    spoken: [],
    lastSpeakerId: null,
    nextSpeakerId: 'PHANTOM', // not a participant → ignored
    isGenerating: false,
    respondingParticipantId: null,
    userParticipantId: 'U',
  },
];

type Row = { kind: 'order'; id: string; scenario: Scenario; out: WireEntry[] };
const rows: Row[] = [];

for (const s of scenarios) {
  const result: TurnSelectionResult = {
    nextSpeakerId: s.nextSpeakerId,
    reason: 'weighted_selection',
    cycleComplete: false,
  };
  const entries = computePredictedTurnOrder({
    participants: s.participants.map(mkPart) as never[],
    turnState: mkState(s.queue, s.spoken, s.lastSpeakerId),
    turnSelectionResult: result,
    isGenerating: s.isGenerating,
    respondingParticipantId: s.respondingParticipantId,
    userParticipantId: s.userParticipantId,
  });
  rows.push({ kind: 'order', id: s.id, scenario: s, out: entries as WireEntry[] });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
