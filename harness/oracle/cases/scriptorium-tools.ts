/**
 * Oracle case — the Project Scriptorium tool trio (`read_conversation` /
 * `upsert_annotation` / `delete_annotation`, W4.1d batch 1).
 *
 * Opens a COPY of the pre-baked fixture, drives v4's REAL tool handlers
 * (`executeReadConversationTool` / `executeUpsertAnnotationTool` /
 * `executeDeleteAnnotationTool`) + the `format*Results` functions for each op in
 * the committed spec order (state accumulates across ops), and emits per-op
 * `{ output, formatted }`. After the sequence, dumps the `conversation_annotations`
 * table (raw, ordered by id — the Rust test re-normalizes both sides by natural
 * key and placeholders the minted id/timestamps).
 *
 * The character NAME the annotation handlers need is passed via the op's
 * `characterName` field (v4's dispatcher resolves it from the calling participant;
 * here the spec supplies it directly, matching the handler's `context.characterName`).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SCRIPTORIUM=/tmp/qt-scriptorium.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/scriptorium-tools.ts \
 *     > /tmp/oracle-scriptorium.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  tool: 'read_conversation' | 'upsert_annotation' | 'delete_annotation';
  chatId: string;
  characterId?: string;
  characterName?: string;
  args: Record<string, unknown>;
}

interface Spec {
  testPepperBase64: string;
  userId: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'scriptorium-tools.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_SCRIPTORIUM;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_SCRIPTORIUM must point at the fixture from build-scriptorium-tools-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-scriptorium-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'scriptorium-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { executeReadConversationTool, formatReadConversationResults } = await import(
    '@/lib/tools/handlers/read-conversation-handler'
  );
  const { executeUpsertAnnotationTool, formatUpsertAnnotationResults } = await import(
    '@/lib/tools/handlers/upsert-annotation-handler'
  );
  const { executeDeleteAnnotationTool, formatDeleteAnnotationResults } = await import(
    '@/lib/tools/handlers/delete-annotation-handler'
  );

  await initializeDatabase();

  const results: Array<{ tool: string; output: unknown; formatted: string }> = [];

  for (const op of spec.ops) {
    if (op.tool === 'read_conversation') {
      const output = await executeReadConversationTool(op.args, {
        userId: spec.userId,
        chatId: op.chatId,
        characterId: op.characterId,
      });
      const formatted = formatReadConversationResults(output);
      results.push({ tool: op.tool, output, formatted });
    } else if (op.tool === 'upsert_annotation') {
      const output = await executeUpsertAnnotationTool(op.args, {
        userId: spec.userId,
        chatId: op.chatId,
        characterName: op.characterName as string,
      });
      const formatted = formatUpsertAnnotationResults(output);
      results.push({ tool: op.tool, output, formatted });
    } else {
      const output = await executeDeleteAnnotationTool(op.args, {
        userId: spec.userId,
        chatId: op.chatId,
        characterName: op.characterName as string,
      });
      const formatted = formatDeleteAnnotationResults(output);
      results.push({ tool: op.tool, output, formatted });
    }
  }

  // Dump the final annotations table state (raw on-disk cells).
  const columns = (
    (await rawQuery('PRAGMA table_info(conversation_annotations)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM conversation_annotations')) as Array<
    Record<string, unknown>
  >;

  await closeDatabase();

  const dump = canonicalizeRows({
    table: 'conversation_annotations',
    columns,
    rawRows,
    orderBy: 'id',
  });

  process.stdout.write(
    JSON.stringify({ case: 'scriptorium-tools', ops: results, dump }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`scriptorium-tools oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
