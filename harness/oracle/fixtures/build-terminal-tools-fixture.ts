/**
 * Read-differential fixture builder — the shared starting state for v4's Prospero
 * terminal tools (`terminal_read` / `terminal_list`, W4.1d batch 1).
 *
 * Seeds a user, a chat, and three `terminal_sessions` rows via v4's REAL
 * `repos.terminalSessions.create(data, { id, createdAt, updatedAt })` (ids +
 * timestamps pinned → fully deterministic). Two sessions belong to the main chat
 * (one live/no-exit, one exited with an exitCode); the third belongs to a
 * DIFFERENT chat (to drive the wrong-chat throw).
 *
 * ## The scrollback-source seam
 *
 * v4's `executeTerminalReadTool` resolves the raw scrollback from the live PTY
 * ring buffer OR from the transcript file (`tailFile(session.transcriptPath)`).
 * There is no live PTY in the oracle (`ptyManager.get` returns undefined on a
 * fresh module), so v4 falls to `tailFile`. This builder writes each session's
 * `scrollbackContent` (from the spec, control bytes and all) to a `.log` file in
 * a sibling directory `<fixtureDb>.scrollback/` and records the ABSOLUTE path as
 * the session's `transcriptPath`. v4's `tailFile` then reads exactly those bytes;
 * the Rust differential reads the same files (via the transcriptPath in the row)
 * and hands the bytes to `execute_terminal_read` — identical bytes on both sides.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TERMTOOLS=/tmp/qt-termtools.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-terminal-tools-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
  existsSync,
} from 'node:fs';
import { tmpdir } from 'node:os';

interface SessionSpec {
  id: string;
  chatId: string;
  label: string | null;
  shell: string;
  cwd: string;
  startedAt: string;
  exitedAt: string | null;
  exitCode: number | null;
  transcriptFile: string;
  scrollbackContent: string;
}

interface Spec {
  testPepperBase64: string;
  chatId: string;
  otherChatId: string;
  userId: string;
  sessions: SessionSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'terminal-tools-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_TERMTOOLS;
  if (!out) {
    throw new Error('QT_FIXTURE_TERMTOOLS must point at the fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  // Scrollback transcript files live in a sibling dir the Rust test can also find.
  const scrollbackDir = out + '.scrollback';
  rmSync(scrollbackDir, { recursive: true, force: true });
  mkdirSync(scrollbackDir, { recursive: true });

  const scratch = mkdtempSync(join(tmpdir(), 'qt-termtools-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { TerminalSessionsRepository } = await import(
    '@/lib/database/repositories/terminal-sessions.repository'
  );
  const { TerminalSessionSchema } = await import('@/lib/schemas/terminal.types');

  await initializeDatabase();
  await ensureCollection('terminal_sessions', TerminalSessionSchema);

  const repo = new TerminalSessionsRepository();

  for (const s of spec.sessions) {
    // Write the transcript file with exact bytes.
    const transcriptPath = join(scrollbackDir, s.transcriptFile);
    writeFileSync(transcriptPath, s.scrollbackContent, 'utf8');

    await repo.create(
      {
        chatId: s.chatId,
        label: s.label,
        shell: s.shell,
        cwd: s.cwd,
        startedAt: s.startedAt,
        exitedAt: s.exitedAt,
        exitCode: s.exitCode,
        transcriptPath,
      } as never,
      { id: s.id, createdAt: s.startedAt, updatedAt: s.startedAt }
    );
  }

  await closeDatabase();
  process.stderr.write(
    `built terminal-tools fixture: ${out} (${spec.sessions.length} sessions; scrollback in ${scrollbackDir})\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`terminal-tools fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
