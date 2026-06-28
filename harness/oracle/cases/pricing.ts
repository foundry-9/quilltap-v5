/**
 * Oracle case #8: LLM cost arithmetic (estimateCost).
 *
 * Drives the REAL pure function from the v4 server's lib/llm/pricing.ts —
 * estimateCost(pricing, promptTokens, completionTokens), the USD cost of a
 * completion given per-1M-token rates. Already exported in v4; no edit needed.
 *
 * estimateCost reads only `promptCostPer1M` / `completionCostPer1M` off the
 * ModelPricing object, so we build a partial cast through `unknown` — faithful
 * at runtime.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/pricing.ts \
 *     > /tmp/oracle-pricing.ndjson
 */

import { estimateCost, type ModelPricing } from '@/lib/llm/pricing';

const pricing = (promptCostPer1M: number, completionCostPer1M: number): ModelPricing =>
  ({ promptCostPer1M, completionCostPer1M } as unknown as ModelPricing);

type Row = {
  kind: 'estimate';
  id: string;
  promptCostPer1M: number;
  completionCostPer1M: number;
  promptTokens: number;
  completionTokens: number;
  out: number;
};

const rows: Row[] = [];

// [id, promptCostPer1M, completionCostPer1M, promptTokens, completionTokens]
const cases: Array<[string, number, number, number, number]> = [
  ['both-zero-tokens', 3.0, 15.0, 0, 0], // 0
  ['free-model', 0, 0, 1500, 500], // 0 — local/Ollama
  ['haiku-typical', 0.25, 1.25, 1500, 500], // small sub-cent
  ['opus-typical', 15.0, 75.0, 2000, 800], // dollars
  ['prompt-only', 3.0, 15.0, 10_000, 0], // completion bucket = 0
  ['completion-only', 3.0, 15.0, 0, 10_000], // prompt bucket = 0
  ['asymmetric-rates', 0.8, 4.0, 1234, 5678], // independent buckets
  ['large-context', 5.0, 25.0, 1_000_000, 250_000], // exactly 1M prompt → 5.0 + ...
  ['repeating-decimal-rate', 1.23456789, 9.87654321, 333, 777], // float fidelity
  ['single-token-each', 3.0, 15.0, 1, 1], // tiny: 3e-6 + 15e-6
];

for (const [id, promptCostPer1M, completionCostPer1M, promptTokens, completionTokens] of cases) {
  rows.push({
    kind: 'estimate',
    id,
    promptCostPer1M,
    completionCostPer1M,
    promptTokens,
    completionTokens,
    out: estimateCost(pricing(promptCostPer1M, completionCostPer1M), promptTokens, completionTokens),
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
