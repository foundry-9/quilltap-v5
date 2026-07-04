/**
 * Tier-2 oracle case — the whisper tool handler (W4.1d batch 1; v4
 * `lib/tools/handlers/whisper-handler.ts` `executeWhisperTool` +
 * `formatWhisperResults`).
 *
 * The handler is a pure DB path (reads chat + characters via `getRepositories()`,
 * writes ONE `chat_messages` row via `repos.chats.addMessage`) — it touches no
 * LLM — so it runs directly under the tsx real-DB path (no jest, no model mock),
 * like the memory-cascade / deletion-chokepoint cases. We copy the pre-seeded
 * fixture, run the fixed op sequence via v4's REAL `executeWhisperTool`, capture
 * each op's Output + the `formatWhisperResults` string, then dump `chat_messages`
 * canonically for the Rust diff.
 *
 * STOP-RULE: the write is ONLY a `chat_messages` row (the ported
 * `chats_messages::add_message` path). v4 does one `repos.chats.addMessage(...)`
 * and a `logger.info` — no post-office / SSE / broadcast side effect.
 *
 * NORMALIZATION SPEC: each successful whisper mints the message `id`
 * (`crypto.randomUUID()`) + `createdAt` (`new Date().toISOString()`); the Rust
 * diff remaps `id` to a per-row token (ordered by the pinned `chatId`) and
 * collapses `createdAt` to `<ts>`. `participantId` (the caller) +
 * `targetParticipantIds` (the resolved target) are the pinned fixture participant
 * ids, so the routing is verified exactly. Failure ops write no row.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WHISPER=/tmp/qt-whisper-main.db \
 *   QT_FIXTURE_WHISPER_MOUNT=/tmp/qt-whisper-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/whisper-tool.ts > /tmp/oracle-whisper.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  name: string;
  chatId: string;
  args: Record<string, unknown>;
  expect: Record<string, unknown>;
  formatted?: string;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  callingParticipantId: string;
  ops: Op[];
}

function assertDeepEqual(got: unknown, want: unknown, label: string): void {
  const g = JSON.stringify(got);
  const w = JSON.stringify(want);
  if (g !== w) {
    throw new Error(`${label}: diverged — got ${g}, want ${w}`);
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'whisper-tool.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_WHISPER;
  const fixtureMount = process.env.QT_FIXTURE_WHISPER_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error(
      'QT_FIXTURE_WHISPER + QT_FIXTURE_WHISPER_MOUNT must point at the seed fixtures from build-whisper-tool-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-whisper-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'whisper-main.db');
  const workMount = join(scratch, 'whisper-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { executeWhisperTool, formatWhisperResults } = await import(
    '@/lib/tools/handlers/whisper-handler'
  );

  await initializeDatabase();

  const returns: unknown[] = [];
  for (const op of spec.ops) {
    const result = await executeWhisperTool(op.args, {
      userId: spec.userId,
      chatId: op.chatId,
      callingParticipantId: spec.callingParticipantId,
    });
    assertDeepEqual(result, op.expect, `op ${op.name} output`);
    const formatted = formatWhisperResults(result);
    if (op.formatted !== undefined) {
      assertDeepEqual(formatted, op.formatted, `op ${op.name} formatted`);
    }
    returns.push({ name: op.name, output: result, formatted });
  }

  const columns = (
    (await rawQuery('PRAGMA table_info(chat_messages)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM chat_messages')) as Array<
    Record<string, unknown>
  >;
  const dump = canonicalizeRows({ table: 'chat_messages', columns, rawRows, orderBy: 'chatId' });

  await closeDatabase();

  const lines: string[] = [];
  lines.push(JSON.stringify({ case: 'whisper-tool', ...dump }));
  lines.push(JSON.stringify({ case: 'whisper-tool', returns }));
  process.stdout.write(lines.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`whisper-tool oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
