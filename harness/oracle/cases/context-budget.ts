/**
 * Oracle case #10 (Wave 1 / B2): context-budget arithmetic.
 *
 * Drives the REAL pure functions from v4's lib/llm/model-context-data.ts:
 * shouldSummarizeConversation, calculateRecentMessageCount, resolveMaxTokens,
 * calculateMaxAvailable, getRecommendedContextAllocation, getSafeInputLimit,
 * hasExtendedContext. All already exported in v4 — no edit needed.
 *
 * The four window-relative functions call getModelContextLimit(provider, model)
 * internally. That resolver also consults the plugin registry, so to stay
 * deterministic the corpus uses ONLY models present in MODEL_CONTEXT_OVERRIDES
 * (which return before any registry call). Each such row also emits the resolved
 * limit so the Rust port — which injects it at the boundary — uses the same
 * value v4 computed.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/context-budget.ts \
 *     > /tmp/oracle-context-budget.ndjson
 */

import {
  shouldSummarizeConversation,
  calculateRecentMessageCount,
  resolveMaxTokens,
  calculateMaxAvailable,
  getRecommendedContextAllocation,
  getSafeInputLimit,
  hasExtendedContext,
  getModelContextLimit,
} from '@/lib/llm/model-context-data';
import type { Provider } from '@/lib/schemas/types';

// Models that resolve via MODEL_CONTEXT_OVERRIDES (deterministic, no registry):
//   anthropic/claude-3-opus -> 200000, openai/gpt-4-turbo -> 128000,
//   gpt-4-32k -> 32768, gpt-3.5-turbo-16k -> 16385, gpt-4-0613 -> 8192.
const OVERRIDE_MODELS: Array<[Provider, string]> = [
  ['ANTHROPIC', 'anthropic/claude-3-opus'],
  ['OPENAI', 'openai/gpt-4-turbo'],
  ['OPENAI', 'gpt-4-32k'],
  ['OPENAI', 'gpt-3.5-turbo-16k'],
  ['OPENAI', 'gpt-4-0613'],
];

type Row =
  | { kind: 'summarize'; id: string; messageCount: number; estimatedTokens: number; contextLimit: number; out: boolean }
  | { kind: 'recentCount'; id: string; availableTokens: number; averageMessageTokens: number; out: number }
  | { kind: 'resolveTokens'; id: string; maxTokens: number | null; modelClass: string | null; out: number }
  | { kind: 'maxAvailable'; id: string; modelContextLimit: number; maxContext: number | null; maxTokens: number | null; modelClass: string | null; out: { maxAvailable: number; maxContext: number; maxTokens: number } }
  | { kind: 'allocation'; id: string; totalLimit: number; out: { totalLimit: number; systemPrompt: number; memories: number; knowledge: number; conversationSummary: number; recentMessages: number; responseReserve: number } }
  | { kind: 'safeInput'; id: string; totalLimit: number; maxResponseTokens: number; out: number }
  | { kind: 'hasExtended'; id: string; totalLimit: number; out: boolean };

const rows: Row[] = [];

// shouldSummarizeConversation(messageCount, estimatedTokens, contextLimit)
const sumCases: Array<[string, number, number, number]> = [
  ['usage-over', 10, 70000, 100000], // 70% > 60 -> true
  ['usage-boundary', 10, 60000, 100000], // exactly 60% (not > 60) -> false
  ['messages-over', 25, 1000, 100000], // 25 > 20 -> true
  ['messages-boundary', 20, 1000, 100000], // 20 not > 20 -> false
  ['neither', 10, 1000, 100000], // false
];
for (const [id, m, e, c] of sumCases) {
  rows.push({ kind: 'summarize', id, messageCount: m, estimatedTokens: e, contextLimit: c, out: shouldSummarizeConversation(m, e, c) });
}

// calculateRecentMessageCount(availableTokens, averageMessageTokens)
const recCases: Array<[string, number, number]> = [
  ['cap-100', 15000, 150], // 100 -> 100
  ['mid-floor', 1000, 150], // floor(6.67)=6
  ['floor-to-min', 300, 150], // 2 -> clamp 4
  ['explicit-avg', 1000, 200], // 5 -> clamp... 5
  ['negative', -100, 150], // floor(-0.67)=-1 -> clamp 4
];
for (const [id, a, avg] of recCases) {
  rows.push({ kind: 'recentCount', id, availableTokens: a, averageMessageTokens: avg, out: calculateRecentMessageCount(a, avg) });
}

// resolveMaxTokens({ maxTokens, modelClass })
const resCases: Array<[string, number | null, string | null]> = [
  ['explicit', 5000, null], // 5000
  ['class-standard', null, 'Standard'], // 16000
  ['class-unknown', null, 'NoSuchClass'], // default 8000
  ['no-class', null, null], // 8000
  ['zero-maxtokens', 0, null], // 0 not >0 -> default 8000
  ['neg-maxtokens-class', -5, 'Deep'], // -5 falls through -> Deep maxOutput 128000
];
for (const [id, mt, mc] of resCases) {
  rows.push({ kind: 'resolveTokens', id, maxTokens: mt, modelClass: mc, out: resolveMaxTokens({ maxTokens: mt, modelClass: mc }) });
}

// calculateMaxAvailable(provider, model, profile)
type MaxAvailProfile = { maxContext?: number | null; maxTokens?: number | null; modelClass?: string | null };
const maCases: Array<[string, Provider, string, MaxAvailProfile]> = [
  ['ctx-override', 'ANTHROPIC', 'anthropic/claude-3-opus', { maxContext: 200000, maxTokens: 8000 }],
  ['from-model-32k', 'OPENAI', 'gpt-4-32k', { maxContext: null, maxTokens: null, modelClass: null }],
  ['class-deep-on-small', 'OPENAI', 'gpt-4-0613', { maxContext: null, maxTokens: null, modelClass: 'Deep' }],
  ['min-floor', 'OPENAI', 'gpt-4-0613', { maxContext: 1000, maxTokens: 100 }],
];
for (const [id, provider, model, profile] of maCases) {
  rows.push({
    kind: 'maxAvailable',
    id,
    modelContextLimit: getModelContextLimit(provider, model),
    maxContext: profile.maxContext ?? null,
    maxTokens: profile.maxTokens ?? null,
    modelClass: profile.modelClass ?? null,
    out: calculateMaxAvailable(provider, model, profile),
  });
}

// getRecommendedContextAllocation(provider, model) — driven by override models.
for (const [provider, model] of OVERRIDE_MODELS) {
  const totalLimit = getModelContextLimit(provider, model);
  rows.push({ kind: 'allocation', id: `alloc-${totalLimit}`, totalLimit, out: getRecommendedContextAllocation(provider, model) });
}

// getSafeInputLimit(provider, model, maxResponseTokens)
const siCases: Array<[Provider, string, number]> = [
  ['ANTHROPIC', 'anthropic/claude-3-opus', 4096],
  ['OPENAI', 'gpt-4-32k', 4096],
  ['OPENAI', 'gpt-4-0613', 8192], // tiny window, large reserve -> 1000 floor
  ['OPENAI', 'openai/gpt-4-turbo', 4096],
];
for (const [provider, model, maxResp] of siCases) {
  const totalLimit = getModelContextLimit(provider, model);
  rows.push({ kind: 'safeInput', id: `safe-${totalLimit}-${maxResp}`, totalLimit, maxResponseTokens: maxResp, out: getSafeInputLimit(provider, model, maxResp) });
}

// hasExtendedContext(provider, model) — includes the 32768 boundary (not > 32768).
for (const [provider, model] of OVERRIDE_MODELS) {
  const totalLimit = getModelContextLimit(provider, model);
  rows.push({ kind: 'hasExtended', id: `ext-${totalLimit}`, totalLimit, out: hasExtendedContext(provider, model) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
