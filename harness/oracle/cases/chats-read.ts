/**
 * Read-differential oracle — v4's `ChatsRepository` findBy* queries (Phase-2, the
 * chats repo — sub-unit 2, read).
 *
 * Opens a COPY of the pre-baked fixture and drives v4's REAL repository read
 * methods for each spec query, emitting each result verbatim. The Rust port
 * (db::chats_read) reads the same fixture and must produce the same results —
 * exactly (no normalization; nothing is mutated).
 *
 * Result shapes: a ChatMetadata or null (findById), a ChatMetadata[] (every other
 * query). The hydrated chat's `participants` is the per-element Zod-parsed array
 * (defaults materialized, nullable-optionals dropped); JSON-object columns are the
 * parsed objects; numbers render the JS way.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHATSREAD=/tmp/qt-chatsread.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-read.ts > /tmp/oracle-chatsread.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Query {
  kind: string;
  id?: string;
  userId?: string;
  characterId?: string;
  chatType?: 'salon' | 'help' | 'brahma';
  limit?: number;
  excludeChatId?: string;
}
interface Spec {
  testPepperBase64: string;
  queries: Query[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'chats-read-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_CHATSREAD;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_CHATSREAD must point at the fixture from build-chats-read-fixture.ts');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chatsread-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'chatsread-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');

  await initializeDatabase();
  const repo = new ChatsRepository();

  const results: Array<{ kind: string; result: unknown }> = [];
  for (const q of spec.queries) {
    let result: unknown;
    switch (q.kind) {
      case 'findById':
        result = await repo.findById(q.id as string);
        break;
      case 'findAll':
        result = await repo.findAll();
        break;
      case 'findByUserId':
        result = await repo.findByUserId(q.userId as string);
        break;
      case 'findByCharacterId':
        result = await repo.findByCharacterId(q.characterId as string);
        break;
      case 'findByType':
        result = await repo.findByType(q.userId as string, q.chatType as 'salon' | 'help' | 'brahma');
        break;
      case 'findRecentSummarizedByCharacter':
        result = await repo.findRecentSummarizedByCharacter(q.characterId as string, {
          limit: q.limit as number,
          excludeChatId: q.excludeChatId,
        });
        break;
      default:
        throw new Error(`unknown query kind: ${q.kind}`);
    }
    results.push({ kind: q.kind, result });
  }

  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'chats-read', queries: results }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats-read oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
