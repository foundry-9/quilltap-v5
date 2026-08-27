/**
 * @jest-environment node
 *
 * P4.43 MEMORY-DEDUP ORACLE: drives v4's REAL `deduplicateAllMemories`
 * (`lib/tools/memory-dedup.ts`) over a FRESH copy of the committed
 * `embedding-profiles-{main,mount}.db` fixture, in BOTH modes:
 *   - `preview` (dryRun=true)  — result JSON only; the memories table is unchanged;
 *   - `apply`   (dryRun=false) — result JSON + the mutated memories table (survivor
 *     content rewritten with `[+]` footnotes, discards deleted, neighbours'
 *     relatedMemoryIds scrubbed through `deleteMemoriesWithUnlinkBatch`).
 *
 * Deterministic — NO LLM, NO jobs. The only seam mocks are `requireActual`
 * pass-throughs so the DB manager, repositories, embedding-service
 * (`cosineSimilarity`), memory-gate (`extractNovelDetails` /
 * `deleteMemoriesWithUnlinkBatch`), vector-store, and the dedup tool itself run
 * v4's REAL code against the REAL fixture DBs. `processedAt` is normalized;
 * per-row `createdAt` / `updatedAt` are omitted from the memories dump (a
 * survivor's `updatedAt` is minted on apply).
 *
 * Run from the v4 checkout under Node 24 (cp to a /tmp mirror; jest ignores
 * .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-dedup-oracle ; rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp $V5W/harness/oracle/cases/memory-dedup.test.ts "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_EP_MGMT_MAIN=$V5W/crates/quilltap-web/tests/fixtures/embedding-profiles-main.db \
 *   QT_EP_MGMT_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/embedding-profiles-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-memory-dedup.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- memory-dedup
 */

import * as fs from 'fs';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const PEPPER = 'ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=';
const USER_ID = 'aa000000-0000-4000-8000-0000000000aa';
const THRESHOLD = 0.8;

function applyMocks(): void {
  const cipherDriverPath = join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  for (const m of [
    '@/lib/database/manager',
    '@/lib/database/repositories',
    '@/lib/repositories/factory',
    '@/lib/database/backends/sqlite/client',
    '@/lib/database/backends/sqlite/mount-index-client',
    '@/lib/database/backends/sqlite/introspection',
    '@/lib/paths',
    '@/lib/tools/memory-dedup',
    // The jest-incomplete `embedding-service` trap: `deduplicateCharacterMemories`
    // calls `cosineSimilarity` from it, which is `undefined` under the default
    // jest mock. Force the real module (see [[p4.9h2a-embedding-profiles-lane]]).
    '@/lib/embedding/embedding-service',
    '@/lib/memory/memory-gate',
    '@/lib/embedding/vector-store',
  ]) {
    jest.doMock(m, () => jest.requireActual(m));
  }
  jest.doMock('@/lib/startup/startup-state', () => {
    const a = jest.requireActual('@/lib/startup/startup-state');
    return {
      __esModule: true,
      ...a,
      startupState: {
        ...a.startupState,
        isReady: () => true,
        waitForReady: async () => true,
        isPepperResolved: () => true,
        getPepperState: () => 'resolved',
        getPhase: () => 'ready',
        isLockedMode: () => false,
      },
    };
  });
}

/** The memories columns the differential compares (dedup-relevant; the minted
 * `updatedAt` and unchanged `createdAt` are omitted). */
const MEM_COLS = [
  'id',
  'characterId',
  'content',
  'summary',
  'importance',
  'embedding',
  'occurredAt',
  'narrativeTime',
  'entities',
  'kind',
  'source',
  'keywords',
  'tags',
  'relatedMemoryIds',
];

async function dumpMemories(): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const main = getRawDatabase() as unknown as {
    prepare: (s: string) => { all: () => Array<Record<string, unknown>> };
  };
  const rows = main.prepare(`SELECT * FROM memories ORDER BY id`).all();
  return rows.map((r) => {
    const out: Record<string, unknown> = {};
    for (const col of MEM_COLS) {
      const v = r[col];
      out[col] = Buffer.isBuffer(v) ? Buffer.from(v).toString('hex') : v ?? null;
    }
    return out;
  });
}

async function runOne(
  mode: 'preview' | 'apply',
  fixtures: { main: string; mount: string },
): Promise<{ mode: string; threshold: number; result: unknown; memories: unknown }> {
  jest.resetModules();
  applyMocks();

  const scratch = mkdtempSync(join(tmpdir(), `qt-dedup-${mode}-`));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'main.db');
  // Place the mount at the derived path AND point SQLITE_MOUNT_INDEX_PATH at it
  // so both the standard client (characters.findAll overlay) and any
  // getMountIndexDatabasePath consumer see it.
  const mountWork = join(scratch, 'data', 'quilltap-mount-index.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { deduplicateAllMemories } = await import('@/lib/tools/memory-dedup');

  await initializeDatabase();
  const result = (await deduplicateAllMemories(USER_ID, THRESHOLD, mode === 'preview')) as Record<
    string,
    unknown
  >;
  const memories = await dumpMemories();
  await closeDatabase();

  // Normalize the one non-deterministic result field.
  result.processedAt = '<processedAt>';

  rmSync(scratch, { recursive: true, force: true });
  return { mode, threshold: THRESHOLD, result, memories };
}

async function main(): Promise<void> {
  const fixtures = {
    main: process.env.QT_EP_MGMT_MAIN ?? '',
    mount: process.env.QT_EP_MGMT_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const preview = await runOne('preview', fixtures);
  const apply = await runOne('apply', fixtures);

  fs.writeFileSync(outPath, JSON.stringify(preview) + '\n' + JSON.stringify(apply) + '\n');
}

it('memory-dedup oracle', async () => {
  await main();
}, 120000);
