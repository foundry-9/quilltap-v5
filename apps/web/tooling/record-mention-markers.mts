/**
 * Record the name-chip markers tier-1 corpus from v4's REAL modules
 * (`lib/chat/mention-markers.ts` + `lib/chat/name-mentions.ts`, v4
 * `9a88f862f`) — P4.D359 item 8. `mention-markers.corpus.spec.ts` replays
 * every row through the SPA's TS twins and byte-compares (`JSON.stringify`).
 *
 * Regenerate — run tsx FROM the pinned v4 worktree (its cwd resolves the `@/`
 * tsconfig aliases the modules import), with `QT_V4_ROOT` naming the same tree:
 *   cd <pin> && QT_V4_ROOT=$PWD PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
 *     npx tsx <v5>/apps/web/tooling/record-mention-markers.mts \
 *     > <v5>/apps/web/src/testing/fixtures/mention-markers.oracle.ndjson
 *
 * Row shape: `{kind, input, expected}`, `expected` = `JSON.stringify(output)`.
 *  - `matcher`: `{names, text}` → `{usable, first, last}`.
 *  - `rehype`: `{tree, mentions}` → the tree after v4's plugin ran on it.
 *  - `split`: `{text}` → the runs.
 *  - `assign`: `{runs, mentions}` → per-run mentions (`undefined` → `null` in JSON).
 */

import { homedir } from 'node:os';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const V4_ROOT = process.env['QT_V4_ROOT'] ?? resolve(homedir(), 'source/quilltap-server');
const load = async <T,>(rel: string): Promise<T> =>
  (await import(pathToFileURL(resolve(V4_ROOT, rel)).href)) as T;

type Node = { type: string; tagName?: string; value?: string; children?: Node[] };
const markers = await load<{
  MENTION_OPEN: string;
  MENTION_MID: string;
  MENTION_CLOSE: string;
  rehypeMentionMarkers(m: ReadonlyArray<{ text: string }>): (t: Node) => void;
  splitMentionMarkers(t: string): unknown;
  assignMentionsToRuns(r: ReadonlyArray<string | null>, m: ReadonlyArray<{ text: string }>): unknown;
}>('lib/chat/mention-markers.ts');
const names = await load<{
  buildNameMatcher(n: ReadonlyArray<string | null | undefined>): RegExp | null;
  firstNameMatch(t: string, m: RegExp): unknown;
  lastNameMatch(t: string, m: RegExp): unknown;
}>('lib/chat/name-mentions.ts');

const { MENTION_OPEN: O, MENTION_MID: M, MENTION_CLOSE: C } = markers;
const mark = (i: number, t: string) => `${O}${i}${M}${t}${C}`;
const el = (tagName: string, ...children: Node[]): Node => ({ type: 'element', tagName, children });
const text = (value: string): Node => ({ type: 'text', value });

const rows: string[] = [];
const emit = (kind: string, input: unknown, out: unknown): void => {
  rows.push(JSON.stringify({ kind, input, expected: JSON.stringify(out) }));
};

// matcher
const matcherRow = (n: Array<string | null>, t: string): void => {
  const m = names.buildNameMatcher(n);
  emit('matcher', { names: n, text: t }, m ? { usable: true, first: names.firstNameMatch(t, m), last: names.lastNameMatch(t, m) } : { usable: false, first: null, last: null });
};
const ABBY = ['Abigail', 'Abby', 'Mrs. Sebold'];
for (const t of ['ask Abby now', 'abby', 'Abbyville', 'Ask Mrs. Sebold.', 'Abby, then Abigail', 'Mrs. Sebold and Sebold', 'no one here']) matcherRow(ABBY, t);
for (const n of [[''], ['  '], [null], ['', '  ', null], []]) matcherRow(n as Array<string | null>, 'Abby');
for (const t of ['Abbyé', 'éAbby', 'Абби Abby', 'Abby2', '2Abby', '_Abby', 'Abby_', 'Abby-ish', '(Abby)', 'Abby’s', 'ABBY Abby']) matcherRow(['Abby'], t);
matcherRow(['Sebold', 'Mrs. Sebold'], 'Mrs. Sebold left; Sebold stayed.');
matcherRow(['Doc', 'Dr. Watson'], 'the doc said Doc, Dr. Watson');
matcherRow(['a.b', 'a*b'], 'axb a.b a*b');
matcherRow(['Абби'], 'Привет, Абби!');

// rehype
const rehypeRow = (tree: Node, mentions: Array<{ text: string }>): void => {
  const input = JSON.parse(JSON.stringify({ tree, mentions }));
  markers.rehypeMentionMarkers(mentions)(tree);
  emit('rehype', input, tree);
};
rehypeRow(el('root', el('p', text('Vergil and Abby; Abby again.')), el('p', text('Vergil later.'))), [{ text: 'Abby' }, { text: 'Vergil' }]);
rehypeRow(el('root', el('p', el('code', text('Abby')), el('em', text('Abby')), text(' then Abby'))), [{ text: 'Abby' }]);
rehypeRow(el('root', el('ul', el('li', text('Greg came')))), [{ text: 'Greg' }]);
rehypeRow(el('root', el('blockquote', el('p', text('Greg wrote')))), [{ text: 'Greg' }]);
rehypeRow(el('root', el('blockquote', text('Greg said'))), [{ text: 'Greg' }]);
rehypeRow(el('root', el('p', el('em', text('Abby'))), el('p', text('Abby'))), [{ text: 'Abby' }]);
rehypeRow(el('root', el('pre', el('code', text('Abby'))), el('p', text('then Abby'))), [{ text: 'Abby' }]);
rehypeRow(el('root', el('ul', el('li', el('p', text('Abby in a loose list'))))), [{ text: 'Abby' }]);
rehypeRow(el('root', el('h2', text('Abby')), el('p', text('Abby'))), [{ text: 'Abby' }]);
rehypeRow(el('root', el('div', text('Abby')), el('p', text('Abby'))), [{ text: 'Abby' }]);
rehypeRow(el('root', el('p', text('Mrs. Sebold met Sebold'))), [{ text: 'Mrs. Sebold' }, { text: 'Sebold' }]);
rehypeRow(el('root', el('p', text('Sebold met Mrs. Sebold'))), [{ text: 'Sebold' }, { text: 'Mrs. Sebold' }]);
rehypeRow(el('root', el('p', text('Abby and Greg'))), [{ text: 'Abby' }, { text: 'Greg' }]);
rehypeRow(el('root', el('p', text('Abby')), el('p', text('Abby'))), [{ text: 'Abby' }]);
rehypeRow(el('root', el('p', text('nobody'))), [{ text: 'Abby' }]);
rehypeRow(el('root', el('p', text('Abby'))), [{ text: '  ' }, { text: 'Abby' }]);
rehypeRow(el('root', el('p', text('Abby'))), []);
rehypeRow(el('root', el('p', el('a', text('Abby')), text(' and Abby'))), [{ text: 'Abby' }]);
rehypeRow(el('root', el('p', el('strong', text('Greg')), text(', then Greg'))), [{ text: 'Greg' }]);

// split
for (const t of ['no marks', `"When ${mark(2, 'Abby')} scolds."`, `${mark(10, 'Greg')}`, `${mark(0, 'A')}${mark(1, 'B')}`, `x ${mark(3, 'Abby')} y ${mark(4, 'Greg')} z`, `${O}x${M}bad${C}`, `${O}1${M}unclosed`]) emit('split', { text: t }, markers.splitMentionMarkers(t));

// assign
const assignRow = (runs: Array<string | null>, mentions: Array<{ text: string }>): void =>
  emit('assign', { runs, mentions }, markers.assignMentionsToRuns(runs, mentions));
assignRow(['Hello.', null, 'Abby came. Greg too.', 'Abby again.'], [{ text: 'Abby' }, { text: 'Greg' }]);
assignRow([null, null], [{ text: 'Abby' }]);
assignRow(['nobody here'], [{ text: 'Abby' }]);
assignRow(['Greg first', 'then Abby'], [{ text: 'Abby' }, { text: 'Greg' }]);
assignRow(['Abby'], [{ text: '' }, { text: 'Abby' }]);
assignRow([], [{ text: 'Abby' }]);

process.stdout.write(rows.join('\n') + '\n');
