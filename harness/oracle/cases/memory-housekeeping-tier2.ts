/**
 * Tier-2 oracle case — the housekeeping sweep (v4 `lib/memory/housekeeping.ts`
 * `runHousekeeping` / `needsHousekeeping`).
 *
 * No model call anywhere: the retention pass is pure scoring, the merge pass
 * searches the ALREADY-STORED vector index against itself, and the apply goes
 * through the deletion chokepoint — so this runs directly under the tsx real-DB
 * path (no jest, no mock). We copy the pre-seeded fixture, run the fixed op
 * sequence via v4's REAL housekeeping, emit each op's full RESULT (one `results`
 * NDJSON line), then dump `memories` + `vector_indices` + `vector_entries`
 * canonically for the Rust diff.
 *
 * NORMALIZATION SPEC (applied by the Rust test to BOTH sides; this case emits
 * raw): sentinel-aware minted-timestamp placeholder on `memories.updatedAt`
 * (chokepoint neighbour scrub mints) and `vector_indices.updatedAt` (a flush
 * that removed entries mints); and in the RESULT comparison, the age/inactive
 * month numbers inside detail reasons are placeholdered (`<m> months`) because
 * they derive from each side's own wall clock — everything else in the results
 * (counts, ids, order, actions, percent/similarity numbers) is compared
 * byte-exact.
 *
 * ⏳ CORPUS FRESHNESS: see build-memory-housekeeping-fixture.ts — refresh the
 * spec's recent dates when regenerating after ~2026-12.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   PATH=$N:$PATH QT_FIXTURE_MEMHOUSEKEEPING=/tmp/qt-mem-housekeeping-fixture.db \
 *     npx tsx ~/source/quilltap-v5/harness/oracle/cases/memory-housekeeping-tier2.ts > /tmp/oracle-mem-housekeeping.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind: 'runHousekeeping' | 'needsHousekeeping';
  characterId: string;
  options: Record<string, unknown>;
}
interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'memory-housekeeping-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_MEMHOUSEKEEPING;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_MEMHOUSEKEEPING must point at the seed fixture from build-memory-housekeeping-fixture.ts',
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mem-housekeeping-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'mem-housekeeping-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { runHousekeeping, needsHousekeeping } = await import('@/lib/memory/housekeeping');

  await initializeDatabase();

  const results: unknown[] = [];
  for (const op of spec.ops) {
    switch (op.kind) {
      case 'runHousekeeping':
        results.push(await runHousekeeping(op.characterId, op.options));
        break;
      case 'needsHousekeeping':
        results.push(await needsHousekeeping(op.characterId, op.options));
        break;
      default:
        throw new Error(`unknown op kind: ${(op as Op).kind}`);
    }
  }

  const tables: Array<{ table: string; orderBy: string }> = [
    { table: 'memories', orderBy: 'id' },
    { table: 'vector_indices', orderBy: 'id' },
    { table: 'vector_entries', orderBy: 'id' },
  ];
  const lines: string[] = [];
  for (const t of tables) {
    const columns = (
      (await rawQuery(`PRAGMA table_info(${t.table})`)) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = (await rawQuery(`SELECT * FROM ${t.table}`)) as Array<
      Record<string, unknown>
    >;
    const dump = canonicalizeRows({ table: t.table, columns, rawRows, orderBy: t.orderBy });
    lines.push(JSON.stringify({ case: 'memory-housekeeping-tier2', ...dump }));
  }

  await closeDatabase();

  lines.push(JSON.stringify({ case: 'memory-housekeeping-tier2', results }));
  process.stdout.write(lines.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-housekeeping-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
