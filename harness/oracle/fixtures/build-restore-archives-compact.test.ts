/**
 * @jest-environment node
 *
 * P4.D46 restore-fixture builder — the ONE archive the compact-backup restore
 * tail needs (`7189a968`).
 *
 * ── WHY A FOURTH BUILDER FILE ────────────────────────────────────────────────
 * `build-restore-archives.test.ts` writes the five original committed archives,
 * `build-restore-archives-dedupe.test.ts` the two P4.d23 ones, and
 * `build-restore-archives-memory-graph.test.ts` the P4.D31 one. Re-running any
 * of them rewrites its whole set (fresh `createdAt` / manifest stamps),
 * invalidating oracles this lane has no business moving. This file writes ONLY
 * `restore-archive-compact.zip` and never opens the other nine. Additive,
 * exactly like its predecessors.
 *
 * ── WHY A NEW ARCHIVE AT ALL ─────────────────────────────────────────────────
 * v4 `7189a968` added the opt-in compact backup: memory embeddings nulled, the
 * six derived embedding collections OMITTED from staging entirely (absent ≡
 * empty to the optional readers; absent is what shrinks the archive), and
 * `manifest.compact: true` (the key omitted when false). Restore then runs step
 * 24a — gated on that manifest flag, a full `EMBEDDING_REINDEX_ALL` enqueued
 * BEFORE the reconcile so the reconcile's dedupe sees it — and step 25, the
 * unconditional post-restore `reconcileEmbeddingDimensions()` whose result
 * lands in `RestoreSummary.embeddingReconcile`. None of the nine committed
 * archives is compact, so none can reach 24a at all.
 *
 * The archive is built by v4's **REAL** `createBackup(userId, {compact: true})`
 * over a copy of the committed `system-data-*` fixture family — which carries
 * an embedding-BEARING memory (MEM_3), a conversation chunk with a real
 * vector, a vector index + entry, embedding status, and a TF-IDF vocabulary
 * row, so the compact strip is a measurement on every one of the six omitted
 * collections plus the nulled memory vector. The restore claim must never
 * depend on v5's zip writer.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-restore-archives-compact
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/fixtures/build-restore-archives-compact.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/system-data.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$V5W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *   QT_ARCHIVE_OUT=$V5W/crates/quilltap-web/tests/fixtures/restore-archives \
 *     $N/npx jest --silent --watchman=false --testTimeout=600000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- build-restore-archives-compact
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

/** The same `files/` seed the base archive carries, so the file phase behaves
 *  identically to `restore-archive.zip` and this archive differs from the full
 *  ones in exactly one respect: compactness. */
const IMAGE_STORAGE_KEY = (userId: string) => `${userId}/portrait.png`;
const IMAGE_BYTES = Buffer.from('quilltap-fixture-portrait-bytes\n', 'utf8');

function applyMocks(userId: string): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/file-storage/manager', () =>
    jest.requireActual('@/lib/file-storage/manager'),
  );
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: userId } }),
  }));
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
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'system-data.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_SD_MAIN ?? '',
    mount: process.env.QT_FIXTURE_SD_MOUNT ?? '',
    llm: process.env.QT_FIXTURE_SD_LLM ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outDir = process.env.QT_ARCHIVE_OUT;
  if (!outDir) throw new Error('QT_ARCHIVE_OUT must point at the fixture directory to write');
  mkdirSync(outDir, { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const scratchRoot = mkdtempSync(join(tmpdir(), 'qt-compact-archive-'));
  jest.resetModules();
  applyMocks(spec.userId);

  const work = mkdtempSync(join(scratchRoot, 'bk-'));
  mkdirSync(join(work, 'data'), { recursive: true });
  copyFileSync(fixtures.main, join(work, 'main.db'));
  copyFileSync(fixtures.mount, join(work, 'mount.db'));
  copyFileSync(fixtures.llm, join(work, 'llm.db'));
  process.env.SQLITE_PATH = join(work, 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = join(work, 'mount.db');
  process.env.SQLITE_LLM_LOGS_PATH = join(work, 'llm.db');
  process.env.QUILLTAP_DATA_DIR = work;

  const imgDest = join(work, 'files', ...IMAGE_STORAGE_KEY(spec.userId).split('/'));
  mkdirSync(dirname(imgDest), { recursive: true });
  writeFileSync(imgDest, IMAGE_BYTES);

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  await initializeDatabase();

  try {
    const { createBackup } = await import('@/lib/backup/backup-service');
    const { zipPath, manifest } = await createBackup(spec.userId, { compact: true });
    if ((manifest as { compact?: unknown }).compact !== true) {
      throw new Error(
        'the v4 tree here does not stamp manifest.compact — this builder must ' +
          'run against a tree carrying 7189a968',
      );
    }
    const out = join(outDir, 'restore-archive-compact.zip');
    copyFileSync(zipPath, out);
    process.stderr.write(`  restore-archive-compact.zip  ${fs.statSync(out).size} bytes\n`);
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    closeLLMLogsSQLiteClient();
    rmSync(scratchRoot, { recursive: true, force: true });
  }
}

test('build the compact restore archive', async () => {
  await main();
});
