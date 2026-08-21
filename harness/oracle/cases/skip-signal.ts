/**
 * Oracle case (P4.d2): the "nothing to add" turn-skipping pure module.
 *
 * Drives the REAL exports of v4's lib/chat/turn-manager/skip-signal.ts
 * (02865bdb — incl. the trailing-sentinel strip arm of detectSkipSentinel):
 * detectSkipSentinel, isTurnPassMessage,
 * findSkippedSinceLastSubstantive, qualifiesForTurnSkipping,
 * isFirstCharacterTurn, isRecentlyAddressed, computeSkipEligibility.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/skip-signal.ts \
 *     > /tmp/oracle-skip-signal.ndjson
 */

import {
  detectSkipSentinel,
  isTurnPassMessage,
  findSkippedSinceLastSubstantive,
  qualifiesForTurnSkipping,
  isFirstCharacterTurn,
  isRecentlyAddressed,
  computeSkipEligibility,
} from '@/lib/chat/turn-manager/skip-signal';
import { buildTurnSkipInstruction } from '@/lib/chat/context-manager';
import type { ChatEvent, ChatParticipantBase, Character } from '@/lib/schemas/types';

type Row =
  | { kind: 'detect'; id: string; response: string; characterName?: string; aliases?: string[]; out: { skip: boolean; cleaned?: string } }
  | { kind: 'turnPass'; id: string; event: unknown; out: boolean }
  | { kind: 'skippedSince'; id: string; events: unknown[]; out: string[] }
  | { kind: 'qualifies'; id: string; participants: unknown[]; out: boolean }
  | { kind: 'firstTurn'; id: string; events: unknown[]; out: boolean }
  | { kind: 'recentlyAddressed'; id: string; events: unknown[]; respondingParticipantId: string; character: { id: string; name: string; aliases?: string[] }; out: boolean }
  | { kind: 'turnSkipNote'; id: string; characterName: string; recentlyAddressed: boolean; out: string }
  | {
      kind: 'eligibility'; id: string; events: unknown[]; participants: unknown[];
      respondingParticipantId: string; character: { id: string; name: string; aliases?: string[] };
      summoned: boolean; turnSkippingEnabled: boolean;
      out: { offerSkip: boolean; mustSpeakReason: string | null; recentlyAddressed: boolean };
    };

const rows: Row[] = [];
const asEvents = (e: unknown[]) => e as unknown as ChatEvent[];
const asParts = (p: unknown[]) => p as unknown as ChatParticipantBase[];
const asChar = (c: { id: string; name: string; aliases?: string[] }) => c as unknown as Character;

// ---- detectSkipSentinel -----------------------------------------------------
const detect = (id: string, response: string, characterName?: string, aliases?: string[]) => {
  const out = detectSkipSentinel(response, characterName, aliases);
  rows.push({ kind: 'detect', id, response, characterName, aliases, out });
};

detect('bare', '[NOTHING TO ADD]');
detect('bare-lower', '[nothing to add]');
detect('bare-mixed-case', '[Nothing To Add]');
detect('bracketless', 'NOTHING TO ADD');
detect('bracketless-punct', 'nothing to add.');
detect('trailing-punct-multi', '[NOTHING TO ADD]!?.');
detect('bold-wrap', '**[NOTHING TO ADD]**');
detect('italic-quote-wrap', '"_[nothing to add]_"');
detect('backtick-wrap', '`[NOTHING TO ADD]`');
detect('mixed-wrap', '**[NOTHING TO ADD]_');
detect('tilde-wrap', '~nothing to add~');
detect('leading-blank-lines', '\n\n  \n[NOTHING TO ADD]');
detect('trailing-whitespace-only', '[NOTHING TO ADD]\n   \n\t');
detect('sentinel-plus-prose', '[NOTHING TO ADD]\nActually, one more thing worth saying.');
detect('sentinel-plus-prose-blank', '[NOTHING TO ADD]\n\nActually, wait.');
detect('prose-then-sentinel', 'I have things to say.\n[NOTHING TO ADD]');
detect('mid-message-not-first', 'Well.\n\n[NOTHING TO ADD]\n\nMore.');
detect('not-sentinel', 'Something to add!');
detect('empty', '');
detect('whitespace-only', '   \n \t ');
detect('inner-punct-not-match', '[NOTHING, TO ADD]');
detect('name-prefix', '[Ada]\n[NOTHING TO ADD]', 'Ada');
detect('name-colon-prefix', 'Ada: [NOTHING TO ADD]', 'Ada');
detect('alias-prefix', '[The Countess] [NOTHING TO ADD]', 'Ada', ['The Countess']);
detect('alias-only-no-name', '[The Countess]\nnothing to add', undefined, ['The Countess']);
detect('no-name-guard', '[NOTHING TO ADD]', undefined, undefined);
detect('no-name-empty-aliases', '[NOTHING TO ADD]', undefined, []);
detect('empty-name-with-alias', '[NOTHING TO ADD]', '', ['Fri']);
detect('content-block-json', '[{"type": "text", "text": "[NOTHING TO ADD]"}]');
detect('content-block-python', `[{'type': 'text', 'text': "[NOTHING TO ADD]"}]`, 'Ada');
detect('name-prefix-prose', '[Ada]\n[NOTHING TO ADD]\nBut actually…', 'Ada');
detect('sentinel-inside-prose-line', 'I say [NOTHING TO ADD] loudly.');
detect('double-bracket', '[[NOTHING TO ADD]]');
detect('unicode-wrapper-adjacent', '«[NOTHING TO ADD]»');
detect('sentinel-crlf', '[NOTHING TO ADD]\r\nmore prose');
// Trailing-sentinel arm (v4 02865bdb): prose that merely ENDS with a lone
// sentinel line → NOT a skip; the trailing sentinel line is stripped.
detect('narration-then-trailing-sentinel', '*I stay where I am, my hand on his arm.*\n\n*There is nothing I need to add.*\n\n[NOTHING TO ADD]');
detect('trailing-sentinel-markdown-wrapped', '*She nods once and says nothing more.*\n\n**[nothing to add]**');
detect('trailing-sentinel-bare', 'I have things to say.\n[NOTHING TO ADD]');
detect('mid-line-phrase-not-stripped', 'There is nothing to add here, but I will speak anyway.');
detect('final-line-not-sentinel-no-clean', 'A real line.\n\nAnother real line.');
detect('sentinel-not-final-line-no-clean', 'Well.\n\n[NOTHING TO ADD]\n\nMore.');
detect('trailing-sentinel-then-whitespace', 'Real prose here.\n\n[NOTHING TO ADD]\n   \n\t');

// ---- isTurnPassMessage ------------------------------------------------------
const tp = (id: string, event: unknown) => rows.push({ kind: 'turnPass', id, event, out: isTurnPassMessage(event) });
const passRecord = (participantId: unknown) => ({
  type: 'message', id: 'm-x', role: 'ASSISTANT', content: 'The Host notes a pass.',
  systemSender: 'host', systemKind: 'turn-pass', participantId: null,
  hostEvent: { participantId },
});
tp('good', passRecord('p1'));
tp('empty-string-pid', passRecord(''));
tp('numeric-pid', passRecord(7));
tp('missing-host-event', { type: 'message', systemSender: 'host', systemKind: 'turn-pass' });
tp('null-host-event', { type: 'message', systemSender: 'host', systemKind: 'turn-pass', hostEvent: null });
tp('wrong-kind', { type: 'message', systemSender: 'host', systemKind: 'status-change', hostEvent: { participantId: 'p1' } });
tp('wrong-sender', { type: 'message', systemSender: 'librarian', systemKind: 'turn-pass', hostEvent: { participantId: 'p1' } });
tp('wrong-type', { type: 'system', systemSender: 'host', systemKind: 'turn-pass', hostEvent: { participantId: 'p1' } });
tp('not-object', 'nope');
tp('null', null);

// ---- findSkippedSinceLastSubstantive ---------------------------------------
const msg = (role: string, participantId: string | null, extra: Record<string, unknown> = {}) => ({
  type: 'message', role, participantId, content: 'hello', ...extra,
});
const skippedSince = (id: string, events: unknown[]) =>
  rows.push({ kind: 'skippedSince', id, events, out: Array.from(findSkippedSinceLastSubstantive(asEvents(events))).sort() });

skippedSince('empty', []);
skippedSince('single-pass', [msg('USER', 'u1'), passRecord('p1')]);
skippedSince('two-passes', [msg('USER', 'u1'), passRecord('p1'), passRecord('p2')]);
skippedSince('substantive-terminates', [passRecord('p1'), msg('ASSISTANT', 'p2'), passRecord('p3')]);
skippedSince('whisper-does-not-terminate', [
  passRecord('p1'),
  msg('ASSISTANT', 'p2', { targetParticipantIds: ['p3'] }),
  passRecord('p3'),
]);
skippedSince('empty-targets-terminates', [passRecord('p1'), msg('USER', 'u1', { targetParticipantIds: [] }), passRecord('p3')]);
skippedSince('null-pid-does-not-terminate', [passRecord('p1'), msg('ASSISTANT', null), passRecord('p3')]);
skippedSince('non-message-skipped', [passRecord('p1'), { type: 'system', role: 'USER', participantId: 'u1' }, passRecord('p3')]);
skippedSince('dup-pass', [passRecord('p1'), passRecord('p1')]);

// ---- qualifiesForTurnSkipping ----------------------------------------------
const part = (id: string, opts: Record<string, unknown> = {}) => ({
  id, type: 'CHARACTER', status: 'active', characterId: `char-${id}`, controlledBy: 'llm', ...opts,
});
const qual = (id: string, participants: unknown[]) =>
  rows.push({ kind: 'qualifies', id, participants, out: qualifiesForTurnSkipping(asParts(participants)) });

qual('duet-human-llm', [part('p1', { controlledBy: 'user' }), part('p2')]);
qual('two-llm', [part('p1'), part('p2')]);
qual('three-chars-one-llm', [part('p1', { controlledBy: 'user' }), part('p2', { controlledBy: 'user' }), part('p3')]);
qual('single-llm', [part('p1')]);
qual('two-llm-one-removed', [part('p1'), part('p2', { status: 'removed' }), part('p3', { controlledBy: 'user' })]);
qual('silent-counts', [part('p1', { status: 'silent' }), part('p2')]);
qual('absent-not-counted', [part('p1', { status: 'absent' }), part('p2'), part('p3', { controlledBy: 'user' })]);
qual('no-character-id', [part('p1', { characterId: null }), part('p2'), part('p3', { controlledBy: 'user' })]);
qual('non-character-type', [part('p1', { type: 'USER' }), part('p2'), part('p3')]);
qual('empty', []);
qual('missing-controlled-by', [
  { id: 'p1', type: 'CHARACTER', status: 'active', characterId: 'c1' },
  { id: 'p2', type: 'CHARACTER', status: 'active', characterId: 'c2' },
]);

// ---- isFirstCharacterTurn ---------------------------------------------------
const firstTurn = (id: string, events: unknown[]) =>
  rows.push({ kind: 'firstTurn', id, events, out: isFirstCharacterTurn(asEvents(events)) });

firstTurn('empty', []);
firstTurn('user-only', [msg('USER', 'u1')]);
firstTurn('greeting-counts', [msg('ASSISTANT', 'p1')]);
firstTurn('staff-does-not-count', [msg('ASSISTANT', null, { systemSender: 'host', systemKind: 'scenario' })]);
firstTurn('assistant-no-pid', [msg('ASSISTANT', null)]);
firstTurn('non-message', [{ type: 'context-summary', role: 'ASSISTANT', participantId: 'p1' }]);
firstTurn('mixed', [msg('USER', 'u1'), msg('ASSISTANT', null, { systemSender: 'host' }), msg('ASSISTANT', 'p1')]);

// ---- isRecentlyAddressed ----------------------------------------------------
const ada = { id: 'char-ada', name: 'Ada', aliases: ['The Countess'] };
const recently = (
  id: string,
  events: unknown[],
  respondingParticipantId: string,
  character: { id: string; name: string; aliases?: string[] } = ada,
) =>
  rows.push({
    kind: 'recentlyAddressed', id, events, respondingParticipantId, character,
    out: isRecentlyAddressed(asEvents(events), respondingParticipantId, asChar(character)),
  });

recently('empty', [], 'p-ada');
recently('mention-hit', [msg('USER', 'u1', { content: 'What does Ada think?' })], 'p-ada');
recently('alias-hit', [msg('USER', 'u1', { content: 'Ask the countess about it.' })], 'p-ada');
recently('no-mention', [msg('USER', 'u1', { content: 'Nothing about her here.' })], 'p-ada');
recently('word-boundary-miss', [msg('USER', 'u1', { content: 'The armada sailed.' })], 'p-ada');
recently('targeted-whisper-hit', [msg('USER', 'u1', { content: 'psst', targetParticipantIds: ['p-ada'] })], 'p-ada');
recently('whisper-away-invisible', [msg('USER', 'u1', { content: 'Ada is great', targetParticipantIds: ['p-basil'] })], 'p-ada');
recently('own-message-boundary', [
  msg('USER', 'u1', { content: 'Ada, hello!' }),
  msg('ASSISTANT', 'p-ada', { content: 'Hello.' }),
  msg('USER', 'u1', { content: 'Basil, your thoughts?' }),
], 'p-ada');
recently('own-whisper-not-boundary', [
  msg('USER', 'u1', { content: 'Ada, hello!' }),
  msg('ASSISTANT', 'p-ada', { content: 'A whisper.', targetParticipantIds: ['u1'] }),
], 'p-ada');
recently('own-staff-not-boundary', [
  msg('USER', 'u1', { content: 'Ada, hello!' }),
  msg('ASSISTANT', 'p-ada', { content: 'Staff note.', systemSender: 'host' }),
], 'p-ada');
recently('silent-message-invisible', [msg('USER', 'u1', { content: 'Ada!', isSilentMessage: true })], 'p-ada');
recently('empty-content-invisible', [msg('USER', 'u1', { content: '   ' })], 'p-ada');
recently('staff-invisible', [msg('ASSISTANT', null, { content: 'Ada is mentioned by the Host.', systemSender: 'host' })], 'p-ada');
// Lookback cap: the mention is 11 visible turns back → outside the window of 10.
const lookbackEvents: unknown[] = [msg('USER', 'u1', { content: 'Ada, are you there?' })];
for (let i = 0; i < 10; i++) lookbackEvents.push(msg('USER', 'u1', { content: `filler ${i}` }));
recently('lookback-cap-miss', lookbackEvents, 'p-ada');
const lookbackEvents2: unknown[] = [msg('USER', 'u1', { content: 'irrelevant' }), msg('USER', 'u1', { content: 'Ada, are you there?' })];
for (let i = 0; i < 9; i++) lookbackEvents2.push(msg('USER', 'u1', { content: `filler ${i}` }));
recently('lookback-cap-hit', lookbackEvents2, 'p-ada');

// --- the direct-address rewrite (v4 e22f7b36) --------------------------------
// v4's own eight new suite shapes, 1:1, over its `alice` character.
const alice = { id: 'char-alice', name: 'Alice', aliases: ['Al'] };
const said = (content: string) => [
  msg('ASSISTANT', 'p-alice', { content: 'I spoke earlier.' }),
  msg('USER', 'p-user', { content }),
];
recently('vocative-by-name', said('Alice, what do you think?'), 'p-alice', alice);
recently('vocative-alias-lead-in', said('Hey Al, over here.'), 'p-alice', alice);
recently('vocative-mid-sentence', said('Look at me last, Alice.'), 'p-alice', alice);
recently('vocative-bare-trailing-question', said('Someone has to run the pod. Alice?'), 'p-alice', alice);
recently('vocative-opens-quoted-line', said('*I turn to her.* "Alice — the three originals are clean."'), 'p-alice', alice);
recently('at-mention', said('Let me ask @Alice: is the pod ready?'), 'p-alice', alice);
recently('third-person-mid-sentence', said('I wonder if Alice is ready to operate the stasis pod tonight.'), 'p-alice', alice);
recently('possessive-roll-call', said("I've been listening through the round — Alice's three joints, the pod read, the heartbeat."), 'p-alice', alice);

// The vocative alternation's own arms.
recently('vocative-after-emdash', said('She paused — Alice, are you well?'), 'p-alice', alice);
recently('lead-in-without-comma', said('well Alice, hi'), 'p-alice', alice);
recently('mid-sentence-no-punctuation', said('I glance at Alice over the bench'), 'p-alice', alice);
recently('vocative-after-newline-in-corpus', [
  msg('ASSISTANT', 'p-alice', { content: 'I spoke earlier.' }),
  msg('USER', 'p-user', { content: 'nothing to see here' }),
  msg('USER', 'p-user', { content: 'Alice?' }),
], 'p-alice', alice);

// Longest-first alternation: name and alias overlap at the same position.
const alOverlap = { id: 'char-al', name: 'Al', aliases: ['Alice'] };
recently('overlap-longer-token', said('Hey Alice.'), 'p-alice', alOverlap);
recently('overlap-shorter-token', said('Hey Al.'), 'p-alice', alOverlap);

// No usable name tokens → buildDirectAddressRegex returns null.
const nameless = { id: 'char-nameless', name: '', aliases: [] as string[] };
recently('no-name-tokens', said('Hey there, anyone about?'), 'p-alice', nameless);
recently('no-name-tokens-whisper-still-hits', [
  msg('ASSISTANT', 'p-alice', { content: 'I spoke earlier.' }),
  msg('USER', 'p-user', { content: 'psst', targetParticipantIds: ['p-alice'] }),
], 'p-alice', nameless);
recently('whitespace-name-alias-carries', said('Al, hello.'), 'p-alice', { id: 'char-ws', name: '   ', aliases: ['Al'] });

// JS-regex fidelity: non-ASCII case folding, regex metacharacters in the name,
// and the `m`-flag / `\s` edges the corpus can reach through message content.
const zoe = { id: 'char-zoe', name: 'Zoë', aliases: [] as string[] };
recently('nonascii-uppercased', said('ZOË, hi'), 'p-alice', zoe);
recently('nonascii-mid-sentence', said('we asked zoë about it'), 'p-alice', zoe);
const drStrange = { id: 'char-dr', name: 'Dr. Strange?', aliases: [] as string[] };
recently('regex-metachar-name', said('Hey Dr. Strange?, are you there?'), 'p-alice', drStrange);
recently('regex-metachar-name-not-literal', said('Hey Dr@Strange!, are you there?'), 'p-alice', drStrange);
recently('crlf-end-of-line', said('Hey Alice\r\nand then we left'), 'p-alice', alice);
recently('cr-only-end-of-line', said('Hey Alice\rand then we left'), 'p-alice', alice);
recently('cr-only-line-start', said('we waited\rAlice, are you there?'), 'p-alice', alice);
recently('ls-end-of-line', said('Hey Alice\u2028and then we left'), 'p-alice', alice);
recently('bom-after-name', said('Hey Alice\ufeff'), 'p-alice', alice);
recently('nel-after-name', said('Hey Alice\u0085'), 'p-alice', alice);
recently('nbsp-after-name', said('Hey Alice\u00a0'), 'p-alice', alice);

// ---- buildTurnSkipInstruction (v4 lib/chat/context-manager.ts) --------------
// The ephemeral Turn note's exact bytes — both the base (which e22f7b36 grew a
// closing paragraph) and the reworded caution.
const note = (id: string, characterName: string, recentlyAddressed: boolean) =>
  rows.push({
    kind: 'turnSkipNote', id, characterName, recentlyAddressed,
    out: buildTurnSkipInstruction(characterName, recentlyAddressed),
  });

note('base', 'Ada', false);
note('with-caution', 'Ada', true);
note('caution-name-interpolation', 'Dr. $1 Strange', true);
note('caution-empty-name', '', true);

// ---- computeSkipEligibility -------------------------------------------------
const elig = (
  id: string,
  events: unknown[],
  participants: unknown[],
  respondingParticipantId: string,
  opts: { summoned?: boolean; turnSkippingEnabled?: boolean; character?: { id: string; name: string; aliases?: string[] } } = {},
) => {
  const character = opts.character ?? ada;
  const out = computeSkipEligibility({
    events: asEvents(events),
    participants: asParts(participants),
    respondingParticipantId,
    respondingCharacter: asChar(character),
    summoned: opts.summoned,
    turnSkippingEnabled: opts.turnSkippingEnabled ?? true,
  });
  rows.push({
    kind: 'eligibility', id, events, participants, respondingParticipantId, character,
    summoned: opts.summoned ?? false, turnSkippingEnabled: opts.turnSkippingEnabled ?? true,
    out: { offerSkip: out.offerSkip, mustSpeakReason: out.mustSpeakReason, recentlyAddressed: out.recentlyAddressed },
  });
};

const groupParts = [part('p-ada'), part('p-basil'), part('p-user', { controlledBy: 'user' })];
const duetParts = [part('p-ada'), part('p-user', { controlledBy: 'user' })];
const baseEvents = [msg('USER', 'p-user', { content: 'Good evening, everyone.' }), msg('ASSISTANT', 'p-basil', { content: 'Evening.' })];

// Precedence matrix, in order.
elig('not-multi-character', baseEvents, duetParts, 'p-ada');
elig('feature-disabled', baseEvents, groupParts, 'p-ada', { turnSkippingEnabled: false });
elig('first-character-turn', [msg('USER', 'p-user', { content: 'hi' })], groupParts, 'p-ada');
elig('summoned', baseEvents, groupParts, 'p-ada', { summoned: true });
elig('already-skipped', [...baseEvents, passRecord('p-ada')], groupParts, 'p-ada');
elig('all-others-skipped', [...baseEvents, passRecord('p-basil'), passRecord('p-user')], groupParts, 'p-ada');
elig('offer', baseEvents, groupParts, 'p-ada');
elig('offer-recently-addressed', [...baseEvents, msg('USER', 'p-user', { content: 'Ada?' })], groupParts, 'p-ada');
// Withheld reasons still compute recentlyAddressed (unconditional first).
elig('disabled-but-addressed', [...baseEvents, msg('USER', 'p-user', { content: 'Ada?' })], groupParts, 'p-ada', { turnSkippingEnabled: false });
// One other has skipped, one hasn't → still offered.
elig('one-other-skipped', [...baseEvents, passRecord('p-basil')], groupParts, 'p-ada');
// Substantive message clears the pass slate → offered again.
elig('pass-then-substantive', [...baseEvents, passRecord('p-ada'), msg('ASSISTANT', 'p-basil', { content: 'I spoke.' })], groupParts, 'p-ada');
// The stall rule counts user-controlled characters too: the LLM other passed AND
// the user-controlled other passed (via the Skip button) → all-others-skipped;
// with only the LLM other passed (above, `one-other-skipped`) the offer stands.
// NOTE the literal vacuous-`.every()` (ZERO other active characters) is
// unreachable through computeSkipEligibility with a consistent roster —
// qualification guarantees ≥2 active characters, so at least one "other"
// always exists; the two-passes `all-others-skipped` case above exercises the
// same `.every()` beyond the trivial length. The why-comment (skipping into an
// empty room is forbidden) is carried in both implementations.
// Responder not in the roster (defensive): every active char is an "other".
elig('responder-not-in-roster', [...baseEvents, passRecord('p-ada'), passRecord('p-basil'), passRecord('p-user')], groupParts, 'p-ghost');

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
