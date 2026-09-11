import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/carina-parser.ndjson';
import { parseCarinaQuery, type CarinaQuery } from './carina-parser';

/**
 * The corpus differential for v4 `lib/chat/carina-parser.ts`'s `parseCarinaQuery`
 * (P4.D181, `f4ad2c8d1`). v4's client imports that module directly; v5's client
 * twin is a transcription, and this is what proves it byte for byte. Recorded by
 * `apps/web/oracle/carina-parser.recorder.ts` against v4's REAL module (see that
 * file's header for the pinned-worktree invocation).
 *
 * The gate (`impersonation-voice/gate.ts`) is the only consumer: a Carina address
 * is machinery and must reach the room verbatim, never rehearsed.
 */

interface Row {
  id: string;
  content: string;
  out: CarinaQuery | null;
}

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

describe("parseCarinaQuery agrees with v4's lib/chat/carina-parser.ts row for row", () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make the it.each below vacuously green.
    expect(ROWS).toHaveLength(49);
  });

  it('the corpus discriminates in BOTH directions', () => {
    // A corpus of all-nulls would pass against a parser that answered null
    // unconditionally; a corpus of all-hits would pass against one that never
    // refused. Both populations must be substantial.
    expect(ROWS.filter((r) => r.out === null).length).toBeGreaterThanOrEqual(20);
    expect(ROWS.filter((r) => r.out !== null).length).toBeGreaterThanOrEqual(20);
  });

  it.each(ROWS)('$id', (row) => {
    expect(parseCarinaQuery(row.content)).toEqual(row.out);
  });
});
