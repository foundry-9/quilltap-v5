import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/carina-parser.ndjson';
import * as carina from './carina-parser';
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

/**
 * `stripCarinaQueryLines` rows (P4.D307, v4 `362426c8f`) carry `kind: 'strip'`;
 * the 83 older rows carry no `kind` at all, which is how they are told apart.
 */
interface StripRow {
  id: string;
  kind: 'strip';
  content: string;
  out: string;
}

const ALL: Array<Row | NameRow | StripRow> = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row | NameRow | StripRow);

const STRIP_ROWS: StripRow[] = ALL.filter((r): r is StripRow => 'kind' in r && r.kind === 'strip');
const ROWS: Row[] = ALL.filter((r): r is Row => !('kind' in r) && 'content' in r);
const NAME_ROWS: NameRow[] = ALL.filter((r): r is NameRow => !('kind' in r) && 'name' in r);

describe("parseCarinaQuery agrees with v4's lib/chat/carina-parser.ts row for row", () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make the it.each below vacuously green.
    expect(ALL).toHaveLength(144);
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

/**
 * The strip a reader of "what was said" applies (v4 `362426c8f`): every line
 * that parses as a query is removed, the rest rejoin with `'\n'`. Read
 * STRUCTURALLY so this describe can run (red) against a twin that predates the
 * export, rather than failing the whole file at import.
 */
const stripCarinaQueryLines = (carina as unknown as { stripCarinaQueryLines?: (text: string) => string })
  .stripCarinaQueryLines;

describe("stripCarinaQueryLines agrees with v4's lib/chat/carina-parser.ts row for row", () => {
  it('carries the whole recorded strip corpus', () => {
    // Twelve hand-written vectors plus the strip of every parse vector.
    expect(STRIP_ROWS).toHaveLength(61);
    expect(STRIP_ROWS.length + ROWS.length + NAME_ROWS.length).toBe(ALL.length);
    // Both outcomes well populated: lines removed, and text left untouched.
    expect(STRIP_ROWS.filter((r) => r.out !== r.content).length).toBeGreaterThanOrEqual(20);
    expect(STRIP_ROWS.filter((r) => r.out === r.content).length).toBeGreaterThanOrEqual(20);
  });

  it('is exported by the twin', () => {
    expect(typeof stripCarinaQueryLines).toBe('function');
  });

  it.each(STRIP_ROWS)('$id', (row) => {
    expect(stripCarinaQueryLines?.(row.content)).toBe(row.out);
  });
});
