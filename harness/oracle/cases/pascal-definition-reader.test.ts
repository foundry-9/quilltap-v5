/**
 * @jest-environment node
 *
 * P4.D30 ORACLE (v4 `83118077`): drives v4's REAL `loadToolsFromMount`
 * (`lib/pascal/custom-tools.ts`) over a FRESH copy of the committed
 * `pascal-run-custom-{main,mount}.db` fixture, one row per store.
 *
 * ## Why this family exists alongside `pascal-custom-tools-discovery`
 *
 * That one mocks the store reads on purpose — it is the shadowing/cap corpus.
 * This one mocks NOTHING below the roster: the definition bytes come out of a
 * real encrypted mount index, through the real storage dispatch, which is
 * exactly the layer `83118077` moved. It is the only place the drift's two
 * reachable effects are observable:
 *
 *  - `Tools/beacon.tool.json` in CHAR_C's vault is a database BLOB (written by
 *    the fixture builder through the real `storeMountFile` with
 *    `treatNativeTextAsDocument: false`, the way every file-storage bridge
 *    writes). The old hand-rolled `readDatabaseDocument` dispatch found no
 *    document row and skipped it silently; the canonical reader loads it.
 *  - `Tools/mangled.tool.json` is the same shape with one invalid UTF-8 byte in
 *    its description. `bytes.toString('utf-8')` substitutes U+FFFD rather than
 *    throwing, so the definition still loads — the decode contract the Rust side
 *    has to match.
 *
 * The third effect (the SOURCE_NOT_FOUND race skip) is not deterministically
 * stageable from a committed fixture; both sides pin it with a unit test
 * instead — v4 with a jest stub, v5 in `pascal::roster::read_tool_file_tests`.
 *
 * Run (v4 @ ff12f491, Node 24 — mirror the case file to /tmp; jest ignores
 * `.claude/`; run from a worktree PINNED at the baseline):
 *   M=/tmp/qt-pascal-reader-mirror; mkdir -p $M/cases $M/fixtures
 *   cp <V5W>/harness/oracle/cases/pascal-definition-reader.test.ts $M/cases/
 *   cp <V5W>/harness/oracle/fixtures/pascal-run-custom.json $M/fixtures/
 *   cd /private/tmp/qt-v4-pin-p4d30-ff12f491
 *   QT_FIXTURE_PASCAL_MAIN=<V5W>/crates/quilltap-web/tests/fixtures/pascal-run-custom-main.db \
 *   QT_FIXTURE_PASCAL_MOUNT=<V5W>/crates/quilltap-web/tests/fixtures/pascal-run-custom-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-pascal-definition-reader.ndjson \
 *     npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$M/cases" -- pascal-definition-reader
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

interface Meta {
  vaultA: string;
  vaultB: string;
  vaultC: string;
  vaultD: string;
}

/** The General store — a fixed id in the builder, not a minted one. */
const GENERAL_MP = '93000000-0000-4000-8000-0000000000aa';

type Tier = 'character' | 'participant' | 'group' | 'project' | 'global';

async function runCase(
  name: string,
  mountPointId: string,
  tier: Tier,
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

  const work = mkdtempSync(join(scratch, 'rd-'));
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
    const { loadToolsFromMount } = await import('@/lib/pascal/custom-tools');
    const { found, errors } = await loadToolsFromMount(mountPointId, tier);
    return { name, found, errors };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'pascal-run-custom.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_PASCAL_MAIN ?? '',
    mount: process.env.QT_FIXTURE_PASCAL_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const meta = JSON.parse(fs.readFileSync(`${fixtures.main}.meta.json`, 'utf8')) as Meta;

  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-pascal-reader-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: Array<[string, string, Tier]> = [
    // Document-stored definitions only — the regression arm: routing through the
    // canonical reader must not move a single byte here.
    ['vault-a-documents', meta.vaultA, 'character'],
    ['vault-b-documents', meta.vaultB, 'participant'],
    // The drift's arm: two BLOB-stored definitions beside a document-stored one.
    ['vault-c-blobs', meta.vaultC, 'participant'],
    // A provisioned vault whose `properties.json` keystone was deleted — the
    // store still lists and reads its `Tools/`; nothing here depends on the
    // overlay keystone.
    ['vault-d-broken-overlay', meta.vaultD, 'character'],
    // The General store, carrying the two availability-gated definitions.
    ['general-store', GENERAL_MP, 'global'],
    // A store id that does not exist: the early return, no throw.
    ['missing-mount', '00000000-0000-4000-8000-00000000dead', 'global'],
  ];

  const out = fs.createWriteStream(outPath);
  for (const [name, mountPointId, tier] of cases) {
    const row = await runCase(name, mountPointId, tier, scratch, fixtures);
    out.write(JSON.stringify(row) + '\n');
  }
  out.end();
  await new Promise((r) => out.on('finish', r));
  rmSync(scratch, { recursive: true, force: true });
}

it('emits the definition-reader corpus', async () => {
  await main();
}, 120000);
