/**
 * Oracle case (P4.d5 unit 2): `llmNumber` — lenient numbers for LLM-supplied
 * tool arguments (v4 `lib/tools/llm-number.ts`, new at `61ec90bd`).
 *
 * Drives v4's REAL exported `llmNumber` two ways:
 *
 *  1. `kind: 'preprocess'` — `llmNumber(z.any())`, so the parse output IS
 *     whatever the preprocess step returned, unfiltered by any inner schema.
 *     This pins the ported function exactly: which values are touched, which are
 *     handed back untouched, and what `Number()` makes of a string.
 *  2. `kind: 'bounded'` — `llmNumber(z.number().int().min(2).max(1000))` (the
 *     rng `type` arm's real inner schema), proving bounds and `.int()` apply
 *     AFTER conversion: `"1001"` fails a max(1000) exactly as `1001` does.
 *
 * The corpus is deliberately heavy on the two traps a Rust port walks into:
 *   * `Number()` is NOT `parseInt` and NOT Rust's `str::parse::<f64>` — `"0x10"`
 *     is 16 (Rust errors), `"5px"` is NaN (parseInt says 5), and `"inf"`/`"nan"`
 *     are NaN (Rust's parser accepts both spellings);
 *   * non-strings must fall through UNTOUCHED — the `z.coerce.number()`
 *     divergence, which would turn `true` into 1 and `null`/`[]` into 0, trading
 *     a rejected call for a wrong one.
 *
 * Run from inside the server checkout (v4 @ a33ac8b8):
 *   cd ~/source/quilltap-server
 *   npx tsx <worktree>/harness/oracle/cases/llm-number.ts \
 *     > /tmp/oracle-llm-number.ndjson
 */

import { z } from 'zod';
import { llmNumber } from '@/lib/tools/llm-number';

/** The preprocess output, unfiltered — `z.any()` accepts whatever it returns. */
const passthrough = llmNumber(z.any());
/** The rng `type` arm's real inner schema — bounds apply AFTER conversion. */
const bounded = llmNumber(z.number().int().min(2).max(1000));

const PREPROCESS_CASES: Array<[string, unknown]> = [
  // --- Strings that convert -------------------------------------------------
  ['quoted-int', '6'],
  ['quoted-zero', '0'],
  ['quoted-negative', '-1'],
  ['quoted-float', '6.5'],
  ['quoted-padded', ' 5 '],
  ['quoted-tab-padded', '\t7\n'],
  ['quoted-exponent', '1e3'],
  ['quoted-exponent-upper', '1E3'],
  ['quoted-exponent-negative', '1e-3'],
  ['quoted-plus', '+5'],
  ['trailing-dot', '1.'],
  ['leading-dot', '.5'],
  ['plus-leading-dot', '+.5'],
  ['leading-zeros', '007'],
  ['big-int', '9007199254740993'],
  // Number(), NOT parseInt: the non-decimal integer literals.
  ['hex', '0x10'],
  ['hex-upper-x', '0X10'],
  ['hex-upper-digits', '0xFF'],
  ['octal', '0o17'],
  ['binary', '0b101'],
  // --- Strings that do NOT convert (the original comes back) ----------------
  ['empty', ''],
  ['whitespace-only', '   '],
  ['tab-only', '\t'],
  ['nonsense', 'nonsense'],
  // parseInt('5px') is 5; Number('5px') is NaN. This pins Number().
  ['numeric-prefix', '5px'],
  ['trailing-space-junk', '5 px'],
  // Rust's parse::<f64> accepts all of these; JS Number() does not.
  ['inf-lower', 'inf'],
  ['infinity-lower', 'infinity'],
  ['nan-word', 'nan'],
  ['NaN-word', 'NaN'],
  ['neg-inf-lower', '-inf'],
  // Number('Infinity') IS Infinity — but not finite, so the original returns.
  ['Infinity-exact', 'Infinity'],
  ['neg-Infinity-exact', '-Infinity'],
  ['plus-Infinity-exact', '+Infinity'],
  ['overflow-exponent', '1e999'],
  // Sign is not allowed on a non-decimal literal.
  ['signed-hex', '-0x10'],
  ['hex-no-digits', '0x'],
  ['hex-bad-digits', '0xZZ'],
  // Numeric separators are a source-literal feature, not a Number() one.
  ['underscore-separator', '1_0'],
  ['space-after-sign', '- 1'],
  ['bare-dot', '.'],
  ['bare-exponent', '1e'],
  ['double-sign', '--1'],
  ['comma', '1,000'],
  // --- Non-strings: UNTOUCHED (the z.coerce.number() divergence) ------------
  ['number', 6],
  ['number-float', 6.5],
  ['number-zero', 0],
  ['number-negative', -3],
  ['true', true],
  ['false', false],
  ['null', null],
  ['empty-array', []],
  ['number-array', [5]],
  ['object', {}],
];

const BOUNDED_CASES: Array<[string, unknown]> = [
  ['quoted-in-range', '6'],
  ['bare-in-range', 6],
  ['quoted-at-min', '2'],
  ['quoted-at-max', '1000'],
  // Bounds apply AFTER conversion — the whole point.
  ['quoted-below-min', '1'],
  ['quoted-above-max', '1001'],
  ['bare-above-max', 1001],
  // .int() applies after conversion too.
  ['quoted-non-integer', '6.5'],
  ['bare-non-integer', 6.5],
  // An integral float IS an integer to Zod's .int() (Number.isInteger(6.0)).
  ['bare-integral-float', 6.0],
  ['quoted-integral-float', '6.0'],
  ['quoted-exponent-in-range', '1e2'],
  ['hex-in-range', '0x10'],
  // Refused, not coerced.
  ['true', true],
  ['null', null],
  ['empty-array', []],
  ['empty-string', ''],
  ['nonsense', 'nonsense'],
  ['Infinity', 'Infinity'],
];

const rows: unknown[] = [];

for (const [id, input] of PREPROCESS_CASES) {
  const parsed = passthrough.safeParse(input);
  rows.push({
    kind: 'preprocess',
    id,
    input,
    // z.any() accepts everything the preprocess returns, so this always succeeds
    // and `output` IS the preprocess result.
    ok: parsed.success,
    output: parsed.success ? parsed.data : null,
    // The type name is what distinguishes "handed back the original string"
    // from "converted to a number" when the JSON rendering could look alike.
    outputType: parsed.success ? typeofOf(parsed.data) : null,
  });
}

for (const [id, input] of BOUNDED_CASES) {
  const parsed = bounded.safeParse(input);
  rows.push({
    kind: 'bounded',
    id,
    input,
    ok: parsed.success,
    output: parsed.success ? parsed.data : null,
  });
}

/** `typeof`, but naming null/array so the diff can see them apart. */
function typeofOf(v: unknown): string {
  if (v === null) return 'null';
  if (Array.isArray(v)) return 'array';
  return typeof v;
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
