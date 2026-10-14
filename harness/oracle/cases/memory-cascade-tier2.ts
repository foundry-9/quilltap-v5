/**
 * Tier-2 oracle case — the memory-service cascade-delete family (v4
 * `lib/memory/memory-service.ts` `deleteMemoryWithVector` /
 * `deleteMemoriesBySourceMessageWithVectors` /
 * `deleteMemoriesBySourceMessagesWithVectors` /
 * `deleteMemoriesByChatIdWithVectors`).
 *
 * P4.D288 (C1-7, the `83334b9a5` pin): v4 `af48f6cc2` DELETED the four
 * `…WithVectors` functions — every deletion is `retireMemories`. The committed
 * op list (§R.7) is RE-AIMED onto each one's successor at the pin, the same way
 * on both sides: `deleteMemoryWithVector` → the item route's shape (the row's
 * owner checked, then `retireMemories([id], 'item-delete')`, `ok = retired >
 * 0`); `deleteMemoriesBySourceMessage(s)` → the messages route's
 * (`findBySourceMessageId` per message, then ONE `retireMemories(…,
 * 'message-cascade')`); `deleteMemoriesByChatId` → `retireChatMemories`
 * (`characterCount` is the report's `charactersAffected`). v4's store is
 * LOADED for every character first (`memoryIndex.shape`), so `vectorsRemoved`
 * counts the loaded index (L-2, `MEMORY_INDEX_ALWAYS_LOADED`). Each return is
 * still asserted against the spec's `expect`, then emitted; only `memories`
 * is dumped (the mirror tables are gone).
 *
 * NORMALIZATION SPEC: sentinel-aware minted-timestamp placeholder on
 * `memories.updatedAt` (the neighbour scrub mints). Everything else is pinned
 * by the fixture builder. This case emits the raw (un-collapsed) dump.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MEMCASCADE=/tmp/qt-mem-cascade-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/memory-cascade-tier2.ts > /tmp/oracle-mem-cascade.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, rmSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
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
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'mem-cascade-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { MemoriesRepository } = await import('@/lib/database/repositories/memories.repository');
  const { retireMemories, retireChatMemories } = await import('@/lib/memory/retire');
  const { memoryIndex } = await import('@/lib/memory/index-store');
  (globalThis as Record<string, unknown>).__quilltapJobHost = { shuttingDown: true };

  await initializeDatabase();
  const repo = new MemoriesRepository();
  // L-2: load every holder's index before the first retirement.
  const holders = (await rawQuery('SELECT DISTINCT characterId FROM memories')) as Array<{ characterId: string }>;
  for (const h of holders) await memoryIndex.shape(h.characterId, 3);

  const bySourceMessages = async (ids: string[]) => {
    const memoryIds: string[] = [];
    for (const id of ids) memoryIds.push(...(await repo.findBySourceMessageId(id)).map((m) => m.id));
    const r = await retireMemories(memoryIds, { reason: 'message-cascade' });
    return { deleted: r.retired, vectorsRemoved: r.vectorsRemoved };
  };

  const returns: unknown[] = [];
  for (const [i, op] of spec.ops.entries()) {
    const label = `op[${i}] ${op.kind}`;
    switch (op.kind) {
      case 'deleteMemoryWithVector': {
        const existing = await repo.findById(op.memoryId as string);
        let ok = false;
        if (existing && existing.characterId === op.characterId) {
          ok = (await retireMemories([op.memoryId as string], { reason: 'item-delete' })).retired > 0;
        }
        assertDeepEqual({ ok }, op.expect, label);
        returns.push({ ok });
        break;
      }
      case 'deleteMemoriesBySourceMessage': {
        const r = await bySourceMessages([op.sourceMessageId as string]);
        assertDeepEqual(r, op.expect, label);
        returns.push(r);
        break;
      }
      case 'deleteMemoriesBySourceMessages': {
        const r = await bySourceMessages(op.sourceMessageIds as string[]);
        assertDeepEqual(r, op.expect, label);
        returns.push(r);
        break;
      }
      case 'deleteMemoriesByChatId': {
        const report = await retireChatMemories(op.chatId as string);
        const r = {
          deleted: report.retired,
          vectorsRemoved: report.vectorsRemoved,
          characterCount: report.charactersAffected,
        };
        assertDeepEqual(r, op.expect, label);
        returns.push(r);
        break;
      }
      default:
        throw new Error(`unknown op kind: ${(op as Op).kind}`);
    }
  }

  const tables: Array<{ table: string; orderBy: string }> = [{ table: 'memories', orderBy: 'id' }];
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
