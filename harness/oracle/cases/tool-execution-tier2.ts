/**
 * Tier-2 oracle case — W4.1c sub-unit c.1 (v4 saveToolMessages).
 *
 * Runs the fixed call sequence (`tool-execution-tier2.json`) via v4's REAL
 * `saveToolMessages` (lib/services/chat-message/tool-execution.service.ts) on a
 * copy of the seed fixture, then dumps the `chat_messages` rows (the TOOL-row
 * write marshaling + content JSON) and the `files` rows (the addLink/addTag image
 * plumbing) canonically, plus each call's return `{ firstToolMessageId,
 * generatedImageIds }`.
 *
 * No model call — saveToolMessages only writes rows — so this is a tsx real-DB
 * oracle (no jest). Minted ids/timestamps are remapped/placeholdered identically
 * on the Rust side (see the Rust test header).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TOOLEXEC=/tmp/qt-toolexec-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/tool-execution-tier2.ts > /tmp/oracle-toolexec.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface WhisperContext {
  userParticipantId: string | null;
  allowCrossCharacterVaultReads: boolean;
}
interface Call {
  name: string;
  chatId: string;
  characterId?: string;
  participantId?: string;
  whisperContext?: WhisperContext;
  generatedImagePaths: Array<Record<string, unknown>>;
  toolMessages: Array<Record<string, unknown>>;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  calls: Call[];
}

async function dumpTable(rawQuery: (sql: string) => Promise<unknown>, table: string) {
  const columns = (
    (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
  return canonicalizeRows({ table, columns, rawRows, orderBy: 'id' });
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'tool-execution-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_TOOLEXEC;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_TOOLEXEC must point at the seed fixture from build-tool-execution-fixture.ts');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-toolexec-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'toolexec-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { saveToolMessages } = await import('@/lib/services/chat-message/tool-execution.service');

  await initializeDatabase();
  const repos = getRepositories();

  const returns: Array<{ name: string; firstToolMessageId: string | null; generatedImageIds: string[] }> = [];
  for (const call of spec.calls) {
    const result = await saveToolMessages(
      repos,
      call.chatId,
      spec.userId,
      call.toolMessages as never,
      call.generatedImagePaths as never,
      call.characterId,
      call.participantId,
      call.whisperContext as never,
    );
    returns.push({ name: call.name, firstToolMessageId: result.firstToolMessageId, generatedImageIds: result.generatedImageIds });
  }

  const messages = await dumpTable(rawQuery, 'chat_messages');
  const chats = await dumpTable(rawQuery, 'chats');
  const files = await dumpTable(rawQuery, 'files');

  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'tool-execution-tier2', messages, chats, files, returns }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`tool-execution-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
