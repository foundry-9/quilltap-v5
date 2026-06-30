/**
 * Tier-2 oracle case — the `memories` repo (Phase-2, main DB).
 *
 * Proves what state v4 leaves the `memories` table in after a fixed
 * create / update / mutate / delete sequence, so the Rust port can be diffed
 * against it (structural DB diff). Exercises the full write/mutation surface:
 * `create` (rich + minimal, embedding BLOB + null), `update`,
 * `updateForCharacter` (owned + not-owned no-op), `updateAccessTime{,Bulk}`,
 * `replaceInMemories` (literal substring replace, case-sensitive),
 * `deleteForCharacter` (not-owned no-op), `bulkDelete`, `delete`,
 * `deleteByChatId`, `deleteBySourceMessageId{,s}`.
 *
 * Flow (mirrors conversation-chunks-tier2.ts):
 *   1. copy the SEED (empty-table) fixture to a fresh working copy;
 *   2. open it through v4's real `initializeDatabase()` and run the op sequence
 *      from `memories-tier2.json` via the real `MemoriesRepository`;
 *   3. close, then dump `memories` canonically (RAW on-disk cells via `rawQuery`:
 *      the embedding cell is a raw Buffer → canonValue lowercase hex / null) and
 *      emit it as one NDJSON row.
 *
 * NORMALIZATION SPEC: minted-timestamp placeholder. ids + createdAt + every
 * payload column are pinned/deterministic; only `updatedAt` (bumped by every
 * mutator) and `lastAccessedAt` (set by updateAccessTime{,Bulk}) are minted and
 * get collapsed to `<ts>` by the Rust harness on BOTH dumps. This case emits the
 * raw (un-collapsed) dump.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MEM=/tmp/qt-mem-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/memories-tier2.ts > /tmp/oracle-mem.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind: string;
  id?: string;
  memoryId?: string;
  characterId?: string;
  chatId?: string;
  sourceMessageId?: string;
  ids?: string[];
  search?: string;
  replace?: string;
  data?: Record<string, unknown>;
  options?: { id: string; createdAt: string; updatedAt: string };
}
interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'memories-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_MEM;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_MEM must point at the seed fixture from build-memories-fixture.ts');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mem-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'mem-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { MemoriesRepository } = await import(
    '@/lib/database/repositories/memories.repository'
  );

  await initializeDatabase();
  const repo = new MemoriesRepository();

  for (const op of spec.ops) {
    switch (op.kind) {
      case 'create':
        await repo.create(op.data as never, op.options);
        break;
      case 'update':
        await repo.update(op.id as string, op.data as never);
        break;
      case 'updateForCharacter':
        await repo.updateForCharacter(op.characterId as string, op.memoryId as string, op.data as never);
        break;
      case 'updateAccessTime':
        await repo.updateAccessTime(op.characterId as string, op.memoryId as string);
        break;
      case 'updateAccessTimeBulk':
        await repo.updateAccessTimeBulk(op.characterId as string, op.ids as string[]);
        break;
      case 'replaceInMemories':
        await repo.replaceInMemories(op.ids as string[], op.search as string, op.replace as string);
        break;
      case 'delete':
        await repo.delete(op.id as string);
        break;
      case 'deleteForCharacter':
        await repo.deleteForCharacter(op.characterId as string, op.memoryId as string);
        break;
      case 'bulkDelete':
        await repo.bulkDelete(op.characterId as string, op.ids as string[]);
        break;
      case 'deleteByChatId':
        await repo.deleteByChatId(op.chatId as string);
        break;
      case 'deleteBySourceMessageId':
        await repo.deleteBySourceMessageId(op.sourceMessageId as string);
        break;
      case 'deleteBySourceMessageIds':
        await repo.deleteBySourceMessageIds(op.ids as string[]);
        break;
      default:
        throw new Error(`unknown op kind: ${op.kind}`);
    }
  }

  const columns = (
    (await rawQuery('PRAGMA table_info(memories)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM memories')) as Array<Record<string, unknown>>;

  await closeDatabase();

  const dump = canonicalizeRows({ table: 'memories', columns, rawRows, orderBy: 'id' });
  process.stdout.write(JSON.stringify({ case: 'memories-tier2', ...dump }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memories-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
