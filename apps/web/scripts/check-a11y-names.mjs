#!/usr/bin/env node
/**
 * SPA-wide guard against form controls and widgets with no accessible name
 * (P4.D358 — the static half of v4's WCAG pass, `eed72d60c` / `dff76ebef`).
 *
 * A control nobody can name is indistinguishable, at every automated layer we
 * have, from one that is named: it renders, TypeScript is satisfied, and the
 * vitest harness never asks a screen reader. v4's release-checklist-13 sweep
 * found 143-odd of them in v5's own templates. This script holds the line
 * the way `check-qt-classes.mjs` holds the class line: it reads every inline
 * Angular `template:` under `src/app` and fails the build on
 *
 *  1. an `<input>` / `<select>` / `<textarea>` (not `type="hidden"`) with none
 *     of: `aria-label` / `[attr.aria-label]` / `aria-labelledby` /
 *     `[attr.aria-labelledby]`; an `id` (static `id="x"`, or bound `[id]` /
 *     `[attr.id]`) that a `<label for>` in the SAME template points at (a bound
 *     id matches a bound `for` whose expression text is identical); or an
 *     enclosing OPEN `<label>` (no `</label>` between it and the control) —
 *     C9 R4 / R5 / R6;
 *  2. an element (not a `<button>`) carrying a static `role="button"` /
 *     `role="switch"` / `role="radio"` with no `aria-label[ledby]` binding and
 *     no text of its own before its closing tag (content names a button, as
 *     the platform says — v4's `role="button"` spans carry no `aria-label`) —
 *     C9 R7 / R8;
 *  3. an element carrying `role="dialog"` / `role="alertdialog"` with no
 *     `aria-label[ledby]` binding (a dialog is never named by its content) —
 *     C9 R1;
 *  4. a `<button>` whose only content is `<qt-icon …>` and that carries no
 *     `aria-label` / `[attr.aria-label]` / `aria-labelledby` / `title` /
 *     `[title]` — C9 R3. `title` counts (`CENSUS_TITLE_COUNTS_AS_NAME`, P4.D358
 *     R-6): the SPA names dozens of icon buttons that way, and the axe sweep is
 *     the stricter judge on buttons.
 *
 * A dynamic `[attr.role]` binding is not policed (its value is runtime).
 *
 * THE ALLOWLIST (`scripts/a11y-names-allowlist.json`, `{ "<path>": "<owner>" }`)
 * names ONLY files a SIBLING lane owns this round, whose fixes travel as
 * HANDOFFs. Their offenders are REPORTED with the owner's name but do not fail
 * the run; an allowlisted file with ZERO offenders IS a failure ("stale
 * allowlist row"), so the unifier's emptying is forced by the tool (§S.4).
 *
 * `--self-test` re-proves the rules over synthetic templates.
 */
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const SPA_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const APP_DIR = 'src/app'
const ALLOWLIST = path.join(SPA_ROOT, 'scripts/a11y-names-allowlist.json')

/** An inline template literal: `template: \`…\``. */
const TEMPLATE = /\btemplate:\s*`((?:[^`\\]|\\.)*)`/g

/** An opening tag (attribute values quoted — Angular bindings may hold `>`), or `</label>`. */
const TOKEN = /<([a-zA-Z][\w-]*)((?:\s+[^\s=>"'/]+(?:\s*=\s*(?:"[^"]*"|'[^']*'))?)*)\s*(\/?)>|<\/(label)\s*>/g

const ATTR = /([^\s=>"'/]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'))?/g

function attrsOf(text) {
  const out = new Map()
  for (const m of text.matchAll(ATTR)) out.set(m[1], m[2] ?? m[3] ?? '')
  return out
}

const NAMED = ['aria-label', '[attr.aria-label]', 'aria-labelledby', '[attr.aria-labelledby]']
const hasName = (attrs) => NAMED.some((a) => attrs.has(a))

/** Offenders in one template: `[offset, reason]`. */
export function templateOffenders(raw) {
  // Blank HTML comments (same length, newlines kept) so a `<select>` quoted in
  // a comment is not a control, and offsets still map to source lines.
  const tpl = raw.replace(/<!--[\s\S]*?-->/g, (c) => c.replace(/[^\n]/g, ' '))
  const tokens = [...tpl.matchAll(TOKEN)]
  const fors = new Set()
  const boundFors = new Set()
  for (const t of tokens) {
    if (t[1]?.toLowerCase() !== 'label') continue
    const a = attrsOf(t[2])
    if (a.has('for')) fors.add(a.get('for'))
    for (const k of ['[for]', '[attr.for]']) if (a.has(k)) boundFors.add(a.get(k).trim())
  }

  const found = []
  let openLabels = 0
  for (const t of tokens) {
    if (t[4]) {
      openLabels = Math.max(0, openLabels - 1)
      continue
    }
    const tag = t[1].toLowerCase()
    const attrs = attrsOf(t[2])
    const selfClosing = t[3] === '/'
    if (tag === 'label') {
      if (!selfClosing) openLabels++
      continue
    }
    if (tag === 'input' || tag === 'select' || tag === 'textarea') {
      if (tag === 'input' && attrs.get('type') === 'hidden') continue
      if (hasName(attrs) || openLabels > 0) continue
      if (attrs.has('id') && fors.has(attrs.get('id'))) continue
      const bound = attrs.get('[id]') ?? attrs.get('[attr.id]')
      if (bound !== undefined && boundFors.has(bound.trim())) continue
      found.push([t.index, `<${tag}> has no accessible name`])
      continue
    }
    const role = attrs.get('role')
    if (role === 'dialog' || role === 'alertdialog') {
      if (!hasName(attrs)) found.push([t.index, `role="${role}" has no aria-label / aria-labelledby`])
      continue
    }
    if ((role === 'button' && tag !== 'button') || role === 'switch' || role === 'radio') {
      if (hasName(attrs)) continue
      if (!selfClosing && ownText(tpl, t.index + t[0].length, tag)) continue
      found.push([t.index, `role="${role}" has no accessible name`])
      continue
    }
    if (tag === 'button' && !selfClosing) {
      if (hasName(attrs) || attrs.has('title') || attrs.has('[title]')) continue
      const body = innerOf(tpl, t.index + t[0].length, 'button')
      const stripped = body.replace(/<!--[\s\S]*?-->/g, '').replace(/<qt-icon\b[^>]*\/?>(?:\s*<\/qt-icon>)?/g, '')
      if (/<qt-icon\b/.test(body) && stripped.trim() === '') {
        found.push([t.index, 'icon-only <button> has no aria-label / title'])
      }
    }
  }
  return found
}

/** Text from `start` to the first `</tag>` (a flat approximation). */
function innerOf(tpl, start, tag) {
  const end = tpl.indexOf(`</${tag}>`, start)
  return end === -1 ? '' : tpl.slice(start, end)
}

/** Does the element have text of its own (literal or `{{ … }}`) before `</tag>`? */
function ownText(tpl, start, tag) {
  const inner = innerOf(tpl, start, tag).replace(/<!--[\s\S]*?-->/g, '').replace(/<[^>]*>/g, ' ')
  return inner.trim() !== ''
}

/** Every `[file, line, reason]` in one source file. */
export function fileOffenders(file, source) {
  const out = []
  for (const m of source.matchAll(TEMPLATE)) {
    const base = m.index + m[0].indexOf('`') + 1
    for (const [offset, reason] of templateOffenders(m[1])) {
      const line = source.slice(0, base + offset).split('\n').length
      out.push([file, line, reason])
    }
  }
  return out
}

function sourceFiles(dir = APP_DIR) {
  const out = []
  for (const entry of readdirSync(path.join(SPA_ROOT, dir))) {
    const rel = `${dir}/${entry}`
    if (statSync(path.join(SPA_ROOT, rel)).isDirectory()) out.push(...sourceFiles(rel))
    else if (entry.endsWith('.ts') && !entry.endsWith('.spec.ts')) out.push(rel)
  }
  return out
}

/**
 * `--self-test`: the guard's own red-first proofs over synthetic templates.
 */
function selfTest() {
  const cases = [
    ['bare input', '<input type="text" />', 1],
    ['hidden input exempt', '<input type="hidden" />', 0],
    ['aria-label', '<input aria-label="Search" />', 0],
    ['bound aria-label', '<select [attr.aria-label]="x()"></select>', 0],
    ['aria-labelledby', '<textarea aria-labelledby="h"></textarea>', 0],
    ['static for/id', '<label for="a">A</label><input id="a" />', 0],
    ['for/id mismatch', '<label for="a">A</label><input id="b" />', 1],
    ['bound for/id', `<label [for]="id + '-x'">A</label><input [id]="id + '-x'" />`, 0],
    ['bound for/id differ', `<label [for]="id + '-x'">A</label><input [id]="id + '-y'" />`, 1],
    ['wrapping label', '<label>Name <input /></label>', 0],
    ['closed label does not wrap', '<label>Name</label><input />', 1],
    ['span caption does not name', '<span>Name</span><select></select>', 1],
    ['role=button span with text', '<span role="button" tabindex="0">{{ name }}</span>', 0],
    ['role=button span, empty', '<span role="button" tabindex="0"></span>', 1],
    ['role=switch with label', '<button role="switch" aria-label="On"></button>', 0],
    ['role=switch, nameless', '<div role="switch" aria-checked="true"></div>', 1],
    ['dialog with labelledby', '<div role="dialog" aria-modal="true" [attr.aria-labelledby]="t"><h2>T</h2></div>', 0],
    ['dialog named by content only', '<div role="dialog" aria-modal="true"><h2>T</h2></div>', 1],
    ['icon-only button', '<button type="button"><qt-icon name="close" /></button>', 1],
    ['icon button with aria-label', '<button type="button" aria-label="Close"><qt-icon name="close" /></button>', 0],
    ['icon button with title', '<button type="button" title="Close"><qt-icon name="close" /></button>', 0],
    ['icon button with text', '<button type="button"><qt-icon name="plus" /> Add</button>', 0],
    ['a > in a binding', '<input [value]="a > b ? 1 : 2" />', 1],
    ['a control quoted in a comment', '<!-- the <select> below --><label>X <select></select></label>', 0],
  ]
  let failed = 0
  for (const [name, tpl, want] of cases) {
    const got = templateOffenders(tpl).length
    if (got === want) console.log(`  ok   ${name}`)
    else {
      failed++
      console.error(`  FAIL ${name}: got ${got} offender(s), want ${want}`)
    }
  }
  // Line numbers count from the file, not the template.
  const lines = fileOffenders('x.ts', "@Component({\n  template: `\n    <p></p>\n    <input />\n  `,\n})")
  if (lines.length === 1 && lines[0][1] === 4) console.log('  ok   file line numbers')
  else {
    failed++
    console.error(`  FAIL file line numbers: ${JSON.stringify(lines)}`)
  }
  console.log(`check-a11y-names --self-test: ${cases.length + 1 - failed}/${cases.length + 1} passed.`)
  process.exit(failed === 0 ? 0 : 1)
}

if (process.argv.includes('--self-test')) selfTest()

const allowlist = existsSync(ALLOWLIST) ? JSON.parse(readFileSync(ALLOWLIST, 'utf8')) : {}
const files = sourceFiles()
let controls = 0
const offenders = []
for (const file of files) {
  const source = readFileSync(path.join(SPA_ROOT, file), 'utf8')
  for (const m of source.matchAll(TEMPLATE)) {
    controls += [...m[1].matchAll(/<(?:input|select|textarea)\b/g)].length
  }
  offenders.push(...fileOffenders(file, source))
}

const byFile = new Map()
for (const o of offenders) byFile.set(o[0], [...(byFile.get(o[0]) ?? []), o])

const failing = offenders.filter(([file]) => !(file in allowlist))
const stale = Object.keys(allowlist).filter((file) => !byFile.has(file))
const owned = new Map()
for (const [file, owner] of Object.entries(allowlist)) {
  const n = byFile.get(file)?.length ?? 0
  if (n) owned.set(owner, (owned.get(owner) ?? 0) + n)
}

if (owned.size > 0) {
  console.log('check-a11y-names: allowlisted (sibling-owned, HANDOFF pending):')
  for (const [owner, n] of [...owned].sort()) console.log(`  ${owner}: ${n} offender(s)`)
  if (process.argv.includes('--verbose')) {
    for (const [file, owner] of Object.entries(allowlist)) {
      for (const [, line, reason] of byFile.get(file) ?? []) console.log(`    ${file}:${line} (${owner}) ${reason}`)
    }
  }
}

if (failing.length === 0 && stale.length === 0) {
  console.log(
    `check-a11y-names: ${files.length} files, ${controls} form controls; every control outside the allowlist is named.`,
  )
  process.exit(0)
}

if (failing.length > 0) {
  const nFiles = new Set(failing.map(([f]) => f)).size
  console.error(
    `\ncheck-a11y-names: ${failing.length} unnamed control(s) / widget(s) in ${nFiles} file(s) ` +
      `(of ${controls} form controls).\n` +
      `Name each with v4's bytes (C9 R1–R12): a <label for> + id, a wrapping <label>,\n` +
      `aria-labelledby to a heading, or v4's aria-label string — never an invented one.\n`,
  )
  for (const [file, line, reason] of failing) console.error(`  ${file}:${line}  ${reason}`)
}
for (const file of stale) {
  console.error(`  ${file}: stale allowlist row (${allowlist[file]}) — no offenders left; delete the row.`)
}
console.error('')
process.exit(1)
