/**
 * Tier-2 oracle case — the memory-service cascade-delete family (v4
 * `lib/memory/memory-service.ts` `deleteMemoryWithVector` /
 * `deleteMemoriesBySourceMessageWithVectors` /
 * `deleteMemoriesBySourceMessagesWithVectors` /
 * `deleteMemoriesByChatIdWithVectors`).
 *
 * Like the deletion-chokepoint case these are module functions calling
 * `getRepositories()` + the DB-backed vector store, and the delete path touches
 * no LLM — so they run directly under the tsx real-DB path (no jest, no model
 * mock). We copy the pre-seeded fixture, run the fixed op sequence via v4's REAL
 * memory-service, assert each op's return against the spec's `expect`, then dump
 * `memories` + `vector_indices` + `vector_entries` canonically (one NDJSON line
 * per table, plus a `returns` line) for the Rust diff.
 *
 * NORMALIZATION SPEC: sentinel-aware minted-timestamp placeholder on
 * `memories.updatedAt` (a neighbour scrubbed by the chokepoint is rewritten
 * through `updateForCharacter`, minting) and `vector_indices.updatedAt` (a store
 * flush that actually removed entries runs `saveMeta`, minting). Everything else
 * is pinned by the fixture builder. This case emits the raw (un-collapsed) dumps.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MEMCASCADE=/tmp/qt-mem-cascade-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/memory-cascade-tier2.ts > /tmp/oracle-mem-cascade.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind:
    | 'deleteMemoryWithVector'
    | 'deleteMemoriesBySourceMessage'
    | 'deleteMemoriesBySourceMessages'
    | 'deleteMemoriesByChatId';
  characterId?: string;
  memoryId?: string;
  sourceMessageId?: string;
  sourceMessageIds?: string[];
  chatId?: string;
  expect: Record<string, unknown>;
}
interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

function assertDeepEqual(got: unknown, want: unknown, label: string): void {
  const g = JSON.stringify(got);
  const w = JSON.stringify(want);
  if (g !== w) {
    throw new Error(`${label}: return diverged from expect — got ${g}, want ${w}`);
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'memory-cascade-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_MEMCASCADE;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_MEMCASCADE must point at the seed fixture from build-memory-cascade-fixture.ts',
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mem-cascade-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'mem-cascade-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const {
    deleteMemoryWithVector,
    deleteMemoriesBySourceMessageWithVectors,
    deleteMemoriesBySourceMessagesWithVectors,
    deleteMemoriesByChatIdWithVectors,
  } = await import('@/lib/memory/memory-service');

  await initializeDatabase();

  const returns: unknown[] = [];
  for (const [i, op] of spec.ops.entries()) {
    const label = `op[${i}] ${op.kind}`;
    switch (op.kind) {
      case 'deleteMemoryWithVector': {
        const ok = await deleteMemoryWithVector(op.characterId as string, op.memoryId as string);
        assertDeepEqual({ ok }, op.expect, label);
        returns.push({ ok });
        break;
      }
      case 'deleteMemoriesBySourceMessage': {
        const r = await deleteMemoriesBySourceMessageWithVectors(op.sourceMessageId as string);
        assertDeepEqual(r, op.expect, label);
        returns.push(r);
        break;
      }
      case 'deleteMemoriesBySourceMessages': {
        const r = await deleteMemoriesBySourceMessagesWithVectors(op.sourceMessageIds as string[]);
        assertDeepEqual(r, op.expect, label);
        returns.push(r);
        break;
      }
      case 'deleteMemoriesByChatId': {
        const r = await deleteMemoriesByChatIdWithVectors(op.chatId as string);
        assertDeepEqual(r, op.expect, label);
        returns.push(r);
        break;
      }
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
    lines.push(JSON.stringify({ case: 'memory-cascade-tier2', ...dump }));
  }

  await closeDatabase();

  lines.push(JSON.stringify({ case: 'memory-cascade-tier2', returns }));
  process.stdout.write(lines.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-cascade-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
