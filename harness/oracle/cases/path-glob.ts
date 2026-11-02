/**
 * Oracle case (P4.D354, v4 `40fab1c54`, bug 261): `doc_list_files`' glob.
 *
 * Drives the REAL `compilePathGlob` of v4's `lib/doc-edit/path-glob.ts` over
 * (1) every `expect` pair of v4's own `path-glob.test.ts` and (2) the edge
 * grid — every PATTERN against every PATH — emitting one
 * `{ pattern, path, matches }` row each. Pure; tier 1 (exact booleans).
 *
 * Run from inside the server checkout (or a pin worktree):
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/path-glob.ts \
 *     > /tmp/oracle-path-glob.ndjson
 */

import { compilePathGlob } from '@/lib/doc-edit/path-glob';

// v4 `lib/doc-edit/__tests__/path-glob.test.ts` — every `expect(matches(p, x))`.
const SUITE: Array<[string, string]> = [
  ['', 'a/b.md'],
  ['**/CHANGELOG.md', 'CHANGELOG.md'],
  ['**/CHANGELOG.md', 'docs/CHANGELOG.md'],
  ['**/CHANGELOG.md', 'a/b/c/CHANGELOG.md'],
  ['**/CHANGELOG.md', 'docs/OLD-CHANGELOG.md'],
  ['*.md', 'notes.md'],
  ['*.md', 'deep/folder/notes.md'],
  ['CHANGELOG.md', 'docs/CHANGELOG.md'],
  ['*.md', 'notes.mdx'],
  ['docs/*.md', 'docs/a.md'],
  ['docs/*.md', 'docs/sub/a.md'],
  ['src/**/*.js', 'src/a.js'],
  ['src/**/*.js', 'src/x/y/a.js'],
  ['src/**/*.js', 'lib/a.js'],
  ['docs/**', 'docs/a/b.md'],
  ['*.{ts,tsx}', 'a.tsx'],
  ['*.{ts,tsx}', 'a.js'],
  ['v?.md', 'v2.md'],
  ['v?.md', 'v10.md'],
  ['v[0-9].md', 'v3.md'],
  ['v[!0-9].md', 'v3.md'],
  ['changelog.MD', 'CHANGELOG.md'],
  ['./docs/*.md', 'docs/a.md'],
  ['/docs/*.md', 'docs/a.md'],
  ['notes (2).md', 'notes (2).md'],
  ['a+b.md', 'aab.md'],
  ['a.md', 'aXmd'],
  ['{a,b', 'a'],
];

// The edge grid (the order's Tier-2 item 20): 40 patterns × 30 paths.
const PATTERNS: string[] = [
  '**/CHANGELOG.md', 'CHANGELOG.md', 'drafts/*.md', '*.{md,txt}', '[!a]*.md', '{',
  'Notes\\sub\\*.md', '*.MD', 'a/**', '**', '**/*', '*/**/x.md', 'a**b', '?', '[a-]',
  '[]', '{,}', '(x).md', '$.md', '^.md', '|', '.', '  *.md  ', '././a.md', '//a.md',
  '[unclosed', 'x]', '[!]', '[^a].md', '[\\]].md', 'a/*', '*/', '**/', 'a{b,c{d,e}}f',
  '}', ',', '*.{md', '📁/*.md', 'ä*.md', 'v[0-9][0-9].md',
];
const PATHS: string[] = [
  'CHANGELOG.md', 'docs/CHANGELOG.md', 'docs/OLD-CHANGELOG.md', 'drafts/x.md', 'drafts/sub/x.md',
  'a.md', 'b.md', 'A.MD', 'notes.txt', 'a/b/c.md', 'a/x.md', 'a/b/x.md', 'ab', 'axxb', 'a/b',
  'x', 'xy', '-.md', '(x).md', '$.md', '^.md', '|', '.', 'Notes/sub/c.md', '/lead/a.md',
  'a\\b.md', 'abdf', 'acef', '📁/notes.md', 'Ägypten.md',
];

const rows: Array<{ group: string; pattern: string; path: string; matches: boolean }> = [];
for (const [pattern, path] of SUITE) {
  rows.push({ group: 'suite', pattern, path, matches: compilePathGlob(pattern)(path) });
}
for (const pattern of PATTERNS) {
  const m = compilePathGlob(pattern);
  for (const path of PATHS) {
    rows.push({ group: 'grid', pattern, path, matches: m(path) });
  }
}
process.stdout.write(rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
