/**
 * Oracle case: effect EXPRESSIONS — the closed tokenizer/parser/evaluator
 * behind an effect's `value` (P4.D35, v4 `c4d4b0de`).
 *
 * Drives the REAL code:
 *   parseExpression, evaluateExpression, formatValue  (lib/pascal/expressions.ts)
 *
 * ## Why this family exists rather than riding the definition corpus
 *
 * The definition corpus reaches `parseExpression` (a bad `value` is a load-time
 * rejection, and its `reason` is spliced into the user-visible rejection
 * sentence), but it cannot reach `evaluateExpression` at all — evaluation only
 * happens at RUN time, inside `resolveEffects`, where each reason is buried in
 * one skip string on one effect of one run. Covering the evaluator's whole
 * error vocabulary that way would need a definition and a staged run per
 * sentence. Driving the two functions directly is the same discipline the
 * `metadata-match` and `dice` families already use for pure leaves.
 *
 * ## Every reason string is byte-compared, not fragment-matched
 *
 * v4's own `expressions.test.ts` asserts `reason.toLowerCase()).toContain(…)`.
 * That is a weaker bar than this port needs: parse reasons ARE user-visible
 * payload (they flow through `formatDefinitionIssues` into the route's
 * `errors[]`), and eval reasons are recorded verbatim in
 * `pascalMeta.effects`-adjacent skip records the Proving Bench displays. So
 * every `reason` here is pinned whole, including the bare-word quoting trap and
 * the position-bearing tokenizer sentences.
 *
 * ## Positions are UTF-16 code units
 *
 * The tokenizer indexes a JS string, so `at position N` counts UTF-16 code
 * units. The corpus carries non-ASCII and astral rows deliberately — a Rust
 * port that walked `char`s or bytes would pass every ASCII row and fail these.
 *
 * Run from inside the server checkout (v4 @ c4d4b0de, Node 24
 * `~/.nvm/versions/node/v24.13.1/bin`):
 *   cd ~/source/quilltap-server
 *   TZ=UTC npx tsx \
 *     <V5W>/harness/oracle/cases/pascal-expressions.ts \
 *     > /tmp/oracle-pascal-expressions.ndjson
 */

import {
  parseExpression,
  evaluateExpression,
  formatValue,
  MAX_EFFECT_EXPRESSION_LENGTH,
  MAX_EFFECT_EXPRESSION_TOKENS,
  MAX_EFFECT_EXPRESSION_DEPTH,
  type ExprValue,
} from '@/lib/pascal/expressions';

const rows: unknown[] = [];

// ---------------------------------------------------------------- parse rows
//
// Both the accepted and the rejected sources ride one list: the row records
// `ok`, and then either the ref list (acceptance) or the whole reason string
// (rejection). Splitting them would let a port that accepts a source v4 rejects
// pass by simply having no row.
const parseCases: Array<[string, string]> = [
  // ---- acceptance: every literal kind, every ref family ------------------
  ['number-int', '1'],
  ['number-float', '1.5'],
  ['number-leading-zeros', '007'],
  ['string-single', "'broken pick'"],
  ['string-double', '"double quoted"'],
  ['string-empty', "''"],
  ['string-escaped-quote', "'it\\'s'"],
  ['string-escaped-double', '"say \\"hi\\""'],
  ['string-escaped-backslash', "'a\\\\b'"],
  ['string-escapes-are-literal-not-c', "'a\\nb'"],
  ['boolean-true', 'true'],
  ['boolean-false', 'false'],
  ['ref-value', '{{value}}'],
  ['ref-roll', '{{roll}}'],
  ['ref-dice', '{{dice}}'],
  ['ref-llm', '{{llm}}'],
  ['ref-params', '{{params.bonus}}'],
  ['ref-metadata', '{{metadata.lockpicks}}'],
  ['ref-metadata-dotted-key', '{{metadata.gear.rope}}'],
  ['ref-state-path', '{{state.encounter.count}}'],
  ['ref-state-bracket-index', '{{state.party[0].hp}}'],
  ['ref-inner-whitespace-trimmed', '{{  value  }}'],
  // `refs` is first-appearance order, DEDUPLICATED. Both properties need a row
  // that repeats a name — without one, a port that simply pushed every ref it
  // walked would pass the whole corpus (proven: that mutation went undetected
  // until these rows existed).
  ['refs-repeated-deduped', '{{state.a}} + {{params.b}} + {{state.a}}'],
  ['refs-order-is-first-appearance', '{{params.z}} * {{params.a}} + {{params.z}}'],
  ['refs-repeated-across-negation-and-parens', '-{{value}} + ({{value}} * {{roll}}) - {{roll}}'],
  ['precedence', '1 + 2 * 3'],
  ['parens', '({{value}} + 1) * 2'],
  ['negate-ref-and-concat', "-{{state.debt}} / 2 + 'gp'"],
  ['double-negate', '--2'],
  ['whitespace-tabs-newlines', '1\t+\n2\r+ 3'],
  ['depth-at-cap', `${'('.repeat(MAX_EFFECT_EXPRESSION_DEPTH)}1${')'.repeat(MAX_EFFECT_EXPRESSION_DEPTH)}`],
  ['tokens-at-cap', Array.from({ length: MAX_EFFECT_EXPRESSION_TOKENS / 2 }, () => '1').join('+')],
  ['length-at-cap', `'${'x'.repeat(MAX_EFFECT_EXPRESSION_LENGTH - 2)}'`],

  // ---- rejection: the whole error vocabulary -----------------------------
  ['empty', ''],
  ['whitespace-only', '   '],
  ['bare-word', 'broken pick'],
  ['bare-word-call', 'foo(1)'],
  ['bare-word-member', 'a.b'],
  ['bare-word-underscore', '_x'],
  ['ref-unknown-family', '{{nonsense}}'],
  ['ref-empty-prefix-params', '{{params.}}'],
  ['ref-empty-prefix-metadata', '{{metadata.}}'],
  ['ref-empty-prefix-state', '{{state.}}'],
  ['ref-empty-name', '{{}}'],
  ['ref-never-closes', '{{value'],
  ['ref-lone-brace', '{value}'],
  ['ref-lone-brace-mid', '1 + {value}'],
  ['string-unterminated', "'unterminated"],
  ['string-unterminated-double', '"unterminated'],
  ['string-dangling-backslash', "'abc\\"],
  ['ends-where-value-expected', '1 +'],
  ['paren-never-closes', '(1 + 2'],
  ['unexpected-after-end', '1 2'],
  ['unexpected-string-after-end', "1 'x'"],
  ['unexpected-ref-after-end', '1 {{value}}'],
  ['unexpected-bool-after-end', '1 true'],
  ['unexpected-lparen-after-end', '1 (2)'],
  ['unexpected-char-percent', '1 % 2'],
  ['rparen-where-value-expected', ')'],
  ['op-where-value-expected', '* 2'],
  ['op-div-where-value-expected', '/ 2'],
  ['op-plus-where-value-expected', '+ 2'],
  ['length-over-cap', `'${'x'.repeat(MAX_EFFECT_EXPRESSION_LENGTH)}'`],
  ['tokens-over-cap', Array.from({ length: MAX_EFFECT_EXPRESSION_TOKENS / 2 + 1 }, () => '1').join('+')],
  ['depth-over-cap', `${'('.repeat(MAX_EFFECT_EXPRESSION_DEPTH + 1)}1${')'.repeat(MAX_EFFECT_EXPRESSION_DEPTH + 1)}`],

  // ---- UTF-16 position arithmetic ----------------------------------------
  // A port indexing bytes or `char`s reports a different position for each of
  // these; every one of them is ASCII-clean up to the offending unit.
  ['position-non-ascii-first', 'é#'],
  ['position-non-ascii-offset', '1 é'],
  ['position-after-accented-string', "'café' % 1"],
  ['position-after-astral-string', "'🎲' % 1"],
  ['position-lone-brace-after-astral', "'🎲' + {x}"],
  ['position-ref-never-closes-after-astral', "'🎲' + {{value"],
];

for (const [id, source] of parseCases) {
  const result = parseExpression(source);
  rows.push({
    kind: 'parse',
    id,
    source,
    ok: result.ok,
    ...(result.ok ? { refs: result.expr.refs } : { reason: result.reason }),
  });
}

// ----------------------------------------------------------------- eval rows
//
// `refs` is the fixture table the resolver reads; an absent key resolves to
// `undefined`, which is the fail-soft path.
const REFS: Record<string, ExprValue> = {
  value: 10,
  roll: 4,
  dice: '3d6: [1, 2, 3] = 6',
  llm: '21',
  'params.bonus': 2,
  'params.label': 'lockpick',
  'params.flag': true,
  'metadata.lockpicks': 3,
  'state.floor': 1,
  'state.debt': 50,
  'state.name': 'Aurum',
};

const evalCases: Array<[string, string]> = [
  // ---- arithmetic --------------------------------------------------------
  ['precedence-mul', '1 + 2 * 3'],
  ['precedence-div', '10 - 6 / 2'],
  ['parens-override', '(1 + 2) * 3'],
  ['assoc-sub', '10 - 3 - 2'],
  ['assoc-div', '12 / 3 / 2'],
  ['negate-literal', '-3 + 1'],
  ['negate-ref', '-{{value}}'],
  ['double-negate', '--2'],
  ['float-result', '1 / 3'],
  ['negative-zero', '-0'],

  // ---- non-finite results are eval failures ------------------------------
  ['div-by-zero', '1 / 0'],
  ['zero-div-zero', '0 / 0'],
  ['negate-string', "-'text'"],

  // ---- concatenation -----------------------------------------------------
  ['concat-left-string', "'rolled ' + 3"],
  ['concat-right-string', "3 + ' rolled'"],
  ['concat-both-strings', "'a' + 'b'"],
  ['concat-number-formatting', "'' + 0.123456"],
  ['concat-integer-formatting', "'' + 7"],
  ['concat-bool-right', "'flag: ' + true"],
  ['concat-bool-left', "false + '!'"],
  ['concat-chained', "'a' + 1 + 2"],
  ['concat-chained-numbers-first', "1 + 2 + 'a'"],

  // ---- no truthiness -----------------------------------------------------
  ['bool-plus-number', 'true + 1'],
  ['bool-plus-bool', 'true + false'],
  ['bool-times-number', 'true * 2'],
  ['string-minus-number', "'a' - 1"],
  ['string-times-number', "'a' * 2"],
  ['number-div-string', "4 / 'b'"],
  ['bool-minus-bool', 'true - false'],

  // ---- references --------------------------------------------------------
  ['refs-every-numeric-family', '{{value}} + {{params.bonus}} + {{state.floor}}'],
  ['ref-dice-is-a-string', "'rolled ' + {{dice}}"],
  ['ref-llm-never-coerced', '{{llm}} * 2'],
  ['ref-llm-concatenates', "'said ' + {{llm}}"],
  ['ref-bool-param-concat', "'flag ' + {{params.flag}}"],
  ['ref-unresolved-metadata', '{{metadata.absent}} + 1'],
  ['ref-unresolved-state', '{{state.absent}} + 1'],
  ['ref-unresolved-alone', '{{params.missing}}'],
  ['ref-roll-arithmetic', '{{roll}} * 2 + {{params.bonus}}'],
  ['ref-string-state-concat', "{{state.name}} + ' owes ' + {{state.debt}}"],

  // ---- literals evaluate to themselves -----------------------------------
  ['literal-number', '42'],
  ['literal-string', "'broken pick'"],
  ['literal-bool', 'true'],
  ['literal-escaped-quote', "'it\\'s'"],
  ['literal-escaped-double', '"say \\"hi\\""'],
];

for (const [id, source] of evalCases) {
  const parsedResult = parseExpression(source);
  if (!parsedResult.ok) throw new Error(`eval case '${id}': fixture failed to parse: ${parsedResult.reason}`);
  const result = evaluateExpression(parsedResult.expr, (name) => REFS[name]);
  rows.push({
    kind: 'eval',
    id,
    source,
    ok: result.ok,
    // The VALUE as JSON text, so the number/string/boolean distinction (and
    // JS number rendering) survives the trip rather than being retyped.
    ...(result.ok ? { valueJson: JSON.stringify(result.value) } : { reason: result.reason }),
  });
}

// ----------------------------------------------------------- formatValue rows
//
// The convention `{{value}}` and a concatenated number must share. Pinned here
// because expressions.ts is now its home (v4 moved it out of custom-tools.ts
// and re-exports it); the execution corpus pins the template side.
const formatCases: Array<[string, number]> = [
  ['integer', 14],
  ['zero', 0],
  ['negative-integer', -7],
  ['four-sig-digits', 0.71349],
  ['rounds-up', 0.123456],
  ['trailing-zeros-folded', 1.5],
  ['large-integer', 1234567],
  ['small-float', 0.000123456],
  ['big-float', 123456.789],
  ['negative-float', -2.34567],
];

for (const [id, input] of formatCases) {
  rows.push({ kind: 'format', id, input, output: formatValue(input) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
