/**
 * Tier-2 oracle case — the memory deletion chokepoint (v4
 * `lib/memory/memory-gate.ts` `deleteMemoryWithUnlink` /
 * `deleteMemoriesWithUnlinkBatch`).
 *
 * These are module functions (not repo methods) that call `getRepositories()` +
 * `rawQuery`, so — like the store-backed cases — they run directly under the tsx
 * real-DB path (no jest, no model mock: deletion touches no LLM). We copy the
 * pre-seeded graph fixture, run the fixed delete sequence via v4's REAL chokepoint,
 * then dump `memories` canonically so the Rust port can be diffed against it.
 *
 * NORMALIZATION SPEC: sentinel-aware minted-timestamp placeholder. The seed pins
 * every id + createdAt + updatedAt to the seed sentinel; a neighbour that gets
 * unlinked is rewritten through `updateForCharacter`, which mints a fresh
 * `updatedAt` — so the Rust harness collapses ONLY a non-sentinel `updatedAt` to
 * `<ts>` (a row left at the sentinel proves it was NOT touched). This case emits
 * the raw (un-collapsed) dump.
 *
 * P4.D288 (C1-7, the `83334b9a5` pin): v4 `af48f6cc2` DELETED the gate's two
 * chokepoints — every deletion is `retireMemories` (`lib/memory/retire.ts`).
 * The committed op list (§R.7 unchanged) is RE-AIMED: each op's ids go
 * through `retireMemories(ids, { reason: 'item-delete' })` on both sides (a
 * single delete as a one-id list). The retirement's lines are captured in op
 * order — the neighbour scrub, the digest scan, the base `deleteMany`, the
 * index eviction (v4's store LOADED first for both holders, L-2), and the
 * closing `Retired memories` / `Nothing to retire` — `durationMs` (real
 * wall-clock both sides) normalized by the Rust harness.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MEMDEL=/tmp/qt-mem-delete-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/memory-delete-tier2.ts > /tmp/oracle-mem-delete.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, rmSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind: 'deleteMemoryWithUnlink' | 'deleteMemoriesWithUnlinkBatch';
  id?: string;
  ids?: string[];
}
interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'memory-delete-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_MEMDEL;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_MEMDEL must point at the seed fixture from build-memory-delete-fixture.ts',
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mem-delete-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'mem-delete-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  const CAPTURED = new Set([
    'Scrubbed deleted ids from related links',
    'Nothing to retire',
    'Found digests naming doomed members',
    'Digest retires with its last member',
    'Scrubbed digest links to retired memories',
    'Entities deleted',
    'Evicted retired memories from the index',
    'Retirement rewrote an unusually large neighbour set',
    'Retired memories',
  ]);
  const { logger } = await import('@/lib/logger');
  const lines: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      if (!CAPTURED.has(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      lines.push({ level: data.level, message: data.message, fields });
    },
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { retireMemories } = await import('@/lib/memory/retire');
  const { memoryIndex } = await import('@/lib/memory/index-store');
  (globalThis as Record<string, unknown>).__quilltapJobHost = { shuttingDown: true };

  await initializeDatabase();
  // L-2: load every holder's index before the first retirement (silently —
  // the capture starts with the ops).
  const holders = (await rawQuery('SELECT DISTINCT characterId FROM memories')) as Array<{ characterId: string }>;
  for (const h of holders) await memoryIndex.shape(h.characterId, 3);
  lines.length = 0;

  for (const op of spec.ops) {
    switch (op.kind) {
      case 'deleteMemoryWithUnlink':
        await retireMemories([op.id as string], { reason: 'item-delete' });
        break;
      case 'deleteMemoriesWithUnlinkBatch':
        await retireMemories(op.ids as string[], { reason: 'item-delete' });
        break;
      default:
        throw new Error(`unknown op kind: ${(op as Op).kind}`);
    }
  }

  const columns = (
    (await rawQuery('PRAGMA table_info(memories)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM memories')) as Array<Record<string, unknown>>;

  await closeDatabase();

  const dump = canonicalizeRows({ table: 'memories', columns, rawRows, orderBy: 'id' });
  process.stdout.write(JSON.stringify({ case: 'memory-delete-tier2', ...dump, lines }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-delete-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
