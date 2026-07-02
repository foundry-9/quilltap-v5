/**
 * Oracle case: turn-manager pure predicates (chat-orchestration wave 1.1,
 * Part B).
 *
 * Drives the REAL pure functions from v4's lib/chat/turn-manager/utils.ts:
 *   isUsersTurn, isParticipantsTurn, getSelectionExplanation.
 * All three are side-effect-free.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/turn-predicates.ts \
 *     > /tmp/oracle-turn-predicates.ndjson
 */

import {
  isUsersTurn,
  isParticipantsTurn,
  getSelectionExplanation,
} from '@/lib/chat/turn-manager/utils';
import type { TurnState, TurnSelectionResult } from '@/lib/chat/turn-manager/types';

type Reason = TurnSelectionResult['reason'];

type UsersTurnRow = {
  kind: 'users_turn';
  id: string;
  nextSpeakerId: string | null;
  reason: string;
  out: boolean;
};

type ParticipantsTurnRow = {
  kind: 'participants_turn';
  id: string;
  nextSpeakerId: string | null;
  reason: string;
  participantId: string;
  out: boolean;
};

type ExplanationRow = {
  kind: 'explanation';
  id: string;
  reason: string;
  out: string;
};

type Row = UsersTurnRow | ParticipantsTurnRow | ExplanationRow;

const rows: Row[] = [];

// A minimal selection result. `isParticipantsTurn`'s `state` arg is unused by
// the implementation, so a throwaway empty state suffices.
const dummyState: TurnState = {
  spokenSinceUserTurn: [],
  currentTurnParticipantId: null,
  queue: [],
  lastSpeakerId: null,
};

const mkResult = (nextSpeakerId: string | null, reason: string): TurnSelectionResult => ({
  nextSpeakerId,
  reason: reason as Reason,
  cycleComplete: false,
});

const reasons: string[] = [
  'queue',
  'weighted_selection',
  'only_character',
  'user_turn',
  'cycle_complete',
  'totally_unknown_reason', // exercises the default branch
];

// ---- isUsersTurn ----------------------------------------------------------
// Cross every reason with a picked id and with null.
const usersTurnCases: Array<[string, string | null, string]> = [];
for (const reason of reasons) {
  usersTurnCases.push([`id-${reason}`, 'p1', reason]);
  usersTurnCases.push([`null-${reason}`, null, reason]);
}
// Empty-string id is truthy-ish? In JS, `'' === null` is false, so nextSpeakerId
// of '' with a non-user reason is NOT the user's turn.
usersTurnCases.push(['empty-id-weighted', '', 'weighted_selection']);
usersTurnCases.push(['empty-id-user-turn', '', 'user_turn']);
for (const [id, nextSpeakerId, reason] of usersTurnCases) {
  rows.push({ kind: 'users_turn', id, nextSpeakerId, reason, out: isUsersTurn(mkResult(nextSpeakerId, reason)) });
}

// ---- isParticipantsTurn ---------------------------------------------------
const participantsTurnCases: Array<[string, string | null, string, string]> = [
  ['match', 'p1', 'weighted_selection', 'p1'],
  ['no-match', 'p1', 'weighted_selection', 'p2'],
  ['null-next-vs-id', null, 'user_turn', 'p1'],
  ['user-turn-matches-own-id', 'p1', 'user_turn', 'p1'], // user char keeps its id
  ['queue-match', 'p2', 'queue', 'p2'],
  ['only-character-match', 'solo', 'only_character', 'solo'],
  ['empty-string-vs-empty', '', 'weighted_selection', ''], // '' === '' true
  ['empty-string-vs-id', '', 'weighted_selection', 'p1'],
  ['nonascii-id-match', 'Élise-𝕏', 'weighted_selection', 'Élise-𝕏'],
  ['nonascii-id-no-match', 'Élise', 'weighted_selection', 'élise'], // case-sensitive
  ['astral-id', '🎩🚂', 'queue', '🎩🚂'],
];
for (const [id, nextSpeakerId, reason, participantId] of participantsTurnCases) {
  rows.push({
    kind: 'participants_turn',
    id,
    nextSpeakerId,
    reason,
    participantId,
    out: isParticipantsTurn(dummyState, participantId, mkResult(nextSpeakerId, reason)),
  });
}

// ---- getSelectionExplanation ----------------------------------------------
for (const reason of reasons) {
  rows.push({ kind: 'explanation', id: `expl-${reason}`, reason, out: getSelectionExplanation(mkResult('p1', reason)) });
}
// A couple more unknowns for the default branch.
for (const reason of ['', 'QUEUE', 'weighted', 'Cycle_Complete']) {
  rows.push({ kind: 'explanation', id: `expl-unknown-${reason || 'empty'}`, reason, out: getSelectionExplanation(mkResult('p1', reason)) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
