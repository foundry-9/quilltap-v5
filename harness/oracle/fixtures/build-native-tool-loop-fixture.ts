/**
 * Tier-3 SEED fixture builder — the native tool loop (Phase-3 Unit-3 wave 4,
 * W4.1e; v4 `runNativeToolLoop`).
 *
 * The loop's ONLY DB write is the agent-mode `repos.chats.update(chatId, {
 * agentTurnCount })` bump, so the seed is a MAIN-db-only set of `chats` rows (no
 * characters / no mount-index — the loop reads nothing from either; detection +
 * tool results + the re-stream are all canned in the differential). Each chat has
 * one CHARACTER participant (v4's `ParticipantTypeEnum` is CHARACTER-only) whose
 * `characterId` is an arbitrary valid UUID (no FK, never dereferenced).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-ntl.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-native-tool-loop-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ChatSpec {
  id: string;
  participant: string;
  character: string;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  chats: ChatSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'native-tool-loop-tier3.json'), 'utf8')) as Spec;

  const outMain = process.env.QT_FIXTURE_OUT;
  if (!outMain) throw new Error('QT_FIXTURE_OUT must point at the fixture .db file');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = outMain + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ntl-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
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
        title: `Loop ${chat.id}`,
        participants: [
          {
            id: chat.participant,
            type: 'CHARACTER',
            characterId: chat.character,
            controlledBy: 'llm',
            status: 'active',
            createdAt: spec.seedTimestamp,
            updatedAt: spec.seedTimestamp,
          },
        ],
        chatType: 'salon',
        contextSummary: null,
        impersonatingParticipantIds: [],
      } as never,
      { id: chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  await closeDatabase();
  process.stderr.write(`built native-tool-loop fixture: ${outMain} (${spec.chats.length} chats)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`native-tool-loop fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
