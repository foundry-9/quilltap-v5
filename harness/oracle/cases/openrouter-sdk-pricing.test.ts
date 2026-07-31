/**
 * @jest-environment node
 *
 * P4.D33 `openrouter_sdk_pricing_equivalence` ORACLE — drives v4's REAL
 * authenticated OpenRouter pricing path (`getProviderPricing('OPENROUTER', …)`
 * → `fetchOpenRouterPricing`) with the **real `@openrouter/sdk`** in the loop and
 * only the network mocked underneath it, emitting the RAW wire pages plus the
 * `ModelPricing[]` v4 built from them.
 *
 * ## Why this exists (the seam the W4.7e oracle cannot see)
 *
 * `pricing-fetcher.test.ts` mocks `@openrouter/sdk` itself and hands
 * `models.list()` a hand-written **camelCase** `sdkBody`, so both sides of that
 * differential consume the same already-remapped object — a corpus that agrees
 * with itself. The real endpoint answers **snake_case**
 * (`context_length` / `supported_parameters`), and the SDK's
 * `Model$inboundSchema` zod transform `remap$`s those to `contextLength` /
 * `supportedParameters` before v4's parse ever sees them. v5's host seam returns
 * the raw body, so nothing reproduced that remap: `parse_openrouter_sdk` read
 * camelCase keys off a snake_case body and every model came back with
 * `contextLength: null` and `supportsTools: false`. Measured against the live
 * catalogue on 2026-07-30 (364 models): v4 resolves 364 context lengths and 298
 * tool-capable models; v5 resolved 0 and 0. This oracle is the instrument that
 * makes that a diff instead of an opinion.
 *
 * Same class as dogfood #24 (SDK-synthesized fields are not on the wire) and
 * P4.11's one-mode corpus: the differential was green because it never fed the
 * port the bytes production feeds it.
 *
 * ## What is mocked, and what deliberately is NOT
 *
 *   - `global.fetch` — serves the scenario's raw pages in order and RECORDS every
 *     request url, so the SDK's pagination is observable in the output.
 *   - `@/lib/repositories/factory` — one OPENROUTER profile + its api key.
 *   - `@openrouter/sdk` is **NOT** stubbed — but it takes TWO deliberate
 *     overrides to reach the real one, and both are load-bearing:
 *       1. v4's `jest.config.ts` maps `^@openrouter/sdk$` to
 *          `__mocks__/@openrouter/sdk.ts`, whose `models.list` is a bare
 *          `jest.fn()`. Left alone it returns `undefined`, `for await` throws,
 *          v4's catch returns `[]`, and this oracle emits four scenarios of
 *          nothing while passing. The `jest.mock` below re-points the specifier
 *          at the package's real entry by absolute path (the mapper is anchored
 *          `^…$`, so a path bypasses it). This is the same wrong-mock trap v4's
 *          own `13f0ebd7` commit message documents.
 *       2. `next/jest` overrides the repo's `transformIgnorePatterns`, so the
 *          package's ESM arrives untransformed ("Unexpected token 'export'").
 *          The run recipe below re-asserts the pattern on the CLI.
 *     Its zod parse, its `remap$`, and its `createPageIterator` then all run for
 *     real; that is the entire point.
 *   - Fake timers pin `fetchedAt`.
 *
 * ## The pagination arm
 *
 * `modelsList` stops paging when a page returns fewer rows than `limit` (500 when
 * the caller passes no request, which v4 does not). The `two-page` scenario
 * returns exactly 500 rows on page 1, so v4 issues a second `GET …?offset=500`
 * and accumulates both pages. v5's host must do the same or it silently truncates
 * a catalogue at 500 models (the live catalogue is at 364 and growing).
 *
 * Run from a v4 checkout pinned at the round baseline, under Node 24. The
 * `--transformIgnorePatterns` override is REQUIRED (see note 2 above); without
 * it the suite still passes and emits empty scenarios.
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd /private/tmp/qt-v4-pin-<order>-<sha>
 *   QT_ORACLE_OUT=/tmp/oracle-openrouter-sdk-pricing.ndjson \
 *     $N/npx jest --silent --watchman=false \
 *       --roots "$PWD" --roots "<staged cases dir>" \
 *       --transformIgnorePatterns "node_modules/(?!(@openrouter/sdk|jose)/)" \
 *       -- openrouter-sdk-pricing
 *
 * Line shape: { id, nowMs, pages, requests, outputs }.
 */

import * as fs from 'fs';

type Scenario = { id: string; nowMs: number; pages: unknown[] };

let current: Scenario;
let pageIdx = 0;
let requests: string[] = [];

// Re-point the specifier at the package's REAL entry, defeating the repo's
// `^@openrouter/sdk$` -> `__mocks__/@openrouter/sdk.ts` mapper. `process.cwd()`
// is the jest rootDir (the pinned v4 checkout).
jest.mock('@openrouter/sdk', () =>
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  require(`${process.cwd()}/node_modules/@openrouter/sdk/esm/index.js`)
);

jest.mock('@/lib/repositories/factory', () => ({
  __esModule: true,
  getRepositories: () => ({
    connections: {
      findAll: async () => [{ provider: 'OPENROUTER', apiKeyId: 'key-1', baseUrl: null }],
      findApiKeyByIdAndUserId: async (id: string) =>
        id === 'key-1' ? { key_value: 'test-openrouter-key' } : null,
    },
  }),
}));

function installFetch() {
  // The SDK hands `fetch` a Request; keep the string/URL arms for safety.
  // @ts-expect-error override global fetch
  global.fetch = jest.fn(async (input: unknown) => {
    const url =
      typeof input === 'string'
        ? input
        : input instanceof URL
          ? input.href
          : (input as { url: string }).url;
    requests.push(url);
    const body = current.pages[pageIdx] ?? { data: [] };
    pageIdx += 1;
    return new Response(JSON.stringify(body), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
  });
}

// ---------------------------------------------------------------------------
// Corpus. The three seed models are REAL objects captured from
// GET https://openrouter.ai/api/v1/models on 2026-07-30, with only the optional
// keys the schema does not require stripped (description / benchmarks /
// reasoning / alias_target / expiration_date / hugging_face_id /
// knowledge_cutoff). Real objects, not hand-written ones, so the SDK's zod
// schema validates them the way it validates production traffic — a
// hand-written model that failed `Model$inboundSchema` would make v4 return []
// and the differential would compare two empty lists and call it parity.
// ---------------------------------------------------------------------------

/** Vision + tools, six-figure context, tiered `pricing.overrides`. */
const VISION_TOOLS_MODEL = {
  id: 'qwen/qwen3.7-flash',
  canonical_slug: 'qwen/qwen3.7-flash-20260727',
  name: 'Qwen: Qwen3.7 Flash',
  created: 1785190561,
  context_length: 1000000,
  architecture: {
    modality: 'text+image+video->text',
    input_modalities: ['text', 'image', 'video'],
    output_modalities: ['text'],
    tokenizer: 'Qwen',
    instruct_type: null,
  },
  pricing: {
    prompt: '0.00000003',
    completion: '0.00000013',
    input_cache_read: '0.000000006',
    input_cache_write: '0.000000038',
    overrides: [
      {
        min_prompt_tokens: 32000,
        prompt: '0.0000001',
        completion: '0.0000004',
        input_cache_read: '0.00000002',
        input_cache_write: '0.000000125',
      },
    ],
  },
  top_provider: { context_length: 1000000, max_completion_tokens: 65536, is_moderated: false },
  per_request_limits: null,
  supported_parameters: [
    'include_reasoning',
    'logprobs',
    'max_tokens',
    'reasoning',
    'seed',
    'temperature',
    'tool_choice',
    'tools',
    'top_p',
  ],
  default_parameters: {
    temperature: null,
    top_p: null,
    top_k: null,
    frequency_penalty: null,
    presence_penalty: null,
    repetition_penalty: null,
  },
  supported_voices: null,
  links: { details: '/api/v1/models/qwen/qwen3.7-flash-20260727/endpoints' },
};

/** Text-only + tools, and free (`"0"` prices — the parseFloat-of-"0" arm). */
const TEXT_TOOLS_FREE_MODEL = {
  id: 'inclusionai/ling-3.0-flash:free',
  canonical_slug: 'inclusionai/ling-3.0-flash-20260723',
  name: 'Ling-3.0-flash (free)',
  created: 1784818580,
  context_length: 262144,
  architecture: {
    modality: 'text->text',
    input_modalities: ['text'],
    output_modalities: ['text'],
    tokenizer: 'Other',
    instruct_type: null,
  },
  pricing: { prompt: '0', completion: '0' },
  top_provider: { context_length: 262144, max_completion_tokens: 32768, is_moderated: false },
  per_request_limits: null,
  supported_parameters: [
    'frequency_penalty',
    'max_tokens',
    'seed',
    'stop',
    'temperature',
    'tool_choice',
    'tools',
    'top_p',
  ],
  default_parameters: {
    temperature: null,
    top_p: null,
    top_k: null,
    frequency_penalty: null,
    presence_penalty: null,
    repetition_penalty: null,
  },
  supported_voices: null,
  links: { details: '/api/v1/models/inclusionai/ling-3.0-flash-20260723/endpoints' },
};

/** Vision, NO tools — the `supportsTools: false` arm stays exercised. */
const VISION_NO_TOOLS_MODEL = {
  id: 'google/gemini-3.1-flash-lite-image',
  canonical_slug: 'google/gemini-3.1-flash-lite-image-20260630',
  name: 'Google: Nano Banana 2 Lite (Gemini 3.1 Flash Lite Image)',
  created: 1782837225,
  context_length: 65536,
  architecture: {
    modality: 'text+image->text+image',
    input_modalities: ['image', 'text'],
    output_modalities: ['image', 'text'],
    tokenizer: 'Gemini',
    instruct_type: null,
  },
  pricing: {
    prompt: '0.00000025',
    completion: '0.0000015',
    image_output: '0.00003',
    web_search: '0.014',
  },
  top_provider: { context_length: 65536, max_completion_tokens: 65536, is_moderated: false },
  per_request_limits: null,
  supported_parameters: [
    'include_reasoning',
    'max_tokens',
    'reasoning',
    'reasoning_effort',
    'response_format',
    'seed',
    'temperature',
    'top_p',
  ],
  default_parameters: {},
  supported_voices: null,
  links: { details: '/api/v1/models/google/gemini-3.1-flash-lite-image-20260630/endpoints' },
};

const SEEDS = [VISION_TOOLS_MODEL, TEXT_TOOLS_FREE_MODEL, VISION_NO_TOOLS_MODEL];

/**
 * `n` schema-valid filler models cloned off the seeds with distinct ids and
 * distinct prices (so `sortByCost` has a total order to express). Used only to
 * reach the SDK's 500-row page boundary.
 */
function filler(n: number, tag: string): unknown[] {
  const out: unknown[] = [];
  for (let i = 0; i < n; i += 1) {
    const seed = SEEDS[i % SEEDS.length] as Record<string, unknown>;
    const price = (0.0000001 * (i + 1)).toFixed(12);
    out.push({
      ...seed,
      id: `${tag}/model-${i}`,
      canonical_slug: `${tag}/model-${i}`,
      name: `${tag} model ${i}`,
      pricing: { prompt: price, completion: price },
    });
  }
  return out;
}

const scenarios: Scenario[] = [
  // A: one page of real models. The remap arm — contextLength / supportedParameters
  // / architecture.modality / the price math / sortByCost, all off snake_case wire.
  {
    id: 'single-page-real-models',
    nowMs: 1_700_000_000_000,
    pages: [{ data: SEEDS, total_count: SEEDS.length, links: { next: null } }],
  },
  // B: exactly 499 rows — one row under the SDK's default `limit` of 500, so the
  // page loop must STOP after one request. The boundary in the cheap direction.
  {
    id: 'single-page-at-499',
    nowMs: 1_700_000_000_000,
    pages: [{ data: filler(499, 'below'), total_count: 499, links: { next: null } }],
  },
  // C: 500 rows then 3 — v4 issues `GET …?offset=500` and accumulates 503 models.
  // A v5 host that fetches one page returns 500 and silently drops the tail.
  {
    id: 'two-page-pagination',
    nowMs: 1_700_000_000_000,
    pages: [
      { data: filler(500, 'p1'), total_count: 503, links: { next: '?offset=500' } },
      { data: SEEDS, total_count: 503, links: { next: null } },
    ],
  },
  // D: 500, 500, then 3. The two-page arm cannot tell `offset + results.length`
  // from `results.length` (page 1's offset is 0, so both give 500); only a THIRD
  // page distinguishes them, at `offset=1000` rather than `offset=500`. Found by
  // mutating the offset arithmetic and watching the two-page corpus stay green.
  {
    id: 'three-page-pagination',
    nowMs: 1_700_000_000_000,
    pages: [
      { data: filler(500, 'q1'), total_count: 1003, links: { next: '?offset=500' } },
      { data: filler(500, 'q2'), total_count: 1003, links: { next: '?offset=1000' } },
      { data: SEEDS, total_count: 1003, links: { next: null } },
    ],
  },
  // E: an empty catalogue — v4 falls through to FALLBACK_PRICING['OPENROUTER']
  // (or [] when it has none). Pins the no-models arm end to end.
  {
    id: 'empty-catalogue',
    nowMs: 1_700_000_000_000,
    pages: [{ data: [], total_count: 0, links: { next: null } }],
  },
];

async function run() {
  const rows: unknown[] = [];
  jest.useFakeTimers();

  for (const scenario of scenarios) {
    jest.resetModules();
    current = scenario;
    pageIdx = 0;
    requests = [];
    installFetch();
    jest.setSystemTime(scenario.nowMs);

    const { getProviderPricing } = await import('@/lib/llm/pricing-fetcher');
    const outputs = await getProviderPricing('OPENROUTER' as never, 'user-1');

    rows.push({
      id: scenario.id,
      nowMs: scenario.nowMs,
      pages: scenario.pages,
      requests,
      outputs,
    });
  }

  jest.useRealTimers();
  const out = process.env.QT_ORACLE_OUT;
  const text = rows.map((r) => JSON.stringify(r)).join('\n') + '\n';
  if (out) fs.writeFileSync(out, text);
  else process.stdout.write(text);
}

test('emit openrouter-sdk-pricing oracle', async () => {
  await run();
});
