#!/usr/bin/env node
/**
 * Repo-wide guard against CSS custom properties that resolve to nothing.
 *
 * A `var(--name)` whose property is defined nowhere, and which carries no
 * valid fallback, makes its whole declaration invalid at computed-value time.
 * The browser then quietly uses the inherited or initial value instead: the
 * rule looks right in the source, every check passes, and the colour (or the
 * focus ring, or the border) simply never appears. That is how the help dialog
 * and the Document Mode chrome spent months painting with shadcn-style names
 * (`hsl(var(--primary))`, `var(--qt-border)`) that this app has never defined.
 *
 * The rule this script enforces, for every `var()` in `app/**\/*.css`:
 *
 *  - the property is defined somewhere we can see — a `--name:` declaration in
 *    the app's CSS or a bundled theme's CSS, a key the theme loader maps tokens
 *    onto (`lib/themes/`, the theme style injector), a property set at runtime
 *    from TypeScript (`'--name'` as a string literal), or one of Tailwind's own
 *    built-ins; or
 *  - it is a per-theme hook, `var(--hook, <fallback>)`, whose fallback itself
 *    resolves by the same rule (recursively — a fallback of `hsl(var(--x))`
 *    with `--x` undefined is still nothing).
 *
 * Fallbacks are checked even when the property is defined: a fallback that
 * names nothing is a safety net with a hole in it.
 *
 * It also rejects `hsl(var(--color-*))` / `hsl(var(--theme-*))`: those tokens
 * already hold complete `hsl(...)` values, so wrapping them again is invalid.
 *
 * Escape hatch: a line containing `css-var-exception` is skipped.
 *
 * Run standalone with `node scripts/check-css-vars.mjs`; `npm run lint` runs
 * it alongside the other repo-wide guards.
 *
 * v5 port of v4's `scripts/check-css-vars.mjs` (`87c3860df`, read at
 * `624d0d25c`; P4.D383). Every rule is v4's, byte for byte; only the four
 * source sets are retargeted at the SPA's homes, and a `--self-test` is added
 * in the `check-qt-classes.mjs` idiom (v4 has none):
 *
 *  - `REPO_ROOT` is the SPA root (`apps/web`, this script's `..`) — `git
 *    ls-files` runs from there, so every path below is SPA-relative;
 *  - definitions come from `src/**` CSS (v4: `app/**`) and the bundled theme
 *    packs under `public/themes/**` (v4: `themes/**`);
 *  - TypeScript string literals come from `src/app/**\/*.ts` minus `*.spec.ts`
 *    (v4: every non-test `.ts`/`.tsx` outside packages / plugins / scripts);
 *  - the checked files are `src/*.css` + `src/**\/*.css` (v4: `app/…`), i.e.
 *    `src/styles.css` and `src/styles/qt-components/*.css`.
 *
 * Like v4's, it reads `.css` files only: an inline Angular `styles:` array or
 * a `[style.--x]` binding is not checked (a `'--x'` literal still DEFINES).
 * `npm test` runs it too, after `check-qt-classes`.
 */

import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

/** Tailwind v4 emits these itself; they are never declared in our sources. */
const TAILWIND_BUILTINS = new Set(['--spacing'])
const TAILWIND_PREFIXES = ['--tw-']

/** Tokens that already hold a complete colour value. */
const COMPLETE_COLOUR_TOKEN = /\bhsla?\(\s*var\(\s*--(color|theme)-/

function tracked(args) {
  return execFileSync('git', ['ls-files', ...args], { cwd: REPO_ROOT, encoding: 'utf8' })
    .split('\n')
    .filter(Boolean)
}

/** Replace comments with blanks, keeping newlines so line numbers survive. */
function stripComments(css) {
  return css.replace(/\/\*[\s\S]*?\*\//g, (c) => c.replace(/[^\n]/g, ' '))
}

/** Every `--name:` declaration in a stylesheet, into `defined`. */
function addCssDefinitions(raw, defined) {
  const css = stripComments(raw)
  for (const m of css.matchAll(/(--[A-Za-z0-9_-]+)\s*:/g)) defined.add(m[1])
}

/** Every `'--name'` string literal in a TypeScript source, into `defined`. */
function addTsLiterals(src, defined) {
  for (const m of src.matchAll(/['"`](--[A-Za-z0-9_-]+)['"`]/g)) defined.add(m[1])
  // Template-built names (`--theme-${key}`) are covered by the CSS side:
  // every such family is also declared with defaults in globals.css.
}

function definedProperties() {
  const defined = new Set(TAILWIND_BUILTINS)

  const cssSources = tracked(['*.css']).filter(
    (f) => f.startsWith('src/') || f.startsWith('public/themes/'),
  )
  for (const file of cssSources) {
    addCssDefinitions(readFileSync(path.join(REPO_ROOT, file), 'utf8'), defined)
  }

  // Properties named as string literals in TypeScript: the theme loader's
  // token map, the injector, and inline `style={{ '--x': … }}` / setProperty.
  const tsSources = tracked(['*.ts']).filter(
    (f) => f.startsWith('src/app/') && !f.endsWith('.spec.ts'),
  )
  for (const file of tsSources) {
    addTsLiterals(readFileSync(path.join(REPO_ROOT, file), 'utf8'), defined)
  }

  return defined
}

function isDefined(name, defined) {
  return defined.has(name) || TAILWIND_PREFIXES.some((p) => name.startsWith(p))
}

/** Index of the `)` that closes the `(` at `open`, or -1. */
function closingParen(text, open) {
  let depth = 0
  for (let i = open; i < text.length; i++) {
    if (text[i] === '(') depth++
    else if (text[i] === ')' && --depth === 0) return i
  }
  return -1
}

/** Index of the first comma at depth 0 in `text`, or -1. */
function topLevelComma(text) {
  let depth = 0
  for (let i = 0; i < text.length; i++) {
    if (text[i] === '(') depth++
    else if (text[i] === ')') depth--
    else if (text[i] === ',' && depth === 0) return i
  }
  return -1
}

/**
 * Every unresolvable `var()` in `text`, as `{ name, offset }`. A `var()` whose
 * property is undefined fails unless its fallback is non-empty and resolves.
 */
function unresolved(text, defined, base = 0) {
  const failures = []
  const re = /var\(\s*/g
  let m
  while ((m = re.exec(text))) {
    const open = m.index + 3
    const close = closingParen(text, open)
    if (close === -1) break
    const inner = text.slice(open + 1, close)
    const comma = topLevelComma(inner)
    const name = (comma === -1 ? inner : inner.slice(0, comma)).trim()
    const fallback = comma === -1 ? null : inner.slice(comma + 1)

    // A fallback is checked even when the property is defined: a fallback
    // naming nothing is dead code that looks like a safety net.
    const nested =
      fallback === null ? [] : unresolved(fallback, defined, base + open + 1 + comma + 1)
    if (!isDefined(name, defined)) {
      if (fallback === null || fallback.trim() === '') {
        failures.push({ name, offset: base + m.index })
      } else if (nested.length > 0) {
        failures.push({ name, offset: base + m.index, nested })
      }
    } else if (nested.length > 0) {
      failures.push({ name, offset: base + m.index, nested, definedName: true })
    }
    re.lastIndex = close + 1
  }
  return failures
}

function lineOf(text, offset) {
  let line = 1
  for (let i = 0; i < offset; i++) if (text[i] === '\n') line++
  return line
}

function describe(failure) {
  if (!failure.nested) return `${failure.name} (undefined, no fallback)`
  if (failure.definedName) {
    return `${failure.name} (fallback ${failure.nested.map(describe).join(', ')})`
  }
  return `${failure.name} (undefined; fallback ${failure.nested.map(describe).join(', ')})`
}

/** Every problem in one checked stylesheet, as `<file>:<line>  <describe>`. */
function problemsIn(file, raw, defined) {
  const problems = []
  const rawLines = raw.split('\n')
  const css = stripComments(raw)
  const skip = (line) => rawLines[line - 1]?.includes('css-var-exception')

  for (const failure of unresolved(css, defined)) {
    const line = lineOf(css, failure.offset)
    if (!skip(line)) problems.push(`${file}:${line}  ${describe(failure)}`)
  }

  css.split('\n').forEach((text, i) => {
    if (COMPLETE_COLOUR_TOKEN.test(text) && !skip(i + 1)) {
      problems.push(
        `${file}:${i + 1}  hsl() wrapped around a --color-*/--theme-* token, which is already a full colour`,
      )
    }
  })
  return problems
}

/**
 * `--self-test`: the guard's own red-first proofs over synthetic sources (v5
 * only — v4 ships none), so `npm test` re-proves the mechanism rather than a
 * one-off manual mutation. Each case names the shape it pins; `defined` seeds
 * the definition set exactly as `definedProperties` would (Tailwind's
 * built-ins included), and `ts` adds a TypeScript source's literals.
 */
function selfTest() {
  const cases = [
    {
      name: 'an undefined property with no fallback is reported',
      css: 'a { color: var(--nope); }',
      defined: [],
      expect: ['x.css:1  --nope (undefined, no fallback)'],
    },
    {
      name: 'an empty fallback is no fallback',
      css: 'a { color: var(--nope,); }',
      defined: [],
      expect: ['x.css:1  --nope (undefined, no fallback)'],
    },
    {
      name: 'an undefined hook with a resolving fallback passes',
      css: 'a { color: var(--qt-hook, var(--color-primary)); }',
      defined: ['--color-primary'],
      expect: [],
    },
    {
      name: 'an undefined hook whose fallback names nothing is reported',
      css: 'a { color: var(--qt-hook, var(--nope)); }',
      defined: [],
      expect: ['x.css:1  --qt-hook (undefined; fallback --nope (undefined, no fallback))'],
    },
    {
      name: 'a DEFINED property whose fallback names nothing is reported',
      css: 'a { color: var(--color-primary, var(--nope)); }',
      defined: ['--color-primary'],
      expect: ['x.css:1  --color-primary (fallback --nope (undefined, no fallback))'],
    },
    {
      name: 'a nested hsl(var(--x)) fallback with --x undefined is still nothing',
      css: 'a { color: var(--qt-hook, hsl(var(--primary))); }',
      defined: [],
      expect: ['x.css:1  --qt-hook (undefined; fallback --primary (undefined, no fallback))'],
    },
    {
      name: 'hsl() around a complete --color-* token is reported',
      css: 'a { color: hsl(var(--color-primary)); }',
      defined: ['--color-primary'],
      expect: [
        'x.css:1  hsl() wrapped around a --color-*/--theme-* token, which is already a full colour',
      ],
    },
    {
      name: "Tailwind's --tw-* prefix and --spacing built-in resolve",
      css: 'a { width: calc(var(--spacing) * 2); box-shadow: var(--tw-ring-shadow); }',
      defined: [],
      expect: [],
    },
    {
      name: 'a css-var-exception line is skipped',
      css: 'a { color: var(--nope); } /* css-var-exception */',
      defined: [],
      expect: [],
    },
    {
      name: 'a var() inside a comment is ignored and later line numbers survive',
      css: '/* var(--in-a-comment)\n   spans lines */\na { color: var(--nope); }',
      defined: [],
      expect: ['x.css:3  --nope (undefined, no fallback)'],
    },
    {
      name: 'a --name: declaration in the CSS defines the name',
      css: ':root { --qt-local: red; }\na { color: var(--qt-local); }',
      defined: [],
      expect: [],
    },
    {
      name: "a '--name' TypeScript literal defines the name",
      css: 'a { color: var(--qt-runtime-x); }',
      ts: "el.style.setProperty('--qt-runtime-x', 'red')",
      defined: [],
      expect: [],
    },
  ]
  let failed = 0
  for (const c of cases) {
    const defined = new Set([...TAILWIND_BUILTINS, ...c.defined])
    addCssDefinitions(c.css, defined)
    if (c.ts) addTsLiterals(c.ts, defined)
    const got = JSON.stringify(problemsIn('x.css', c.css, defined))
    const want = JSON.stringify(c.expect)
    if (got === want) console.log(`  ok   ${c.name}`)
    else {
      failed++
      console.error(`  FAIL ${c.name}\n       got  ${got}\n       want ${want}`)
    }
  }
  console.log(`check-css-vars --self-test: ${cases.length - failed}/${cases.length} passed.`)
  process.exit(failed === 0 ? 0 : 1)
}

function main() {
  const defined = definedProperties()
  const problems = []

  for (const file of tracked(['src/*.css', 'src/**/*.css'])) {
    const raw = readFileSync(path.join(REPO_ROOT, file), 'utf8')
    problems.push(...problemsIn(file, raw, defined))
  }

  if (problems.length > 0) {
    console.error(
      `\nUndefined CSS custom properties (${problems.length}). Point each at a real token ` +
        '(`--color-*`, a `--qt-*` variable from _variables.css) or give the hook a valid fallback:\n',
    )
    for (const p of problems) console.error(`  ${p}`)
    console.error('')
    process.exit(1)
  }
  console.log(`check-css-vars: ${defined.size} custom properties defined, every var() resolves.`)
}

if (process.argv.includes('--self-test')) selfTest()
else main()
