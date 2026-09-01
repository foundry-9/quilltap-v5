#!/usr/bin/env node
/**
 * SPA-wide guard against `qt-*` classes that resolve to nothing.
 *
 * A CSS class that does not exist is indistinguishable, at every automated
 * layer we have, from one that exists and happens to inherit: the markup
 * renders, TypeScript is satisfied, and jsdom does no cascade. Only a person
 * looking at a real browser can tell, and only if the missing colour happens
 * to be far from the inherited one. That is how v4's bug 39
 * (`qt-text-danger`), bug 100 (`qt-text-*-foreground`) and bug 102 (most of
 * the `hover:qt-bg-*` family) each survived for months — and v5 inherited all
 * three by porting the sheet.
 *
 * Two shapes recur, and this script fails the build on both:
 *
 *  1. A base utility nobody defined — usually the Tailwind name with a `qt-`
 *     bolted on (`qt-text-destructive-foreground`), or an opacity step the
 *     sheet never grew (`qt-bg-muted/50`).
 *  2. A variant form of a real utility. Tailwind generates variants only for
 *     utilities it knows about, and a class declared inside `@layer utilities`
 *     is invisible to it — so `hover:qt-bg-muted` is not "qt-bg-muted, on
 *     hover", it is an undefined class name. Every state form has to be written
 *     out by hand in `_utilities.css`, escaped, with its own pseudo-selector.
 *
 * SCOPE. The guard covers the four *utility* families — `qt-bg-`, `qt-text-`,
 * `qt-border-`, `qt-shadow-` — plus any `qt-*` token carrying a variant prefix,
 * whatever its family. It deliberately does not police bare component classes
 * on ORDINARY template elements (`qt-card`,
 * `qt-chat-sidebar-section-participants`): plenty of those are emitted purely
 * as hooks for themes to target and are *meant* to have no rule in the app's
 * own CSS. Widening the net there would mean an allowlist that rots, which is
 * worse than the gap.
 *
 * ONE bare-class site is policed anyway: an Angular component's OWN HOST
 * (`@Component({ host: { class: '…' } })`, static or a `[class.qt-…]`
 * binding). A theme hook that resolves to nothing inherits silently; a host
 * class that resolves to nothing is different in kind — it is the ONE place
 * an unruled `qt-*` name is load-bearing rather than decorative, because an
 * unstyled Angular custom element defaults to `display: inline`, and that has
 * shipped as a real bug three times (dogfood #97 `qt-tab-view`'s
 * `qt-standalone-document-view` host, the Almanack's `qt-entity-tabs`, dogfood
 * #107 `qt-markdown-field`). This is the NARROW form of that invariant — see
 * `docs/developer/porting/status-log.md` (P4.D142) for why the WIDER one (any
 * component host must have an explicit display, by a covering utility class,
 * host style, or bare-element rule — not just a `qt-*` class) was surveyed and
 * deliberately deferred rather than guessed at: the survey found roughly a
 * dozen existing hosts with no class, no style, and no bare-element rule at
 * all, and each needs its own visual judgment call this script cannot make.
 *
 * Escape hatch: a line containing `qt-class-exception` is skipped.
 *
 * v5 adaptation of v4's `scripts/check-qt-classes.mjs` (`309aaa97`): class
 * strings live in Angular inline templates inside `.ts` files and in `.html`
 * files rather than in `.tsx`, and the stylesheets it validates against are
 * `src/styles.css` + `src/styles/qt-components/*.css`. The bundled themes under
 * `public/themes/` are excluded from the *defined* set on purpose — like v4's
 * `app/`-only filter, a theme targeting a hook is not the app defining one.
 *
 * One further v5-only shape has to be subtracted: an Angular **component
 * selector** is a `qt-`-prefixed token too, and one of them —
 * `qt-text-replacement-settings` — collides head-on with the `qt-text-` family.
 * v4 cannot have this problem (its components are PascalCase JSX). Rather than
 * an allowlist that rots, the selectors are read out of the source itself, so a
 * new `<qt-bg-…>` component is subtracted the day it is written.
 *
 * Run standalone with `node scripts/check-qt-classes.mjs` (or `npm run lint`);
 * `npm test` runs it ahead of the unit suite so the workspace gate covers it,
 * and `--self-test` re-proves the guard's own mechanism over synthetic sources.
 *
 * ⚠ `definedClasses()` scans raw CSS text, comments included — a `.qt-foo`
 * spelled inside a CSS comment counts as a definition (P4.D142 hit this with
 * its own prose, and `_variables.css`'s RANGE banner names `.qt-range` that
 * way). Never rely on a comment-only mention being flagged.
 */

import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const SPA_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

/** Families whose bare (variant-free) names must resolve. */
const GUARDED_FAMILIES = /^qt-(bg|text|border|shadow)-/

/** Source trees whose class strings the SPA is responsible for. */
const SOURCE_GLOBS = ['*.ts', '*.html']
const SKIPPED_PREFIXES = ['scripts/', 'node_modules/']

/** Stylesheets that count as the app's own definitions. */
const DEFINING_PREFIXES = ['src/']

/**
 * A class token as it appears in markup: an optional chain of variant prefixes
 * (`hover:`, `focus:`, `group-hover/thumb:`) followed by the `qt-` name, which
 * may carry an opacity modifier (`/50`).
 */
const TOKEN = /((?:[a-z][a-z0-9-]*(?:\/[a-z0-9-]+)?:)*)(qt-[a-z0-9-]+(?:\/[0-9]+)?)/g

/**
 * The same thing as a CSS selector, where every `:` and `/` is backslashed.
 * Matched loosely on purpose: we only care that the class name appears at the
 * head of some selector, not what follows it.
 */
const SELECTOR = /\.((?:[a-z][a-z0-9-]*(?:\\\/[a-z0-9-]+)?\\:)*qt-[a-z0-9-]+(?:\\\/[0-9]+)?)/g

/** `selector: 'qt-…'` on an Angular component — a tag name, never a class. */
const COMPONENT_SELECTOR = /selector:\s*'(qt-[a-z0-9-]+)'/g

/**
 * An `@Component({ … })` decorator's metadata, up to (excluding) its own
 * `template:` key. Every component in this codebase declares an inline
 * `template:` (never `templateUrl`), so the FIRST `template:` after the
 * decorator opens bounds ONE decorator's header even in a file with several
 * components stacked back to back — the search resumes after each match, so
 * the next `@Component({` is found fresh rather than re-scanning consumed
 * text. The bound is `\btemplate:` on purpose, not "`template:` at 2-space
 * indent": a one-line `@Component({ selector: …, template: '…' })` (two spec
 * hosts write exactly that) matched no header under the indented form, and a
 * lazy `[\s\S]*?` then spanned from it into the NEXT component's header,
 * attributing that component's `host:` block to the wrong site and leaving
 * its own unchecked (the unification review's catch; `--self-test` pins it).
 */
const COMPONENT_HEADER = /@Component\(\{([\s\S]*?)\btemplate:/g

/** A component header's `host: { … }` object (assumed non-nested — true of
 * every host block in this codebase today; see the mutation-proof note by
 * `hostClasses` below if that stops holding). */
const HOST_BLOCK = /(?:^|\s)host:\s*\{([^{}]*)\}/
const HOST_STATIC_CLASS = /class:\s*'([^']*)'/
const HOST_DYNAMIC_CLASS = /\[class\.(qt-[a-z0-9-]+)\]/g

function tracked(globs) {
  return execFileSync('git', ['ls-files', ...globs], { cwd: SPA_ROOT, encoding: 'utf8' })
    .split('\n')
    .filter(Boolean)
}

/** Every `qt-*` class the SPA's own stylesheets define. */
function definedClasses() {
  const defined = new Set()
  const sheets = tracked(['*.css']).filter((f) => DEFINING_PREFIXES.some((p) => f.startsWith(p)))
  for (const file of sheets) {
    const css = readFileSync(path.join(SPA_ROOT, file), 'utf8')
    for (const match of css.matchAll(SELECTOR)) {
      defined.add(match[1].replaceAll('\\', ''))
    }
  }
  return defined
}

/** Every Angular component selector in the tree, so its tag is not read as a class. */
function componentSelectors(sources) {
  const selectors = new Set()
  for (const [, source] of sources) {
    for (const match of source.matchAll(COMPONENT_SELECTOR)) selectors.add(match[1])
  }
  return selectors
}

/** Every guarded `qt-*` token the SPA's markup reaches for, with its sites. */
function usedClasses(sources, selectors) {
  const used = new Map()
  for (const [file, source] of sources) {
    source.split('\n').forEach((line, i) => {
      if (line.includes('qt-class-exception')) return
      // `--qt-foo` is a custom property, not a class.
      for (const match of line.replaceAll(/--qt-[a-z0-9-]+/g, '').matchAll(TOKEN)) {
        const [token, variants, base] = match
        if (!variants && !GUARDED_FAMILIES.test(base)) continue
        if (selectors.has(base)) continue
        if (!used.has(token)) used.set(token, [])
        used.get(token).push(`${file}:${i + 1}`)
      }
    })
  }
  return used
}

/**
 * Every `qt-*` token an Angular component's OWN host declares — a static
 * `class: '…'` string and any `[class.qt-…]` conditional binding alike (the
 * chat sidebar's collapsed/overlay states are conditional and still need
 * this: `sidebar/chat-sidebar.ts`). Unlike `usedClasses`, this is NOT limited
 * to the four guarded utility families or variant-prefixed tokens — a bare
 * host class name is exactly what this check exists for (see the SCOPE note
 * at the top of the file).
 */
function hostClasses(sources) {
  const used = new Map()
  for (const [file, source] of sources) {
    for (const match of source.matchAll(COMPONENT_HEADER)) {
      const header = match[1]
      if (header.includes('qt-class-exception')) continue
      const hostMatch = header.match(HOST_BLOCK)
      if (!hostMatch) continue
      const hostBody = hostMatch[1]
      const site = `${file}:${source.slice(0, match.index).split('\n').length}`
      const tokens = new Set()
      const staticMatch = hostBody.match(HOST_STATIC_CLASS)
      if (staticMatch) {
        for (const t of staticMatch[1].split(/\s+/)) {
          if (/^qt-[a-z0-9-]+$/.test(t)) tokens.add(t)
        }
      }
      for (const m of hostBody.matchAll(HOST_DYNAMIC_CLASS)) tokens.add(m[1])
      for (const token of tokens) {
        if (!used.has(token)) used.set(token, [])
        used.get(token).push(site)
      }
    }
  }
  return used
}

/** The unresolved `qt-*` tokens over a source set, against a defined set. */
function unresolvedClasses(sources, defined) {
  const used = usedClasses(sources, componentSelectors(sources))
  for (const [token, sites] of hostClasses(sources)) {
    if (!used.has(token)) used.set(token, [])
    used.get(token).push(...sites)
  }
  return [...used.entries()].filter(([token]) => !defined.has(token))
}

/**
 * `--self-test`: the guard's own red-first proofs over synthetic sources, so
 * the mechanism is re-proven by `npm test` rather than by a one-off manual
 * mutation. Each case names the shape it pins.
 */
function selfTest() {
  const cases = [
    {
      name: 'a host class with no rule is reported at the component site',
      source: "@Component({\n  selector: 'qt-a',\n  host: { class: 'qt-nope block' },\n  template: '<p></p>',\n})\nclass A {}",
      defined: ['qt-other'],
      expect: [['qt-nope', ['x.ts:1']]],
    },
    {
      name: "a one-line @Component header is parsed, and does not swallow the next component's host block",
      source: "@Component({ selector: 'qt-one', template: '' })\nclass One {}\n\n@Component({\n  selector: 'qt-two',\n  host: { class: 'qt-second-host' },\n  template: '<p></p>',\n})\nclass Two {}",
      defined: [],
      expect: [['qt-second-host', ['x.ts:4']]],
    },
    {
      name: 'a conditional [class.qt-…] host binding is policed too',
      source: "@Component({\n  selector: 'qt-b',\n  host: { class: 'qt-b-host', '[class.qt-b-collapsed]': 'collapsed()' },\n  template: '<p></p>',\n})\nclass B {}",
      defined: ['qt-b-host'],
      expect: [['qt-b-collapsed', ['x.ts:1']]],
    },
    {
      name: 'a host whose classes all resolve reports nothing',
      source: "@Component({\n  selector: 'qt-c',\n  host: { class: 'qt-c-host' },\n  template: '<p class=\"qt-bg-card\"></p>',\n})\nclass C {}",
      defined: ['qt-c-host', 'qt-bg-card'],
      expect: [],
    },
    {
      name: 'a component selector is a tag, never a class (the qt-text-… collision)',
      source: "@Component({\n  selector: 'qt-text-thing',\n  template: '<qt-text-thing></qt-text-thing>',\n})\nclass T {}",
      defined: [],
      expect: [],
    },
  ]
  let failed = 0
  for (const c of cases) {
    const got = JSON.stringify(unresolvedClasses([['x.ts', c.source]], new Set(c.defined)))
    const want = JSON.stringify(c.expect)
    if (got === want) console.log(`  ok   ${c.name}`)
    else {
      failed++
      console.error(`  FAIL ${c.name}\n       got  ${got}\n       want ${want}`)
    }
  }
  console.log(`check-qt-classes --self-test: ${cases.length - failed}/${cases.length} passed.`)
  process.exit(failed === 0 ? 0 : 1)
}

if (process.argv.includes('--self-test')) selfTest()

const sources = tracked(SOURCE_GLOBS)
  .filter((file) => !SKIPPED_PREFIXES.some((p) => file.startsWith(p)))
  .map((file) => [file, readFileSync(path.join(SPA_ROOT, file), 'utf8')])

const defined = definedClasses()
const unresolved = unresolvedClasses(sources, defined)

const missing = unresolved.sort((a, b) => b[1].length - a[1].length)

if (missing.length === 0) {
  console.log(
    `check-qt-classes: ${defined.size} qt-* classes defined, every guarded reference resolves.`
  )
  process.exit(0)
}

const total = missing.reduce((n, [, sites]) => n + sites.length, 0)
console.error(
  `\ncheck-qt-classes: ${missing.length} qt-* class name(s) used in ${total} place(s) resolve to no CSS rule.\n` +
    `These render as nothing at all — no error, no warning, just an element that keeps\n` +
    `whatever it inherited. Define them in src/styles/qt-components/_utilities.css, or\n` +
    `change the markup to a class that exists.\n` +
    `A variant form (hover:, focus:, disabled:, …) needs its own hand-written escaped\n` +
    `selector — Tailwind generates none for classes declared in @layer utilities.\n`
)
for (const [token, sites] of missing) {
  console.error(`  ${token}`)
  for (const site of sites.slice(0, 5)) console.error(`      ${site}`)
  if (sites.length > 5) console.error(`      … and ${sites.length - 5} more`)
}
console.error('')
process.exit(1)
