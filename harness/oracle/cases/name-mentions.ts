/**
 * Oracle case (tier 1, P4.D347): v4's name matcher — `buildNameMatcher`,
 * `firstNameMatch`, `lastNameMatch` (`lib/chat/name-mentions.ts`, `9a88f862f`).
 *
 * Drives the REAL module. Each row is one matcher (a name and its aliases)
 * over one text: `{ kind: 'match', id, names, text, matcher, first, last }`,
 * where `matcher` is whether `buildNameMatcher` returned one and `first` /
 * `last` are `{ index, text }` (UTF-16 index) or null.
 *
 * The corpus is the retired `mentioned-characters.ts` corpus RE-CUT against the
 * new contract (one matcher per candidate — case-sensitive, Unicode-letter
 * bounded, no longer a lowercased id map), plus the order's rows: longest-first,
 * José (now a match), an astral-plane name, a name inside a word, a name after
 * `_`, lower-case `vergil` vs `Vergil`, empty / whitespace-only tokens, a
 * regex-metacharacter name, first vs last on a repeated name, and a UTF-16
 * index after a surrogate pair. v4 ships no `name-mentions.test.ts` at the pin.
 *
 * Run from inside the (pinned) server checkout:
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   npx tsx $V5W/harness/oracle/cases/name-mentions.ts \
 *     > /tmp/oracle-name-mentions.ndjson
 */

import { buildNameMatcher, firstNameMatch, lastNameMatch } from '@/lib/chat/name-mentions';

type Names = Array<string | null>;
const rows: unknown[] = [];

function row(id: string, names: Names, text: string) {
  const matcher = buildNameMatcher(names);
  rows.push({
    kind: 'match',
    id,
    names,
    text,
    matcher: matcher !== null,
    first: matcher ? firstNameMatch(text, matcher) : null,
    last: matcher ? lastNameMatch(text, matcher) : null,
  });
}

// --- The retired mentioned-characters corpus, re-cut (one row per candidate).
const recut: Array<[string, string, Array<{ id: string; name: string; aliases?: string[] }>]> = [
  ['simple-name', 'I think Bob is here', [{ id: 'c1', name: 'Bob' }]],
  ['case-insensitive', 'i think BOB is here', [{ id: 'c1', name: 'Bob' }]],
  ['no-mention', 'nobody is here', [{ id: 'c1', name: 'Bob' }]],
  ['alias-match', 'where is Bobby', [{ id: 'c1', name: 'Bob', aliases: ['Bobby'] }]],
  ['shared-alias-two-ids', 'the Captain spoke', [
    { id: 'c1', name: 'Alice', aliases: ['Captain'] },
    { id: 'c2', name: 'Bob', aliases: ['Captain'] },
  ]],
  ['longest-first', 'I saw John Smith downtown', [{ id: 'c1', name: 'John', aliases: ['John Smith'] }]],
  ['substring-not-matched', 'a whole category', [{ id: 'c1', name: 'cat' }]],
  ['punctuation-boundary', 'Bob, Alice, and Bob', [
    { id: 'c1', name: 'Bob' },
    { id: 'c2', name: 'Alice' },
  ]],
  ['unicode-trailing-quirk', 'I met José today', [{ id: 'c1', name: 'José' }]],
  ['ascii-name-ok', 'I met Jose today', [{ id: 'c1', name: 'Jose' }]],
  ['multiple-mentions-one-id', 'Bob and Bob and Bob', [{ id: 'c1', name: 'Bob' }]],
  ['empty-corpus', '', [{ id: 'c1', name: 'Bob' }]],
  ['empty-names-skipped', 'nothing matches', [{ id: 'c1', name: '   ', aliases: ['', '  '] }]],
  ['dedup-token-across-candidates', 'the Twin appears', [
    { id: 'c1', name: 'Twin' },
    { id: 'c2', name: 'Twin' },
  ]],
  ['hyphen-boundary', 'Bob-like behavior', [{ id: 'c1', name: 'Bob' }]],
  ['underscore-no-match', 'Bob_42 logged in', [{ id: 'c1', name: 'Bob' }]],
];
for (const [id, text, cands] of recut) {
  for (const c of cands) row(`recut-${id}-${c.id}`, [c.name, ...(c.aliases ?? [])], text);
}

// --- The new contract's own rows.
row('longest-first-mrs', ['Sebold', 'Mrs. Sebold'], 'Then Mrs. Sebold arrived, and Sebold left.');
row('jose-accented', ['José'], 'José, José and josé.');
row('astral-name', ['𝒜da'], 'Is 𝒜da here? 𝒜da!');
row('name-inside-word', ['Vergil'], 'the Vergilian verses');
row('name-after-underscore', ['Vergil'], 'see file_Vergil now');
row('name-after-digit', ['Vergil'], 'version2Vergil');
row('lower-vergil', ['Vergil'], 'not scolding vergil today');
row('upper-vergil', ['Vergil'], 'not scolding Vergil today');
row('empty-tokens', ['', '   ', null], 'anything at all');
row('whitespace-trimmed-token', ['  Ada  '], 'Ada was here');
row('metachar-name', ['Mrs. Sebold (ret.)'], 'We wrote to Mrs. Sebold (ret.) today, not Mrs! Sebold (ret.)');
row('metachar-literal-dot', ['Mrs. Sebold'], 'Mrs! Sebold arrived');
row('first-vs-last', ['Ada'], 'Ada met Bea, then Ada left.');
row('index-after-surrogate', ['Ada'], '😀 Ada and 😀😀 Ada');
row('alias-as-written', ['Abigail', 'Abby'], 'Then Abby, then Abigail.');
row('apostrophe-boundary', ['Ada'], "Ada's book");
row('combining-mark-boundary', ['Jose'], 'José smiled');
row('dedup-first-seen', ['Ann', 'Anna', 'Ann'], 'Anna and Ann');
row('equal-length-stable', ['Abe', 'Bea'], 'Bea then Abe');

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
