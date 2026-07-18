/**
 * @jest-environment node
 *
 * P4.6ay unit 12 ORACLE (part A): drives v4's REAL `lib/pascal/workbench.ts`
 * (`buildCustomToolLibrary` + `listCustomToolDestinations`) over a FRESH copy of
 * the committed `workbench-{main,mount}.db` fixture per case. Diffed against the
 * Rust port (`pascal::workbench`).
 *
 * v4's own `__tests__/unit/lib/pascal/workbench.test.ts` is the case template,
 * but it primes a MOCKED repository world; this oracle drives the real repos
 * over a real baked instance, so the repo composition (findAllRaw vs the
 * hydrating overlay, findEnabled's disabled filter, the group/project link
 * tables) is proven too — the fixture carries a deliberately BROKEN character
 * vault for exactly that reason.
 *
 * Run (Node 24, from the v4 checkout — mirror the case file to /tmp; jest
 * ignores `.claude/`):
 *   QT_FIXTURE_WORKBENCH_MAIN=<v5>/crates/quilltap-web/tests/fixtures/workbench-main.db \
 *   QT_FIXTURE_WORKBENCH_MOUNT=<v5>/crates/quilltap-web/tests/fixtures/workbench-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-pascal-workbench.ndjson \
 *     npx jest --silent --roots "$PWD" --roots "$M/cases" -- pascal-workbench
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

type CaseName = 'library' | 'destinations';

async function runCase(
  spec: Spec,
  name: CaseName,
  scratch: string,
  fixtures: { main: string; mount: string },
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
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
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

  const work = mkdtempSync(join(scratch, 'wb-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  await initializeDatabase();

  try {
    const { buildCustomToolLibrary, listCustomToolDestinations } = await import(
      '@/lib/pascal/workbench'
    );
    const value =
      name === 'library' ? await buildCustomToolLibrary() : await listCustomToolDestinations();
    return { name, value };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'workbench.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_WORKBENCH_MAIN ?? '',
    mount: process.env.QT_FIXTURE_WORKBENCH_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-workbench-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const out = fs.createWriteStream(outPath);
  for (const name of ['library', 'destinations'] as CaseName[]) {
    const row = await runCase(spec, name, scratch, fixtures);
    out.write(JSON.stringify(row) + '\n');
  }
  out.end();
  await new Promise((r) => out.on('finish', r));
  rmSync(scratch, { recursive: true, force: true });
}

it('emits the pascal workbench oracle', async () => {
  await main();
}, 120000);
