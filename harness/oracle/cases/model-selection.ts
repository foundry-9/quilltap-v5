/**
 * Oracle case #9 (Wave 1 / B1): cost-aware model selection + model classes.
 *
 * Drives the REAL pure functions from v4's lib/llm/pricing.ts (the selection
 * siblings of the already-ported estimateCost) and lib/llm/model-classes.ts.
 * All already exported in v4 — no edit needed.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/model-selection.ts \
 *     > /tmp/oracle-model-selection.ndjson
 */

import {
  getAverageCostPer1M,
  sortByCost,
  findCheapestModel,
  getModelsUnderCost,
  calculateCostTier,
  calculateSavings,
  type ModelPricing,
} from '@/lib/llm/pricing';
import {
  getModelClass,
  isValidModelClassName,
} from '@/lib/llm/model-classes';

// The selection helpers read only id, the two rates, contextLength, and the two
// capability flags — build a partial cast through `unknown`.
const m = (
  modelId: string,
  promptCostPer1M: number,
  completionCostPer1M: number,
  contextLength: number | null,
  supportsVision = false,
  supportsTools = false,
): ModelPricing =>
  ({ modelId, promptCostPer1M, completionCostPer1M, contextLength, supportsVision, supportsTools } as unknown as ModelPricing);

type WireModel = {
  modelId: string;
  promptCostPer1M: number;
  completionCostPer1M: number;
  contextLength: number | null;
  supportsVision: boolean;
  supportsTools: boolean;
};
const wire = (x: ModelPricing): WireModel => {
  const o = x as unknown as WireModel;
  return {
    modelId: o.modelId,
    promptCostPer1M: o.promptCostPer1M,
    completionCostPer1M: o.completionCostPer1M,
    contextLength: o.contextLength,
    supportsVision: o.supportsVision,
    supportsTools: o.supportsTools,
  };
};

type Row =
  | { kind: 'avg'; id: string; model: WireModel; out: number }
  | { kind: 'tier'; id: string; model: WireModel; out: number }
  | { kind: 'savings'; id: string; expensive: WireModel; cheaper: WireModel; out: number }
  | { kind: 'sort'; id: string; models: WireModel[]; out: string[] }
  | { kind: 'underCost'; id: string; models: WireModel[]; max: number; out: string[] }
  | { kind: 'cheapest'; id: string; models: WireModel[]; opts: { requireVision?: boolean; requireTools?: boolean; minContextLength?: number }; out: string | null }
  | { kind: 'modelClass'; id: string; name: string; out: { name: string; tier: string; maxContext: number; maxOutput: number; tags: string[]; quality: number } | null }
  | { kind: 'validName'; id: string; name: string; out: boolean };

const rows: Row[] = [];

// Shared fixture set for sort / underCost / cheapest. Averages:
//   a=9, b=0.75, c=0, d=45, e=0.75  (b and e tie → stability check).
const a = m('a', 3, 15, 200000, true, true);
const b = m('b', 0.25, 1.25, 128000, false, true);
const c = m('c', 0, 0, 8000, false, false);
const d = m('d', 15, 75, 200000, true, true);
const e = m('e', 0.25, 1.25, 64000, true, false);
const set = [a, b, c, d, e];
const setWire = set.map(wire);

// avg
for (const [id, model] of [['avg-a', a], ['avg-c', c], ['avg-b', b]] as Array<[string, ModelPricing]>) {
  rows.push({ kind: 'avg', id, model: wire(model), out: getAverageCostPer1M(model) });
}

// tier — models whose avg equals the probe value (prompt=completion=X → avg X).
for (const [id, x] of [['tier-0', 0], ['tier-0.3', 0.3], ['tier-1', 1], ['tier-5', 5], ['tier-30', 30], ['tier-100', 100]] as Array<[string, number]>) {
  const model = m(`t${x}`, x, x, 100000);
  rows.push({ kind: 'tier', id, model: wire(model), out: calculateCostTier(model) });
}

// savings
const sav: Array<[string, ModelPricing, ModelPricing]> = [
  ['savings-80', m('x', 10, 10, null), m('y', 2, 2, null)], // ((10-2)/10)*100 = 80
  ['savings-free-expensive', m('x', 0, 0, null), m('y', 2, 2, null)], // expensive free → 0
  ['savings-equal', m('x', 5, 5, null), m('y', 5, 5, null)], // 0
];
for (const [id, ex, ch] of sav) {
  rows.push({ kind: 'savings', id, expensive: wire(ex), cheaper: wire(ch), out: calculateSavings(ex, ch) });
}

// sort — stable; expected order [c, b, e, a, d] (b before e on the 0.75 tie).
rows.push({ kind: 'sort', id: 'sort-all', models: setWire, out: sortByCost(set).map(x => (x as unknown as WireModel).modelId) });

// underCost — input order preserved (filter, no sort): avg<=1.0 → b, c, e.
rows.push({ kind: 'underCost', id: 'under-1', models: setWire, max: 1.0, out: getModelsUnderCost(set, 1.0).map(x => (x as unknown as WireModel).modelId) });
rows.push({ kind: 'underCost', id: 'under-0', models: setWire, max: 0, out: getModelsUnderCost(set, 0).map(x => (x as unknown as WireModel).modelId) }); // only c

// cheapest
const cheapestCases: Array<[string, ModelPricing[], { requireVision?: boolean; requireTools?: boolean; minContextLength?: number }]> = [
  ['cheap-none', set, {}], // c
  ['cheap-vision', set, { requireVision: true }], // a,d,e → e
  ['cheap-tools', set, { requireTools: true }], // a,b,d → b
  ['cheap-vision-tools', set, { requireVision: true, requireTools: true }], // a,d → a
  ['cheap-minctx', set, { minContextLength: 150000 }], // a,d → a
  ['cheap-minctx-zero', set, { minContextLength: 0 }], // 0 falsy → no filter → c
  ['cheap-none-match', set, { requireVision: true, minContextLength: 2000000 }], // none → null
  ['cheap-empty', [], {}], // null
];
for (const [id, models, opts] of cheapestCases) {
  const chosen = findCheapestModel(models, opts);
  rows.push({ kind: 'cheapest', id, models: models.map(wire), opts, out: chosen ? (chosen as unknown as WireModel).modelId : null });
}

// modelClass
for (const [id, name] of [['mc-compact', 'Compact'], ['mc-deep', 'Deep'], ['mc-missing', 'Nonexistent']] as Array<[string, string]>) {
  const mc = getModelClass(name);
  rows.push({
    kind: 'modelClass',
    id,
    name,
    out: mc ? { name: mc.name, tier: mc.tier, maxContext: mc.maxContext, maxOutput: mc.maxOutput, tags: [...mc.tags], quality: mc.quality } : null,
  });
}

// validName
for (const [id, name] of [['valid-standard', 'Standard'], ['valid-lowercase', 'compact'], ['valid-empty', '']] as Array<[string, string]>) {
  rows.push({ kind: 'validName', id, name, out: isValidModelClassName(name) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
