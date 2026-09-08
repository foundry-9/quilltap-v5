/**
 * Parity specs for `schema.ts` — v4's `__tests__/unit/lib/progressions/
 * schema.test.ts` at `25f534c0b`, transcribed case for case.
 *
 * The accept/reject matrix pins what a progression IS: which fields are
 * required, which defaults fill in, and which malformed entries the runtime
 * refuses. v4's third suite in that file — the `qtap-progression.schema.json`
 * mirror agreement — lives in `schema-mirror.spec.ts` beside the vendored
 * schema.
 *
 * The exact rejection SENTENCES are pinned separately and more strictly by
 * `schema.oracle.spec.ts`, which replays a corpus recorded from v4's real Zod.
 */

import { describe, expect, it } from 'vitest';

import {
  MAX_PROGRESSIONS_PER_CHARACTER,
  PROGRESSION_ID_PATTERN,
  REPORT_FREQUENCY_PATTERN,
  isWritableProgressionField,
  parseIsoInstant,
  safeParseProgression,
  safeParseProgressions,
  type Progression,
} from './schema';

/** The narrowest progression the schema will accept — v4's `BASE`. */
const BASE = {
  name: 'Cannon recharge',
  startTime: '2026-09-08T14:02:10Z',
  endTime: '2026-09-08T14:12:10Z',
  timeIncrement: 'minute',
};

function accepts(doc: unknown): boolean {
  return safeParseProgression(doc).success;
}

/** v4's `ProgressionSchema.parse` — the throwing form the tests lean on. */
function parse(doc: unknown): Progression {
  const result = safeParseProgression(doc);
  if (!result.success) throw new Error('expected the progression to parse');
  return result.data;
}

function rejection(doc: unknown): string {
  const result = safeParseProgression(doc);
  if (result.success) throw new Error('expected the progression to be rejected, but it parsed');
  return result.issues.map((i) => `${i.path.join('.')}: ${i.message}`).join('; ');
}

describe('parseIsoInstant', () => {
  it('takes an instant with Z', () => {
    expect(parseIsoInstant('2026-08-01T00:00:00Z')).toBe(Date.parse('2026-08-01T00:00:00Z'));
  });

  it('takes an instant with a numeric offset, colon or not', () => {
    expect(parseIsoInstant('2026-08-01T00:00:00-05:00')).toBe(Date.parse('2026-08-01T05:00:00Z'));
    expect(parseIsoInstant('2026-08-01T00:00:00-0500')).toBe(Date.parse('2026-08-01T05:00:00Z'));
  });

  it('takes fractional seconds', () => {
    expect(Number.isFinite(parseIsoInstant('2026-08-01T00:00:00.123Z'))).toBe(true);
  });

  it('takes a minute-precision instant', () => {
    expect(parseIsoInstant('2026-08-01T00:00Z')).toBe(Date.parse('2026-08-01T00:00:00Z'));
  });

  it('refuses a timestamp with no zone — a local reading is not an instant', () => {
    expect(Number.isNaN(parseIsoInstant('2026-08-01T00:00:00'))).toBe(true);
  });

  it('refuses a bare date, a year, and prose', () => {
    for (const value of ['2026-08-01', '2026', 'next Tuesday', '']) {
      expect(Number.isNaN(parseIsoInstant(value))).toBe(true);
    }
  });

  it('refuses a non-string', () => {
    for (const value of [null, undefined, 42, {}, []]) {
      expect(Number.isNaN(parseIsoInstant(value))).toBe(true);
    }
  });

  it('refuses a shape-legal date that names no real day', () => {
    expect(Number.isNaN(parseIsoInstant('2026-13-45T00:00:00Z'))).toBe(true);
  });
});

describe('ProgressionSchema', () => {
  it('accepts the narrowest entry', () => {
    expect(accepts(BASE)).toBe(true);
  });

  it('fills the documented defaults', () => {
    const parsed = parse(BASE);
    expect(parsed.percentageReport).toBe(true);
    expect(parsed.reportFrequency).toBe('turn');
    expect(parsed.onComplete).toBe('keep');
  });

  it('accepts the fully furnished entry from the spec', () => {
    expect(
      accepts({
        ...BASE,
        description: 'You are carrying a child.',
        percentageReport: false,
        reportFrequency: '1h',
        quantity: { total: 1.0, unit: 'MJ', precision: 1 },
        reportTemplate: '{{description}} You are {{elapsedWhole}} along; due in {{remaining}}.',
        onComplete: 'once',
        updatedAt: '2026-09-08T14:02:10Z',
      }),
    ).toBe(true);
  });

  it('defaults quantity precision to one decimal place', () => {
    const parsed = parse({ ...BASE, quantity: { total: 1, unit: 'MJ' } });
    expect(parsed.quantity?.precision).toBe(1);
  });

  it('requires endTime strictly after startTime', () => {
    expect(rejection({ ...BASE, endTime: BASE.startTime })).toContain('strictly after startTime');
    expect(rejection({ ...BASE, endTime: '2026-09-08T14:00:00Z' })).toContain(
      'strictly after startTime',
    );
  });

  it('refuses an unknown key — the shape is strict so a v2 field can be added additively', () => {
    expect(accepts({ ...BASE, stages: [] })).toBe(false);
  });

  it('refuses each missing required field', () => {
    for (const field of ['name', 'startTime', 'endTime', 'timeIncrement']) {
      const doc: Record<string, unknown> = { ...BASE };
      delete doc[field];
      expect(accepts(doc)).toBe(false);
    }
  });

  it('refuses an empty name and one over 80 characters', () => {
    expect(accepts({ ...BASE, name: '' })).toBe(false);
    expect(accepts({ ...BASE, name: 'x'.repeat(81) })).toBe(false);
    expect(accepts({ ...BASE, name: 'x'.repeat(80) })).toBe(true);
  });

  it('refuses an increment outside the seven units', () => {
    expect(accepts({ ...BASE, timeIncrement: 'fortnight' })).toBe(false);
  });

  it('refuses a quantity total that is zero, negative or non-finite', () => {
    for (const total of [0, -1, Infinity, NaN]) {
      expect(accepts({ ...BASE, quantity: { total, unit: 'MJ' } })).toBe(false);
    }
  });

  it('refuses a report template over the ceiling and an empty one', () => {
    expect(accepts({ ...BASE, reportTemplate: '' })).toBe(false);
    expect(accepts({ ...BASE, reportTemplate: 'x'.repeat(501) })).toBe(false);
    expect(accepts({ ...BASE, reportTemplate: 'x'.repeat(500) })).toBe(true);
  });

  it('refuses an onComplete outside keep/once', () => {
    expect(accepts({ ...BASE, onComplete: 'delete' })).toBe(false);
  });
});

describe('REPORT_FREQUENCY_PATTERN', () => {
  const takes = (value: string) => REPORT_FREQUENCY_PATTERN.test(value);

  it('takes the two words', () => {
    expect(takes('turn')).toBe(true);
    expect(takes('increment')).toBe(true);
  });

  it('takes <n><unit> for each of the five period units', () => {
    for (const value of ['30s', '5m', '1h', '2d', '3w']) expect(takes(value)).toBe(true);
  });

  it('refuses a zero or leading-zero count, a bare unit, and a bare number', () => {
    for (const value of ['0h', '01h', 'h', '5', '']) expect(takes(value)).toBe(false);
  });

  it('refuses month and year periods — those are increments, not wall-clock buckets', () => {
    expect(takes('1M')).toBe(false);
    expect(takes('1y')).toBe(false);
  });

  it('refuses free text and a cron expression', () => {
    expect(takes('every hour')).toBe(false);
    expect(takes('0 * * * *')).toBe(false);
  });

  it('is the rule the schema actually applies', () => {
    expect(accepts({ ...BASE, reportFrequency: '90m' })).toBe(true);
    expect(accepts({ ...BASE, reportFrequency: 'sometimes' })).toBe(false);
  });
});

describe('PROGRESSION_ID_PATTERN', () => {
  it('takes lowercase identifiers with digits, underscores and hyphens', () => {
    for (const id of ['cannon', 'c', 'main_gun-2', 'a'.repeat(64)]) {
      expect(PROGRESSION_ID_PATTERN.test(id)).toBe(true);
    }
  });

  it('refuses uppercase, a leading digit, a leading underscore, spaces, dots and 65 characters', () => {
    for (const id of ['Cannon', '2cannon', '_cannon', 'main gun', 'a.b', '', 'a'.repeat(65)]) {
      expect(PROGRESSION_ID_PATTERN.test(id)).toBe(false);
    }
  });
});

describe('ProgressionsSchema', () => {
  it('accepts a record keyed by identifier', () => {
    expect(safeParseProgressions({ cannon: BASE, pregnancy: BASE }).success).toBe(true);
  });

  it('accepts the empty record — a character carrying nothing', () => {
    expect(safeParseProgressions({}).success).toBe(true);
  });

  it('refuses a key that is not an identifier', () => {
    expect(safeParseProgressions({ 'Cannon Recharge': BASE }).success).toBe(false);
  });

  it(`refuses more than ${MAX_PROGRESSIONS_PER_CHARACTER} entries`, () => {
    const build = (count: number) =>
      Object.fromEntries(Array.from({ length: count }, (_, i) => [`p${i}`, BASE]));
    expect(safeParseProgressions(build(MAX_PROGRESSIONS_PER_CHARACTER)).success).toBe(true);
    expect(safeParseProgressions(build(MAX_PROGRESSIONS_PER_CHARACTER + 1)).success).toBe(false);
  });
});

describe('isWritableProgressionField', () => {
  it('admits every field a Pascal effect may write, plus the remove pseudo-field', () => {
    for (const field of [
      'name',
      'description',
      'startTime',
      'endTime',
      'timeIncrement',
      'percentageReport',
      'reportFrequency',
      'onComplete',
      'reportTemplate',
      'quantity.total',
      'quantity.unit',
      'quantity.precision',
      'remove',
    ]) {
      expect(isWritableProgressionField(field)).toBe(true);
    }
  });

  it('refuses updatedAt — the applier stamps that, an author does not', () => {
    expect(isWritableProgressionField('updatedAt')).toBe(false);
  });

  it('refuses derived fields and the whole quantity block', () => {
    for (const field of ['percent', 'complete', 'elapsed', 'quantity', 'id', '']) {
      expect(isWritableProgressionField(field)).toBe(false);
    }
  });
});
