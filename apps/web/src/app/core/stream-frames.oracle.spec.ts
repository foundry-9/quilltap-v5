import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';

import corpusText from '../../testing/fixtures/stream-frames.oracle.ndjson';
import type { DoneOutcome } from './core-contract';

/**
 * The corpus differential for v4 `lib/chat/stream-frames.ts`'s `parseFrame` /
 * `doneLeavesNoReply` (P4.D384 item 3, v4 `604a082c4`, pinned at `624d0d25c`).
 * Recorded by `apps/web/oracle/stream-frames.recorder.ts` against v4's REAL
 * module (its header carries the pinned-worktree recipe).
 *
 * Three things are compared per row:
 *  - `JSON.stringify(out)` BYTE-for-byte — the key order is the point (Zod
 *    re-emits every schema level in SCHEMA order, and strips unknown keys);
 *  - the WARN calls the row caused, in order — the once-per-kind Set is
 *    module-global, so the rows are replayed IN FILE ORDER through ONE fresh
 *    module instance;
 *  - on a refused row, the first three issue texts v4's schema raised
 *    (`frameIssueTexts`), so the issue generator is pinned on every class and
 *    not only on each kind's first WARN.
 */

interface ParseRow {
  section: 'null' | 'valid' | 'drop' | 'golden';
  id: string;
  raw: string;
  out: unknown;
  warns: Array<{ msg: string; meta: unknown }>;
  issues?: string[];
}
interface OutcomeRow {
  section: 'doneLeavesNoReply';
  id: string;
  outcome: DoneOutcome;
  out: boolean;
}
type Row = ParseRow | OutcomeRow;

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);
const PARSE_ROWS = ROWS.filter((r): r is ParseRow => r.section !== 'doneLeavesNoReply');
const OUTCOME_ROWS = ROWS.filter((r): r is OutcomeRow => r.section === 'doneLeavesNoReply');

/** The counts `src/testing/fixtures/README.md` records — a short file must fail. */
const EXPECTED = { null: 11, valid: 66, drop: 46, golden: 51, doneLeavesNoReply: 5 } as const;

type Module = typeof import('./stream-frames');

describe("parseFrame agrees with v4's lib/chat/stream-frames.ts row for row", () => {
  it('carries the whole recorded corpus, section by section', () => {
    const counts: Record<string, number> = {};
    for (const r of ROWS) counts[r.section] = (counts[r.section] ?? 0) + 1;
    expect(counts).toEqual(EXPECTED);
    expect(ROWS).toHaveLength(179);
  });

  describe('replayed in order through one fresh module', () => {
    let mod: Module;
    const results: Array<{ out: string; warns: unknown[] }> = [];

    beforeAll(async () => {
      vi.resetModules();
      mod = await import('./stream-frames');
      const spy = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
      try {
        for (const row of PARSE_ROWS) {
          spy.mockClear();
          const out = mod.parseFrame(row.raw);
          results.push({
            out: JSON.stringify(out),
            warns: spy.mock.calls.map(([msg, meta]) => ({ msg, meta })),
          });
        }
      } finally {
        spy.mockRestore();
      }
    });

    it.each(PARSE_ROWS.map((r, i) => [r.section, r.id, i] as const))('%s %s', (_s, _id, i) => {
      const row = PARSE_ROWS[i];
      expect(results[i].out).toBe(JSON.stringify(row.out));
      expect(results[i].warns).toEqual(row.warns);
    });

    it.each(PARSE_ROWS.filter((r) => r.issues).map((r) => [r.id, r] as const))(
      'issue texts: %s',
      (_id, row) => {
        expect(mod.frameIssueTexts(JSON.parse(row.raw.trim()))).toEqual(row.issues);
      },
    );
  });

  it.each(OUTCOME_ROWS.map((r) => [r.outcome, r.out] as const))('doneLeavesNoReply(%s) → %s', async (outcome, expected) => {
    const { doneLeavesNoReply } = await import('./stream-frames');
    expect(doneLeavesNoReply(outcome)).toBe(expected);
  });
});

describe('stream-frames — beyond the corpus', () => {
  afterEach(() => vi.restoreAllMocks());

  it('passes the z.custom / z.unknown members through BY REFERENCE', async () => {
    const { parseFrameValue } = await import('./stream-frames');
    const round = { id: 'r', bids: [] };
    const result = { text: 'x' };
    const message = { id: 'm', deep: { a: 1 } };
    expect((parseFrameValue({ kind: 'storytellerRound', round }) as { round: unknown }).round).toBe(round);
    expect((parseFrameValue({ kind: 'toolResult', name: 'n', success: true, result }) as { result: unknown }).result).toBe(result);
    expect((parseFrameValue({ kind: 'posted', what: 'hostAnnouncement', message }) as { message: unknown }).message).toBe(message);
  });

  it('assertNever names the value', async () => {
    const { assertNever } = await import('./stream-frames');
    expect(() => assertNever({ kind: 'x' } as never)).toThrow('Unhandled stream frame variant: {"kind":"x"}');
  });

  it('parseFrameValue WARNs a refused object exactly as parseFrame does', async () => {
    vi.resetModules();
    const { parseFrameValue } = await import('./stream-frames');
    const spy = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    expect(parseFrameValue({ kind: 'handAnswered', participantId: 'p', characterName: 'Bo' })).toBeNull();
    expect(parseFrameValue({ kind: 'handAnswered' })).toBeNull();
    expect(spy).toHaveBeenCalledTimes(1);
    expect(spy).toHaveBeenCalledWith('[StreamFrames] Ignoring an unrecognised stream frame', {
      kind: 'handAnswered',
      issues: ['yielded: Invalid input: expected boolean, received undefined'],
    });
  });
});
