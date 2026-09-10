import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/download-utils.ndjson';
import { withDownloadFlag } from './download-utils';

/**
 * The corpus differential for v4 `lib/download-utils.ts`'s `withDownloadFlag`
 * (P4.D176, `78b381a96`) — the one piece of the gallery's download machinery
 * with real branching worth a byte-for-byte equivalence test. Recorded by
 * `apps/web/oracle/download-utils.recorder.ts` against v4's REAL module (see
 * that file's header for the pinned-worktree invocation).
 */

interface Row {
  id: string;
  out: string;
}

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

describe("withDownloadFlag agrees with v4's lib/download-utils.ts row for row", () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make the it.each below vacuously green.
    expect(ROWS).toHaveLength(14);
  });

  it.each(ROWS)('$id', (row) => {
    expect(withDownloadFlag(row.id)).toBe(row.out);
  });
});
