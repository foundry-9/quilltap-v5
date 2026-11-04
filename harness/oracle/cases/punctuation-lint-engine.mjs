/**
 * Client oracle — the composer's punctuation-lint engine and prose-mode
 * predicate, EXECUTED out of v4's own `lib/punctuation-lint/engine.ts` and
 * `lib/punctuation-lint/prose-mode.ts` rather than transcribed from them
 * (P4.D352; the round's "tsx leaf recorder" for `engine.v4.json`).
 *
 * v5's twins are byte copies at `apps/web/src/app/punctuation-lint/`. Both v4
 * files import NOTHING (v4 holds that with an ESLint fence), so this recorder
 * reads them through `git show` at the pin into one temp dir and `import()`s
 * them: Node 24's type stripping erases the types — no bundler, no aliases, no
 * jest, no `node_modules`. The SPA spec `engine.oracle.spec.ts` diffs v5
 * against these bytes field by field.
 *
 * The corpus:
 *   (a) every row of BOTH committed vector files, read through `git show` at
 *       the pin too — and the recorder ASSERTS each executed answer equals the
 *       row's `expect` (exit 1 otherwise): the executed engine and the
 *       contract corpus agree at the pin;
 *   (b) a generated corpus — short drafts, each linted with no caret and with
 *       the caret at EVERY offset of every block, in and out of prose mode;
 *   (c) `classifyProse` over every generated draft;
 *   (d) `isProseTemplate` over Novel's delimiter set and its near misses.
 *
 * REGEN RECIPE (run from anywhere; writes the committed oracle in place):
 *
 *   export PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH
 *   QT_V4_PIN=236c04e35 node ~/source/quilltap-v5/harness/oracle/cases/punctuation-lint-engine.mjs \
 *     > ~/source/quilltap-v5/apps/web/src/app/punctuation-lint/engine.v4.json
 *
 * Override the checkout or the pin with QT_V4_CHECKOUT / QT_V4_PIN. Reading
 * through `git show` at the pin makes the recipe independent of the v4 working
 * tree (drift-ledger §5.1's PIN REQUIRED rule is satisfied by construction).
 * The output is deterministic (no wall-clock stamp) so a re-run byte-compares.
 */

import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { isDeepStrictEqual } from 'node:util';

const CHECKOUT = process.env.QT_V4_CHECKOUT ?? `${process.env.HOME}/source/quilltap-server`;
const PIN = process.env.QT_V4_PIN ?? '236c04e35';

const show = (path) =>
  execFileSync('git', ['-C', CHECKOUT, 'show', `${PIN}:${path}`], { encoding: 'utf8' });
const md5 = (s) => createHash('md5').update(s).digest('hex');

const engineSrc = show('lib/punctuation-lint/engine.ts');
const proseSrc = show('lib/punctuation-lint/prose-mode.ts');
const vectors = JSON.parse(show('lib/punctuation-lint/fixtures/punctuation-vectors.json'));
const proseVectors = JSON.parse(show('lib/punctuation-lint/fixtures/prose-vectors.json'));

const dir = mkdtempSync(join(tmpdir(), 'qt-punctuation-lint-'));
process.on('exit', () => rmSync(dir, { recursive: true, force: true }));
writeFileSync(join(dir, 'engine.ts'), engineSrc);
writeFileSync(join(dir, 'prose-mode.ts'), proseSrc);

const engine = await import(pathToFileURL(join(dir, 'engine.ts')).href);
const prose = await import(pathToFileURL(join(dir, 'prose-mode.ts')).href);

const rows = [];
let mismatches = 0;

// (a) the committed vectors, executed — and checked against their `expect`.
for (const c of vectors.cases) {
  const input = { blocks: c.blocks, caret: c.caret ?? null, proseMode: false };
  const out = engine.lintPunctuation(input.blocks, { caret: input.caret });
  if (!isDeepStrictEqual(out, c.expect)) {
    mismatches++;
    process.stderr.write(`vector mismatch: punctuation-vectors / ${c.name}\n`);
  }
  rows.push({ id: `vector:${c.name}`, kind: 'lint', input, out });
}
for (const c of proseVectors.lint) {
  const input = { blocks: c.blocks, caret: c.caret ?? null, proseMode: true };
  const out = engine.lintPunctuation(input.blocks, { caret: input.caret, proseMode: true });
  if (!isDeepStrictEqual(out, c.expect)) {
    mismatches++;
    process.stderr.write(`vector mismatch: prose-vectors lint / ${c.name}\n`);
  }
  rows.push({ id: `prose-vector:${c.name}`, kind: 'lint', input, out });
}
for (const c of proseVectors.classify) {
  const input = { blocks: c.blocks };
  const out = engine.classifyProse(input.blocks);
  if (!isDeepStrictEqual(out, c.expect)) {
    mismatches++;
    process.stderr.write(`vector mismatch: prose-vectors classify / ${c.name}\n`);
  }
  rows.push({ id: `prose-vector:${c.name}`, kind: 'classify', input, out });
}

// (b) + (c) the generated drafts (survey §5.2). Each is a list of blocks; a
// `null` is a code block (a barrier).
const DRAFTS = [
  ['nested-double-quote', ['She said "he told me "run" and left."']],
  ['stray-curly-close', ['Then” he left.']],
  ['stray-straight-close', ['Then" he left.']],
  ['floating-quote', ['He said " hello.']],
  ['glued-quote', ['word"word']],
  ['backwards-curly', ['”Hello,“ she said.']],
  ['unclosed-continued', ['"First part of it\n\n"Second part."']],
  ['unclosed-not-continued', ['"First part of it\n\nThen nothing.']],
  ['unclosed-then-empty-paragraph', ['"Still talking\n\n']],
  ['asterisk-single-open', ['*one']],
  ['asterisk-double-open', ['**two']],
  ['asterisk-arithmetic', ['2 * 3']],
  ['scene-break', ['* * *']],
  ['emphasis-closed', ['a *word* here']],
  ['ooc-open-alone', ['((']],
  ['ooc-close-alone', ['))']],
  ['ooc-in-dialogue', ['"Say ((x)) now."']],
  ['ooc-unclosed', ['Text ((aside']],
  ['null-barrier', ['"open', null, '"next."']],
  ['two-blocks-paragraphs', ['"Hi," she said.\n\nAnd then.', 'More *text* here.']],
  ['inch-mark', ['He is 6" tall.']],
  ['apostrophe-nineties', ["Back in the '90s \"we\" did."]],
  ['curly-straight-mix', ['“Hello," she said "bye”.']],
  ['opening-context', ['("Yes," she said.)']],
  ['no-dialogue-narration', ['She set the cup down.']],
  ['no-dialogue-with-ooc', ['She waited. ((brb)) Then left.']],
  ['dialogue-and-narration', ['She waited. "Well?" Nothing.']],
  ['soft-break', ['Line one\n"line two']],
  ['empty-block', ['']],
  ['whitespace-paragraphs', ['  \n \n  "x"']],
];

for (const [name, blocks] of DRAFTS) {
  const carets = [null];
  blocks.forEach((text, block) => {
    if (text === null) return;
    for (let offset = 0; offset <= text.length; offset++) carets.push({ block, offset });
  });
  for (const proseMode of [false, true]) {
    for (const caret of carets) {
      const input = { blocks, caret, proseMode };
      const out = engine.lintPunctuation(blocks, { caret, proseMode });
      const at = caret === null ? 'none' : `${caret.block}:${caret.offset}`;
      rows.push({ id: `draft:${name}:caret=${at}:prose=${proseMode}`, kind: 'lint', input, out });
    }
  }
  rows.push({ id: `draft:${name}`, kind: 'classify', input: { blocks }, out: engine.classifyProse(blocks) });
}

// (d) isProseTemplate. Novel's delimiter set restated from v4
// `lib/chat/novel-template.ts:26-46` at the pin (that file imports app code, so
// it cannot be loaded here without the alias machinery).
const NOVEL_DELIMITERS = [
  {
    kind: 'wrap',
    name: 'Dialogue',
    buttonName: 'Dia',
    delimiters: '"',
    style: 'qt-chat-dialogue',
    matchCurlyQuotes: true,
    closeOptional: true,
  },
  {
    kind: 'wrap',
    name: 'Emphasis',
    buttonName: 'Emph',
    delimiters: '*',
    style: 'qt-roleplay-plain',
    hideDelimiter: true,
    addOns: { bold: true, italic: true, reverse: false, underline: 'none', border: 'none', font: '' },
  },
  { kind: 'wrap', name: 'Out of Character', buttonName: 'OOC', delimiters: ['((', '))'], style: 'qt-chat-ooc' },
];
// `undefined` cannot survive JSON; the sentinel stands for it.
const TEMPLATES = [
  ['novel', { delimiters: NOVEL_DELIMITERS, narrationDelimiters: null }],
  ['novel-star-narration', { delimiters: NOVEL_DELIMITERS, narrationDelimiters: '*' }],
  ['novel-pair-narration', { delimiters: NOVEL_DELIMITERS, narrationDelimiters: ['[', ']'] }],
  ['null-narration-single-quote-wrap', { narrationDelimiters: null, delimiters: [{ kind: 'wrap', delimiters: "'" }] }],
  ['null-narration-kindless-quote', { narrationDelimiters: null, delimiters: [{ delimiters: '"' }] }],
  ['null-narration-lineprefix-quote', { narrationDelimiters: null, delimiters: [{ kind: 'linePrefix', delimiters: '"' }] }],
  ['null-narration-ooc-only', { narrationDelimiters: null, delimiters: [{ kind: 'wrap', delimiters: ['((', '))'] }] }],
  ['null-narration-no-delimiters', { narrationDelimiters: null }],
  ['null-narration-null-delimiters', { narrationDelimiters: null, delimiters: null }],
  ['absent-narration', { delimiters: NOVEL_DELIMITERS }],
  ['null', null],
  ['undefined', '__undefined__'],
  ['empty-object', {}],
];
for (const [name, template] of TEMPLATES) {
  const arg = template === '__undefined__' ? undefined : template;
  rows.push({ id: `template:${name}`, kind: 'prose', input: { template }, out: prose.isProseTemplate(arg) });
}

if (mismatches > 0) {
  process.stderr.write(`${mismatches} vector row(s) disagree with v4's executed engine at ${PIN}\n`);
  process.exit(1);
}

const out = [
  '{',
  `  "_source": ${JSON.stringify({
    checkout: 'quilltap-server',
    pin: PIN,
    files: ['lib/punctuation-lint/engine.ts', 'lib/punctuation-lint/prose-mode.ts'],
    engineMd5: md5(engineSrc),
    proseModeMd5: md5(proseSrc),
  })},`,
  '  "rows": [',
  rows.map((r) => `    ${JSON.stringify(r)}`).join(',\n'),
  '  ]',
  '}',
];
process.stdout.write(out.join('\n') + '\n');
