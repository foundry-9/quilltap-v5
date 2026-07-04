/**
 * @jest-environment node
 *
 * Differential ORACLE for the two NON-embedding search tools: `request_full_context`
 * and `project_info` (v4 executeRequestFullContextTool / executeProjectInfoTool +
 * their format* fns). Drives v4's REAL handlers over the shared two-DB fixture,
 * ONE fresh copy per case (request_full_context WRITES a chats column), and emits
 * one NDJSON line per case: `{ label, resultJson: JSON.stringify(output),
 * formatted: format*(output) }`. request_full_context cases additionally emit the
 * full `chats` row (`chatRow`) so the flag-flip + updatedAt-preservation is proven.
 *
 * Real-DB-under-jest (same recipe as memory-gate-tier3): resetModules + doMock past
 * jest.setup's global DB mocks, better-sqlite3 → better-sqlite3-multiple-ciphers by
 * absolute path. NO model boundary is touched (neither tool embeds).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-search-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-search-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-search-tools-fixture.ts
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-search-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-search-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-search-tools-readwrite.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- search-tools-readwrite
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

// Canonicalize a raw SQLite cell the way the Rust harness's dump_table_json_conn
// does (BLOB -> lowercase hex, null explicit, everything else as-is; an
// integer-valued REAL from better-sqlite3 already arrives as a JS number that
// JSON.stringify collapses). Keeps the chats-row dump byte-comparable.
function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

interface Spec {
  testPepperBase64: string;
  userId: string;
  projectId: string;
  minimalProjectId: string;
  bogusProjectId: string;
  chatAId: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'search-tools.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_TMP_MAIN;
  const mountFixture = process.env.QT_FIXTURE_TMP_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_TMP_MAIN and QT_FIXTURE_TMP_MOUNT must point at the seeded fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  // Restore the real DB stack past jest.setup's global mocks (no model boundary).
  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

  const lines: string[] = [];

  // Each case runs on its own fresh copy (request_full_context writes) so state
  // never leaks between cases.
  type Case = {
    label: string;
    tool: 'project_info' | 'request_full_context';
    args: unknown;
    // project_info context
    projectId?: string;
    // request_full_context context
    chatId?: string;
  };
  const cases: Case[] = [
    // project_info
    { label: 'pi_get_info_rich', tool: 'project_info', args: { action: 'get_info' }, projectId: spec.projectId },
    { label: 'pi_get_info_minimal', tool: 'project_info', args: { action: 'get_info' }, projectId: spec.minimalProjectId },
    { label: 'pi_get_instructions_present', tool: 'project_info', args: { action: 'get_instructions' }, projectId: spec.projectId },
    { label: 'pi_get_instructions_none', tool: 'project_info', args: { action: 'get_instructions' }, projectId: spec.minimalProjectId },
    { label: 'pi_project_not_found', tool: 'project_info', args: { action: 'get_info' }, projectId: spec.bogusProjectId },
    { label: 'pi_invalid_action', tool: 'project_info', args: { action: 'nope' }, projectId: spec.projectId },
    { label: 'pi_invalid_nonobject', tool: 'project_info', args: 'not-an-object', projectId: spec.projectId },
    // request_full_context
    { label: 'rfc_write', tool: 'request_full_context', args: {}, chatId: spec.chatAId },
    { label: 'rfc_invalid_nonobject', tool: 'request_full_context', args: 'nope', chatId: spec.chatAId },
  ];

  for (const c of cases) {
    // Fresh copy per case.
    const scratch = mkdtempSync(join(tmpdir(), 'qt-search-rw-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'main-work.db');
    const mountWork = join(scratch, 'mount-work.db');
    copyFileSync(mainFixture, mainWork);
    copyFileSync(mountFixture, mountWork);

    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    jest.resetModules();
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

    const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');

    await initializeDatabase();

    let resultJson: string;
    let formatted: string;
    let chatRow: unknown = undefined;

    if (c.tool === 'project_info') {
      const { executeProjectInfoTool, formatProjectInfoResults } = await import(
        '@/lib/tools/handlers/project-info-handler'
      );
      const out = await executeProjectInfoTool(c.args, {
        userId: spec.userId,
        projectId: c.projectId as string,
      });
      resultJson = JSON.stringify(out);
      formatted = formatProjectInfoResults(out);
    } else {
      const { executeRequestFullContextTool, formatRequestFullContextResults } = await import(
        '@/lib/tools/handlers/request-full-context-handler'
      );
      const out = await executeRequestFullContextTool(c.args, { chatId: c.chatId as string });
      resultJson = JSON.stringify(out);
      formatted = formatRequestFullContextResults(out);
      // Dump the full chats row canonicalized (proves the flag flips +
      // updatedAt + every other column preserved). Column order from table_info
      // so it matches the Rust dump's SELECT * column order.
      const colInfo = (await rawQuery('PRAGMA table_info(chats)')) as Array<{ name: string }>;
      const rows = (await rawQuery('SELECT * FROM chats WHERE id = ?', [c.chatId])) as Array<
        Record<string, unknown>
      >;
      const raw = rows[0];
      if (raw) {
        const obj: Record<string, unknown> = {};
        for (const col of colInfo) obj[col.name] = canonValue(raw[col.name]);
        chatRow = obj;
      } else {
        chatRow = null;
      }
    }

    await closeDatabase();

    lines.push(JSON.stringify({ label: c.label, resultJson, formatted, chatRow }));
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`search-tools-readwrite oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('search-tools readwrite oracle', async () => {
  await main();
});
