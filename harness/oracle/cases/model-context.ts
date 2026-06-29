/**
 * Oracle case #30 (Wave 7 / B22): getModelContextLimit + consumers.
 *
 * Drives the REAL helpers:
 *   getModelContextLimit, hasExtendedContext, getSafeInputLimit
 *     (lib/llm/model-context-data.ts)
 *
 * Registry seam: the function reads plugin model-info, FALLBACK_PRICING, and the
 * registry default. We capture that injected context per provider ("providerctx"
 * rows) so the Rust port can be fed exactly what the real function saw, then the
 * "query" rows assert the three outputs. In a bare run no plugin exposes
 * getModelInfo (model-info is empty) and the registry default is 8192, so the
 * override tables, FALLBACK_PRICING, and the provider defaults are what decide.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/model-context.ts \
 *     > /tmp/oracle-model-context.ndjson
 */

import {
  getModelContextLimit,
  hasExtendedContext,
  getSafeInputLimit,
} from '@/lib/llm/model-context-data';
import { FALLBACK_PRICING } from '@/lib/llm/pricing';
import { getProvider, getDefaultContextWindow } from '@/lib/plugins/provider-registry';

const rows: unknown[] = [];

const PROVIDERS = [
  'ANTHROPIC',
  'OPENAI',
  'GOOGLE',
  'GROK',
  'OLLAMA',
  'OPENROUTER',
  'OPENAI_COMPATIBLE',
  'WEIRD_UNKNOWN', // exercises the final provider-default → 8192 fallback
];

for (const p of PROVIDERS) {
  const plugin: any = getProvider(p as any);
  const modelInfo =
    plugin && typeof plugin.getModelInfo === 'function'
      ? plugin.getModelInfo().map((m: any) => ({ id: m.id, contextWindow: m.contextWindow ?? null }))
      : [];
  const fp = ((FALLBACK_PRICING as any)[p] || []).map((m: any) => ({
    modelId: m.modelId,
    contextLength: m.contextLength ?? null,
  }));
  rows.push({
    kind: 'providerctx',
    provider: p,
    modelInfo,
    fallbackPricing: fp,
    registryDefault: getDefaultContextWindow(p as any),
  });
}

// (provider, model, maxResponseTokens) — covering every lookup branch.
const queries: Array<[string, string, number]> = [
  // exact overrides
  ['OPENAI', 'gpt-4-32k', 4096],
  ['OLLAMA', 'phi3:mini', 4096],
  ['OLLAMA', 'mistral:7b', 4096],
  ['OPENAI', 'gpt-3.5-turbo-16k', 4096],
  ['OPENAI', 'gpt-4-0613', 4096],
  ['OLLAMA', 'llama3.2:3b', 2048],
  // provider-prefixed overrides
  ['ANTHROPIC', 'claude-3-opus', 4096],
  ['OPENAI', 'gpt-4', 4096],
  ['GOOGLE', 'gemini-pro', 8192],
  // FALLBACK_PRICING hits (exact + substring)
  ['ANTHROPIC', 'claude-sonnet-4-5-20250929', 4096],
  ['OPENAI', 'gpt-4o', 4096],
  ['OPENAI', 'gpt-4o-mini', 4096],
  ['OPENAI', 'gpt-4o-2024-11-20', 4096],
  ['GROK', 'grok-2', 4096],
  // provider-default fallbacks (unknown model)
  ['ANTHROPIC', 'totally-unknown-xyz', 4096],
  ['OPENAI', 'totally-unknown-xyz', 4096],
  ['GOOGLE', 'totally-unknown-xyz', 4096],
  ['OLLAMA', 'totally-unknown-xyz', 4096],
  ['OPENROUTER', 'totally-unknown-xyz', 4096],
  ['OPENAI_COMPATIBLE', 'totally-unknown-xyz', 4096],
  ['WEIRD_UNKNOWN', 'totally-unknown-xyz', 4096],
  // safe-input floor (huge response reserve clamps to 1000)
  ['OLLAMA', 'phi3:mini', 1000000],
  ['ANTHROPIC', 'claude-sonnet-4-5-20250929', 8192],
];
for (const [provider, model, maxResp] of queries) {
  rows.push({
    kind: 'query',
    provider,
    model,
    maxResponseTokens: maxResp,
    limit: getModelContextLimit(provider as any, model),
    extended: hasExtendedContext(provider as any, model),
    safeInput: getSafeInputLimit(provider as any, model, maxResp),
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
