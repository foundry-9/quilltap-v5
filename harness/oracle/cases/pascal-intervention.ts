/**
 * Tier-1 oracle (P4.D345): operator intervention's PURE half, driving v4's
 * REAL `lib/pascal/intervention/{eligibility,overrides,recompute,spans,
 * veto-result}` and `executeCustomTool` with a scripted `draw` / `consult` /
 * `pins` — never a reimplementation. The rows are seeded by CONTENT from v4's
 * own `intervention-recompute.test.ts` (its 22 cases) and widened with the
 * tables the order names: every `checkOverrides` sentence, the override /
 * decision / body parses (Zod 4.6.7's sentences, joined as `readBody` joins
 * them), the span phrasing, the eligibility matrix, `pinsFromOverrides`, the
 * facts and the dealt-vs-final diff.
 *
 * Nothing here draws randomness: every run is handed its draw, which is the
 * point (a held run replays, never rerolls). The Rust side proves it by
 * handing the engine an EMPTY `FixedBytes` (any draw panics).
 *
 * Generate (v4 at the `236c04e35` pin, Node 24):
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TZ=UTC npx tsx "$V5W/harness/oracle/cases/pascal-intervention.ts" \
 *     > /tmp/oracle-pascal-intervention.ndjson
 * Run:
 *   QT_ORACLE_PASCAL_INTERVENTION=/tmp/oracle-pascal-intervention.ndjson \
 *     cargo test -p quilltap-harness --test pascal_intervention_pure_equivalence
 */

import { executeCustomTool } from '@/lib/pascal/custom-tools';
import { QtapCustomToolSchema } from '@/lib/pascal/custom-tool.types';
import { interventionEligibility, isIntervenable } from '@/lib/pascal/intervention/eligibility';
import * as ov from '@/lib/pascal/intervention/overrides';
import * as rc from '@/lib/pascal/intervention/recompute';
import { describeSpan, withinSpan } from '@/lib/pascal/intervention/spans';
import { vetoResultFor } from '@/lib/pascal/intervention/veto-result';
import { createRequire } from 'node:module';

// v4's OWN zod (the pin's `node_modules`, the instance v4's schemas were built
// with) for the two body schemas `interventions.ts` keeps module-private.
const { z } = createRequire(`${process.cwd()}/package.json`)('zod') as typeof import('zod');

type Row = Record<string, unknown>;
const rows: Row[] = [];
const emit = (row: Row) => rows.push(row);

const SNAPSHOT = {
  params: null,
  private: undefined,
  metadata: {},
  progress: {},
  now: 1_000,
  character: 'Mira',
  state: {},
  inventory: null,
};

// ---------------------------------------------------------------- definitions
const RANGE_DOC = {
  name: 'pick',
  description: 'Pick the lock.',
  intervention: { enabled: true },
  parameters: { top: { type: 'number', default: 10, min: 1, max: 100 } },
  roll: { min: 0, max: { $param: 'top' } },
  outcomes: [
    { when: { gte: 5 }, message: 'Open at {{value}}.', state: 'success' },
    { when: true, message: 'Still locked at {{value}}.', state: 'failure' },
  ],
  effects: [
    { when: { outcome: { eq: 'success' } }, target: 'state.opened', value: 'true' },
    { log: '{{character}} tried the lock.' },
  ],
};
const DICE_DOC = {
  name: 'hit',
  description: 'Swing.',
  intervention: { enabled: true },
  roll: '2d6+1',
  outcomes: [
    { when: { gte: 10 }, message: 'Hit: {{dice}}', state: 'success' },
    { when: true, message: 'Miss: {{dice}}', state: 'failure' },
  ],
};
const ORACLE_DOC = {
  name: 'oracle',
  description: 'Ask.',
  intervention: { enabled: true },
  parameters: { topic: { type: 'string', default: 'rain' } },
  llm: { prompt: 'Will it {{params.topic}}?', errorMessage: 'Silence.' },
  outcomes: [
    { when: { llm: { eq: 'YES' } }, message: 'Yes: {{llm}}', state: 'success' },
    { when: true, message: 'No: {{llm}}', state: 'failure' },
  ],
};
const DEALT_FIRST_DOC = {
  name: 'teller',
  description: 'Tell.',
  intervention: { enabled: true },
  parameters: { topic: { type: 'string', default: 'rain' } },
  llm: { prompt: 'Describe {{params.topic}}: {{outcome.state}}', errorMessage: 'Silence.' },
  outcomes: [{ when: true, message: '{{llm}}', state: 'info' }],
};
// The croupier: every intervention axis at once — a `$param` bound, the full
// transform, a consult whose prompt quotes a parameter, a declared chip label,
// four effect kinds, a hidden parameter, a hidden effect, a hidden field.
const CROUPIER_DOC = {
  name: 'croupier',
  description: 'Deal a hand.',
  chipLabel: '{{params.mood}} at {{value}}',
  announceAsCharacter: true,
  intervention: { enabled: true, fields: { chipLabel: false } },
  parameters: {
    stake: { type: 'number', default: 4, min: 1, max: 20 },
    mood: { type: 'string', default: 'calm' },
    secret: { type: 'integer', default: 2, intervene: false },
  },
  roll: { min: 1, max: { $param: 'stake' }, multiplier: 3, offset: 0.5, round: true },
  llm: { prompt: 'The dealer is {{params.mood}}; the hand is {{outcome.state}}.', errorMessage: 'The dealer says nothing.' },
  outcomes: [
    { when: { gte: 8 }, message: 'A strong hand ({{value}}): {{llm}}', state: 'success', brief: 'strong at {{value}}' },
    { when: { gte: 4 }, message: 'A middling hand ({{value}}).', state: 'partial' },
    { when: true, message: 'A poor hand ({{value}}).', state: 'failure' },
  ],
  effects: [
    { target: 'state.hands', value: '{{value}} + 1' },
    { when: { outcome: { eq: 'success' } }, wardrobe: { action: 'wear', items: ['{{params.mood}} hat'] } },
    { inventory: { action: 'equip', items: ['lantern'] }, intervene: false },
    { log: '{{character}} dealt {{value}}.' },
  ],
};
const NEG_DOC = {
  name: 'span',
  description: 'Span.',
  intervention: { enabled: true },
  roll: { min: 1, max: 21, multiplier: -2 },
  outcomes: [{ when: true, message: '{{value}}', state: 'info' }],
};
const HIDDEN_DOC = {
  ...RANGE_DOC,
  revealOdds: false,
  chipLabel: 'Chip {{value}}',
  intervention: { enabled: true, fields: { roll: false, title: false, outcome: false } },
  parameters: { top: { type: 'number', default: 10, intervene: false } },
  effects: [RANGE_DOC.effects[0], { ...RANGE_DOC.effects[1], intervene: false }],
};
const HIDDEN_DICE_DOC = { ...DICE_DOC, intervention: { enabled: true, fields: { roll: false } } };
const LOOSE_DOC = { ...RANGE_DOC, intervention: { enabled: false, fields: { roll: false } } };
const BARE_DOC = { name: 'bare', description: 'Bare.', outcomes: [{ when: true, message: '-', state: 'info' }] };
const CONSULT_HIDDEN_DOC = { ...ORACLE_DOC, intervention: { enabled: true, fields: { llmPrompt: false, llmAnswer: false } } };

const DOCS: Record<string, unknown> = {
  range: RANGE_DOC,
  dice: DICE_DOC,
  oracle: ORACLE_DOC,
  dealtFirst: DEALT_FIRST_DOC,
  croupier: CROUPIER_DOC,
  neg: NEG_DOC,
  hidden: HIDDEN_DOC,
  hiddenDice: HIDDEN_DICE_DOC,
  loose: LOOSE_DOC,
  bare: BARE_DOC,
  consultHidden: CONSULT_HIDDEN_DOC,
};
const def = (key: string) => QtapCustomToolSchema.parse(DOCS[key]);
for (const [key, doc] of Object.entries(DOCS)) emit({ kind: 'definition', key, doc });

// ---------------------------------------------------------------- eligibility
for (const key of Object.keys(DOCS)) {
  for (const rollForm of ['range', 'dice'] as const) {
    const d = def(key);
    emit({ kind: 'eligibility', id: `${key}-${rollForm}`, def: key, rollForm, intervenable: isIntervenable(d), out: interventionEligibility(d, rollForm) });
  }
}

// ---------------------------------------------------------------- the parses
const parseRow = (id: string, schemaName: string, schema: { safeParse: (v: unknown) => { success: boolean; data?: unknown; error?: { issues: Array<{ message: string }> } } }, input: unknown) => {
  const parsed = schema.safeParse(input);
  emit({
    kind: 'parse',
    id,
    schema: schemaName,
    input,
    success: parsed.success,
    data: parsed.success ? parsed.data : null,
    keys: parsed.success && parsed.data && typeof parsed.data === 'object' ? Object.keys(parsed.data as object) : null,
    message: parsed.success ? null : parsed.error!.issues.map((issue) => issue.message).join('; '),
  });
};
const previewSchema = z.object({ id: z.string().min(1), overrides: ov.InterventionOverridesSchema });
const resolveSchema = z.object({ id: z.string().min(1), decision: ov.InterventionDecisionSchema });

const OVERRIDE_INPUTS: Array<[string, unknown]> = [
  ['empty', {}],
  ['every-key', {
    params: { top: 3, mood: 'grim', flag: true }, roll: 2.5, dice: [1, 6], value: -4, llmPrompt: 'Ask again',
    llmAnswer: { ok: false, output: 'no' }, outcomeIndex: 1, message: 'Pinned', state: 'partial', chipLabel: 'Chip',
    title: 'Title', visibility: 'whisper', announcer: 'character',
    effects: { '0': { mode: 'on', value: 3 }, '2': { mode: 'off' }, '1': { mode: 'auto', items: [' a ', 'b'], log: 'L' } },
  }],
  ['author-order', { title: 'T', roll: 1, params: { b: 1, a: 2 } }],
  ['param-int-like-keys', { params: { b: 1, '2': 2, '10': 3, a: 4 } }],
  ['not-object', 'yes'],
  ['null', null],
  ['array', []],
  ['unknown-key', { bogus: 1 }],
  ['unknown-keys-two', { bogus: 1, roll: 2, other: 3 }],
  ['roll-string', { roll: '3' }],
  ['roll-null', { roll: null }],
  ['dice-empty', { dice: [] }],
  ['dice-float', { dice: [1.5, 2] }],
  ['dice-string-face', { dice: ['1'] }],
  ['dice-over-cap', { dice: Array.from({ length: 101 }, () => 1) }],
  ['dice-not-array', { dice: 3 }],
  ['value-bool', { value: true }],
  ['llm-prompt-empty', { llmPrompt: '' }],
  ['llm-prompt-long', { llmPrompt: 'x'.repeat(4001) }],
  ['llm-answer-missing-output', { llmAnswer: { ok: true } }],
  ['llm-answer-extra', { llmAnswer: { ok: true, output: 'y', reason: 'z' } }],
  ['llm-answer-output-long', { llmAnswer: { ok: true, output: 'x'.repeat(100001) } }],
  ['outcome-negative', { outcomeIndex: -1 }],
  ['outcome-float', { outcomeIndex: 1.5 }],
  ['outcome-negative-float', { outcomeIndex: -1.5 }],
  ['outcome-string', { outcomeIndex: '1' }],
  ['message-empty', { message: '' }],
  ['message-long', { message: 'x'.repeat(104001) }],
  ['state-bad', { state: 'win' }],
  ['chip-empty', { chipLabel: '' }],
  ['chip-long', { chipLabel: 'x'.repeat(161) }],
  ['title-long', { title: 'x'.repeat(81) }],
  ['title-astral-80', { title: '\u{1f600}'.repeat(80) }],
  ['visibility-bad', { visibility: 'loud' }],
  ['announcer-bad', { announcer: 'narrator' }],
  ['params-not-record', { params: [1] }],
  ['params-null-value', { params: { top: null } }],
  ['params-object-value', { params: { top: { x: 1 } } }],
  ['effects-key-not-index', { effects: { first: { mode: 'on' } } }],
  ['effects-key-negative', { effects: { '-1': { mode: 'on' } } }],
  ['effects-mode-bad', { effects: { '0': { mode: 'maybe' } } }],
  ['effects-mode-missing', { effects: { '0': {} } }],
  ['effects-pin-extra', { effects: { '0': { mode: 'on', colour: 'red' } } }],
  ['effects-items-over-cap', { effects: { '0': { mode: 'on', items: Array.from({ length: 17 }, () => 'x') } } }],
  ['effects-item-long', { effects: { '0': { mode: 'on', items: ['x'.repeat(401)] } } }],
  ['effects-value-null', { effects: { '0': { mode: 'on', value: null } } }],
  ['effects-log-long', { effects: { '0': { mode: 'on', log: 'x'.repeat(104001) } } }],
  ['issue-order', { announcer: 'x', roll: 's', unknown: 1, state: 'y', params: { a: null } }],
];
for (const [id, input] of OVERRIDE_INPUTS) parseRow(`overrides-${id}`, 'overrides', ov.InterventionOverridesSchema, input);

const DECISION_INPUTS: Array<[string, unknown]> = [
  ['deal', { kind: 'deal', overrides: { roll: 2 } }],
  ['deal-empty-overrides', { kind: 'deal', overrides: {} }],
  ['deal-missing-overrides', { kind: 'deal' }],
  ['deal-bad-overrides', { kind: 'deal', overrides: { roll: 'x' } }],
  ['as-rolled', { kind: 'as-rolled' }],
  ['as-rolled-extra', { kind: 'as-rolled', overrides: {} }],
  ['veto', { kind: 'veto' }],
  ['veto-cause', { kind: 'veto', cause: 'stop' }],
  ['unknown-kind', { kind: 'reroll' }],
  ['missing-kind', {}],
  ['kind-number', { kind: 3 }],
  ['not-object', 'deal'],
  ['null', null],
];
for (const [id, input] of DECISION_INPUTS) parseRow(`decision-${id}`, 'decision', ov.InterventionDecisionSchema, input);

const BODY_INPUTS: Array<[string, 'preview' | 'resolve', unknown]> = [
  ['preview-ok', 'preview', { id: 'h1', overrides: { roll: 1 }, extra: true }],
  ['preview-missing-overrides', 'preview', { id: 'h1' }],
  ['preview-missing-id', 'preview', { overrides: {} }],
  ['preview-empty-id', 'preview', { id: '', overrides: {} }],
  ['preview-id-number', 'preview', { id: 7, overrides: { roll: 'x' } }],
  ['preview-not-object', 'preview', [1]],
  ['preview-null', 'preview', null],
  ['resolve-ok', 'resolve', { id: 'h1', decision: { kind: 'veto' } }],
  ['resolve-missing-decision', 'resolve', { id: 'h1' }],
  ['resolve-bad-kind', 'resolve', { id: 'h1', decision: { kind: 'later' } }],
  ['resolve-both-bad', 'resolve', { id: '', decision: 5 }],
];
for (const [id, schemaName, input] of BODY_INPUTS) {
  parseRow(`body-${id}`, schemaName, schemaName === 'preview' ? previewSchema : resolveSchema, input);
}

// ---------------------------------------------------------------- checkOverrides
const CHECK_ROWS: Array<[string, string, { rollForm: 'range' | 'dice'; diceCount?: number }, Record<string, unknown>]> = [
  ['clean', 'range', { rollForm: 'range' }, { params: { top: 5 }, roll: 2, value: 3, outcomeIndex: 1, message: 'm', state: 'info', title: 't', visibility: 'whisper', announcer: 'character', effects: { '1': { mode: 'off' } } }],
  ['hidden-param', 'hidden', { rollForm: 'range' }, { params: { top: 3 } }],
  ['undeclared-param', 'range', { rollForm: 'range' }, { params: { ghost: 3 } }],
  ['hidden-roll', 'hidden', { rollForm: 'range' }, { roll: 3 }],
  ['hidden-roll-by-dice', 'hiddenDice', { rollForm: 'dice', diceCount: 2 }, { dice: [1, 2] }],
  // The ORDER row: a wrong dice count AND a hidden roll — the hidden roll wins.
  ['hidden-roll-before-count', 'hiddenDice', { rollForm: 'dice', diceCount: 2 }, { dice: [1, 2, 3] }],
  ['roll-on-dice', 'dice', { rollForm: 'dice', diceCount: 2 }, { roll: 3 }],
  ['dice-on-range', 'range', { rollForm: 'range' }, { dice: [1] }],
  ['dice-count', 'dice', { rollForm: 'dice', diceCount: 2 }, { dice: [1, 2, 3] }],
  ['dice-count-ok', 'dice', { rollForm: 'dice', diceCount: 2 }, { dice: [1, 9] }],
  ['value-on-dice', 'dice', { rollForm: 'dice', diceCount: 2 }, { value: 3 }],
  ['llm-prompt-no-llm', 'range', { rollForm: 'range' }, { llmPrompt: 'Ask' }],
  ['llm-answer-no-llm', 'range', { rollForm: 'range' }, { llmAnswer: { ok: true, output: 'x' } }],
  ['llm-prompt-hidden', 'consultHidden', { rollForm: 'range' }, { llmPrompt: 'Ask' }],
  ['llm-answer-hidden', 'consultHidden', { rollForm: 'range' }, { llmAnswer: { ok: true, output: 'x' } }],
  ['outcome-hidden', 'hidden', { rollForm: 'range' }, { outcomeIndex: 0 }],
  ['message-ok-on-hidden', 'hidden', { rollForm: 'range' }, { message: 'x' }],
  ['chip-no-chip', 'range', { rollForm: 'range' }, { chipLabel: 'x' }],
  ['chip-hidden', 'croupier', { rollForm: 'range' }, { chipLabel: 'x' }],
  ['chip-declared', 'hidden', { rollForm: 'range' }, { chipLabel: 'x' }],
  ['title-hidden', 'hidden', { rollForm: 'range' }, { title: 'x' }],
  ['outcome-out-of-range', 'dice', { rollForm: 'dice', diceCount: 2 }, { outcomeIndex: 9 }],
  ['effect-hidden', 'hidden', { rollForm: 'range' }, { effects: { '1': { mode: 'off' } } }],
  ['effect-undeclared', 'range', { rollForm: 'range' }, { effects: { '5': { mode: 'on' } } }],
  ['effect-croupier-hidden', 'croupier', { rollForm: 'range' }, { effects: { '0': { mode: 'on' }, '2': { mode: 'off' } } }],
  ['param-before-roll', 'hidden', { rollForm: 'range' }, { roll: 3, params: { top: 1 } }],
  ['loose-flags-inert', 'loose', { rollForm: 'range' }, { roll: 1 }],
];
for (const [id, key, dealt, raw] of CHECK_ROWS) {
  const overrides = ov.InterventionOverridesSchema.parse(raw);
  emit({ kind: 'check', id, def: key, dealt, overrides: raw, out: ov.checkOverrides(def(key), dealt, overrides) });
}

// ---------------------------------------------------------------- pins + presentation
for (const [id, raw] of OVERRIDE_INPUTS) {
  const parsed = ov.InterventionOverridesSchema.safeParse(raw);
  if (!parsed.success) continue;
  emit({ kind: 'pins', id: `pins-${id}`, overrides: raw, out: rc.pinsFromOverrides(parsed.data) });
}
for (const key of ['range', 'croupier', 'bare']) {
  for (const [id, raw] of [['none', {}], ['title', { title: 'T' }], ['announcer', { announcer: 'pascal' }], ['both', { title: 'T', announcer: 'character' }]] as const) {
    for (const visibility of ['public', 'whisper'] as const) {
      emit({ kind: 'presentation', id: `${key}-${id}-${visibility}`, def: key, visibility, overrides: raw, out: rc.presentationFor(def(key), visibility, raw) });
    }
  }
}

// ---------------------------------------------------------------- recompute
type Draw = { form: 'range'; unit: number } | { form: 'dice'; faces: number[] };
type Consult = { ok: boolean; output: string; prompt: string; reason?: string; provider?: string; model?: string };
interface RecomputeCase {
  id: string;
  def: string;
  draw: Draw;
  consult?: Consult;
  overrides: Record<string, unknown>;
  snapshot?: Partial<typeof SNAPSHOT> & Record<string, unknown>;
}
const CONSULT: Consult = { ok: true, output: 'YES', prompt: 'Will it rain?' };
const RECOMPUTE: RecomputeCase[] = [
  { id: 'unit-kept', def: 'range', draw: { form: 'range', unit: 0.9 }, overrides: {} },
  { id: 'unit-kept-wider', def: 'range', draw: { form: 'range', unit: 0.9 }, overrides: { params: { top: 50 } } },
  { id: 'roll-pinned-through-param', def: 'range', draw: { form: 'range', unit: 0.9 }, overrides: { params: { top: 50 }, roll: 2 } },
  { id: 'param-clamped', def: 'range', draw: { form: 'range', unit: 0.5 }, overrides: { params: { top: 500 } } },
  { id: 'param-coerced-not-clamped', def: 'range', draw: { form: 'range', unit: 0.5 }, overrides: { params: { top: 20 } } },
  { id: 'param-bad-type', def: 'range', draw: { form: 'range', unit: 0.5 }, overrides: { params: { top: 'many' } } },
  { id: 'param-undeclared', def: 'range', draw: { form: 'range', unit: 0.5 }, overrides: { params: { ghost: 1 } } },
  { id: 'dice-replayed', def: 'dice', draw: { form: 'dice', faces: [2, 3] }, overrides: {} },
  { id: 'dice-pinned-clamped', def: 'dice', draw: { form: 'dice', faces: [2, 3] }, overrides: { dice: [9, 6] } },
  { id: 'dice-pinned-zero-face', def: 'dice', draw: { form: 'dice', faces: [2, 3] }, overrides: { dice: [0, -4] } },
  { id: 'dice-wrong-count', def: 'dice', draw: { form: 'dice', faces: [2, 3] }, overrides: { dice: [1, 2, 3] } },
  { id: 'dice-recorded-wrong-count', def: 'dice', draw: { form: 'dice', faces: [2] }, overrides: {} },
  { id: 'outcome-forced', def: 'range', draw: { form: 'range', unit: 0.1 }, overrides: { outcomeIndex: 0 } },
  { id: 'outcome-forced-same', def: 'range', draw: { form: 'range', unit: 0.9 }, overrides: { outcomeIndex: 0 } },
  { id: 'outcome-forced-hidden-odds', def: 'hidden', draw: { form: 'range', unit: 0.1 }, overrides: { outcomeIndex: 0 } },
  { id: 'outcome-no-row', def: 'range', draw: { form: 'range', unit: 0.1 }, overrides: { outcomeIndex: 7 } },
  { id: 'message-literal', def: 'range', draw: { form: 'range', unit: 0.1 }, overrides: { message: 'Exactly {{value}} as written.' } },
  { id: 'message-pinned-row-moved', def: 'range', draw: { form: 'range', unit: 0.1 }, overrides: { message: 'Fits row 2.', roll: 7 } },
  { id: 'state-pinned', def: 'range', draw: { form: 'range', unit: 0.9 }, overrides: { state: 'failure' } },
  { id: 'effects-auto', def: 'range', draw: { form: 'range', unit: 0.1 }, overrides: {} },
  { id: 'effects-forced', def: 'range', draw: { form: 'range', unit: 0.1 }, overrides: { effects: { '0': { mode: 'on', value: '{{value}}' }, '1': { mode: 'off' } } } },
  { id: 'effects-relogged', def: 'range', draw: { form: 'range', unit: 0.1 }, overrides: { effects: { '1': { mode: 'auto', log: 'Nobody touched {{it}}.' } } } },
  { id: 'effects-log-blank', def: 'range', draw: { form: 'range', unit: 0.1 }, overrides: { effects: { '1': { mode: 'on', log: '   ' } } } },
  { id: 'effects-on-number', def: 'range', draw: { form: 'range', unit: 0.1 }, overrides: { effects: { '0': { mode: 'on', value: 12.5 } } } },
  { id: 'effects-on-bool', def: 'range', draw: { form: 'range', unit: 0.1 }, overrides: { effects: { '0': { mode: 'on', value: false } } } },
  { id: 'effects-croupier-items', def: 'croupier', draw: { form: 'range', unit: 0.99 }, consult: { ok: true, output: 'Fine.', prompt: 'The dealer is calm; the hand is success.' }, overrides: { effects: { '1': { mode: 'auto', items: ['  top hat ', '', '{{params.mood}} cane'] }, '2': { mode: 'on', items: ['   '] } } } },
  { id: 'effects-croupier-off-wardrobe', def: 'croupier', draw: { form: 'range', unit: 0.99 }, consult: { ok: true, output: 'Fine.', prompt: 'The dealer is calm; the hand is success.' }, overrides: { effects: { '1': { mode: 'off' }, '3': { mode: 'auto' } } } },
  { id: 'presentation-overridden', def: 'range', draw: { form: 'range', unit: 0.1 }, overrides: { title: 'The Lock', announcer: 'character', visibility: 'whisper' } },
  { id: 'visibility-public-over-private', def: 'range', draw: { form: 'range', unit: 0.1 }, overrides: { visibility: 'public' }, snapshot: { private: true } },
  { id: 'snapshot-private', def: 'range', draw: { form: 'range', unit: 0.1 }, overrides: {}, snapshot: { private: true } },
  { id: 'snapshot-params', def: 'range', draw: { form: 'range', unit: 0.5 }, overrides: { params: { top: 30 } }, snapshot: { params: { top: 4 } } },
  { id: 'snapshot-nobody', def: 'range', draw: { form: 'range', unit: 0.5 }, overrides: {}, snapshot: { character: null } },
  { id: 'consult-first-stands-in', def: 'oracle', draw: { form: 'range', unit: 0.5 }, consult: CONSULT, overrides: {} },
  { id: 'consult-first-stale', def: 'oracle', draw: { form: 'range', unit: 0.5 }, consult: CONSULT, overrides: { params: { topic: 'snow' } } },
  { id: 'consult-first-pinned-prompt', def: 'oracle', draw: { form: 'range', unit: 0.5 }, consult: CONSULT, overrides: { llmPrompt: 'Will it hail?' } },
  { id: 'dealt-first-stale', def: 'dealtFirst', draw: { form: 'range', unit: 0.5 }, consult: { ok: true, output: 'Drizzle.', prompt: 'Describe rain: info' }, overrides: { params: { topic: 'hail' } } },
  { id: 'pinned-answer-repicks', def: 'oracle', draw: { form: 'range', unit: 0.5 }, consult: CONSULT, overrides: { llmAnswer: { ok: true, output: 'NO' }, params: { topic: 'snow' } } },
  { id: 'pinned-answer-failed', def: 'oracle', draw: { form: 'range', unit: 0.5 }, consult: { ...CONSULT, reason: 'r', provider: 'P', model: 'M' }, overrides: { llmAnswer: { ok: false, output: 'quiet' } } },
  { id: 'consult-verbatim-extra-keys', def: 'oracle', draw: { form: 'range', unit: 0.5 }, consult: { ok: false, output: 'Silence.', prompt: 'Will it rain?', reason: 'timeout', provider: 'P', model: 'M' }, overrides: {} },
  { id: 'consult-absent', def: 'oracle', draw: { form: 'range', unit: 0.5 }, overrides: {} },
  { id: 'croupier-plain', def: 'croupier', draw: { form: 'range', unit: 0.4 }, consult: { ok: true, output: 'Mm.', prompt: 'The dealer is calm; the hand is partial.' }, overrides: {} },
  { id: 'croupier-mood-stale', def: 'croupier', draw: { form: 'range', unit: 0.4 }, consult: { ok: true, output: 'Mm.', prompt: 'The dealer is calm; the hand is partial.' }, overrides: { params: { mood: 'grim' } } },
  { id: 'croupier-forced-row-stale', def: 'croupier', draw: { form: 'range', unit: 0.4 }, consult: { ok: true, output: 'Mm.', prompt: 'The dealer is calm; the hand is partial.' }, overrides: { outcomeIndex: 0, chipLabel: 'Pinned chip' } },
  { id: 'croupier-value-pinned', def: 'croupier', draw: { form: 'range', unit: 0.4 }, consult: { ok: true, output: 'Mm.', prompt: 'The dealer is calm; the hand is partial.' }, overrides: { value: 100 } },
  { id: 'span-wider', def: 'range', draw: { form: 'range', unit: 0.5 }, overrides: { params: { top: 50 } } },
  { id: 'span-neg', def: 'neg', draw: { form: 'range', unit: 0.25 }, overrides: {} },
  { id: 'span-pins-outside', def: 'range', draw: { form: 'range', unit: 0.5 }, overrides: { roll: 12, value: -1 } },
  { id: 'span-pins-inside', def: 'range', draw: { form: 'range', unit: 0.5 }, overrides: { roll: 9.9 } },
  { id: 'span-pin-on-open-end', def: 'range', draw: { form: 'range', unit: 0.5 }, overrides: { roll: 10 } },
  { id: 'span-neg-pin-on-open-end', def: 'neg', draw: { form: 'range', unit: 0.25 }, overrides: { value: -42 } },
  { id: 'span-croupier-float', def: 'croupier', draw: { form: 'range', unit: 0.4 }, consult: { ok: true, output: 'Mm.', prompt: 'The dealer is calm; the hand is partial.' }, overrides: { params: { stake: 1.2345678 }, roll: 99.987654321 } },
  { id: 'span-dice-pin-value', def: 'dice', draw: { form: 'dice', faces: [6, 6] }, overrides: {} },
];

const asResult = async (fn: () => Promise<unknown>) => {
  try {
    return { ok: await fn(), error: null };
  } catch (error) {
    return { ok: null, error: (error as Error).message };
  }
};

async function main() {
  for (const c of RECOMPUTE) {
    const d = def(c.def);
    const snapshot = { ...SNAPSHOT, ...(c.snapshot ?? {}) };
    const parsed = ov.InterventionOverridesSchema.parse(c.overrides);
    // The baseline: the run as first dealt (the draw and consult, no pins).
    const baseline = await asResult(() =>
      executeCustomTool(d, snapshot.params, {
        metadata: snapshot.metadata,
        progress: snapshot.progress,
        now: snapshot.now,
        ...(snapshot.character ? { character: snapshot.character } : {}),
        state: snapshot.state,
        inventory: null,
        ...(snapshot.private !== undefined ? { private: snapshot.private as boolean } : {}),
        draw: c.draw,
        ...(c.consult ? { consult: c.consult } : {}),
      })
    );
    const recomputed = await asResult(() =>
      rc.recomputeRun({
        definition: d,
        snapshot: snapshot as never,
        draw: c.draw,
        ...(c.consult ? { consult: c.consult } : {}),
        overrides: parsed,
        ...(baseline.ok ? { dealtOutcomeIndex: (baseline.ok as { outcomeIndex: number }).outcomeIndex } : {}),
      })
    );
    let facts: unknown = null;
    if (baseline.ok && recomputed.ok) {
      const base = baseline.ok as Parameters<typeof rc.factsOf>[1];
      const run = recomputed.ok as Awaited<ReturnType<typeof rc.recomputeRun>>;
      const dealtFacts = rc.factsOf(d, base, rc.presentationFor(d, base.visibility));
      const finalFacts = rc.factsOf(d, run.result, run.presentation);
      facts = {
        before: dealtFacts,
        after: finalFacts,
        changed: rc.changedFields(dealtFacts, finalFacts),
        self: rc.changedFields(finalFacts, finalFacts),
        summary: rc.summarizeEffects(d, run.result.effects),
      };
    }
    emit({
      kind: 'recompute',
      id: c.id,
      def: c.def,
      snapshot: { ...snapshot, private: snapshot.private ?? null },
      draw: c.draw,
      consult: c.consult ?? null,
      overrides: c.overrides,
      baseline: baseline.ok,
      baselineError: baseline.error,
      out: recomputed.ok,
      error: recomputed.error,
      facts,
    });
  }

  // ------------------------------------------------------------ spans
  const SPANS: Array<[string, { min: number; max: number; minExclusive?: boolean; maxExclusive?: boolean }]> = [
    ['exact', { min: 5, max: 5 }],
    ['closed', { min: 1, max: 20 }],
    ['half-open', { min: 1, max: 21, maxExclusive: true }],
    ['min-open', { min: -42, max: -2, minExclusive: true }],
    ['both-open', { min: 0, max: 1, minExclusive: true, maxExclusive: true }],
    ['equal-but-open', { min: 3, max: 3, maxExclusive: true }],
    ['equal-min-open', { min: 3, max: 3, minExclusive: true }],
    ['floats', { min: 0.123456, max: 9.87654321, maxExclusive: true }],
    ['float-tiny', { min: 0.000012345678, max: 1e-7 }],
    ['float-exponent', { min: 1.23456e25, max: 9.87654e30 }],
    ['int-huge', { min: 1e21, max: 2e21 }],
    ['negative-zero', { min: -0, max: 0.5 }],
    ['rounding-carry', { min: 9.99995, max: 99.995 }],
  ];
  const PROBES = [-50, -42, -2, -1, 0, 0.5, 1, 3, 5, 9.99, 20, 21, 1e21];
  for (const [id, span] of SPANS) {
    emit({ kind: 'span', id, span, describe: describeSpan(span), within: PROBES.map((n) => [n, withinSpan(n, span)]) });
  }

  for (const transparent of [false, true]) emit({ kind: 'veto', id: `veto-${transparent}`, transparent, out: vetoResultFor(transparent) });

  for (const row of rows) process.stdout.write(`${JSON.stringify(row)}\n`);
}

main().catch((error) => {
  process.stderr.write(`${(error as Error).stack}\n`);
  process.exit(1);
});
