import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/intervention-summary.oracle.ndjson';
import type { MessageDto } from '../core/core-contract';
import { describeSpan, type RunSpan } from './intervention-spans';
import { getInterventionSummary } from './system-message-labels';

/**
 * The corpus differential for the hold's two pieces of operator-facing prose
 * (P4.D346 item 4, R-9): v4's `getInterventionSummary` (`app/salon/[id]/
 * components/system-message-labels.ts:210-257`, `839575d89` — the "adjusted"
 * badge's tooltip) and v4's `describeSpan` (`lib/pascal/intervention/
 * spans.ts`, `8ae8a127c` — the dialog's span hints). Recorded by
 * `apps/web/oracle/intervention-summary.recorder.ts` against v4's REAL
 * functions at the pin `236c04e35`, over v4's own twelve test cases and the
 * edges a transcription could mis-spell.
 */

type Row =
  | { fn: 'getInterventionSummary'; id: string; message: Pick<MessageDto, 'systemSender' | 'pascalMeta'>; out: string | null }
  | { fn: 'describeSpan'; id: string; span: RunSpan; out: string };

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

const summaries = ROWS.filter((r): r is Extract<Row, { fn: 'getInterventionSummary' }> => r.fn === 'getInterventionSummary');
const spans = ROWS.filter((r): r is Extract<Row, { fn: 'describeSpan' }> => r.fn === 'describeSpan');

describe('the intervention prose agrees with v4 row for row', () => {
  it('carries the whole recorded corpus', () => {
    expect(ROWS).toHaveLength(38);
    expect(summaries).toHaveLength(24);
    expect(spans).toHaveLength(14);
  });

  describe('getInterventionSummary', () => {
    it.each(summaries)('$id', (row) => {
      expect(getInterventionSummary(row.message)).toBe(row.out);
    });
  });

  describe('describeSpan', () => {
    it.each(spans)('$id', (row) => {
      expect(describeSpan(row.span)).toBe(row.out);
    });
  });
});
