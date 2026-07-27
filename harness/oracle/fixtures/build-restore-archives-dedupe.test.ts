/**
 * @jest-environment node
 *
 * P4.d23 restore-fixture builder — the TWO archives the file-replay dedupe needs.
 *
 * ── WHY A SECOND BUILDER FILE ────────────────────────────────────────────────
 * `build-restore-archives.test.ts` writes the five committed archives the
 * P4.9G5 family already depends on. Re-running it would rewrite all five (every
 * archive carries fresh `createdAt`/manifest stamps), invalidating oracles this
 * lane has no business moving. This file therefore writes ONLY the two new
 * archives and never opens the other five.
 *
 * ── WHAT THEY ARE FOR ────────────────────────────────────────────────────────
 * Neither hazard the phase-order ruling names is reachable against the existing
 * five, for two separate reasons:
 *
 *   1. No committed archive has a user file stored IN a document store — the
 *      one `files` row carries the legacy disk key `<userId>/portrait.png`, so
 *      the archive never carries the store rows for it and there is nothing for
 *      a replay to collide with.
 *   2. None carries a Quilltap Uploads mount or a `userUploadsMountPointId`, so
 *      in `replace` mode NEITHER engine restores a user file at all (the pointer
 *      dangles and both warn identically). The disaster-recovery case is
 *      unexercised.
 *
 *   restore-archive-uploads.zip  a FIRST-generation archive from an instance
 *                                with a provisioned Quilltap Uploads mount and
 *                                a file uploaded through v4's REAL bridge, so
 *                                its `files` row carries a
 *                                `mount-blob:<mp>:<blob>` storage key AND the
 *                                archive carries that blob's store rows
 *                                (`uploads/ledger.txt`).
 *   restore-archive-gen2.zip     a SECOND-generation archive: the uploads
 *                                archive restored by v4's REAL restore into a
 *                                fresh instance whose uploads pointer was
 *                                aligned to the archive's mount (what disaster
 *                                recovery looks like), then backed up again. Its
 *                                file therefore also sits at `restored/…`, which
 *                                is exactly where a replay wants to write.
 *
 * Both are produced by v4's **REAL** `createBackup`, like the other five — the
 * restore claim must never depend on v5's zip writer. The uploads archive starts
 * from the committed `system-data-*` fixture family (test-pepper synthetic); the
 * fixture files themselves are only ever COPIED, never written.
 *
 * The Quilltap Uploads mount is minted by v4's own provisioning migration rather
 * than hand-inserted, so its id is a fresh UUID on every regeneration. That is
 * deliberate: nothing may hard-code it. The differential reads it out of the
 * archive.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-restore-archives-dedupe
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/fixtures/build-restore-archives-dedupe.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/system-data.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$V5W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *   QT_ARCHIVE_OUT=$V5W/crates/quilltap-web/tests/fixtures/restore-archives \
 *     $N/npx jest --silent --watchman=false --testTimeout=600000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- build-restore-archives-dedupe
 */

import * as fs from 'fs';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

/** The target user a `restore` retargets everything to — the oracle's constant. */
const SINGLE_USER_ID = 'ffffffff-ffff-ffff-ffff-ffffffffffff';

const IMAGE_STORAGE_KEY = (userId: string) => `${userId}/portrait.png`;
const IMAGE_BYTES = Buffer.from('quilltap-fixture-portrait-bytes\n', 'utf8');

/** The store-backed upload. Plain text, so no bitmap transcode is in play. */
const LEDGER_FILE_ID = '99999999-aaaa-4bbb-8ccc-dddddddddddd';
const LEDGER_NAME = 'ledger.txt';
const LEDGER_BYTES = Buffer.from('the ledger of the Aetheric Company\n', 'utf8');

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
  jest.doMock('@/lib/file-storage/user-uploads-bridge', () =>
    jest.requireActual('@/lib/file-storage/user-uploads-bridge'),
  );
  jest.doMock('@/lib/file-storage/project-store-bridge', () =>
    jest.requireActual('@/lib/file-storage/project-store-bridge'),
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

/** Point every database env var at `work` and boot v4's real manager. */
async function openInstance(work: string, userId: string): Promise<void> {
  jest.resetModules();
  applyMocks(userId);
  mkdirSync(join(work, 'data'), { recursive: true });
  process.env.SQLITE_PATH = join(work, 'quilltap.db');
  // The mount-index must sit where BOTH the manager env var and
  // `getMountIndexDatabasePath()` resolve, or the provisioning migrations write
  // a different file than the manager reads (the provision oracle's note).
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

/** The three built-in mount stores, minted by v4's own provisioning migrations. */
async function provisionBuiltinMounts(): Promise<void> {
  const { provisionLanternBackgroundsMountMigration } = await import(
    '@/migrations/scripts/provision-lantern-backgrounds-mount'
  );
  const { provisionUserUploadsMountMigration } = await import(
    '@/migrations/scripts/provision-user-uploads-mount'
  );
  const { provisionGeneralMountMigration } = await import(
    '@/migrations/scripts/provision-general-mount'
  );
  await provisionLanternBackgroundsMountMigration.run();
  await provisionUserUploadsMountMigration.run();
  await provisionGeneralMountMigration.run();
}

/**
 * A fresh v4 instance, exactly the way `system-restore.test.ts` builds the
 * restore target: every lazily-created table touched, then the deterministic
 * first-boot seed and the built-in templates and mounts.
 */
async function seedFreshInstance(): Promise<void> {
  const { rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  const repos = getRepositories() as Record<string, unknown>;
  for (const [key, repo] of Object.entries(repos)) {
    if (key === 'wardrobe') continue;
    const r = repo as { count?: () => Promise<number>; findAll?: () => Promise<unknown[]> };
    try {
      if (typeof r.count === 'function') await r.count();
      else if (typeof r.findAll === 'function') await r.findAll();
    } catch {
      /* vault-only / no table — not part of a fresh schema */
    }
  }
  const anyRepos = repos as Record<string, any>;
  const zero = '00000000-0000-0000-0000-000000000000';
  await anyRepos.chats?.getMessageCount(zero);
  await anyRepos.connections?.getApiKeysByUserId(zero);
  await anyRepos.vectorIndices?.findMetaByCharacterId(zero);
  await anyRepos.docMountBlobs?.findByFileId(zero);

  const { getOrCreateSingleUser } = await import('@/lib/auth/single-user');
  await getOrCreateSingleUser();
  await getOrCreateSingleUser();
  const { getSeedEmbeddingProfiles, prepareSeedEmbeddingProfile } = await import('@/first-startup');
  await anyRepos.embeddingProfiles.create(
    prepareSeedEmbeddingProfile(getSeedEmbeddingProfiles()[0], SINGLE_USER_ID),
  );
  await anyRepos.roleplayTemplates.seedBuiltInTemplates();
  await provisionBuiltinMounts();
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

  const scratchRoot = mkdtempSync(join(tmpdir(), 'qt-dedupe-archives-'));
  let uploadsMountPointId = '';
  try {
    // ── 1. The uploads archive ───────────────────────────────────────────────
    const work = mkdtempSync(join(scratchRoot, 'gen1-'));
    // The mount-index has to land where the manager looks for it; the committed
    // fixture is a plain sibling file, so copy it into `data/`.
    mkdirSync(join(work, 'data'), { recursive: true });
    copyFileSync(fixtures.main, join(work, 'quilltap.db'));
    copyFileSync(fixtures.mount, join(work, 'data', 'quilltap-mount-index.db'));
    copyFileSync(fixtures.llm, join(work, 'quilltap-llm-logs.db'));
    const imgDest = join(work, 'files', ...IMAGE_STORAGE_KEY(spec.userId).split('/'));
    mkdirSync(dirname(imgDest), { recursive: true });
    writeFileSync(imgDest, IMAGE_BYTES);

    await openInstance(work, spec.userId);
    try {
      // The fixture has no Quilltap Uploads mount. Mint one with v4's own
      // provisioning migration (which also scaffolds `uploads/`, `restored/`, …)
      // and then write a real upload through the real bridge.
      await provisionBuiltinMounts();
      const { writeUserUploadToMountStore } = await import(
        '@/lib/file-storage/user-uploads-bridge'
      );
      const written = await writeUserUploadToMountStore({
        filename: LEDGER_NAME,
        content: LEDGER_BYTES,
        contentType: 'text/plain',
        subfolder: 'uploads',
      });
      uploadsMountPointId = written.mountPointId;

      // The `files` row exactly as `app/api/v1/files/shared.ts:186` writes it.
      const { getRepositories } = await import('@/lib/repositories/factory');
      await (getRepositories() as any).files.create(
        {
          userId: spec.userId,
          originalFilename: LEDGER_NAME,
          mimeType: written.storedMimeType,
          size: written.sizeBytes,
          sha256: createHash('sha256').update(LEDGER_BYTES).digest('hex'),
          source: 'UPLOADED',
          category: 'DOCUMENT',
          storageKey: written.storageKey,
          projectId: null,
          folderPath: null,
          linkedTo: [],
          tags: [],
        },
        { id: LEDGER_FILE_ID },
      );

      const { createBackup } = await import('@/lib/backup/backup-service');
      const { zipPath } = await createBackup(spec.userId);
      copyFileSync(zipPath, join(outDir, 'restore-archive-uploads.zip'));
    } finally {
      await closeInstance();
    }

    // ── 2. The second-generation archive ─────────────────────────────────────
    //
    // A fresh instance, its uploads pointer aligned to the archive's mount (the
    // shape disaster recovery has: you are restoring your own backup onto your
    // own instance), then v4's REAL restore, then a backup of the result. The
    // replayed file lands at `restored/ledger.txt` alongside the archived
    // `uploads/ledger.txt`, which is what makes a THIRD restore collide.
    const gen2 = mkdtempSync(join(scratchRoot, 'gen2-'));
    await openInstance(gen2, SINGLE_USER_ID);
    try {
      await seedFreshInstance();
      // Align the uploads pointer with the archive's own mount. `lib/instance-
      // settings` exports no setter for this key (the provisioning migration
      // writes it directly), so use the migration's own raw upsert — a bare id,
      // NOT JSON-encoded.
      const { rawQuery: raw } = await import('@/lib/database/manager');
      await raw(
        'INSERT INTO "instance_settings" ("key", "value") VALUES (?, ?) ' +
          'ON CONFLICT("key") DO UPDATE SET "value" = excluded."value"',
        ['userUploadsMountPointId', uploadsMountPointId],
      );

      const { restore } = await import('@/lib/backup/restore/restore');
      const summary = await restore(join(outDir, 'restore-archive-uploads.zip'), {
        mode: 'replace',
        targetUserId: SINGLE_USER_ID,
      });
      process.stderr.write(
        `  gen2 seed restore: files=${summary.files} warnings=${JSON.stringify(summary.warnings)}\n`,
      );
      // Both of the archive's files replay into the aligned uploads mount under
      // `restored/`: the legacy disk-key `portrait.png` and the store-backed
      // `ledger.txt`. v4 warns here that the ARCHIVED `restored` folder row
      // collided with the one its own replay had just created at `22a-bis` —
      // that is v4's own knowingly-kept residual (`found-bugs.md:385-397`),
      // observed for the first time, and it is part of what makes this archive a
      // faithful second generation.
      if (summary.files !== 2) {
        throw new Error(
          `the gen-2 seed restore must bring back both files (files=${summary.files})`,
        );
      }

      const { createBackup } = await import('@/lib/backup/backup-service');
      const { zipPath } = await createBackup(SINGLE_USER_ID);
      copyFileSync(zipPath, join(outDir, 'restore-archive-gen2.zip'));
    } finally {
      await closeInstance();
    }

    for (const f of ['restore-archive-uploads.zip', 'restore-archive-gen2.zip']) {
      process.stderr.write(`  ${f}  ${fs.statSync(join(outDir, f)).size} bytes\n`);
    }
  } finally {
    rmSync(scratchRoot, { recursive: true, force: true });
  }
}

test('build the restore dedupe archives', async () => {
  await main();
});
