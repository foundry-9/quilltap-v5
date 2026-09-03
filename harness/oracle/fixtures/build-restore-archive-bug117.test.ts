/**
 * @jest-environment node
 *
 * P4.D152 restore-fixture builder — the ONE archive bug 117 needs.
 *
 * ── WHY A THIRD BUILDER FILE ─────────────────────────────────────────────────
 * `build-restore-archives.test.ts` writes five committed archives and
 * `build-restore-archives-dedupe.test.ts` two more; every archive carries fresh
 * `createdAt`/manifest stamps, so re-running either would rewrite archives this
 * lane has no business moving. This file writes ONLY
 * `restore-archive-bug117.zip` and never opens the others. Its helper block is
 * copied verbatim from the dedupe builder so the two cannot drift apart in how
 * they boot v4.
 *
 * ── WHAT IT IS FOR ───────────────────────────────────────────────────────────
 * v4 `0b0617fee` taught restore to record the bytes it actually stored:
 *
 *   - the REPLAY branch takes `sha256` from the bridge, not from the archive;
 *   - the CARRIED-STORE-ROWS branch, which skips the replay and never sees a
 *     bridge, resolves the ARCHIVED `doc_mount_blobs.sha256` by the parsed blob
 *     id.
 *
 * Neither is reachable against the seven existing archives: their `files` rows
 * all carry a `sha256` that already agrees with the bytes, so a restore that
 * copies the archive's value and one that asks the bridge write the SAME row and
 * the arm is vacuous. This archive carries the damage bug 117 describes — a
 * `files.sha256` naming bytes that exist nowhere — on BOTH branches:
 *
 *   `portrait.png`   the fixture's legacy DISK-key row, its `sha256` rewritten
 *                    to a deliberate non-matching value. The restore replays it
 *                    through the bridge, so post-fix the row records the
 *                    bridge's hash and pre-fix it re-records the lie.
 *   `plate.png`      a REAL PNG uploaded through v4's REAL uploads bridge, which
 *                    transcodes it to WebP — then its `files.sha256` is rewritten
 *                    to the PRE-TRANSCODE hash, which is exactly what a
 *                    pre-4.9.0 `uploadChatFile` wrote. The archive carries the
 *                    blob, so the restore takes the carried branch and must read
 *                    the archived blob's own hash.
 *
 * Both files' STORED bytes come back byte-identical on either side (the carried
 * branch copies the archive's blob; the replay's bytes are text-shaped and no
 * codec can decode them), so the two engines' restored state stays comparable
 * row for row and no new comparand is needed — `files.sha256` is already in the
 * state dump.
 *
 * Produced by v4's **REAL** `createBackup`, like the other seven. The uploads
 * mount is minted by v4's own provisioning migration, so its id is a fresh UUID
 * on every regeneration; nothing may hard-code it.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-restore-archive-bug117
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/fixtures/build-restore-archive-bug117.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/system-data.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$V5W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *   QT_ARCHIVE_OUT=$V5W/crates/quilltap-web/tests/fixtures/restore-archives \
 *     $N/npx jest --silent --watchman=false --testTimeout=600000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- build-restore-archive-bug117
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

/**
 * The store-backed upload: a real 1x1 PNG, the smallest input sharp will decode
 * and re-encode. Its blob is stored as WebP, so the input hash and the stored
 * hash genuinely differ — which is the whole point.
 */
const PLATE_FILE_ID = '11700000-aaaa-4bbb-8ccc-dddddddddd01';
const PLATE_NAME = 'plate.png';
const PLATE_BYTES = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
  'base64',
);

/**
 * The lie. Not a hash of anything — a fixed sentinel, so a restore that copies
 * the archive's value writes a row that names bytes which exist nowhere, exactly
 * as bug 117's own rows do. Using a literal rather than a real-but-wrong hash
 * makes the failure legible in a diff.
 */
const PRE_TRANSCODE_LIE = 'b117b117b117b117b117b117b117b117b117b117b117b117b117b117b117b117';

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

  const scratchRoot = mkdtempSync(join(tmpdir(), 'qt-bug117-archive-'));
  try {
    const work = mkdtempSync(join(scratchRoot, 'gen1-'));
    mkdirSync(join(work, 'data'), { recursive: true });
    copyFileSync(fixtures.main, join(work, 'quilltap.db'));
    copyFileSync(fixtures.mount, join(work, 'data', 'quilltap-mount-index.db'));
    copyFileSync(fixtures.llm, join(work, 'quilltap-llm-logs.db'));
    const imgDest = join(work, 'files', ...IMAGE_STORAGE_KEY(spec.userId).split('/'));
    mkdirSync(dirname(imgDest), { recursive: true });
    writeFileSync(imgDest, IMAGE_BYTES);

    await openInstance(work, spec.userId);
    try {
      await provisionBuiltinMounts();

      // The CARRIED arm: a real PNG through v4's REAL bridge, which transcodes
      // it to WebP. `written.sha256` is the stored (WebP) hash; the row is
      // deliberately given the lie instead.
      const { writeUserUploadToMountStore } = await import(
        '@/lib/file-storage/user-uploads-bridge'
      );
      const written = await writeUserUploadToMountStore({
        filename: PLATE_NAME,
        content: PLATE_BYTES,
        contentType: 'image/png',
        subfolder: 'uploads',
      });
      if (written.storedMimeType !== 'image/webp') {
        throw new Error(
          `the plate must transcode (storedMimeType=${written.storedMimeType}) or the carried arm is vacuous`,
        );
      }
      if (written.sha256 === createHash('sha256').update(PLATE_BYTES).digest('hex')) {
        throw new Error('stored hash equals the input hash — the transcode did nothing');
      }

      const { getRepositories } = await import('@/lib/repositories/factory');
      await (getRepositories() as any).files.create(
        {
          userId: spec.userId,
          originalFilename: PLATE_NAME,
          mimeType: written.storedMimeType,
          size: written.sizeBytes,
          sha256: PRE_TRANSCODE_LIE,
          source: 'UPLOADED',
          category: 'IMAGE',
          storageKey: written.storageKey,
          projectId: null,
          folderPath: null,
          linkedTo: [],
          tags: [],
        },
        { id: PLATE_FILE_ID },
      );

      // The REPLAY arm: the fixture's own legacy DISK-key row, its sha256
      // rewritten to the same lie. Its bytes are text-shaped, so no codec can
      // decode them and the replay stores them unchanged on either side —
      // which keeps the restored state comparable while the recorded hash
      // stays a real discriminator.
      const { rawQuery } = await import('@/lib/database/manager');
      const updated = (await rawQuery(
        'SELECT "id" FROM "files" WHERE "storageKey" = ?',
        [IMAGE_STORAGE_KEY(spec.userId)],
      )) as Array<{ id: string }>;
      if (updated.length !== 1) {
        throw new Error(
          `expected exactly one disk-key file row to poison, found ${updated.length}`,
        );
      }
      await rawQuery('UPDATE "files" SET "sha256" = ? WHERE "storageKey" = ?', [
        PRE_TRANSCODE_LIE,
        IMAGE_STORAGE_KEY(spec.userId),
      ]);

      // ── Trim to the question ────────────────────────────────────────────
      //
      // The committed `system-data-*` fixture has grown since the dedupe
      // archives were built, and the extra content it now carries drags two
      // v4-side behaviours into this archive that have nothing to do with bug
      // 117: v4's restore REFUSES one memory whose `embedding` column is an
      // object where its Zod union wants a Float32Array/array/Buffer/string,
      // and the archived `restored` folder collides with the one v4's own
      // replay creates at 22a-bis (v4's knowingly-kept residual,
      // `found-bugs.md:385-397`). Both are real and both are somebody else's
      // lane; carrying them here would make this archive a differential about
      // three things at once. So the memories go, and so does the scaffolded
      // `restored` folder that nothing in this archive uses.
      await rawQuery('DELETE FROM "memories"');
      // The project-bound `atlas-plates.bin` is the fixture's only store-backed
      // file inside a PROJECT store, and it lands squarely on the standing ruled
      // divergence at `restore/orchestrator.rs` (v5's carried-store-rows skip vs
      // v4's unconditional re-ingest) — v4 mints a second blob for it, v5 reuses
      // the archived one. That divergence is asserted elsewhere and is not this
      // archive's question; its `files` row goes, and the store rows it pointed
      // at stay as ordinary document-store content.
      await rawQuery('DELETE FROM "files" WHERE "projectId" IS NOT NULL');
      const mi = (await import('@/lib/database/backends/sqlite/mount-index-client'))
        .getRawMountIndexDatabase();
      if (!mi) throw new Error('mount-index handle unavailable while trimming');
      mi.prepare("DELETE FROM doc_mount_folders WHERE name = 'restored'").run();

      const { createBackup } = await import('@/lib/backup/backup-service');
      const { zipPath } = await createBackup(spec.userId);
      copyFileSync(zipPath, join(outDir, 'restore-archive-bug117.zip'));
    } finally {
      await closeInstance();
    }

    const f = 'restore-archive-bug117.zip';
    process.stderr.write(`  ${f}  ${fs.statSync(join(outDir, f)).size} bytes\n`);
  } finally {
    rmSync(scratchRoot, { recursive: true, force: true });
  }
}

test('build the bug-117 restore archive', async () => {
  await main();
});
