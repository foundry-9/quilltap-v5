import Ajv2020 from 'ajv/dist/2020';
import { describe, expect, it } from 'vitest';

import mirror from '../../../public/schemas/qtap-progression.schema.json';
import { MAX_PROGRESSIONS_PER_CHARACTER, safeParseProgressions } from './schema';

/**
 * The mirror suite — v4's third suite in
 * `__tests__/unit/lib/progressions/schema.test.ts` at `25f534c0b`, transcribed,
 * whose header rides across:
 *
 * > The agreement suite guards `public/schemas/qtap-progression.schema.json`
 * > against the Zod schema, which is the runtime source of truth — a mirror
 * > that has drifted is worse than none, because it green-lights a
 * > `metadata.json` the loader will drop, or red-lines one it would have kept.
 * >
 * > One corpus, both validators, assert they agree. The `endTime > startTime`
 * > cross-field rule is the one accepted divergence — JSON Schema draft 2020-12
 * > cannot compare two sibling properties — and is asserted explicitly below
 * > rather than silently tolerated.
 *
 * v5 runs the same corpus through its own hand-ported `safeParseProgressions`
 * and through Ajv over the schema file the SPA actually SERVES (a byte-copy of
 * v4's, held there by `crates/quilltap-harness/tests/public_schemas_vendor_
 * guard.rs`). So this asserts three things agreeing, not two: v4's Zod, v5's
 * port, and the published mirror.
 *
 * @module progressions/schema-mirror.spec
 */

const BASE = {
  name: 'Cannon recharge',
  startTime: '2026-09-08T14:02:10Z',
  endTime: '2026-09-08T14:12:10Z',
  timeIncrement: 'minute',
};

describe('qtap-progression.schema.json mirrors the runtime schema', () => {
  const ajv = new Ajv2020({ strict: false, allErrors: true });
  const validate = ajv.compile(mirror);

  /** Every specimen, and whether the RUNTIME takes it — v4's own corpus. */
  const CORPUS: Array<{ label: string; doc: unknown; zodAccepts: boolean }> = [
    { label: 'the narrowest entry', doc: { cannon: BASE }, zodAccepts: true },
    { label: 'the empty record', doc: {}, zodAccepts: true },
    {
      label: 'a fully furnished entry',
      doc: {
        pregnancy: {
          name: 'Pregnancy',
          description: 'You are carrying a child.',
          startTime: '2026-08-01T00:00:00Z',
          endTime: '2027-05-01T00:00:00Z',
          timeIncrement: 'week',
          percentageReport: false,
          reportFrequency: '1h',
          reportTemplate: '{{description}} You are {{elapsedWhole}} along; due in {{remaining}}.',
          onComplete: 'keep',
          updatedAt: '2026-08-01T00:00:00Z',
        },
      },
      zodAccepts: true,
    },
    {
      label: 'an entry with a quantity block',
      doc: { cannon: { ...BASE, quantity: { total: 1.0, unit: 'MJ', precision: 1 } } },
      zodAccepts: true,
    },
    { label: 'an unknown field', doc: { cannon: { ...BASE, stages: [] } }, zodAccepts: false },
    { label: 'a non-identifier id', doc: { 'Cannon Recharge': BASE }, zodAccepts: false },
    {
      label: 'a missing timeIncrement',
      doc: { cannon: { ...BASE, timeIncrement: undefined } },
      zodAccepts: false,
    },
    {
      label: 'an unknown increment',
      doc: { cannon: { ...BASE, timeIncrement: 'fortnight' } },
      zodAccepts: false,
    },
    {
      label: 'a zoneless startTime',
      doc: { cannon: { ...BASE, startTime: '2026-09-08T14:02:10' } },
      zodAccepts: false,
    },
    {
      label: 'a cron cadence',
      doc: { cannon: { ...BASE, reportFrequency: '0 * * * *' } },
      zodAccepts: false,
    },
    {
      label: 'a zero-count cadence',
      doc: { cannon: { ...BASE, reportFrequency: '0h' } },
      zodAccepts: false,
    },
    { label: 'an empty name', doc: { cannon: { ...BASE, name: '' } }, zodAccepts: false },
    {
      label: 'a non-positive quantity total',
      doc: { cannon: { ...BASE, quantity: { total: 0, unit: 'MJ' } } },
      zodAccepts: false,
    },
    {
      label: 'an onComplete outside the enum',
      doc: { cannon: { ...BASE, onComplete: 'delete' } },
      zodAccepts: false,
    },
    {
      label: `more than ${MAX_PROGRESSIONS_PER_CHARACTER} entries`,
      doc: Object.fromEntries(
        Array.from({ length: MAX_PROGRESSIONS_PER_CHARACTER + 1 }, (_, i) => [`p${i}`, BASE]),
      ),
      zodAccepts: false,
    },
  ];

  it.each(CORPUS)('agrees on $label', ({ doc, zodAccepts }) => {
    // `undefined` isn't representable in JSON; JSON.parse/stringify is how the
    // file would actually reach a validator, and drops the key as a hand edit
    // omitting it would.
    const asJson: unknown = JSON.parse(JSON.stringify(doc));
    expect(safeParseProgressions(asJson).success).toBe(zodAccepts);
    expect(validate(asJson)).toBe(zodAccepts);
  });

  it('diverges on the one cross-field rule JSON Schema cannot express', () => {
    const backwards = { cannon: { ...BASE, endTime: '2026-09-08T14:00:00Z' } };
    expect(safeParseProgressions(backwards).success).toBe(false);
    expect(validate(backwards)).toBe(true);
  });
});
