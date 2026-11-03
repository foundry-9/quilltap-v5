/**
 * Oracle case (tier 1, P4.D347): v4's absent-character name chips —
 * `presenceOf`, `spokenText`, `findAbsentMentions` (`lib/chat/absent-mentions.ts`,
 * `9a88f862f`) over the REAL `offSceneCardFields` (`host-notifications/writer.ts`)
 * and the REAL `stripToolArtifacts`.
 *
 * Rows:
 *   { kind: 'presence', id, participants, chatType, personaName, probes, out }
 *     — `out[i]` is `isInRoom(probes[i])`;
 *   { kind: 'spoken', id, input, out };
 *   { kind: 'find', id, messages, candidates, participants, chatType,
 *     personaName, userCharacterName, out } — presence through the real
 *     `presenceOf`, `out` the mentions in v4's order.
 *
 * Run from inside the (pinned) server checkout:
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   npx tsx $V5W/harness/oracle/cases/absent-mentions.ts \
 *     > /tmp/oracle-absent-mentions.ndjson
 */

import { findAbsentMentions, presenceOf, spokenText, type MentionCandidate } from '@/lib/chat/absent-mentions';
import type { ChatParticipantBase } from '@/lib/schemas/chat.types';

const rows: unknown[] = [];
type Probe = { id: string; name: string };
type ChatType = 'salon' | 'autonomous' | 'help' | undefined;

const seat = (id: string, characterId: string | null, status = 'active') => ({ id, characterId, status, type: 'CHARACTER' });

function presence(id: string, participants: unknown[] | null, chatType: ChatType, personaName: string | null | undefined, probes: Probe[]) {
  const isInRoom = presenceOf({
    participants: participants as ChatParticipantBase[] | null,
    chatType: chatType as never,
    personaName,
  });
  rows.push({ kind: 'presence', id, participants, chatType: chatType ?? null, personaName: personaName ?? null, probes, out: probes.map(p => isInRoom(p)) });
}

function spoken(id: string, input: { type?: string; role?: string; content?: string; systemSender?: string | null }) {
  rows.push({ kind: 'spoken', id, input, out: spokenText(input) });
}

function find(
  id: string,
  messages: Array<Record<string, unknown>>,
  candidates: MentionCandidate[],
  participants: unknown[],
  chatType: ChatType,
  personaName: string | null,
  userCharacterName: string | null,
) {
  const out = findAbsentMentions({
    messages: messages as never,
    candidates,
    isInRoom: presenceOf({ participants: participants as ChatParticipantBase[], chatType: chatType as never, personaName }),
    userCharacterName,
  });
  rows.push({ kind: 'find', id, messages, candidates, participants, chatType: chatType ?? null, personaName, userCharacterName, out });
}

const she = { subject: 'she', object: 'her', possessive: 'hers' };
const cand = (o: Partial<MentionCandidate> & { id: string; name: string }): MentionCandidate =>
  ({ aliases: [], pronouns: null, identity: null, description: null, ...o }) as MentionCandidate;
const friday = cand({ id: 'friday', name: 'Friday' });
const charlie = cand({ id: 'charlie', name: 'Charlie' });
const abigail = cand({ id: 'abigail', name: 'Abigail', aliases: ['Abby'], pronouns: she, identity: 'Married to {{user}}.' });
const vergil = cand({ id: 'vergil', name: 'Vergil', description: '  {{char}} is an AI.  ' });

// ---- presenceOf -------------------------------------------------------------
presence('v4-seated-not-removed', [seat('p1', 'friday'), seat('p2', 'vergil', 'removed')], 'salon', undefined, [friday, vergil]);
presence('v4-persona-salon', [], 'salon', 'Charlie', [charlie]);
presence('v4-persona-autonomous-bug-172', [], 'autonomous', 'Charlie', [charlie]);
presence('persona-case-and-whitespace', [], 'salon', '  CHARLIE ', [charlie, { id: 'c2', name: ' charlie  ' }]);
presence('persona-empty-name', [], 'salon', '', [{ id: 'e', name: '' }]);
presence('persona-absent-chat-type', [], undefined, 'Charlie', [charlie]);
presence('seat-without-character', [seat('p1', null)], 'salon', null, [friday]);
presence('null-participants', null, 'salon', null, [friday]);
presence('absent-status-is-seated', [seat('p1', 'friday', 'absent'), seat('p2', 'vergil', 'silent')], 'salon', null, [friday, vergil]);

// ---- spokenText -------------------------------------------------------------
spoken('v4-user', { type: 'message', role: 'USER', content: 'hi' });
spoken('v4-assistant', { type: 'message', role: 'ASSISTANT', content: 'yo there' });
spoken('v4-staff', { type: 'message', role: 'ASSISTANT', content: 'Abby', systemSender: 'host' });
spoken('v4-context-summary', { type: 'context-summary', role: 'USER', content: 'x' });
spoken('v4-tool-role', { type: 'message', role: 'TOOL', content: 'x' });
spoken('no-type', { role: 'USER', content: 'typeless' });
spoken('empty-content', { type: 'message', role: 'USER', content: '' });
spoken('empty-sender-is-falsy', { type: 'message', role: 'USER', content: 'hi', systemSender: '' });
spoken('lower-role', { type: 'message', role: 'user', content: 'lower' });
spoken('tool-only-assistant', { type: 'message', role: 'ASSISTANT', content: '[Tool call made]\n{"a": 1}' });
spoken('short-assistant-under-20', { type: 'message', role: 'ASSISTANT', content: '[Tool call made] ok' });
spoken('assistant-with-tool-marker', { type: 'message', role: 'ASSISTANT', content: '[Tool call made]\nAbigail has gone to fetch the ledger now.' });

// ---- findAbsentMentions -----------------------------------------------------
const v4Messages = [
  { id: 'm1', type: 'message', role: 'ASSISTANT', content: 'The Host names Vergil.', systemSender: 'host' },
  { id: 'm2', type: 'message', role: 'USER', content: 'Friday, Abby says hi.' },
  { id: 'm3', type: 'message', role: 'ASSISTANT', content: 'Abigail and Vergil, then.' },
];
find('v4-first-spoken-mention-as-written', v4Messages, [friday, charlie, abigail, vergil], [seat('p1', 'friday')], 'salon', 'Charlie', 'Charlie');
find('v4-no-id-and-in-room', [{ type: 'message', role: 'USER', content: 'Abby' }], [abigail], [], 'salon', null, null);
find('two-names-one-message-reverse-order', [
  { id: 'm1', type: 'message', role: 'USER', content: 'Abigail and Vergil went out.' },
], [abigail, vergil], [], 'salon', null, 'Charlie');
find('staff-row-skipped', [
  { id: 'm1', type: 'message', role: 'ASSISTANT', content: 'Vergil, said the Host.', systemSender: 'host' },
  { id: 'm2', type: 'message', role: 'USER', content: 'Who is Vergil?' },
], [vergil], [], 'salon', null, null);
find('second-mention-not-repeated', [
  { id: 'm1', type: 'message', role: 'USER', content: 'Vergil.' },
  { id: 'm2', type: 'message', role: 'USER', content: 'Vergil again.' },
], [vergil], [], 'salon', null, null);
find('alias-hit-as-written', [
  { id: 'm1', type: 'message', role: 'USER', content: 'Abby came.' },
], [abigail], [], 'salon', null, 'Charlie');
find('row-without-id', [
  { type: 'message', role: 'USER', content: 'Vergil' },
  { id: '', type: 'message', role: 'USER', content: 'Vergil' },
  { id: 'm3', type: 'message', role: 'USER', content: 'and Vergil' },
], [vergil], [], 'salon', null, null);
find('persona-named-in-salon', [
  { id: 'm1', type: 'message', role: 'ASSISTANT', content: 'Charlie is here, love.' },
], [charlie], [], 'salon', 'Charlie', 'Charlie');
find('persona-named-in-autonomous', [
  { id: 'm1', type: 'message', role: 'ASSISTANT', content: 'Charlie is away, love.' },
], [charlie], [], 'autonomous', 'Charlie', 'Charlie');
find('no-candidates', [{ id: 'm1', type: 'message', role: 'USER', content: 'Vergil' }], [], [], 'salon', null, null);
find('card-pronouns-and-description', [
  { id: 'm1', type: 'message', role: 'USER', content: 'Hello Abigail and Vergil.' },
], [abigail, vergil], [], 'salon', null, null);
find('three-in-one-message', [
  { id: 'm1', type: 'message', role: 'USER', content: 'Ann, Bea and Cy.' },
], [cand({ id: 'a', name: 'Ann' }), cand({ id: 'b', name: 'Bea' }), cand({ id: 'c', name: 'Cy' })], [], 'salon', null, null);
find('seated-candidate-skipped', [
  { id: 'm1', type: 'message', role: 'USER', content: 'Friday and Vergil.' },
], [friday, vergil], [seat('p1', 'friday')], 'salon', null, null);

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
