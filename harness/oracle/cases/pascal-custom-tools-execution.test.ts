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
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
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

    const messages: Array<[string, string]> = [
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
      ['empty-braces', 'empty {{}} here'],
      ['adjacent', '{{value}}{{value}}'],
      ['repeated', '{{value}} and {{value}} and {{roll}}'],
      ['no-placeholder', 'nothing to do'],
      ['brace-soup', '{{{value}}}'],
      ['unclosed', 'a {{value b'],
      ['nested-ish', '{{val{{value}}ue}}'],
    ];

    for (const [id, message] of messages) {
      emit({
        kind: 'renderTemplate',
        id,
        // Emitted so the port re-parses the definition through its own schema
        // rather than needing a Deserialize for the parsed shape.
        tool,
        message,
        out: renderTemplate(message, { value: 12.3456, roll: 0.6789, dice: '3d6: [4, 2, 6] = 12', params }),
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

    const whens: Array<[string, unknown]> = [
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
    ];

    for (const [id, when] of whens) {
      // Parsed through the real schema, then evaluated at a fixed subject. A
      // catch-all can only be the LAST outcome, so it stands alone.
      const outcomes =
        when === true ? [CATCH_ALL] : [{ when, message: '-', state: 'info' }, CATCH_ALL];
      const def = define({ ...tool, outcomes });
      const parsedWhen = def.outcomes[0].when as When;
      let out: unknown = null;
      let error: string | null = null;
      try {
        out = matchesWhen(parsedWhen, { value: 12, roll: 0.6, params }, 'probe');
      } catch (e) {
        error = (e as Error).message;
      }
      emit({ kind: 'matchesWhen', id, tool: def, when: parsedWhen, out, error });
    }
  });
});

// ------------------------------------------------------------ executeCustomTool
describe('executeCustomTool', () => {
  it('emits', () => {
    const OUTCOMES = [
      { when: { gte: 0.9 }, message: 'triumph at {{value}}', state: 'success' },
      { when: { gte: 0.5 }, message: 'partial at {{value}}', state: 'partial' },
      { when: true, message: 'failure at {{value}}', state: 'failure' },
    ];

    const cases: Array<[string, unknown, Record<string, unknown> | null | undefined, { private?: boolean } | undefined]> = [
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
    ];

    cases.forEach(([id, doc, supplied, overrides], i) => {
      const def = define(doc);
      const p = pool(i + 1);
      mockRngState = { pool: p, cursor: 0 };

      let out: unknown = null;
      let error: string | null = null;
      try {
        out = executeCustomTool(def, supplied, overrides);
      } catch (e) {
        error = (e as Error).message;
      }

      emit({
        kind: 'executeCustomTool',
        id,
        tool: def,
        supplied: supplied ?? null,
        overrides: overrides ?? null,
        poolHex: p.toString('hex'),
        // The cursor is the point: a degenerate range draws NOTHING.
        bytesConsumed: mockRngState.cursor,
        out,
        error,
      });
    });
  });
});

afterAll(() => {
  if (!OUT) throw new Error('set QT_ORACLE_OUT');
  fs.writeFileSync(OUT, rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
});
