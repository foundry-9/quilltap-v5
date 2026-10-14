/**
 * Oracle case #19 (Wave 3 / B10): pure memory name-resolution leaves.
 *
 * Drives the REAL pure helpers:
 *   reinforcedImportance           (lib/memory/scoring/reinforced-importance.ts)
 *   formatNameWithPronouns         (lib/memory/format-utils.ts)
 *   namesForAboutCharacter, namesForHolder (lib/memory/about-character-resolution.ts)
 * No impurity; no injection.
 *
 * P4.D289 re-aimed the reinforced rows at the pin (`83334b9a5`): v4
 * `e0ac7493a` moved the formula into `lib/memory/scoring/` and `c2795733f`
 * (bug 216) changed it to `log2(max(1, count))`, so six of the eight rows
 * move value (`zero-count` / `zero-base` do not). The name-helper rows are
 * untouched.
 *
 * Run from inside a v4 checkout (or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/memory-name-helpers.ts" > /tmp/oracle-memory-name-helpers.ndjson
 */

import { reinforcedImportance } from '@/lib/memory/scoring/reinforced-importance';
import { formatNameWithPronouns } from '@/lib/memory/format-utils';
import { namesForAboutCharacter, namesForHolder } from '@/lib/memory/about-character-resolution';
import type { Pronouns } from '@/lib/schemas/character.types';
import type { Character } from '@/lib/schemas/types';

const rows: unknown[] = [];

// ---- reinforcedImportance ------------------------------------------------
const reinforced: Array<[string, number, number]> = [
  ['zero-count', 0.5, 0],
  ['one', 0.5, 1],
  ['power-of-two', 0.5, 3],
  ['saturates-cap', 0.9, 64],
  ['zero-base', 0.0, 0],
  ['cap-at-7', 0.97, 7],
  ['exact-16', 0.3, 15],
  ['hundred', 0.5, 100],
];
for (const [id, base, count] of reinforced) {
  rows.push({ kind: 'reinforced', id, base, count, out: reinforcedImportance(base, count) });
}

// ---- formatNameWithPronouns ----------------------------------------------
const pr = (s: string, o: string, p: string): Pronouns => ({ subject: s, object: o, possessive: p });
const formats: Array<[string, string, Pronouns | null]> = [
  ['she-her', 'Friday', pr('she', 'her', 'her')],
  ['no-pronouns', 'Bob', null],
  ['they-them', 'Quill', pr('they', 'them', 'their')],
];
for (const [id, name, pronouns] of formats) {
  rows.push({ kind: 'format', id, name, pronouns, out: formatNameWithPronouns(name, pronouns) });
}

// ---- namesForAboutCharacter ----------------------------------------------
const asChar = (c: { name: string; aliases?: string[]; controlledBy: string }) =>
  c as unknown as Pick<Character, 'name' | 'aliases' | 'controlledBy'>;
const aboutCases: Array<[string, { name: string; aliases?: string[]; controlledBy: string }]> = [
  ['llm-with-aliases', { name: 'Friday', aliases: ['Fri', 'Friday B'], controlledBy: 'llm' }],
  ['user-adds-generic', { name: 'Bertie', aliases: ['Wooster'], controlledBy: 'user' }],
  ['drops-empty-aliases', { name: 'Jeeves', aliases: ['', '   ', 'J'], controlledBy: 'llm' }],
  ['keeps-untrimmed', { name: ' Friday ', aliases: [], controlledBy: 'llm' }],
  ['no-aliases-field', { name: 'Solo', controlledBy: 'user' }],
];
for (const [id, c] of aboutCases) {
  rows.push({ kind: 'about', id, character: c, out: namesForAboutCharacter(asChar(c)) });
}

// ---- namesForHolder ------------------------------------------------------
const asHolder = (c: { name: string; aliases?: string[] }) =>
  c as unknown as Pick<Character, 'name' | 'aliases'>;
const holderCases: Array<[string, { name: string; aliases?: string[] }]> = [
  ['with-aliases', { name: 'Holder', aliases: ['H', ''] }],
  ['no-user-aliases-ever', { name: 'Carina', aliases: ['C'] }],
  ['no-aliases-field', { name: 'Aurora' }],
];
for (const [id, c] of holderCases) {
  rows.push({ kind: 'holder', id, character: c, out: namesForHolder(asHolder(c)) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
