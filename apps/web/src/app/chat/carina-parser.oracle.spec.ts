import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/carina-parser.ndjson';
import {
  CARINA_LINE_RE,
  isCarinaInvocableName,
  parseCarinaQuery,
  type CarinaQuery,
} from './carina-parser';

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

/** `isCarinaInvocableName` rows (P4.D224, v4 `3376b3dfa`) carry `name`, not `content`. */
interface NameRow {
  id: string;
  name: string;
  out: boolean;
  /** `parseCarinaQuery('@' + name + ': hello')?.characterName ?? null` on v4's side. */
  parsedName: string | null;
}

const ALL: Array<Row | NameRow> = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row | NameRow);

const ROWS: Row[] = ALL.filter((r): r is Row => 'content' in r);
const NAME_ROWS: NameRow[] = ALL.filter((r): r is NameRow => 'name' in r);

describe("parseCarinaQuery agrees with v4's lib/chat/carina-parser.ts row for row", () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make the it.each below vacuously green.
    expect(ALL).toHaveLength(83);
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

describe("isCarinaInvocableName agrees with v4's lib/chat/carina-parser.ts row for row", () => {
  it('carries the whole recorded name corpus', () => {
    expect(NAME_ROWS).toHaveLength(34);
    // Both verdicts must be well populated, or a constant answer would pass.
    expect(NAME_ROWS.filter((r) => r.out).length).toBeGreaterThanOrEqual(10);
    expect(NAME_ROWS.filter((r) => !r.out).length).toBeGreaterThanOrEqual(15);
  });

  it.each(NAME_ROWS)('$id', (row) => {
    expect(isCarinaInvocableName(row.name)).toBe(row.out);
    expect(parseCarinaQuery(`@${row.name}: hello`)?.characterName ?? null).toBe(row.parsedName);
  });

  it('rebuilding LINE_RE from NAME_SOURCE left the pattern byte-identical (v4 `3376b3dfa`)', () => {
    // The literal the SPA twin carried before the rebuild, from `f4ad2c8d1`.
    const before = /^@([\w][\w ]*\w)([?:])\s*(.*)$/;
    expect(CARINA_LINE_RE.source).toBe(before.source);
    expect(CARINA_LINE_RE.flags).toBe(before.flags);
  });
});
