/**
 * Oracle case #14 (Wave 2 / B6a): all-LLM pause thresholds + participant filters.
 *
 * Drives REAL pure functions from v4's lib/chat/turn-manager/all-llm-pause.ts
 * and the participant-list helpers in utils.ts.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/turn-pause-filters.ts \
 *     > /tmp/oracle-turn-pause-filters.ndjson
 */

import {
  getNextPauseInterval,
  shouldPauseForAllLLM,
  getCurrentPauseThreshold,
  getNextPauseThreshold,
  getTurnsUntilNextPause,
} from '@/lib/chat/turn-manager/all-llm-pause';
import {
  findUserParticipant,
  findActiveUserParticipant,
  findUserControlledParticipants,
  getActiveLLMParticipants,
  getActiveCharacterParticipants,
  isMultiCharacterChat,
  isAllLLMChat,
} from '@/lib/chat/turn-manager/utils';
import type { ChatParticipantBase } from '@/lib/schemas/types';

type WirePart = { id: string; status: string; controlledBy: string; characterId: string | null };
const asParts = (ps: WirePart[]) => ps as unknown as ChatParticipantBase[];

type Row =
  | { kind: 'pause'; id: string; fn: string; turnCount: number; out: number | boolean }
  | { kind: 'find'; id: string; fn: 'user' | 'active'; participants: WirePart[]; activeId: string | null; out: string | null }
  | { kind: 'list'; id: string; fn: 'userControlled' | 'activeLLM' | 'activeChar'; participants: WirePart[]; out: string[] }
  | { kind: 'pred'; id: string; fn: 'multi' | 'allLLM'; participants: WirePart[]; out: boolean };

const rows: Row[] = [];

// ---- all-LLM pause (integer math) -----------------------------------------
const intervalCases = [0, 3, 6, 12, 24];
for (const c of intervalCases) rows.push({ kind: 'pause', id: `interval-${c}`, fn: 'nextInterval', turnCount: c, out: getNextPauseInterval(c) });

const turnCounts = [0, 1, 2, 3, 4, 5, 6, 7, 11, 12, 13, 24, 48, 49];
for (const t of turnCounts) {
  rows.push({ kind: 'pause', id: `should-${t}`, fn: 'should', turnCount: t, out: shouldPauseForAllLLM(t) });
  rows.push({ kind: 'pause', id: `current-${t}`, fn: 'current', turnCount: t, out: getCurrentPauseThreshold(t) });
  rows.push({ kind: 'pause', id: `next-${t}`, fn: 'next', turnCount: t, out: getNextPauseThreshold(t) });
  rows.push({ kind: 'pause', id: `until-${t}`, fn: 'until', turnCount: t, out: getTurnsUntilNextPause(t) });
}

// ---- participant filters ---------------------------------------------------
const p = (id: string, status: string, controlledBy: string, characterId: string | null): WirePart => ({ id, status, controlledBy, characterId });

const mixed: WirePart[] = [
  p('u1', 'active', 'user', 'char-u1'),
  p('c1', 'active', 'llm', 'char-1'),
  p('c2', 'silent', 'llm', 'char-2'),
  p('u2', 'active', 'user', 'char-u2'),
  p('c3', 'absent', 'llm', 'char-3'), // not present
  p('c4', 'active', 'llm', null), // no characterId → not active-LLM
];
const oneUserOnly: WirePart[] = [p('u1', 'active', 'user', 'char-u1')];
const allLLM: WirePart[] = [p('c1', 'active', 'llm', 'char-1'), p('c2', 'active', 'llm', 'char-2')];
const empty: WirePart[] = [];

const findUserId = (ps: WirePart[]): string | null => {
  const r = findUserParticipant(asParts(ps));
  return r ? (r as unknown as WirePart).id : null;
};
const findActiveId = (ps: WirePart[], activeId: string | null): string | null => {
  const r = findActiveUserParticipant(asParts(ps), activeId);
  return r ? (r as unknown as WirePart).id : null;
};
const ids = (rs: ChatParticipantBase[]): string[] => rs.map(r => (r as unknown as WirePart).id);

// findUserParticipant
rows.push({ kind: 'find', id: 'user-first', fn: 'user', participants: mixed, activeId: null, out: findUserId(mixed) });
rows.push({ kind: 'find', id: 'user-none', fn: 'user', participants: allLLM, activeId: null, out: findUserId(allLLM) });

// findActiveUserParticipant
rows.push({ kind: 'find', id: 'active-selected', fn: 'active', participants: mixed, activeId: 'u2', out: findActiveId(mixed, 'u2') });
rows.push({ kind: 'find', id: 'active-fallback', fn: 'active', participants: mixed, activeId: 'nope', out: findActiveId(mixed, 'nope') }); // bad id → fallback to first user
rows.push({ kind: 'find', id: 'active-empty-id', fn: 'active', participants: mixed, activeId: '', out: findActiveId(mixed, '') }); // '' falsy → fallback
rows.push({ kind: 'find', id: 'active-null', fn: 'active', participants: mixed, activeId: null, out: findActiveId(mixed, null) });

// list filters
for (const [id, ps] of [['mixed', mixed], ['allLLM', allLLM], ['empty', empty]] as Array<[string, WirePart[]]>) {
  rows.push({ kind: 'list', id: `userCtrl-${id}`, fn: 'userControlled', participants: ps, out: ids(findUserControlledParticipants(asParts(ps))) });
  rows.push({ kind: 'list', id: `activeLLM-${id}`, fn: 'activeLLM', participants: ps, out: ids(getActiveLLMParticipants(asParts(ps))) });
  rows.push({ kind: 'list', id: `activeChar-${id}`, fn: 'activeChar', participants: ps, out: ids(getActiveCharacterParticipants(asParts(ps))) });
}

// predicates
for (const [id, ps] of [['mixed', mixed], ['allLLM', allLLM], ['oneUser', oneUserOnly], ['empty', empty]] as Array<[string, WirePart[]]>) {
  rows.push({ kind: 'pred', id: `multi-${id}`, fn: 'multi', participants: ps, out: isMultiCharacterChat(asParts(ps)) });
  rows.push({ kind: 'pred', id: `allLLM-${id}`, fn: 'allLLM', participants: ps, out: isAllLLMChat(asParts(ps)) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
