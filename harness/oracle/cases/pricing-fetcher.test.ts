/**
 * @jest-environment node
 *
 * W4.7e `pricing_fetcher_equivalence` ORACLE — drives v4's REAL pricing-fetcher +
 * cost-estimation + checkModelSupportsTools over scripted fetch/SDK/repo mocks,
 * emitting per-scenario the inputs (so the Rust `services::pricing_fetcher` can
 * replay on a fresh `PricingFetcher` with the same `SeqFetch`) and v4's outputs.
 *
 * Mocks (mirroring the Rust seams):
 *   - `global.fetch` — the openrouter.ai public models endpoint (scripted ok/fail
 *     per call) and Ollama `/api/tags`.
 *   - `@openrouter/sdk` — the authenticated SDK `models.list()` (camelCase shape).
 *   - `@/lib/repositories/factory` — `connections.findAll` (profiles) +
 *     `findApiKeyByIdAndUserId` (keys), standing in for v4's repos.
 *   - Fake timers control `Date.now()` / `new Date()` per op (TTL + fetchedAt).
 *
 * Each scenario resets the module registry so v4's module-global pricing caches
 * start empty (matching the Rust fresh-instance).
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-pricing-fetcher.ndjson \
 *     $N/npx jest --silent --watchman=false \
 *       --roots "$PWD" --roots "$V5/harness/oracle/cases" -- pricing-fetcher
 */

import * as fs from 'fs';

// Mutable per-scenario config the mocks read at call time.
type Profile = { provider: string; apiKeyId: string | null; baseUrl: string | null };
type Scenario = {
  id: string;
  publicBody: unknown;
  publicScript: Array<'ok' | 'fail'>;
  sdkBody: unknown | null;
  ollamaBody: unknown | null;
  profiles: Profile[];
  apiKeys: Record<string, string>;
  ops: Op[];
};
type Op =
  | { kind: 'getORForModel'; provider: string; modelId: string; nowMs: number }
  | { kind: 'checkTools'; provider: string; modelName: string; nowMs: number }
  | {
      kind: 'estimate';
      provider: string;
      modelName: string;
      promptTokens: number;
      completionTokens: number;
      nowMs: number;
    }
  | {
      kind: 'findCheapest';
      nowMs: number;
      requireVision?: boolean;
      requireTools?: boolean;
      excludeProviders?: string[];
    };

let current: Scenario;
let publicScriptIdx = 0;

jest.mock('@openrouter/sdk', () => ({
  __esModule: true,
  OpenRouter: class {
    models = {
      list: async () => {
        if (!current.sdkBody) throw new Error('sdk unavailable');
        return current.sdkBody;
      },
    };
  },
}));

jest.mock('@/lib/repositories/factory', () => ({
  __esModule: true,
  getRepositories: () => ({
    connections: {
      findAll: async () => current.profiles,
      findApiKeyByIdAndUserId: async (id: string) =>
        current.apiKeys[id] ? { key_value: current.apiKeys[id] } : null,
    },
  }),
}));

function installFetch() {
  // @ts-expect-error override global fetch
  global.fetch = jest.fn(async (url: string) => {
    const u = String(url);
    if (u.includes('openrouter.ai/api/v1/models')) {
      const outcome = current.publicScript[publicScriptIdx] ?? 'fail';
      publicScriptIdx += 1;
      if (outcome === 'fail') {
        return { ok: false, status: 500, json: async () => ({}) };
      }
      return { ok: true, status: 200, json: async () => current.publicBody };
    }
    if (u.endsWith('/api/tags')) {
      if (!current.ollamaBody) return { ok: false, status: 500, json: async () => ({}) };
      return { ok: true, status: 200, json: async () => current.ollamaBody };
    }
    return { ok: false, status: 404, json: async () => ({}) };
  });
}

const publicBody = {
  data: [
    {
      id: 'openai/gpt-4o',
      name: 'GPT-4o',
      pricing: { prompt: '0.0000025', completion: '0.00001' },
      context_length: 128000,
      architecture: { modality: 'text+image->text' },
      supported_parameters: ['tools', 'temperature'],
    },
    {
      id: 'openai/gpt-4o-mini',
      name: 'GPT-4o mini',
      pricing: { prompt: '0.00000015', completion: '0.0000006' },
      context_length: 128000,
      architecture: { modality: 'text->text' },
      supported_parameters: ['tools'],
    },
    {
      id: 'anthropic/claude-3-haiku',
      name: 'Claude 3 Haiku',
      pricing: { prompt: '0.00000025', completion: '0.00000125' },
      context_length: 200000,
      architecture: { modality: 'text+image->text' },
      supported_parameters: [],
    },
    {
      // A GOOGLE-slug model NOT in FALLBACK_PRICING, so the estimate cascade's
      // registry/fallback steps miss and it lands on 'openrouter-estimate'.
      id: 'google/gemini-3.0-ultra',
      name: 'Gemini 3.0 Ultra',
      pricing: { prompt: '0.000002', completion: '0.000008' },
      context_length: 1000000,
      architecture: { modality: 'text+image->text' },
      supported_parameters: ['tools'],
    },
  ],
};

const sdkBody = {
  data: [
    {
      id: 'openai/gpt-4o',
      name: 'GPT-4o',
      pricing: { prompt: '0.0000025', completion: '0.00001' },
      contextLength: 128000,
      architecture: { modality: 'text+image->text' },
      supportedParameters: ['tools'],
    },
    {
      id: 'meta/llama',
      name: 'Llama',
      pricing: { prompt: '0.0000001', completion: '0.0000001' },
      contextLength: 8192,
      supportedParameters: [],
    },
  ],
};

const scenarios: Scenario[] = [
  // A: parse + slug matching (single public fetch; multiple lookups)
  {
    id: 'parse-and-slug',
    publicBody,
    publicScript: ['ok'],
    sdkBody: null,
    ollamaBody: null,
    profiles: [],
    apiKeys: {},
    ops: [
      { kind: 'getORForModel', provider: 'OPENAI', modelId: 'gpt-4o', nowMs: 0 }, // exact
      { kind: 'getORForModel', provider: 'OPENAI', modelId: 'gpt-4o-2024-11-20', nowMs: 0 }, // fuzzy (query includes name)
      { kind: 'getORForModel', provider: 'ANTHROPIC', modelId: 'claude-3-haiku-20240307', nowMs: 0 }, // fuzzy (query includes 'claude-3-haiku')
      { kind: 'getORForModel', provider: 'OPENAI', modelId: 'nonexistent', nowMs: 0 }, // miss
      { kind: 'getORForModel', provider: 'OLLAMA', modelId: 'gpt-4o', nowMs: 0 }, // non-slug provider -> null
    ],
  },
  // B: TTL + negative cache windows
  {
    id: 'ttl-and-negative-cache',
    publicBody,
    publicScript: ['ok', 'fail', 'ok'],
    sdkBody: null,
    ollamaBody: null,
    profiles: [],
    apiKeys: {},
    ops: [
      { kind: 'getORForModel', provider: 'OPENAI', modelId: 'gpt-4o', nowMs: 0 }, // fetch#1 ok
      { kind: 'getORForModel', provider: 'OPENAI', modelId: 'gpt-4o', nowMs: 3600000 }, // 1h: cached
      { kind: 'getORForModel', provider: 'OPENAI', modelId: 'gpt-4o', nowMs: 90000000 }, // 25h: stale -> fetch#2 fail -> []
      { kind: 'getORForModel', provider: 'OPENAI', modelId: 'gpt-4o', nowMs: 90060000 }, // +1min: neg cache -> []
      { kind: 'getORForModel', provider: 'OPENAI', modelId: 'gpt-4o', nowMs: 90360000 }, // +6min: neg expired -> fetch#3 ok
    ],
  },
  // C: checkModelSupportsTools matrix (FALLBACK + OpenRouter cache)
  {
    id: 'check-tools-fallback',
    publicBody,
    publicScript: [],
    sdkBody: null,
    ollamaBody: null,
    profiles: [{ provider: 'ANTHROPIC', apiKeyId: null, baseUrl: null }],
    apiKeys: {},
    ops: [
      { kind: 'checkTools', provider: 'ANTHROPIC', modelName: 'claude-haiku-4-5-20251001', nowMs: 0 },
      { kind: 'checkTools', provider: 'ANTHROPIC', modelName: 'unknown-model', nowMs: 0 },
      { kind: 'checkTools', provider: 'OPENAI_COMPATIBLE', modelName: 'x', nowMs: 0 },
    ],
  },
  {
    id: 'check-tools-openrouter',
    publicBody,
    publicScript: [],
    sdkBody,
    ollamaBody: null,
    profiles: [{ provider: 'OPENROUTER', apiKeyId: 'k1', baseUrl: null }],
    apiKeys: { k1: 'sk-or-test' },
    ops: [
      { kind: 'checkTools', provider: 'OPENROUTER', modelName: 'openai/gpt-4o', nowMs: 0 }, // in cache, tools true
      { kind: 'checkTools', provider: 'OPENROUTER', modelName: 'meta/llama', nowMs: 0 }, // in cache, tools false
      { kind: 'checkTools', provider: 'OPENROUTER', modelName: 'not/there', nowMs: 0 }, // not in cache -> true
    ],
  },
  // D: estimateMessageCost cascade with source tags
  {
    id: 'estimate-cascade',
    publicBody,
    publicScript: ['ok'], // consumed by the openrouter-estimate op
    sdkBody,
    ollamaBody: null,
    profiles: [
      { provider: 'ANTHROPIC', apiKeyId: null, baseUrl: null },
      { provider: 'OPENROUTER', apiKeyId: 'k1', baseUrl: null },
    ],
    apiKeys: { k1: 'sk-or-test' },
    ops: [
      // OPENROUTER model in the SDK-fetched cache -> 'openrouter'
      { kind: 'estimate', provider: 'OPENROUTER', modelName: 'openai/gpt-4o', promptTokens: 1000, completionTokens: 500, nowMs: 0 },
      // ANTHROPIC known FALLBACK model -> registry (substring via getModelPricingFromRegistry)
      { kind: 'estimate', provider: 'ANTHROPIC', modelName: 'claude-haiku-4-5-20251001', promptTokens: 1000, completionTokens: 500, nowMs: 0 },
      // GOOGLE model not in FALLBACK but present in openrouter public (google slug) -> openrouter-estimate
      { kind: 'estimate', provider: 'GOOGLE', modelName: 'gemini-3.0-ultra', promptTokens: 100, completionTokens: 100, nowMs: 0 },
      // fully unknown -> unavailable
      { kind: 'estimate', provider: 'OPENAI_COMPATIBLE', modelName: 'mystery', promptTokens: 100, completionTokens: 100, nowMs: 0 },
    ],
  },
  // E: findCheapestAvailableModel filters (OpenRouter SDK cache + fallback)
  {
    id: 'find-cheapest',
    publicBody,
    publicScript: [],
    sdkBody,
    ollamaBody: null,
    profiles: [{ provider: 'OPENROUTER', apiKeyId: 'k1', baseUrl: null }],
    apiKeys: { k1: 'sk-or-test' },
    ops: [
      { kind: 'findCheapest', nowMs: 0 }, // cheapest overall
      { kind: 'findCheapest', nowMs: 0, requireTools: true }, // only tools-capable
      { kind: 'findCheapest', nowMs: 0, excludeProviders: ['OPENROUTER'] }, // exclude the only provider -> null
    ],
  },
];

async function run() {
  const rows: unknown[] = [];
  jest.useFakeTimers();

  for (const scenario of scenarios) {
    jest.resetModules();
    current = scenario;
    publicScriptIdx = 0;
    installFetch();

    const { getOpenRouterPricingForModel, findCheapestAvailableModel } = await import(
      '@/lib/llm/pricing-fetcher'
    );
    const { checkModelSupportsTools } = await import('@/lib/tools/pseudo-tool-support');
    const { estimateMessageCost } = await import('@/lib/services/cost-estimation.service');

    const outputs: unknown[] = [];
    for (const op of scenario.ops) {
      jest.setSystemTime(op.nowMs);
      if (op.kind === 'getORForModel') {
        outputs.push(await getOpenRouterPricingForModel(op.provider as never, op.modelId));
      } else if (op.kind === 'checkTools') {
        outputs.push(await checkModelSupportsTools(op.provider as never, op.modelName, 'user-1'));
      } else if (op.kind === 'estimate') {
        outputs.push(
          await estimateMessageCost(
            op.provider as never,
            op.modelName,
            op.promptTokens,
            op.completionTokens,
            'user-1'
          )
        );
      } else {
        outputs.push(
          await findCheapestAvailableModel('user-1', {
            requireVision: op.requireVision,
            requireTools: op.requireTools,
            excludeProviders: op.excludeProviders as never,
          })
        );
      }
    }

    rows.push({
      id: scenario.id,
      publicBody: scenario.publicBody,
      publicScript: scenario.publicScript,
      sdkBody: scenario.sdkBody,
      ollamaBody: scenario.ollamaBody,
      profiles: scenario.profiles,
      apiKeys: scenario.apiKeys,
      ops: scenario.ops,
      outputs,
    });
  }

  jest.useRealTimers();
  const out = process.env.QT_ORACLE_OUT;
  const text = rows.map((r) => JSON.stringify(r)).join('\n') + '\n';
  if (out) fs.writeFileSync(out, text);
  else process.stdout.write(text);
}

test('emit pricing-fetcher oracle', async () => {
  await run();
});
