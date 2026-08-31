/**
 * Oracle case (W4.1d batch 3a): the doc-edit pure leaves.
 *
 * Drives the REAL exported functions from lib/doc-edit/{diacritics, mime-registry,
 * unified-diff, markdown-parser}.ts over a committed corpus, emitting one NDJSON
 * row per (function, case) for a field-by-field tier-1 diff.
 *
 * Seams (normalized): JSON.parse's V8 error TEXT is engine-specific, so every
 * mime parse/serialize/validate failure message (and each JSONL per-line error)
 * is replaced with the sentinel "<ERR>" on BOTH sides — the structure (ok, the
 * parsed value, which lines failed, the raw line) is compared exactly, only the
 * message text is normalized. findHeadingSection's thrown messages ARE
 * reproducible (built from headings, not V8) and compared byte-for-byte.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   LOG_LEVEL=error npx tsx ~/source/quilltap-v5/harness/oracle/cases/doc-edit-leaves.ts \
 *     > /tmp/oracle-doc-edit-leaves.ndjson
 */

import {
  normalizeDiacritics,
  findAllMatches,
  findUniqueMatch,
} from '@/lib/doc-edit/diacritics';
import {
  TYPOGRAPHIC_FOLDINGS,
  foldTypography,
  hasTypographicVariants,
} from '@/lib/doc-edit/typographic-folding';
import {
  detectMimeFromExtension,
  isJsonMime,
  isJsonlMime,
  isJsonFamily,
  parseContent,
  serializeContent,
  validateJson,
  type DocMimeType,
} from '@/lib/doc-edit/mime-registry';
import { generateUnifiedDiff, formatAutosaveNotification } from '@/lib/doc-edit/unified-diff';
import { diffLines, changedBlockIndices } from '@/lib/doc-edit/line-diff';
import {
  slugifyHeading,
  parseHeadingTree,
  findHeadingSection,
  readHeadingContent,
  replaceHeadingContent,
  serializeFrontmatter,
  updateFrontmatterInContent,
} from '@/lib/doc-edit/markdown-parser';

const rows: unknown[] = [];
const ERR = '<ERR>';

/** Normalize a parse/serialize/validate result: replace failure message text. */
function normResult(r: any): any {
  if (r && r.ok === false) return { ok: false };
  if (r && r.ok === true && Array.isArray(r.value)) {
    // JSONL line results — normalize per-line error text, keep structure. Guard
    // object-ness (a plain JSON array value like [1,2,3] passes through).
    return {
      ok: true,
      value: r.value.map((line: any) =>
        line !== null && typeof line === 'object' && 'error' in line && line.error !== undefined
          ? { ...line, error: ERR }
          : line,
      ),
    };
  }
  return r;
}

// ---- diacritics ----
for (const [id, text] of [
  ['nfd-accent', 'Nimuë'],
  ['nfd-cafe', 'café'],
  ['nfd-plain', 'plain ascii'],
  ['nfd-hangul', '가나다'],
  ['nfd-multi', 'Zoë Śląsk Ñoño'],
  ['nfd-empty', ''],
] as Array<[string, string]>) {
  rows.push({ kind: 'diacritics-normalize', id, result: normalizeDiacritics(text) });
}
for (const [id, haystack, needle, norm, cs] of [
  ['m-base-vs-accent', 'say Nimuë now', 'Nimue', true, true],
  ['m-accent-vs-base', 'Nimue speaks', 'Nimuë', true, true],
  ['m-multiple', 'aXaXa', 'a', true, true],
  ['m-ci', 'HELLO hello', 'hello', true, false],
  ['m-no-normalize', 'café cafe', 'café', false, true],
  ['m-not-found', 'abc', 'xyz', true, true],
  ['m-empty-needle', 'abc', '', true, true],
  ['m-cafe-ci-norm', 'CAFÉ café', 'cafe', true, false],
] as Array<[string, string, string, boolean, boolean]>) {
  const options = { normalizeDiacritics: norm, caseSensitive: cs };
  rows.push({
    kind: 'diacritics-match',
    id,
    all: findAllMatches(haystack, needle, options),
    unique: findUniqueMatch(haystack, needle, options),
  });
}

// ---- typographic folding (bug 109, v4 487ae16b1) ----
// The fold table itself, as ordered [key, value] pairs so the port's
// transcription is compared entry-for-entry AND in order (the byte-exact
// static-data transcription rule; ~26 rows, small enough to transcribe).
rows.push({
  kind: 'typographic-fold-table',
  id: 'table',
  entries: Object.entries(TYPOGRAPHIC_FOLDINGS),
});

for (const [id, text] of [
  ['quotes', '‘a’ “b”'],
  ['dashes', 'a–b—c−d'],
  ['ellipsis', 'wait… now'],
  ['spaces', 'a\u00A0b\u202Fc\u2003d'],
  ['untouched', '\u00ABmot\u00BB\u200B\u00AD'],
  ['plain-ascii', "plain ascii's fine"],
  ['curly-apostrophe', 'curly’s not'],
  ['primes', '′ and ″'],
  ['empty', ''],
  ['astral', '😀’'],
] as Array<[string, string]>) {
  rows.push({
    kind: 'typographic-fold',
    id,
    text,
    folded: foldTypography(text),
    hasVariants: hasTypographicVariants(text),
  });
}

// findAllMatches / findUniqueMatch with the fold in play. Inputs are carried in
// the row so the corpus has ONE definition; the answers are still v4's.
for (const [id, haystack, needle, options] of [
  // The real failing passage from the instance bug 109 was found on.
  [
    'veyra-no-fold-default',
    'Cometary reservoir: a few short-period comets dipping inside Veyra-5’s orbit.\n',
    "dipping inside Veyra-5's orbit.",
    {},
  ],
  [
    'veyra-fold-rescues',
    'Cometary reservoir: a few short-period comets dipping inside Veyra-5’s orbit.\n',
    "dipping inside Veyra-5's orbit.",
    { foldTypography: true },
  ],
  ['em-dash-by-hyphen', 'the rail is a notary — not a warden', 'notary - not', { foldTypography: true }],
  ['em-dash-exact', 'the rail is a notary — not a warden', 'notary — not', { foldTypography: true }],
  // The one fold that is not one-to-one: the map back must survive it.
  ['ellipsis-mapback', 'she paused… then spoke', 'paused... then', { foldTypography: true }],
  ['ellipsis-mid-haystack', 'a… b… c', 'b... c', { foldTypography: true }],
  ['ellipsis-in-needle', 'she paused... then spoke', 'paused… then', { foldTypography: true }],
  ['nbsp', 'Chapter\u00A014 begins here', 'Chapter 14', { foldTypography: true }],
  ['double-quotes', 'she called it “thin” today', 'called it "thin"', { foldTypography: true }],
  // Exact first: a file with both spellings answers with the caller's.
  [
    'both-spellings-prefers-exact',
    "first Veyra-5's orbit, then Veyra-5’s orbit",
    "Veyra-5's orbit",
    { foldTypography: true },
  ],
  ['folded-ambiguous', 'hers’ and hers’ again', "hers' a", { foldTypography: true }],
  ['multiple-exact-never-folds', 'alpha beta alpha', 'alpha', { foldTypography: true }],
  ['composes-with-diacritics', 'Nimuë’s letter', "Nimue's letter", { foldTypography: true }],
  [
    'honours-normalize-false',
    'Nimuë’s letter',
    "Nimue's letter",
    { foldTypography: true, normalizeDiacritics: false },
  ],
  ['findall-off-by-default', 'a’b', "a'b", {}],
  ['grep-all-spellings', "one Veyra-5's, two Veyra-5’s, three Veyra-5’s", "Veyra-5's", { foldTypography: true }],
  ['fold-ci-independent', 'A’B', "a'b", { foldTypography: true, caseSensitive: false }],
  ['fold-without-diacritics', 'café’s', "cafe's", { foldTypography: true, normalizeDiacritics: false }],
  ['empty-needle-with-fold', 'a’b', '', { foldTypography: true }],
] as Array<[string, string, string, Record<string, boolean>]>) {
  rows.push({
    kind: 'typographic-match',
    id,
    haystack,
    needle,
    options,
    all: findAllMatches(haystack, needle, options),
    unique: findUniqueMatch(haystack, needle, options),
  });
}

// v4's replay shape (bug 109's "How to verify" / the commit's closing paragraph):
// five typographic failures now resolve to a UNIQUE match, and the genuinely
// stale find texts still miss. The instance log is private, so this reproduces
// the SHAPE over a synthetic document carrying one member of each fold family.
const REPLAY_FILE = [
  '# Hestia — survey notes',
  '',
  '## Cometary reservoir',
  '',
  'A few short-period comets dip inside Veyra-5’s orbit each decade.',
  'The reservoir is “thin but persistent” by every measure we have.',
  '',
  '## Open items',
  '',
  '- Family vote — not scheduled.',
  '- Sylvain’s first entry (his tempo).',
  '- She paused… then filed the addendum.',
  '- Chapter\u00A014 begins the second survey.',
  '',
].join('\n');

// Five valid edits, refused before the fold, all in the same direction the bug
// records: the file curly, the find text straight.
const REPLAY_TYPOGRAPHIC: Array<[string, string]> = [
  ['apostrophe', "dip inside Veyra-5's orbit each decade."],
  ['double-quote', 'is "thin but persistent" by every'],
  ['em-dash', '- Family vote - not scheduled.'],
  ['ellipsis', '- She paused... then filed the addendum.'],
  ['nbsp', '- Chapter 14 begins the second survey.'],
];

// Twenty-five genuinely stale find texts: the fold must not rescue any of them.
const REPLAY_STALE: string[] = [
  "dip outside Veyra-5's orbit each decade.",
  'A dozen short-period comets dip inside',
  '## Cometary reservoirs',
  'The reservoir is thick and fleeting',
  '- Family vote scheduled for Tuesday.',
  "- Sylvain's second entry (his tempo).",
  "- Sylvain's first entry (her tempo).",
  '- She paused, then filed the addendum.',
  '- Chapter 15 begins the second survey.',
  '- Chapter 14 begins the third survey.',
  '# Hestia - field notes',
  '## Closed items',
  'orbit each century.',
  'by every metric we have.',
  'begins the first survey.',
  'then filed the amendment.',
  'short-period asteroids',
  "Veyra-6's orbit",
  'not yet scheduled.',
  '(his cadence).',
  'a reservoir nobody surveyed',
  'the addendum was withdrawn',
  '- Marchpane volunteered.',
  'the tempo he prefers',
  'no comets at all',
];

// The document itself, emitted once so the corpus has ONE definition and the
// port reads it from the row rather than transcribing it.
rows.push({ kind: 'typographic-replay-file', id: 'file', file: REPLAY_FILE });

for (const [id, needle] of REPLAY_TYPOGRAPHIC) {
  rows.push({
    kind: 'typographic-replay',
    id: `resolves-${id}`,
    needle,
    unique: findUniqueMatch(REPLAY_FILE, needle, { foldTypography: true }),
  });
}
for (let i = 0; i < REPLAY_STALE.length; i++) {
  const needle = REPLAY_STALE[i];
  rows.push({
    kind: 'typographic-replay',
    id: `stale-${String(i + 1).padStart(2, '0')}`,
    needle,
    unique: findUniqueMatch(REPLAY_FILE, needle, { foldTypography: true }),
  });
}

// ---- mime-registry ----
for (const [id, p] of [
  ['ext-json', 'a.json'],
  ['ext-jsonl', 'a.JSONL'],
  ['ext-ndjson', 'x.ndjson'],
  ['ext-md', 'dir/b.md'],
  ['ext-markdown', 'c.markdown'],
  ['ext-txt', 'd.TXT'],
  ['ext-yaml', 'e.yaml'],
  ['ext-yml', 'f.yml'],
  ['ext-none', 'noext'],
  ['ext-dotfile', '.gitignore'],
  ['ext-unknown', 'g.xml'],
] as Array<[string, string]>) {
  rows.push({ kind: 'mime-detect', id, result: detectMimeFromExtension(p) ?? null });
}
for (const [id, m] of [
  ['is-appjson', 'application/json'],
  ['is-ndjson', 'application/x-ndjson'],
  ['is-md', 'text/markdown'],
] as Array<[string, string]>) {
  rows.push({
    kind: 'mime-predicate',
    id,
    isJson: isJsonMime(m),
    isJsonl: isJsonlMime(m),
    isFamily: isJsonFamily(m),
  });
}
for (const [id, content, mime] of [
  ['parse-json-ok', '{"a":1,"b":[2,3]}', 'application/json'],
  ['parse-json-empty-obj', '{}', 'application/json'],
  ['parse-json-array', '[1,2,3]', 'application/json'],
  ['parse-json-bad', '{bad', 'application/json'],
  ['parse-jsonl-ok', '{"a":1}\n{"b":2}\n', 'application/x-ndjson'],
  ['parse-jsonl-blank-lines', '{"a":1}\n\n  \n{"b":2}', 'application/x-ndjson'],
  ['parse-jsonl-mixed', '{"a":1}\nnotjson\n{"c":3}', 'application/x-ndjson'],
  ['parse-md-unsupported', 'text', 'text/markdown'],
] as Array<[string, string, DocMimeType]>) {
  rows.push({ kind: 'mime-parse', id, result: normResult(parseContent(content, mime)) });
}
for (const [id, value, mime, pretty] of [
  ['ser-json-obj', { a: 1, b: 'two' }, 'application/json', true],
  ['ser-json-compact', { a: 1 }, 'application/json', false],
  ['ser-json-nested', { a: { b: [1, 2] }, c: 'x' }, 'application/json', true],
  ['ser-jsonl-arr', [{ a: 1 }, { b: 2 }], 'application/x-ndjson', true],
  ['ser-jsonl-empty', [], 'application/x-ndjson', true],
  ['ser-jsonl-notarray', { a: 1 }, 'application/x-ndjson', true],
] as Array<[string, unknown, DocMimeType, boolean]>) {
  rows.push({
    kind: 'mime-serialize',
    id,
    result: normResult(serializeContent(value, mime, { pretty })),
  });
}
for (const [id, content, mime] of [
  ['val-json-ok', '{"a":1}', 'application/json'],
  ['val-json-bad', '{bad', 'application/json'],
  ['val-jsonl-ok', '{"a":1}\n{"b":2}', 'application/x-ndjson'],
  ['val-jsonl-bad', '{"a":1}\nnope', 'application/x-ndjson'],
] as Array<[string, string, DocMimeType]>) {
  rows.push({ kind: 'mime-validate', id, result: normResult(validateJson(content, mime)) });
}

// ---- unified-diff ----
// The literal + programmatically-generated case inputs. The generated ones
// (`gen*` helpers below) MUST be reproduced byte-identically on the Rust side.
const numbered = (count: number): string[] => Array.from({ length: count }, (_, i) => `line${i + 1}`);
const withChange = (base: string[], edits: Array<[number, string]>): string[] => {
  const out = [...base];
  for (const [i, v] of edits) out[i] = v;
  return out;
};

const diffCases: Array<[string, string, string]> = [
  // Legacy corpus (re-generated under the new Myers/hunk algorithm).
  ['diff-identical', 'a\nb\nc\n', 'a\nb\nc\n'],
  ['diff-one-line', 'a\nb\nc\n', 'a\nB\nc\n'],
  ['diff-add', 'a\nc\n', 'a\nb\nc\n'],
  ['diff-remove', 'a\nb\nc\n', 'a\nc\n'],
  ['diff-append', 'a\n', 'a\nb\nc\n'],
  ['diff-multi-hunk', 'a\nb\nc\nd\ne\n', 'a\nB\nc\nd\nE\n'],
  ['diff-full-replace', 'x\ny\n', 'p\nq\n'],
  ['diff-empty-old', '', 'new\n'],
  // New behavior (v4 8617ce7a): Myers + git-style hunks.
  ['diff-both-empty', '', ''],
  ['diff-shifted-insert', 'a\nb\nc', 'a\nNEW\nb\nc'],
  ['diff-single-line-file', 'only', 'changed'],
  ['diff-create-from-empty', '', 'hello\nworld'],
  ['diff-empty-from-content', 'a\nb', ''],
  ['diff-context-start', 'A\nb\nc\nd\ne', 'X\nb\nc\nd\ne'],
  ['diff-context-end', 'a\nb\nc\nd\nE', 'a\nb\nc\nd\nZ'],
  ['diff-unicode', 'café\nNimuë\n世界', 'café\nNimué\n世界'],
  // Context cap: 12 lines, change on line 7 → `@@ -4,7 +4,7 @@` (3 ctx each side).
  ['diff-context-cap', numbered(12).join('\n'), withChange(numbered(12), [[6, 'CHANGED']]).join('\n')],
  // Distant changes split into two hunks (20 lines, edits far apart).
  ['diff-distant', numbered(20).join('\n'), withChange(numbered(20), [[1, 'TOP'], [18, 'BOTTOM']]).join('\n')],
  // Nearby changes coalesce into one hunk (10 lines, one unchanged line between).
  ['diff-coalesce', numbered(10).join('\n'), withChange(numbered(10), [[3, 'A'], [5, 'B']]).join('\n')],
  // Oversized input (> MAX_DIFFABLE_LINES = 10000 combined): whole-file fallback.
  ['diff-huge-fallback', Array(5001).fill('x').join('\n'), Array(5000).fill('y').join('\n')],
];
for (const [id, oldT, newT] of diffCases) {
  rows.push({
    kind: 'diff',
    id,
    diff: generateUnifiedDiff(oldT, newT, 'f.md'),
    notify: formatAutosaveNotification(oldT, newT, 'f.md') ?? null,
  });
}

// ---- line-diff: diffLines (drive the exported primitive directly) ----
for (const [id, oldL, newL] of [
  ['dl-identical', ['a', 'b'], ['a', 'b']],
  ['dl-insert', ['a', 'b', 'c'], ['a', 'NEW', 'b', 'c']],
  ['dl-modify', ['a', 'b', 'c'], ['a', 'B', 'c']],
  ['dl-replace-all', ['a', 'b'], ['x', 'y']],
  ['dl-empty-old', [], ['a', 'b']],
  ['dl-empty-new', ['a', 'b'], []],
  ['dl-both-empty', [], []],
] as Array<[string, string[], string[]]>) {
  rows.push({ kind: 'diff-lines', id, result: diffLines(oldL, newL) });
}

// ---- line-diff: changedBlockIndices (emit as a sorted array) ----
for (const [id, base, cur] of [
  ['cb-unchanged', ['a', 'b', 'c'], ['a', 'b', 'c']],
  ['cb-insert-top', ['intro', 'section one', 'body one'], ['intro', 'NEW SECTION', 'section one', 'body one']],
  ['cb-modify', ['a', 'b', 'c'], ['a', 'B', 'c']],
  ['cb-pure-delete', ['a', 'b', 'c'], ['a', 'c']],
  ['cb-replace-all', ['a', 'b'], ['x', 'y']],
  ['cb-empty-doc', [], ['a', 'b']],
] as Array<[string, string[], string[]]>) {
  rows.push({
    kind: 'changed-blocks',
    id,
    result: [...changedBlockIndices(base, cur)].sort((a, b) => a - b),
  });
}

// ---- markdown headings ----
for (const [id, text] of [
  ['slug-basic', 'Character Backstory'],
  ['slug-punct', 'Hello, World! (v2)'],
  ['slug-dashes', 'A -- B'],
  ['slug-trim', '  Spaced Out  '],
  ['slug-underscore', 'snake_case name'],
  ['slug-only-punct', '!!!'],
  ['slug-numbers', 'Chapter 3.1'],
] as Array<[string, string]>) {
  rows.push({ kind: 'slug', id, result: slugifyHeading(text) });
}

const doc =
  '# Title\nintro text\n## Section A\naaa\n### Sub A1\nsub body\n## Section A\nbbb\n```\n# not a heading\n```\n# Title\ntail\n';
rows.push({ kind: 'heading-tree', id: 'doc', result: parseHeadingTree(doc) });
for (const [id, text, level] of [
  ['find-title', 'title', undefined],
  ['find-section-dup', 'section a', undefined],
  ['find-sub', 'Sub A1', 3],
  ['find-missing', 'nope', undefined],
  ['find-wrong-level', 'Sub A1', 2],
] as Array<[string, string, number | undefined]>) {
  let r: any;
  try {
    const h = findHeadingSection(doc, text, level);
    r = { ok: true, heading: h, read: readHeadingContent(doc, h) };
  } catch (e: any) {
    r = { ok: false, message: e.message };
  }
  rows.push({ kind: 'find-heading', id, result: r });
}
// replaceHeadingContent
{
  const simple = '# A\nold body\n## B\nkeep\n';
  const h = findHeadingSection(simple, 'A');
  rows.push({
    kind: 'replace-heading',
    id: 'preserve',
    result: replaceHeadingContent(simple, h, 'NEW\n', true),
  });
  rows.push({
    kind: 'replace-heading',
    id: 'no-preserve',
    result: replaceHeadingContent(simple, h, 'NEW\n', false),
  });
}

// ---- frontmatter serialize / update ----
for (const [id, data] of [
  ['fm-strings', { title: 'My Doc', author: 'Ada' }],
  ['fm-mixed', { title: 'Doc', count: 3, active: true, embed: false, note: null }],
  ['fm-colon', { summary: 'has: a colon' }],
  ['fm-array', { tags: ['a', 'b', 'c'] }],
  ['fm-num-array', { nums: [1, 2, 3] }],
  ['fm-empty', {}],
  ['fm-policy', { character_read: 'no', embed: false }],
  ['fm-quoted-word', { flag: 'true' }],
] as Array<[string, Record<string, unknown>]>) {
  rows.push({ kind: 'fm-serialize', id, result: serializeFrontmatter(data) });
}
for (const [id, content, updates, replaceAll] of [
  ['upd-merge', '---\ntitle: Old\nkeep: yes\n---\nbody here\n', { title: 'New' }, false],
  ['upd-delete', '---\ntitle: Doc\ndrop: gone\n---\nbody\n', { drop: null }, false],
  ['upd-add-new', 'no frontmatter body\n', { title: 'Added' }, false],
  ['upd-replace', '---\na: 1\nb: 2\n---\nbody\n', { only: 'this' }, true],
] as Array<[string, string, Record<string, unknown>, boolean]>) {
  rows.push({
    kind: 'fm-update',
    id,
    result: updateFrontmatterInContent(content, updates, replaceAll),
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
