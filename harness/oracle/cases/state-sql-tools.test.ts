/**
 * @jest-environment node
 *
 * Differential ORACLE for the W4.1d5 `state` + `run_sql` tools. Drives v4's REAL
 * handlers (executeStateTool / formatStateResults, executeRunSqlTool) over the
 * shared three-DB fixture, ONE fresh copy of all three DBs per case (state WRITES),
 * emitting one NDJSON line per case:
 *   - state: { label, kind:'state', resultJson, formatted, chatsDump, projectState }
 *   - run_sql: { label, kind:'run_sql', resultJson }
 *
 * `chatsDump` is the canonicalized `chats` table (proves chat-state writes; no
 * updatedAt mint → zero normalization). `projectState` is the project's read-back
 * `state` (proves project-state writes through the store overlay — the read-back
 * approach, per the vault_wardrobe_public precedent, since the overlay bytes are
 * already proven by projects_tier2).
 *
 * Real-DB-under-jest (search-tools recipe): resetModules + doMock past jest.setup's
 * global DB mocks; better-sqlite3 → better-sqlite3-multiple-ciphers by abs path;
 * the three raw sqlite client getters → requireActual so run_sql reaches the real
 * handles. NO model boundary is touched.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5 ; STAGE=/tmp/qt-oracle-stage
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-state-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-state-mount.db \
 *   QT_FIXTURE_TMP_LLM=/tmp/qt-state-llm.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-state-sql-tools-fixture.ts
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-state-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-state-mount.db \
 *   QT_FIXTURE_TMP_LLM=/tmp/qt-state-llm.db QT_ORACLE_OUT=/tmp/oracle-state-sql-tools.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- state-sql-tools
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

interface Spec {
  testPepperBase64: string;
  userId: string;
  wrongUserId: string;
  chatProjectId: string;
  chatSoloId: string;
  projectId: string;
  charAId: string;
  charBId: string;
  charDId: string;
  groupAlphaId: string;
  groupTwin2Id: string;
}

interface StateCase {
  label: string;
  kind: 'state';
  chatKey: 'chatProjectId' | 'chatSoloId' | 'bogus';
  /** Responding character (the group-tier scope); absent -> no characterId. */
  charKey?: 'charAId' | 'charBId' | 'charDId';
  useWrongUser?: boolean;
  args: unknown;
  /** Resolve spec-relative values in args (e.g. the group-by-id ref). */
  argsFrom?: (spec: Spec) => unknown;
}
interface SqlCase {
  label: string;
  kind: 'run_sql';
  args: unknown;
}
type Case = StateCase | SqlCase;

const STATE_CASES: StateCase[] = [
  { label: 'fetch_chat_root', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'fetch', context: 'chat' } },
  { label: 'fetch_chat_path', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'fetch', context: 'chat', path: 'hp' } },
  { label: 'fetch_chat_nested', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'fetch', context: 'chat', path: 'flags.seen' } },
  { label: 'fetch_chat_missing', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'fetch', context: 'chat', path: 'nope' } },
  { label: 'fetch_project_path', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'fetch', context: 'project', path: 'weather' } },
  { label: 'fetch_merged_project_key', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'fetch', path: 'difficulty' } },
  { label: 'fetch_merged_chat_key', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'fetch', path: 'hp' } },
  { label: 'fetch_project_not_in_project', kind: 'state', chatKey: 'chatSoloId', args: { operation: 'fetch', context: 'project', path: 'x' } },
  { label: 'set_chat_new', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'set', context: 'chat', path: 'score', value: 42 } },
  { label: 'set_chat_overwrite', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'set', context: 'chat', path: 'hp', value: 5 } },
  { label: 'set_chat_nested_create', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'set', context: 'chat', path: 'stats.level', value: 3 } },
  { label: 'set_project', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'set', context: 'project', path: 'weather', value: 'stormy' } },
  { label: 'set_underscore_refused', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'set', path: '_secret', value: 1 } },
  { label: 'delete_chat', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'delete', context: 'chat', path: 'mood' } },
  { label: 'delete_chat_missing', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'delete', context: 'chat', path: 'nope' } },
  { label: 'delete_project', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'delete', context: 'project', path: 'difficulty' } },
  { label: 'permission_denied', kind: 'state', chatKey: 'chatProjectId', useWrongUser: true, args: { operation: 'fetch', context: 'chat' } },
  { label: 'chat_not_found', kind: 'state', chatKey: 'bogus', args: { operation: 'fetch', context: 'chat' } },
  { label: 'validation_bad_op', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'bogus' } },
  { label: 'validation_nonobject', kind: 'state', chatKey: 'chatProjectId', args: 'nope' },
  // ── P4.d10: the four-tier families (f48f34dc) ──
  { label: 'fetch_merged_zero_groups', kind: 'state', chatKey: 'chatProjectId', charKey: 'charDId', args: { operation: 'fetch', path: 'gscore' } },
  { label: 'fetch_merged_one_group', kind: 'state', chatKey: 'chatProjectId', charKey: 'charAId', args: { operation: 'fetch', path: 'gscore' } },
  { label: 'fetch_merged_two_groups', kind: 'state', chatKey: 'chatProjectId', charKey: 'charBId', args: { operation: 'fetch', path: 'twin' } },
  { label: 'fetch_merged_general_key', kind: 'state', chatKey: 'chatProjectId', charKey: 'charAId', args: { operation: 'fetch', path: 'gen_only' } },
  { label: 'fetch_group_sole', kind: 'state', chatKey: 'chatProjectId', charKey: 'charAId', args: { operation: 'fetch', context: 'group', path: 'gscore' } },
  { label: 'fetch_group_by_id', kind: 'state', chatKey: 'chatProjectId', charKey: 'charBId', args: {}, argsFrom: (s) => ({ operation: 'fetch', context: 'group', group: s.groupTwin2Id, path: 'twin' }) },
  { label: 'fetch_group_by_name', kind: 'state', chatKey: 'chatProjectId', charKey: 'charAId', args: { operation: 'fetch', context: 'group', group: 'alpha lodge', path: 'k' } },
  { label: 'fetch_group_ambiguous_name', kind: 'state', chatKey: 'chatProjectId', charKey: 'charBId', args: { operation: 'fetch', context: 'group', group: 'TWIN LODGE', path: 'twin' } },
  { label: 'fetch_group_ref_required', kind: 'state', chatKey: 'chatProjectId', charKey: 'charBId', args: { operation: 'fetch', context: 'group', path: 'twin' } },
  { label: 'fetch_group_no_character', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'fetch', context: 'group', path: 'twin' } },
  { label: 'fetch_general', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'fetch', context: 'general', path: 'gen_only' } },
  { label: 'fetch_general_root', kind: 'state', chatKey: 'chatSoloId', args: { operation: 'fetch', context: 'general' } },
  { label: 'set_group_sole', kind: 'state', chatKey: 'chatProjectId', charKey: 'charAId', args: { operation: 'set', context: 'group', path: 'gscore', value: 2 } },
  { label: 'set_group_underscore_refused', kind: 'state', chatKey: 'chatProjectId', charKey: 'charAId', args: { operation: 'set', context: 'group', path: '_hidden', value: 1 } },
  { label: 'set_general_new_key', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'set', context: 'general', path: 'new_flag', value: 'lit' } },
  { label: 'set_general_overwrite', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'set', context: 'general', path: 'k', value: 'general2' } },
  { label: 'delete_group_key', kind: 'state', chatKey: 'chatProjectId', charKey: 'charAId', args: { operation: 'delete', context: 'group', path: 'gscore' } },
  { label: 'delete_general_key', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'delete', context: 'general', path: 'gen_only' } },
  { label: 'delete_general_underscore_refused', kind: 'state', chatKey: 'chatProjectId', args: { operation: 'delete', context: 'general', path: '_secret' } },
  { label: 'delete_group_missing_key', kind: 'state', chatKey: 'chatProjectId', charKey: 'charAId', args: { operation: 'delete', context: 'group', path: 'nope' } },
];

const SQL_CASES: SqlCase[] = [
  { label: 'sql_main_select', kind: 'run_sql', args: { sql: 'SELECT id, title FROM chats ORDER BY id' } },
  { label: 'sql_main_blob', kind: 'run_sql', args: { sql: 'SELECT id, embedding FROM memories' } },
  { label: 'sql_main_default_db', kind: 'run_sql', args: { sql: 'SELECT 1 AS n' } },
  { label: 'sql_mount', kind: 'run_sql', args: { sql: 'SELECT name FROM doc_mount_points ORDER BY name', database: 'mount-index' } },
  { label: 'sql_llm', kind: 'run_sql', args: { sql: 'SELECT id, provider, modelName FROM llm_logs', database: 'llm-logs' } },
  { label: 'sql_readonly_pragma', kind: 'run_sql', args: { sql: 'PRAGMA table_info(chats)' } },
  { label: 'sql_truncation', kind: 'run_sql', args: { sql: 'SELECT 1 AS n UNION ALL SELECT 2 UNION ALL SELECT 3', max_rows: 2 } },
  { label: 'sql_write_refusal', kind: 'run_sql', args: { sql: 'DELETE FROM chats' } },
  { label: 'sql_write_in_cte', kind: 'run_sql', args: { sql: 'WITH t AS (SELECT 1) DELETE FROM chats' } },
  { label: 'sql_multi_statement', kind: 'run_sql', args: { sql: 'SELECT 1; SELECT 2' } },
  { label: 'sql_empty', kind: 'run_sql', args: { sql: '   ' } },
  { label: 'sql_prepare_error', kind: 'run_sql', args: { sql: 'SELECT * FROM nonexistent_table' } },
  { label: 'sql_mutating_pragma', kind: 'run_sql', args: { sql: 'PRAGMA journal_mode = WAL' } },
  { label: 'sql_validation_nonobject', kind: 'run_sql', args: 'nope' },
];

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'state-sql-tools.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_TMP_MAIN;
  const mountFixture = process.env.QT_FIXTURE_TMP_MOUNT;
  const llmFixture = process.env.QT_FIXTURE_TMP_LLM;
  if (!mainFixture || !mountFixture || !llmFixture) {
    throw new Error('QT_FIXTURE_TMP_MAIN / _MOUNT / _LLM must point at the seeded fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );

  const lines: string[] = [];
  const cases: Case[] = [...STATE_CASES, ...SQL_CASES];

  for (const c of cases) {
    const scratch = mkdtempSync(join(tmpdir(), 'qt-state-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'main-work.db');
    const mountWork = join(scratch, 'mount-work.db');
    const llmWork = join(scratch, 'llm-work.db');
    copyFileSync(mainFixture, mainWork);
    copyFileSync(mountFixture, mountWork);
    copyFileSync(llmFixture, llmWork);

    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
    process.env.SQLITE_LLM_LOGS_PATH = llmWork;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    jest.resetModules();
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
    jest.doMock('@/lib/database/backends/sqlite/client', () =>
      jest.requireActual('@/lib/database/backends/sqlite/client'),
    );
    jest.doMock('@/lib/database/backends/sqlite/llm-logs-client', () =>
      jest.requireActual('@/lib/database/backends/sqlite/llm-logs-client'),
    );
    jest.doMock('@/lib/database/backends/sqlite/mount-index-client', () =>
      jest.requireActual('@/lib/database/backends/sqlite/mount-index-client'),
    );

    const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
    await initializeDatabase();

    if (c.kind === 'state') {
      const { executeStateTool, formatStateResults } = await import(
        '@/lib/tools/handlers/state-handler'
      );
      const { getRepositories } = await import('@/lib/repositories/factory');
      const chatId =
        c.chatKey === 'bogus'
          ? 'deadbeef-0000-4000-8000-000000000000'
          : (spec[c.chatKey] as string);
      const args = c.argsFrom ? c.argsFrom(spec) : c.args;
      const out = await executeStateTool(args, {
        userId: c.useWrongUser ? spec.wrongUserId : spec.userId,
        chatId,
        // projectId omitted → the handler falls back to chat.projectId.
        ...(c.charKey ? { characterId: spec[c.charKey] as string } : {}),
      });
      const resultJson = JSON.stringify(out);
      const formatted = formatStateResults(out);

      // Dump the chats table canonicalized (column order via table_info).
      const colInfo = (await rawQuery('PRAGMA table_info(chats)')) as Array<{ name: string }>;
      const rows = (await rawQuery('SELECT * FROM chats ORDER BY id')) as Array<
        Record<string, unknown>
      >;
      const chatsDump = rows.map((raw) => {
        const obj: Record<string, unknown> = {};
        for (const col of colInfo) obj[col.name] = canonValue(raw[col.name]);
        return obj;
      });

      // Read back the project state (proves the store-overlay write).
      let projectState: unknown = null;
      const project = await getRepositories().projects.findById(spec.projectId);
      projectState = project ? (project.state ?? null) : null;

      // Read back the Alpha group state + the general state (prove the P4.d10
      // group/general-tier writes the same way).
      const alpha = await getRepositories().groups.findById(spec.groupAlphaId);
      const groupAlphaState: unknown = alpha ? ((alpha as { state?: unknown }).state ?? null) : null;
      const { readGeneralState } = await import('@/lib/mount-index/general-state');
      const generalState = await readGeneralState();

      // Close the mount-index client so the NEXT case's fresh copy is read
      // through a fresh client (closeDatabase() does NOT close it, so a
      // persistent singleton would otherwise leak this case's writes forward).
      const { closeMountIndexSQLiteClient } = await import(
        '@/lib/database/backends/sqlite/mount-index-client'
      );
      closeMountIndexSQLiteClient();

      lines.push(JSON.stringify({ label: c.label, kind: 'state', resultJson, formatted, chatsDump, projectState, groupAlphaState, generalState }));
    } else {
      const { executeRunSqlTool } = await import('@/lib/tools/handlers/run-sql-handler');
      // Touch the llm-logs client so getRawLLMLogsDatabase() returns a handle.
      const { getRawLLMLogsDatabase } = await import(
        '@/lib/database/backends/sqlite/llm-logs-client'
      );
      getRawLLMLogsDatabase();
      const out = await executeRunSqlTool(c.args, { userId: spec.userId });
      const resultJson = JSON.stringify(out);
      lines.push(JSON.stringify({ label: c.label, kind: 'run_sql', resultJson }));
    }

    await closeDatabase();
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`state-sql-tools oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('state-sql-tools oracle', async () => {
  await main();
});
