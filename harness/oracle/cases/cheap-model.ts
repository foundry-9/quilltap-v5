/**
 * Oracle case #29 (Wave 7 / B21): cheap-model classifiers.
 *
 * Drives the REAL helpers:
 *   isCheapModel, estimateModelCost, getCheapestModel (lib/llm/cheap-llm.ts)
 *
 * In a bare run the plugin registry returns no cheap config
 * (getCheapModelConfig → null), so every call takes the hardcoded fallback-table
 * path the Rust port reproduces (registry list empty / default None).
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/cheap-model.ts \
 *     > /tmp/oracle-cheap-model.ndjson
 */

import { isCheapModel, estimateModelCost, getCheapestModel } from '@/lib/llm/cheap-llm';
import type { Provider } from '@/lib/schemas/types';

const rows: unknown[] = [];

const PROVIDERS: Provider[] = [
  'ANTHROPIC',
  'OPENAI',
  'GOOGLE',
  'GROK',
  'OLLAMA',
  'OPENROUTER',
  'OPENAI_COMPATIBLE',
];

// Model-name shapes exercising every branch: exact-recommended hits, the
// expensive vetoes (opus/o1/o3/ultra/pro), the "4o w/o mini" + sonnet mid-tier
// vetoes, each cheap indicator, dashed-vs-undashed o1/o3, and unknowns.
const MODELS = [
  'claude-haiku-4-5-20251001',
  'claude-3-haiku-20240307',
  'gpt-4o-mini',
  'gpt-3.5-turbo',
  'gemini-2.0-flash',
  'gemini-1.5-flash',
  'grok-2-mini',
  'llama3.2:3b',
  'phi3:mini',
  'claude-opus-4-1',
  'claude-3-5-sonnet',
  'gpt-4o',
  'o1-preview',
  'o1mini',
  'o3-mini',
  'o3mini',
  'gemini-1.5-pro',
  'gemini-2.0-pro',
  'grok-2-ultra',
  'mixtral:8x7b',
  'gemma2:2b',
  'mistral:7b-instant',
  'some-tiny-model',
  'a-small-thing',
  'turbo-v2',
  'flash-lite',
  'totally-unknown-xyz',
  'GPT-4O-MINI',
];

for (const provider of PROVIDERS) {
  for (const model of MODELS) {
    rows.push({
      kind: 'classify',
      provider,
      model,
      cheap: isCheapModel(provider, model),
      cost: estimateModelCost(provider, model),
    });
  }
  rows.push({ kind: 'cheapest', provider, out: getCheapestModel(provider) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
