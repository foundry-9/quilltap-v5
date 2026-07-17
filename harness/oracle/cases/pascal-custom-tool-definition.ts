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
 * Run from inside the server checkout (v4 @ d68638b4, Node 24):
 *   cd ~/source/quilltap-server
 *   npx tsx \
 *     ~/source/quilltap-v5/.claude/worktrees/pascal-custom-tools-porting-243efb/harness/oracle/cases/pascal-custom-tool-definition.ts \
 *     > /tmp/oracle-pascal-definition.ndjson
 */

import {
  QtapCustomToolSchema,
  collectUnknownKeys,
  displayTitle,
  formatDefinitionIssues,
  MAX_DESCRIPTION_LENGTH,
  MAX_MESSAGE_LENGTH,
  MAX_OUTCOMES,
  MAX_TITLE_LENGTH,
} from '@/lib/pascal/custom-tool.types';

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

const NUM_PARAM = { parameters: { scale: { type: 'number', default: 1 } } };
const INT_PARAM = { parameters: { steps: { type: 'integer', default: 2 } } };
const STR_PARAM = { parameters: { material: { type: 'string', default: 'brass' } } };
const BOOL_PARAM = { parameters: { lit: { type: 'boolean', default: true } } };

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

  // ---- shape / non-object -------------------------------------------------
  ['not-an-object', 'nope'],
  ['null-doc', null],
  ['array-doc', []],
  ['empty-object', {}],
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

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
