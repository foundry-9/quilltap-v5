import { describe, expect, it } from 'vitest';

import {
  ALMANACK_NAME,
  ALMANACK_PHASE_COUNT,
  ALMANACK_PHASES,
  ALMANACK_TITLE,
} from './almanack-phases';

/**
 * The §1 phases mirror, pinned literally.
 *
 * v4's manifest is ONE file shared by its server pipeline and its card
 * (`lib/tools/almanack/phases.ts`). v5 cannot share across the Rust/TS boundary,
 * so the manifest exists three times — v4's file, P4.37's Rust constant, and the
 * TS mirror this spec guards. Because there is no oracle to diff against at
 * runtime, the equivalence obligation is discharged the way the byte-copied
 * schema assets discharge theirs: the values are asserted LITERALLY here, so
 * re-reading v4's file is a mechanical diff against this one block, and a drift
 * on either side goes red rather than quietly re-weighting the progress bar.
 *
 * Transcribed at v4 `f7f1a956`.
 */
describe('ALMANACK_PHASES (mirror of v4 lib/tools/almanack/phases.ts)', () => {
  it('carries v4’s seven phases in order, with byte-equal keys/labels/shares', () => {
    expect(ALMANACK_PHASES).toEqual([
      { key: 'premises', label: 'Taking the measure of the premises…', estimatedShare: 0.05 },
      { key: 'machinery', label: 'Cataloguing the machinery…', estimatedShare: 0.25 },
      { key: 'ledgers', label: 'Auditing the ledgers…', estimatedShare: 0.2 },
      { key: 'scriptorium', label: 'Touring the Scriptorium…', estimatedShare: 0.2 },
      { key: 'personae', label: 'Assembling the dramatis personae…', estimatedShare: 0.12 },
      { key: 'wire-records', label: 'Reading the wire records…', estimatedShare: 0.13 },
      { key: 'binding', label: 'Binding the volume…', estimatedShare: 0.05 },
    ]);
  });

  it('the shares sum to 1 (v4’s stated invariant) and the count matches', () => {
    const sum = ALMANACK_PHASES.reduce((acc, p) => acc + p.estimatedShare, 0);
    // Float addition of the seven literals lands a hair off 1; v4 documents the
    // invariant as exact, so pin it at the precision the arithmetic allows.
    expect(sum).toBeCloseTo(1, 10);
    expect(ALMANACK_PHASE_COUNT).toBe(7);
    expect(ALMANACK_PHASES).toHaveLength(ALMANACK_PHASE_COUNT);
  });

  it('spells the feature the one way every surface must', () => {
    expect(ALMANACK_NAME).toBe('The Almanack');
    expect(ALMANACK_TITLE).toBe('The Almanack (System Report)');
  });
});
