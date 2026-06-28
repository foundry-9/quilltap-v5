/**
 * Oracle case #22 (Wave 5 / B13): word-boundary about-character name matchers.
 *
 * Drives the REAL regex matchers:
 *   nameAppears, countNameOccurrences, resolveAboutCharacterId
 *     (lib/memory/about-character-resolution.ts)
 * These use the Unicode word-boundary + lookahead regex
 * `(^|[^\p{L}\p{N}_])NAME(?=$|[^\p{L}\p{N}_])`. The corpus exercises
 * case-insensitivity, punctuation/underscore/digit boundaries, Unicode letters,
 * adjacency (shared single delimiter), and the resolve rule order.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/about-character-matchers.ts \
 *     > /tmp/oracle-about-character-matchers.ndjson
 */

import {
  nameAppears,
  countNameOccurrences,
  resolveAboutCharacterId,
} from '@/lib/memory/about-character-resolution';
import type { Character } from '@/lib/schemas/types';

const rows: unknown[] = [];

// ---- nameAppears / countNameOccurrences -----------------------------------
const matchCases: Array<[string, string[], string]> = [
  ['simple', ['Bob'], 'I saw Bob today'],
  ['case-insensitive', ['bob'], 'I saw BOB today'],
  ['substring-rejected', ['cat'], 'a whole category of things'],
  ['leading-period', ['Bob'], 'hi .Bob there'],
  ['punctuation-both', ['Bob'], 'Bob, where are you, Bob?'],
  ['adjacent-shared-delim', ['Bob'], 'Bob Bob Bob'],
  ['glued-no-match', ['Bob'], 'BobBob'],
  ['underscore-is-word', ['bob'], 'bob_smith was here'],
  ['digit-after-rejected', ['Agent'], 'Agent7 reporting'],
  ['unicode-name', ['José'], 'I met José yesterday'],
  ['unicode-digit-after', ['José'], 'codename José5 active'],
  ['unicode-case-fold', ['naïve'], 'so NAÏVE of them'],
  ['multiple-names-sum', ['Bob', 'Alice'], 'Bob met Alice and Bob again'],
  ['regex-special-name', ['A.B'], 'see A.B here but not AxB'],
  ['empty-haystack', ['Bob'], ''],
  ['whitespace-name-skipped', ['  ', 'Bob'], 'Bob is here'],
  ['hyphen-boundary', ['Bob'], 'Bob-Alice pairing'],
  ['name-at-end', ['Bob'], 'the man called Bob'],
  ['name-at-start', ['Bob'], 'Bob arrived'],
  ['duplicate-name-doublecounts', ['Bob', 'Bob'], 'just Bob'],
];
for (const [id, names, haystack] of matchCases) {
  rows.push({ kind: 'appears', id, names, haystack, out: nameAppears(names, haystack) });
  rows.push({ kind: 'count', id, names, haystack, out: countNameOccurrences(names, haystack) });
}

// ---- resolveAboutCharacterId ----------------------------------------------
type AboutChar = Pick<Character, 'name' | 'aliases' | 'controlledBy'>;
type HolderChar = Pick<Character, 'name' | 'aliases'>;
const about = (name: string, aliases: string[], controlledBy: string): AboutChar =>
  ({ name, aliases, controlledBy } as unknown as AboutChar);
const holder = (name: string, aliases: string[]): HolderChar =>
  ({ name, aliases } as unknown as HolderChar);

interface ResolveCase {
  id: string;
  holderCharacterId: string;
  holderCharacter: HolderChar | null;
  proposedAboutCharacterId: string | null;
  proposedAboutCharacter: AboutChar | null;
  text: string;
}
const resolveCases: ResolveCase[] = [
  {
    id: 'null-proposal',
    holderCharacterId: 'h1',
    holderCharacter: holder('Friday', []),
    proposedAboutCharacterId: null,
    proposedAboutCharacter: null,
    text: 'anything',
  },
  {
    id: 'self-reference',
    holderCharacterId: 'h1',
    holderCharacter: holder('Friday', []),
    proposedAboutCharacterId: 'h1',
    proposedAboutCharacter: about('Friday', [], 'llm'),
    text: 'Friday did a thing',
  },
  {
    id: 'about-data-unavailable',
    holderCharacterId: 'h1',
    holderCharacter: holder('Friday', []),
    proposedAboutCharacterId: 'a1',
    proposedAboutCharacter: null,
    text: 'someone did a thing',
  },
  {
    id: 'about-absent-flips-to-holder',
    holderCharacterId: 'h1',
    holderCharacter: holder('Friday', []),
    proposedAboutCharacterId: 'a1',
    proposedAboutCharacter: about('Alice', ['Ally'], 'llm'),
    text: 'Friday cooked dinner',
  },
  {
    id: 'holder-dominates',
    holderCharacterId: 'h1',
    holderCharacter: holder('Friday', []),
    proposedAboutCharacterId: 'a1',
    proposedAboutCharacter: about('Alice', [], 'llm'),
    text: 'Friday and Friday and Alice talked, then Friday left',
  },
  {
    id: 'about-present-keep',
    holderCharacterId: 'h1',
    holderCharacter: holder('Friday', []),
    proposedAboutCharacterId: 'a1',
    proposedAboutCharacter: about('Alice', [], 'llm'),
    text: 'Alice and Alice talked while Friday watched',
  },
  {
    id: 'tie-goes-to-about',
    holderCharacterId: 'h1',
    holderCharacter: holder('Friday', []),
    proposedAboutCharacterId: 'a1',
    proposedAboutCharacter: about('Alice', [], 'llm'),
    text: 'Friday saw Alice',
  },
  {
    id: 'user-generic-aliases',
    holderCharacterId: 'h1',
    holderCharacter: holder('Friday', []),
    proposedAboutCharacterId: 'a1',
    proposedAboutCharacter: about('Bertie', [], 'user'),
    text: 'the user asked a question',
  },
  {
    id: 'no-holder-no-tiebreak',
    holderCharacterId: 'h1',
    holderCharacter: null,
    proposedAboutCharacterId: 'a1',
    proposedAboutCharacter: about('Alice', [], 'llm'),
    text: 'Friday and Friday and Alice, then Friday',
  },
];
for (const c of resolveCases) {
  const out = resolveAboutCharacterId({
    holderCharacterId: c.holderCharacterId,
    holderCharacter: c.holderCharacter,
    proposedAboutCharacterId: c.proposedAboutCharacterId,
    proposedAboutCharacter: c.proposedAboutCharacter,
    text: c.text,
  });
  rows.push({
    kind: 'resolve',
    id: c.id,
    holderCharacterId: c.holderCharacterId,
    holderCharacter: c.holderCharacter,
    proposedAboutCharacterId: c.proposedAboutCharacterId,
    proposedAboutCharacter: c.proposedAboutCharacter,
    text: c.text,
    out,
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
