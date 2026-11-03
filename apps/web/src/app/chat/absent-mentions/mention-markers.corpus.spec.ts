import { describe, expect, it } from 'vitest';

import corpusText from '../../../testing/fixtures/mention-markers.oracle.ndjson';
import {
  MENTION_CLOSE,
  MENTION_MID,
  MENTION_OPEN,
  assignMentionsToRuns,
  rehypeMentionMarkers,
  splitMentionMarkers,
  type HastNode,
} from './mention-markers';
import { buildNameMatcher, firstNameMatch, lastNameMatch } from './name-mentions';

/**
 * The tier-1 corpus differential for v4 `lib/chat/mention-markers.ts` +
 * `lib/chat/name-mentions.ts` at the pin `236c04e35` (`9a88f862f` — P4.D359
 * item 8). Recorded by `apps/web/tooling/record-mention-markers.mts` against
 * v4's REAL modules; every row replays through the TS twins and byte-compares
 * the `JSON.stringify`'d result.
 */

interface Row {
  kind: 'matcher' | 'rehype' | 'split' | 'assign';
  input: Record<string, unknown>;
  expected: string;
}

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);
const of = (kind: Row['kind']) => ROWS.filter((r) => r.kind === kind);

function run(row: Row): unknown {
  const i = row.input;
  switch (row.kind) {
    case 'matcher': {
      const m = buildNameMatcher(i['names'] as Array<string | null>);
      const t = i['text'] as string;
      return m
        ? { usable: true, first: firstNameMatch(t, m), last: lastNameMatch(t, m) }
        : { usable: false, first: null, last: null };
    }
    case 'rehype': {
      const tree = structuredClone(i['tree']) as HastNode;
      rehypeMentionMarkers(i['mentions'] as Array<{ text: string }>)(tree);
      return tree;
    }
    case 'split':
      return splitMentionMarkers(i['text'] as string);
    case 'assign':
      return assignMentionsToRuns(
        i['runs'] as Array<string | null>,
        i['mentions'] as Array<{ text: string }>,
      );
  }
}

describe('name-chip markers agree with v4 row for row (236c04e35)', () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make every `it.each` below vacuously green.
    expect(ROWS).toHaveLength(59);
    expect(of('matcher')).toHaveLength(27);
    expect(of('rehype')).toHaveLength(19);
    expect(of('split')).toHaveLength(7);
    expect(of('assign')).toHaveLength(6);
  });

  it('the three private-use markers are U+E000 / U+E001 / U+E002 (ruling R-12)', () => {
    expect([MENTION_OPEN, MENTION_MID, MENTION_CLOSE].map((c) => c.codePointAt(0))).toEqual([
      0xe000, 0xe001, 0xe002,
    ]);
  });

  it.each(ROWS.map((row, n) => [n, row.kind, row] as const))('row %i — %s', (_n, _kind, row) => {
    expect(JSON.stringify(run(row))).toBe(row.expected);
  });
});
