/**
 * The effect-expression parser/evaluator twin, pinned against v4's REAL
 * `lib/pascal/expressions.ts` at `c4d4b0de` (P4.D36 unit 1).
 *
 * There are THREE copies of this grammar — v4's TS, this browser port, and the
 * Rust twin P4.D35 carries — and v4's is the oracle for both ports. The
 * sentences matter as much as the verdicts: a `reason` here reaches an author
 * through `formatDefinitionIssues` (which the SERVER also renders for the same
 * file), through the Workbench's side-effect audit, and through a skipped
 * effect's recorded reason. A browser that phrased one differently would be
 * disagreeing with the server about the same document.
 *
 * So this is the same stand-in shape `custom-tool-types.state.spec.ts` and
 * `.llm.spec.ts` use, with the same teeth: every row below was CAPTURED by
 * driving v4's own `parseExpression` / `evaluateExpression` / `formatValue`
 * over the source text, and is byte-compared here. The committed §C corpus is
 * P4.D35's to regenerate; when its new expression-rejection rows land, the
 * corpus replay covers this vocabulary a second time, through the schema.
 *
 * Regen recipe (the capture script is scratch, the ROWS are the artifact):
 *
 * ```ts
 * // in ~/source/quilltap-server @ c4d4b0de, `npx tsx <script>` with Node 24:
 * import { parseExpression, evaluateExpression, formatValue } from '@/lib/pascal/expressions'
 * const p = parseExpression(source)
 * ({ ok: p.ok, reason: p.ok ? null : p.reason, refs: p.ok ? p.expr.refs : null })
 * // eval rows additionally: evaluateExpression(p.expr, (name) => REFS[name])
 * ```
 *
 * The `REFS` table the eval rows resolve against is {@link REFS} below — it is
 * part of the captured contract, not a convenience.
 *
 * Two findings the capture surfaced, recorded rather than inferred:
 *
 *  - the grammar has **no exponent literal**: `1e0` tokenizes `1` then the bare
 *    word `e0`, and fails with the quoting-trap sentence (`reject-exponent-
 *    literal`). Every number is `\d+(\.\d+)?`.
 *  - `parseExpression('   ')` reports `the expression is empty` from the
 *    TOKEN-count guard, not the source-length one — whitespace-only source is
 *    non-empty but tokenizes to nothing (`whitespace-only`).
 *
 * v4 anchors: `lib/pascal/expressions.ts` — `tokenize`, `Parser.factor`,
 * `describeToken`, `evaluateNode`, `finiteOrFail`, `formatValue`. v4's own
 * suite (`__tests__/unit/lib/pascal/expressions.test.ts`, 213 lines) is ported
 * case-for-case at the end of this file; it asserts fragments where these rows
 * assert bytes, so both are kept.
 */

import { describe, expect, it } from 'vitest';

import {
  MAX_EFFECT_EXPRESSION_DEPTH,
  MAX_EFFECT_EXPRESSION_LENGTH,
  MAX_EFFECT_EXPRESSION_TOKENS,
  evaluateExpression,
  formatValue,
  parseExpression,
  type ExprValue,
} from './expressions';

/** The subjects the captured eval rows resolve against — v4's capture table. */
const REFS: Record<string, ExprValue> = {
  value: 10,
  roll: 4,
  dice: '3d6: [1, 2, 3] = 6',
  llm: '21',
  'params.bonus': 2,
  'params.label': 'Jackie',
  'params.flag': true,
  'state.floor': 1,
  'state.encounter.count': 7,
  'metadata.lockpicks': 3,
  'metadata.house': 'Aurum',
  'params.huge': 1e308,
};

/** One captured parse verdict. */
interface ParseRow {
  kind: 'parse';
  id: string;
  source: string;
  ok: boolean;
  reason: string | null;
  refs: string[] | null;
}

/** One captured evaluation, against {@link REFS}. */
interface EvalRow {
  kind: 'eval';
  id: string;
  source: string;
  ok: boolean;
  value: ExprValue | null;
  reason: string | null;
}

/** One captured {@link formatValue} rendering. */
interface FormatRow {
  kind: 'format';
  id: string;
  input: number;
  out: string;
}

type Row = ParseRow | EvalRow | FormatRow;

const CAPTURED: readonly Row[] = [
  {"kind": "parse", "id": "int-literal", "source": "1", "ok": true, "reason": null, "refs": []},
  {"kind": "parse", "id": "float-literal", "source": "1.5", "ok": true, "reason": null, "refs": []},
  {"kind": "parse", "id": "single-quoted", "source": "'broken pick'", "ok": true, "reason": null, "refs": []},
  {"kind": "parse", "id": "double-quoted", "source": "\"double quoted\"", "ok": true, "reason": null, "refs": []},
  {"kind": "parse", "id": "true", "source": "true", "ok": true, "reason": null, "refs": []},
  {"kind": "parse", "id": "false", "source": "false", "ok": true, "reason": null, "refs": []},
  {"kind": "parse", "id": "ref-value", "source": "{{value}}", "ok": true, "reason": null, "refs": ["value"]},
  {"kind": "parse", "id": "ref-roll", "source": "{{roll}}", "ok": true, "reason": null, "refs": ["roll"]},
  {"kind": "parse", "id": "ref-dice", "source": "{{dice}}", "ok": true, "reason": null, "refs": ["dice"]},
  {"kind": "parse", "id": "ref-llm", "source": "{{llm}}", "ok": true, "reason": null, "refs": ["llm"]},
  {"kind": "parse", "id": "ref-params", "source": "{{params.bonus}}", "ok": true, "reason": null, "refs": ["params.bonus"]},
  {"kind": "parse", "id": "ref-metadata", "source": "{{metadata.lockpicks}}", "ok": true, "reason": null, "refs": ["metadata.lockpicks"]},
  {"kind": "parse", "id": "ref-state-nested", "source": "{{state.encounter.count}}", "ok": true, "reason": null, "refs": ["state.encounter.count"]},
  {"kind": "parse", "id": "precedence", "source": "1 + 2 * 3", "ok": true, "reason": null, "refs": []},
  {"kind": "parse", "id": "parens", "source": "({{value}} + 1) * 2", "ok": true, "reason": null, "refs": ["value"]},
  {"kind": "parse", "id": "negate-ref-concat", "source": "-{{state.debt}} / 2 + 'gp'", "ok": true, "reason": null, "refs": ["state.debt"]},
  {"kind": "parse", "id": "ref-padded", "source": "{{  value  }}", "ok": true, "reason": null, "refs": ["value"]},
  {"kind": "parse", "id": "refs-deduped", "source": "{{state.a}} + {{params.b}} + {{state.a}}", "ok": true, "reason": null, "refs": ["state.a", "params.b"]},
  {"kind": "parse", "id": "escaped-single", "source": "'it\\'s'", "ok": true, "reason": null, "refs": []},
  {"kind": "parse", "id": "escaped-double", "source": "\"say \\\"hi\\\"\"", "ok": true, "reason": null, "refs": []},
  {"kind": "parse", "id": "depth-at-cap", "source": "((((((((1))))))))", "ok": true, "reason": null, "refs": []},
  {"kind": "parse", "id": "tokens-at-cap", "source": "1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1", "ok": true, "reason": null, "refs": []},
  {"kind": "parse", "id": "length-at-cap", "source": "'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'", "ok": true, "reason": null, "refs": []},
  {"kind": "parse", "id": "whitespace-only", "source": "   ", "ok": false, "reason": "the expression is empty", "refs": null},
  {"kind": "parse", "id": "reject-empty", "source": "", "ok": false, "reason": "the expression is empty", "refs": null},
  {"kind": "parse", "id": "reject-length", "source": "'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'", "ok": false, "reason": "the expression exceeds 500 characters", "refs": null},
  {"kind": "parse", "id": "reject-tokens", "source": "1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1", "ok": false, "reason": "the expression exceeds 64 tokens", "refs": null},
  {"kind": "parse", "id": "reject-bare-word", "source": "broken pick", "ok": false, "reason": "\"broken\" is a bare word — there are no identifiers here. Quote literal text ('broken') or use a {{ref}}", "refs": null},
  {"kind": "parse", "id": "reject-call", "source": "foo(1)", "ok": false, "reason": "\"foo\" is a bare word — there are no identifiers here. Quote literal text ('foo') or use a {{ref}}", "refs": null},
  {"kind": "parse", "id": "reject-dotted-word", "source": "a.b", "ok": false, "reason": "\"a\" is a bare word — there are no identifiers here. Quote literal text ('a') or use a {{ref}}", "refs": null},
  {"kind": "parse", "id": "reject-unknown-ref", "source": "{{nonsense}}", "ok": false, "reason": "{{nonsense}} is not a reference this grammar knows — use value, roll, dice, llm, params.x, metadata.key, or state.path", "refs": null},
  {"kind": "parse", "id": "reject-empty-param-ref", "source": "{{params.}}", "ok": false, "reason": "{{params.}} is not a reference this grammar knows — use value, roll, dice, llm, params.x, metadata.key, or state.path", "refs": null},
  {"kind": "parse", "id": "reject-unclosed-ref", "source": "{{value", "ok": false, "reason": "the reference starting at position 0 never closes with \"}}\"", "refs": null},
  {"kind": "parse", "id": "reject-lone-brace", "source": "{value}", "ok": false, "reason": "a reference opens with \"{{\", not a lone \"{\" (at position 0)", "refs": null},
  {"kind": "parse", "id": "reject-unterminated-string", "source": "'unterminated", "ok": false, "reason": "a string opened with ' never closes", "refs": null},
  {"kind": "parse", "id": "reject-dangling-backslash", "source": "'abc\\", "ok": false, "reason": "a string ends on a dangling backslash", "refs": null},
  {"kind": "parse", "id": "reject-trailing-op", "source": "1 +", "ok": false, "reason": "the expression ends where a value was expected", "refs": null},
  {"kind": "parse", "id": "reject-unclosed-paren", "source": "(1 + 2", "ok": false, "reason": "an opening \"(\" never closes", "refs": null},
  {"kind": "parse", "id": "reject-leftover-number", "source": "1 2", "ok": false, "reason": "unexpected number 2 after the end of the expression", "refs": null},
  {"kind": "parse", "id": "reject-leftover-string", "source": "1 'x'", "ok": false, "reason": "unexpected string 'x' after the end of the expression", "refs": null},
  {"kind": "parse", "id": "reject-leftover-boolean", "source": "1 true", "ok": false, "reason": "unexpected true after the end of the expression", "refs": null},
  {"kind": "parse", "id": "reject-leftover-ref", "source": "1 {{value}}", "ok": false, "reason": "unexpected {{value}} after the end of the expression", "refs": null},
  {"kind": "parse", "id": "reject-leftover-lparen", "source": "1 (2)", "ok": false, "reason": "unexpected \"(\" after the end of the expression", "refs": null},
  {"kind": "parse", "id": "reject-leftover-rparen", "source": "(1))", "ok": false, "reason": "unexpected \")\" after the end of the expression", "refs": null},
  {"kind": "parse", "id": "reject-bad-char", "source": "1 % 2", "ok": false, "reason": "unexpected character \"%\" at position 2", "refs": null},
  {"kind": "parse", "id": "reject-percent-alone", "source": "%", "ok": false, "reason": "unexpected character \"%\" at position 0", "refs": null},
  {"kind": "parse", "id": "reject-leading-star", "source": "* 3", "ok": false, "reason": "\"*\" appears where a value was expected", "refs": null},
  {"kind": "parse", "id": "reject-leading-slash", "source": "/ 3", "ok": false, "reason": "\"/\" appears where a value was expected", "refs": null},
  {"kind": "parse", "id": "reject-leading-rparen", "source": ") 1", "ok": false, "reason": "\")\" appears where a value was expected", "refs": null},
  {"kind": "parse", "id": "reject-empty-parens", "source": "()", "ok": false, "reason": "\")\" appears where a value was expected", "refs": null},
  {"kind": "parse", "id": "reject-depth-past-cap", "source": "(((((((((1)))))))))", "ok": false, "reason": "parentheses nest deeper than 8 levels", "refs": null},
  {"kind": "parse", "id": "reject-double-op", "source": "1 + * 2", "ok": false, "reason": "\"*\" appears where a value was expected", "refs": null},
  {"kind": "parse", "id": "reject-exponent-literal", "source": "1e0", "ok": false, "reason": "\"e0\" is a bare word — there are no identifiers here. Quote literal text ('e0') or use a {{ref}}", "refs": null},
  {"kind": "eval", "id": "precedence-mul", "source": "1 + 2 * 3", "ok": true, "value": 7, "reason": null},
  {"kind": "eval", "id": "precedence-div", "source": "10 - 6 / 2", "ok": true, "value": 7, "reason": null},
  {"kind": "eval", "id": "parens-override", "source": "(1 + 2) * 3", "ok": true, "value": 9, "reason": null},
  {"kind": "eval", "id": "left-assoc-minus", "source": "10 - 3 - 2", "ok": true, "value": 5, "reason": null},
  {"kind": "eval", "id": "left-assoc-div", "source": "12 / 3 / 2", "ok": true, "value": 2, "reason": null},
  {"kind": "eval", "id": "negate-literal", "source": "-3 + 1", "ok": true, "value": -2, "reason": null},
  {"kind": "eval", "id": "negate-ref", "source": "-{{value}}", "ok": true, "value": -10, "reason": null},
  {"kind": "eval", "id": "double-negate", "source": "--2", "ok": true, "value": 2, "reason": null},
  {"kind": "eval", "id": "div-by-zero", "source": "1 / 0", "ok": false, "value": null, "reason": "\"/\" produced a value that is not a finite number"},
  {"kind": "eval", "id": "zero-over-zero", "source": "0 / 0", "ok": false, "value": null, "reason": "\"/\" produced a value that is not a finite number"},
  {"kind": "eval", "id": "negate-string", "source": "-'text'", "ok": false, "value": null, "reason": "\"-\" negates a string, and only numbers can be negated"},
  {"kind": "eval", "id": "concat-string-number", "source": "'rolled ' + 3", "ok": true, "value": "rolled 3", "reason": null},
  {"kind": "eval", "id": "concat-number-string", "source": "3 + ' rolled'", "ok": true, "value": "3 rolled", "reason": null},
  {"kind": "eval", "id": "concat-strings", "source": "'a' + 'b'", "ok": true, "value": "ab", "reason": null},
  {"kind": "eval", "id": "concat-float", "source": "'' + 0.123456", "ok": true, "value": "0.1235", "reason": null},
  {"kind": "eval", "id": "concat-int", "source": "'' + 7", "ok": true, "value": "7", "reason": null},
  {"kind": "eval", "id": "concat-true", "source": "'flag: ' + true", "ok": true, "value": "flag: true", "reason": null},
  {"kind": "eval", "id": "concat-false", "source": "false + '!'", "ok": true, "value": "false!", "reason": null},
  {"kind": "eval", "id": "bool-plus-number", "source": "true + 1", "ok": false, "value": null, "reason": "\"+\" adds a boolean and a number — it takes two numbers, or a string to concatenate"},
  {"kind": "eval", "id": "bool-plus-bool", "source": "true + false", "ok": false, "value": null, "reason": "\"+\" adds a boolean and a boolean — it takes two numbers, or a string to concatenate"},
  {"kind": "eval", "id": "bool-times", "source": "true * 2", "ok": false, "value": null, "reason": "\"*\" takes two numbers, got a boolean and a number"},
  {"kind": "eval", "id": "string-minus", "source": "'a' - 1", "ok": false, "value": null, "reason": "\"-\" takes two numbers, got a string and a number"},
  {"kind": "eval", "id": "string-times", "source": "'a' * 2", "ok": false, "value": null, "reason": "\"*\" takes two numbers, got a string and a number"},
  {"kind": "eval", "id": "number-div-string", "source": "4 / 'b'", "ok": false, "value": null, "reason": "\"/\" takes two numbers, got a number and a string"},
  {"kind": "eval", "id": "refs-sum", "source": "{{value}} + {{params.bonus}} + {{state.floor}}", "ok": true, "value": 13, "reason": null},
  {"kind": "eval", "id": "concat-dice", "source": "'rolled ' + {{dice}}", "ok": true, "value": "rolled 3d6: [1, 2, 3] = 6", "reason": null},
  {"kind": "eval", "id": "llm-times", "source": "{{llm}} * 2", "ok": false, "value": null, "reason": "\"*\" takes two numbers, got a string and a number"},
  {"kind": "eval", "id": "unresolved-ref", "source": "{{metadata.absent}} + 1", "ok": false, "value": null, "reason": "{{metadata.absent}} did not resolve to a value"},
  {"kind": "eval", "id": "unresolved-state", "source": "{{state.nowhere}} + 1", "ok": false, "value": null, "reason": "{{state.nowhere}} did not resolve to a value"},
  {"kind": "eval", "id": "bool-ref-concat", "source": "'flag ' + {{params.flag}}", "ok": true, "value": "flag true", "reason": null},
  {"kind": "eval", "id": "string-ref-concat", "source": "{{params.label}} + '!'", "ok": true, "value": "Jackie!", "reason": null},
  {"kind": "eval", "id": "overflow-times", "source": "{{params.huge}} * {{params.huge}}", "ok": false, "value": null, "reason": "\"*\" produced a value that is not a finite number"},
  {"kind": "eval", "id": "overflow-plus", "source": "{{params.huge}} + {{params.huge}} + {{params.huge}}", "ok": false, "value": null, "reason": "\"+\" produced a value that is not a finite number"},
  {"kind": "eval", "id": "huge-product", "source": "99999999 * 99999999 * 99999999 * 99999999 * 99999999", "ok": true, "value": 9.99999950000001e+39, "reason": null},
  {"kind": "format", "id": "format-14", "input": 14, "out": "14"},
  {"kind": "format", "id": "format-0.71349", "input": 0.71349, "out": "0.7135"},
  {"kind": "format", "id": "format-0.123456", "input": 0.123456, "out": "0.1235"},
  {"kind": "format", "id": "format-7", "input": 7, "out": "7"},
  {"kind": "format", "id": "format--0.5", "input": -0.5, "out": "-0.5"},
  {"kind": "format", "id": "format-1e+21", "input": 1e+21, "out": "1e+21"},
  {"kind": "format", "id": "format-0.30000000000000004", "input": 0.30000000000000004, "out": "0.3"},
  {"kind": "format", "id": "format-1234567", "input": 1234567, "out": "1234567"},
  {"kind": "format", "id": "format-0.3333333333333333", "input": 0.3333333333333333, "out": "0.3333"},];

const parseRows = CAPTURED.filter((r): r is ParseRow => r.kind === 'parse');
const evalRows = CAPTURED.filter((r): r is EvalRow => r.kind === 'eval');
const formatRows = CAPTURED.filter((r): r is FormatRow => r.kind === 'format');

describe('effect expressions — the captured v4 rows', () => {
  it('the table is the expected shape (a truncated capture must not pass silently)', () => {
    // Composition, never a hand-written count (the `harness-corpus-shape-
    // constants-rot` rule): the partition is total, all three kinds are
    // non-empty, both parse verdicts appear, and every id is unique because the
    // ids are the test names below.
    expect(parseRows.length + evalRows.length + formatRows.length).toBe(CAPTURED.length);
    expect(parseRows.length).toBeGreaterThan(0);
    expect(evalRows.length).toBeGreaterThan(0);
    expect(formatRows.length).toBeGreaterThan(0);
    expect(parseRows.some((r) => r.ok)).toBe(true);
    expect(parseRows.some((r) => !r.ok)).toBe(true);
    expect(evalRows.some((r) => r.ok)).toBe(true);
    expect(evalRows.some((r) => !r.ok)).toBe(true);
    expect(new Set(CAPTURED.map((r) => r.id)).size).toBe(CAPTURED.length);

    // A rejected row must carry the sentence to compare against; an accepted
    // one must carry its ref list. Either missing would compare against `null`
    // and pass vacuously.
    for (const row of parseRows) {
      expect(row.ok ? row.refs : row.reason, row.id).not.toBeNull();
    }
    for (const row of evalRows) {
      if (!row.ok) expect(typeof row.reason, row.id).toBe('string');
    }
  });

  describe('parseExpression', () => {
    for (const row of parseRows) {
      it(`matches v4 — ${row.id}`, () => {
        const result = parseExpression(row.source);
        if (result.ok) {
          expect(row.ok, `v5 accepted; v4 rejected with: ${row.reason}`).toBe(true);
          expect(result.expr.refs).toEqual(row.refs);
          // The source rides along verbatim, for records and messages.
          expect(result.expr.source).toBe(row.source);
        } else {
          expect(row.ok, `v5 rejected with: ${result.reason}; v4 accepted`).toBe(false);
          expect(result.reason).toBe(row.reason);
        }
      });
    }
  });

  describe('evaluateExpression', () => {
    for (const row of evalRows) {
      it(`matches v4 — ${row.id}`, () => {
        const parsed = parseExpression(row.source);
        expect(parsed.ok, `${row.id} should parse`).toBe(true);
        if (!parsed.ok) return;

        const result = evaluateExpression(parsed.expr, (name) => REFS[name]);
        if (result.ok) {
          expect(row.ok, `v5 evaluated; v4 failed with: ${row.reason}`).toBe(true);
          expect(result.value).toBe(row.value);
        } else {
          expect(row.ok, `v5 failed with: ${result.reason}; v4 evaluated`).toBe(false);
          expect(result.reason).toBe(row.reason);
        }
      });
    }
  });

  describe('formatValue', () => {
    for (const row of formatRows) {
      it(`matches v4 — ${row.id}`, () => {
        expect(formatValue(row.input)).toBe(row.out);
      });
    }
  });
});

/**
 * v4's own suite, ported case-for-case (`__tests__/unit/lib/pascal/
 * expressions.test.ts`). It asserts fragments and behaviours where the captured
 * rows assert bytes; keeping both means a future v4 rewording is caught by the
 * rows while the INTENT stays legible here.
 */

/** Parse-or-throw, for tests that only care about evaluation. */
function parsed(source: string) {
  const result = parseExpression(source);
  if (!result.ok) throw new Error(`fixture failed to parse: ${result.reason}`);
  return result.expr;
}

/** Evaluate with a fixture ref table; absent keys resolve to undefined. */
function evaluate(source: string, refs: Record<string, ExprValue> = {}) {
  return evaluateExpression(parsed(source), (name) => refs[name]);
}

function value(source: string, refs: Record<string, ExprValue> = {}): ExprValue {
  const result = evaluate(source, refs);
  if (!result.ok) throw new Error(`fixture failed to evaluate: ${result.reason}`);
  return result.value;
}

describe('parseExpression — acceptance', () => {
  for (const source of [
    '1',
    '1.5',
    "'broken pick'",
    '"double quoted"',
    'true',
    'false',
    '{{value}}',
    '{{roll}}',
    '{{dice}}',
    '{{llm}}',
    '{{params.bonus}}',
    '{{metadata.lockpicks}}',
    '{{state.encounter.count}}',
    '1 + 2 * 3',
    '({{value}} + 1) * 2',
    "-{{state.debt}} / 2 + 'gp'",
  ]) {
    it(`accepts ${source}`, () => {
      expect(parseExpression(source).ok).toBe(true);
    });
  }

  it('reports the refs an expression names, deduplicated, in order', () => {
    const result = parseExpression('{{state.a}} + {{params.b}} + {{state.a}}');
    expect(result.ok && result.expr.refs).toEqual(['state.a', 'params.b']);
  });

  it('supports escaped quotes inside strings', () => {
    expect(value("'it\\'s'")).toBe("it's");
    expect(value('"say \\"hi\\""')).toBe('say "hi"');
  });
});

describe('parseExpression — rejection', () => {
  for (const [source, fragment] of [
    ['', 'empty'],
    ['broken pick', 'bare word'],
    ['foo(1)', 'bare word'],
    ['{{nonsense}}', 'not a reference'],
    ['{{params.}}', 'not a reference'],
    ['{{value', 'never closes'],
    ["'unterminated", 'never closes'],
    ['1 +', 'ends where a value was expected'],
    ['(1 + 2', 'never closes'],
    ['1 2', 'unexpected'],
    ['a.b', 'bare word'],
    ['{value}', 'lone'],
    ['1 % 2', 'unexpected character'],
  ] as const) {
    it(`rejects ${source}`, () => {
      const result = parseExpression(source);
      expect(result.ok).toBe(false);
      if (!result.ok) expect(result.reason.toLowerCase()).toContain(fragment.toLowerCase());
    });
  }

  it('rejects source longer than the cap', () => {
    const long = `'${'x'.repeat(MAX_EFFECT_EXPRESSION_LENGTH)}'`;
    const result = parseExpression(long);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.reason).toContain(`${MAX_EFFECT_EXPRESSION_LENGTH}`);
  });

  it('rejects more tokens than the cap', () => {
    const many = Array.from({ length: MAX_EFFECT_EXPRESSION_TOKENS / 2 + 1 }, () => '1').join('+');
    const result = parseExpression(many);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.reason).toContain(`${MAX_EFFECT_EXPRESSION_TOKENS}`);
  });

  it('rejects parentheses nested past the depth cap, and accepts them at it', () => {
    const at = `${'('.repeat(MAX_EFFECT_EXPRESSION_DEPTH)}1${')'.repeat(MAX_EFFECT_EXPRESSION_DEPTH)}`;
    const past = `${'('.repeat(MAX_EFFECT_EXPRESSION_DEPTH + 1)}1${')'.repeat(MAX_EFFECT_EXPRESSION_DEPTH + 1)}`;
    expect(parseExpression(at).ok).toBe(true);
    const result = parseExpression(past);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.reason).toContain('nest');
  });
});

describe('evaluateExpression — arithmetic', () => {
  it('applies precedence: * and / bind tighter than + and -', () => {
    expect(value('1 + 2 * 3')).toBe(7);
    expect(value('10 - 6 / 2')).toBe(7);
  });

  it('parentheses override precedence', () => {
    expect(value('(1 + 2) * 3')).toBe(9);
  });

  it('associates left to right', () => {
    expect(value('10 - 3 - 2')).toBe(5);
    expect(value('12 / 3 / 2')).toBe(2);
  });

  it('negates numbers, including refs', () => {
    expect(value('-3 + 1')).toBe(-2);
    expect(value('-{{value}}', { value: 4 })).toBe(-4);
    expect(value('--2')).toBe(2);
  });

  it('fails soft on division by zero', () => {
    const result = evaluate('1 / 0');
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.reason).toContain('finite');
  });

  it('fails soft on 0 / 0', () => {
    expect(evaluate('0 / 0').ok).toBe(false);
  });

  it('fails soft when negating a non-number', () => {
    expect(evaluate("-'text'").ok).toBe(false);
  });
});

describe('evaluateExpression — strings and booleans', () => {
  it('concatenates when either + operand is a string', () => {
    expect(value("'rolled ' + 3")).toBe('rolled 3');
    expect(value("3 + ' rolled'")).toBe('3 rolled');
    expect(value("'a' + 'b'")).toBe('ab');
  });

  it('stringifies numbers in concatenation via formatValue', () => {
    expect(value("'' + 0.123456")).toBe(formatValue(0.123456));
    expect(value("'' + 7")).toBe('7');
  });

  it('stringifies booleans in concatenation', () => {
    expect(value("'flag: ' + true")).toBe('flag: true');
    expect(value("false + '!'")).toBe('false!');
  });

  it('refuses boolean arithmetic — no truthiness', () => {
    expect(evaluate('true + 1').ok).toBe(false);
    expect(evaluate('true + false').ok).toBe(false);
    expect(evaluate('true * 2').ok).toBe(false);
  });

  it('refuses -, *, / with any non-number operand', () => {
    expect(evaluate("'a' - 1").ok).toBe(false);
    expect(evaluate("'a' * 2").ok).toBe(false);
    expect(evaluate("4 / 'b'").ok).toBe(false);
  });
});

describe('evaluateExpression — references', () => {
  it('substitutes refs of every family', () => {
    expect(
      value('{{value}} + {{params.bonus}} + {{state.floor}}', {
        value: 10,
        'params.bonus': 2,
        'state.floor': 1,
      }),
    ).toBe(13);
  });

  it('concatenates string refs — {{dice}} and {{llm}} are strings', () => {
    expect(value("'rolled ' + {{dice}}", { dice: '3d6: [1, 2, 3] = 6' })).toBe(
      'rolled 3d6: [1, 2, 3] = 6',
    );
  });

  it('never coerces {{llm}} numerically', () => {
    const result = evaluate('{{llm}} * 2', { llm: '21' });
    expect(result.ok).toBe(false);
  });

  it('fails soft when a ref does not resolve', () => {
    const result = evaluate('{{metadata.absent}} + 1');
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.reason).toContain('metadata.absent');
  });
});

describe('formatValue', () => {
  it('renders integers plain and floats to 4 significant digits', () => {
    expect(formatValue(14)).toBe('14');
    expect(formatValue(0.71349)).toBe('0.7135');
  });
});
