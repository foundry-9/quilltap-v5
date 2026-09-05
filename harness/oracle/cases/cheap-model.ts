/**
 * Oracle case #29 (Wave 7 / B21): cheap-model classifiers.
 *
 * Drives the REAL helper:
 *   getCheapestModel (lib/llm/cheap-llm.ts)
 *
 * P4.D157 (v4 d4138b96b, the 4.9 dead-code sweep) DELETED isCheapModel and
 * estimateModelCost; the `classify` rows they produced left this case with the
 * v5 twins (neither had a production caller on either side — v5's
 * `is_cheap_model` was reached only from `estimate_model_cost`, which nothing
 * called). `getCheapestModel` survives and stays live in v5 (`cheap_llm.rs`),
 * so the family survives on its `cheapest` rows. Do not re-add the deleted
 * names: a named import of a deleted export makes this whole case fail to LINK,
 * emitting a ZERO-byte NDJSON.
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

import { getCheapestModel } from '@/lib/llm/cheap-llm';
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

for (const provider of PROVIDERS) {
  rows.push({ kind: 'cheapest', provider, out: getCheapestModel(provider) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
