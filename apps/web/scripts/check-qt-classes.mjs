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
 * (`qt-card`, `qt-chat-sidebar-section-participants`): plenty of those are
 * emitted purely as hooks for themes to target and are *meant* to have no rule
 * in the app's own CSS. Widening the net there would mean an allowlist that
 * rots, which is worse than the gap.
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
 * `npm test` runs it ahead of the unit suite so the workspace gate covers it.
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
 * Sites this lane (P4.D117) measurably found inert and was forbidden to touch:
 * `src/app/screens/new-chat/**` belongs to the parallel P4.D116 lane, and the
 * round's ownership table is binding. Every entry is `qt-text-tertiary`, whose
 * v4 rewrite is `qt-text-secondary` (`309aaa97`).
 *
 * This is a tripwire, not an allowlist: an entry that no longer resolves to
 * nothing is an ERROR, so the block cannot outlive the fix. **The unifier
 * applies the rewrite in those files and deletes this block.**
 */
const PENDING_CROSS_LANE_SITES = [
  ['qt-text-tertiary', 'src/app/screens/new-chat/green-room-dialog.ts'],
  ['qt-text-tertiary', 'src/app/screens/new-chat/outfit-slots-preview.ts'],
  ['qt-text-tertiary', 'src/app/screens/new-chat/outfit-slots-preview.spec.ts'],
]

const isPending = (token, site) =>
  PENDING_CROSS_LANE_SITES.some(([t, file]) => t === token && site.startsWith(`${file}:`))

const sources = tracked(SOURCE_GLOBS)
  .filter((file) => !SKIPPED_PREFIXES.some((p) => file.startsWith(p)))
  .map((file) => [file, readFileSync(path.join(SPA_ROOT, file), 'utf8')])

const defined = definedClasses()
const unresolved = [...usedClasses(sources, componentSelectors(sources)).entries()].filter(
  ([token]) => !defined.has(token)
)

const stale = PENDING_CROSS_LANE_SITES.filter(
  ([token, file]) => !unresolved.some(([t, sites]) => t === token && sites.some((s) => s.startsWith(`${file}:`)))
)
if (stale.length > 0) {
  console.error(
    `\ncheck-qt-classes: PENDING_CROSS_LANE_SITES is stale — ${stale.length} entr(y/ies) name a site\n` +
      `that now resolves (or no longer exists). The cross-lane hand-off is done: delete them.\n`
  )
  for (const [token, file] of stale) console.error(`  ${token}  ${file}`)
  console.error('')
  process.exit(1)
}

const missing = unresolved
  .map(([token, sites]) => [token, sites.filter((site) => !isPending(token, site))])
  .filter(([, sites]) => sites.length > 0)
  .sort((a, b) => b[1].length - a[1].length)

if (missing.length === 0) {
  const held = PENDING_CROSS_LANE_SITES.length
  console.log(
    `check-qt-classes: ${defined.size} qt-* classes defined, every guarded reference resolves` +
      (held > 0 ? ` (${held} cross-lane site(s) held — see PENDING_CROSS_LANE_SITES).` : '.')
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
