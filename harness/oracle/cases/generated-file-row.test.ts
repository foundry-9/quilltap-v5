/**
 * @jest-environment node
 *
 * Tier-2 ORACLE for the generated `files` row (P4.D283; v4
 * `lib/files/generated-file-row.ts` `createGeneratedFileRow`, NEW at
 * `9ecebf986`), ported to `quilltap-core::files::generated_file_row`.
 *
 * Drives v4's REAL `createGeneratedFileRow(repos, input)` over a FRESH copy
 * of the wardrobe-item-image-generation fixture's MAIN db per case (it carries
 * the owner and the `files` table; the mount db is not touched), with v4's
 * logger spied for the `[GeneratedFileRow]` line. Emits per case: the
 * returned row's id (`<minted>` when the input pinned none), the one `files`
 * row (`SELECT *` by id, BLOBs hex), and the captured lines.
 *
 * Run (Node 24, from the v4 checkout — a /tmp mirror, jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-gfr-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/generated-file-row.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/generated-file-row.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WIIG_MAIN=/tmp/qt-gfr-main.db QT_FIXTURE_WIIG_MOUNT=/tmp/qt-gfr-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wardrobe-item-image-generation-fixture.ts
 *   QT_FIXTURE_GFR_MAIN=/tmp/qt-gfr-main.db QT_ORACLE_OUT=/tmp/oracle-generated-file-row.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "cases/generated-file-row\.test\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

interface Case {
  name: string;
  input: Record<string, unknown> & { id: string | null };
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  cases: Case[];
}

const LOG_PREFIXES = ['[GeneratedFileRow]'];

function sanitize(meta: unknown): unknown {
  if (meta === undefined) return null;
  return JSON.parse(JSON.stringify(meta, (_k, v) => (v instanceof Error ? v.message : v)));
}

async function runCase(spec: Spec, c: Case, scratch: string, mainFixture: string) {
  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

  const work = mkdtempSync(join(scratch, 'gfr-'));
  const mainWork = join(work, 'main.db');
  copyFileSync(mainFixture, mainWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = join(work, 'mount.db');
  process.env.SQLITE_LLM_LOGS_PATH = join(work, 'llm-logs.db');

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const loggerModule = await import('@/lib/logger');
  const { createGeneratedFileRow } = await import('@/lib/files/generated-file-row');

  await initializeDatabase();
  const repos = getRepositories();
  const raw = getRawDatabase();
  if (!raw) throw new Error('DB handle unavailable');

  const logs: Array<{ level: string; message: string; meta: unknown }> = [];
  const spies: jest.SpyInstance[] = [];
  for (const level of ['debug', 'info', 'warn', 'error'] as const) {
    spies.push(
      jest.spyOn(loggerModule.logger as never, level as never).mockImplementation(((
        message: string,
        meta?: unknown,
      ) => {
        if (typeof message === 'string' && LOG_PREFIXES.some((p) => message.startsWith(p))) {
          logs.push({ level, message, meta: sanitize(meta) });
        }
      }) as never),
    );
  }

  const { id, ...rest } = c.input;
  const input = { userId: spec.userId, ...(id ? { id } : {}), ...rest };
  let returnedId: string;
  let threw: string | null = null;
  try {
    const row = await createGeneratedFileRow(repos, input as never);
    returnedId = row.id;
  } catch (e) {
    threw = (e as Error).message;
    returnedId = '';
  }
  for (const s of spies) s.mockRestore();

  try {
    const columns = (raw.prepare('PRAGMA table_info(files)').all() as Array<{ name: string }>).map(
      (col) => col.name,
    );
    const rows = (raw.prepare('SELECT * FROM files WHERE id = ?').all(returnedId) as Array<
      Record<string, unknown>
    >).map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    });
    return {
      name: c.name,
      returnedId: id ? returnedId : returnedId ? '<minted>' : '',
      mintedIdIsUuid: id ? null : /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(returnedId),
      threw,
      row: rows[0] ?? null,
      logs,
    };
  } finally {
    await closeDatabase();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'generated-file-row.json'), 'utf8'),
  ) as Spec;
  const mainFixture = process.env.QT_FIXTURE_GFR_MAIN;
  if (!mainFixture || !existsSync(mainFixture)) {
    throw new Error('QT_FIXTURE_GFR_MAIN must point at the MAIN seed from build-wardrobe-item-image-generation-fixture.ts');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-gfr-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];
  for (const c of spec.cases) outLines.push(JSON.stringify(await runCase(spec, c, scratch, mainFixture)));
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`generated-file-row oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('generated-file-row tier-2 oracle', async () => {
  await main();
});

const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
