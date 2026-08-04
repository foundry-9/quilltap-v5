/**
 * @jest-environment node
 *
 * P4.28 restore-fixture builder — the archive that carries ORPHANED doc-store
 * rows (dogfood finding #58).
 *
 * ── WHY A THIRD BUILDER FILE ────────────────────────────────────────────────
 * `build-restore-archives.test.ts` writes the five original committed archives
 * and `build-restore-archives-dedupe.test.ts` the P4.d23 pair; re-running either
 * rewrites archives whose oracles this lane has no business moving (every
 * archive carries fresh `createdAt`/manifest stamps). This file writes exactly
 * ONE archive and opens no other.
 *
 * ── WHAT IT REPRODUCES ──────────────────────────────────────────────────────
 * Measured on the real dogfood instance, 2026-08-03: `doc_mount_file_links`
 * holds **43 rows whose `mountPointId` matches no `doc_mount_points` row**, and
 * `doc_mount_folders` holds **118**. Their `relativePath`s are
 * `physical-prompts.json` (21), `physical-description.md` (21) and
 * `images/avatar.webp` (1) across 21 vanished character-vault mount points —
 * i.e. exactly the "~50 failures over three repeating names" the walk reported.
 * `fileId` orphans: zero, so the failing constraint is `mountPointId`.
 *
 * The chain is: a document store is deleted without its links and folders going
 * with it. Nothing notices on a live instance — read connections do not enable
 * foreign keys, and a modern `doc_mount_file_links` (generateDDL) declares none
 * at all. Backup then dumps the table with a raw `SELECT *`, orphans included,
 * and restore inserts them into a target whose constraints ARE live (v5 opens
 * writable connections with `PRAGMA foreign_keys = ON`, and a
 * migration-vintage link table carries `fileId` → `doc_mount_files` and
 * `mountPointId` → `doc_mount_points`). Every orphan then fails with a bare
 * `FOREIGN KEY constraint failed`.
 *
 * This archive reproduces that in miniature: the committed `system-data-*`
 * fixture with ONE character vault's mount point deleted and its 9 links + 7
 * folders (and their 4 chunks, of the archive's 28 links total) left behind,
 * backed up by v4's **REAL** `createBackup`. The deletion
 * is a plain `DELETE` on the mount-index connection — which is precisely how the
 * real ones arose, since neither app's delete path is running with the
 * constraint on.
 *
 * Pairs with `crates/quilltap-web/tests/fixtures/migration-vintage/` (the
 * FK-bearing target) in `restore_vintage_equivalence`.
 *
 * Run (Node 24, from the PINNED v4 worktree — cp to a /tmp mirror; jest ignores
 * .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   PIN=/private/tmp/qt-v4-pin-p4.28-c4d4b0de
 *   TMPO=/tmp/qt-restore-archive-orphans
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/fixtures/build-restore-archive-orphan-links.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/system-data.json" "$TMPO/fixtures/"
 *   cd "$PIN"
 *   QT_FIXTURE_SD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$V5W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *   QT_ARCHIVE_OUT=$V5W/crates/quilltap-web/tests/fixtures/restore-archives \
 *     $N/npx jest --silent --watchman=false --testTimeout=600000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "build-restore-archive-orphan-links\.test\.ts$"
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

/**
 * The vault whose mount point is removed. Pinned by NAME rather than id so a
 * fixture rebuild (which mints fresh vault ids) cannot silently orphan nothing.
 */
const ORPHANED_STORE_NAME = 'Riya Character Vault';

function applyMocks(userId: string): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/file-storage/manager', () => jest.requireActual('@/lib/file-storage/manager'));
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

async function openInstance(work: string, userId: string): Promise<void> {
  jest.resetModules();
  applyMocks(userId);
  mkdirSync(join(work, 'data'), { recursive: true });
  process.env.SQLITE_PATH = join(work, 'quilltap.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = join(work, 'data', 'quilltap-mount-index.db');
  process.env.SQLITE_LLM_LOGS_PATH = join(work, 'quilltap-llm-logs.db');
  process.env.QUILLTAP_DATA_DIR = work;
  delete process.env.SQLITE_WAL_MODE;
  const { initializeDatabase } = await import('@/lib/database/manager');
  await initializeDatabase();
}

async function closeInstance(): Promise<void> {
  const { closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  await closeDatabase();
  closeMountIndexSQLiteClient();
  closeLLMLogsSQLiteClient();
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
  process.env.LOG_LEVEL = 'error';

  const scratchRoot = mkdtempSync(join(tmpdir(), 'qt-orphan-archive-'));
  try {
    const work = mkdtempSync(join(scratchRoot, 'src-'));
    mkdirSync(join(work, 'data'), { recursive: true });
    copyFileSync(fixtures.main, join(work, 'quilltap.db'));
    copyFileSync(fixtures.mount, join(work, 'data', 'quilltap-mount-index.db'));
    copyFileSync(fixtures.llm, join(work, 'quilltap-llm-logs.db'));

    await openInstance(work, spec.userId);
    try {
      const { getRawMountIndexDatabase } = await import(
        '@/lib/database/backends/sqlite/mount-index-client'
      );
      const midb = getRawMountIndexDatabase();
      if (!midb) throw new Error('mount-index DB handle unavailable');

      const row = midb
        .prepare(`SELECT "id" FROM "doc_mount_points" WHERE "name" = ?`)
        .get(ORPHANED_STORE_NAME) as { id: string } | undefined;
      if (!row?.id) throw new Error(`no mount point named ${ORPHANED_STORE_NAME}`);

      const links = (
        midb
          .prepare(`SELECT COUNT(*) AS n FROM "doc_mount_file_links" WHERE "mountPointId" = ?`)
          .get(row.id) as { n: number }
      ).n;
      const folders = (
        midb
          .prepare(`SELECT COUNT(*) AS n FROM "doc_mount_folders" WHERE "mountPointId" = ?`)
          .get(row.id) as { n: number }
      ).n;
      if (links === 0 || folders === 0) {
        throw new Error(
          `${ORPHANED_STORE_NAME} must have both links and folders to orphan ` +
            `(links=${links}, folders=${folders})`,
        );
      }

      // The store row alone. Its links, folders, files, documents and blobs all
      // stay — which is the observed real-instance state, and is what a delete
      // running without `PRAGMA foreign_keys = ON` leaves behind.
      midb.prepare(`DELETE FROM "doc_mount_points" WHERE "id" = ?`).run(row.id);

      const { createBackup } = await import('@/lib/backup/backup-service');
      const { zipPath } = await createBackup(spec.userId);
      copyFileSync(zipPath, join(outDir, 'restore-archive-orphan-links.zip'));
      process.stderr.write(
        `  orphaned "${ORPHANED_STORE_NAME}" (${row.id}): ` +
          `${links} link(s), ${folders} folder(s) left behind\n`,
      );
    } finally {
      await closeInstance();
    }
  } finally {
    rmSync(scratchRoot, { recursive: true, force: true });
  }
}

it('builds the orphaned-doc-store-rows restore archive', async () => {
  await main();
}, 600000);
