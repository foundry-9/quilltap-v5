/**
 * Oracle case (tier 1, P4.D347): v4's referent ledger + closing echo —
 * `buildReferentLedger`, `buildClosingEcho` (`lib/chat/context/referent-ledger.ts`,
 * `f7a1789df` / `9a88f862f`).
 *
 * Drives the REAL module. Rows:
 *   { kind: 'ledger', id, lines, people, responderId, out }
 *     — a person's optional `card` string is what its `renderCard` returns
 *       (and `rendered` lists the ids whose card was actually rendered — v4
 *       renders only listed, not-in-room people);
 *   { kind: 'echo', id, newUserMessage, speakerName, out }.
 *
 * The quote (`sentenceAround`) and the where-named text (`whereNamed`) are
 * private in v4 and reach the output only through the ledger entry bytes,
 * which is where these rows pin them.
 *
 * Two rows cut a surrogate pair with a JS `slice` (the named divergences
 * `ECHO_SLICE_SURROGATE` / `SENTENCE_SLICE_SURROGATE`): v4's output carries a
 * LONE surrogate, which `JSON.stringify` writes as a `\udXXX` escape.
 *
 * Run from inside the (pinned) server checkout:
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   npx tsx $V5W/harness/oracle/cases/referent-ledger.ts \
 *     > /tmp/oracle-referent-ledger.ndjson
 */

import {
  buildClosingEcho,
  buildReferentLedger,
  CLOSING_ECHO_MAX_CHARS,
  REFERENT_LEDGER_WINDOW_LINES,
} from '@/lib/chat/context/referent-ledger';

type Pronouns = { subject: string; object: string; possessive: string };
type Person = {
  id: string;
  name: string;
  aliases?: string[] | null;
  pronouns?: Pronouns | null;
  inRoom: boolean;
  card?: string;
};
type Line = { speaker: string; isResponder: boolean; content: string };

const rows: unknown[] = [];

function ledger(id: string, lines: Line[], people: Person[], responderId: string) {
  const rendered: string[] = [];
  const out = buildReferentLedger({
    lines,
    responderId,
    people: people.map(p => ({
      id: p.id,
      name: p.name,
      aliases: p.aliases,
      pronouns: p.pronouns,
      inRoom: p.inRoom,
      renderCard:
        p.card === undefined
          ? undefined
          : () => {
              rendered.push(p.id);
              return p.card as string;
            },
    })),
  });
  rows.push({ kind: 'ledger', id, lines, people, responderId, out, rendered });
}

function echo(id: string, newUserMessage: string | null, speakerName: string) {
  const out = buildClosingEcho({ newUserMessage, speakerName });
  rows.push({ kind: 'echo', id, newUserMessage, speakerName, out });
}

const she = { subject: 'she', object: 'her', possessive: 'her' };
const he = { subject: 'he', object: 'him', possessive: 'his' };
const they = { subject: 'they', object: 'them', possessive: 'their' };

// v4's own `referent-ledger.test.ts` cast.
const people: Person[] = [
  { id: 'friday', name: 'Friday', pronouns: she, inRoom: true },
  { id: 'charlie', name: 'Charlie', pronouns: he, inRoom: true },
  { id: 'abigail', name: 'Abigail', aliases: ['Abby', 'The Fourth'], pronouns: she, inRoom: false },
  { id: 'vergil', name: 'Vergil', aliases: ['Gyro'], pronouns: he, inRoom: false },
  { id: 'greg', name: 'Greg', aliases: ['Doc'], pronouns: he, inRoom: false },
];
const withCards: Person[] = people.map(p => ({
  ...p,
  ...(p.inRoom ? {} : { card: `### ${p.name}\nCard for ${p.name}.` }),
}));
const L = (speaker: string, isResponder: boolean, content: string): Line => ({ speaker, isResponder, content });

// ---- v4's `it`s, transcribed ------------------------------------------------
ledger('v4-lists-off-stage-most-recent-first', [
  L('Friday', true, "You told Greg before you told me. If you say I don't have a preference, Abigail will absolutely make that a formal finding. You've seen how she documents."),
  L('Charlie', false, '"When she\'s not scolding Vergil for his bedside manner."\n\nI grin.'),
], people, 'friday');
ledger('v4-leaves-out-responder-and-latest-author', [
  L('Friday', true, 'Charlie, love. Friday is my name.'),
  L('Charlie', false, 'Friday, Charlie here. Abby says hello.'),
], people, 'friday');
ledger('v4-marks-in-room-and-counts-lines-back', [
  L('Asa', false, 'Charlie said to wait.'),
  L('Friday', true, 'Then we wait.'),
  L('Abigail', false, 'Fine.'),
], people, 'friday');
ledger('v4-case-sensitive-word-boundaries', [
  L('Charlie', false, 'Check the doc, then the Vergilian verses, then the greg-orian chant.'),
], people, 'friday');
ledger('v4-alias-doc', [L('Charlie', false, 'Ask Doc.')], people, 'friday');
ledger('v4-only-scans-the-window', [
  L('Charlie', false, 'Abigail was here.'),
  ...Array.from({ length: REFERENT_LEDGER_WINDOW_LINES }, () => L('Charlie', false, 'Nothing to see.')),
], people, 'friday');
ledger('v4-empty-with-no-lines', [], people, 'friday');
ledger('v4-pronouns-not-recorded', [L('Charlie', false, 'Sunny called.')], [
  ...people,
  { id: 'x', name: 'Sunny', inRoom: false },
], 'friday');
ledger('v4-cards-of-listed-absent-only', [
  L('Asa', false, 'Charlie and Abigail went to find Vergil.'),
], withCards, 'friday');
ledger('v4-no-particulars-when-all-in-room', [L('Asa', false, 'Charlie is here.')], withCards, 'friday');

// ---- the order's rows ------------------------------------------------------
ledger('motivating-case', [
  L('Friday', true, "Abigail will absolutely make that a formal finding. You've seen how she documents."),
  L('Charlie', false, "When she's not scolding Vergil"),
], people, 'friday');
ledger('tie-inside-one-line', [
  L('Asa', false, 'Vergil gave Abigail the file, and Greg watched.'),
], people, 'friday');
ledger('cap-of-eight-on-nine', [
  L('Asa', false, 'P1 P2 P3 P4 P5 P6 P7 P8 P9 all came.'),
], Array.from({ length: 9 }, (_, i) => ({ id: `p${i + 1}`, name: `P${i + 1}`, pronouns: they, inRoom: false })), 'friday');
ledger('responder-last-turn-vs-later-responder', [
  L('Friday', true, 'Abigail is on her way.'),
  L('Charlie', false, 'Good.'),
  L('Friday', true, 'Greg is not.'),
  L('Charlie', false, 'Fine.'),
], people, 'friday');
ledger('one-line-back-singular', [
  L('Asa', false, 'Vergil is late.'),
  L('Charlie', false, 'Again.'),
], people, 'friday');
ledger('five-lines-back', [
  L('Asa', false, 'Vergil is late.'),
  L('Charlie', false, 'Again.'),
  L('Asa', false, 'Yes.'),
  L('Charlie', false, 'Hm.'),
  L('Asa', false, 'Well.'),
  L('Charlie', false, 'So.'),
], people, 'friday');
ledger('latest-speaker-name-case-and-trim', [
  L('Asa', false, 'Abigail spoke to Vergil.'),
  L('  abigail ', false, 'I did.'),
], people, 'friday');
ledger('carry-closers-curly', [
  L('Asa', false, '“Vergil left.” She sighed. It was late, and the rest of this sentence is long enough to matter for nothing.'),
], people, 'friday');
ledger('carry-closers-apostrophe-star', [
  L('Asa', false, "Fine, *Greg said.*' And then he went home. Then more."),
], people, 'friday');
ledger('ellipsis-boundary', [
  L('Asa', false, 'Well… Abigail is here… really.'),
], people, 'friday');
ledger('no-carry-when-too-long', [
  L('Asa', false, `Vergil left. ${'x'.repeat(230)}. End.`),
], people, 'friday');
ledger('earlier-sentence-start', [
  L('Asa', false, 'First thing. Second thing! Third: Vergil arrives? Then more.'),
], people, 'friday');
ledger('newline-boundary-and-collapse', [
  L('Asa', false, 'Line one\n\nGreg\tcame   in\nand sat.'),
], people, 'friday');
const longSentence = (pre: number, post: number) =>
  `${'a'.repeat(pre)} Vergil ${'b'.repeat(post)}`;
ledger('long-sentence-name-at-start', [L('Asa', false, longSentence(2, 400))], people, 'friday');
ledger('long-sentence-name-in-middle', [L('Asa', false, longSentence(200, 200))], people, 'friday');
ledger('long-sentence-name-at-end', [L('Asa', false, longSentence(400, 2))], people, 'friday');
ledger('cards-with-and-without-body', [
  L('Asa', false, 'Abigail, Vergil and Greg.'),
], people.map(p =>
  p.inRoom ? p : { ...p, card: p.id === 'greg' ? '### Greg' : `### ${p.name}\nAliases: x\nBody for {{user}}.` },
), 'friday');
ledger('surrogates-in-quote', [
  L('Asa', false, '😀 Vergil smiled 😀. Next.'),
], people, 'friday');
// SENTENCE_SLICE_SURROGATE: the 220-unit window's `from` lands on the LOW half.
ledger('sentence-slice-surrogate-from', [
  L('Asa', false, `${'a'.repeat(50)}😀${'b'.repeat(108)} Vergil ${'c'.repeat(200)}`),
], people, 'friday');
// SENTENCE_SLICE_SURROGATE: the window's `to` cuts after the HIGH half.
ledger('sentence-slice-surrogate-to', [
  L('Asa', false, `${'a'.repeat(5)} Vergil ${'b'.repeat(206)}😀${'c'.repeat(100)}`),
], people, 'friday');
ledger('responder-excluded-by-id', [L('Asa', false, 'Friday and Vergil.')], people, 'friday');
ledger('duplicate-person-id', [L('Asa', false, 'Vergil and Gyro.')], [
  { id: 'vergil', name: 'Vergil', pronouns: he, inRoom: false },
  { id: 'vergil', name: 'Gyro', pronouns: he, inRoom: false },
], 'friday');
ledger('no-responder-flag-lines', [
  L('Charlie', false, 'Abigail?'),
  L('Charlie', false, 'Hello?'),
], people, 'friday');

echo('v4-quotes-whole-line', '"When she\'s not scolding Vergil."\n\nI grin.', 'Charlie');
echo('v4-empty-undefined', null, 'Charlie');
echo('v4-empty-whitespace', '   ', 'Charlie');
echo('v4-closing-paragraphs', `${'A'.repeat(CLOSING_ECHO_MAX_CHARS)}\n\nThe last word.`, 'Charlie');
echo('under-limit', 'x'.repeat(1499), 'Ann');
echo('at-limit', 'x'.repeat(1500), 'Ann');
echo('over-limit-one-paragraph', 'y'.repeat(1501), 'Ann');
echo('over-limit-three-paragraphs', `${'a'.repeat(700)}\n\n${'b'.repeat(700)}\n\n${'c'.repeat(700)}`, 'Ann');
echo('blank-paragraph', `${'a'.repeat(1400)}\n\n   \n\n${'b'.repeat(200)}`, 'Ann');
echo('first-kept-oversized', `short\n\n${'z'.repeat(1600)}`, 'Ann');
echo('crlf', `${'a'.repeat(900)}\r\n\r\n${'b'.repeat(900)}\r\nend`, 'Ann');
echo('trailing-newline', 'Hello there.\n', 'Ann');
echo('inner-blank-lines', 'one\n\n\ntwo', 'Ann');
echo('surrogates-in-echo', 'I 😀 you, 😀.', 'Ann');
// ECHO_SLICE_SURROGATE: `p.slice(-1500)` starts on the LOW half of the pair.
echo('echo-slice-surrogate', `😀${'a'.repeat(1499)}`, 'Ann');

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
