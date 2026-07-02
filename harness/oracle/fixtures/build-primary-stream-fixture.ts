/**
 * Tier-3 SEED fixture builder — the primary-stream / recovery / provider-failover
 * services (Phase-3 Unit-3 wave 3).
 *
 * Creates the corpus chats via v4's REAL `repos.chats.create` (ids + timestamps
 * pinned to the sentinel, each with one CHARACTER participant) and materializes
 * the empty `chat_messages` table (via a `getMessageCount` read), so both the
 * jest oracle and the Rust port — neither of which owns DDL here — can INSERT the
 * assistant / recovery messages the services write. Main-DB only: these services
 * touch only `chats` + `chat_messages`.
 *
 * The op sequence itself is NOT run here; both sides run it on a fresh copy.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-primary-stream.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-primary-stream-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ParticipantSpec {
  id: string;
  type: string;
  characterId: string;
}
interface ChatSpec {
  id: string;
  userId: string;
  title: string;
  participants: ParticipantSpec[];
}
interface Spec {
  testPepperBase64: string;
  sentinel: string;
  chats: ChatSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'primary-stream-tier3.json'), 'utf8')
  ) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) throw new Error('QT_FIXTURE_OUT must point at the seed fixture .db to write');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-primary-stream-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');

  await initializeDatabase();

  const repo = new ChatsRepository();
  for (const c of spec.chats) {
    const participants = c.participants.map((p) => ({
      id: p.id,
      type: p.type,
      characterId: p.characterId,
      createdAt: spec.sentinel,
      updatedAt: spec.sentinel,
    }));
    await repo.create(
      {
        userId: c.userId,
        title: c.title,
        participants,
      } as never,
      { id: c.id, createdAt: spec.sentinel, updatedAt: spec.sentinel }
    );
  }
  // Force chat_messages into existence (empty) so the port can INSERT into it.
  await repo.getMessageCount(spec.chats[0].id);

  await closeDatabase();
  process.stderr.write(`built primary-stream seed fixture: ${out} (${spec.chats.length} chats)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`primary-stream fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
