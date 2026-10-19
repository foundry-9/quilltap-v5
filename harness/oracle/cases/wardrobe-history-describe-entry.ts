/**
 * Tier-1 oracle case (P4.D315 Tier 1 item 14): v4's REAL
 * `lib/wardrobe/history/describe-entry.ts` at the `e8e17b2d9` pin —
 * `resolveHistorySlots`, `staleItemsOf`, `summarizeHistorySlots`,
 * `changedByLabel` — over pools built by v4's REAL `buildWearablePool`
 * (`lib/wardrobe/pool.ts`) from the corpus's LAYERS, so no stub pool on either
 * side (the Rust twin feeds the same layers to `build_wearable_pool`).
 *
 * Corpus `harness/oracle/fixtures/wardrobe-history-describe-entry.json`:
 * `slotCases` (duplicates, unknown ids, archived, `archivedAt: ''` / `true`,
 * all-empty, top + bottom empty, hair-only, `**` in a title, leading /
 * trailing spaces, U+FEFF / U+0085 / U+00A0 / U+3000 edges, a composite id, a
 * group-archived item, stale dedup across slots) → one line each with the
 * resolved slots (JSON, key order), the stale items and the summary; and
 * `labelCases` (every source × actor absent / self / other / `''` × actorName
 * null / a name / `''` × toolName absent / `''` / a name × selfLabel `you` /
 * `the character` / a name) → one line each with the label.
 *
 * No DB. Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   $N/npx tsx $V5W/harness/oracle/cases/wardrobe-history-describe-entry.ts \
 *     > /tmp/oracle-wh-describe.ndjson
 */

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'wardrobe-history-describe-entry.json'), 'utf8'),
  );
  const { buildWearablePool } = await import('@/lib/wardrobe/pool');
  const { resolveHistorySlots, staleItemsOf, summarizeHistorySlots, changedByLabel } = await import(
    '@/lib/wardrobe/history/describe-entry'
  );
  const noMounts = { groupMountPointIds: [], projectMountPointIds: [] };
  const pools: Record<string, unknown> = {};
  for (const [name, layers] of Object.entries(spec.pools)) {
    pools[name] = buildWearablePool(spec.characterId, noMounts as never, layers as never);
  }
  const out: string[] = [];
  for (const c of spec.slotCases) {
    const resolved = resolveHistorySlots(pools[c.pool] as never, c.slots);
    out.push(
      JSON.stringify({
        kind: 'slots',
        name: c.name,
        resolved,
        stale: staleItemsOf(resolved),
        summary: summarizeHistorySlots(resolved),
      }),
    );
  }
  spec.labelCases.forEach((c: { entry: never; actorName: string | null; selfLabel: string }, i: number) => {
    out.push(JSON.stringify({ kind: 'label', index: i, label: changedByLabel(c.entry, c.actorName, c.selfLabel) }));
  });
  process.stdout.write(out.join('\n') + '\n');
}

main().catch((err) => {
  process.stderr.write(`wardrobe-history-describe-entry oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
