#!/usr/bin/env node
/**
 * The SPA-wide `title=` census (work order P4.75, item 5).
 *
 * v4 renders native browser tooltips as a plain `title` attribute on DOM
 * elements. This script answers, mechanically and re-runnably, which of those
 * v4 sites v5 reproduces — so a drift round can re-run it rather than eyeballing
 * two trees.
 *
 * ## What counts as a site
 *
 * ONLY `title` on a **DOM element** (a lowercase JSX/Angular tag). A `title` on
 * a React component (`<Modal title=…>`, `<Avatar title=…>`) is a prop, not a
 * tooltip, and is counted separately as `component-prop` — v5's analogous input
 * carries it, and neither renders a native tooltip.
 *
 * ## Classification (per v4 DOM site with a STRING value)
 *
 *   ok       — the same string appears in v5 as a `title`/`[title]`/
 *              `[attr.title]` attribute, or as a `qt-tooltip` `content`
 *              (v4 itself converted nine action-bar buttons to its Tooltip at
 *              P4.D131 — v5 followed, so a Tooltip is a hit, not a gap).
 *   missing   — the string appears nowhere in v5. Either v5 renders the element
 *              without the title (class ii — FILL IT), or v5 has no such
 *              surface at all (class iii — an unported surface's order owns it).
 *              The two are separated by hand; the script groups `missing` by v4
 *              file so a whole unported surface shows up as one block.
 *
 * v4 sites whose value is an EXPRESSION are reported separately: a string match
 * cannot judge them, and the fill must transcribe v4's expression.
 *
 * ## Usage
 *
 *   node apps/web/scripts/title-census.mjs [--v4 <path-to-quilltap-server>]
 *                                          [--json] [--show-ok]
 *
 * Exit code is always 0 — this is a report, not a gate. (Making it a gate would
 * need an allowlist of the class-(iii) surfaces, and an allowlist that rots is
 * worse than a report someone reads; see `check-qt-classes.mjs`'s header for the
 * same argument.)
 */

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = fileURLToPath(new URL('.', import.meta.url));
const V5_APP = resolve(HERE, '../src/app');

function parseArgs(argv) {
  const out = { v4: resolve(process.env.HOME ?? '', 'source/quilltap-server'), json: false, showOk: false };
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === '--v4') { out.v4 = resolve(argv[i + 1]); i += 1; }
    else if (argv[i] === '--json') out.json = true;
    else if (argv[i] === '--show-ok') out.showOk = true;
  }
  return out;
}

function walk(dir, exts, acc = []) {
  let entries;
  try { entries = readdirSync(dir); } catch { return acc; }
  for (const name of entries) {
    if (name === 'node_modules' || name === '.next' || name === 'dist' || name.startsWith('.')) continue;
    const full = join(dir, name);
    const st = statSync(full);
    if (st.isDirectory()) walk(full, exts, acc);
    else if (exts.some((e) => name.endsWith(e))) acc.push(full);
  }
  return acc;
}

/**
 * The tag whose open-tag contains `index`. Walks backwards to the nearest `<`
 * that starts a tag name — JSX/Angular attributes only ever live inside one, and
 * a `<` inside an earlier attribute string would have to be unescaped inside the
 * SAME tag to fool this (no such site exists in either tree; the `component-prop`
 * split below is the sanity check that it worked).
 */
function enclosingTag(text, index) {
  for (let i = index; i >= 0; i -= 1) {
    if (text[i] !== '<') continue;
    const m = /^<([A-Za-z][A-Za-z0-9._-]*)/.exec(text.slice(i, i + 64));
    if (m) return m[1];
  }
  return null;
}

/**
 * True for a DOM element: lowercase first letter, no dot (JSX namespaces), and —
 * on the v5 side — not one of the app's OWN component selectors.
 *
 * v4's components are PascalCase JSX, so `isDomTag` alone separates a native
 * `title` attribute from a component prop there. Angular's are `qt-`-prefixed
 * custom elements, indistinguishable from a DOM tag by spelling — and
 * `<qt-collapsible-card title="Taboo">` is an INPUT, not a tooltip. The
 * selectors are read out of the source (the `check-qt-classes.mjs` idiom) so a
 * new component is subtracted the day it is written, with no allowlist to rot.
 */
function isDomTag(tag, componentSelectors) {
  if (!tag || !/^[a-z]/.test(tag) || tag.includes('.')) return false;
  return !componentSelectors?.has(tag);
}

/** Every `selector: 'x-y'` declared under the v5 app. */
function componentSelectors(root) {
  const out = new Set();
  for (const file of walk(root, ['.ts'])) {
    const text = readFileSync(file, 'utf8');
    for (const m of text.matchAll(/selector:\s*'([a-z][a-z0-9-]*)'/g)) out.add(m[1]);
  }
  return out;
}

/** A one-line excerpt for the report. */
function lineAt(text, index) {
  const start = text.lastIndexOf('\n', index) + 1;
  const end = text.indexOf('\n', index);
  return text.slice(start, end === -1 ? text.length : end).trim();
}

function lineNo(text, index) {
  let n = 1;
  for (let i = 0; i < index; i += 1) if (text[i] === '\n') n += 1;
  return n;
}

/**
 * Read the attribute value that starts at `at` (the char after `title=`).
 * Returns `{ kind: 'string', value }` | `{ kind: 'expr', value }` | null.
 */
function readJsxValue(text, at) {
  const c = text[at];
  if (c === '"' || c === "'") {
    const end = text.indexOf(c, at + 1);
    if (end === -1) return null;
    return { kind: 'string', value: text.slice(at + 1, end) };
  }
  if (c !== '{') return null;
  // Balance braces so a nested object/ternary is read whole.
  let depth = 0;
  let end = at;
  for (; end < text.length; end += 1) {
    if (text[end] === '{') depth += 1;
    else if (text[end] === '}') { depth -= 1; if (depth === 0) break; }
  }
  const inner = text.slice(at + 1, end).trim();
  const lit = /^(['"`])([^'"`$]*)\1$/.exec(inner);
  if (lit) return { kind: 'string', value: lit[2] };
  return { kind: 'expr', value: inner };
}

/** Angular value: always quoted (`title="x"`, `[title]="expr"`). */
function readNgValue(text, at, bound) {
  const c = text[at];
  if (c !== '"' && c !== "'") return null;
  const end = text.indexOf(c, at + 1);
  if (end === -1) return null;
  const raw = text.slice(at + 1, end);
  if (!bound) {
    // A plain attribute may still interpolate: title="{{ expr }}".
    return raw.includes('{{') ? { kind: 'expr', value: raw } : { kind: 'string', value: raw };
  }
  const lit = /^\s*(['"])(.*)\1\s*$/.exec(raw);
  return lit ? { kind: 'string', value: lit[2] } : { kind: 'expr', value: raw };
}

function collectV4(root) {
  const files = [...walk(join(root, 'app'), ['.tsx']), ...walk(join(root, 'components'), ['.tsx'])];
  const sites = [];
  for (const file of files) {
    const text = readFileSync(file, 'utf8');
    const re = /(^|[\s{])title=/g;
    let m;
    while ((m = re.exec(text)) !== null) {
      const at = m.index + m[0].length;
      const tag = enclosingTag(text, m.index);
      const value = readJsxValue(text, at);
      if (!value) continue;
      sites.push({
        file: relative(root, file),
        line: lineNo(text, m.index),
        tag,
        dom: isDomTag(tag),
        kind: value.kind,
        value: value.value,
        excerpt: lineAt(text, m.index),
      });
    }
  }
  return sites;
}

function collectV5(root) {
  const selectors = componentSelectors(root);
  const files = walk(root, ['.ts', '.html']).filter((f) => !f.endsWith('.spec.ts'));
  const titles = [];
  const tooltips = [];
  /**
   * Every single/double-quoted string literal in the v5 app. v5 frequently holds
   * tooltip copy in a TS table and binds it (`[title]="seg.title"`), which no
   * attribute scan can see — so a "missing" row is split by whether its string
   * exists in the source at all. Deliberately loose: it answers "is this copy in
   * the tree", not "is it wired to a title".
   */
  const literals = new Set();
  for (const file of files) {
    const text = readFileSync(file, 'utf8');
    const re = /(^|[\s{])(\[attr\.title\]|\[title\]|title)=/g;
    let m;
    while ((m = re.exec(text)) !== null) {
      const at = m.index + m[0].length;
      const bound = m[2] !== 'title';
      const tag = enclosingTag(text, m.index);
      const value = readNgValue(text, at, bound);
      if (!value) continue;
      titles.push({
        file: relative(root, file),
        line: lineNo(text, m.index),
        tag,
        dom: isDomTag(tag, selectors),
        kind: value.kind,
        value: value.value,
        excerpt: lineAt(text, m.index),
      });
    }
    for (const m2 of text.matchAll(/'((?:[^'\\\n]|\\.){2,400})'|"((?:[^"\\\n]|\\.){2,400})"/g)) {
      literals.add((m2[1] ?? m2[2]).replace(/\\'/g, "'").replace(/\\"/g, '"'));
    }

    const tre = /(^|[\s{])(\[content\]|content)=/g;
    while ((m = tre.exec(text)) !== null) {
      const tag = enclosingTag(text, m.index);
      if (tag !== 'qt-tooltip') continue;
      const value = readNgValue(text, m.index + m[0].length, m[2] !== 'content');
      if (value?.kind === 'string') {
        tooltips.push({ file: relative(root, file), line: lineNo(text, m.index), value: value.value });
      }
    }
  }
  return { titles, tooltips, literals };
}

const args = parseArgs(process.argv.slice(2));
const v4 = collectV4(args.v4);
if (v4.length === 0) {
  // `walk` swallows a missing directory so a partial tree still reports; a
  // wholly empty v4 side means the path is wrong, and a silent "0 missing" is
  // the worst possible answer to give.
  console.error(
    `No v4 title sites found under ${args.v4} — pass --v4 <path to a quilltap-server checkout or pinned worktree>.`,
  );
  process.exit(2);
}
const v5 = collectV5(V5_APP);

// DOM titles only — a component INPUT named `title` carrying the same string is
// not a native tooltip, and counting it would mark a real gap "ok".
const v5Strings = new Set([
  ...v5.titles.filter((s) => s.dom && s.kind === 'string').map((s) => s.value),
  ...v5.tooltips.map((s) => s.value),
]);

const v4Dom = v4.filter((s) => s.dom);
const v4Prop = v4.filter((s) => !s.dom);
const v4DomStrings = v4Dom.filter((s) => s.kind === 'string');
const v4DomExprs = v4Dom.filter((s) => s.kind === 'expr');

for (const s of v4DomStrings) {
  s.status = v5Strings.has(s.value)
    ? 'ok'
    : v5.literals.has(s.value)
      ? 'bound'
      : 'missing';
}

const missing = v4DomStrings.filter((s) => s.status === 'missing');
const bound = v4DomStrings.filter((s) => s.status === 'bound');
const ok = v4DomStrings.filter((s) => s.status === 'ok');

const v4Strings = new Set(v4DomStrings.map((s) => s.value));
const v5Only = v5.titles
  .filter((s) => s.dom && s.kind === 'string' && !v4Strings.has(s.value))
  .filter((s) => !v5.tooltips.some((t) => t.value === s.value));

function byFile(list) {
  const map = new Map();
  for (const s of list) {
    if (!map.has(s.file)) map.set(s.file, []);
    map.get(s.file).push(s);
  }
  return [...map.entries()].sort((a, b) => b[1].length - a[1].length);
}

if (args.json) {
  console.log(JSON.stringify({ v4Dom: v4Dom.length, v4Prop: v4Prop.length, ok: ok.length, bound, missing, v4DomExprs, v5Only }, null, 2));
} else {
  console.log(`v4 (${args.v4})`);
  console.log(`  title= occurrences ............ ${v4.length}`);
  console.log(`  on DOM elements ............... ${v4Dom.length}   (${v4DomStrings.length} string, ${v4DomExprs.length} expression)`);
  console.log(`  on components (props, N/A) .... ${v4Prop.length}`);
  console.log(`v5 (${relative(process.cwd(), V5_APP)})`);
  console.log(`  title/[title]/[attr.title] .... ${v5.titles.length} (${v5.titles.filter((s) => s.dom).length} on DOM elements)`);
  console.log(`  qt-tooltip string contents .... ${v5.tooltips.length}`);
  console.log('');
  console.log(`CLASSIFICATION of the ${v4DomStrings.length} v4 DOM string titles`);
  console.log(`  ok (a v5 DOM title / tooltip) . ${ok.length}`);
  console.log(`  bound (the string is in the v5 source, not in an attribute —`);
  console.log(`         a TS table read through [title]) ... ${bound.length}`);
  console.log(`  missing (absent from v5 entirely) ... ${missing.length}`);
  console.log('');
  console.log('--- missing, by v4 file (a whole unported surface shows as one block) ---');
  for (const [file, list] of byFile(missing)) {
    console.log(`\n${file}  (${list.length})`);
    for (const s of list) console.log(`  :${s.line}  <${s.tag}>  ${JSON.stringify(s.value)}`);
  }
  console.log('\n--- bound: the string lives in the v5 source, off the attribute ---');
  for (const [file, list] of byFile(bound)) {
    console.log(`\n${file}  (${list.length})`);
    for (const s of list) console.log(`  :${s.line}  <${s.tag}>  ${JSON.stringify(s.value)}`);
  }
  console.log('\n--- v4 DOM titles with an EXPRESSION value (judge by hand) ---');
  for (const [file, list] of byFile(v4DomExprs)) {
    console.log(`\n${file}  (${list.length})`);
    for (const s of list) console.log(`  :${s.line}  <${s.tag}>  {${s.value.replace(/\s+/g, ' ').slice(0, 100)}}`);
  }
  console.log('\n--- v5 DOM string titles with no v4 twin (v5-only; check they are not invented) ---');
  for (const [file, list] of byFile(v5Only)) {
    console.log(`\n${file}  (${list.length})`);
    for (const s of list) console.log(`  :${s.line}  <${s.tag}>  ${JSON.stringify(s.value)}`);
  }
  if (args.showOk) {
    console.log('\n--- ok ---');
    for (const [file, list] of byFile(ok)) {
      console.log(`\n${file}  (${list.length})`);
      for (const s of list) console.log(`  :${s.line}  <${s.tag}>  ${JSON.stringify(s.value)}`);
    }
  }
}
