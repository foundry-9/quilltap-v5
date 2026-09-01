/**
 * Tier-1 oracle — v4's `appliesToModels` matchers (`84f33ce94`).
 *
 * Imports v4's REAL `lib/plugins/model-matchers.ts` and records what
 * `modelMatchesPattern` / `fieldAppliesToModel` actually return over a fixed
 * corpus. Nothing here reimplements the matcher.
 *
 * WHY beyond the 1:1 transcription of v4's own unit test: the glob arm builds
 * a `RegExp` out of the pattern, and the questions a transcribed suite never
 * asks are the ones that bite — which characters the escape class covers
 * (`-` and `/` are NOT in it), what a bare `*` or a doubled `**` compiles to,
 * whether the anchors make a glob a whole-string match rather than a substring
 * search, and how the escape treats a pattern that is already regex-shaped.
 * The corpus asks all of them, so v5's twin is pinned to v4's ACTUAL regex
 * behaviour rather than to the eleven expectations v4's suite happened to
 * write.
 *
 * The output is a JSON vector file consumed by
 * `apps/web/src/app/screens/settings/providers/model-matchers.spec.ts` — the
 * SPA has no jest, so the comparand is committed rather than diffed in Rust.
 *
 * Regenerate (from a v4 worktree PINNED at `2ece98c90` — the LoRA train's tip;
 * the module arrives at `84f33ce94` and is untouched after, drift-ledger §5.1.
 * Node 24; jest ignores `/.claude/` paths so the case is mirrored to /tmp
 * first):
 *
 *   V5=~/source/quilltap-v5
 *   PIN=/tmp/qt-v4-pin-p4d139-2ece98c90
 *   mkdir -p /tmp/qt-oracle-model-matchers
 *   cp $V5/harness/oracle/cases/model-matchers.test.ts /tmp/qt-oracle-model-matchers/
 *   cd $PIN
 *   PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   QT_ORACLE_OUT=$V5/apps/web/src/app/screens/settings/providers/__fixtures__/model-matchers-vectors.json \
 *     npx jest --silent --roots "$PWD" --roots /tmp/qt-oracle-model-matchers \
 *       -- "model-matchers\.test\.ts$"
 *
 * Verify the pin: the module does not exist before `84f33ce94`, so a run from
 * a baseline-pinned tree fails to resolve the import outright.
 *
 * @module harness/oracle/cases/model-matchers
 */

import { writeFileSync } from 'fs'

import { describe, expect, it } from '@jest/globals'

import { fieldAppliesToModel, modelMatchesPattern } from '@/lib/plugins/model-matchers'

/** `[model, pattern]` pairs put to v4's real `modelMatchesPattern`. */
const PATTERN_CASES: Array<[string, string]> = [
  // v4's own suite, so the recording is a superset of the transcription.
  ['flux-lora', 'flux-lora'],
  ['hidream', 'flux-lora'],
  ['flux-lora/inpainting', 'flux-lora'],
  ['flux', 'flux-lora'],
  ['wavespeed-ai/krea-v2/turbo-lora', 'wavespeed-ai/*'],
  ['pruna-ai/p-image/edit-lora', 'wavespeed-ai/*'],
  ['z-image-turbo-lora', '*-lora'],
  ['z-image-turbo', '*-lora'],
  ['gpt-image-125', 'gpt-image-1.5*'],
  ['gpt-image-1.5', 'gpt-image-1.5*'],
  ['anything', ''],

  // What the escape class does NOT cover. `-` and `/` are absent from
  // /[.*+?^${}()|[\]\\]/g, and neither is special outside a character class,
  // so they pass through as themselves. A twin that "helpfully" escaped more
  // characters would still pass every case above.
  ['a-b/c', 'a-b/*'],
  ['a-b/c', '*/c'],

  // A bare glob is total; a doubled one collapses to `.*.*`, still total.
  ['whatever', '*'],
  ['', '*'],
  ['whatever', '**'],

  // The anchors are the whole matter: a glob is a whole-string match, not a
  // substring search.
  ['prefix-flux-2-dev', 'flux-2-*'],
  ['flux-2-dev-suffix', 'flux-2-*'],
  ['flux-2-dev', 'flux-2-*'],
  ['a/b/c', '*/b/*'],

  // Regex metacharacters that are not `*` must survive as literals.
  ['gpt-image-1x5', 'gpt-image-1.5*'],
  ['a+b', 'a+b*'],
  ['axxb', 'a+b*'],
  ['a(b)c', 'a(b)*'],
  ['a[b]c', 'a[b]*'],
  ['a|b', 'a|b*'],
  ['a$b', 'a$b*'],
  ['a^b', 'a^b*'],
  ['a{2}b', 'a{2}*'],
  ['aab', 'a{2}*'],
  ['a.b', 'a.b'],

  // Prefix, not equality — and prefix is case-sensitive.
  ['flux-lora', 'flux'],
  ['FLUX-LORA', 'flux-lora'],
  ['flux-lora', ' '],

  // Every string starts with the empty prefix, so the plain-prefix arm alone
  // would answer `true` here — the empty-pattern guard fires first and makes
  // it `false`. The ORDER of those two lines is this case's entire content.
  ['flux', ''],
  ['', ''],
  ['', 'flux'],
]

/** `[list, model]` pairs put to v4's real `fieldAppliesToModel`. */
const FIELD_CASES: Array<[string[] | undefined, string | undefined]> = [
  // v4's own suite.
  [undefined, 'hidream'],
  [[], 'hidream'],
  [['flux-lora'], undefined],
  [['hidream', 'flux-2-*'], 'flux-2-dev-lora'],
  [['hidream', 'flux-2-*'], 'recraft-v3'],

  // The empty string is the shape v5's panel actually passes for "unknown"
  // (its `modelName` input defaults to `''`, never `undefined`). It must
  // resolve toward showing, exactly as `undefined` does.
  [['flux-lora'], ''],
  [['hidream', 'flux-2-*'], ''],

  // An empty-pattern entry inside a non-empty list matches nothing, so a list
  // of nothing but empties HIDES rather than shows — the two empty-list arms
  // are not the same arm.
  [[''], 'hidream'],
  [['', 'hidream'], 'hidream'],
  [['', 'hidream'], 'recraft-v3'],

  [['*'], 'anything'],
  [['flux', 'hidream'], 'flux-2-dev'],
]

describe('model-matchers oracle', () => {
  it('records v4 over the corpus', () => {
    const patterns = PATTERN_CASES.map(([model, pattern]) => ({
      model,
      pattern,
      matches: modelMatchesPattern(model, pattern),
    }))
    const fields = FIELD_CASES.map(([appliesToModels, model]) => ({
      appliesToModels: appliesToModels ?? null,
      model: model ?? null,
      applies: fieldAppliesToModel(appliesToModels, model),
    }))

    const out = process.env.QT_ORACLE_OUT
    if (out) {
      writeFileSync(out, `${JSON.stringify({ patterns, fields }, null, 2)}\n`)
    }
    // The recording itself is the assertion the oracle owes: every case ran.
    expect(patterns).toHaveLength(PATTERN_CASES.length)
    expect(fields).toHaveLength(FIELD_CASES.length)
  })
})
