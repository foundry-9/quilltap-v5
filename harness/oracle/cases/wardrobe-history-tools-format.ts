/**
 * Tier-1 oracle case (P4.D316 Tier 1 item 11): v4's REAL
 * `filterStaleItems` (`lib/wardrobe/history/rollback.ts`), `windowHistory` +
 * `formatWardrobeHistoryResults` (`lib/tools/handlers/wardrobe-history-handler.ts`)
 * and `formatWardrobeRollbackResults` (`lib/tools/handlers/wardrobe-rollback-handler.ts`)
 * at the `e8e17b2d9` pin, over pools built by v4's REAL `buildWearablePool`
 * from the corpus's LAYERS (the Rust twin feeds the same layers to
 * `build_wearable_pool`).
 *
 * Corpus `harness/oracle/fixtures/wardrobe-history-tools-format.json`. One
 * NDJSON line per case: `filter` (the kept slots + the skipped items),
 * `window` (the shown entry indexes + the gap flag), `rollbackFormat` /
 * `historyFormat` (the formatted text).
 *
 * No DB. Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   $N/npx tsx $V5W/harness/oracle/cases/wardrobe-history-tools-format.ts \
 *     > /tmp/oracle-wh-tools-format.ndjson
 */

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'wardrobe-history-tools-format.json'), 'utf8'),
  );
  const { buildWearablePool } = await import('@/lib/wardrobe/pool');
  const { filterStaleItems } = await import('@/lib/wardrobe/history/rollback');
  const { windowHistory, formatWardrobeHistoryResults } = await import(
    '@/lib/tools/handlers/wardrobe-history-handler'
  );
  const { formatWardrobeRollbackResults } = await import('@/lib/tools/handlers/wardrobe-rollback-handler');
  const { makeEmptyEquippedSlots } = await import('@/lib/schemas/wardrobe.types');
  const noMounts = { groupMountPointIds: [], projectMountPointIds: [] };
  const pool = buildWearablePool(spec.characterId, noMounts as never, spec.pools.main as never);
  const slotsOf = (s: Record<string, string[]>) => ({ ...makeEmptyEquippedSlots(), ...s });
  const out: string[] = [];
  for (const c of spec.filterCases) {
    const r = filterStaleItems(pool as never, slotsOf(c.target) as never, slotsOf(c.live) as never);
    out.push(JSON.stringify({ kind: 'filter', name: c.name, slots: r.slots, skipped: r.skipped }));
  }
  for (const c of spec.windowCases) {
    const rows = (c.rows as Array<[number, boolean]>).map(([entryIndex, isCurrent]) => ({ entryIndex, isCurrent }));
    const r = windowHistory(rows as never, c.limit);
    out.push(
      JSON.stringify({
        kind: 'window',
        name: c.name,
        shown: r.rows.map((row: { entryIndex: number }) => row.entryIndex),
        gapBeforeWindow: r.gapBeforeWindow,
      }),
    );
  }
  for (const c of spec.rollbackFormatCases) {
    out.push(JSON.stringify({ kind: 'rollbackFormat', name: c.name, text: formatWardrobeRollbackResults(c.output) }));
  }
  for (const c of spec.historyFormatCases) {
    out.push(JSON.stringify({ kind: 'historyFormat', name: c.name, text: formatWardrobeHistoryResults(c.output) }));
  }
  process.stdout.write(out.join('\n') + '\n');
}

main().catch((err) => {
  process.stderr.write(`wardrobe-history-tools-format oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
