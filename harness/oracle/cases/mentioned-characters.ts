/**
 * Oracle case #23 (Wave 5 / B14): findMentionedCharacterIds.
 *
 * Drives the REAL matcher:
 *   findMentionedCharacterIds (lib/chat/context/mentioned-characters.ts)
 * which builds `\b(?:tok1|tok2|…)\b` (gi, no u) longest-token-first and maps
 * lowercased hits back to character ids. The corpus exercises alias matches,
 * shared aliases, longest-first preference, case-insensitivity, ASCII-`\b`
 * boundaries, the non-ASCII-trailing-boundary quirk, and dedup across
 * candidates.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/mentioned-characters.ts \
 *     > /tmp/oracle-mentioned-characters.ndjson
 */

import { findMentionedCharacterIds } from '@/lib/chat/context/mentioned-characters';
import type { Character } from '@/lib/schemas/types';

interface Cand {
  id: string;
  name: string;
  aliases?: string[];
}
const asCandidates = (cs: Cand[]): Character[] => cs as unknown as Character[];

const rows: unknown[] = [];

const cases: Array<[string, string, Cand[]]> = [
  ['simple-name', 'I think Bob is here', [{ id: 'c1', name: 'Bob' }]],
  ['case-insensitive', 'i think BOB is here', [{ id: 'c1', name: 'Bob' }]],
  ['no-mention', 'nobody is here', [{ id: 'c1', name: 'Bob' }]],
  ['alias-match', 'where is Bobby', [{ id: 'c1', name: 'Bob', aliases: ['Bobby'] }]],
  [
    'shared-alias-two-ids',
    'the Captain spoke',
    [
      { id: 'c1', name: 'Alice', aliases: ['Captain'] },
      { id: 'c2', name: 'Bob', aliases: ['Captain'] },
    ],
  ],
  [
    'longest-first',
    'I saw John Smith downtown',
    [
      { id: 'c1', name: 'John' },
      { id: 'c2', name: 'John Smith' },
    ],
  ],
  ['substring-not-matched', 'a whole category', [{ id: 'c1', name: 'cat' }]],
  ['punctuation-boundary', 'Bob, Alice, and Bob', [
    { id: 'c1', name: 'Bob' },
    { id: 'c2', name: 'Alice' },
  ]],
  ['unicode-trailing-quirk', 'I met José today', [{ id: 'c1', name: 'José' }]],
  ['ascii-name-ok', 'I met Jose today', [{ id: 'c1', name: 'Jose' }]],
  ['multiple-mentions-one-id', 'Bob and Bob and Bob', [{ id: 'c1', name: 'Bob' }]],
  ['empty-corpus', '', [{ id: 'c1', name: 'Bob' }]],
  ['no-candidates', 'Bob is here', []],
  ['empty-names-skipped', 'nothing matches', [{ id: 'c1', name: '   ', aliases: ['', '  '] }]],
  [
    'dedup-token-across-candidates',
    'the Twin appears',
    [
      { id: 'c1', name: 'Twin' },
      { id: 'c2', name: 'Twin' },
    ],
  ],
  ['hyphen-boundary', 'Bob-like behavior', [{ id: 'c1', name: 'Bob' }]],
  ['underscore-no-match', 'Bob_42 logged in', [{ id: 'c1', name: 'Bob' }]],
];

for (const [id, corpus, cands] of cases) {
  const result = findMentionedCharacterIds(corpus, asCandidates(cands));
  rows.push({
    kind: 'mentioned',
    id,
    corpus,
    candidates: cands,
    out: [...result].sort(),
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
