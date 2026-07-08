/**
 * Tier-3 SEED fixture builder for the Ariel terminal-announcement writer
 * differential (P4.1c — `postArielSessionOpened/TerminalOutput/SessionClosed`
 * + `reconcileTerminalSessionsForChat`).
 *
 * Bakes the seed BOTH sides start from, in ONE database (main only — the
 * writers touch nothing else), via v4's REAL repositories:
 *   - one `chats` row with two CHARACTER participants (P1 LLM-controlled,
 *     P2 user-controlled; the character ids are bare UUIDs — the Ariel
 *     writers never read characters), id + timestamps pinned;
 *   - the empty `chat_messages` table materialized (a `getMessageCount`
 *     read) so the Rust port — which does NOT own DDL — can INSERT;
 *   - the `terminal_sessions` seed rows the reconcile pass sweeps: one live
 *     orphan for the main chat (reconciled), one already-exited row for the
 *     main chat (skipped), one live orphan for another chat (untouched).
 *
 * The op sequence is run on a COPY of this seed on each side.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_ARIEL_MAIN=/tmp/qt-ariel-main.db \
 *     $N/npx tsx <v5>/harness/oracle/fixtures/build-ariel-writers-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface TerminalSeed {
  id: string;
  chatRef: 'main' | 'other';
  shell: string;
  cwd: string;
  exited: boolean;
  exitCode?: number;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  chat: { id: string; title: string };
  otherChatId: string;
  charAId: string;
  charBId: string;
  p1Id: string;
  p2Id: string;
  terminalSeeds: TerminalSeed[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'ariel-writers-tier3.json'), 'utf8')
  ) as Spec;

  const mainOut = process.env.QT_FIXTURE_ARIEL_MAIN;
  if (!mainOut) {
    throw new Error('QT_FIXTURE_ARIEL_MAIN must point at the .db file to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = mainOut + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ariel-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  // Never touched by these writers; a scratch path keeps the lazy mount client
  // pointed away from anything real.
  process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'mount-scratch.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  const repos = getRepositories();

  const participants = [
    {
      id: spec.p1Id,
      type: 'CHARACTER',
      characterId: spec.charAId,
      controlledBy: 'llm',
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    },
    {
      id: spec.p2Id,
      type: 'CHARACTER',
      characterId: spec.charBId,
      controlledBy: 'user',
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    },
  ];

  await repos.chats.create(
    {
      userId: spec.userId,
      title: spec.chat.title,
      participants,
    } as never,
    { id: spec.chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
  );

  // Materialize the empty chat_messages table so the Rust port can INSERT.
  await repos.chats.getMessageCount(spec.chat.id);

  // Terminal-session seed rows (the reconcile corpus). All timestamps pinned
  // to the seed sentinel so a minted exitedAt/updatedAt is detectable.
  for (const seed of spec.terminalSeeds) {
    const chatId = seed.chatRef === 'main' ? spec.chat.id : spec.otherChatId;
    await (repos as never as {
      terminalSessions: {
        create(data: unknown, options: unknown): Promise<unknown>;
      };
    }).terminalSessions.create(
      {
        chatId,
        label: null,
        shell: seed.shell,
        cwd: seed.cwd,
        startedAt: spec.seedTimestamp,
        exitedAt: seed.exited ? spec.seedTimestamp : null,
        exitCode: seed.exited ? seed.exitCode ?? null : null,
        transcriptPath: null,
      },
      { id: seed.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  await closeDatabase();

  process.stderr.write(`built ariel-writers seed fixture: main=${mainOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`ariel-writers fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
