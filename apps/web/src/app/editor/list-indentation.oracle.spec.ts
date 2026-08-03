import { describe, expect, it } from 'vitest';

import oracle from './__fixtures__/list-indentation-oracle.json';
import {
  applyListIndentUnit,
  detectListIndentUnit,
  indentSourceLines,
  normalizeListIndentForLexical,
  outdentSourceLines,
} from './list-indentation';

/**
 * The tier-1 differential (P4.D40 deliverable 3): every row was produced by
 * v4's REAL `components/chat/lexical/transformers/list-indentation.ts`
 * (`harness/oracle/cases/list-indentation.ts`, whose header carries the regen
 * recipe), never reimplemented. This spec replays the corpus against the v5
 * port and byte-compares. Fix the port, not the fixture.
 *
 * Mutation-proof (D24): temporarily swap `MAX_UNIT`/`Math.max` in
 * `list-indentation.ts` and confirm a corpus row goes red before restoring —
 * recorded in the lane's status-log entry, not re-run here.
 */
describe('list-indentation — v4 real-module differential', () => {
  describe('detectListIndentUnit', () => {
    for (const c of oracle.detect) {
      it(`matches v4: ${c.name}`, () => {
        expect(detectListIndentUnit(c.md), c.md).toBe(c.unit);
      });
    }
  });

  describe('applyListIndentUnit', () => {
    for (const c of oracle.apply) {
      it(`matches v4: ${c.name}`, () => {
        expect(applyListIndentUnit(c.md, c.unit), c.md).toBe(c.out);
      });
    }
  });

  describe('indentSourceLines / outdentSourceLines', () => {
    for (const c of oracle.source) {
      it(`matches v4: ${c.name}`, () => {
        const result =
          c.direction === 'indent'
            ? indentSourceLines({ value: c.value, start: c.start, end: c.end }, c.unit)
            : outdentSourceLines({ value: c.value, start: c.start, end: c.end }, c.unit);
        expect(result.value, c.value).toBe(c.outValue);
        expect(result.cursor, c.value).toBe(c.outCursor);
      });
    }
  });

  describe('normalizeListIndentForLexical', () => {
    for (const c of oracle.normalize) {
      it(`matches v4: ${c.name}`, () => {
        expect(normalizeListIndentForLexical(c.md), c.md).toBe(c.out);
      });
    }
  });

  it('records the whole v4 corpus (no silent vector loss)', () => {
    expect(oracle.detect.length).toBe(8);
    expect(oracle.apply.length).toBe(8);
    expect(oracle.source.length).toBe(7);
    expect(oracle.normalize.length).toBe(14);
    // The four rows recorded FROM real documents are the reason this family
    // exists; a corpus regenerated without them would still pass every
    // assertion above.
    expect(oracle.normalize.filter((c) => c.name.startsWith('DOGFOOD')).length).toBe(4);
  });
});
