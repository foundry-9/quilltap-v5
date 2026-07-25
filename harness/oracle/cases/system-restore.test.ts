/**
 * @jest-environment node
 *
 * P4.9G5 restore ORACLE — drives v4's REAL restore code over the COMMITTED
 * archive family (`crates/quilltap-web/tests/fixtures/restore-archives/`), the
 * same bytes the Rust side reads.
 *
 * ── PART 1: preview (`lib/backup/restore/preview.ts:20`) ─────────────────────
 * `previewRestore(zipPath)` is filesystem-only — it extracts, counts, and
 * cleans up, touching no database. Each case emits either the 41-key
 * `RestoreSummary` or the thrown message, verbatim: the preview route leaks
 * `error.message` to the client (`system/restore/route.ts:176`), so the
 * malformed-archive wording is part of the contract.
 *
 *   preview_full              the whole archive
 *   preview_legacy            + outfit-presets.json and the legacy
 *                             equippedOutfit shape (both parse-time folds)
 *   preview_minimal           every OPTIONAL data file absent — the [] fallbacks
 *   preview_missing_required  data/tags.json absent — `readJsonArrayFile` throws
 *   preview_malformed         no quilltap-backup-* root, no manifest.json
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-sysrestore-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/system-restore.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_RESTORE_ARCHIVES=$V5W/crates/quilltap-web/tests/fixtures/restore-archives \
 *   QT_ORACLE_OUT=/tmp/oracle-system-restore.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- system-restore
 */

import * as fs from 'fs';
import { join, dirname } from 'node:path';
import { mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface PreviewCase {
  name: string;
  archive: string;
}

const PREVIEW_CASES: PreviewCase[] = [
  { name: 'preview_full', archive: 'restore-archive.zip' },
  { name: 'preview_legacy', archive: 'restore-archive-legacy.zip' },
  { name: 'preview_minimal', archive: 'restore-archive-minimal.zip' },
  { name: 'preview_missing_required', archive: 'restore-archive-missing-required.zip' },
  { name: 'preview_malformed', archive: 'restore-archive-malformed.zip' },
];

/**
 * ── PART 2: restore, `mode: 'replace'` (`lib/backup/restore/restore.ts:35`) ──
 *
 * ⚠ **This half has NO Rust consumer yet** — P4.9G5 unit 4 is OPEN. It lands
 * here ahead of the port for two reasons, both recorded in the lane record:
 *
 *  1. It is the **evidence** for two v4 bugs the lane found by running v4's own
 *     restore against v4's own backup of a modern instance. Re-run it and read
 *     `summary.warnings` — every `doc_mount_points` and `doc_mount_file_links`
 *     row is rejected by Zod (`dumpMountIndexTable`, `backup-service.ts:72`, is
 *     a RAW `SELECT *`, so the archive carries `includePatterns` as JSON *text*
 *     and `enabled`/`allowEmbed` as INTEGER 0/1, and `restore.ts` feeds those
 *     straight into schema-validating `create`s), and every user file is missed
 *     (`getFileFromExtractedBackup`, `archive.ts:334`, gates the storageKey
 *     lookup on `backupFormat === 2` while a modern manifest declares `4`).
 *  2. It is the **ready-made oracle** unit 4's tier-2 DB-state diff consumes, so
 *     the resumed lane starts from a working generator rather than re-deriving
 *     the fresh-instance recipe.
 *
 * A tier-2 DB-STATE differential, not a summary diff: the archive is restored
 * into a FRESHLY PROVISIONED, EMPTY instance and every table in all three
 * partitions is dumped row by row. A row-count map would pass on a graph whose
 * foreign keys all point at nothing; the row dump would not.
 *
 * The fresh instance is built exactly the way `build-provision-oracle.ts` builds
 * one (that differential proves a v5-provisioned instance matches it on schema
 * and seed rows — see the lane record for the baseline gap that remains).
 *
 *   restore_replace          the full archive
 *   restore_legacy_archive   the pre-rework archive, so both parse-time folds
 *                            reach the WRITE path (phase 22f-bis)
 *   restore_minimal          every optional collection absent
 */
const TEST_PEPPER = '3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=';
const SINGLE_USER_ID = 'ffffffff-ffff-ffff-ffff-ffffffffffff';

const RESTORE_CASES = [
  { name: 'restore_replace', archive: 'restore-archive.zip' },
  { name: 'restore_legacy_archive', archive: 'restore-archive-legacy.zip' },
  { name: 'restore_minimal', archive: 'restore-archive-minimal.zip' },
];

/** jest.setup stubs the file-storage manager; the restore file phase IS the
 *  thing under test, so the real modules are restored (`jest-real-db-oracle`). */
function applyMocks(): void {
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

/** Every table in one partition, in rowid (insertion) order. BLOB columns are
 *  reported as `sha256:<hex>` so bytes are diffed without being carried. */
function dumpPartition(db: import('better-sqlite3').Database): Record<string, unknown> {
  const { createHash } = require('node:crypto');
  const tables = (
    db
      .prepare(
        `SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name`,
      )
      .all() as Array<{ name: string }>
  ).map((r) => r.name);
  const out: Record<string, unknown> = {};
  for (const t of tables) {
    const rows = db.prepare(`SELECT * FROM "${t}"`).all() as Array<Record<string, unknown>>;
    out[t] = rows.map((row) => {
      const o: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(row)) {
        o[k] = Buffer.isBuffer(v)
          ? `sha256:${createHash('sha256').update(v).digest('hex')}`
          : (v ?? null);
      }
      return o;
    });
  }
  return out;
}

async function runRestoreCase(
  c: { name: string; archive: string },
  archives: string,
  scratchRoot: string,
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks();

  const work = mkdtempSync(join(scratchRoot, 'inst-'));
  mkdirSync(join(work, 'data'), { recursive: true });
  const mainPath = join(work, 'quilltap.db');
  // The mount-index must sit where BOTH the manager env var and
  // `getMountIndexDatabasePath()` resolve, or the mount-provisioning migrations
  // write a different file than the manager reads (the provision oracle's note).
  const miPath = join(work, 'data', 'quilltap-mount-index.db');
  const llPath = join(work, 'quilltap-llm-logs.db');
  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.SQLITE_PATH = mainPath;
  process.env.SQLITE_MOUNT_INDEX_PATH = miPath;
  process.env.SQLITE_LLM_LOGS_PATH = llPath;
  process.env.QUILLTAP_DATA_DIR = work;
  delete process.env.SQLITE_WAL_MODE;

  const { initializeDatabase, rawQuery, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );

  try {
    await initializeDatabase();
    await rawQuery(
      'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
    );

    // Touch every repo so the lazily-created tables all exist (the fresh-instance
    // recipe from `build-provision-oracle.ts`).
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

    // The deterministic first-boot seed + the built-in templates and mounts.
    const { getOrCreateSingleUser } = await import('@/lib/auth/single-user');
    await getOrCreateSingleUser();
    await getOrCreateSingleUser();
    const { getSeedEmbeddingProfiles, prepareSeedEmbeddingProfile } = await import(
      '@/first-startup'
    );
    await anyRepos.embeddingProfiles.create(
      prepareSeedEmbeddingProfile(getSeedEmbeddingProfiles()[0], SINGLE_USER_ID),
    );
    await anyRepos.roleplayTemplates.seedBuiltInTemplates();
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

    const { restore } = await import('@/lib/backup/restore/restore');
    const summary = await restore(join(archives, c.archive), {
      mode: 'replace',
      targetUserId: SINGLE_USER_ID,
    });

    const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
    const { getRawMountIndexDatabase } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    const { getRawLLMLogsDatabase } = await import(
      '@/lib/database/backends/sqlite/llm-logs-client'
    );
    return {
      name: c.name,
      summary,
      state: {
        main: dumpPartition(getRawDatabase()),
        mountIndex: dumpPartition(getRawMountIndexDatabase()),
        llmLogs: dumpPartition(getRawLLMLogsDatabase()),
      },
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    closeLLMLogsSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const archives = process.env.QT_RESTORE_ARCHIVES;
  if (!archives) throw new Error('QT_RESTORE_ARCHIVES must point at the committed archive dir');
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];

  const { previewRestore } = await import('@/lib/backup/restore/preview');
  for (const c of PREVIEW_CASES) {
    const zipPath = join(archives, c.archive);
    if (!fs.existsSync(zipPath)) throw new Error(`missing archive fixture: ${zipPath}`);
    try {
      const preview = await previewRestore(zipPath);
      outLines.push(JSON.stringify({ name: c.name, preview }));
    } catch (error) {
      outLines.push(
        JSON.stringify({
          name: c.name,
          error: error instanceof Error ? error.message : String(error),
        }),
      );
    }
  }

  const scratchRoot = mkdtempSync(join(tmpdir(), 'qt-restore-oracle-'));
  try {
    for (const c of RESTORE_CASES) {
      outLines.push(JSON.stringify(await runRestoreCase(c, archives, scratchRoot)));
    }
  } finally {
    rmSync(scratchRoot, { recursive: true, force: true });
  }

  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`system-restore oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('system-restore oracle', async () => {
  await main();
});
