import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/transcript-reconcile.ndjson';
import type { MessageDto } from '../core/core-contract';
import type { SwipeState } from './chat-view-model';
import { isProvisionalMessage, reconcileTranscript } from './transcript-reconcile';

/**
 * The corpus differential for v4 `app/salon/[id]/hooks/transcript-reconcile.ts`
 * (`5029075bb`). The module is pure and React-free, so v5's copy is a
 * transcription and this is what proves it. Recorded by
 * `apps/web/oracle/transcript-reconcile.recorder.ts` against v4's REAL module
 * from a `31436bae4` pin (see that file's header for the invocation).
 *
 * The three properties the module exists to hold are each compared in the only
 * form a differential can see them:
 *
 *   - the read is the authority → `ids`;
 *   - the operator's swipe selection survives → `swipeStates`;
 *   - unchanged rows keep their object identity → `reusedFrom` (an index map
 *     into `previous`) plus `messagesIsPrevious` / `swipeStatesIsPrevious`.
 *
 * `reusedFrom` is what a port cannot fake: rebuilding every row returns the
 * right ids and an all-null map.
 */

interface Row {
  id: string;
  in: {
    rows: MessageDto[];
    previous: MessageDto[];
    previousSwipeStates: Record<string, SwipeState>;
  };
  out: {
    ids: string[];
    reusedFrom: (number | null)[];
    messagesIsPrevious: boolean;
    swipeStatesIsPrevious: boolean;
    swipeStates: Record<string, { current: number; total: number; messageIds: string[] }>;
    provisionalFlags: boolean[];
  };
}

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

describe("reconcileTranscript agrees with v4's transcript-reconcile.ts row for row", () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make the it.each below vacuously green.
    expect(ROWS).toHaveLength(38);
  });

  it('the corpus discriminates on every axis it asserts', () => {
    // Each of these would pass against a port that answered one way always.
    const identical = ROWS.filter((r) => r.out.messagesIsPrevious);
    expect(identical.length).toBeGreaterThanOrEqual(4);
    expect(ROWS.length - identical.length).toBeGreaterThanOrEqual(20);

    const carriedSwipe = ROWS.filter((r) => r.out.swipeStatesIsPrevious);
    expect(carriedSwipe.length).toBeGreaterThanOrEqual(20);
    expect(ROWS.length - carriedSwipe.length).toBeGreaterThanOrEqual(4);

    // Object identity must be exercised in both directions, or `reusedFrom`
    // proves nothing about reuse.
    expect(ROWS.filter((r) => r.out.reusedFrom.some((x) => x !== null)).length).toBeGreaterThanOrEqual(10);
    expect(ROWS.filter((r) => r.out.reusedFrom.some((x) => x === null)).length).toBeGreaterThanOrEqual(10);

    // Provisional bubbles, and rows that have none.
    expect(ROWS.filter((r) => r.out.provisionalFlags.some(Boolean)).length).toBeGreaterThanOrEqual(10);
    expect(ROWS.filter((r) => !r.out.provisionalFlags.some(Boolean)).length).toBeGreaterThanOrEqual(10);

    // Swipe groups.
    expect(ROWS.filter((r) => Object.keys(r.out.swipeStates).length > 0).length).toBeGreaterThanOrEqual(5);
  });

  it.each(ROWS)('$id', (row) => {
    const result = reconcileTranscript(row.in.rows, row.in.previous, row.in.previousSwipeStates);

    expect(result.messages.map((m) => m.id)).toEqual(row.out.ids);

    // Which output objects are the very objects that came in, by reference.
    const reusedFrom = result.messages.map((m) => {
      const index = row.in.previous.indexOf(m);
      return index >= 0 ? index : null;
    });
    expect(reusedFrom).toEqual(row.out.reusedFrom);

    expect(result.messages === row.in.previous).toBe(row.out.messagesIsPrevious);
    expect(result.swipeStates === row.in.previousSwipeStates).toBe(row.out.swipeStatesIsPrevious);

    const swipeStates: Record<string, { current: number; total: number; messageIds: string[] }> = {};
    for (const [groupId, state] of Object.entries(result.swipeStates)) {
      swipeStates[groupId] = {
        current: state.current,
        total: state.total,
        messageIds: state.messages.map((m) => m.id),
      };
    }
    expect(swipeStates).toEqual(row.out.swipeStates);

    expect(row.in.previous.map((m) => isProvisionalMessage(m))).toEqual(row.out.provisionalFlags);
  });
});
