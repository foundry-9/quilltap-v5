/**
 * The v4-side recorder behind `src/app/editor/formatting-toolbar.oracle.spec.ts`.
 *
 * v4 `df8d4d70a` gave every Markdown button in `components/chat/
 * FormattingToolbar.tsx` an accessible name through `markdownButtonName` — the
 * tooltip's plain words, keeping the visible glyph when the words don't already
 * contain it ("Heading 4 (H4)"), so a voice-control user can say what they see
 * (WCAG 2.5.3). v4 does NOT export it, so this recorder reads the PIN's own
 * bytes of `FormattingToolbar.tsx`, slices EXACTLY the `function
 * markdownButtonName(…) {…}` declaration, transpiles it with the pin's
 * `typescript` and evaluates it — v4's real code, not a re-typing (ruling
 * PL-11; the `harness/tools/extract-prompt-text.cjs` precedent of reading v4
 * source text). v5 exports its twin so the corpus can reach it.
 *
 * Row kinds (the `kind` key): `formats` (v4's `MARKDOWN_FORMATS`, imported from
 * `@/lib/chat/annotations` — label / tooltip / type, in order — so the spec can
 * pin v5's button table to it), and `name` (one `markdownButtonName` call).
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/lib/...` and
 * would not compile in the SPA's own tsconfig.
 *
 * Run it from a lane-unique pinned v4 worktree at `624d0d25c` (Node 24.13.1):
 *
 * ```bash
 * PIN=/tmp/qt-v4-pin-<order>-624d0d25c
 * git -C ~/source/quilltap-server worktree add --detach "$PIN" 624d0d25c
 * (cd "$PIN" && npm ci --offline --ignore-scripts --no-audit --no-fund && npm rebuild better-sqlite3)
 * export PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH
 * cp <V5>/apps/web/oracle/markdown-button-name.recorder.ts "$PIN/"
 * rm -f <V5>/apps/web/src/testing/fixtures/markdown-button-name.oracle.ndjson
 * cd "$PIN" && npx tsx markdown-button-name.recorder.ts \
 *   > <V5>/apps/web/src/testing/fixtures/markdown-button-name.oracle.ndjson
 * wc -l <V5>/apps/web/src/testing/fixtures/markdown-button-name.oracle.ndjson
 * ```
 *
 * It reads `components/chat/FormattingToolbar.tsx` from the working directory
 * (the pin), so it must run there. Expect the line count recorded in
 * `src/testing/fixtures/README.md`'s `markdown-button-name.oracle.ndjson`
 * section; a second run must reproduce the file byte for byte (`cmp`).
 */

import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import ts from 'typescript';

import { MARKDOWN_FORMATS } from '@/lib/chat/annotations';

const SOURCE = readFileSync(join(process.cwd(), 'components/chat/FormattingToolbar.tsx'), 'utf8');

/** Slice exactly one top-level `function markdownButtonName(…) {…}` declaration. */
function sliceDeclaration(source: string): string {
  const head = 'function markdownButtonName(';
  const first = source.indexOf(head);
  if (first < 0 || source.indexOf(head, first + 1) >= 0) {
    throw new Error('expected exactly one markdownButtonName declaration');
  }
  // The declaration starts at the line's beginning and ends at the first
  // column-0 closing brace after it.
  const start = source.lastIndexOf('\n', first) + 1;
  const end = source.indexOf('\n}', first);
  if (end < 0) throw new Error('markdownButtonName has no closing brace at column 0');
  const decl = source.slice(start, end + 2);
  if (!decl.startsWith(head) || !decl.endsWith('}')) throw new Error(`bad slice: ${decl}`);
  return decl;
}

const declaration = sliceDeclaration(SOURCE);
const js = ts.transpileModule(`${declaration}\nexport { markdownButtonName };\n`, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
}).outputText;
const mod: { exports: Record<string, unknown> } = { exports: {} };
// eslint-disable-next-line no-new-func
new Function('module', 'exports', js)(mod, mod.exports);
const markdownButtonName = mod.exports['markdownButtonName'] as (f: { label: string; tooltip: string }) => string;
if (typeof markdownButtonName !== 'function') throw new Error('slice did not evaluate to a function');

const rows: unknown[] = [];
const seen = new Set<string>();
function emit(row: { id: string; kind: string } & Record<string, unknown>): void {
  if (seen.has(row.id)) throw new Error(`duplicate vector label: ${row.id}`);
  seen.add(row.id);
  rows.push(row);
}

emit({
  id: 'formats',
  kind: 'formats',
  // The sliced bytes ride along so a v4 rewrite of the function shows in the diff.
  declaration,
  formats: MARKDOWN_FORMATS.map((f) => ({ label: f.label, tooltip: f.tooltip, type: f.type })),
});

const name = (id: string, label: string, tooltip: string) =>
  emit({ id, kind: 'name', label, tooltip, out: markdownButtonName({ label, tooltip }) });

// v4's eleven buttons, in order.
for (const f of MARKDOWN_FORMATS) name(`format ${f.type}`, f.label, f.tooltip);

// The rule's edges.
name('label already in tooltip', 'Bold', 'Bold');
name('label as a word of the tooltip', 'Bold', 'Make Bold');
name('case-insensitive containment', 'b', 'Bold');
name('upper tooltip, lower label', 'h4', 'HEADING H4');
name('alphanumeric label not contained', 'H7', 'Heading 7');
name('digit-only label contained', '4', 'Heading 4');
name('digit-only label not contained', '9', 'Heading 4');
name('bullet label', '•', 'Unordered List');
name('ordered label with space', '1. …', 'Ordered List');
name('curly quote label', '“', 'Blockquote');
name('empty label', '', 'Bold');
name('empty tooltip', 'B', '');
name('both empty', '', '');
name('astral label', '𝔄', 'Fraktur A');
name('accented label', 'É', 'Emphasis');
name('label with underscore', 'H_1', 'Heading');
name('label with space', 'H 1', 'Heading');
name('long alnum label', 'Strikethrough2', 'Strike');
name('dotted-i tooltip', 'I', 'İtalic');

for (const row of rows) process.stdout.write(`${JSON.stringify(row)}\n`);
