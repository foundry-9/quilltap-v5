/**
 * Oracle case (P4.D343): declared tool effects — v4 `b15fe9404` at the pin
 * `236c04e35`.
 *
 * Drives the REAL code:
 *   BUILT_IN_TOOL_EFFECTS, effectOfToolCall, maxEffectOfTool,
 *   highestToolEffect, describeCeilingRefusal          (lib/tools/effects.ts)
 *   isWithinCeiling, lowerToolEffect, maxToolEffect,
 *   isComputedToolEffect, TOOL_EFFECTS                 (lib/tools/effect-types.ts)
 *   the four computed specs' usesAllowedUnder          (state / edit_custom_tool /
 *                                                       doc_open_document / run_custom)
 *   customToolEffect                                   (lib/pascal/tool-effect.ts)
 *   QtapCustomToolSchema                               (lib/pascal/custom-tool.types.ts)
 *
 * One NDJSON row per probe: `{kind, id, ...inputs, out}`. The table row dumps
 * every key sorted with `{computed: maxEffect}` for a computed spec, plus the
 * count. Seeds: v4 `lib/tools/__tests__/effects.test.ts` (4 `it`s).
 *
 * Run from inside the pin (Node 24 `~/.nvm/versions/node/v24.13.1/bin`):
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   npx tsx "$V5W/harness/oracle/cases/tool-effects.ts" \
 *     > /tmp/oracle-tool-effects.ndjson
 */

import {
  BUILT_IN_TOOL_EFFECTS,
  effectOfToolCall,
  maxEffectOfTool,
  highestToolEffect,
  describeCeilingRefusal,
} from '@/lib/tools/effects';
import {
  isComputedToolEffect,
  isWithinCeiling,
  lowerToolEffect,
  maxToolEffect,
  TOOL_EFFECTS,
  type ToolEffect,
} from '@/lib/tools/effect-types';
import * as effectsModule from '@/lib/tools/effects';
import { customToolEffect } from '@/lib/pascal/tool-effect';
import { QtapCustomToolSchema } from '@/lib/pascal/custom-tool.types';

const out = (row: Record<string, unknown>) => process.stdout.write(JSON.stringify(row) + '\n');

// --- the table ---------------------------------------------------------------
const names = Object.keys(BUILT_IN_TOOL_EFFECTS).sort();
const table: Record<string, unknown> = {};
for (const name of names) {
  const spec = BUILT_IN_TOOL_EFFECTS[name];
  table[name] = isComputedToolEffect(spec) ? { computed: spec.maxEffect } : spec;
}
out({ kind: 'table', id: 'table', count: names.length, out: table });
// `8becfc6ce` deleted `isComputedTool`; v5 writes no twin.
out({ kind: 'noIsComputedTool', id: 'no-isComputedTool', out: 'isComputedTool' in effectsModule });

// --- effectOfToolCall ----------------------------------------------------------
type Ctx = { customToolEffects?: [string, ToolEffect][]; pluginToolEffects?: [string, ToolEffect][] };
const calls: { id: string; name: string; input: unknown; ctx?: Ctx }[] = [
  { id: 'static-read', name: 'doc_read_file', input: {} },
  { id: 'static-destroy', name: 'doc_delete_file', input: {} },
  { id: 'static-none', name: 'rng', input: { sides: 6 } },
  { id: 'state-fetch', name: 'state', input: { operation: 'fetch' } },
  { id: 'state-set', name: 'state', input: { operation: 'set', path: 'a', value: 1 } },
  { id: 'state-delete', name: 'state', input: { operation: 'delete', path: 'a' } },
  { id: 'state-bad-op', name: 'state', input: { operation: 'peek' } },
  { id: 'state-bad-context', name: 'state', input: { operation: 'fetch', context: 'galaxy' } },
  { id: 'state-missing', name: 'state', input: {} },
  { id: 'state-null', name: 'state', input: null },
  { id: 'state-array', name: 'state', input: [] },
  { id: 'state-path-number', name: 'state', input: { operation: 'fetch', path: 3 } },
  { id: 'ect-schema', name: 'edit_custom_tool', input: { action: 'schema' } },
  { id: 'ect-validate', name: 'edit_custom_tool', input: { mount_point: 'Lore', file: 'coin' } },
  { id: 'ect-save', name: 'edit_custom_tool', input: { mount_point: 'Lore', file: 'coin', content: '{}' } },
  { id: 'ect-save-null', name: 'edit_custom_tool', input: { mount_point: 'Lore', file: 'coin', content: null } },
  { id: 'ect-schema-with-content', name: 'edit_custom_tool', input: { action: 'schema', content: {} } },
  { id: 'ect-refine-fail', name: 'edit_custom_tool', input: { mount_point: 'Lore' } },
  { id: 'ect-empty-file', name: 'edit_custom_tool', input: { mount_point: 'Lore', file: '' } },
  { id: 'ect-bad-action', name: 'edit_custom_tool', input: { action: 'save', mount_point: 'a', file: 'b' } },
  { id: 'ect-missing', name: 'edit_custom_tool', input: undefined },
  { id: 'dod-path', name: 'doc_open_document', input: { path: 'Notes/a.md' } },
  { id: 'dod-uri', name: 'doc_open_document', input: { uri: 'qtap://self/Notes/a.md' } },
  { id: 'dod-blank', name: 'doc_open_document', input: { title: 'Fresh' } },
  { id: 'dod-empty-path', name: 'doc_open_document', input: { path: '' } },
  { id: 'dod-empty', name: 'doc_open_document', input: {} },
  { id: 'dod-missing', name: 'doc_open_document', input: undefined },
  { id: 'dod-bad-scope', name: 'doc_open_document', input: { path: 'a.md', scope: 'galaxy' } },
  { id: 'dod-bad-mode', name: 'doc_open_document', input: { path: 'a.md', mode: 'wide' } },
  { id: 'dod-null-path', name: 'doc_open_document', input: { path: null } },
  { id: 'dod-scope-store', name: 'doc_open_document', input: { path: 'a.md', scope: 'document_store', mount_point: 'Lore', mode: 'focus' } },
  { id: 'rc-hit-read', name: 'run_custom', input: { tool: 'coin' }, ctx: { customToolEffects: [['coin', 'read'], ['ledger', 'write']] } },
  { id: 'rc-hit-write', name: 'run_custom', input: { tool: 'ledger', parameters: { amount: 2 } }, ctx: { customToolEffects: [['coin', 'read'], ['ledger', 'write']] } },
  { id: 'rc-miss', name: 'run_custom', input: { tool: 'unknown' }, ctx: { customToolEffects: [['coin', 'read']] } },
  { id: 'rc-no-map', name: 'run_custom', input: { tool: 'coin' } },
  { id: 'rc-invalid', name: 'run_custom', input: { parameters: {} }, ctx: { customToolEffects: [['coin', 'read']] } },
  { id: 'rc-bad-params', name: 'run_custom', input: { tool: 'coin', parameters: [1] }, ctx: { customToolEffects: [['coin', 'read']] } },
  { id: 'rc-null-params', name: 'run_custom', input: { tool: 'coin', parameters: null }, ctx: { customToolEffects: [['coin', 'read']] } },
  { id: 'plugin-hit', name: 'mcp_x', input: {}, ctx: { pluginToolEffects: [['mcp_x', 'read']] } },
  { id: 'plugin-miss', name: 'mcp_unknown', input: {} },
  { id: 'plugin-shadowed', name: 'doc_read_file', input: {}, ctx: { pluginToolEffects: [['doc_read_file', 'destroy']] } },
];
const toCtx = (ctx?: Ctx) =>
  ctx
    ? {
        ...(ctx.customToolEffects ? { customToolEffects: new Map(ctx.customToolEffects) } : {}),
        ...(ctx.pluginToolEffects ? { pluginToolEffects: new Map(ctx.pluginToolEffects) } : {}),
      }
    : undefined;
for (const c of calls) {
  out({
    kind: 'effectOf',
    id: c.id,
    name: c.name,
    input: c.input === undefined ? null : c.input,
    inputAbsent: c.input === undefined,
    ctx: c.ctx ?? {},
    out: effectOfToolCall(c.name, c.input, toCtx(c.ctx)),
  });
}

// --- maxEffectOfTool -----------------------------------------------------------
for (const [id, name, ctx] of [
  ['max-state', 'state', undefined],
  ['max-run-custom', 'run_custom', { customToolEffects: [['coin', 'read']] }],
  ['max-destroy', 'doc_delete_file', undefined],
  ['max-plugin', 'mcp_x', { pluginToolEffects: [['mcp_x', 'none']] }],
  ['max-unknown', 'mcp_y', undefined],
] as [string, string, Ctx | undefined][]) {
  out({ kind: 'maxEffect', id, name, ctx: ctx ?? {}, out: maxEffectOfTool(name, toCtx(ctx)) });
}

// --- highestToolEffect ---------------------------------------------------------
for (const [id, effects] of [
  ['highest-empty', []],
  ['highest-mixed', ['read', 'none', 'write']],
  ['highest-destroy', ['destroy', 'read']],
  ['highest-none', ['none', 'none']],
] as [string, ToolEffect[]][]) {
  out({ kind: 'highest', id, effects, out: highestToolEffect(effects) });
}

// --- the pairwise helpers -------------------------------------------------------
for (const a of TOOL_EFFECTS) {
  for (const b of TOOL_EFFECTS) {
    out({ kind: 'within', id: `within-${a}-${b}`, effect: a, ceiling: b, out: isWithinCeiling(a, b) });
    out({ kind: 'maxPair', id: `max-${a}-${b}`, a, b, out: maxToolEffect(a, b) });
    out({ kind: 'lower', id: `lower-${a}-${b}`, ceiling: a, other: b, out: lowerToolEffect(a, b) });
    out({ kind: 'refusal', id: `refusal-${a}-${b}`, name: 'doc_write_file', effect: a, ceiling: b, out: describeCeilingRefusal('doc_write_file', a, b) });
  }
  out({ kind: 'lower', id: `lower-undefined-${a}`, ceiling: null, other: a, out: lowerToolEffect(undefined, a) });
}

// --- usesAllowedUnder for the four computed × four ceilings --------------------
for (const name of names) {
  const spec = BUILT_IN_TOOL_EFFECTS[name];
  if (!isComputedToolEffect(spec)) continue;
  for (const ceiling of TOOL_EFFECTS) {
    out({ kind: 'usesAllowed', id: `uses-${name}-${ceiling}`, name, ceiling, out: spec.usesAllowedUnder(ceiling) });
  }
}

// --- customToolEffect over parsed definitions -----------------------------------
const base = {
  roll: { min: 0.7, max: 0.7 },
  outcomes: [{ when: true, message: 'Done.', state: 'success' }],
};
const defs: { id: string; def: Record<string, unknown> }[] = [
  { id: 'ct-read', def: { name: 'coin', description: 'Flip a coin.', ...base } },
  { id: 'ct-empty-effects', def: { name: 'idle', description: 'Nothing.', effects: [], ...base } },
  { id: 'ct-write', def: { name: 'ledger', description: 'Count.', effects: [{ target: 'state.count', value: '1' }], ...base } },
  { id: 'ct-progress', def: { name: 'kindle', description: 'Kettle.', effects: [{ target: 'progress.kettle.endTime', value: '{{now}} + 300000' }], ...base } },
  { id: 'ct-wardrobe', def: { name: 'robe', description: 'Robe.', effects: [{ wardrobe: { action: 'wear', items: ['Cloak of Night'] } }], ...base } },
  { id: 'ct-inventory', def: { name: 'pick', description: 'Pick.', effects: [{ inventory: { action: 'pick_up', items: ['tape measure'], equip: false } }], ...base } },
  { id: 'ct-log', def: { name: 'diary', description: 'Diary.', effects: [{ log: 'It happened.' }], ...base } },
  { id: 'ct-when-only', def: { name: 'cond', description: 'Cond.', effects: [{ when: { state: { a: { eq: 1 } } }, target: 'state.a', value: '2' }], ...base } },
];
for (const d of defs) {
  const parsed = QtapCustomToolSchema.safeParse(d.def);
  out({ kind: 'customTool', id: d.id, def: d.def, parsed: parsed.success, out: parsed.success ? customToolEffect(parsed.data) : null });
}
