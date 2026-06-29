/**
 * Tier-1 oracle case — the vault wardrobe-component pure leaves.
 *
 * Drives three REAL v4 pure functions:
 *   - lib/database/repositories/vault-overlay/parsers.ts:
 *       parseComponentItemsField, parseWardrobeTypesField
 *   - lib/wardrobe/expand-composites.ts: detectComponentCycles
 *
 * One NDJSON row per case; the Rust port
 * (quilltap_core::vault_overlay::{parse_component_items_field,
 * parse_wardrobe_types_field, detect_component_cycles}) must match exactly.
 *
 * Run from the v4 server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-component-leaves.ts \
 *     > /tmp/oracle-vault-component-leaves.ndjson
 */

import {
  parseComponentItemsField,
  parseWardrobeTypesField,
} from '@/lib/database/repositories/vault-overlay/parsers';
import { detectComponentCycles } from '@/lib/wardrobe/expand-composites';
import type { WardrobeItem } from '@/lib/schemas/wardrobe.types';

type Row =
  | { kind: 'componentItems'; id: string; raw: unknown; out: string[] }
  | { kind: 'wardrobeTypes'; id: string; raw: unknown; out: string[] | null }
  | {
      kind: 'cycles';
      id: string;
      selfId: string;
      componentItemIds: string[];
      graph: Record<string, string[]>;
      out: string[][];
    };

const rows: Row[] = [];

// ── parseComponentItemsField ───────────────────────────────────────────────
const componentInputs: Array<{ id: string; raw: unknown }> = [
  { id: 'ci-null', raw: null },
  { id: 'ci-nonarray-str', raw: 'abc' },
  { id: 'ci-nonarray-num', raw: 5 },
  { id: 'ci-nonarray-obj', raw: { a: 1 } },
  { id: 'ci-empty', raw: [] },
  { id: 'ci-mixed', raw: ['a', ' b ', '', '   ', 3, null, 'c'] },
  { id: 'ci-trim-only', raw: ['  x  '] },
];
for (const { id, raw } of componentInputs) {
  rows.push({ kind: 'componentItems', id, raw, out: parseComponentItemsField(raw) });
}

// ── parseWardrobeTypesField ────────────────────────────────────────────────
const typeInputs: Array<{ id: string; raw: unknown }> = [
  { id: 'wt-empty', raw: [] },
  { id: 'wt-nonarray', raw: 'top' },
  { id: 'wt-valid', raw: ['top', 'bottom'] },
  { id: 'wt-dedup', raw: ['top', 'top', 'footwear'] },
  { id: 'wt-all', raw: ['top', 'bottom', 'footwear', 'accessories'] },
  { id: 'wt-unknown', raw: ['top', 'hat'] },
  { id: 'wt-nonstring', raw: ['top', 5] },
  { id: 'wt-single', raw: ['accessories'] },
];
for (const { id, raw } of typeInputs) {
  rows.push({ kind: 'wardrobeTypes', id, raw, out: parseWardrobeTypesField(raw) });
}

// ── detectComponentCycles ──────────────────────────────────────────────────
function runCycles(
  selfId: string,
  componentItemIds: string[],
  graph: Record<string, string[]>,
): string[][] {
  const itemsById = new Map<string, WardrobeItem>();
  for (const [id, componentIds] of Object.entries(graph)) {
    itemsById.set(id, { id, componentItemIds: componentIds } as unknown as WardrobeItem);
  }
  return detectComponentCycles(selfId, componentItemIds, itemsById);
}

const cycleInputs: Array<{
  id: string;
  selfId: string;
  componentItemIds: string[];
  graph: Record<string, string[]>;
}> = [
  { id: 'cy-self', selfId: 'a', componentItemIds: ['a'], graph: {} },
  { id: 'cy-none', selfId: 'a', componentItemIds: ['b'], graph: { b: [] } },
  { id: 'cy-indirect', selfId: 'a', componentItemIds: ['b'], graph: { b: ['a'] } },
  { id: 'cy-subcycle', selfId: 'a', componentItemIds: ['b'], graph: { b: ['c'], c: ['b'] } },
  {
    id: 'cy-diamond-ok',
    selfId: 'a',
    componentItemIds: ['b', 'c'],
    graph: { b: ['d'], c: ['d'], d: [] },
  },
  {
    id: 'cy-unknown-ref',
    selfId: 'a',
    componentItemIds: ['ghost'],
    graph: { b: ['a'] },
  },
  {
    id: 'cy-deep',
    selfId: 'a',
    componentItemIds: ['b'],
    graph: { b: ['c'], c: ['d'], d: ['a'] },
  },
];
for (const c of cycleInputs) {
  rows.push({
    kind: 'cycles',
    id: c.id,
    selfId: c.selfId,
    componentItemIds: c.componentItemIds,
    graph: c.graph,
    out: runCycles(c.selfId, c.componentItemIds, c.graph),
  });
}

for (const row of rows) {
  process.stdout.write(JSON.stringify(row) + '\n');
}
