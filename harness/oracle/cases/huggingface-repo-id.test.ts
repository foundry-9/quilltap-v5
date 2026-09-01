/**
 * Tier-1 oracle — v4's LoRA-source repo-id reader (`2ece98c90`).
 *
 * Imports v4's REAL `lib/image-gen/huggingface-repo-id.ts` and records what
 * `extractHuggingFaceRepoId` / `huggingFaceCardUrl` actually return over a
 * fixed corpus. Nothing here reimplements the parse.
 *
 * WHY a recording ON TOP of v4's own table: v4's four repo-id cases live not
 * beside the module but inside `__tests__/unit/image-gen/huggingface-lookup.test.ts`
 * (the module is re-exported from the lookup), and they are transcribed 1:1 in
 * the consuming spec. They cover the happy shapes and eight refusals; they do
 * not reach the machinery. The client half decides whether the Query button is
 * offered at all, so its edges have to come from somewhere — and every
 * interesting one
 * is a question about `new URL(...)` and a hostname regex rather than about
 * the source string: what `URL` does to a userinfo `@`, a port, a trailing
 * dot, an uppercase host, a `//` path, percent-encoding, and whether
 * `(^|\.)huggingface\.co$` refuses `nothuggingface.co` while accepting
 * `hf.huggingface.co`. The corpus asks all of those against v4 itself.
 *
 * The output is a JSON vector file consumed by
 * `apps/web/src/app/screens/settings/images/huggingface-repo-id.spec.ts` — the
 * SPA has no jest, so the comparand is committed rather than diffed in Rust.
 *
 * Regenerate (from a v4 worktree PINNED at `2ece98c90` — the module IS this
 * commit, drift-ledger §5.1. Node 24; jest ignores `/.claude/` paths so the
 * case is mirrored to /tmp first):
 *
 *   V5=~/source/quilltap-v5
 *   PIN=/tmp/qt-v4-pin-p4d139-2ece98c90
 *   mkdir -p /tmp/qt-oracle-hf-repo-id
 *   cp $V5/harness/oracle/cases/huggingface-repo-id.test.ts /tmp/qt-oracle-hf-repo-id/
 *   cd $PIN
 *   PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   QT_ORACLE_OUT=$V5/apps/web/src/app/screens/settings/images/__fixtures__/huggingface-repo-id-vectors.json \
 *     npx jest --silent --roots "$PWD" --roots /tmp/qt-oracle-hf-repo-id \
 *       -- "huggingface-repo-id\.test\.ts$"
 *
 * Verify the pin: the module does not exist before `2ece98c90`, so a run from
 * a baseline-pinned tree fails to resolve the import outright.
 *
 * @module harness/oracle/cases/huggingface-repo-id
 */

import { writeFileSync } from 'fs'

import { describe, expect, it } from '@jest/globals'

import {
  extractHuggingFaceRepoId,
  huggingFaceCardUrl,
} from '@/lib/image-gen/huggingface-repo-id'

const SOURCES: string[] = [
  // The two shapes the feature exists for.
  'owner/model-name',
  'https://huggingface.co/owner/model-name',
  // The deep link the fal-hosted models want, named in v4's own doc.
  'https://huggingface.co/owner/model-name/resolve/main/weights.safetensors',
  'https://huggingface.co/owner/model-name/blob/main/x.safetensors',

  // Trimming, and the empty forms.
  '  owner/model-name  ',
  '',
  '   ',
  '\towner/model-name\n',

  // Weights hosted elsewhere: the editor's signal not to offer the button.
  'https://example.com/owner/model-name/weights.safetensors',
  'https://cdn.example.org/x.safetensors',

  // The hostname regex is `(^|\.)huggingface\.co$`. These are the arms.
  'https://huggingface.co/owner/name',
  'https://nothuggingface.co/owner/name',
  'https://huggingface.co.evil.example/owner/name',
  'https://hf.huggingface.co/owner/name',
  'https://HUGGINGFACE.CO/owner/name',
  'HTTPS://huggingface.co/owner/name',
  'https://huggingface.co./owner/name',
  'http://huggingface.co/owner/name',
  'https://huggingface.co:443/owner/name',
  'https://user:pw@huggingface.co/owner/name',

  // Not a URL at all by the `^https?://` test, so it falls to the bare form
  // (and fails REPO_ID_PATTERN because of the slashes and the colon).
  'ftp://huggingface.co/owner/name',
  'huggingface.co/owner/name',
  '//huggingface.co/owner/name',

  // Segment counting: `filter(Boolean)` drops the empties, so doubled and
  // trailing slashes do not shift the first two segments.
  'https://huggingface.co/owner',
  'https://huggingface.co/',
  'https://huggingface.co',
  'https://huggingface.co//owner//name',
  'https://huggingface.co/owner/name/',
  'https://huggingface.co/models/owner/name',

  // REPO_ID_PATTERN: two segments, each starting alphanumeric, then
  // [A-Za-z0-9._-]. These probe both ends of that class.
  'Owner/Model_Name-1.0',
  'owner/.hidden',
  '.owner/name',
  '-owner/name',
  'owner/-name',
  'owner/name.safetensors',
  'owner/name/extra',
  'owner',
  'owner/',
  '/name',
  'own er/name',
  'owner/na me',
  'öwner/name',
  'owner/näme',
  'o/n',
  '0/9',
  'owner//name',

  // Percent-encoding survives `URL`'s pathname untouched, so the pattern sees
  // the `%` and refuses.
  'https://huggingface.co/own%20er/name',
  // ...but a query string and a fragment are not pathname, so they vanish.
  'https://huggingface.co/owner/name?download=true',
  'https://huggingface.co/owner/name#card',
]

describe('huggingface-repo-id oracle', () => {
  it('records v4 over the corpus', () => {
    const rows = SOURCES.map((source) => {
      const repoId = extractHuggingFaceRepoId(source)
      return {
        source,
        repoId,
        cardUrl: repoId === null ? null : huggingFaceCardUrl(repoId),
      }
    })

    const out = process.env.QT_ORACLE_OUT
    if (out) {
      writeFileSync(out, `${JSON.stringify(rows, null, 2)}\n`)
    }
    expect(rows).toHaveLength(SOURCES.length)
  })
})
