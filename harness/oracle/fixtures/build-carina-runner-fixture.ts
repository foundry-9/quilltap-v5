/**
 * Tier-3 SEED fixture builder for the Carina markup runner
 * (v4 `runCarinaMarkupQuery`, lib/services/carina/markup-runner.ts).
 *
 * Bakes the seed BOTH differential sides start from: one `chats` row (two
 * CHARACTER participants — an LLM answerer and a user-controlled asker persona),
 * created via v4's REAL `repos.chats.create` with the id + timestamps pinned to
 * `seedTimestamp`, and the empty `chat_messages` table materialized (via a
 * `getMessageCount` read) so the Rust port — which does NOT own DDL — can INSERT
 * into the same table. The op sequence is run on a COPY of this seed on each
 * side.
 *
 * The runner posts Carina answers (through the mocked-then-real
 * `postCarinaResponse`) whose `addMessage` metadata side-effect reads this chat,
 * so the chat must exist with its participants.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-carina-runner.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-carina-runner-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ParticipantSeed {
  id: string;
  type: string;
  characterId: string;
  controlledBy?: string;
}
interface ChatSeed {
  id: string;
  userId: string;
  title: string;
  participants: ParticipantSeed[];
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  chat: ChatSeed;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'carina-runner-tier3.json'), 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the seed fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-carina-runner-build-'));
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

  const participants = spec.chat.participants.map((p) => ({
    id: p.id,
    type: p.type,
    characterId: p.characterId,
    controlledBy: p.controlledBy ?? 'llm',
    createdAt: spec.seedTimestamp,
    updatedAt: spec.seedTimestamp,
  }));

  await repo.create(
    {
      userId: spec.chat.userId,
      title: spec.chat.title,
      participants,
    } as never,
    { id: spec.chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
  );

  // Materialize the empty chat_messages table so the Rust port can INSERT.
  await repo.getMessageCount(spec.chat.id);

  await closeDatabase();
  process.stderr.write(`built carina-runner seed fixture: ${out}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`carina-runner fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
