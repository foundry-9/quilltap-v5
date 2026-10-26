import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/scenario-sort.oracle.ndjson';
import {
  compareScenarios,
  scenarioSortKey,
  sortScenarios,
  type SortableScenario,
} from './scenario-sort';

/**
 * The corpus differential for v4 `lib/scenarios/sort.ts` (`1d0a31132`, pinned at
 * `322a5488e` — P4.D338). Recorded by `apps/web/oracle/scenario-sort.recorder.ts`
 * against v4's REAL module over the grid the Rust family replays; see that
 * file's header for the pinned-worktree invocation. The comparator calls the
 * host's default-locale `localeCompare` (v4's own semantics), so the corpus is
 * recorded — and this spec runs — under an en-US host.
 */

type Row =
  | { kind: 'key'; value: string; output: string }
  | { kind: 'compare'; name: string; a: SortableScenario; b: SortableScenario; output: number }
  | { kind: 'sort'; name: string; items: SortableScenario[]; output: number[] };

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

describe('scenario sort — the v4 corpus (apps/web/oracle/scenario-sort.recorder.ts)', () => {
  it('carries the whole recorded grid (a truncated corpus must not pass)', () => {
    const count = (k: Row['kind']) => ROWS.filter((r) => r.kind === k).length;
    expect(count('key')).toBeGreaterThanOrEqual(30);
    expect(count('compare')).toBeGreaterThanOrEqual(24);
    expect(count('sort')).toBeGreaterThanOrEqual(7);
  });

  it('every recorded row replays through the twin', () => {
    const failures: string[] = [];
    for (const row of ROWS) {
      if (row.kind === 'key') {
        const got = scenarioSortKey(row.value);
        if (got !== row.output) failures.push(`key ${JSON.stringify(row.value)}: ${JSON.stringify(row.output)} vs ${JSON.stringify(got)}`);
      } else if (row.kind === 'compare') {
        const got = Math.sign(compareScenarios(row.a, row.b));
        if (got !== row.output) failures.push(`compare ${row.name}: ${row.output} vs ${got}`);
      } else {
        const tagged = row.items.map((item, index) => ({ item, index }));
        const got = sortScenarios(tagged, (t) => t.item).map((t) => t.index);
        if (JSON.stringify(got) !== JSON.stringify(row.output)) {
          failures.push(`sort ${row.name}: ${JSON.stringify(row.output)} vs ${JSON.stringify(got)}`);
        }
      }
    }
    expect(failures).toEqual([]);
  });
});
