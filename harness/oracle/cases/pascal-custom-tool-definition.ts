/**
 * Oracle case: the Pascal custom-tool DEFINITION format (P4.6ay unit 1).
 *
 * Drives the REAL code:
 *   QtapCustomToolSchema, displayTitle, formatDefinitionIssues,
 *   collectUnknownKeys  (lib/pascal/custom-tool.types.ts)
 *
 * ## Why this pins FULL rejection strings, Zod's own included
 *
 * `formatDefinitionIssues` is not a test convenience — `loadToolsFromMount`
 * stores its output as `CustomToolLoadError.reason`, and the
 * `/api/v1/chats/{id}/custom-tools` GET route returns that string verbatim in
 * `errors[]`. It is user-visible payload on a route this port also has to
 * diff, so the strings are byte-compared here rather than regex-matched. v4's
 * own test only `toMatch(/…/)`s them, which is a weaker bar; normalizing them
 * here would be hiding a diff the route differential would then have to hide
 * too.
 *
 * That means the Rust port must reproduce Zod 4.4.3's built-in messages AND
 * its issue ORDER (the join is `'; '`), not merely the author-written
 * `ctx.addIssue` messages. The corpus below is built to provoke every issue
 * code the format can actually reach.
 *
 * ## Why the input travels as raw JSON TEXT
 *
 * `JSON.stringify` cannot carry every value `JSON.parse` can produce, so a
 * structured `input` field is a lie for exactly the cases worth testing: an
 * `Infinity` operand (the only way to reach `.finite()`) stringifies to `null`,
 * which would have sent the port chasing a rejection its input could never
 * cause. Shipping the bytes instead means both sides parse the same text with
 * their own JSON parser — which is also what production does, since
 * `readToolFile` hands `JSON.parse` the file's contents.
 *
 * ## The one recorded divergence: overflow literals
 *
 * `{"gt": 1e999}` parses to `Infinity` in JS (so v4 rejects it at the schema,
 * via `.finite()`), but serde_json rejects it at the PARSE with "number out of
 * range" (so v5 rejects it at `read_tool_file`). Both refuse the file; only the
 * reason string differs, and it differs one layer below this schema. No corpus
 * row can assert that as equality, so it is recorded rather than hidden — see
 * the port's note on `finite`. Every other route to `.finite()` is closed:
 * JSON has no `Infinity` or `NaN` literal.
 *
 * Run from inside the server checkout (v4 @ 231be14c, Node 24):
 *   cd /tmp/qt-v4-pin-231be14c          # the pinned detached worktree
 *   TZ=UTC npx tsx \
 *     <V5W>/harness/oracle/cases/pascal-custom-tool-definition.ts \
 *     > /tmp/oracle-pascal-definition.ndjson
 *
 * ## The `gate` rows (P4.d19, v4 `6864bf0e`)
 *
 * Two kinds ride in this one file because both are pure and both are the
 * definition format's business: the `definition` rows below cover the two new
 * top-level keys through the same schema path as everything else, and the
 * `gate` rows at the end drive `evaluateToolGate`/`hasToolGate` directly over
 * (definition, fact sheet) pairs — the evaluator has no other differential, and
 * it is where the fail-soft-is-fail-CLOSED asymmetry lives.
 */

import {
  QtapCustomToolSchema,
  collectUnknownKeys,
  displayTitle,
  formatDefinitionIssues,
  MAX_CHIP_LABEL_LENGTH,
  MAX_DESCRIPTION_LENGTH,
  MAX_EFFECTS,
  MAX_EFFECT_EXPRESSION_LENGTH,
  MAX_EFFECT_TARGET_LENGTH,
  MAX_MESSAGE_LENGTH,
  MAX_OUTCOMES,
  MAX_TITLE_LENGTH,
  MAX_TRIGGERS,
  MAX_TRIGGER_PHRASE_LENGTH,
  MIN_TRIGGER_PHRASE_LENGTH,
} from '@/lib/pascal/custom-tool.types';
import { evaluateToolGate, hasToolGate } from '@/lib/pascal/tool-gate';
// P4.D329: the gate rows' bags — the bench's mock, and v4's REAL context with its
// answers recorded (`../lib/recorded-inventory.ts`).
import { mockInventoryContext, type MockInventoryRow } from '@/lib/pascal/bench-inventory';
import type { InventoryContext } from '@/lib/inventory/refs';
import { contextOver, obj, recording, vivienneItems } from '../lib/recorded-inventory';
// P4.D169: the writable-field rows are generated from v4's own exported list,
// so a field added upstream arrives in the corpus rather than being forgotten.
import { WRITABLE_PROGRESSION_FIELDS } from '@/lib/progressions/schema';
import { flattenProgressions, UNIT_MS } from '@/lib/progressions/engine';

const rows: unknown[] = [];

// ---------------------------------------------------------------- displayTitle
const titleCases: Array<[string, { name: string; title?: string }]> = [
  ['authored', { name: 'scan_hawking_radiation', title: 'Hawking Sweep' }],
  ['derived-underscores', { name: 'scan_hawking_radiation' }],
  ['derived-hyphens', { name: 'force-the-lock' }],
  ['single-word', { name: 'unlock' }],
  ['authored-trimmed', { name: 'saving_throw', title: '  Save vs. the House  ' }],
  ['whitespace-title-falls-back', { name: 'saving_throw', title: '   ' }],
  ['mixed-separators', { name: 'a_b-c_d' }],
  ['runs-of-separators', { name: 'a__b--c' }],
  ['trailing-separator', { name: 'probe_' }],
  ['digits-in-words', { name: 'roll_2d6_twice' }],
];
for (const [id, def] of titleCases) {
  rows.push({ kind: 'title', id, input: def, out: displayTitle(def) });
}

// ------------------------------------------------------------------- the corpus
const BASE = {
  name: 'probe',
  description: 'A probe.',
  outcomes: [{ when: true, message: '-', state: 'info' }],
};

function withWhen(when: unknown, extra: Record<string, unknown> = {}) {
  return {
    ...BASE,
    ...extra,
    outcomes: [
      { when, message: '-', state: 'info' },
      { when: true, message: '-', state: 'info' },
    ],
  };
}

/** The 616930db consult block, as v4's own suite spells it. */
const LLM_BLOCK = {
  llm: { prompt: 'Answer YES or NO: is {{value}} auspicious?', errorMessage: 'The wire went dead.' },
};

/**
 * A definition carrying `effects` (P4.D35, v4 `c4d4b0de`), with the trailing
 * catch-all `BASE` already supplies. `effects` rides as `unknown` so the corpus
 * can pose a non-array and a non-object element too.
 */
function withEffects(effects: unknown, extra: Record<string, unknown> = {}) {
  return { ...BASE, ...extra, effects };
}

/** P4.D333: a definition gated by `clause` (the any-of rows pose arrays). */
function withGate(clause: 'availableWhen' | 'withheldWhen', gate: unknown, extra: Record<string, unknown> = {}) {
  return { ...BASE, ...extra, [clause]: gate };
}

/** P4.D333: one write effect under the condition `when`. */
function withEffectWhen(when: unknown, extra: Record<string, unknown> = {}) {
  return withEffects([{ when, target: 'state.hits', value: 1 }], extra);
}

/**
 * P4.D333: v4's bug-257 narrator (`outcome-brief.test.ts`'s `scanner()`) —
 * a consult told the verdict the table reached.
 */
function scanner(overrides: Record<string, unknown> = {}) {
  return {
    name: 'entanglement_scan',
    description: 'Scan a shard for entanglement.',
    parameters: { layers: { type: 'number', default: 95 } },
    roll: { min: 1, max: 100 },
    llm: {
      prompt: 'Write a report. State: {{outcome.state}}, row {{outcome.index}}. Conclusion to reach: {{outcome.brief}}',
      errorMessage: 'The link falters.',
    },
    outcomes: [
      {
        when: { roll: { gte: { $param: 'layers' } } },
        message: '{{llm}}',
        state: 'success',
        brief: 'Confirmed at {{params.layers}} layers.',
      },
      { when: true, message: '{{llm}}', state: 'failure', brief: 'Not found.' },
    ],
    ...overrides,
  };
}

const NUM_PARAM = { parameters: { scale: { type: 'number', default: 1 } } };
const INT_PARAM = { parameters: { steps: { type: 'integer', default: 2 } } };
const STR_PARAM = { parameters: { material: { type: 'string', default: 'brass' } } };
const BOOL_PARAM = { parameters: { lit: { type: 'boolean', default: true } } };
const CATCH_ALL = { when: true, message: 'fallback', state: 'info' };

const corpus: Array<[string, unknown]> = [
  // ---- accepted -----------------------------------------------------------
  ['base', BASE],
  ['authored-title', { ...BASE, title: 'Probe the Thing' }],
  ['title-at-cap', { ...BASE, title: 'x'.repeat(MAX_TITLE_LENGTH) }],
  ['legacy-value-comparator', withWhen({ gte: 0.3, lte: 0.6 })],
  ['roll-subject', withWhen({ roll: { gte: 15 } })],
  ['params-subject-anded', withWhen({ gt: 1, params: { scale: { gt: 12 } } }, NUM_PARAM)],
  ['eq-string-param', withWhen({ params: { material: { eq: 'brass' } } }, STR_PARAM)],
  ['neq-boolean-param', withWhen({ params: { lit: { neq: false } } }, BOOL_PARAM)],
  ['param-ref-operand', withWhen({ gte: { $param: 'scale' } }, NUM_PARAM)],
  ['param-ref-operand-integer', withWhen({ gte: { $param: 'steps' } }, INT_PARAM)],
  // ---- accepted: the d68638b4 metadata subject -----------------------------
  ['metadata-anded-with-value', withWhen({ gt: 0.6, metadata: { hasAnsibleAccess: { eq: true } } })],
  ['metadata-only-subject', withWhen({ metadata: { clearanceLevel: { gte: 3 } } })],
  ['metadata-key-non-identifier', withWhen({ metadata: { 'Clearance Level': { gte: 3 } } })],
  ['metadata-key-caps', withWhen({ metadata: { HOUSE: { eq: 'Aurum' } } })],
  ['metadata-param-operand', withWhen({ metadata: { clearanceLevel: { gte: { $param: 'scale' } } } }, NUM_PARAM)],
  ['metadata-made-up-key', withWhen({ metadata: { utterlyMadeUp: { eq: true } } })],
  ['metadata-ordering-any-key', withWhen({ metadata: { faction: { gt: 1 } } })],
  ['metadata-multi-key', withWhen({ metadata: { clearanceLevel: { gte: 3 }, HOUSE: { eq: 'Aurum' } } })],
  ['roll-range-full', { ...BASE, roll: { min: 0, max: 20, multiplier: 2, offset: 1, round: true } }],
  ['roll-range-param-refs', { ...BASE, ...NUM_PARAM, roll: { max: { $param: 'scale' } } }],
  ['roll-dice', { ...BASE, roll: '3d6+2' }],
  ['roll-dice-bare', { ...BASE, roll: '1d20' }],
  ['unknown-top-level-key-tolerated', { ...BASE, persist: true }],
  ['schema-key', { ...BASE, $schema: 'https://example.invalid/x.json' }],
  ['reveal-odds-false', { ...BASE, revealOdds: false }],
  ['default-visibility-whisper', { ...BASE, defaultVisibility: 'whisper' }],
  ['disabled-tombstone', { ...BASE, disabled: true }],
  ['param-numeric-bounds', { ...BASE, parameters: { scale: { type: 'number', default: 1, min: 0, max: 10 } } }],
  ['param-min-equals-max', { ...BASE, parameters: { scale: { type: 'number', default: 1, min: 5, max: 5 } } }],
  ['param-integer-default', { ...BASE, parameters: { steps: { type: 'integer', default: 3 } } }],
  ['outcomes-at-cap', {
    ...BASE,
    outcomes: [
      ...Array.from({ length: MAX_OUTCOMES - 1 }, () => ({ when: { gt: 0.5 }, message: '-', state: 'info' })),
      { when: true, message: '-', state: 'info' },
    ],
  }],

  // ---- rejected: the author-written rules ---------------------------------
  ['when-tests-nothing', withWhen({})],
  ['when-empty-params', withWhen({ params: {} })],
  ['when-empty-roll-comparator', withWhen({ roll: {} })],
  ['undeclared-param-tested', withWhen({ params: { scael: { gt: 12 } } }, NUM_PARAM)],
  ['undeclared-param-operand', withWhen({ gte: { $param: 'nope' } }, NUM_PARAM)],
  ['ordering-a-string-param', withWhen({ params: { material: { gt: 1 } } }, STR_PARAM)],
  ['param-wrong-type-eq', withWhen({ params: { material: { eq: 5 } } }, STR_PARAM)],
  ['roll-ref-non-numeric', { ...BASE, ...STR_PARAM, roll: { offset: { $param: 'material' } } }],
  ['roll-ref-undeclared', { ...BASE, roll: { offset: { $param: 'ghost' } } }],
  ['no-trailing-catch-all', { ...BASE, outcomes: [{ when: { gt: 1 }, message: '-', state: 'info' }] }],
  ['early-catch-all', {
    ...BASE,
    outcomes: [
      { when: true, message: '-', state: 'info' },
      { when: { gt: 1 }, message: '-', state: 'info' },
      { when: true, message: '-', state: 'info' },
    ],
  }],
  ['param-min-max-on-string', { ...BASE, parameters: { material: { type: 'string', default: 'x', min: 1 } } }],
  ['param-min-exceeds-max', { ...BASE, parameters: { scale: { type: 'number', default: 1, min: 10, max: 0 } } }],
  ['param-default-wrong-type-number', { ...BASE, parameters: { scale: { type: 'number', default: 'x' } } }],
  ['param-default-not-integer', { ...BASE, parameters: { steps: { type: 'integer', default: 1.5 } } }],
  ['param-default-wrong-type-string', { ...BASE, parameters: { material: { type: 'string', default: 1 } } }],
  ['param-default-wrong-type-boolean', { ...BASE, parameters: { lit: { type: 'boolean', default: 'yes' } } }],
  ['param-name-not-identifier', { ...BASE, parameters: { 'Bad Name': { type: 'number', default: 1 } } }],
  ['params-key-not-identifier', withWhen({ params: { 'Bad Key': { gt: 1 } } }, NUM_PARAM)],
  // ---- rejected: metadata subject load-time rules --------------------------
  ['metadata-empty-object', withWhen({ metadata: {} })],
  ['metadata-empty-comparator', withWhen({ metadata: { faction: {} } })],
  ['metadata-empty-string-key', withWhen({ metadata: { '': { eq: 1 } } })],
  ['metadata-misspelled-comparator', withWhen({ metadata: { faction: { eq: 'Aurum', nonsense: 1 } } })],
  ['metadata-param-operand-undeclared', withWhen({ metadata: { clearanceLevel: { gte: { $param: 'nope' } } } }, NUM_PARAM)],
  // A `z.record` reports its own parsed type, not `object` — the three
  // pre-existing record sites, alongside the gate's new fourth. (P4.d19 found
  // v5 answering `expected object` at all four; no prior row reached one.)
  ['when-params-not-an-object', withWhen({ params: 42 })],
  ['when-metadata-not-an-object', withWhen({ metadata: 'HOUSE' })],
  ['parameters-not-an-object', { ...BASE, parameters: [] }],
  ['too-many-parameters', {
    ...BASE,
    parameters: Object.fromEntries(
      Array.from({ length: 9 }, (_, i) => [`p${i}`, { type: 'number', default: 0 }])
    ),
  }],

  // ---- rejected: Zod's own built-ins (the strings this port must reproduce)
  ['title-empty', { ...BASE, title: '' }],
  ['title-past-cap', { ...BASE, title: 'x'.repeat(MAX_TITLE_LENGTH + 1) }],
  ['description-past-cap', { ...BASE, description: 'x'.repeat(MAX_DESCRIPTION_LENGTH + 1) }],
  ['description-missing', { name: 'probe', outcomes: BASE.outcomes }],
  ['name-missing', { description: 'A probe.', outcomes: BASE.outcomes }],
  ['name-not-identifier', { ...BASE, name: 'Probe It' }],
  ['name-leading-digit', { ...BASE, name: '2probe' }],
  ['name-too-long', { ...BASE, name: 'a'.repeat(65) }],
  // Zod ≥ 4.5.4 (v4 `6e1a64ea6`) measures strings in code points once the
  // UTF-16 count crosses a bound: 60 astral characters are 120 units (over the
  // 80-unit title max under 4.4.3) but 60 code points (within it now); 81 miss
  // it in both measures.
  ['title-astral-within-max', { ...BASE, title: '\u{1F3A9}'.repeat(60) }],
  ['title-astral-over-max', { ...BASE, title: '\u{1F3A9}'.repeat(81) }],
  ['name-wrong-type', { ...BASE, name: 42 }],
  ['outcomes-missing', { name: 'probe', description: 'A probe.' }],
  ['outcomes-empty', { ...BASE, outcomes: [] }],
  ['outcomes-past-cap', {
    ...BASE,
    outcomes: [
      ...Array.from({ length: MAX_OUTCOMES }, () => ({ when: { gt: 0.5 }, message: '-', state: 'info' })),
      { when: true, message: '-', state: 'info' },
    ],
  }],
  ['outcome-message-empty', { ...BASE, outcomes: [{ when: true, message: '', state: 'info' }] }],
  ['outcome-message-past-cap', {
    ...BASE,
    outcomes: [{ when: true, message: 'x'.repeat(MAX_MESSAGE_LENGTH + 1), state: 'info' }],
  }],
  ['unknown-outcome-state', { ...BASE, outcomes: [{ when: true, message: '-', state: 'triumph' }] }],
  ['unknown-visibility', { ...BASE, defaultVisibility: 'secret' }],
  ['unknown-parameter-type', { ...BASE, parameters: { scale: { type: 'complex', default: 1 } } }],
  ['string-compared-with-value', withWhen({ eq: 'brass' })],

  // ---- rejected: the a33ac8b8 STRICTNESS (nested strict; top level tolerant)
  ['unknown-key-inside-when', withWhen({ nonsense: 1 })],
  ['unknown-key-beside-real-comparator', withWhen({ gt: 1, gt3: 2 })],
  ['unknown-key-inside-outcome', { ...BASE, outcomes: [{ when: true, message: '-', state: 'info', x: 1 }] }],
  ['unknown-key-inside-roll-range', { ...BASE, roll: { min: 0, max: 1, bogus: 3 } }],
  ['unknown-key-inside-param-ref', withWhen({ gte: { $param: 'scale', extra: 1 } }, NUM_PARAM)],
  ['unknown-key-inside-roll-comparator', withWhen({ roll: { gte: 1, gte2: 2 } })],
  ['unknown-key-inside-params-comparator', withWhen({ params: { scale: { gt: 1, nope: 2 } } }, NUM_PARAM)],
  ['malformed-param-ref', withWhen({ gte: { $param: 'Not An Identifier' } }, NUM_PARAM)],

  // ---- issue ORDER and multiplicity (the join is '; ', so order is contract)
  ['outcome-bad-field-and-unknown-key', { ...BASE, outcomes: [{ when: true, message: '', state: 'info', x: 1 }] }],
  ['multiple-unknown-keys-in-when', withWhen({ gt: 1, aa: 2, bb: 3 })],
  ['param-multiple-refine-issues', { ...BASE, parameters: { scale: { type: 'number', default: 'x', min: 10, max: 0 } } }],
  ['param-shape-and-refine', { ...BASE, parameters: { material: { type: 'string', default: 1, min: 1 } } }],
  // Does the top-level superRefine still run when the shape already failed?
  // (If it did, `validateOutcomeOrdering` would walk an undefined `outcomes`.)
  ['shape-failure-suppresses-refine', { name: 42, description: 'A probe.', outcomes: [{ when: { gt: 1 }, message: '-', state: 'info' }] }],
  ['two-outcomes-both-bad', {
    ...BASE,
    outcomes: [
      { when: { params: { ghost: { gt: 1 } } }, message: '-', state: 'info' },
      { when: { gte: { $param: 'phantom' } }, message: '-', state: 'info' },
    ],
  }],

  // ---- rejected: the UNION flattening (' — or — ') -------------------------
  ['roll-malformed-dice', { ...BASE, roll: '3dd6' }],
  ['roll-dice-out-of-bounds', { ...BASE, roll: '1d1' }],
  ['roll-dice-too-many', { ...BASE, roll: '101d6' }],
  ['roll-wrong-type', { ...BASE, roll: 42 }],
  ['roll-null', { ...BASE, roll: null }],
  ['when-wrong-literal', withWhen(false)],
  ['when-wrong-type', withWhen('always')],
  ['when-null', withWhen(null)],
  ['roll-range-and-dice-both-broken', { ...BASE, roll: { min: 'x' } }],

  // ---- accepted: the 616930db llm block + subject --------------------------
  ['llm-block', { ...BASE, ...LLM_BLOCK }],
  ['llm-block-max-output', { ...BASE, llm: { ...LLM_BLOCK.llm, maxOutput: 50_000 } }],
  ['llm-block-max-output-at-floor', { ...BASE, llm: { ...LLM_BLOCK.llm, maxOutput: 1 } }],
  ['llm-block-max-output-at-ceiling', { ...BASE, llm: { ...LLM_BLOCK.llm, maxOutput: 100_000 } }],
  ['llm-answer-eq', withWhen({ llm: { eq: 'YES' } }, LLM_BLOCK)],
  ['llm-ok-only', withWhen({ llm: { ok: false } }, LLM_BLOCK)],
  ['llm-ok-anded-with-comparator', withWhen({ llm: { ok: true, eq: 'YES' } }, LLM_BLOCK)],
  ['llm-ordering-test', withWhen({ llm: { gte: 5 } }, LLM_BLOCK)],
  ['llm-param-operand', withWhen({ llm: { eq: { $param: 'scale' } } }, { ...NUM_PARAM, ...LLM_BLOCK })],
  ['llm-anded-with-value', withWhen({ gt: 0.5, llm: { ok: true } }, LLM_BLOCK)],

  // ---- accepted: the 616930db containment comparators ----------------------
  ['contains-string-param', withWhen({ params: { material: { contains: 'ras' } } }, STR_PARAM)],
  ['ncontains-string-param', withWhen({ params: { material: { ncontains: 'iron' } } }, STR_PARAM)],
  ['contains-param-substring', withWhen(
    { params: { cargo: { contains: { $param: 'sought' } } } },
    { parameters: { cargo: { type: 'string', default: 'silk' }, sought: { type: 'string', default: 'opium' } } }
  )],
  ['contains-metadata-key', withWhen({ metadata: { faction: { contains: 'Aurum' } } })],
  ['ncontains-metadata-key', withWhen({ metadata: { faction: { ncontains: 'Ferro' } } })],
  ['contains-llm-answer', withWhen({ llm: { contains: 'west door' } }, LLM_BLOCK)],
  ['contains-llm-param-substring', withWhen(
    { llm: { contains: { $param: 'material' } } },
    { ...STR_PARAM, ...LLM_BLOCK }
  )],
  ['contains-and-ncontains-anded', withWhen(
    { params: { material: { contains: 'ra', ncontains: 'iron' } } },
    STR_PARAM
  )],

  // ---- rejected: the llm block --------------------------------------------
  ['llm-block-missing-error-message', { ...BASE, llm: { prompt: 'Speak.' } }],
  ['llm-block-empty-prompt', { ...BASE, llm: { prompt: '', errorMessage: 'Silence.' } }],
  ['llm-block-empty-error-message', { ...BASE, llm: { prompt: 'Speak.', errorMessage: '' } }],
  ['llm-block-unknown-key', { ...BASE, llm: { prompt: 'Speak.', errorMessage: 'Silence.', model: 'gpt' } }],
  ['llm-block-not-an-object', { ...BASE, llm: 'Speak.' }],
  ['llm-block-prompt-past-cap', { ...BASE, llm: { prompt: 'x'.repeat(4001), errorMessage: 'Silence.' } }],
  ['llm-max-output-zero', { ...BASE, llm: { ...LLM_BLOCK.llm, maxOutput: 0 } }],
  ['llm-max-output-negative', { ...BASE, llm: { ...LLM_BLOCK.llm, maxOutput: -5 } }],
  ['llm-max-output-fractional', { ...BASE, llm: { ...LLM_BLOCK.llm, maxOutput: 12.5 } }],
  ['llm-max-output-past-ceiling', { ...BASE, llm: { ...LLM_BLOCK.llm, maxOutput: 100_001 } }],
  ['llm-max-output-wrong-type', { ...BASE, llm: { ...LLM_BLOCK.llm, maxOutput: 'lots' } }],

  // ---- rejected: the llm subject ------------------------------------------
  ['llm-test-without-block', withWhen({ llm: { eq: 'YES' } })],
  ['llm-empty-comparator', withWhen({ llm: {} }, LLM_BLOCK)],
  ['llm-undeclared-param-operand', withWhen({ llm: { eq: { $param: 'ghost' } } }, LLM_BLOCK)],
  ['llm-misspelled-comparator', withWhen({ llm: { eq: 'a', gt3: 1 } }, LLM_BLOCK)],
  ['llm-ok-wrong-type', withWhen({ llm: { ok: 'yes' } }, LLM_BLOCK)],
  ['llm-not-an-object', withWhen({ llm: 'YES' }, LLM_BLOCK)],

  // ---- rejected: containment load rules ------------------------------------
  ['contains-numeric-param', withWhen({ params: { scale: { contains: '1' } } }, NUM_PARAM)],
  ['contains-bare-value', withWhen({ contains: 'x' })],
  ['contains-raw-roll', withWhen({ roll: { contains: 'x' } })],
  ['contains-empty-substring', withWhen({ params: { material: { contains: '' } } }, STR_PARAM)],
  ['contains-number-substring', withWhen({ params: { material: { contains: 42 } } }, STR_PARAM)],
  ['contains-param-substring-numeric', withWhen(
    { params: { material: { contains: { $param: 'scale' } } } },
    { parameters: { ...STR_PARAM.parameters, ...NUM_PARAM.parameters } }
  )],
  ['contains-param-substring-undeclared', withWhen(
    { params: { material: { contains: { $param: 'ghost' } } } },
    STR_PARAM
  )],
  ['ncontains-boolean-param', withWhen({ params: { lit: { ncontains: 'x' } } }, BOOL_PARAM)],
  ['contains-metadata-undeclared-operand', withWhen(
    { metadata: { faction: { contains: { $param: 'ghost' } } } },
    STR_PARAM
  )],

  // ---- $state references (P4.d10, v4 f48f34dc) ----------------------------
  // accepted: the two closed indirection forms side by side.
  ['state-roll-numeric-fallback', {
    name: 'draw', description: 'A draw.',
    roll: { min: { $state: 'game.low', fallback: 0 }, max: { $state: 'game.high', fallback: 6 } },
    outcomes: [CATCH_ALL],
  }],
  ['state-operand-and-param-default', {
    name: 'draw', description: 'A draw.',
    parameters: { threshold: { type: 'number', default: { $state: 'game.threshold', fallback: 5 } } },
    outcomes: [
      { when: { gte: { $state: 'game.difficulty', fallback: 3 } }, message: 'hard', state: 'success' },
      CATCH_ALL,
    ],
  }],
  ['state-string-operand-contains', withWhen(
    { params: { material: { contains: { $state: 'game.needle', fallback: 'brass' } } } },
    STR_PARAM
  )],
  ['state-eq-bool-fallback', withWhen(
    { params: { lit: { eq: { $state: 'game.lamp', fallback: true } } } },
    BOOL_PARAM
  )],
  ['state-integer-default-whole-fallback', {
    name: 'draw', description: 'A draw.',
    parameters: { count: { type: 'integer', default: { $state: 'game.count', fallback: 3 } } },
    outcomes: [CATCH_ALL],
  }],
  // rejected: the fallback is required and load-bearing.
  ['state-missing-fallback', {
    name: 'draw', description: 'A draw.',
    roll: { min: { $state: 'game.low' } },
    outcomes: [CATCH_ALL],
  }],
  ['state-roll-string-fallback', {
    name: 'draw', description: 'A draw.',
    roll: { min: { $state: 'game.low', fallback: 'nope' } },
    outcomes: [CATCH_ALL],
  }],
  ['state-roll-bool-fallback', {
    name: 'draw', description: 'A draw.',
    roll: { multiplier: { $state: 'game.x', fallback: true } },
    outcomes: [CATCH_ALL],
  }],
  ['state-default-type-mismatch', {
    name: 'draw', description: 'A draw.',
    parameters: { count: { type: 'integer', default: { $state: 'game.count', fallback: 'x' } } },
    outcomes: [CATCH_ALL],
  }],
  ['state-default-fractional-integer-fallback', {
    name: 'draw', description: 'A draw.',
    parameters: { count: { type: 'integer', default: { $state: 'game.count', fallback: 1.5 } } },
    outcomes: [CATCH_ALL],
  }],
  ['state-empty-path', withWhen({ gt: { $state: '', fallback: 1 } })],
  ['state-extra-key', withWhen({ gt: { $state: 'a', fallback: 1, persist: 'x' } })],
  ['state-array-fallback', withWhen({ gt: { $state: 'a', fallback: [1] } })],
  ['state-ordering-string-fallback', withWhen({ gt: { $state: 'a', fallback: 's' } })],
  ['state-contains-number-fallback', withWhen(
    { params: { material: { contains: { $state: 'a', fallback: 3 } } } },
    STR_PARAM
  )],
  ['state-not-quite-a-ref', withWhen({ gt: { $state: 'a', fallbak: 1 } })],

  // === P4.D319: announceAsCharacter + {{character}} (append-only) ===
  // v4 `a05c6f755`: the definition key sits between `defaultVisibility` and
  // `parameters` (Zod's issue ORDER is the shape's key order), is
  // `z.boolean().optional()` (null refused), and is no longer reported by
  // `collectUnknownKeys` (17 known keys). `{{character}}` is a known ref in
  // an effect expression (the refusal sentence unchanged); a wardrobe ref is
  // a template, kept verbatim at load. The ids are fixed by the P4.D319 order
  // (R-3) so the SPA twin's regen writes the same bytes.
  ['announce-as-character-true', { ...BASE, announceAsCharacter: true }],
  ['announce-as-character-false', { ...BASE, announceAsCharacter: false }],
  ['announce-as-character-string', { ...BASE, announceAsCharacter: 'yes' }],
  ['announce-as-character-null', { ...BASE, announceAsCharacter: null }],
  ['announce-as-character-issue-order', { ...BASE, defaultVisibility: 'loud', announceAsCharacter: 'yes', parameters: 5 }],
  ['unknown-keys-announce-as-character-known', { ...BASE, announceAsCharacter: true, bogus: 1 }],
  ['effect-value-character', withEffects([{ target: 'metadata.lastWinner', value: "{{character}} + ' wins'" }])],
  ['effect-value-character-dotted', withEffects([{ target: 'metadata.lastWinner', value: "{{character.name}} + ' wins'" }])],
  ['wardrobe-item-character', withEffects([{ wardrobe: { action: 'wear', items: ["{{character}}'s armour"] } }])],
  // === end P4.D319 ===

  // === P4.D333 rows (append-only; P4.D329 / P4.D331 merge BY ID) ===
  // v4 `a53024d3c` (the OR / any-of half) and `322a5488e` (bug 257's `brief`),
  // seeded by CONTENT from v4's own `custom-tool-definition.test.ts` any-of
  // corpus and `outcome-brief.test.ts`. No row here carries an Inventory or a
  // Log key — those grammars are sibling lanes', and a row of theirs here would
  // be residue on this lane's own branch. Every sentence is RECORDED by the
  // regen, never hand-derived (Zod's union hoisting across three branches).
  // -- any-of outcomes
  ['anyof-outcome-two-groups', withWhen([{ gte: 0.9 }, { metadata: { lucky: { eq: true } } }])],
  ['anyof-outcome-one-element', withWhen([{ gte: 0.9 }])],
  ['anyof-outcome-empty', withWhen([])],
  ['anyof-outcome-nested', withWhen([[{ gte: 0.9 }]])],
  ['anyof-outcome-empty-group', withWhen([{ gte: 0.9 }, {}])],
  ['anyof-outcome-past-the-cap', withWhen(Array.from({ length: 9 }, (_, i) => ({ gte: i })))],
  ['anyof-outcome-past-the-cap-with-bad-group', withWhen([...Array.from({ length: 8 }, (_, i) => ({ gte: i })), {}])],
  ['anyof-outcome-at-the-cap', withWhen(Array.from({ length: 8 }, (_, i) => ({ gte: i })))],
  ['anyof-outcome-scalar-when', withWhen(5)],
  ['anyof-outcome-string-when', withWhen('x')],
  ['anyof-outcome-long-string-when', withWhen('abcdefghij')],
  ['anyof-outcome-empty-string-when', withWhen('')],
  ['anyof-outcome-null-when', withWhen(null)],
  ['anyof-outcome-is-never-a-catch-all', { ...BASE, outcomes: [{ when: [{ gte: 0.5 }], message: '-', state: 'info' }] }],
  ['anyof-outcome-ghost-param-indexed-path', withWhen([{ gte: 0.9 }, { params: { ghost: { eq: 1 } } }])],
  ['anyof-outcome-ghost-param-bare-path', withWhen({ params: { ghost: { eq: 1 } } })],
  ['anyof-outcome-llm-in-group-no-block', withWhen([{ gte: 0.9 }, { llm: { eq: 'YES' } }])],
  // `roll: 5` rather than a bad operand: an operand's own union grows with the
  // Inventory lane's `$inventory` arm, which would make the row sibling residue.
  ['anyof-outcome-array-element-fails-hard', withWhen([{ gte: 0.9 }, { roll: 5 }])],
  ['anyof-outcome-array-element-unrecognized-key', withWhen([{ gte: 0.9, bogus: 1 }])],
  ['anyof-outcome-progress-bad-key-in-group', withWhen([{ gte: 0.9 }, { progress: { cannon: { eq: true } } }])],
  // -- any-of gates
  ['anyof-gate-two-groups', withGate('availableWhen', [{ metadata: { a: { eq: true } } }, { metadata: { b: { eq: true } } }])],
  ['anyof-gate-one-element', withGate('availableWhen', [{ metadata: { a: { eq: true } } }])],
  ['anyof-gate-empty', withGate('withheldWhen', [])],
  ['anyof-gate-empty-group', withGate('availableWhen', [{ metadata: { a: { eq: 1 } } }, {}])],
  ['anyof-gate-bare-empty-object-unburied', withGate('availableWhen', {})],
  ['anyof-gate-past-the-cap', withGate('availableWhen', Array.from({ length: 9 }, (_, i) => ({ metadata: { [`k${i}`]: { eq: 1 } } })))],
  ['anyof-gate-both-clauses-arrays', withGate('availableWhen', [{ metadata: { a: { eq: 1 } } }], { withheldWhen: [{ metadata: { b: { eq: 1 } } }] })],
  ['anyof-gate-progress-bad-key-in-group-1', withGate('withheldWhen', [{ metadata: { a: { eq: 1 } } }, { progress: { cannon: { eq: true } } }])],
  ['anyof-gate-scalar', withGate('availableWhen', 5)],
  ['anyof-gate-long-string', withGate('availableWhen', 'abcdefghij')],
  ['anyof-gate-array-element-fails-hard', withGate('availableWhen', [{ metadata: { a: { eq: 1 } } }, { metadata: { b: { gte: 'x' } } }])],
  // -- any-of effect conditions
  ['anyof-effect-two-groups', withEffectWhen([{ outcome: { eq: 'success' } }, { metadata: { lucky: { eq: true } } }])],
  ['anyof-effect-empty', withEffectWhen([])],
  ['anyof-effect-ghost-param-indexed-path', withEffectWhen([{ outcome: { eq: 'success' } }, { params: { ghost: { eq: 1 } } }])],
  ['anyof-effect-ghost-param-bare-path', withEffectWhen({ params: { ghost: { eq: 1 } } })],
  ['anyof-effect-outcome-only-group', withEffectWhen([{ outcome: { neq: 'failure' } }])],
  ['anyof-effect-past-the-cap', withEffectWhen(Array.from({ length: 9 }, (_, i) => ({ gte: i })))],
  ['anyof-effect-array-element-fails-hard', withEffectWhen([{ outcome: { eq: 'success' } }, { roll: 5 }])],
  ['anyof-effect-long-string', withEffectWhen('abcdefghij')],
  // -- briefs (bug 257)
  ['brief-narrator-pattern', scanner()],
  ['brief-prompt-reads-outcome-table-tests-llm', scanner({ outcomes: [
    { when: { llm: { contains: 'confirmed' } }, message: '{{llm}}', state: 'success', brief: 'x' },
    { when: true, message: '{{llm}}', state: 'failure' },
  ] })],
  ['brief-prompt-reads-outcome-ok-test', scanner({ outcomes: [
    { when: { llm: { ok: false } }, message: '{{llm}}', state: 'failure' },
    { when: true, message: '{{llm}}', state: 'success' },
  ] })],
  ['brief-prompt-reads-outcome-in-anyof-group', scanner({ outcomes: [
    { when: [{ gte: 0.9 }, { llm: { eq: 'YES' } }], message: '{{llm}}', state: 'success' },
    { when: true, message: '{{llm}}', state: 'failure' },
  ] })],
  ['brief-without-llm-block', { name: 'plain', description: 'No consult here.', outcomes: [{ when: true, message: 'ok', state: 'info', brief: 'Say ok.' }] }],
  ['brief-quotes-llm', scanner({ outcomes: [{ when: true, message: '{{llm}}', state: 'info', brief: 'See {{llm}}' }] })],
  ['brief-quotes-outcome-index', scanner({ outcomes: [{ when: true, message: '{{llm}}', state: 'info', brief: 'Row {{outcome.index}}' }] })],
  ['brief-empty', scanner({ outcomes: [{ when: true, message: '{{llm}}', state: 'info', brief: '' }] })],
  ['brief-empty-no-llm-block', { ...BASE, outcomes: [{ when: true, message: '-', state: 'info', brief: '' }] }],
  ['brief-too-long', scanner({ outcomes: [{ when: true, message: '{{llm}}', state: 'info', brief: '\u{1F3B2}'.repeat(1001) }] })],
  ['brief-at-the-limit-astral', scanner({ outcomes: [{ when: true, message: '{{llm}}', state: 'info', brief: '\u{1F3B2}'.repeat(1000) }] })],
  ['brief-not-a-string', scanner({ outcomes: [{ when: true, message: '{{llm}}', state: 'info', brief: 7 }] })],
  ['brief-issue-order', scanner({
    llm: { prompt: 'Report {{outcome.state}} then {{ outcome.brief }}.', errorMessage: 'The link falters.' },
    outcomes: [
      { when: { llm: { eq: 'YES' } }, message: '{{llm}}', state: 'success', brief: 'See {{llm}} and {{outcome.state}}' },
      { when: true, message: '{{llm}}', state: 'failure', brief: 'Not found.' },
    ],
  })],
  ['brief-issue-order-no-llm-block', { ...BASE, outcomes: [
    { when: { gte: 0.5 }, message: '-', state: 'success', brief: 'See {{llm}}' },
    { when: true, message: '-', state: 'info', brief: 'Plain.' },
  ] }],
  ['brief-unknown-key-allowed-list', scanner({ outcomes: [{ when: true, message: '{{llm}}', state: 'info', brief: 'x', bogus: 1 }] })],
  ['brief-whitespace-key', scanner({
    llm: { prompt: 'Tell it: {{ outcome.brief }}', errorMessage: 'The link falters.' },
    outcomes: [
      { when: { llm: { ok: true } }, message: '{{llm}}', state: 'success' },
      { when: true, message: '{{llm}}', state: 'failure' },
    ],
  })],
  ['brief-key-order-in-data', scanner({ outcomes: [{ brief: 'First written.', state: 'info', message: '{{llm}}', when: true }] })],
  ['brief-prompt-reads-unknown-outcome-field', scanner({
    llm: { prompt: 'Verdict: {{outcome.verdict}}', errorMessage: 'The link falters.' },
    outcomes: [
      { when: { llm: { eq: 'YES' } }, message: '{{llm}}', state: 'success' },
      { when: true, message: '{{llm}}', state: 'failure' },
    ],
  })],
  // accepted shapes the `brief` floor counts
  ['brief-on-consult-first-table', scanner({
    llm: { prompt: 'Does it hold? YES or NO.', errorMessage: 'The link falters.' },
    outcomes: [
      { when: { llm: { eq: 'YES' } }, message: '{{llm}}', state: 'success', brief: 'Never read by the prompt.' },
      { when: true, message: '{{llm}}', state: 'failure' },
    ],
  })],
  ['brief-on-anyof-row', scanner({ outcomes: [
    { when: [{ gte: 90 }, { metadata: { lucky: { eq: true } } }], message: '{{llm}}', state: 'success', brief: 'A lucky find.' },
    { when: true, message: '{{llm}}', state: 'failure', brief: 'Nothing.' },
  ] })],
  ['brief-quotes-state-metadata-now', scanner({ outcomes: [
    { when: true, message: '{{llm}}', state: 'info', brief: 'At {{now}}, {{metadata.rank}} saw {{state.depth}} via {{dice}}.' },
  ] })],
  ['brief-on-one-row-only', scanner({ outcomes: [
    { when: { gte: 50 }, message: '{{llm}}', state: 'success' },
    { when: true, message: '{{llm}}', state: 'failure', brief: 'Not found.' },
  ] })],
  // === end P4.D333 rows ===

  // ---- the 6864bf0e AVAILABILITY GATES ------------------------------------
  // accepted: one clause, literal operands, every comparator family.
  ['gate-available-when', { ...BASE, availableWhen: { metadata: { toolAbilities: { contains: 'programmable' } } } }],
  ['gate-withheld-when', { ...BASE, withheldWhen: { metadata: { novice: { eq: true } } } }],
  ['gate-ordering-operands', { ...BASE, availableWhen: { metadata: { clearance: { gte: 3, lt: 9 } } } }],
  ['gate-eq-string', { ...BASE, availableWhen: { metadata: { HOUSE: { eq: 'Aurum' } } } }],
  ['gate-eq-number', { ...BASE, availableWhen: { metadata: { rank: { eq: 2 } } } }],
  ['gate-neq-boolean', { ...BASE, withheldWhen: { metadata: { retired: { neq: false } } } }],
  ['gate-ncontains', { ...BASE, withheldWhen: { metadata: { faction: { ncontains: 'Ferro' } } } }],
  ['gate-multi-key', { ...BASE, availableWhen: { metadata: { clearance: { gte: 3 }, HOUSE: { eq: 'Aurum' } } } }],
  ['gate-key-non-identifier', { ...BASE, availableWhen: { metadata: { 'Clearance Level': { gte: 3 } } } }],
  // The gate sits between `disabled` and `revealOdds`: this row pins that the
  // parsed key ORDER lands where v4 declares it, not where the file wrote it.
  ['gate-with-neighbours-out-of-order', {
    ...BASE, revealOdds: true, availableWhen: { metadata: { rank: { gte: 1 } } }, disabled: false,
  }],

  // rejected: the two clauses are not complements, so a file declares one.
  ['gate-both-clauses', {
    ...BASE,
    availableWhen: { metadata: { toolAbilities: { contains: 'programmable' } } },
    withheldWhen: { metadata: { novice: { eq: true } } },
  }],
  // rejected: a gate is answered BEFORE the deal, so its operands are literals.
  ['gate-param-operand', { ...BASE, ...NUM_PARAM, availableWhen: { metadata: { clearance: { gte: { $param: 'scale' } } } } }],
  ['gate-state-operand', { ...BASE, availableWhen: { metadata: { clearance: { gte: { $state: 'game.bar', fallback: 3 } } } } }],
  ['gate-contains-param-operand', { ...BASE, ...STR_PARAM, withheldWhen: { metadata: { faction: { contains: { $param: 'material' } } } } }],
  // rejected: the record's own rules.
  ['gate-empty-metadata', { ...BASE, availableWhen: { metadata: {} } }],
  ['gate-empty-string-key', { ...BASE, availableWhen: { metadata: { '': { eq: 1 } } } }],
  ['gate-empty-comparator', { ...BASE, availableWhen: { metadata: { rank: {} } } }],
  ['gate-empty-contains', { ...BASE, availableWhen: { metadata: { faction: { contains: '' } } } }],
  // rejected: the strictObjects, at both levels.
  ['gate-unknown-comparator-key', { ...BASE, availableWhen: { metadata: { rank: { gte: 1, nonsense: 2 } } } }],
  ['gate-unknown-subject-key', { ...BASE, availableWhen: { metadata: { rank: { gte: 1 } }, roll: { gte: 1 } } }],
  ['gate-missing-metadata', { ...BASE, availableWhen: {} }],
  ['gate-not-an-object', { ...BASE, availableWhen: 'everyone' }],
  ['gate-null', { ...BASE, withheldWhen: null }],
  // rejected: literal operands are TYPED. `.finite()` itself is unreachable
  // from a JSON parse (see the overflow-literal note above), so the reachable
  // half of that rule is what a wrong-typed literal does at each key.
  ['gate-ordering-string-operand', { ...BASE, availableWhen: { metadata: { clearance: { gte: '3' } } } }],
  ['gate-ordering-boolean-operand', { ...BASE, availableWhen: { metadata: { clearance: { lt: true } } } }],
  ['gate-contains-number-operand', { ...BASE, availableWhen: { metadata: { faction: { contains: 42 } } } }],
  ['gate-eq-object-operand', { ...BASE, availableWhen: { metadata: { rank: { eq: { a: 1 } } } } }],

  // ---- chipLabel (P4.D35, v4 `c4d4b0de`) ----------------------------------
  // A template, not a reference site: there is deliberately NO load-time rule
  // that its placeholders resolve — an unknown one renders verbatim, exactly as
  // in an outcome message (the `renderTemplate` doctrine). The Workbench warns;
  // the loader does not.
  ['chip-label-templated', { ...BASE, chipLabel: 'Agent lambda — {{params.label}}' }],
  ['chip-label-at-cap', { ...BASE, chipLabel: 'x'.repeat(MAX_CHIP_LABEL_LENGTH) }],
  ['chip-label-unknown-placeholder-tolerated', { ...BASE, chipLabel: '{{params.never_declared}}' }],
  ['chip-label-empty', { ...BASE, chipLabel: '' }],
  ['chip-label-over-cap', { ...BASE, chipLabel: 'x'.repeat(MAX_CHIP_LABEL_LENGTH + 1) }],
  ['chip-label-not-a-string', { ...BASE, chipLabel: 7 }],
  ['chip-label-null', { ...BASE, chipLabel: null }],

  // ---- triggers (362426c8f) -----------------------------------------------
  // Phrases that run the tool when said in the chat. Stored AS WRITTEN; the
  // two refine rules (no padding, no case-fold repeat) are `validateTriggers`,
  // the FIFTH superRefine step. Zod 4.6.7: an oversized list fails with its
  // `too_big` alone (no element is parsed), and that issue aborts.
  ['triggers-stored-as-written', { ...BASE, triggers: ['Open Sesame'] }],
  ['triggers-at-cap', { ...BASE, triggers: Array.from({ length: MAX_TRIGGERS }, (_, i) => `phrase ${i}`) }],
  ['triggers-over-cap', { ...BASE, triggers: Array.from({ length: MAX_TRIGGERS + 1 }, (_, i) => `phrase ${i}`) }],
  ['triggers-empty-array', { ...BASE, triggers: [] }],
  ['triggers-too-short', { ...BASE, triggers: ['ok'] }],
  ['triggers-at-min', { ...BASE, triggers: ['x'.repeat(MIN_TRIGGER_PHRASE_LENGTH)] }],
  ['triggers-over-long', { ...BASE, triggers: ['x'.repeat(MAX_TRIGGER_PHRASE_LENGTH + 1)] }],
  ['triggers-at-max', { ...BASE, triggers: ['x'.repeat(MAX_TRIGGER_PHRASE_LENGTH)] }],
  ['triggers-not-an-array', { ...BASE, triggers: 'abracadabra' }],
  ['triggers-null', { ...BASE, triggers: null }],
  ['triggers-element-not-a-string', { ...BASE, triggers: [7] }],
  ['triggers-padded', { ...BASE, triggers: ['abracadabra', 'open sesame '] }],
  ['triggers-case-duplicate', { ...BASE, triggers: ['Abracadabra', 'ABRACADABRA'] }],
  // BOTH rules at one index, whitespace first.
  ['triggers-padded-duplicate', { ...BASE, triggers: ['open sesame ', 'OPEN SESAME '] }],
  // The map is never updated for a repeat: a third copy names the FIRST too.
  ['triggers-third-copy-names-first', { ...BASE, triggers: ['a b c', 'A B C', 'a B c'] }],
  // Fold-identity rows: the fold is lower-casing, not case-folding, and these
  // are where a future `to_lowercase` / `toLocaleLowerCase` divergence shows.
  ['triggers-unicode-fold', { ...BASE, triggers: ['Straße', 'STRASSE'] }],
  ['triggers-turkish-i', { ...BASE, triggers: ['İstanbul', 'istanbul'] }],
  ['triggers-final-sigma', { ...BASE, triggers: ['ΟΔΥΣΣΕΥΣ', 'οδυσσευς'] }],
  // Issue ORDER across chipLabel → triggers → description.
  ['triggers-with-chip-and-description-issues', { ...BASE, chipLabel: '', triggers: ['ok'], description: '' }],
  ['shape-order-chip-triggers-description', { name: 'probe', description: 'A probe.', triggers: ['abracadabra'], chipLabel: 'Chip', outcomes: [{ when: true, message: '-', state: 'info' }] }],
  // `triggers` is a KNOWN top-level key; `persist` stays reserved.
  ['unknown-keys-triggers-known', { ...BASE, triggers: ['abracadabra'], persist: true }],

  // ---- P4.D307 rows (the SPA twin's), merged BY ID at unification ----
  // The ids P4.D305 / P4.D306 already carry keep the server lanes' inputs;
  // three ids P4.D307 recorded over a DIFFERENT input are kept under a
  // suffixed id so both inputs stay pinned. The Zod 4.6.7 length-check rows:
  // an array over its `.max(N)` fails with ONLY that `too_big` (no element
  // parsed), and a string input fails the type check AND runs the length
  // checks with the STRING origin.
  ['shape-order-chip-triggers-description-wrong-types', { name: 'probe', chipLabel: 5, triggers: 5, description: 5, outcomes: [{ when: true, message: '-', state: 'info' }] }],
  ['unknown-keys-triggers-known-bogus-key', { ...BASE, triggers: ['abracadabra'], bogus: 1 }],
  ['effects-wardrobe-key-order', withEffects([{ wardrobe: { slot: 'hair', mode: 'add_to_slot', items: ['Ribbon'], action: 'wear' }, when: { outcome: { eq: 'success' } } }])],
  ['effects-wardrobe-take-off-with-mode', withEffects([{ wardrobe: { action: 'take_off', items: ['Hat'], mode: 'replace' } }])],
  ['effects-wardrobe-take-off-no-items', withEffects([{ wardrobe: { action: 'take_off', slot: 'top' } }])],
  ['effects-wardrobe-clear-slot-with-items', withEffects([{ wardrobe: { action: 'clear_slot', slot: 'hair', items: ['Wig'] } }])],
  ['effects-wardrobe-at-cap', withEffects([{ wardrobe: { action: 'wear', items: Array.from({ length: 8 }, (_, i) => `Item ${i}`) } }])],
  ['effects-wardrobe-empty-item-string', withEffects([{ wardrobe: { action: 'wear', items: [''] } }])],
  ['effects-write-when-order-literal-value', withEffects([{ when: { params: { ghost: { gt: 1 } } }, target: 'nowhere', value: 1 }])],
  ['effects-wardrobe-when-order', withEffects([{ when: { params: { ghost: { gt: 1 } } }, wardrobe: { action: 'clear_slot' } }])],
  ['effects-over-cap-bad-elements', withEffects(Array.from({ length: 17 }, () => ({ target: 'nowhere', value: 1 })))],
  ['outcomes-over-cap-bad-elements', { ...BASE, outcomes: Array.from({ length: 33 }, () => ({ when: 5, message: '-', state: 'info' })) }],
  ['effects-string-over-cap', withEffects('x'.repeat(20))],
  ['outcomes-string-over-cap', { ...BASE, outcomes: 'x'.repeat(40) }],
  ['outcomes-empty-string', { ...BASE, outcomes: '' }],
  ['triggers-empty-string', { ...BASE, triggers: '' }],
  ['effects-wardrobe-nine-bad-items', withEffects([{ wardrobe: { action: 'wear', items: Array.from({ length: 9 }, () => 7) } }])],
  ['triggers-nine-bad-elements', { ...BASE, triggers: Array.from({ length: 9 }, () => 7) }],
  // ---- end P4.D307 rows ----

  // === P4.D329 rows (merged BY ID — R-F) ===
  // v4 `e1872c94f` (the Inventory subjects, the `$inventory` operand, the
  // `inventory` effect) + `a53024d3c`'s `inventoryAggregate`. P4.D333 owns the
  // ONE regen of the shared corpus; these ids are merged into it by id.
  // -- the `inventory` subject in a `when`
  ['inventory-when-state-eq', withWhen({ inventory: { sword: { eq: 'equipped' } } })],
  ['inventory-when-state-neq', withWhen({ inventory: { lantern: { neq: 'absent' } } })],
  ['inventory-when-state-both', withWhen({ inventory: { lantern: { neq: 'absent', eq: 'carried' } } })],
  ['inventory-when-state-param-string', withWhen({ inventory: { sword: { eq: { $param: 'material' } } } }, STR_PARAM)],
  ['inventory-when-state-param-number', withWhen({ inventory: { sword: { eq: { $param: 'scale' } } } }, NUM_PARAM)],
  ['inventory-when-state-param-integer-neq', withWhen({ inventory: { sword: { neq: { $param: 'steps' } } } }, INT_PARAM)],
  ['inventory-when-state-param-undeclared', withWhen({ inventory: { sword: { eq: { $param: 'ghost' } } } })],
  ['inventory-when-state-ordering-key', withWhen({ inventory: { sword: { gt: 1 } } })],
  ['inventory-when-state-worn', withWhen({ inventory: { sword: { eq: 'worn' } } })],
  ['inventory-when-state-empty-comparator', withWhen({ inventory: { sword: {} } })],
  ['inventory-when-state-null-comparator', withWhen({ inventory: { sword: null } })],
  ['inventory-when-templated-key', withWhen({ inventory: { '{{params.scale}}': { eq: 'equipped' } } }, NUM_PARAM)],
  ['inventory-when-templated-key-undeclared', withWhen({ inventory: { '{{params.ghost}}': { eq: 'equipped' } } })],
  ['inventory-when-templated-key-llm-no-block', withWhen({ inventory: { '{{llm}}': { eq: 'equipped' } } })],
  ['inventory-when-templated-key-llm-block', withWhen({ inventory: { '{{llm}}': { eq: 'equipped' } } }, LLM_BLOCK)],
  ['inventory-when-key-201', withWhen({ inventory: { ['k'.repeat(201)]: { eq: 'equipped' } } })],
  ['inventory-when-key-200', withWhen({ inventory: { ['k'.repeat(200)]: { eq: 'equipped' } } })],
  ['inventory-when-key-empty', withWhen({ inventory: { '': { eq: 'equipped' } } })],
  ['inventory-when-key-astral-201-units', withWhen({ inventory: { ['😀'.repeat(100) + 'a']: { eq: 'equipped' } } })],
  ['inventory-when-record-not-object', withWhen({ inventory: 5 })],
  ['inventory-when-empty-record-alone', withWhen({ inventory: {} })],
  ['inventory-when-empty-record-beside-metadata', withWhen({ metadata: { a: { eq: 1 } }, inventory: {} })],
  // -- `inventoryCount` in a `when` (a NUMERIC comparator — operands may refer)
  ['inventory-count-when', withWhen({ inventoryCount: { ring: { gte: 2 } } })],
  ['inventory-count-when-contains', withWhen({ inventoryCount: { ring: { contains: 'x' } } })],
  ['inventory-count-when-param-number', withWhen({ inventoryCount: { ring: { gte: { $param: 'scale' } } } }, NUM_PARAM)],
  ['inventory-count-when-param-string', withWhen({ inventoryCount: { ring: { gte: { $param: 'material' } } } }, STR_PARAM)],
  ['inventory-count-when-eq-string-param', withWhen({ inventoryCount: { ring: { eq: { $param: 'material' } } } }, STR_PARAM)],
  ['inventory-count-when-inventory-operand', withWhen({ inventoryCount: { ring: { gte: { $inventory: 'ring.metadata.level', fallback: 2 } } } })],
  ['inventory-count-when-key-placeholder-undeclared', withWhen({ inventoryCount: { '{{params.ghost}}': { lt: 1 } } })],
  ['inventory-count-when-empty-comparator', withWhen({ inventoryCount: { ring: {} } })],
  // -- `inventoryAggregate` in a `when` (`a53024d3c`)
  ['aggregate-when-equipped-count', withWhen({ inventoryAggregate: { 'equipped.count': { eq: 1 } } })],
  ['aggregate-when-owned-names', withWhen({ inventoryAggregate: { 'owned.names': { eq: 1 } } })],
  ['aggregate-when-reachable-titles', withWhen({ inventoryAggregate: { 'reachable.titles': { eq: 1 } } })],
  ['aggregate-when-metadata-list', withWhen({ inventoryAggregate: { 'equipped.metadata.toolAbilities': { eq: 1 } } })],
  ['aggregate-when-metadata-fold', withWhen({ inventoryAggregate: { 'equipped.metadata.toolVersion.max': { eq: 1 } } })],
  ['aggregate-when-metadata-dotted', withWhen({ inventoryAggregate: { 'owned.metadata.a.b': { eq: 1 } } })],
  ['aggregate-when-not-a-scope', withWhen({ inventoryAggregate: { 'carried.names': { eq: 'x' } } })],
  ['aggregate-when-not-a-field', withWhen({ inventoryAggregate: { 'equipped.weight': { eq: 'x' } } })],
  ['aggregate-when-leading-dot', withWhen({ inventoryAggregate: { '.count': { eq: 1 } } })],
  ['aggregate-when-no-dot', withWhen({ inventoryAggregate: { equipped: { eq: 1 } } })],
  ['aggregate-when-metadata-empty', withWhen({ inventoryAggregate: { 'equipped.metadata.': { eq: 1 } } })],
  ['aggregate-when-two-bad-keys', withWhen({ inventoryAggregate: { 'nowhere.count': { eq: 1 }, 'owned.size': { eq: 1 } } })],
  ['aggregate-when-param-undeclared', withWhen({ inventoryAggregate: { 'equipped.names': { eq: { $param: 'ghost' } } } })],
  ['aggregate-when-param-declared', withWhen({ inventoryAggregate: { 'equipped.names': { contains: { $param: 'material' } } } }, STR_PARAM)],
  ['aggregate-when-state-operand', withWhen({ inventoryAggregate: { 'equipped.count': { gte: { $state: 'floor', fallback: 1 } } } })],
  ['aggregate-when-inventory-operand', withWhen({ inventoryAggregate: { 'equipped.count': { gte: { $inventory: 'ring.metadata.level', fallback: 1 } } } })],
  ['aggregate-when-comparator-bad-key', withWhen({ inventoryAggregate: { 'equipped.count': { above: 1 } } })],
  // -- the three records together, in SHAPE order (the issue order)
  ['inventory-when-three-records-order', withWhen({ inventoryAggregate: { 'x.y': { eq: 1 } }, inventoryCount: { ring: { contains: 'x' } }, inventory: { sword: { gt: 1 } }, llm: { ok: true } })],
  ['inventory-when-must-test-something', withWhen({})],
  // -- effect conditions carry the same records
  ['inventory-effect-when-records', withEffects([{ when: { inventory: { sword: { eq: 'carried' } }, inventoryCount: { ring: { lt: 3 } }, inventoryAggregate: { 'owned.count': { gte: 1 } } }, target: 'state.x', value: 1 }])],
  ['inventory-effect-when-must-test-something', withEffects([{ when: {}, target: 'state.x', value: 1 }])],
  ['inventory-effect-when-empty-inventory-record', withEffects([{ when: { inventory: {} }, target: 'state.x', value: 1 }])],
  ['inventory-effect-when-key-placeholder-undeclared', withEffects([{ when: { inventory: { '{{params.ghost}}': { eq: 'carried' } } }, target: 'state.x', value: 1 }])],
  // -- the gate's three records
  ['inventory-gate-literal', { ...BASE, availableWhen: { inventory: { sword: { eq: 'equipped' } } } }],
  ['inventory-gate-param-refused', { ...BASE, ...STR_PARAM, availableWhen: { inventory: { sword: { eq: { $param: 'material' } } } } }],
  ['inventory-gate-placeholder-key', { ...BASE, ...NUM_PARAM, availableWhen: { inventory: { '{{params.scale}}': { eq: 'equipped' } } } }],
  ['inventory-gate-placeholder-key-withheld', { ...BASE, withheldWhen: { inventory: { '{{character}}': { eq: 'absent' } } } }],
  ['inventory-gate-count-literal', { ...BASE, withheldWhen: { inventoryCount: { ring: { gte: 2 } } } }],
  ['inventory-gate-count-param-refused', { ...BASE, ...NUM_PARAM, withheldWhen: { inventoryCount: { ring: { gte: { $param: 'scale' } } } } }],
  ['inventory-gate-count-contains', { ...BASE, withheldWhen: { inventoryCount: { ring: { contains: 'x' } } } }],
  ['inventory-gate-count-placeholder-key', { ...BASE, withheldWhen: { inventoryCount: { '{{params.x}}': { gte: 1 } } } }],
  ['aggregate-gate-literal', { ...BASE, availableWhen: { inventoryAggregate: { 'equipped.names': { eq: 'x' } } } }],
  ['aggregate-gate-param-refused', { ...BASE, ...STR_PARAM, availableWhen: { inventoryAggregate: { 'equipped.names': { eq: { $param: 'material' } } } } }],
  ['aggregate-gate-bad-key', { ...BASE, availableWhen: { inventoryAggregate: { 'carried.names': { eq: 'x' } } } }],
  ['aggregate-gate-key-placeholder-not-checked', { ...BASE, availableWhen: { inventoryAggregate: { 'equipped.metadata.{{x}}': { eq: 'x' } } } }],
  ['inventory-gate-empty-records', { ...BASE, availableWhen: { inventory: {}, inventoryCount: {}, inventoryAggregate: {} } }],
  ['inventory-gate-empty-gate', { ...BASE, availableWhen: {} }],
  ['inventory-gate-metadata-plus-empty-inventory', { ...BASE, availableWhen: { metadata: { a: { eq: 1 } }, inventory: {} } }],
  ['inventory-gate-unknown-key', { ...BASE, availableWhen: { inventroy: { sword: { eq: 'equipped' } } } }],
  ['inventory-gate-shape-order', { ...BASE, availableWhen: { inventoryAggregate: { 'x.y': { eq: 1 } }, inventoryCount: { r: {} }, inventory: { s: {} }, progress: { 'bad key': { eq: 1 } }, metadata: { '': { eq: 1 } } } }],
  ['inventory-gate-record-not-object', { ...BASE, availableWhen: { inventoryCount: [1] } }],
  // -- the `$inventory` operand
  ['inventory-operand-whole-roll', { ...BASE, roll: { $inventory: 'sword.metadata.damage', fallback: '1d4' } }],
  ['inventory-operand-whole-roll-key-order', { ...BASE, roll: { fallback: '1d4', $inventory: 'sword.metadata.damage' } }],
  ['inventory-operand-whole-roll-numeric-fallback', { ...BASE, roll: { $inventory: 'sword.metadata.damage', fallback: 4 } }],
  ['inventory-operand-whole-roll-prose-fallback', { ...BASE, roll: { $inventory: 'sword.metadata.damage', fallback: 'far' } }],
  ['inventory-operand-whole-roll-boolean-fallback', { ...BASE, roll: { $inventory: 'sword.metadata.damage', fallback: true } }],
  ['inventory-operand-whole-roll-title-path', { ...BASE, roll: { $inventory: 'sword.title', fallback: '1d4' } }],
  ['inventory-operand-roll-max-string-fallback', { ...BASE, roll: { max: { $inventory: 'sword.metadata.reach', fallback: 'far' } } }],
  ['inventory-operand-roll-min-numeric', { ...BASE, roll: { min: { $inventory: 'sword.metadata.reach', fallback: 1 }, max: 10 } }],
  ['inventory-operand-state-roll-min-string-fallback', { ...BASE, roll: { min: { $state: 'floor', fallback: 'low' } } }],
  ['inventory-operand-roll-two-kinds', { ...BASE, roll: { min: { $state: 'floor', fallback: true }, max: { $inventory: 'a.metadata.b', fallback: 'x' } } }],
  ['inventory-operand-ordering-string-fallback', withWhen({ gte: { $inventory: 'sword.metadata.bonus', fallback: 'x' } })],
  ['inventory-operand-eq-numeric', withWhen({ eq: { $inventory: 'sword.metadata.bonus', fallback: 0 } })],
  ['inventory-operand-param-eq-string', withWhen({ params: { material: { eq: { $inventory: 'sword.metadata.metal', fallback: 'iron' } } } }, STR_PARAM)],
  ['inventory-operand-contains-string', withWhen({ params: { material: { contains: { $inventory: 'sword.metadata.metal', fallback: 'ir' } } } }, STR_PARAM)],
  ['inventory-operand-contains-number-fallback', withWhen({ params: { material: { contains: { $inventory: 'sword.metadata.metal', fallback: 3 } } } }, STR_PARAM)],
  ['inventory-operand-in-gate-refused', { ...BASE, availableWhen: { metadata: { level: { gte: { $inventory: 'sword.metadata.level', fallback: 1 } } } } }],
  ['inventory-operand-default', { ...BASE, parameters: { bonus: { type: 'number', default: { $inventory: 'sword.metadata.bonus', fallback: 0 } } } }],
  ['inventory-operand-default-string-fallback-on-number', { ...BASE, parameters: { bonus: { type: 'number', default: { $inventory: 'sword.metadata.bonus', fallback: 'x' } } } }],
  ['inventory-operand-default-integer-fractional', { ...BASE, parameters: { bonus: { type: 'integer', default: { $inventory: 'sword.metadata.bonus', fallback: 1.5 } } } }],
  ['inventory-operand-default-bad-path', { ...BASE, parameters: { bonus: { type: 'number', default: { $inventory: 'sword', fallback: 0 } } } }],
  ['inventory-operand-path-empty', withWhen({ eq: { $inventory: '', fallback: 0 } })],
  ['inventory-operand-path-319', withWhen({ eq: { $inventory: 'a.metadata.' + 'k'.repeat(308), fallback: 0 } })],
  ['inventory-operand-path-320', withWhen({ eq: { $inventory: 'a.metadata.' + 'k'.repeat(309), fallback: 0 } })],
  ['inventory-operand-path-321', withWhen({ eq: { $inventory: 'a.metadata.' + 'k'.repeat(310), fallback: 0 } })],
  ['inventory-operand-path-astral-321-units', withWhen({ eq: { $inventory: 'a.metadata.' + '😀'.repeat(155), fallback: 0 } })],
  ['inventory-operand-extra-key', withWhen({ eq: { $inventory: 'a.metadata.b', fallback: 0, note: 'x' } })],
  ['inventory-operand-no-fallback', withWhen({ eq: { $inventory: 'a.metadata.b' } })],
  ['inventory-operand-null-fallback', withWhen({ eq: { $inventory: 'a.metadata.b', fallback: null } })],
  ['inventory-operand-and-state-together', withWhen({ eq: { $inventory: 'a.metadata.b', $state: 'x', fallback: 0 } })],
  ['inventory-operand-key-order', withWhen({ eq: { fallback: 2, $inventory: 'a.metadata.b' } })],
  // -- the `inventory` effect gesture
  ['inventory-effect-equip', withEffects([{ inventory: { action: 'equip', items: ['Excalibur', '{{params.material}}'] } }], STR_PARAM)],
  ['inventory-effect-unequip', withEffects([{ inventory: { action: 'unequip', items: ['lantern'] } }])],
  ['inventory-effect-pick-up', withEffects([{ inventory: { action: 'pick_up', items: ['magic bow'] } }])],
  ['inventory-effect-pick-up-no-equip-key-order', withEffects([{ inventory: { equip: false, items: ['magic bow'], action: 'pick_up' } }])],
  ['inventory-effect-when-and-gesture-key-order', withEffects([{ inventory: { action: 'equip', items: ['x'] }, when: { outcome: { eq: 'success' } } }])],
  ['inventory-effect-equip-with-equip-key', withEffects([{ inventory: { action: 'equip', items: ['x'], equip: true } }])],
  ['inventory-effect-unequip-with-equip-false', withEffects([{ inventory: { action: 'unequip', items: ['x'], equip: false } }])],
  ['inventory-effect-items-empty', withEffects([{ inventory: { action: 'pick_up', items: [] } }])],
  ['inventory-effect-items-empty-equip-on-equip', withEffects([{ inventory: { action: 'equip', items: [], equip: true } }])],
  ['inventory-effect-items-nine', withEffects([{ inventory: { action: 'equip', items: Array.from({ length: 9 }, (_, i) => `o${i}`) } }])],
  ['inventory-effect-items-eight', withEffects([{ inventory: { action: 'equip', items: Array.from({ length: 8 }, (_, i) => `o${i}`) } }])],
  ['inventory-effect-items-missing', withEffects([{ inventory: { action: 'equip' } }])],
  ['inventory-effect-items-string', withEffects([{ inventory: { action: 'equip', items: 'sword' } }])],
  ['inventory-effect-item-empty-string', withEffects([{ inventory: { action: 'equip', items: [''] } }])],
  ['inventory-effect-item-201', withEffects([{ inventory: { action: 'equip', items: ['i'.repeat(201)] } }])],
  ['inventory-effect-action-drop', withEffects([{ inventory: { action: 'drop', items: ['x'] } }])],
  ['inventory-effect-action-missing', withEffects([{ inventory: { items: ['x'] } }])],
  ['inventory-effect-equip-not-boolean', withEffects([{ inventory: { action: 'pick_up', items: ['x'], equip: 'yes' } }])],
  ['inventory-effect-gesture-unknown-key', withEffects([{ inventory: { action: 'equip', items: ['x'], slot: 'hair' } }])],
  ['inventory-effect-gesture-not-object', withEffects([{ inventory: 'equip x' }])],
  ['inventory-effect-item-placeholder-undeclared', withEffects([{ inventory: { action: 'equip', items: ['{{params.ghost}}', 'ok'] } }])],
  ['inventory-effect-item-llm-no-block', withEffects([{ inventory: { action: 'pick_up', items: ['{{llm}}'] } }])],
  ['inventory-effect-item-llm-block', withEffects([{ inventory: { action: 'pick_up', items: ['{{llm}}'] } }], LLM_BLOCK)],
  // -- the FOUR-kind sentences (shared with the Log — R-G)
  ['inventory-effect-with-wardrobe', withEffects([{ inventory: { action: 'equip', items: ['x'] }, wardrobe: { action: 'wear', items: ['Hat'] } }])],
  ['inventory-effect-with-write', withEffects([{ inventory: { action: 'equip', items: ['x'] }, target: 'state.x', value: 1 }])],
  ['inventory-effect-with-target-only', withEffects([{ inventory: { action: 'equip', items: ['x'] }, target: 'state.x' }])],
  ['inventory-effect-write-and-wardrobe', withEffects([{ wardrobe: { action: 'wear', items: ['Hat'] }, target: 'state.x', value: 1 }])],
  ['inventory-effect-needs-something', withEffects([{}])],
  ['inventory-effect-needs-something-with-when', withEffects([{ when: { outcome: { eq: 'success' } } }])],
  ['inventory-effect-gesture-and-when-issues', withEffects([{ when: { params: { ghost: { gt: 1 } } }, inventory: { action: 'equip', items: ['{{params.ghost}}'], equip: false } }])],
  ['inventory-effect-unknown-key-list', withEffects([{ inventory: { action: 'equip', items: ['x'] }, colour: 'red' }])],
  // === end P4.D329 rows ===

  // ---- effects: accepted --------------------------------------------------
  ['effect-literal-number', withEffects([{ target: 'state.encounter.count', value: 3 }])],
  ['effect-literal-negative-number', withEffects([{ target: 'state.debt', value: -12.5 }])],
  ['effect-literal-boolean', withEffects([{ target: 'metadata.lockBroken', value: true }])],
  ['effect-quoted-string-expression', withEffects([{ target: 'metadata.lockpick', value: "'broken pick'" }])],
  ['effect-expression-over-subjects', withEffects([{ target: 'state.tally', value: '{{state.tally}} + {{params.scale}}' }], NUM_PARAM)],
  ['effect-outcome-condition-eq', withEffects([{ when: { outcome: { eq: 'success' } }, target: 'state.wins', value: 1 }])],
  ['effect-outcome-condition-neq', withEffects([{ when: { outcome: { neq: 'failure' } }, target: 'state.attempts', value: 1 }])],
  ['effect-outcome-condition-both', withEffects([{ when: { outcome: { eq: 'success', neq: 'failure' } }, target: 'state.wins', value: 1 }])],
  ['effect-outcome-anded-with-value-test', withEffects([{ when: { gt: 0.5, outcome: { neq: 'failure' } }, target: 'state.x', value: 1 }])],
  ['effect-metadata-condition', withEffects([{ when: { metadata: { hasKey: { eq: true } } }, target: 'state.opened', value: true }])],
  ['effect-roll-condition', withEffects([{ when: { roll: { gte: 10 } }, target: 'state.big', value: true }])],
  ['effect-params-condition', withEffects([{ when: { params: { scale: { gt: 1 } } }, target: 'state.x', value: 1 }], NUM_PARAM)],
  ['effect-llm-reference-with-block', withEffects([{ target: 'metadata.verdict', value: '{{llm}}' }], LLM_BLOCK)],
  ['effect-llm-when-subject-with-block', withEffects([{ when: { llm: { ok: true } }, target: 'state.x', value: 1 }], LLM_BLOCK)],
  ['effect-metadata-key-taken-whole', withEffects([{ target: 'metadata.ansible.tool', value: true }])],
  ['effect-deep-underscore-allowed', withEffects([{ target: 'state.encounter._notes', value: "'ok'" }])],
  ['effect-bracket-index-path', withEffects([{ target: 'state.party[0].hp', value: 1 }])],
  ['effect-value-string-at-cap', withEffects([{ target: 'state.x', value: `'${'x'.repeat(MAX_EFFECT_EXPRESSION_LENGTH - 2)}'` }])],
  ['effects-at-cap', withEffects(Array.from({ length: MAX_EFFECTS }, (_, i) => ({ target: `state.k${i}`, value: 1 })))],
  ['effects-empty-array', withEffects([])],
  ['effect-state-ref-operand-in-condition', withEffects([{ when: { gte: { $state: 'threshold', fallback: 0.5 } }, target: 'state.x', value: 1 }])],

  // ---- effects: rejected --------------------------------------------------
  // The quoting trap and the whole `parseEffectTarget` rejection vocabulary.
  // Each of these renders its reason at an `effects.N.*` path, and the reason
  // is the parser's own sentence spliced in — so this is where the expression
  // grammar's error strings become user-visible payload.
  ['effect-bare-prose-value', withEffects([{ target: 'metadata.lockpick', value: 'broken pick' }])],
  ['effect-target-no-prefix', withEffects([{ target: 'somewhere.else', value: 1 }])],
  ['effect-target-empty-state-path', withEffects([{ target: 'state.', value: 1 }])],
  ['effect-target-empty-metadata-key', withEffects([{ target: 'metadata.', value: 1 }])],
  ['effect-target-underscore-guard', withEffects([{ target: 'state._secrets.combo', value: 1 }])],
  ['effect-target-empty-string', withEffects([{ target: '', value: 1 }])],
  ['effect-target-over-cap', withEffects([{ target: `state.${'x'.repeat(MAX_EFFECT_TARGET_LENGTH)}`, value: 1 }])],
  ['effect-target-missing', withEffects([{ value: 1 }])],
  ['effect-target-not-a-string', withEffects([{ target: 7, value: 1 }])],
  ['effect-value-missing', withEffects([{ target: 'state.x' }])],
  ['effect-value-empty-string', withEffects([{ target: 'state.x', value: '' }])],
  ['effect-value-over-cap', withEffects([{ target: 'state.x', value: `'${'x'.repeat(MAX_EFFECT_EXPRESSION_LENGTH)}'` }])],
  ['effect-value-null', withEffects([{ target: 'state.x', value: null }])],
  ['effect-value-object', withEffects([{ target: 'state.x', value: { a: 1 } }])],
  ['effect-value-array', withEffects([{ target: 'state.x', value: [1] }])],
  ['effect-expression-undeclared-param', withEffects([{ target: 'state.x', value: '{{params.ghost}} + 1' }])],
  ['effect-llm-reference-no-block', withEffects([{ target: 'metadata.verdict', value: '{{llm}}' }])],
  ['effect-llm-when-subject-no-block', withEffects([{ when: { llm: { ok: true } }, target: 'state.x', value: 1 }])],
  ['effect-when-undeclared-param', withEffects([{ when: { params: { ghost: { gt: 1 } } }, target: 'state.x', value: 1 }])],
  ['effect-when-empty', withEffects([{ when: {}, target: 'state.x', value: 1 }])],
  ['effect-when-empty-outcome', withEffects([{ when: { outcome: {} }, target: 'state.x', value: 1 }])],
  ['effect-when-unknown-outcome-state', withEffects([{ when: { outcome: { eq: 'triumph' } }, target: 'state.x', value: 1 }])],
  ['effect-when-unknown-outcome-key', withEffects([{ when: { outcome: { is: 'success' } }, target: 'state.x', value: 1 }])],
  ['effect-when-outcome-not-an-object', withEffects([{ when: { outcome: 'success' }, target: 'state.x', value: 1 }])],
  ['effect-when-unknown-subject-key', withEffects([{ when: { outcome: { eq: 'success' }, bogus: 1 }, target: 'state.x', value: 1 }])],
  ['effect-when-not-an-object', withEffects([{ when: 'always', target: 'state.x', value: 1 }])],
  ['effect-unknown-key', withEffects([{ target: 'state.x', value: 1, bogus: true }])],
  ['effect-not-an-object', withEffects(['state.x'])],
  ['effects-not-an-array', withEffects({ target: 'state.x', value: 1 })],
  ['effects-over-cap', withEffects(Array.from({ length: MAX_EFFECTS + 1 }, (_, i) => ({ target: `state.k${i}`, value: 1 })))],
  // Two bad effects: the issue ORDER across array elements, and that a check
  // (`at most N effects`) is skipped once an element aborted.
  ['effects-two-bad-elements', withEffects([{ target: 'nowhere', value: 1 }, { target: 'state.x', value: 'bare prose' }])],
  // An effect issue and an OUTCOME issue together — validateEffects runs LAST
  // in the superRefine, so its sentences trail the others.
  ['effect-and-outcome-issue', { ...BASE, outcomes: [{ when: { params: { ghost: { gt: 1 } } }, message: '-', state: 'info' }, CATCH_ALL], effects: [{ target: 'nowhere', value: 1 }] }],

  // ---- wardrobe gestures (f0db8cbb8, P4.D306) -----------------------------
  // An effect is a write XOR a gesture: the `superRefine`'s three sentences at
  // their paths, `wardrobeGestureProblems`' nine per-action rules at
  // `effects.N.wardrobe.<field>`, the item-reference placeholder checks, the
  // Zod shape's own refusals (`.min(1)`, `.max(8)`, `.max(200)`, strict, the
  // three enums), and `validateEffects`' NEW issue order (`when` first).
  ['effects-wardrobe-each-action', withEffects([
    { wardrobe: { action: 'wear', items: ['Cloak of Night'] } },
    { wardrobe: { action: 'wear', items: ['Ribbon'], mode: 'add_to_slot', slot: 'hair' } },
    { wardrobe: { action: 'wear', items: ['Regalia'], mode: 'replace' } },
    { when: { outcome: { eq: 'failure' } }, wardrobe: { action: 'take_off', items: ['Sundress'] } },
    { wardrobe: { action: 'take_off', items: ['Sundress'], slot: 'top' } },
    { wardrobe: { action: 'clear_slot', slot: 'hair' } },
  ])],
  // The author's key order is NOT the parsed order — Zod emits schema order.
  ['effects-wardrobe-author-key-order', withEffects([
    { wardrobe: { slot: 'hair', mode: 'add_to_slot', items: ['Ribbon'], action: 'wear' }, when: { outcome: { neq: 'failure' } } },
  ])],
  ['effects-wardrobe-param-template', withEffects([{ wardrobe: { action: 'wear', items: ['{{params.material}} gloves'] } }], STR_PARAM)],
  ['effects-wardrobe-unknown-family', withEffects([{ wardrobe: { action: 'wear', items: ['{{mystery}}'] } }])],
  ['effects-both-halves', withEffects([{ target: 'state.x', value: 1, wardrobe: { action: 'clear_slot', slot: 'hair' } }])],
  ['effects-target-and-wardrobe', withEffects([{ target: 'state.x', wardrobe: { action: 'clear_slot', slot: 'hair' } }])],
  ['effects-empty-object', withEffects([{}])],
  ['effects-when-only', withEffects([{ when: { outcome: { eq: 'success' } } }])],
  ['effects-write-no-value', withEffects([{ target: 'state.x' }])],
  ['effects-write-no-target', withEffects([{ value: 1 }])],
  ['effects-wardrobe-add-to-slot-no-slot', withEffects([{ wardrobe: { action: 'wear', items: ['Hat'], mode: 'add_to_slot' } }])],
  ['effects-wardrobe-slot-without-add-to-slot', withEffects([{ wardrobe: { action: 'wear', items: ['Hat'], slot: 'hair' } }])],
  ['effects-wardrobe-replace-with-slot', withEffects([{ wardrobe: { action: 'wear', items: ['Hat'], mode: 'replace', slot: 'hair' } }])],
  ['effects-wardrobe-wear-no-items', withEffects([{ wardrobe: { action: 'wear' } }])],
  ['effects-wardrobe-wear-no-items-add-to-slot', withEffects([{ wardrobe: { action: 'wear', mode: 'add_to_slot' } }])],
  ['effects-wardrobe-take-off-mode', withEffects([{ wardrobe: { action: 'take_off', items: ['Hat'], mode: 'replace' } }])],
  ['effects-wardrobe-take-off-no-items-mode', withEffects([{ wardrobe: { action: 'take_off', mode: 'wear' } }])],
  ['effects-wardrobe-clear-slot-no-slot', withEffects([{ wardrobe: { action: 'clear_slot' } }])],
  ['effects-wardrobe-clear-slot-items', withEffects([{ wardrobe: { action: 'clear_slot', slot: 'hair', items: ['Wig'] } }])],
  ['effects-wardrobe-clear-slot-everything-wrong', withEffects([{ wardrobe: { action: 'clear_slot', items: ['Wig'], mode: 'wear' } }])],
  ['effects-wardrobe-undeclared-param-at-index-1', withEffects([{ wardrobe: { action: 'wear', items: ['Hat', '{{params.ghost}}'] } }])],
  ['effects-wardrobe-llm-no-block', withEffects([{ wardrobe: { action: 'wear', items: ['{{llm}}'] } }])],
  ['effects-wardrobe-llm-with-block', withEffects([{ wardrobe: { action: 'wear', items: ['{{llm}}'] } }], LLM_BLOCK)],
  ['effects-wardrobe-two-refs-one-item', withEffects([{ wardrobe: { action: 'take_off', items: ['{{params.ghost}} {{llm}}'] } }])],
  ['effects-wardrobe-nine-items', withEffects([{ wardrobe: { action: 'wear', items: Array.from({ length: 9 }, (_, i) => `Item ${i}`) } }])],
  ['effects-wardrobe-eight-items', withEffects([{ wardrobe: { action: 'wear', items: Array.from({ length: 8 }, (_, i) => `Item ${i}`) } }])],
  ['effects-wardrobe-201-char-ref', withEffects([{ wardrobe: { action: 'wear', items: ['x'.repeat(201)] } }])],
  ['effects-wardrobe-200-char-ref', withEffects([{ wardrobe: { action: 'wear', items: ['x'.repeat(200)] } }])],
  ['effects-wardrobe-empty-ref', withEffects([{ wardrobe: { action: 'wear', items: [''] } }])],
  ['effects-wardrobe-non-string-ref', withEffects([{ wardrobe: { action: 'wear', items: ['Hat', 5] } }])],
  ['effects-wardrobe-empty-items', withEffects([{ wardrobe: { action: 'wear', items: [] } }])],
  ['effects-wardrobe-clear-slot-empty-items', withEffects([{ wardrobe: { action: 'clear_slot', slot: 'hair', items: [] } }])],
  ['effects-wardrobe-items-not-an-array', withEffects([{ wardrobe: { action: 'wear', items: 'Hat' } }])],
  ['effects-wardrobe-items-long-string', withEffects([{ wardrobe: { action: 'wear', items: 'Cloak of Night' } }])],
  ['effects-wardrobe-unknown-key', withEffects([{ wardrobe: { action: 'clear_slot', slot: 'hair', who: 'Bob' } }])],
  ['effects-wardrobe-unknown-action', withEffects([{ wardrobe: { action: 'dress', items: ['Hat'] } }])],
  ['effects-wardrobe-missing-action', withEffects([{ wardrobe: { items: ['Hat'] } }])],
  ['effects-wardrobe-unknown-mode', withEffects([{ wardrobe: { action: 'wear', items: ['Hat'], mode: 'layer' } }])],
  ['effects-wardrobe-unknown-slot', withEffects([{ wardrobe: { action: 'clear_slot', slot: 'gloves' } }])],
  ['effects-wardrobe-not-an-object', withEffects([{ wardrobe: 'wear' }])],
  ['effects-wardrobe-null', withEffects([{ wardrobe: null }])],
  ['effects-wardrobe-and-unknown-effect-key', withEffects([{ wardrobe: { action: 'clear_slot', slot: 'hair' }, bogus: 1 }])],
  ['effects-wardrobe-bad-when-and-bad-gesture', withEffects([{ when: { params: { ghost: { gt: 1 } } }, wardrobe: { action: 'clear_slot' } }])],
  ['effects-cap-mixed', withEffects(Array.from({ length: MAX_EFFECTS + 1 }, (_, i) =>
    i % 2 ? { target: `state.k${i}`, value: 1 } : { wardrobe: { action: 'clear_slot', slot: 'hair' } }))],
  ['effects-over-cap-with-bad-element', withEffects([{ wardrobe: { action: 'dress' } }, ...Array.from({ length: MAX_EFFECTS }, (_, i) => ({ target: `state.k${i}`, value: 1 }))])],
  ['effects-cap-mixed-at-cap', withEffects(Array.from({ length: MAX_EFFECTS }, (_, i) =>
    i % 2 ? { target: `state.k${i}`, value: 1 } : { wardrobe: { action: 'clear_slot', slot: 'hair' } }))],
  // The reorder: a write with BOTH a bad `when` subject and a bad target lists
  // the `when` issue FIRST now (v4 `validateEffects` `:1279-1291`).
  ['effects-write-when-order', withEffects([{ when: { params: { ghost: { gt: 1 } } }, target: 'nowhere', value: '{{params.phantom}}' }])],
  ['effects-mixed-writes-and-gestures', withEffects([
    { target: 'state.encounter.count', value: 1 },
    { wardrobe: { action: 'wear', items: ['Cloak of Night', 'Boots'] } },
    { target: 'metadata.ansibleTool', value: "'lens'" },
    { wardrobe: { action: 'take_off', items: ['{{params.material}}'] } },
  ], STR_PARAM)],

  // ---- P4.D169: the progress subject, at LOAD time ------------------------
  // Every writable field, one row each, generated from v4's own exported list
  // so a field added upstream arrives here rather than being forgotten. The
  // accepted rows carry the parsed `data`, which is where the third
  // `EffectTarget` arm's shape (`kind`/`id`/`field`/`raw`) becomes payload.
  ...WRITABLE_PROGRESSION_FIELDS.map(
    (field) =>
      [
        `effect-progress-target-${field.replace('.', '-')}`,
        withEffects([{ target: `progress.cannon.${field}`, value: 1 }]),
      ] as [string, unknown]
  ),
  // A gate may now test progressions alone, or both sheets at once.
  ['gate-progress-only', { ...BASE, availableWhen: { progress: { 'cannon.complete': { eq: true } } } }],
  ['gate-metadata-and-progress', { ...BASE, availableWhen: { metadata: { hasAnsibleAccess: { eq: true } }, progress: { 'cannon.complete': { eq: true } } } }],
  ['gate-withheld-progress-only', { ...BASE, withheldWhen: { progress: { 'cannon.complete': { eq: false } } } }],
  ['gate-progress-empty-record', { ...BASE, availableWhen: { progress: {} } }],
  ['gate-progress-and-empty-metadata', { ...BASE, availableWhen: { metadata: {}, progress: { 'cannon.complete': { eq: true } } } }],
  // An outcome row and an effect condition may each test the sheet.
  ['when-progress-well-shaped-key', withWhen({ progress: { 'cannon.complete': { eq: true } } })],
  ['when-progress-anded-with-value', withWhen({ gt: 0.4, progress: { 'cannon.complete': { eq: false } } })],
  ['when-progress-param-operand', withWhen({ progress: { 'cannon.percent': { gte: { $param: 'scale' } } } }, NUM_PARAM)],
  ['when-progress-empty-record', withWhen({ progress: {} })],
  ['effect-when-progress-condition', withEffects([{ when: { progress: { 'kettle.complete': { eq: true } } }, target: 'state.poured', value: true }])],
  ['effect-when-progress-empty-record', withEffects([{ when: { progress: {} }, target: 'state.x', value: 1 }])],
  // The record-KEY grammar. Zod reports a refused key as its own
  // `Invalid key in record` and `formatDefinitionIssues` never walks the
  // nested reason, so what an author reads is the PATH naming the offending
  // key — which is exactly what these five rows pin.
  ...['cannon', 'Cannon.complete', 'cannon.', '.complete', 'cannon.complete.extra'].map(
    (key, i) =>
      [
        `when-progress-bad-key-${i}`,
        withWhen({ progress: { [key]: { eq: true } } }),
      ] as [string, unknown]
  ),
  ['gate-progress-bad-key', { ...BASE, availableWhen: { progress: { cannon: { eq: true } } } }],
  // The `parseEffectTarget` refusals, each sentence whole.
  ['effect-progress-unwritable-field', withEffects([{ target: 'progress.cannon.percent', value: 1 }])],
  ['effect-progress-updated-at', withEffects([{ target: 'progress.cannon.updatedAt', value: 1 }])],
  ['effect-progress-id-not-an-identifier', withEffects([{ target: 'progress.Cannon.endTime', value: 1 }])],
  ['effect-progress-names-only-an-id', withEffects([{ target: 'progress.cannon', value: 1 }])],
  ['effect-progress-empty', withEffects([{ target: 'progress.', value: 1 }])],
  ['effect-progress-empty-field', withEffects([{ target: 'progress.cannon.', value: 1 }])],
  // The reserved key, through the METADATA door. An effect's value is always a
  // primitive, so this write would replace every span the character carries
  // with a string — refused where the author can still be told why.
  ['effect-target-reserved-progressions', withEffects([{ target: 'metadata.progressions', value: 1 }])],
  ['effect-target-reserved-progressions-dotted', withEffects([{ target: 'metadata.progressions.cannon', value: 1 }])],
  // ...and the two neighbours it must NOT catch.
  ['effect-target-progressions-notes-allowed', withEffects([{ target: 'metadata.progressionsNotes', value: 1 }])],
  ['effect-target-metadata-progress-dotted-allowed', withEffects([{ target: 'metadata.progress.thing', value: 1 }])],

  // ---- where the two new keys sit in the SHAPE ----------------------------
  // Zod walks a shape in declaration order, so the shape position of
  // `chipLabel` (after `title`, before `description`) and of `effects` (after
  // `llm`, before `outcomes`) is an ISSUE-ORDER contract, not just a key order.
  // Only a row that fails at several of those keys at once can see it — every
  // other effects row above would pass with the keys parsed in any order.
  ['shape-order-title-chip-description', { name: 'probe', title: 5, chipLabel: 5, description: 5, outcomes: [{ when: true, message: '-', state: 'info' }] }],
  ['shape-order-llm-effects-outcomes', { ...BASE, llm: 5, effects: 5, outcomes: 5 }],
  ['shape-order-chip-and-effects', { ...BASE, chipLabel: 5, effects: 5 }],

  // ---- shape / non-object -------------------------------------------------
  ['not-an-object', 'nope'],
  ['null-doc', null],
  ['array-doc', []],
  ['empty-object', {}],

  // === P4.D331: the log effect (append-only) ===
  // v4 `e4a60ad9e` at the `322a5488e` pin: a FOURTH effect kind, `log` — a
  // template `z.string().min(1).max(1000)` (code points), its placeholders
  // audited as an outcome message's (`effects.N.log`). The ids are fixed by the
  // P4.D331 order so the one regen (P4.D333, R-F) merges them by id.
  ['log-accepts-plain', withEffects([{ log: 'Tried the {{params.door}}: rolled {{value}}' }], { parameters: { door: { type: 'string', default: 'north door' } } })],
  ['log-accepts-with-when', withEffects([{ when: { outcome: { eq: 'failure' } }, log: '{{character}} failed (rolled {{value}}).' }], { outcomes: [{ when: true, message: '-', state: 'failure' }] })],
  ['log-beside-write', withEffects([{ target: 'metadata.x', value: 1, log: 'and a line' }])],
  ['log-beside-wardrobe', withEffects([{ wardrobe: { action: 'wear', items: ['hat'] }, log: 'and a line' }])],
  ['log-empty', withEffects([{ log: '' }])],
  ['log-1001', withEffects([{ log: 'x'.repeat(1001) }])],
  ['log-1000-astral', withEffects([{ log: '\u{1f600}'.repeat(1000) }])],
  ['log-undeclared-param', withEffects([{ log: 'Rolled {{params.ghost}}' }])],
  ['log-llm-without-block', withEffects([{ log: 'The oracle says {{llm}}' }])],
  ['log-llm-with-block', withEffects([{ log: 'The oracle says {{llm}}' }], LLM_BLOCK)],
  ['log-not-a-string', withEffects([{ log: 5 }])],
  ['effect-empty-four-kinds', withEffects([{}])],
  // === end P4.D331 ===
];

/**
 * Cases whose bytes a TS literal cannot express — fed through `JSON.parse`
 * exactly as `readToolFile` does.
 */
const rawCorpus: Array<[string, string]> = [
  // A later duplicate key wins in both JSON.parse and serde_json — so the
  // catch-all here is the SECOND `when`, and the definition loads.
  ['raw-duplicate-key-last-wins', '{"name":"probe","description":"A probe.","outcomes":[{"when":{"gt":1},"when":true,"message":"-","state":"info"}]}'],
  // A large finite literal: `.finite()` is satisfied, so this is an ordinary
  // accept. Held at 1e15 rather than 1e308 deliberately — above i64::MAX the
  // two runtimes render the SAME number differently (JS `1e+308`, serde
  // `1e308`), which is the pre-existing exotic-number seam documented at
  // `model/tool_wire/parsers.rs`. That seam is not this format's to close, and
  // a row tripping it would report a number-formatting difference as a schema
  // failure. This pins the boundary where the two renderings do agree.
  ['raw-large-finite-operand', '{"name":"probe","description":"A probe.","outcomes":[{"when":{"gt":1e15},"message":"-","state":"info"},{"when":true,"message":"-","state":"info"}]}'],
  // -0 and exponent forms survive the parse as plain numbers.
  ['raw-negative-zero-operand', '{"name":"probe","description":"A probe.","outcomes":[{"when":{"gte":-0.0},"message":"-","state":"info"},{"when":true,"message":"-","state":"info"}]}'],
  // Escapes and non-ASCII in a message, which the template renderer later sees.
  ['raw-unicode-message', '{"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"the caf\\u00e9 \\ud83c\\udfb2 \\"quoted\\"","state":"info"}]}'],
];

function emitDefinition(id: string, text: string): void {
  // Mirrors readToolFile: the file's bytes, through JSON.parse.
  const doc = JSON.parse(text);
  const result = QtapCustomToolSchema.safeParse(doc);
  rows.push({
    kind: 'definition',
    id,
    // The bytes both sides parse — never a re-stringified structure.
    inputJson: text,
    success: result.success,
    // The exact sentence an author reads on the load-error badge, and the exact
    // bytes the custom-tools GET route returns in `errors[]`.
    reason: result.success ? null : formatDefinitionIssues(result.error),
    // The parsed data on success: proves defaults and key order land identically.
    data: result.success ? JSON.stringify(result.data) : null,
    unknownKeys: collectUnknownKeys(doc),
  });
}

for (const [id, doc] of corpus) emitDefinition(id, JSON.stringify(doc));
for (const [id, text] of rawCorpus) emitDefinition(id, text);

// ------------------------------------------------------ the availability gate
// `evaluateToolGate` / `hasToolGate` (P4.d19, v4 `6864bf0e`) driven directly,
// because the roster only ever shows the VERDICT's consequence and the
// asymmetry that matters — an empty sheet fails every `availableWhen` and
// satisfies no `withheldWhen` — is invisible from there.
const AVAILABLE = (metadata: Record<string, unknown>) => ({ ...BASE, availableWhen: { metadata } });
const WITHHELD = (metadata: Record<string, unknown>) => ({ ...BASE, withheldWhen: { metadata } });

/**
 * P4.D169: the flattened progression sheet a gate reads, derived through the
 * REAL engine at a frozen instant and RECORDED on the row, so both sides
 * compare the same sheet. Deriving it on each side would make this a second
 * copy of the engine differential (P4.D167's) rather than a gate one.
 *
 * One character carrying a ten-minute cannon recharge, exactly as v4's own
 * suite poses it.
 */
const PROGRESS_START = Date.parse('2026-09-08T14:00:00Z');
const PROGRESS_METADATA = {
  faction: 'Ordo Aurum',
  progressions: {
    cannon: {
      name: 'Cannon recharge',
      startTime: '2026-09-08T14:00:00Z',
      endTime: '2026-09-08T14:10:00Z',
      timeIncrement: 'minute',
      quantity: { total: 1.0, unit: 'MJ', precision: 1 },
    },
  },
};
const sheetAt = (nowMs: number) => flattenProgressions(PROGRESS_METADATA, nowMs);
const CHARGING = sheetAt(PROGRESS_START + UNIT_MS.minute);
const CHARGED = sheetAt(PROGRESS_START + 11 * UNIT_MS.minute);
const AVAILABLE_PROGRESS = (progress: Record<string, unknown>) => ({ ...BASE, availableWhen: { progress } });
const WITHHELD_PROGRESS = (progress: Record<string, unknown>) => ({ ...BASE, withheldWhen: { progress } });
const CHARGED_GATE = AVAILABLE_PROGRESS({ 'cannon.complete': { eq: true } });

const gateCases: Array<
  [string, unknown, Record<string, unknown> | null, (Record<string, unknown> | null)?]
> = [
  // ---- ungated: every file written before the keys existed --------------
  ['ungated-empty-sheet', BASE, {}],
  ['ungated-null-sheet', BASE, null],
  ['ungated-populated-sheet', BASE, { rank: 3 }],

  // ---- availableWhen: satisfied, unsatisfied, and the fail-CLOSED arms ----
  ['available-contains-holds', AVAILABLE({ toolAbilities: { contains: 'programmable' } }), { toolAbilities: 'programmable, ambulatory' }],
  ['available-contains-misses', AVAILABLE({ toolAbilities: { contains: 'programmable' } }), { toolAbilities: 'ambulatory' }],
  ['available-absent-key-fails-closed', AVAILABLE({ toolAbilities: { contains: 'programmable' } }), {}],
  ['available-null-sheet-fails-closed', AVAILABLE({ toolAbilities: { contains: 'programmable' } }), null],
  ['available-gte-holds', AVAILABLE({ clearance: { gte: 3 } }), { clearance: 3 }],
  ['available-gte-misses', AVAILABLE({ clearance: { gte: 3 } }), { clearance: 2 }],
  ['available-eq-boolean', AVAILABLE({ trusted: { eq: true } }), { trusted: true }],
  ['available-eq-boolean-strict', AVAILABLE({ trusted: { eq: true } }), { trusted: 'true' }],
  ['available-neq-holds', AVAILABLE({ house: { neq: 'Ferro' } }), { house: 'Aurum' }],
  ['available-ncontains-holds', AVAILABLE({ house: { ncontains: 'Ferro' } }), { house: 'Aurum' }],
  ['available-multi-key-all-hold', AVAILABLE({ clearance: { gte: 3 }, house: { eq: 'Aurum' } }), { clearance: 4, house: 'Aurum' }],
  ['available-multi-key-one-misses', AVAILABLE({ clearance: { gte: 3 }, house: { eq: 'Aurum' } }), { clearance: 4, house: 'Ferro' }],
  ['available-multi-key-one-absent', AVAILABLE({ clearance: { gte: 3 }, house: { eq: 'Aurum' } }), { clearance: 4 }],
  ['available-anded-range', AVAILABLE({ clearance: { gte: 3, lt: 9 } }), { clearance: 5 }],
  ['available-anded-range-out', AVAILABLE({ clearance: { gte: 3, lt: 9 } }), { clearance: 9 }],

  // ---- the fail-soft type rules, read through the gate --------------------
  ['available-key-holds-null', AVAILABLE({ clearance: { gte: 3 } }), { clearance: null }],
  ['available-key-holds-array', AVAILABLE({ clearance: { gte: 3 } }), { clearance: [3, 4] }],
  ['available-key-holds-object', AVAILABLE({ clearance: { gte: 3 } }), { clearance: { level: 3 } }],
  ['available-orders-a-string', AVAILABLE({ clearance: { gte: 3 } }), { clearance: '5' }],
  ['available-searches-a-number', AVAILABLE({ house: { contains: 'Aur' } }), { house: 7 }],
  ['available-ncontains-a-number', AVAILABLE({ house: { ncontains: 'Ferro' } }), { house: 7 }],

  // ---- withheldWhen: the OTHER half of the asymmetry ---------------------
  ['withheld-holds', WITHHELD({ novice: { eq: true } }), { novice: true }],
  ['withheld-misses', WITHHELD({ novice: { eq: true } }), { novice: false }],
  ['withheld-absent-key-offers', WITHHELD({ novice: { eq: true } }), {}],
  ['withheld-null-sheet-offers', WITHHELD({ novice: { eq: true } }), null],
  ['withheld-key-holds-array-offers', WITHHELD({ novice: { eq: true } }), { novice: [true] }],
  ['withheld-multi-key-one-misses-offers', WITHHELD({ novice: { eq: true }, probation: { eq: true } }), { novice: true, probation: false }],
  ['withheld-ncontains-holds', WITHHELD({ house: { ncontains: 'Aurum' } }), { house: 'Ferro' }],

  // ---- P4.D169: the progress sheet, through the SAME comparison table -----
  // The weapon that is not offered until it has finished charging — the whole
  // feature in two rows, and the reason a gate is answered before the deal.
  ['progress-withheld-while-charging', CHARGED_GATE, {}, CHARGING],
  ['progress-offered-once-charged', CHARGED_GATE, {}, CHARGED],
  // The same asymmetry the metadata rows above pin, on the new sheet: an
  // availableWhen fails CLOSED where a withheldWhen fails OPEN. Three shapes of
  // "no sheet at all" — empty, absent, null — because a port may treat them
  // differently and v4 does not.
  // (Absent and `null` collapse to one arm on the Rust side, whose third
  // parameter is one `Option`. That is not a hole: these two rows are what
  // PROVE v4 answers them alike, which is the premise the collapse rests on.)
  ['progress-available-empty-sheet-fails-closed', CHARGED_GATE, {}, {}],
  ['progress-available-absent-sheet-fails-closed', CHARGED_GATE, {}],
  ['progress-available-null-sheet-fails-closed', CHARGED_GATE, {}, null],
  ['progress-withheld-empty-sheet-offers', WITHHELD_PROGRESS({ 'cannon.complete': { eq: true } }), {}, {}],
  ['progress-withheld-holds', WITHHELD_PROGRESS({ 'cannon.complete': { eq: true } }), {}, CHARGED],
  // Both sheets in one gate, ANDed: each of the three ways it can fail.
  ['progress-and-metadata-both-hold', { ...BASE, availableWhen: { metadata: { hasAnsibleAccess: { eq: true } }, progress: { 'cannon.complete': { eq: true } } } }, { hasAnsibleAccess: true }, CHARGED],
  ['progress-and-metadata-metadata-misses', { ...BASE, availableWhen: { metadata: { hasAnsibleAccess: { eq: true } }, progress: { 'cannon.complete': { eq: true } } } }, { hasAnsibleAccess: false }, CHARGED],
  ['progress-and-metadata-progress-misses', { ...BASE, availableWhen: { metadata: { hasAnsibleAccess: { eq: true } }, progress: { 'cannon.complete': { eq: true } } } }, { hasAnsibleAccess: true }, CHARGING],
  // The fail-soft rules again, on the derived sheet: an id the character does
  // not carry, a field that does not exist, and an ordering comparator handed a
  // string — none of them throws, all of them decline.
  ['progress-absent-progression-declines', AVAILABLE_PROGRESS({ 'zeppelin.complete': { eq: true } }), {}, CHARGED],
  ['progress-absent-field-declines', AVAILABLE_PROGRESS({ 'cannon.trimester': { eq: 2 } }), {}, CHARGED],
  ['progress-orders-a-string-field-declines', AVAILABLE_PROGRESS({ 'cannon.state': { gt: 1 } }), {}, CHARGING],
  // The derived numeric fields a gate is actually written against.
  ['progress-percent-ordering-holds', AVAILABLE_PROGRESS({ 'cannon.percent': { gte: 50 } }), {}, sheetAt(PROGRESS_START + 5 * UNIT_MS.minute)],
  ['progress-percent-ordering-misses', AVAILABLE_PROGRESS({ 'cannon.percent': { gte: 50 } }), {}, sheetAt(PROGRESS_START + 4 * UNIT_MS.minute)],
  ['progress-percent-uncapped-past-the-end', AVAILABLE_PROGRESS({ 'cannon.percent': { gt: 100 } }), {}, CHARGED],
  ['progress-remaining-goes-negative', AVAILABLE_PROGRESS({ 'cannon.remainingMs': { lt: 0 } }), {}, CHARGED],
  ['progress-state-string-eq', AVAILABLE_PROGRESS({ 'cannon.state': { eq: 'active' } }), {}, CHARGING],
  ['progress-state-string-eq-before-start', AVAILABLE_PROGRESS({ 'cannon.state': { eq: 'active' } }), {}, sheetAt(PROGRESS_START - UNIT_MS.minute)],
  ['progress-end-time-is-epoch-ms', AVAILABLE_PROGRESS({ 'cannon.endTime': { gt: PROGRESS_START } }), {}, sheetAt(PROGRESS_START)],
  // An authored-but-EMPTY record beside a real one: it tests nothing, so it
  // cannot withhold, and the gate answers on the other sheet alone.
  ['progress-empty-record-beside-metadata', { ...BASE, availableWhen: { metadata: { rank: { gte: 1 } }, progress: {} } }, { rank: 2 }, CHARGING],

  // === P4.D333 rows (append-only; P4.D329 / P4.D331 merge BY ID) ===
  // v4 `a53024d3c`: an any-of clause names the group that held — `heldGroup`
  // on an opening `availableWhen` and a closing `withheldWhen`, ABSENT on an
  // availableWhen miss and for any bare-object clause. Key order: `available,
  // withheldBy, heldGroup`.
  ['anyof-available-opens-on-0', { ...BASE, availableWhen: [{ metadata: { clearance: { gte: 3 } } }, { metadata: { house: { eq: 'Aurum' } } }] }, { clearance: 4, house: 'Aurum' }],
  ['anyof-available-opens-on-1', { ...BASE, availableWhen: [{ metadata: { clearance: { gte: 3 } } }, { metadata: { house: { eq: 'Aurum' } } }] }, { clearance: 1, house: 'Aurum' }],
  ['anyof-available-opens-on-none', { ...BASE, availableWhen: [{ metadata: { clearance: { gte: 3 } } }, { metadata: { house: { eq: 'Aurum' } } }] }, { clearance: 1, house: 'Ferro' }],
  ['anyof-available-one-element-array-opens-on-0', { ...BASE, availableWhen: [{ metadata: { clearance: { gte: 3 } } }] }, { clearance: 4 }],
  ['anyof-available-bare-object-no-held-group', { ...BASE, availableWhen: { metadata: { clearance: { gte: 3 } } } }, { clearance: 4 }],
  ['anyof-withheld-closes-on-1', { ...BASE, withheldWhen: [{ metadata: { novice: { eq: true } } }, { metadata: { probation: { eq: true } } }] }, { novice: false, probation: true }],
  ['anyof-withheld-closes-on-0', { ...BASE, withheldWhen: [{ metadata: { novice: { eq: true } } }, { metadata: { probation: { eq: true } } }] }, { novice: true, probation: true }],
  ['anyof-withheld-closes-on-none-offers', { ...BASE, withheldWhen: [{ metadata: { novice: { eq: true } } }, { metadata: { probation: { eq: true } } }] }, { novice: false, probation: false }],
  ['anyof-withheld-bare-object-no-held-group', { ...BASE, withheldWhen: { metadata: { novice: { eq: true } } } }, { novice: true }],
  ['anyof-available-empty-sheet-fails-closed', { ...BASE, availableWhen: [{ metadata: { a: { eq: 1 } } }, { metadata: { b: { eq: 1 } } }] }, {}],
  ['anyof-available-null-sheet-fails-closed', { ...BASE, availableWhen: [{ metadata: { a: { eq: 1 } } }, { metadata: { b: { eq: 1 } } }] }, null],
  ['anyof-withheld-empty-sheet-offers', { ...BASE, withheldWhen: [{ metadata: { a: { eq: 1 } } }, { metadata: { b: { eq: 1 } } }] }, {}],
  ['anyof-progress-group-opens-on-1', { ...BASE, availableWhen: [{ metadata: { hasAnsibleAccess: { eq: true } } }, { progress: { 'cannon.complete': { eq: true } } }] }, { hasAnsibleAccess: false }, CHARGED],
  ['anyof-both-sheets-in-one-group-and-other-group-bare', { ...BASE, availableWhen: [{ metadata: { rank: { gte: 5 } } }, { metadata: { hasAnsibleAccess: { eq: true } }, progress: { 'cannon.complete': { eq: true } } }] }, { rank: 1, hasAnsibleAccess: true }, CHARGED],
  // The list rule (`metadata-match.ts`, `a53024d3c`) read through the gate: a
  // key holding a list of primitives compares entry by entry.
  ['anyof-available-list-eq-holds', { ...BASE, availableWhen: { metadata: { knownLanguages: { eq: 'Trade Cant' } } } }, { knownLanguages: ['Trade Cant', 'Latin'] }],
  ['anyof-available-list-contains-holds', { ...BASE, availableWhen: { metadata: { knownLanguages: { contains: 'Cant' } } } }, { knownLanguages: ['Trade Cant', 7] }],
  ['anyof-available-list-ordering-declines', { ...BASE, availableWhen: { metadata: { knownLanguages: { gte: 1 } } } }, { knownLanguages: [1, 2] }],
  ['anyof-available-list-of-objects-declines', { ...BASE, availableWhen: { metadata: { sealedOrders: { eq: 'red' } } } }, { sealedOrders: [{ seal: 'red' }] }],
  // === end P4.D333 rows ===
];

// === P4.D329: gate rows posed against an Inventory bag (item 8) ===
// v4 `custom-tools-inventory.test.ts`'s `the gate` (swing / flee over
// `vivienne()`), the measure-twice tape and kind-name gates (`cad017599`'s
// pure twins), and `inventory-aggregate.test.ts`'s null-context group — each
// bag v4's REAL `inventoryContextFrom` context with its answers RECORDED, or
// the bench's mock rows, or `null`.
const SWING = { ...BASE, availableWhen: { inventory: { sword: { eq: 'equipped' } } } };
const FLEE = { ...BASE, withheldWhen: { inventoryCount: { ring: { gte: 2 } } } };
const TITLE = "Friday's Yellow Tape Measure";
const tapeCtx = (holder: 'friday' | 'ottoline') =>
  contextOver(holder === 'friday' ? 'c-friday' : 'c-ottoline', 'chat-1', {
    items: [
      obj('tape-arch', { name: 'tape measure', isArchetype: true, metadata: { length_feet: 16 } }),
      holder === 'friday'
        ? obj('tape-friday', { name: 'tape measure', title: TITLE, parentId: 'tape-arch', metadata: { length_feet: 25 } })
        : obj('tape-ottoline', { name: 'tape measure', parentId: 'tape-arch', metadata: { length_feet: 16 } }),
      obj('res-arch', { name: 'ansible resonator', isArchetype: true }),
      obj('res-1', { name: 'ansible resonator', parentId: 'res-arch' }),
      obj('res-2', { name: 'ansible resonator', title: "Charlie's Resonator", parentId: 'res-arch' }),
      obj('walkie', { name: 'ansible walkie-talkie' }),
      obj('spy-arch', { name: 'ansible spyglass', isArchetype: true }),
    ],
    own: ['tape-friday', 'tape-ottoline', 'res-1', 'res-2', 'walkie'],
    equipped: holder === 'friday' ? ['tape-friday', 'res-1', 'res-2', 'walkie'] : ['tape-ottoline'],
  });
const MEASURE = { ...BASE, availableWhen: { inventory: { [TITLE]: { eq: 'equipped' } } } };
const PROBE = (ref: string) => ({ ...BASE, availableWhen: { inventory: { [ref]: { eq: 'equipped' } } } });
type GateBag = { recorded: () => InventoryContext } | { rows: MockInventoryRow[] } | null | undefined;
const recordedOf = (make: () => InventoryContext): GateBag => ({ recorded: make });
const inventoryGateCases: Array<[string, unknown, GateBag]> = [
  ['inventory-gate-swing-vivienne', SWING, recordedOf(() => contextOver('c-viv', 'chat-1', vivienneItems()))],
  ['inventory-gate-swing-empty-mock', SWING, { rows: [] }],
  ['inventory-gate-swing-null', SWING, null],
  ['inventory-gate-swing-absent', SWING, undefined],
  ['inventory-gate-swing-mock-rows', SWING, { rows: [{ ref: 'Sword', state: 'equipped' }] }],
  ['inventory-gate-flee-vivienne', FLEE, recordedOf(() => contextOver('c-viv', 'chat-1', vivienneItems()))],
  ['inventory-gate-flee-null-offers', FLEE, null],
  ['inventory-gate-flee-mock-count-2', FLEE, { rows: [{ ref: 'ring', state: 'equipped', count: 2 }] }],
  ['inventory-gate-flee-mock-count-1', FLEE, { rows: [{ ref: 'ring', state: 'equipped', count: 1 }] }],
  ['inventory-gate-count-lt1-null-closed', { ...BASE, availableWhen: { inventoryCount: { axe: { lt: 1 } } } }, null],
  ['inventory-gate-count-lt1-vivienne', { ...BASE, availableWhen: { inventoryCount: { axe: { lt: 1 } } } }, recordedOf(() => contextOver('c-viv', 'chat-1', vivienneItems()))],
  ['inventory-gate-absent-eq-null-closed', { ...BASE, availableWhen: { inventory: { axe: { eq: 'absent' } } } }, null],
  ['inventory-gate-neq-vivienne', { ...BASE, withheldWhen: { inventory: { lantern: { neq: 'carried' } } } }, recordedOf(() => contextOver('c-viv', 'chat-1', vivienneItems()))],
  ['measure-twice-gate-friday-opens', MEASURE, recordedOf(() => tapeCtx('friday'))],
  ['measure-twice-gate-ottoline-closed', MEASURE, recordedOf(() => tapeCtx('ottoline'))],
  ['measure-twice-gate-null-closed', MEASURE, null],
  ['measure-twice-gate-kind-name-friday', PROBE('tape measure'), recordedOf(() => tapeCtx('friday'))],
  ['measure-twice-gate-kind-name-ottoline', PROBE('tape measure'), recordedOf(() => tapeCtx('ottoline'))],
  ['measure-twice-gate-resonator-kind', PROBE('ansible resonator'), recordedOf(() => tapeCtx('friday'))],
  ['measure-twice-gate-resonator-title', PROBE("Charlie's Resonator"), recordedOf(() => tapeCtx('friday'))],
  ['measure-twice-gate-walkie', PROBE('ansible walkie-talkie'), recordedOf(() => tapeCtx('friday'))],
  ['measure-twice-gate-spyglass-closed', PROBE('ansible spyglass'), recordedOf(() => tapeCtx('friday'))],
  ['aggregate-gate-null-closed', { ...BASE, availableWhen: { inventoryAggregate: { 'equipped.count': { gte: 0 } } } }, null],
  ['aggregate-gate-null-withheld-offers', { ...BASE, withheldWhen: { inventoryAggregate: { 'equipped.count': { gte: 0 } } } }, null],
  ['aggregate-gate-count-vivienne', { ...BASE, availableWhen: { inventoryAggregate: { 'equipped.count': { gte: 3 } } } }, recordedOf(() => contextOver('c-viv', 'chat-1', vivienneItems()))],
  ['aggregate-gate-fold-vivienne', { ...BASE, withheldWhen: { inventoryAggregate: { 'owned.metadata.toolVersion.max': { gte: 5 } } } }, recordedOf(() => contextOver('c-viv', 'chat-1', vivienneItems()))],
  ['aggregate-gate-mock-count', { ...BASE, availableWhen: { inventoryAggregate: { 'reachable.count': { eq: 2 } } } }, { rows: [{ ref: 'rope', state: 'reachable' }, { ref: 'cup', state: 'carried' }, { ref: 'ghost', state: 'absent' }] }],
  ['inventory-gate-all-three-vivienne', { ...BASE, availableWhen: { inventory: { sword: { eq: 'equipped' } }, inventoryCount: { ring: { eq: 2 } }, inventoryAggregate: { 'equipped.metadata.bonus.sum': { eq: 3 } } } }, recordedOf(() => contextOver('c-viv', 'chat-1', vivienneItems()))],
  ['inventory-gate-metadata-first-declines', { ...BASE, availableWhen: { metadata: { rank: { gte: 9 } }, inventory: { sword: { eq: 'equipped' } } } }, recordedOf(() => contextOver('c-viv', 'chat-1', vivienneItems()))],
];
for (const [id, definition, bag] of inventoryGateCases) {
  const parsed = QtapCustomToolSchema.safeParse(definition);
  if (!parsed.success) throw new Error(`gate case '${id}': definition does not load`);
  let inventory: unknown;
  let ctx: InventoryContext | null | undefined;
  let recorded: (() => unknown) | null = null;
  if (bag === undefined) {
    ctx = undefined;
  } else if (bag === null) {
    ctx = null;
    inventory = null;
  } else if ('rows' in bag) {
    ctx = mockInventoryContext(bag.rows);
    inventory = { rows: bag.rows };
  } else {
    const r = recording(bag.recorded());
    ctx = r.ctx;
    recorded = () => ({ recorded: r.recorded });
  }
  const verdict = JSON.stringify(evaluateToolGate(parsed.data, {}, undefined, ctx));
  if (recorded) inventory = recorded();
  rows.push({
    kind: 'gate',
    id,
    inputJson: JSON.stringify(definition),
    metadata: {},
    ...(inventory === undefined ? {} : { inventory }),
    hasGate: hasToolGate(parsed.data),
    verdict,
  });
}
// === end P4.D329 gate rows ===

for (const [id, definition, metadata, progress] of gateCases) {
  const parsed = QtapCustomToolSchema.safeParse(definition);
  if (!parsed.success) throw new Error(`gate case '${id}': definition does not load`);
  rows.push({
    kind: 'gate',
    id,
    // The bytes both sides parse, exactly as the definition rows travel.
    inputJson: JSON.stringify(definition),
    metadata,
    // P4.D169: the flattened sheet, RECORDED rather than re-derived, and
    // deliberately `undefined` on a row that poses none — v4's third argument
    // is optional, and "absent" is a different arm from "empty" and "null".
    ...(progress === undefined ? {} : { progress }),
    hasGate: hasToolGate(parsed.data),
    // The VERDICT object as it is serialized — `withheldBy` must be ABSENT, not
    // null, whenever the tool is available.
    verdict:
      progress === undefined
        ? JSON.stringify(evaluateToolGate(parsed.data, metadata))
        : JSON.stringify(evaluateToolGate(parsed.data, metadata, progress)),
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
