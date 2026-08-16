/**
 * Oracle case #10 (Wave 1 / B2): context-budget arithmetic.
 *
 * Drives the REAL pure functions from v4's lib/llm/model-context-data.ts:
 * shouldSummarizeConversation, calculateRecentMessageCount, resolveMaxTokens,
 * calculateMaxAvailable, getRecommendedContextAllocation, getSafeInputLimit,
 * hasExtendedContext, and (since v4 `f933ba9c`, bug 70) resolveContextWindow +
 * computeSafeInputLimit. All already exported in v4 — no edit needed.
 *
 * The window-relative functions call getModelContextLimit(provider, model)
 * internally. That resolver also consults the plugin registry, so to stay
 * deterministic the corpus uses ONLY models present in MODEL_CONTEXT_OVERRIDES
 * (which return before any registry call) — plus one deliberately unrecognised
 * `hf.co/...` Ollama tag, which is the bug's own shape and still resolves
 * without the registry (this case file never initializes it, so the plugin and
 * registry-default stages are inert and the hardcoded provider default decides).
 * Each such row also emits the resolved limit so the Rust port — which injects
 * it at the boundary — uses the same value v4 computed.
 *
 * `hasExtendedContext` takes NO profile: v4's `f933ba9c` deliberately left it on
 * the bare lookup while routing the other three through resolveContextWindow.
 * Measured, not assumed — the hasExtended rows stay profile-free.
 *
 * NO Infinity row: `JSON.stringify(Infinity)` is `null`, so an Infinity
 * maxContext would reach the Rust side as "absent" and the row would assert the
 * opposite of what v4 did (`p4.6ay-units-1-3-zod-and-js`). A huge FINITE window
 * covers the same arm honestly.
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
  resolveContextWindow,
  computeSafeInputLimit,
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
  | { kind: 'allocation'; id: string; modelContextLimit: number; maxContext: number | null; out: { totalLimit: number; systemPrompt: number; memories: number; knowledge: number; conversationSummary: number; recentMessages: number; responseReserve: number; safetyMargin: number; safeInputLimit: number } }
  | { kind: 'safeInput'; id: string; modelContextLimit: number; maxContext: number | null; maxResponseTokens: number; out: number }
  | { kind: 'hasExtended'; id: string; totalLimit: number; out: boolean }
  | { kind: 'resolveWindow'; id: string; modelContextLimit: number; maxContext: number | null; profileAbsent: boolean; out: number }
  | { kind: 'computeSafe'; id: string; totalLimit: number; responseReserve: number; out: number };

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

// The bug-70 shape: a model name no table knows, on a provider whose hardcoded
// default is the conservative 8192. Without a profile the budget is 8192; with
// one it is whatever the user set.
const UNK_PROVIDER: Provider = 'OLLAMA';
const UNK_MODEL = 'hf.co/unsloth/Qwen3.8-27B-GGUF:UD-Q4_K_XL';

// resolveContextWindow(provider, model, profile) — the single source of truth
// (v4 `f933ba9c`). `profileAbsent` distinguishes "no profile argument at all"
// from "a profile carrying null", which JSON cannot: both ship maxContext null.
type WindowProfile = { maxContext?: number | null } | null | undefined;
const rcCases: Array<[string, Provider, string, WindowProfile]> = [
  ['profile-wins', UNK_PROVIDER, UNK_MODEL, { maxContext: 65536 }],
  ['profile-wins-override-model', 'OPENAI', 'gpt-4-32k', { maxContext: 65536 }],
  ['profile-below-lookup', 'ANTHROPIC', 'anthropic/claude-3-opus', { maxContext: 8000 }],
  ['zero-falls-through', UNK_PROVIDER, UNK_MODEL, { maxContext: 0 }],
  ['negative-falls-through', UNK_PROVIDER, UNK_MODEL, { maxContext: -1 }],
  ['null-falls-through', UNK_PROVIDER, UNK_MODEL, { maxContext: null }],
  ['empty-profile-falls-through', UNK_PROVIDER, UNK_MODEL, {}],
  ['null-profile-falls-through', UNK_PROVIDER, UNK_MODEL, null],
  ['absent-profile-falls-through', UNK_PROVIDER, UNK_MODEL, undefined],
  ['huge-finite', UNK_PROVIDER, UNK_MODEL, { maxContext: 10000000 }],
  ['one-token', UNK_PROVIDER, UNK_MODEL, { maxContext: 1 }],
];
for (const [id, provider, model, profile] of rcCases) {
  rows.push({
    kind: 'resolveWindow',
    id,
    modelContextLimit: getModelContextLimit(provider, model),
    maxContext: profile?.maxContext ?? null,
    profileAbsent: profile === undefined || profile === null,
    out: resolveContextWindow(provider, model, profile),
  });
}

// computeSafeInputLimit(totalLimit, responseReserve) — the one owner of the
// builder/validator formula.
const csCases: Array<[string, number, number]> = [
  ['v4-doc-case', 8192, 2048], // 8192 - 2048 - ceil(819.2) = 5324
  ['floor-1000', 4096, 8192], // negative -> 1000 floor
  ['big-window', 200000, 8192],
  ['ceil-boundary', 65536, 4096], // ceil(6553.6) = 6554
  ['zero-window', 0, 0], // 0 - 0 - 0 -> 1000 floor
];
for (const [id, total, reserve] of csCases) {
  rows.push({ kind: 'computeSafe', id, totalLimit: total, responseReserve: reserve, out: computeSafeInputLimit(total, reserve) });
}

// getRecommendedContextAllocation(provider, model, profile) — driven by override
// models, plus the profile arms the fix added.
for (const [provider, model] of OVERRIDE_MODELS) {
  const modelContextLimit = getModelContextLimit(provider, model);
  rows.push({ kind: 'allocation', id: `alloc-${modelContextLimit}`, modelContextLimit, maxContext: null, out: getRecommendedContextAllocation(provider, model) });
}
const allocProfileCases: Array<[string, Provider, string, number | null]> = [
  // The reported case: an unrecognised tag budgeted at 8192 vs the real 65536.
  ['alloc-unknown-bare', UNK_PROVIDER, UNK_MODEL, null],
  ['alloc-unknown-profile-65536', UNK_PROVIDER, UNK_MODEL, 65536],
  ['alloc-unknown-profile-zero', UNK_PROVIDER, UNK_MODEL, 0],
  // A profile SMALLER than the lookup still wins (this is what shrinks the
  // build-context corpus's compression ops from the 200k tier to the 8k tier).
  ['alloc-profile-shrinks', 'ANTHROPIC', 'anthropic/claude-3-opus', 8000],
  // Reserve-tier boundaries under a profile window.
  ['alloc-profile-32000', 'OPENAI', 'gpt-4-0613', 32000],
  ['alloc-profile-31999', 'OPENAI', 'gpt-4-0613', 31999],
  ['alloc-profile-200000', 'OPENAI', 'gpt-4-0613', 200000],
];
for (const [id, provider, model, maxContext] of allocProfileCases) {
  const profile = maxContext === null ? undefined : { maxContext };
  rows.push({
    kind: 'allocation',
    id,
    modelContextLimit: getModelContextLimit(provider, model),
    maxContext,
    out: getRecommendedContextAllocation(provider, model, profile),
  });
}

// getSafeInputLimit(provider, model, maxResponseTokens, profile)
const siCases: Array<[Provider, string, number, number | null]> = [
  ['ANTHROPIC', 'anthropic/claude-3-opus', 4096, null],
  ['OPENAI', 'gpt-4-32k', 4096, null],
  ['OPENAI', 'gpt-4-0613', 8192, null], // tiny window, large reserve -> 1000 floor
  ['OPENAI', 'openai/gpt-4-turbo', 4096, null],
  [UNK_PROVIDER, UNK_MODEL, 4096, null], // 8192 lookup, no profile
  [UNK_PROVIDER, UNK_MODEL, 4096, 65536], // the profile governs here too
  [UNK_PROVIDER, UNK_MODEL, 4096, 0], // degenerate -> falls back to the lookup
];
for (const [provider, model, maxResp, maxContext] of siCases) {
  const modelContextLimit = getModelContextLimit(provider, model);
  const profile = maxContext === null ? undefined : { maxContext };
  rows.push({
    kind: 'safeInput',
    id: `safe-${modelContextLimit}-${maxResp}-${maxContext ?? 'none'}`,
    modelContextLimit,
    maxContext,
    maxResponseTokens: maxResp,
    out: getSafeInputLimit(provider, model, maxResp, profile),
  });
}

// hasExtendedContext(provider, model) — includes the 32768 boundary (not > 32768).
for (const [provider, model] of OVERRIDE_MODELS) {
  const totalLimit = getModelContextLimit(provider, model);
  rows.push({ kind: 'hasExtended', id: `ext-${totalLimit}`, totalLimit, out: hasExtendedContext(provider, model) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
