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
 * P4.D265 (v4 `3f7320138` F8): the chokepoint's lines are captured too (a
 * transport spliced into the singleton logger) and emitted as `lines: [{level,
 * message, fields}]` in op order — the batch line now carries `countSource`
 * (`repository` on every parent-side call this oracle makes; the child's
 * `resolved-ids` arm is unreachable here and is a v5 unit pin). `durationMs`
 * is real wall-clock on both sides — the Rust harness normalizes it.
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
    '[MemoryGate] deleteMemoryWithUnlink complete',
    '[MemoryGate] deleteMemoryWithUnlink touched an unusually large neighbour set',
    '[MemoryGate] deleteMemoriesWithUnlinkBatch complete',
    '[MemoryGate] deleteMemoriesWithUnlinkBatch touched an unusually large neighbour set',
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
  const { deleteMemoryWithUnlink, deleteMemoriesWithUnlinkBatch } = await import(
    '@/lib/memory/memory-gate'
  );

  await initializeDatabase();

  for (const op of spec.ops) {
    switch (op.kind) {
      case 'deleteMemoryWithUnlink':
        await deleteMemoryWithUnlink(op.id as string);
        break;
      case 'deleteMemoriesWithUnlinkBatch':
        await deleteMemoriesWithUnlinkBatch(op.ids as string[]);
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
