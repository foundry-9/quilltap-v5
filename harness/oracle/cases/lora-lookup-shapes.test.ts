/**
 * Tier-1 oracle — the shapes v4's LoRA lookup actually hands the editor
 * (`2ece98c90`).
 *
 * Drives v4's REAL `lookupHuggingFaceLora` with `global.fetch` mocked over v4's
 * OWN payload fixtures (`REALISM_LORA_PAYLOAD` and the trigger-phrase /
 * multi-base / gated / ambiguous-weights / 401 variants from
 * `__tests__/unit/image-gen/huggingface-lookup.test.ts`) and records the
 * resulting `HuggingFaceLookupResult` objects verbatim.
 *
 * WHY record rather than hand-write the fixtures: the SPA's `LoraQueryResult`
 * panel renders these objects, and hand-writing them would mean inventing what
 * v4's fact derivation produces — the `base_model:adapter:` tag merge, the
 * card-first dedupe, the `gated` string-or-false, the `.safetensors` sibling
 * filter, the `instance_prompt` read. Recording them means the panel spec and
 * P4.D138's server mock corpus are the same shapes by construction, which is
 * what the order asks for.
 *
 * The network is never touched: `global.fetch` is replaced for every case, and
 * the payloads are v4's own trimmed captures.
 *
 * The output is a JSON vector file consumed by
 * `apps/web/src/app/screens/settings/images/lora-query-result.spec.ts` — the
 * SPA has no jest, so the comparand is committed rather than diffed in Rust.
 *
 * Regenerate (from a v4 worktree PINNED at `2ece98c90` — the module IS this
 * commit, drift-ledger §5.1. Node 24; jest ignores `/.claude/` paths so the
 * case is mirrored to /tmp first):
 *
 *   V5=~/source/quilltap-v5
 *   PIN=/tmp/qt-v4-pin-p4d139-2ece98c90
 *   mkdir -p /tmp/qt-oracle-lora-lookup
 *   cp $V5/harness/oracle/cases/lora-lookup-shapes.test.ts /tmp/qt-oracle-lora-lookup/
 *   cd $PIN
 *   PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   QT_ORACLE_OUT=$V5/apps/web/src/app/screens/settings/images/__fixtures__/lora-lookup-shapes.json \
 *     npx jest --silent --roots "$PWD" --roots /tmp/qt-oracle-lora-lookup \
 *       -- "lora-lookup-shapes\.test\.ts$"
 *
 * Verify the pin: the module does not exist before `2ece98c90`, so a run from
 * a baseline-pinned tree fails to resolve the import outright.
 *
 * @module harness/oracle/cases/lora-lookup-shapes
 */

/**
 * @jest-environment node
 */

import { writeFileSync } from 'fs'

import { describe, expect, it, jest } from '@jest/globals'

import { lookupHuggingFaceLora } from '@/lib/image-gen/huggingface-lookup'

jest.mock('@/lib/logger', () => ({
  logger: {
    debug: jest.fn(),
    info: jest.fn(),
    warn: jest.fn(),
    error: jest.fn(),
  },
}))

/** v4's own trimmed capture of the live XLabs-AI/flux-RealismLora response. */
const REALISM_LORA_PAYLOAD = {
  id: 'XLabs-AI/flux-RealismLora',
  private: false,
  pipeline_tag: 'text-to-image',
  library_name: 'diffusers',
  tags: [
    'diffusers',
    'lora',
    'Flux',
    'text-to-image',
    'base_model:black-forest-labs/FLUX.1-dev',
    'base_model:adapter:black-forest-labs/FLUX.1-dev',
    'license:other',
  ],
  downloads: 15707,
  likes: 1232,
  gated: false,
  lastModified: '2024-08-22T10:19:23.000Z',
  cardData: {
    license: 'other',
    pipeline_tag: 'text-to-image',
    tags: ['lora', 'Flux', 'diffusers'],
    base_model: 'black-forest-labs/FLUX.1-dev',
  },
  siblings: [
    { rfilename: '.gitattributes' },
    { rfilename: 'README.md' },
    { rfilename: 'lora.safetensors' },
  ],
}

interface Case {
  name: string
  source: string
  payload: unknown
  status?: number
}

const CASES: Case[] = [
  // v4's own fixtures, case for case.
  { name: 'realism-lora', source: 'XLabs-AI/flux-RealismLora', payload: REALISM_LORA_PAYLOAD },
  {
    name: 'trigger-phrase',
    source: 'Datou1111/shou_xin',
    payload: {
      id: 'Datou1111/shou_xin',
      tags: ['lora', 'base_model:adapter:black-forest-labs/FLUX.1-dev'],
      cardData: { instance_prompt: 'shou_xin, pencil sketch' },
      siblings: [{ rfilename: 'shou_xin.safetensors' }],
    },
  },
  {
    name: 'multi-base',
    source: 'someone/multi-base',
    payload: {
      id: 'someone/multi-base',
      tags: [
        'lora',
        'base_model:adapter:black-forest-labs/FLUX.1-dev',
        'base_model:adapter:black-forest-labs/FLUX.2-dev',
      ],
      cardData: { base_model: ['black-forest-labs/FLUX.1-dev'] },
      siblings: [],
    },
  },
  {
    name: 'gated',
    source: 'XLabs-AI/flux-RealismLora',
    payload: { ...REALISM_LORA_PAYLOAD, gated: 'auto' },
  },
  {
    name: 'ambiguous-weights',
    source: 'lovis93/Flux-2-Multi-Angles-LoRA-v2',
    payload: {
      id: 'lovis93/Flux-2-Multi-Angles-LoRA-v2',
      tags: ['lora'],
      siblings: [
        { rfilename: 'README.md' },
        { rfilename: 'flux-multi-angles-v2-72poses-comfy.safetensors' },
        { rfilename: 'flux-multi-angles-v2-72poses-fal.safetensors' },
      ],
    },
  },
  {
    name: 'unauthorized',
    source: 'nobody/nothing-at-all',
    payload: { error: 'Invalid username or password.' },
    status: 401,
  },

  // Shapes the panel renders that v4's suite does not build: a repo tagged an
  // adapter but not a LoRA, one tagged neither, and a card naming no base
  // model at all. Each drives a distinct `kindCopy` / `Trained on` arm.
  {
    name: 'adapter-not-lora',
    source: 'someone/plain-adapter',
    payload: {
      id: 'someone/plain-adapter',
      tags: ['adapter', 'base_model:adapter:black-forest-labs/FLUX.1-dev'],
      siblings: [{ rfilename: 'adapter.safetensors' }],
    },
  },
  {
    name: 'not-an-adapter',
    source: 'someone/full-checkpoint',
    payload: {
      id: 'someone/full-checkpoint',
      tags: ['text-to-image'],
      pipeline_tag: 'text-to-image',
      downloads: 42,
      likes: null,
      siblings: [{ rfilename: 'model.safetensors' }],
    },
  },
  {
    name: 'no-base-model',
    source: 'someone/bare',
    payload: { id: 'someone/bare', tags: ['lora'], siblings: [] },
  },
  // The source the button is never offered for — the panel still has to render
  // its failure copy, because the server can answer it.
  { name: 'not-a-repo-id', source: 'https://cdn.example.com/w.safetensors', payload: {} },
]

describe('lora-lookup-shapes oracle', () => {
  it('records v4 over the corpus', async () => {
    const originalFetch = global.fetch
    const rows: Array<{ name: string; source: string; result: unknown }> = []
    try {
      for (const c of CASES) {
        const status = c.status ?? 200
        global.fetch = jest.fn().mockResolvedValue({
          ok: status >= 200 && status < 300,
          status,
          json: async () => c.payload,
        } as Response) as unknown as typeof fetch
        rows.push({ name: c.name, source: c.source, result: await lookupHuggingFaceLora(c.source) })
      }
    } finally {
      global.fetch = originalFetch
    }

    const out = process.env.QT_ORACLE_OUT
    if (out) {
      writeFileSync(out, `${JSON.stringify(rows, null, 2)}\n`)
    }
    expect(rows).toHaveLength(CASES.length)
  })
})
