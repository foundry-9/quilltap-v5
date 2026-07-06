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
