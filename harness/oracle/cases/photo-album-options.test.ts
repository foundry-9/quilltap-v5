/**
 * @jest-environment node
 *
 * P4.D275 tier-2 ORACLE for v4's `listAllPhotoAlbumOptions` (`lib/photos/
 * photo-album-options.ts`, `01a83539d`) — the wardrobe picture viewer's
 * every-store save-target list — ported as `quilltap-core::photos::
 * photo_album_options::list_all_photo_album_options`.
 *
 * Drives v4's REAL function over `getRepositories()` with the real cipher
 * binding (the DB stack doMocked back to the real modules — the
 * `wardrobe-item-images-routes.test.ts` idiom; jest.setup mocks them all).
 * Per case: a FRESH module graph and fixture copy, the case's `main` /
 * `mount` SQL plants (`fixtures/photo-album-options.json`), then ONE call,
 * with every log line the call emits captured through the logger's shared
 * transports (children included). Emits `{ name, albums, logs }`.
 *
 * Run (Node 24, from the v4 checkout or the round's pin — a /tmp mirror,
 * jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-pao-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/photo-album-options.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/wardrobe-item-images-tier2.json" "$TMPO/fixtures/"
 *   cp "$V5W/harness/oracle/fixtures/photo-album-options.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   rm -f /tmp/qt-pao-main.db /tmp/qt-pao-mount.db
 *   QT_FIXTURE_WII_MAIN=/tmp/qt-pao-main.db QT_FIXTURE_WII_MOUNT=/tmp/qt-pao-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wardrobe-item-images-fixture.ts
 *   QT_FIXTURE_PAO_MAIN=/tmp/qt-pao-main.db QT_FIXTURE_PAO_MOUNT=/tmp/qt-pao-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-photo-album-options.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "cases/photo-album-options\.test\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Case {
  name: string;
  main: string[];
  mount: string[];
}

async function runCase(
  pepper: string,
  c: Case,
  scratch: string,
  mainFixture: string,
  mountFixture: string,
): Promise<Record<string, unknown>> {
  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () =>
    jest.requireActual('@/lib/repositories/factory'),
  );
  jest.doMock('@/lib/startup/startup-state', () => {
    const actual = jest.requireActual('@/lib/startup/startup-state');
    return {
      __esModule: true,
      ...actual,
      startupState: {
        ...actual.startupState,
        isReady: () => true,
        waitForReady: async () => true,
        isPepperResolved: () => true,
        getPepperState: () => 'resolved',
        getPhase: () => 'ready',
        isLockedMode: () => false,
      },
    };
  });

  const work = mkdtempSync(join(scratch, 'pao-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);
  process.env.ENCRYPTION_MASTER_PEPPER = pepper;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = join(work, 'llm-logs.db');
  // DEBUG so the function's lines reach the capture; the console transport is
  // spliced out below, so nothing reaches stdout.
  process.env.LOG_LEVEL = 'debug';

  const { logger } = await import('@/lib/logger');
  const logs: Array<{ level: string; message: string; fields: Array<[string, unknown]> }> = [];
  let armed = false;
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      if (!armed) return;
      const data = d as {
        level: string;
        message: string;
        context?: Record<string, unknown>;
        error?: { message: string };
      };
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      if (data.error) fields.error = data.error.message;
      logs.push({ level: data.level, message: data.message, fields: Object.entries(fields) });
    },
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { listAllPhotoAlbumOptions } = await import('@/lib/photos/photo-album-options');

  await initializeDatabase();
  const raw = getRawDatabase();
  const midb = getRawMountIndexDatabase();
  if (!raw || !midb) throw new Error('DB handles unavailable');
  for (const sql of c.main) raw.exec(sql);
  for (const sql of c.mount) midb.exec(sql);

  try {
    armed = true;
    const albums = await listAllPhotoAlbumOptions(getRepositories());
    armed = false;
    return { name: c.name, albums: JSON.parse(JSON.stringify(albums)), logs };
  } finally {
    armed = false;
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const base = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'wardrobe-item-images-tier2.json'), 'utf8'),
  ) as { testPepperBase64: string };
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'photo-album-options.json'), 'utf8'),
  ) as { cases: Case[] };
  const mainFixture = process.env.QT_FIXTURE_PAO_MAIN;
  const mountFixture = process.env.QT_FIXTURE_PAO_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_PAO_MAIN and QT_FIXTURE_PAO_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-pao-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;

  const out: string[] = [];
  for (const c of spec.cases) {
    out.push(
      JSON.stringify(await runCase(base.testPepperBase64, c, scratch, mainFixture, mountFixture)),
    );
  }
  fs.writeFileSync(outPath, out.join('\n') + '\n');
  process.stderr.write(`photo-album-options oracle wrote ${outPath} (${out.length} cases)\n`);
}

test('photo-album-options oracle', async () => {
  await main();
});

const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
