/**
 * Tier-3 SEED fixture builder — the compression cache (v4
 * `lib/services/chat-message/compression-cache.service.ts`).
 *
 * The cache functions receive their messages/systemPrompt/options as arguments;
 * the only DB state they touch is the `chats.compressionCache` column. So the
 * fixture is minimal — three empty chats (main DB only), created via v4's REAL
 * `repos.chats.create` (pinned ids/timestamps, `compressionCache` NULL):
 *   - chatA: trigger → get → invalidate (the full lifecycle),
 *   - chatD: trigger with too-few messages (the guard — no compression, column
 *     stays NULL),
 *   - chatE: get on a chat with no cache (the miss).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-compcache.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-compression-cache-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  chats: Array<{ id: string }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'compression-cache-tier3.json'), 'utf8')
  ) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) throw new Error('QT_FIXTURE_OUT must point at the fixture .db file');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-compcache-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  const repos = getRepositories();

  for (const chat of spec.chats) {
    await repos.chats.create(
      {
        userId: spec.userId,
        title: `Fixture ${chat.id}`,
        participants: [],
        chatType: 'salon',
      } as never,
      { id: chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  await closeDatabase();
  process.stderr.write(`built compression-cache fixture: ${out} (${spec.chats.length} chats)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`compression-cache fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
