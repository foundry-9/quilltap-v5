import { describe, expect, it } from 'vitest';

import corpusText from '../../../../testing/fixtures/memory-maintenance-text.ndjson';
import { MAINTENANCE_PHASES, MEMORY_CAP_BOUNDS } from '../../../core/core-contract';
import type { MaintenanceSummary } from '../../../core/core-contract';
import { PHASE_LABELS } from '../../../memory/maintenance-report';
import { IMPORTANCE_HIGH, IMPORTANCE_LOW, importanceBand } from '../../../memory/memory-format';
import { summaryLine } from './memory-maintenance-ledger';
import { MEMORY_SETTING_DEFAULTS } from './memory-setting-defaults';

/**
 * P4.D300 Tier 2 — the corpus differential for the memory SPA's pure text
 * against v4 at the pin `83334b9a5`: `MEMORY_SETTING_DEFAULTS` (each schema's
 * REAL `.parse({})`), `MEMORY_CAP_BOUNDS`, the importance band (`e0ac7493a`),
 * `MAINTENANCE_PHASES`, the report's `PHASE_LABELS`, and the ledger's
 * `summaryLine` over every arm. Recorded by
 * `apps/web/oracle/memory-maintenance-text.recorder.ts` against v4's REAL
 * modules (the two `.tsx` components load under `tsx`) — see that file's header
 * for the pinned-worktree invocation. Objects compare through `JSON.stringify`:
 * key order is part of the recorded value.
 */

interface Row {
  fn: string;
  id: string;
  args: unknown[];
  out: unknown;
}

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

const SPA: Record<string, (row: Row) => unknown> = {
  MEMORY_SETTING_DEFAULTS: (row) => MEMORY_SETTING_DEFAULTS[row.id as keyof typeof MEMORY_SETTING_DEFAULTS],
  MEMORY_CAP_BOUNDS: () => MEMORY_CAP_BOUNDS,
  IMPORTANCE: () => ({ high: IMPORTANCE_HIGH, low: IMPORTANCE_LOW }),
  MAINTENANCE_PHASES: () => MAINTENANCE_PHASES,
  PHASE_LABELS: () => PHASE_LABELS,
  importanceBand: (row) => importanceBand(row.args[0] as number),
  summaryLine: (row) => summaryLine(row.args[0] as MaintenanceSummary),
};

describe("the memory SPA's pure text agrees with v4 83334b9a5 row for row", () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make the it.each below vacuously green.
    expect(ROWS).toHaveLength(29);
    expect(ROWS.filter((r) => r.fn === 'importanceBand')).toHaveLength(10);
    expect(ROWS.filter((r) => r.fn === 'summaryLine')).toHaveLength(12);
    expect(ROWS.every((r) => r.fn in SPA)).toBe(true);
  });

  it.each(ROWS)('$fn $id', (row) => {
    expect(JSON.stringify(SPA[row.fn](row))).toBe(JSON.stringify(row.out));
  });
});
