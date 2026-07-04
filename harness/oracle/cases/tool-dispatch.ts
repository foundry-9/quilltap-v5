/**
 * Oracle case — the dispatcher end-to-end differential (W4.1d batch 1).
 *
 * Opens a COPY of the pre-baked fixture and drives v4's REAL
 * `executeToolCallWithContext` (`lib/chat/tool-executor.ts` — the built-in
 * dispatch if-chain, UNMOCKED) for each op in the committed spec order (state
 * accumulates across ops). Emits per-op the full `ToolResult`
 * (`{ toolName, success, result, error?, message?, metadata? }`, JSON.stringify's
 * undefined-drop). After the sequence, dumps `conversation_annotations` (the write
 * ops mint fresh ids/timestamps → the Rust test placeholders them).
 *
 * The Rust side runs the same batch through the `BuiltInToolRunner` (the ported
 * dispatch) over a copy of the SAME seed — proving dispatch + handler + harness
 * compose end-to-end for a read, two writes (incl. the dispatcher's
 * character-NAME resolution from `callingParticipantId`), a pure tool, a handler
 * failure, an invalid-input failure, and the unknown-tool fallback
 * (`Unknown tool: <name>`, v4's no-plugin branch).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TOOLDISPATCH=/tmp/qt-tooldispatch-main.db \
 *   QT_FIXTURE_TOOLDISPATCH_MOUNT=/tmp/qt-tooldispatch-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/tool-dispatch.ts \
 *     > /tmp/oracle-tooldispatch.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  tool: string;
  chatId: string;
  characterId?: string;
  callingParticipantId?: string;
  args: Record<string, unknown>;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'tool-dispatch.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_TOOLDISPATCH;
  const fixtureMount = process.env.QT_FIXTURE_TOOLDISPATCH_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error(
      'QT_FIXTURE_TOOLDISPATCH and QT_FIXTURE_TOOLDISPATCH_MOUNT must point at the built fixture .db files'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-tooldispatch-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'main.db');
  const workMount = join(scratch, 'mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { executeToolCallWithContext } = await import('@/lib/chat/tool-executor');

  await initializeDatabase();

  const results: Array<{ tool: string; result: unknown }> = [];
  for (const op of spec.ops) {
    const toolResult = await executeToolCallWithContext(
      { name: op.tool, arguments: op.args },
      {
        chatId: op.chatId,
        userId: spec.userId,
        characterId: op.characterId,
        callingParticipantId: op.callingParticipantId,
      }
    );
    results.push({ tool: op.tool, result: toolResult });
  }

  const columns = (
    (await rawQuery('PRAGMA table_info(conversation_annotations)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM conversation_annotations')) as Array<
    Record<string, unknown>
  >;

  closeMountIndexSQLiteClient();
  await closeDatabase();

  const dump = canonicalizeRows({
    table: 'conversation_annotations',
    columns,
    rawRows,
    orderBy: 'id',
  });

  process.stdout.write(JSON.stringify({ case: 'tool-dispatch', ops: results, dump }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`tool-dispatch oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
