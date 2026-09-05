/**
 * @jest-environment node
 *
 * P4.6ay unit 3 ORACLE: the Pascal custom-tool EXECUTION core. Drives v4's REAL
 * `lib/pascal/custom-tools.ts` (`executeCustomTool`, `resolveParams`,
 * `matchesWhen`, `renderTemplate`, `formatValue`), every definition parsed
 * through the REAL `QtapCustomToolSchema` so a case can never drift from what
 * actually loads.
 *
 * ## The byte source
 *
 * `crypto.randomBytes` is mocked over a scripted pool, per case, and the cursor
 * is reported as `bytesConsumed` — which is the point: it pins that a degenerate
 * `min === max` range short-circuits WITHOUT drawing, and that the dice path's
 * rejection sampling draws exactly what v5's `FixedBytes` draws. The mock THROWS
 * when the pool runs dry, mirroring `FixedBytes`' panic, so a byte shortage
 * surfaces loudly instead of quietly diverging.
 *
 * v4's own execution test mocks no randomness at all, so it can only assert
 * ranges; this is the stronger bar.
 *
 * Run (v4 @ d68638b4, Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-pascal-exec-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/pascal-custom-tools-execution.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-pascal-execution.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- pascal-custom-tools-execution
 */

import * as fs from 'fs';

// `var` + a hoisted function declaration: jest hoists the factory above the
// module's imports, so anything it reads at FACTORY time would be in the TDZ.
// The factory only closes over the binding; `mockRngDraw` runs later, per case.
var mockRngState: { pool: Buffer; cursor: number } = { pool: Buffer.alloc(0), cursor: 0 };

function mockRngDraw(n: number): Buffer {
  const { pool, cursor } = mockRngState;
  if (cursor + n > pool.length) {
    throw new Error(`byte pool exhausted: needed ${n} at ${cursor} of ${pool.length}`);
  }
  mockRngState.cursor = cursor + n;
  return pool.subarray(cursor, cursor + n);
}

jest.mock('crypto', () => {
  const actual = jest.requireActual('crypto');
  return { ...actual, randomBytes: (n: number) => mockRngDraw(n) };
});

import {
  executeCustomTool,
  resolveParams,
  matchesWhen,
  renderTemplate,
  formatValue,
} from '@/lib/pascal/custom-tools';
import { QtapCustomToolSchema, type QtapCustomTool, type When } from '@/lib/pascal/custom-tool.types';

const OUT = process.env.QT_ORACLE_OUT;
const rows: unknown[] = [];
const emit = (r: unknown) => rows.push(r);

/** Parse through the real schema so cases can never drift from what loads. */
function define(doc: unknown): QtapCustomTool {
  const result = QtapCustomToolSchema.safeParse(doc);
  if (!result.success) {
    throw new Error(`case definition is not valid: ${result.error.issues.map((i) => i.message).join('; ')}`);
  }
  return result.data;
}

/** A deterministic, case-specific byte pool — emitted so v5 draws the same. */
function pool(seed: number, length = 64): Buffer {
  const b = Buffer.alloc(length);
  for (let i = 0; i < length; i++) b[i] = (i * 37 + seed * 101 + 11) & 0xff;
  return b;
}

const CATCH_ALL = { when: true as const, message: 'fallback', state: 'info' as const };

// ---------------------------------------------------------------- formatValue
describe('formatValue', () => {
  it('emits', () => {
    const values = [
      0, 1, -1, 42, 1000000, 0.5, 1.5, -2.5, 0.1, 1 / 3, 2 / 3, 0.123456789, 12.3456789,
      123.456789, 1234.56789, 0.000123456, 1e-7, 1.5e-7, 1e21, 1e20, 2.675, 0.15, 9.999,
      99.995, -0.0001234, 3.9999999, 100.5, 0.0001, 1e-6,
    ];
    for (const v of values) emit({ kind: 'formatValue', id: String(v), input: v, out: formatValue(v) });
  });
});

// -------------------------------------------------------------- renderTemplate
describe('renderTemplate', () => {
  it('emits', () => {
    const tool = define({
      name: 'probe',
      description: 'A probe.',
      parameters: {
        scale: { type: 'number', default: 1.5 },
        steps: { type: 'integer', default: 3 },
        label: { type: 'string', default: 'brass' },
        loud: { type: 'boolean', default: true },
      },
      outcomes: [CATCH_ALL],
    });
    const params = resolveParams(tool, {});

    // A fixed metadata sheet, threaded into every render so the port draws the
    // same. Non-`{{metadata.*}}` messages ignore it.
    const META: Record<string, unknown> = {
      clearance: 5,
      frac: 1.5,
      house: 'Aurum',
      hasAccess: true,
      nested: { a: 1 },
      list: [1, 2],
    };

    // The 616930db consult subject, threaded into every render. Rows that want
    // the "no consult ran" path carry an explicit `null` third element.
    const LLM = { ok: true, output: 'the West Door' };

    const messages: Array<[string, string, ({ ok: boolean; output: string } | null)?]> = [
      ['value', 'rolled {{value}}'],
      ['roll', 'raw {{roll}}'],
      ['dice', 'dice {{dice}}'],
      ['all-three', '{{value}} / {{roll}} / {{dice}}'],
      ['params-number', 'scale {{params.scale}}'],
      ['params-integer', 'steps {{params.steps}}'],
      ['params-string', 'label {{params.label}}'],
      ['params-boolean', 'loud {{params.loud}}'],
      ['whitespace-inside', '{{  value  }}'],
      ['unknown-placeholder', 'what is {{nonsense}} here'],
      ['unknown-param', 'missing {{params.ghost}}'],
      ['params-prefix-only', 'bare {{params.}}'],
      // v4 `0506517d3` correction (e): `{{params.toString}}` no longer renders
      // prototype function source. The pre-fix renderer tested `name in
      // vars.params`, which reaches `Object.prototype`, and then `String(v)`d
      // whatever it found — so a character's message could carry the text of
      // `Object.prototype.toString`. The post-fix path looks the value up and
      // renders only a primitive, so the placeholder is left as written. v5's
      // params lookup is an association-list scan with no prototype to reach,
      // and these rows are how the corpus says so on both sides.
      ['params-prototype-tostring', 'x {{params.toString}} y'],
      ['params-prototype-constructor', 'x {{params.constructor}} y'],
      ['params-prototype-hasownproperty', 'x {{params.hasOwnProperty}} y'],
      ['params-prototype-proto', 'x {{params.__proto__}} y'],
      ['empty-braces', 'empty {{}} here'],
      ['adjacent', '{{value}}{{value}}'],
      ['repeated', '{{value}} and {{value}} and {{roll}}'],
      ['no-placeholder', 'nothing to do'],
      ['brace-soup', '{{{value}}}'],
      ['unclosed', 'a {{value b'],
      ['nested-ish', '{{val{{value}}ue}}'],
      // The d68638b4 {{metadata.*}} family.
      ['metadata-number', 'clr {{metadata.clearance}}'],
      ['metadata-float', 'f {{metadata.frac}}'],
      ['metadata-string', 'house {{metadata.house}}'],
      ['metadata-boolean', 'acc {{metadata.hasAccess}}'],
      ['metadata-missing', 'x {{metadata.ghost}} y'],
      ['metadata-non-primitive-object', 'n {{metadata.nested}} m'],
      ['metadata-non-primitive-list', 'l {{metadata.list}} m'],
      ['metadata-prefix-only', 'bare {{metadata.}}'],
      // The same prototype question on the metadata sheet. This arm was already
      // safe pre-fix (`vars.metadata?.[name]` then `isPrimitive`), so the rows
      // pin that the collapse did not move it.
      ['metadata-prototype-tostring', 'x {{metadata.toString}} y'],
      ['metadata-prototype-constructor', 'x {{metadata.constructor}} y'],
      ['metadata-whitespace', '{{  metadata.house  }}'],
      // The 616930db {{llm}} placeholder.
      ['llm-renders-output', 'the oracle says {{llm}}'],
      ['llm-whitespace', '{{  llm  }}'],
      ['llm-repeated', '{{llm}} and {{llm}}'],
      ['llm-with-value', '{{value}} — {{llm}}'],
      ['llm-prefixed-key-unknown', 'x {{llm.output}} y'],
      // No consult ran: the placeholder is left exactly as written.
      ['llm-absent-left-verbatim', 'the oracle says {{llm}}', null],
      // A failed consult still renders — its output is the author's errorMessage.
      ['llm-failed-renders-error-message', 'the oracle says {{llm}}', { ok: false, output: 'The wire went dead.' }],
      ['llm-empty-output', 'x{{llm}}y', { ok: true, output: '' }],
    ];

    for (const entry of messages) {
      const [id, message] = entry;
      const llm = entry.length > 2 ? (entry[2] ?? undefined) : LLM;
      emit({
        kind: 'renderTemplate',
        id,
        // Emitted so the port re-parses the definition through its own schema
        // rather than needing a Deserialize for the parsed shape.
        tool,
        message,
        metadata: META,
        llm: llm ?? null,
        out: renderTemplate(message, {
          value: 12.3456,
          roll: 0.6789,
          dice: '3d6: [4, 2, 6] = 12',
          params,
          metadata: META,
          llm,
        }),
      });
    }
  });
});

// ---------------------------------------------------------------- resolveParams
describe('resolveParams', () => {
  it('emits', () => {
    const tool = define({
      name: 'unlock',
      description: 'Attempt to pick the lock.',
      parameters: {
        bonus: { type: 'number', default: 0, min: 0, max: 10 },
        tries: { type: 'integer', default: 1 },
        label: { type: 'string', default: 'lock' },
        loud: { type: 'boolean', default: false },
      },
      outcomes: [CATCH_ALL],
    });
    const none = define({ name: 'bare', description: 'No parameters.', outcomes: [CATCH_ALL] });

    const cases: Array<[string, QtapCustomTool, Record<string, unknown> | null | undefined]> = [
      ['all-defaults', tool, {}],
      ['null-supplied', tool, null],
      ['undefined-supplied', tool, undefined],
      ['explicit-values', tool, { bonus: 3, tries: 2, label: 'vault', loud: true }],
      ['null-value-falls-back', tool, { bonus: null }],
      // The quoting habit v4 accepts on purpose.
      ['numeric-string', tool, { bonus: '4' }],
      ['numeric-string-spaces', tool, { bonus: '  4  ' }],
      ['empty-string-is-zero', tool, { bonus: '' }],
      ['hex-string', tool, { bonus: '0x8' }],
      ['boolean-to-number', tool, { bonus: true }],
      ['array-to-number', tool, { bonus: [5] }],
      // integer rounds THEN clamps, and JS rounds halves toward +Infinity.
      ['integer-rounds', tool, { tries: 2.4 }],
      ['integer-rounds-half-up', tool, { tries: 2.5 }],
      ['integer-rounds-negative-half', tool, { tries: -2.5 }],
      ['clamp-low', tool, { bonus: -5 }],
      ['clamp-high', tool, { bonus: 99 }],
      ['round-then-clamp', tool, { bonus: 10.6 }],
      ['boolean-true-string', tool, { loud: 'true' }],
      ['boolean-false-string', tool, { loud: 'false' }],
      ['string-from-number', tool, { label: 5 }],
      ['string-from-boolean', tool, { label: false }],
      ['string-from-array', tool, { label: [1, 2] }],
      ['string-from-object', tool, { label: {} }],
      // Rejections.
      ['unknown-key', tool, { bonuss: 5 }],
      ['unknown-key-on-paramless', none, { anything: 1 }],
      ['nan-number', tool, { bonus: 'abc' }],
      ['object-number', tool, { bonus: {} }],
      ['bad-boolean', tool, { loud: 'yes' }],
      ['bad-boolean-number', tool, { loud: 1 }],
    ];

    for (const [id, def, supplied] of cases) {
      let out: unknown = null;
      let error: string | null = null;
      try {
        out = resolveParams(def, supplied);
      } catch (e) {
        error = (e as Error).message;
      }
      emit({ kind: 'resolveParams', id, tool: def, supplied: supplied ?? null, out, error });
    }
  });
});

// ------------------------------------------------------------------ matchesWhen
describe('matchesWhen', () => {
  it('emits', () => {
    const tool = define({
      name: 'probe',
      description: 'A probe.',
      parameters: {
        difficulty: { type: 'number', default: 10 },
        material: { type: 'string', default: 'brass' },
        lit: { type: 'boolean', default: true },
      },
      outcomes: [CATCH_ALL],
    });
    const params = resolveParams(tool, {});

    // A fixed metadata sheet for the metadata subject rows.
    const SHEET: Record<string, unknown> = {
      clearance: 5,
      house: 'Aurum',
      hasAccess: true,
      nested: { a: 1 },
      list: [1, 2],
    };

    // The 616930db consult block a `when.llm` row needs on its definition, and
    // the subject those rows are evaluated against.
    const LLM_BLOCK = { llm: { prompt: 'Speak.', errorMessage: 'The wire went dead.' } };

    const whens: Array<
      [string, unknown, Record<string, unknown>?, { ok: boolean; output: string } | null?]
    > = [
      ['catch-all', true],
      ['gt-holds', { gt: 5 }],
      ['gt-fails', { gt: 500 }],
      ['gte-boundary', { gte: 12 }],
      ['lt-holds', { lt: 20 }],
      ['lte-boundary', { lte: 12 }],
      ['eq-holds', { eq: 12 }],
      ['eq-fails', { eq: 13 }],
      ['neq-holds', { neq: 13 }],
      ['anded-both-hold', { gte: 10, lte: 20 }],
      ['anded-one-fails', { gte: 10, lte: 11 }],
      // `roll` tests the RAW pre-transform draw, which differs from the value.
      ['roll-subject-holds', { roll: { lt: 1 } }],
      ['roll-subject-fails', { roll: { gt: 1 } }],
      ['value-and-roll', { gt: 5, roll: { lt: 1 } }],
      ['params-number', { params: { difficulty: { eq: 10 } } }],
      ['params-string-eq', { params: { material: { eq: 'brass' } } }],
      ['params-string-neq', { params: { material: { neq: 'iron' } } }],
      ['params-boolean', { params: { lit: { eq: true } } }],
      ['params-fails', { params: { difficulty: { gt: 99 } } }],
      // The opposed check: the operand is what the caller supplied.
      ['param-ref-operand-holds', { gte: { $param: 'difficulty' } }],
      ['param-ref-operand-fails', { gt: { $param: 'difficulty' } }],
      ['all-three-subjects', { gt: 5, roll: { lt: 1 }, params: { material: { eq: 'brass' } } }],
      // The d68638b4 metadata subject — fail-soft against the SHEET above.
      ['md-holds', { metadata: { clearance: { gte: 3 } } }, SHEET],
      ['md-fails', { metadata: { clearance: { gte: 9 } } }, SHEET],
      ['md-absent-key', { metadata: { missing: { eq: 1 } } }, SHEET],
      ['md-non-primitive-object', { metadata: { nested: { eq: 1 } } }, SHEET],
      ['md-non-primitive-list', { metadata: { list: { gt: 0 } } }, SHEET],
      ['md-string-eq', { metadata: { house: { eq: 'Aurum' } } }, SHEET],
      ['md-string-neq', { metadata: { house: { neq: 'Brass' } } }, SHEET],
      ['md-bool-eq', { metadata: { hasAccess: { eq: true } } }, SHEET],
      ['md-ordering-on-string-declines', { metadata: { house: { gt: 1 } } }, SHEET],
      ['md-eq-number-vs-string', { metadata: { clearance: { eq: 'five' } } }, SHEET],
      ['md-param-operand-holds', { metadata: { clearance: { lte: { $param: 'difficulty' } } } }, SHEET],
      ['md-param-operand-fails', { metadata: { clearance: { gte: { $param: 'difficulty' } } } }, SHEET],
      ['md-anded-value-holds', { gt: 5, metadata: { clearance: { gte: 3 } } }, SHEET],
      ['md-anded-value-fails', { gt: 50, metadata: { clearance: { gte: 3 } } }, SHEET],
      ['md-no-sheet-declines', { metadata: { clearance: { gte: 3 } } }, undefined],

      // ---- the 616930db containment comparators: params (STRICT, throws) ----
      ['params-contains-holds', { params: { material: { contains: 'ras' } } }],
      ['params-contains-fails', { params: { material: { contains: 'iron' } } }],
      ['params-contains-case-sensitive', { params: { material: { contains: 'BRASS' } } }],
      ['params-contains-whole', { params: { material: { contains: 'brass' } } }],
      ['params-contains-untrimmed', { params: { material: { contains: ' brass' } } }],
      ['params-ncontains-holds', { params: { material: { ncontains: 'iron' } } }],
      ['params-ncontains-fails', { params: { material: { ncontains: 'ras' } } }],
      ['params-contains-anded', { params: { material: { contains: 'ra', ncontains: 'iron' } } }],
      ['params-contains-param-needle', { params: { material: { contains: { $param: 'material' } } } }],
      // NOTE: `matchesComparator`'s strict `requireString` THROW (a non-string
      // haystack or needle) is UNREACHABLE from a loadable definition — the
      // load-time containment check rejects those first, exactly as
      // `requireNumber`'s ordering arm is unreachable. Both sides keep the guard
      // as a regression tripwire; no corpus row can express it, because
      // `define()` refuses the definition. The `contains-numeric-param` /
      // `ncontains-boolean-param` rows in the DEFINITION corpus pin that
      // rejection instead.

      // ---- containment on metadata (FAIL-SOFT) ------------------------------
      ['md-contains-holds', { metadata: { house: { contains: 'uru' } } }, SHEET],
      ['md-contains-fails', { metadata: { house: { contains: 'Ferro' } } }, SHEET],
      ['md-contains-case-sensitive', { metadata: { house: { contains: 'aurum' } } }, SHEET],
      ['md-ncontains-holds', { metadata: { house: { ncontains: 'Ferro' } } }, SHEET],
      ['md-ncontains-fails', { metadata: { house: { ncontains: 'uru' } } }, SHEET],
      // Absence is not a miss: a number subject DECLINES even under ncontains.
      ['md-ncontains-number-subject-declines', { metadata: { clearance: { ncontains: 'x' } } }, SHEET],
      ['md-contains-number-subject-declines', { metadata: { clearance: { contains: '5' } } }, SHEET],
      ['md-contains-absent-key-declines', { metadata: { missing: { ncontains: 'x' } } }, SHEET],
      ['md-contains-non-primitive-declines', { metadata: { nested: { ncontains: 'x' } } }, SHEET],

      // ---- the 616930db llm subject (FAIL-SOFT, forgiving) ------------------
      ['llm-ok-true-holds', { llm: { ok: true } }, undefined, { ok: true, output: 'YES' }],
      ['llm-ok-true-fails', { llm: { ok: true } }, undefined, { ok: false, output: 'The wire went dead.' }],
      ['llm-ok-false-holds', { llm: { ok: false } }, undefined, { ok: false, output: 'The wire went dead.' }],
      ['llm-no-consult-declines', { llm: { ok: true } }, undefined, null],
      ['llm-no-consult-ok-false-still-declines', { llm: { ok: false } }, undefined, null],
      // eq: case-insensitive, trimmed, forgiving ONE trailing `.` or `!`.
      ['llm-eq-exact', { llm: { eq: 'YES' } }, undefined, { ok: true, output: 'YES' }],
      ['llm-eq-case-folds', { llm: { eq: 'YES' } }, undefined, { ok: true, output: 'yes' }],
      ['llm-eq-answer-padded', { llm: { eq: 'YES' } }, undefined, { ok: true, output: '  yes  ' }],
      ['llm-eq-trailing-period', { llm: { eq: 'YES' } }, undefined, { ok: true, output: 'Yes.' }],
      ['llm-eq-trailing-bang', { llm: { eq: 'YES' } }, undefined, { ok: true, output: 'Yes!' }],
      ['llm-eq-trailing-question-not-forgiven', { llm: { eq: 'YES' } }, undefined, { ok: true, output: 'Yes?' }],
      ['llm-eq-double-period-not-forgiven', { llm: { eq: 'YES' } }, undefined, { ok: true, output: 'Yes..' }],
      ['llm-eq-operand-padded', { llm: { eq: '  YES  ' } }, undefined, { ok: true, output: 'yes' }],
      ['llm-eq-numeric-both', { llm: { eq: 42 } }, undefined, { ok: true, output: '42' }],
      ['llm-eq-numeric-answer-padded', { llm: { eq: 42 } }, undefined, { ok: true, output: ' 42 ' }],
      ['llm-eq-numeric-answer-decimal', { llm: { eq: 42 } }, undefined, { ok: true, output: '42.0' }],
      ['llm-eq-numeric-vs-prose', { llm: { eq: 42 } }, undefined, { ok: true, output: 'forty-two' }],
      ['llm-eq-number-operand-vs-prose-string-path', { llm: { eq: 42 } }, undefined, { ok: true, output: '42 sir' }],
      ['llm-eq-boolean-operand', { llm: { eq: true } }, undefined, { ok: true, output: 'TRUE' }],
      ['llm-neq-holds', { llm: { neq: 'NO' } }, undefined, { ok: true, output: 'yes' }],
      ['llm-neq-fails', { llm: { neq: 'NO' } }, undefined, { ok: true, output: 'no.' }],
      // Ordering: needs a finite numeric answer AND a numeric operand.
      ['llm-gte-numeric-holds', { llm: { gte: 5 } }, undefined, { ok: true, output: '7' }],
      ['llm-gte-numeric-fails', { llm: { gte: 5 } }, undefined, { ok: true, output: '3' }],
      ['llm-gte-boundary', { llm: { gte: 5 } }, undefined, { ok: true, output: '5' }],
      ['llm-lt-holds', { llm: { lt: 5 } }, undefined, { ok: true, output: '4.5' }],
      ['llm-gte-prose-declines', { llm: { gte: 5 } }, undefined, { ok: true, output: 'quite a lot' }],
      ['llm-gte-empty-declines', { llm: { gte: 5 } }, undefined, { ok: true, output: '   ' }],
      ['llm-gte-hex-answer', { llm: { gte: 5 } }, undefined, { ok: true, output: '0x10' }],
      ['llm-gte-infinity-declines', { llm: { gte: 5 } }, undefined, { ok: true, output: 'Infinity' }],
      ['llm-gt-string-operand-declines', { llm: { gt: { $param: 'material' } } }, undefined, { ok: true, output: '7' }],
      ['llm-anded-ordering', { llm: { gte: 5, lte: 10 } }, undefined, { ok: true, output: '7' }],
      ['llm-anded-ordering-one-fails', { llm: { gte: 5, lte: 6 } }, undefined, { ok: true, output: '7' }],
      // contains/ncontains: trimmed, case-insensitive, NO punctuation forgiveness.
      ['llm-contains-holds', { llm: { contains: 'west door' } }, undefined, { ok: true, output: 'the West Door' }],
      ['llm-contains-fails', { llm: { contains: 'east door' } }, undefined, { ok: true, output: 'the West Door' }],
      ['llm-contains-operand-padded', { llm: { contains: '  west  ' } }, undefined, { ok: true, output: 'the West Door' }],
      ['llm-contains-no-punctuation-forgiveness', { llm: { contains: 'door.' } }, undefined, { ok: true, output: 'the West Door' }],
      ['llm-ncontains-holds', { llm: { ncontains: 'east' } }, undefined, { ok: true, output: 'the West Door' }],
      ['llm-ncontains-fails', { llm: { ncontains: 'west' } }, undefined, { ok: true, output: 'the West Door' }],
      // A numeric needle can only arrive via a `$param` ref: the literal form is
      // `StringOperandSchema` and a bare `42` is rejected at LOAD (pinned in the
      // definition corpus). The ref form is only ref-checked, never type-checked,
      // so this is the one route to `String(operand)` on a number.
      ['llm-contains-numeric-param-needle', { llm: { contains: { $param: 'difficulty' } } }, undefined, { ok: true, output: 'exactly 10 of them' }],
      ['llm-contains-numeric-param-needle-fails', { llm: { contains: { $param: 'difficulty' } } }, undefined, { ok: true, output: 'exactly 11 of them' }],
      // The metadata twin: a numeric needle DECLINES fail-soft rather than
      // stringifying, because that arm demands a string on both sides.
      ['md-contains-numeric-param-needle-declines', { metadata: { house: { contains: { $param: 'difficulty' } } } }, SHEET],
      ['llm-contains-on-failed-consult', { llm: { contains: 'wire' } }, undefined, { ok: false, output: 'The wire went dead.' }],
      // ok ANDed with an answer comparator.
      ['llm-ok-and-eq-both-hold', { llm: { ok: true, eq: 'YES' } }, undefined, { ok: true, output: 'yes' }],
      ['llm-ok-and-eq-ok-fails-first', { llm: { ok: true, eq: 'YES' } }, undefined, { ok: false, output: 'yes' }],
      // A $param operand resolves; an unresolvable one would throw (load-checked).
      ['llm-param-operand-holds', { llm: { eq: { $param: 'material' } } }, undefined, { ok: true, output: 'BRASS' }],
      ['llm-param-operand-contains', { llm: { contains: { $param: 'material' } } }, undefined, { ok: true, output: 'a Brass fitting' }],
      // llm ANDed with the value subject.
      ['llm-anded-with-value-holds', { gt: 5, llm: { ok: true } }, undefined, { ok: true, output: 'YES' }],
      ['llm-anded-with-value-fails', { gt: 50, llm: { ok: true } }, undefined, { ok: true, output: 'YES' }],
    ];

    for (const entry of whens) {
      const [id, when, metadata] = entry;
      const llm = entry.length > 3 ? (entry[3] ?? undefined) : undefined;
      // Parsed through the real schema, then evaluated at a fixed subject. A
      // catch-all can only be the LAST outcome, so it stands alone. A `when.llm`
      // row needs the tool to declare an `llm` block or it would not load.
      const outcomes =
        when === true ? [CATCH_ALL] : [{ when, message: '-', state: 'info' }, CATCH_ALL];
      const testsLlm = typeof when === 'object' && when !== null && 'llm' in when;
      const def = define({ ...tool, ...(testsLlm ? LLM_BLOCK : {}), outcomes });
      const parsedWhen = def.outcomes[0].when as When;
      let out: unknown = null;
      let error: string | null = null;
      try {
        out = matchesWhen(parsedWhen, { value: 12, roll: 0.6, params, metadata, llm }, 'probe');
      } catch (e) {
        error = (e as Error).message;
      }
      emit({
        kind: 'matchesWhen',
        id,
        tool: def,
        when: parsedWhen,
        metadata: metadata ?? null,
        llm: llm ?? null,
        out,
        error,
      });
    }
  });
});

// ------------------------------------------------------------ executeCustomTool
describe('executeCustomTool', () => {
  it('emits', async () => {
    const OUTCOMES = [
      { when: { gte: 0.9 }, message: 'triumph at {{value}}', state: 'success' },
      { when: { gte: 0.5 }, message: 'partial at {{value}}', state: 'partial' },
      { when: true, message: 'failure at {{value}}', state: 'failure' },
    ];

    /**
     * The scripted oracle a case hands `executeCustomTool`, in a shape the port
     * can rebuild byte-for-byte. `null`/absent leaves the seam UNWIRED, which is
     * the "no invoker in this context" failure path.
     */
    type LlmScript =
      | { answer: string; provider?: string; model?: string }
      | { fail: string }
      | { throws: string }
      | null;

    function invokerFor(script: LlmScript, seen: { options: unknown; prompt: string | null }) {
      if (!script) return undefined;
      return async (prompt: string, options?: { maxOutputChars: number }) => {
        seen.prompt = prompt;
        seen.options = options ?? null;
        if ('throws' in script) throw new Error(script.throws);
        if ('fail' in script) return { ok: false as const, reason: script.fail };
        return {
          ok: true as const,
          output: script.answer,
          ...(script.provider ? { provider: script.provider } : {}),
          ...(script.model ? { model: script.model } : {}),
        };
      };
    }

    const cases: Array<
      [
        string,
        unknown,
        Record<string, unknown> | null | undefined,
        { private?: boolean; metadata?: Record<string, unknown> } | undefined,
        LlmScript?,
      ]
    > = [
      ['default-range', { name: 'probe', description: 'd', outcomes: OUTCOMES }, {}, undefined],
      [
        'explicit-range',
        { name: 'probe', description: 'd', roll: { min: 0, max: 20 }, outcomes: [{ when: true, message: '{{value}}', state: 'info' }] },
        {},
        undefined,
      ],
      // The transform order is load-bearing: multiply, then offset, THEN round.
      [
        'transform-order',
        { name: 'probe', description: 'd', roll: { min: 0, max: 10, multiplier: 3, offset: 0.5, round: true }, outcomes: [{ when: true, message: '{{value}} raw {{roll}}', state: 'info' }] },
        {},
        undefined,
      ],
      [
        'round-false',
        { name: 'probe', description: 'd', roll: { min: 0, max: 10, multiplier: 3, offset: 0.5 }, outcomes: [{ when: true, message: '{{value}}', state: 'info' }] },
        {},
        undefined,
      ],
      // A degenerate range must NOT draw — bytesConsumed pins it.
      [
        'min-equals-max-short-circuits',
        { name: 'probe', description: 'd', roll: { min: 7, max: 7 }, outcomes: [{ when: true, message: '{{value}}', state: 'info' }] },
        {},
        undefined,
      ],
      [
        'min-equals-max-with-transform',
        { name: 'probe', description: 'd', roll: { min: 7, max: 7, multiplier: 2, offset: 1 }, outcomes: [{ when: true, message: '{{value}}', state: 'info' }] },
        {},
        undefined,
      ],
      [
        'param-ref-bounds',
        {
          name: 'probe',
          description: 'd',
          parameters: { hi: { type: 'number', default: 20 } },
          roll: { min: 0, max: { $param: 'hi' } },
          outcomes: [{ when: true, message: '{{value}}', state: 'info' }],
        },
        { hi: 100 },
        undefined,
      ],
      // Dice: raw === value === total, and Form A's transform never applies.
      [
        'dice-simple',
        { name: 'probe', description: 'd', roll: '3d6', outcomes: [{ when: true, message: '{{value}} — {{dice}} — raw {{roll}}', state: 'info' }] },
        {},
        undefined,
      ],
      [
        'dice-with-modifier',
        { name: 'probe', description: 'd', roll: '3d6+2', outcomes: [{ when: true, message: '{{value}} — {{dice}}', state: 'info' }] },
        {},
        undefined,
      ],
      [
        'dice-d20',
        { name: 'probe', description: 'd', roll: '1d20', outcomes: [{ when: { gte: 15 }, message: 'hit {{value}}', state: 'success' }, { when: true, message: 'miss {{value}}', state: 'failure' }] },
        {},
        undefined,
      ],
      [
        'dice-negative-modifier',
        { name: 'probe', description: 'd', roll: '2d6-1', outcomes: [{ when: true, message: '{{value}} — {{dice}}', state: 'info' }] },
        {},
        undefined,
      ],
      // Visibility resolution.
      ['visibility-default', { name: 'probe', description: 'd', roll: { min: 1, max: 1 }, outcomes: [CATCH_ALL] }, {}, undefined],
      ['visibility-authored-whisper', { name: 'probe', description: 'd', defaultVisibility: 'whisper', roll: { min: 1, max: 1 }, outcomes: [CATCH_ALL] }, {}, undefined],
      ['visibility-override-private', { name: 'probe', description: 'd', roll: { min: 1, max: 1 }, outcomes: [CATCH_ALL] }, {}, { private: true }],
      ['visibility-override-public', { name: 'probe', description: 'd', defaultVisibility: 'whisper', roll: { min: 1, max: 1 }, outcomes: [CATCH_ALL] }, {}, { private: false }],
      // Errors.
      [
        'min-above-max',
        {
          name: 'probe',
          description: 'd',
          parameters: { lo: { type: 'number', default: 5 } },
          roll: { min: { $param: 'lo' }, max: 1 },
          outcomes: [CATCH_ALL],
        },
        {},
        undefined,
      ],
      ['unknown-param-run', { name: 'probe', description: 'd', roll: { min: 1, max: 1 }, outcomes: [CATCH_ALL] }, { nope: 1 }, undefined],
      [
        'first-match-wins',
        {
          name: 'probe',
          description: 'd',
          roll: { min: 3, max: 3 },
          outcomes: [
            { when: { gte: 1 }, message: 'first', state: 'success' },
            { when: { gte: 2 }, message: 'second', state: 'partial' },
            { when: true, message: 'third', state: 'info' },
          ],
        },
        {},
        undefined,
      ],
      // ---- the d68638b4 metadata subject + metadataTested record -----------
      // A metadata-gated outcome that hits, rendering {{metadata.*}} and
      // recording metadataTested. min===max draws nothing.
      [
        'metadata-gated-hits',
        {
          name: 'probe',
          description: 'd',
          roll: { min: 1, max: 1 },
          outcomes: [
            { when: { metadata: { clearance: { gte: 3 } } }, message: 'granted at {{metadata.clearance}}', state: 'success' },
            { when: true, message: 'denied', state: 'failure' },
          ],
        },
        {},
        { metadata: { clearance: 5 } },
      ],
      // The same table with no sheet → the metadata row declines, the catch-all
      // answers, metadataTested is absent from the result.
      [
        'metadata-gated-no-sheet',
        {
          name: 'probe',
          description: 'd',
          roll: { min: 1, max: 1 },
          outcomes: [
            { when: { metadata: { clearance: { gte: 3 } } }, message: 'granted', state: 'success' },
            { when: true, message: 'denied', state: 'failure' },
          ],
        },
        {},
        undefined,
      ],
      // metadataTested records only the keys the WINNING row tested (primitives),
      // never the whole sheet — `extra` is on the sheet but untested.
      [
        'metadata-tested-records-only-winner-keys',
        {
          name: 'probe',
          description: 'd',
          roll: { min: 1, max: 1 },
          outcomes: [
            { when: { metadata: { clearance: { gte: 3 }, house: { eq: 'Aurum' } } }, message: 'ok', state: 'success' },
            { when: true, message: 'no', state: 'failure' },
          ],
        },
        {},
        { metadata: { clearance: 5, house: 'Aurum', extra: 'ignored' } },
      ],
      // A non-primitive tested key is NOT recorded even when the row wins on its
      // other, primitive keys.
      [
        'metadata-tested-skips-non-primitive',
        {
          name: 'probe',
          description: 'd',
          roll: { min: 1, max: 1 },
          outcomes: [
            { when: { metadata: { clearance: { gte: 3 } } }, message: 'ok', state: 'success' },
            { when: true, message: 'no', state: 'failure' },
          ],
        },
        {},
        { metadata: { clearance: 5, roster: [1, 2, 3] } },
      ],
      // metadata + private together.
      [
        'metadata-with-private-override',
        {
          name: 'probe',
          description: 'd',
          roll: { min: 1, max: 1 },
          outcomes: [
            { when: { metadata: { clearance: { gte: 3 } } }, message: 'ok', state: 'success' },
            { when: true, message: 'no', state: 'failure' },
          ],
        },
        {},
        { private: true, metadata: { clearance: 5 } },
      ],

      // ---- the 616930db consult: the whole resolveLlmConsult table ----------
      // The prompt is rendered from the ROLL, so it quotes a value the consult
      // could not have known before the draw — and it carries no {{llm}}.
      [
        'llm-answered-branches-and-renders',
        {
          name: 'probe',
          description: 'd',
          roll: { min: 7, max: 7 },
          llm: { prompt: 'Is {{value}} auspicious? Answer YES or NO.', errorMessage: 'The wire went dead.' },
          outcomes: [
            { when: { llm: { eq: 'YES' } }, message: 'the oracle said {{llm}} at {{value}}', state: 'success' },
            { when: true, message: 'the oracle said {{llm}}', state: 'failure' },
          ],
        },
        {},
        undefined,
        { answer: '  Yes.  ', provider: 'openai', model: 'gpt-tiny' },
      ],
      // A failed consult never fails the run: output becomes the AUTHOR's words.
      [
        'llm-failed-becomes-error-message',
        {
          name: 'probe',
          description: 'd',
          roll: { min: 7, max: 7 },
          llm: { prompt: 'Speak.', errorMessage: 'The wire went dead.' },
          outcomes: [
            { when: { llm: { ok: true } }, message: 'answered: {{llm}}', state: 'success' },
            { when: true, message: 'silence: {{llm}}', state: 'failure' },
          ],
        },
        {},
        undefined,
        { fail: 'the provider refused' },
      ],
      // An invoker that throws lands in the same place as one that reports failure.
      [
        'llm-invoker-throws',
        {
          name: 'probe',
          description: 'd',
          roll: { min: 7, max: 7 },
          llm: { prompt: 'Speak.', errorMessage: 'The wire went dead.' },
          outcomes: [{ when: true, message: '{{llm}}', state: 'info' }],
        },
        {},
        undefined,
        { throws: 'the consult timed out after 60s' },
      ],
      // No invoker wired at all — the entrance that forgot, or the bench that
      // scripted nothing.
      [
        'llm-no-invoker-wired',
        {
          name: 'probe',
          description: 'd',
          roll: { min: 7, max: 7 },
          llm: { prompt: 'Speak.', errorMessage: 'The wire went dead.' },
          outcomes: [{ when: true, message: '{{llm}}', state: 'info' }],
        },
        {},
        undefined,
        null,
      ],
      // Empty after trim is a failure, not an empty answer.
      [
        'llm-empty-answer-is-a-failure',
        {
          name: 'probe',
          description: 'd',
          roll: { min: 7, max: 7 },
          llm: { prompt: 'Speak.', errorMessage: 'The wire went dead.' },
          outcomes: [{ when: true, message: '{{llm}}', state: 'info' }],
        },
        {},
        undefined,
        { answer: '   \n  ' },
      ],
      // trim, cap, RE-trim — the cap lands mid-word and the re-trim eats the
      // space it exposed.
      [
        'llm-maxoutput-trim-cap-retrim',
        {
          name: 'probe',
          description: 'd',
          roll: { min: 7, max: 7 },
          llm: { prompt: 'Speak.', errorMessage: 'nope', maxOutput: 10 },
          outcomes: [{ when: true, message: '[{{llm}}]', state: 'info' }],
        },
        {},
        undefined,
        { answer: '   abcdefghi jklmnop   ' },
      ],
      // The author's errorMessage is NEVER capped by maxOutput.
      [
        'llm-error-message-never-capped',
        {
          name: 'probe',
          description: 'd',
          roll: { min: 7, max: 7 },
          llm: { prompt: 'Speak.', errorMessage: 'The wire went dead, and stayed that way.', maxOutput: 5 },
          outcomes: [{ when: true, message: '[{{llm}}]', state: 'info' }],
        },
        {},
        undefined,
        { fail: 'nothing doing' },
      ],
      // The cap applies to the model's answer at the DEFAULT when unset.
      [
        'llm-default-cap-passed-to-invoker',
        {
          name: 'probe',
          description: 'd',
          roll: { min: 7, max: 7 },
          llm: { prompt: 'Speak.', errorMessage: 'nope' },
          outcomes: [{ when: true, message: '{{llm}}', state: 'info' }],
        },
        {},
        undefined,
        { answer: 'brief' },
      ],
      // provider/model ride only when the invoker offers them.
      [
        'llm-no-provider-or-model',
        {
          name: 'probe',
          description: 'd',
          roll: { min: 7, max: 7 },
          llm: { prompt: 'Speak.', errorMessage: 'nope' },
          outcomes: [{ when: true, message: '{{llm}}', state: 'info' }],
        },
        {},
        undefined,
        { answer: 'bare' },
      ],
      // The consult runs even when no outcome tests it — the record is kept.
      [
        'llm-untested-still-recorded',
        {
          name: 'probe',
          description: 'd',
          roll: { min: 7, max: 7 },
          llm: { prompt: 'Speak.', errorMessage: 'nope' },
          outcomes: [{ when: true, message: 'nothing about the oracle', state: 'info' }],
        },
        {},
        undefined,
        { answer: 'unheeded' },
      ],
      // A tool with no llm block never consults, even with an invoker in hand:
      // `llm` is absent from the result and {{llm}} stays verbatim.
      [
        'llm-block-absent-invoker-ignored',
        {
          name: 'probe',
          description: 'd',
          roll: { min: 7, max: 7 },
          outcomes: [{ when: true, message: 'x {{llm}} y', state: 'info' }],
        },
        {},
        undefined,
        { answer: 'never asked' },
      ],
      // The prompt renders params and metadata too, and the roll it quotes is
      // the DICE total — so the consult sees the draw. Dice DO consume bytes.
      [
        'llm-prompt-renders-dice-and-params',
        {
          name: 'probe',
          description: 'd',
          parameters: { target: { type: 'string', default: 'the door' } },
          roll: '3d6',
          llm: { prompt: 'Rolled {{value}} ({{dice}}) against {{params.target}}; clearance {{metadata.clearance}}.', errorMessage: 'nope' },
          outcomes: [{ when: true, message: '{{llm}}', state: 'info' }],
        },
        {},
        { metadata: { clearance: 5 } },
        { answer: 'noted' },
      ],
      // llm ANDed with metadata: both subjects on one row, metadataTested kept.
      [
        'llm-anded-with-metadata',
        {
          name: 'probe',
          description: 'd',
          roll: { min: 7, max: 7 },
          llm: { prompt: 'Speak.', errorMessage: 'nope' },
          outcomes: [
            { when: { llm: { contains: 'granted' }, metadata: { clearance: { gte: 3 } } }, message: 'in: {{llm}}', state: 'success' },
            { when: true, message: 'out', state: 'failure' },
          ],
        },
        {},
        { metadata: { clearance: 5 } },
        { answer: 'Access GRANTED to the west wing.' },
      ],
      // A containment test on a string param, in a real run (the strict arm).
      [
        'params-containment-in-a-run',
        {
          name: 'probe',
          description: 'd',
          parameters: { cargo: { type: 'string', default: 'silk and opium' } },
          roll: { min: 7, max: 7 },
          outcomes: [
            { when: { params: { cargo: { contains: 'opium' } } }, message: 'contraband', state: 'failure' },
            { when: true, message: 'clean', state: 'success' },
          ],
        },
        {},
        undefined,
      ],
      // ---- $state end-to-end (P4.d10, f48f34dc) ----
      // A $state roll bound engages the value (min == max → deterministic, no draw).
      [
        'state-roll-bounds-present',
        {
          name: 'fixed',
          description: 'd',
          roll: { min: { $state: 'game.n', fallback: 2 }, max: { $state: 'game.n', fallback: 2 } },
          outcomes: [CATCH_ALL],
        },
        {},
        { state: { game: { n: 9 } } },
      ],
      [
        'state-roll-bounds-absent',
        {
          name: 'fixed',
          description: 'd',
          roll: { min: { $state: 'game.n', fallback: 2 }, max: { $state: 'game.n', fallback: 2 } },
          outcomes: [CATCH_ALL],
        },
        {},
        { state: {} },
      ],
      // A $state operand decides which outcome wins.
      [
        'state-operand-gate-passes',
        {
          name: 'gate',
          description: 'd',
          roll: { min: 5, max: 5 },
          outcomes: [
            { when: { gte: { $state: 'game.difficulty', fallback: 10 } }, message: 'passed', state: 'success' },
            CATCH_ALL,
          ],
        },
        {},
        { state: { game: { difficulty: 3 } } },
      ],
      [
        'state-operand-gate-falls-back',
        {
          name: 'gate',
          description: 'd',
          roll: { min: 5, max: 5 },
          outcomes: [
            { when: { gte: { $state: 'game.difficulty', fallback: 10 } }, message: 'passed', state: 'success' },
            CATCH_ALL,
          ],
        },
        {},
        { state: {} },
      ],
      // {{state.path}} in the winning message + a $state parameter default.
      [
        'state-template-and-default',
        {
          name: 'weather',
          description: 'd',
          parameters: { bonus: { type: 'number', default: { $state: 'player.bonus', fallback: 1 } } },
          roll: { min: 1, max: 1 },
          outcomes: [
            { when: true, message: 'It is {{state.weather}} (depth {{state.depth.z}}), bonus {{params.bonus}}.', state: 'info' },
          ],
        },
        {},
        { state: { weather: 'foggy', depth: { z: 3 }, player: { bonus: 7 } } },
      ],
      // No state offered at all: every ref falls back (overrides.state omitted).
      [
        'state-omitted-entirely',
        {
          name: 'gate',
          description: 'd',
          roll: { min: { $state: 'game.n', fallback: 4 }, max: { $state: 'game.n', fallback: 4 } },
          outcomes: [CATCH_ALL],
        },
        {},
        undefined,
      ],

      // ---- chipLabel (P4.D35, c4d4b0de) -----------------------------------
      // Rendered ONCE, after the outcome is chosen, with the message's own
      // subjects — so it may quote anything the message may, including the
      // winning row's own numbers.
      [
        'chip-label-renders-with-message-subjects',
        {
          name: 'agent',
          description: 'd',
          chipLabel: 'Agent lambda — {{params.label}} ({{value}})',
          parameters: { label: { type: 'string', default: 'Jackie' } },
          roll: { min: 7, max: 7 },
          outcomes: [CATCH_ALL],
        },
        {},
        undefined,
      ],
      // An unknown placeholder renders VERBATIM — there is no load-time
      // reference rule on a chipLabel, only the renderTemplate doctrine.
      [
        'chip-label-unknown-placeholder-verbatim',
        {
          name: 'agent',
          description: 'd',
          chipLabel: 'x {{params.ghost}} y',
          roll: { min: 1, max: 1 },
          outcomes: [CATCH_ALL],
        },
        {},
        undefined,
      ],
      // The label may quote the consult, which has answered by the time it
      // renders (the message's subjects include `llm`).
      [
        'chip-label-quotes-the-consult',
        {
          name: 'agent',
          description: 'd',
          chipLabel: 'says {{llm}}',
          roll: { min: 1, max: 1 },
          llm: { prompt: 'Answer.', errorMessage: 'The wire went dead.' },
          outcomes: [CATCH_ALL],
        },
        {},
        undefined,
        { answer: 'the West Door' },
      ],
      // Absent chipLabel → the key is absent from the result, not empty.
      [
        'chip-label-absent',
        { name: 'agent', description: 'd', roll: { min: 1, max: 1 }, outcomes: [CATCH_ALL] },
        {},
        undefined,
      ],

      // ---- effects: resolution (still PURE — nothing is written here) ------
      [
        'effects-literals-and-expression',
        {
          name: 'lockpick',
          description: 'd',
          parameters: { scale: { type: 'number', default: 2 } },
          roll: { min: 6, max: 6 },
          effects: [
            { target: 'state.encounter.count', value: 3 },
            { target: 'metadata.lockBroken', value: true },
            { target: 'metadata.lockpick', value: "'broken pick'" },
            { target: 'state.tally', value: '{{state.tally}} + {{params.scale}}' },
          ],
          outcomes: [CATCH_ALL],
        },
        {},
        { state: { tally: 10 } },
      ],
      // The `outcome` subject: one effect fires on the winner, one does not.
      [
        'effects-outcome-conditioned',
        {
          name: 'saving_throw',
          description: 'd',
          roll: { min: 18, max: 18 },
          effects: [
            { when: { outcome: { eq: 'success' } }, target: 'state.wins', value: 1 },
            { when: { outcome: { eq: 'failure' } }, target: 'state.losses', value: 1 },
            { when: { outcome: { neq: 'failure' } }, target: 'state.attempts', value: 1 },
          ],
          outcomes: [
            { when: { gte: 15 }, message: 'made it', state: 'success' },
            CATCH_ALL,
          ],
        },
        {},
        undefined,
      ],
      // Every skip reason the resolver can record, in one run: a condition that
      // does not hold, an expression that cannot evaluate (a ref resolving to
      // nothing, and a division by zero).
      [
        'effects-skip-reasons',
        {
          name: 'skips',
          description: 'd',
          roll: { min: 1, max: 1 },
          effects: [
            { when: { gte: 100 }, target: 'state.never', value: 1 },
            { target: 'state.unresolved', value: '{{metadata.absent}} + 1' },
            { target: 'state.divzero', value: '{{value}} / 0' },
            { target: 'state.fine', value: '{{value}} + 1' },
          ],
          outcomes: [CATCH_ALL],
        },
        {},
        undefined,
      ],
      // The other subjects an effect condition shares with an outcome row.
      [
        'effects-condition-over-shared-subjects',
        {
          name: 'shared',
          description: 'd',
          parameters: { scale: { type: 'number', default: 4 } },
          roll: { min: 9, max: 9 },
          effects: [
            { when: { roll: { gte: 5 } }, target: 'state.bigRoll', value: true },
            { when: { params: { scale: { gt: 3 } } }, target: 'state.scaled', value: true },
            { when: { metadata: { clearance: { gte: 3 } } }, target: 'state.cleared', value: true },
            { when: { metadata: { clearance: { gte: 99 } } }, target: 'state.never', value: true },
          ],
          outcomes: [CATCH_ALL],
        },
        {},
        { metadata: { clearance: 5 } },
      ],
      // {{dice}} is a STRING subject in an expression — Form B supplies the
      // breakdown, and concatenation is the only thing it can take part in.
      [
        'effects-dice-subject-is-a-string',
        {
          name: 'dicey',
          description: 'd',
          roll: '3d6',
          effects: [
            { target: 'state.lastRoll', value: "'rolled ' + {{dice}}" },
            { target: 'state.arithmetic', value: '{{dice}} * 2' },
          ],
          outcomes: [CATCH_ALL],
        },
        {},
        undefined,
      ],
      // An `llm` reference in an effect value, and an `llm` when-subject.
      [
        'effects-consult-subjects',
        {
          name: 'consulted',
          description: 'd',
          roll: { min: 1, max: 1 },
          llm: { prompt: 'Answer.', errorMessage: 'The wire went dead.' },
          effects: [
            { target: 'metadata.verdict', value: '{{llm}}' },
            { when: { llm: { ok: true } }, target: 'state.consulted', value: true },
          ],
          outcomes: [CATCH_ALL],
        },
        {},
        undefined,
        { answer: 'the West Door' },
      ],
      // An empty `effects: []` array declares none, so the key is ABSENT from
      // the result rather than an empty list.
      [
        'effects-empty-array-yields-no-key',
        { name: 'none', description: 'd', roll: { min: 1, max: 1 }, effects: [], outcomes: [CATCH_ALL] },
        {},
        undefined,
      ],
      // Order is DECLARATION order, and each entry keeps its own index — a
      // skipped entry does not renumber the ones after it.
      [
        'effects-indices-survive-skips',
        {
          name: 'indexed',
          description: 'd',
          roll: { min: 1, max: 1 },
          effects: [
            { when: { gte: 100 }, target: 'state.a', value: 1 },
            { target: 'state.b', value: 2 },
            { when: { gte: 100 }, target: 'state.c', value: 3 },
            { target: 'state.d', value: 4 },
          ],
          outcomes: [CATCH_ALL],
        },
        {},
        undefined,
      ],
      // A bracket-index target and a whole-key metadata target: the parsed
      // `target` object rides out in the result, so its shape is pinned here.
      [
        'effects-target-shapes',
        {
          name: 'shapes',
          description: 'd',
          roll: { min: 1, max: 1 },
          effects: [
            { target: 'state.party[0].hp', value: 5 },
            { target: 'metadata.ansible.tool', value: true },
            { target: 'state.encounter._notes', value: "'deep underscore is fine'" },
          ],
          outcomes: [CATCH_ALL],
        },
        {},
        undefined,
      ],
    ];

    let index = 0;
    for (const entry of cases) {
      const [id, doc, supplied, overrides] = entry;
      const script: LlmScript = entry.length > 4 ? (entry[4] ?? null) : null;
      const i = index++;
      const def = define(doc);
      const p = pool(i + 1);
      mockRngState = { pool: p, cursor: 0 };

      const seen: { options: unknown; prompt: string | null } = { options: null, prompt: null };
      const llmInvoke = invokerFor(script, seen);

      let out: unknown = null;
      let error: string | null = null;
      try {
        out = await executeCustomTool(def, supplied, {
          ...(overrides ?? {}),
          ...(llmInvoke ? { llmInvoke } : {}),
        });
      } catch (e) {
        error = (e as Error).message;
      }

      emit({
        kind: 'executeCustomTool',
        id,
        tool: def,
        supplied: supplied ?? null,
        overrides: overrides ?? null,
        // The scripted oracle, in the shape the port rebuilds.
        llmScript: script,
        // What the core actually asked, and with what advertised cap — the two
        // facts the result alone would not pin.
        llmPromptSeen: seen.prompt,
        llmOptionsSeen: seen.options,
        poolHex: p.toString('hex'),
        // The cursor is the point: a degenerate range draws NOTHING.
        bytesConsumed: mockRngState.cursor,
        out,
        error,
      });
    }
  });
});

// ------------------------------------------------------- $state (P4.d10, f48f34dc)
describe('$state resolution', () => {
  it('emits', async () => {
    // The mock merged state every row resolves against.
    const STATE: Record<string, unknown> = {
      weather: 'foggy',
      crew: [1, 2],
      depth: { z: 3 },
      lamp: true,
      n: 7,
      frac: 1.5,
    };

    // renderTemplate: the {{state.path}} arm.
    const tmplTool = define({ name: 'probe', description: 'd', outcomes: [CATCH_ALL] });
    const tmplParams = resolveParams(tmplTool, {}, STATE);
    const templates: Array<[string, string]> = [
      ['state-string', 'It is {{state.weather}}.'],
      ['state-nested-number', 'depth {{state.depth.z}}'],
      ['state-boolean', 'lamp {{state.lamp}}'],
      ['state-float-formats', 'f {{state.frac}}'],
      ['state-missing-left-verbatim', 'x {{state.ghost}} y'],
      ['state-non-primitive-left-verbatim', 'crew {{state.crew}} here'],
      ['state-prefix-only', 'bare {{state.}}'],
      // `state.` is path-parsed rather than key-indexed, but a prototype name is
      // still a legal path segment — pinned for the same reason.
      ['state-prototype-tostring', 'x {{state.toString}} y'],
      ['state-whitespace', '{{  state.weather  }}'],
    ];
    for (const [id, message] of templates) {
      emit({
        kind: 'renderTemplate',
        id,
        tool: tmplTool,
        message,
        metadata: {},
        llm: null,
        state: STATE,
        out: renderTemplate(message, {
          value: 12.3456,
          roll: 0.6789,
          dice: '3d6: [4, 2, 6] = 12',
          params: tmplParams,
          metadata: {},
          state: STATE,
        }),
      });
    }

    // resolveParams: a $state default across present / absent / wrong-type.
    const defTool = define({
      name: 'draw',
      description: 'd',
      parameters: { bonus: { type: 'number', default: { $state: 'player.bonus', fallback: 1 } } },
      outcomes: [CATCH_ALL],
    });
    const paramCases: Array<[string, Record<string, unknown>]> = [
      ['state-default-present', { player: { bonus: 7 } }],
      ['state-default-absent', {}],
      ['state-default-wrong-type', { player: { bonus: 'huge' } }],
      ['state-default-non-finite-guard', { player: { bonus: 7.25 } }],
    ];
    for (const [id, state] of paramCases) {
      let out: unknown = null;
      let error: string | null = null;
      try {
        out = resolveParams(defTool, {}, state);
      } catch (e) {
        error = (e as Error).message;
      }
      emit({ kind: 'resolveParams', id, tool: defTool, supplied: null, state, out, error });
    }

    // matchesWhen: $state operands on ordering / eq / containment / metadata.
    const whenTool = (when: unknown, extra: Record<string, unknown> = {}) =>
      define({
        name: 'probe',
        description: 'd',
        ...extra,
        outcomes: [
          { when, message: '-', state: 'info' },
          CATCH_ALL,
        ],
      });
    const whenCases: Array<[string, QtapCustomTool, Record<string, unknown>, Record<string, unknown> | null]> = [
      // value 12 >= difficulty(3) → true; absent → fallback 20 → false.
      ['state-gte-present', whenTool({ gte: { $state: 'game.difficulty', fallback: 20 } }), { game: { difficulty: 3 } }, null],
      ['state-gte-falls-back', whenTool({ gte: { $state: 'game.difficulty', fallback: 20 } }), {}, null],
      // eq against a $state string on a string param.
      ['state-eq-string-param', whenTool(
        { params: { material: { eq: { $state: 'game.mat', fallback: 'iron' } } } },
        { parameters: { material: { type: 'string', default: 'brass' } } },
      ), { game: { mat: 'brass' } }, null],
      ['state-eq-string-param-falls-back', whenTool(
        { params: { material: { eq: { $state: 'game.mat', fallback: 'iron' } } } },
        { parameters: { material: { type: 'string', default: 'brass' } } },
      ), {}, null],
      // containment with a $state needle.
      ['state-contains-needle', whenTool(
        { params: { cargo: { contains: { $state: 'game.needle', fallback: 'silk' } } } },
        { parameters: { cargo: { type: 'string', default: 'silk and opium' } } },
      ), { game: { needle: 'opium' } }, null],
      // a metadata comparator with a $state operand.
      ['state-metadata-operand', whenTool(
        { metadata: { clearance: { gte: { $state: 'game.bar', fallback: 9 } } } },
      ), { game: { bar: 2 } }, { clearance: 5 }],
      // wrong-typed state value at the path → the fallback decides.
      ['state-wrong-type-falls-back', whenTool({ gte: { $state: 'game.difficulty', fallback: 20 } }), { game: { difficulty: 'hard' } }, null],
    ];
    for (const [id, tool, state, metadata] of whenCases) {
      const params = resolveParams(tool, {}, state);
      let out: unknown = null;
      let error: string | null = null;
      try {
        out = matchesWhen(
          tool.outcomes[0].when as When,
          { value: 12, roll: 0.6, params, metadata: metadata ?? {}, state },
          'probe',
        );
      } catch (e) {
        error = (e as Error).message;
      }
      emit({ kind: 'matchesWhen', id, tool, metadata: metadata ?? null, llm: null, state, out, error });
    }
  });
});

afterAll(() => {
  if (!OUT) throw new Error('set QT_ORACLE_OUT');
  fs.writeFileSync(OUT, rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
});
