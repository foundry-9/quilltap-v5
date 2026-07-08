/**
 * Tier-3 ORACLE for the Ariel terminal-announcement writers + the
 * terminal-session reconcile pass (P4.1c).
 *
 * Drives v4's REAL `postArielSessionOpenedAnnouncement` /
 * `postArielTerminalOutputAnnouncement` / `postArielSessionClosedAnnouncement`
 * (`lib/services/ariel-notifications/writer.ts`) and the REAL
 * `reconcileTerminalSessionsForChat` (`lib/terminal/reconcile.ts` — the PTY
 * manager's session map is empty under the oracle, so every live-orphan row
 * reconciles; the Rust side passes the matching `is_live = |_| false` probe)
 * against a COPY of the seed fixture, then dumps `chat_messages` (the persona
 * rows byte-for-byte), `chats` (the addMessage side-effect), and
 * `terminal_sessions` (the reconcile exit stamps), plus the per-case posted
 * booleans / reconcile counts. NO seams are mocked — the writers are
 * self-contained; this runs under plain `tsx`.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_ARIEL_MAIN=/tmp/qt-ariel-main.db \
 *     $N/npx tsx <v5>/harness/oracle/cases/ariel-writers-tier3.ts \
 *     > /tmp/oracle-ariel.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Case {
  name: string;
  kind: 'opened' | 'output' | 'closed' | 'reconcile';
  chatRef?: 'missing';
  sessionId?: string;
  label?: string | null;
  shell?: string;
  cwd?: string;
  reason?: 'idle' | 'max-age' | 'session-closed';
  cleanedOutput?: string;
  repeatSegment?: string;
  repeatTimes?: number;
  exitCode?: number | null;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  chat: { id: string };
  missingChatId: string;
  cases: Case[];
}

function caseOutput(c: Case): string {
  if (c.cleanedOutput !== undefined) return c.cleanedOutput;
  if (c.repeatSegment !== undefined && c.repeatTimes !== undefined) {
    return c.repeatSegment.repeat(c.repeatTimes);
  }
  throw new Error(`case ${c.name}: no cleanedOutput`);
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'ariel-writers-tier3.json'), 'utf8')
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_ARIEL_MAIN;
  if (!mainFixture || !existsSync(mainFixture)) {
    throw new Error(
      'QT_FIXTURE_ARIEL_MAIN must point at the fixture from build-ariel-writers-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ariel-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'ariel-main-work.db');
  copyFileSync(mainFixture, mainWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'mount-scratch.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const writer = await import('@/lib/services/ariel-notifications/writer');
  const { reconcileTerminalSessionsForChat } = await import('@/lib/terminal/reconcile');

  await initializeDatabase();

  const results: Array<{ name: string; posted?: boolean; reconciled?: number }> = [];

  for (const c of spec.cases) {
    const chatId = c.chatRef === 'missing' ? spec.missingChatId : spec.chat.id;
    switch (c.kind) {
      case 'opened': {
        const posted = await writer.postArielSessionOpenedAnnouncement({
          chatId,
          sessionId: c.sessionId!,
          label: c.label,
          shell: c.shell!,
          cwd: c.cwd!,
        });
        results.push({ name: c.name, posted: posted !== null });
        break;
      }
      case 'output': {
        const posted = await writer.postArielTerminalOutputAnnouncement({
          chatId,
          sessionId: c.sessionId!,
          cleanedOutput: caseOutput(c),
          reason: c.reason!,
        });
        results.push({ name: c.name, posted: posted !== null });
        break;
      }
      case 'closed': {
        const posted = await writer.postArielSessionClosedAnnouncement({
          chatId,
          sessionId: c.sessionId!,
          exitCode: c.exitCode ?? null,
        });
        results.push({ name: c.name, posted: posted !== null });
        break;
      }
      case 'reconcile': {
        const reconciled = await reconcileTerminalSessionsForChat(spec.chat.id);
        results.push({ name: c.name, reconciled });
        break;
      }
      default:
        throw new Error(`unknown case kind ${(c as { kind: string }).kind}`);
    }
  }

  const dumpTable = async (table: string, orderBy: string) => {
    const columns = (
      (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
    ).map((col) => col.name);
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  // chat_messages ordered by `content` — every Ariel body embeds a distinct
  // `<!-- terminalSessionId:… -->` comment, so contents are unique and the
  // order is deterministic on both sides. `chats` and `terminal_sessions` are
  // keyed by id.
  const chatMessages = await dumpTable('chat_messages', 'content');
  const chats = await dumpTable('chats', 'id');
  const terminalSessions = await dumpTable('terminal_sessions', 'id');

  await closeDatabase();

  process.stdout.write(
    JSON.stringify({
      case: 'ariel-writers-tier3',
      results,
      chatMessages,
      chats,
      terminalSessions,
    }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`ariel-writers-tier3 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
