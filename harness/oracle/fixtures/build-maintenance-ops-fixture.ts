/**
 * Fixture builder for the P4.1d maintenance-ops differential (v4
 * `lib/background-jobs/scheduled-maintenance.ts` `runScheduledMaintenance` —
 * incl. the two newly-ported repo ops `sweepOrphanedFiles` and
 * `cleanupClosedSessions`).
 *
 * Seeds (main + mount-index DBs, through v4's REAL repos + raw INSERTs):
 *   - `background_jobs` rows keyed on `completedAt` RELATIVE TO BUILD TIME
 *     (the maintenance pass uses the wall clock; day-granularity windows are
 *     robust to the minutes between building and running):
 *       j-completed-old (10 d)   → reaped   (COMPLETED window = 7 d)
 *       j-completed-recent (1 d) → kept
 *       j-dead-mid (10 d)        → kept     (DEAD window = 30 d — the split!)
 *       j-dead-old (40 d)        → reaped
 *       j-failed-old (40 d)      → kept     (FAILED is never reaped)
 *       j-pending                → kept
 *   - `terminal_sessions` (`exitedAt`-keyed, 30 d window):
 *       t-old-default (40 d, transcriptPath NULL)   → reaped; the DEFAULT
 *         `<logs>/terminals/<id>.log` transcript is unlinked (counted)
 *       t-old-explicit (40 d, pinned transcriptPath) → reaped; the explicit
 *         file is unlinked (counted)
 *       t-old-nofile (40 d, NULL, no file)           → reaped; ENOENT (not counted)
 *       t-recent (1 d)                               → kept
 *       t-live (exitedAt NULL, ancient startedAt)    → NEVER touched
 *   - mount-index `doc_mount_files`: f-linked (has a `doc_mount_file_links`
 *     row) kept; f-orphan-1 / f-orphan-2 → swept (2).
 *   - empty `chats`/`files`/`characters` tables so the asset collapse runs to
 *     a clean zero; `instance_settings` for the pass stamp.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MAIN=/tmp/qt-maint-ops-main.db QT_FIXTURE_MOUNT=/tmp/qt-maint-ops-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-maintenance-ops-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  explicitTranscriptPath: string;
  jobs: {
    completedOldDays: number;
    completedRecentDays: number;
    deadMidDays: number;
    deadOldDays: number;
    failedOldDays: number;
  };
  terminals: { closedOldDays: number; closedRecentDays: number };
}

const DAY_MS = 24 * 60 * 60 * 1000;

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'maintenance-ops.json'), 'utf8')) as Spec;

  const outMain = process.env.QT_FIXTURE_MAIN;
  const outMount = process.env.QT_FIXTURE_MOUNT;
  if (!outMain || !outMount) throw new Error('QT_FIXTURE_MAIN and QT_FIXTURE_MOUNT must be set');
  for (const base of [outMain, outMount]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = base + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-maint-ops-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, ensureCollection, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const { UserSchema, CharacterSchema, FileEntrySchema } = await import('@/lib/schemas/types');
  const { TerminalSessionSchema } = await import('@/lib/schemas/terminal.types');
  const { DocMountFileSchema, DocMountFileLinkSchema } = await import(
    '@/lib/schemas/mount-index.types'
  );

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('files', FileEntrySchema);
  await ensureCollection('terminal_sessions', TerminalSessionSchema);

  const repos = getRepositories();
  const ts = spec.seedTimestamp;

  // The chats + background_jobs tables (repo touches materialize them —
  // ChatSchema carries a .refine(), so ensureCollection rejects it; the repo's
  // own lazy getCollection path builds the right DDL).
  await repos.chats.findAll();
  await repos.chats.getMessageCount('00000000-0000-4000-8000-000000000000');
  await repos.backgroundJobs.getStats();
  // The stamp store (v4's version guard normally creates it at boot).
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
    [],
  );

  const now = Date.now();
  const ago = (days: number) => new Date(now - days * DAY_MS).toISOString();

  // Background jobs (statuses + completedAt drive the reaper).
  const job = async (
    id: string,
    status: string,
    completedAtDays: number | null,
  ) => {
    await repos.backgroundJobs.create(
      {
        userId: spec.userId,
        type: 'MEMORY_HOUSEKEEPING',
        status,
        payload: { marker: id },
        priority: 0,
        attempts: 0,
        maxAttempts: 3,
        lastError: null,
        scheduledAt: ts,
        startedAt: null,
        completedAt: completedAtDays === null ? null : ago(completedAtDays),
      } as never,
      { id, createdAt: ts, updatedAt: ts },
    );
  };
  await job('a0000000-0000-4000-8000-0000000000b1', 'COMPLETED', spec.jobs.completedOldDays);
  await job('a0000000-0000-4000-8000-0000000000b2', 'COMPLETED', spec.jobs.completedRecentDays);
  await job('a0000000-0000-4000-8000-0000000000b3', 'DEAD', spec.jobs.deadMidDays);
  await job('a0000000-0000-4000-8000-0000000000b4', 'DEAD', spec.jobs.deadOldDays);
  await job('a0000000-0000-4000-8000-0000000000b5', 'FAILED', spec.jobs.failedOldDays);
  await job('a0000000-0000-4000-8000-0000000000b6', 'PENDING', null);

  // Terminal sessions.
  const session = async (
    id: string,
    exitedAtDays: number | null,
    transcriptPath: string | null,
  ) => {
    await repos.terminalSessions.create(
      {
        chatId: '00000000-0000-4000-8000-000000000000',
        label: null,
        shell: 'zsh',
        cwd: '/tmp',
        startedAt: '2020-01-01T00:00:00.000Z',
        exitedAt: exitedAtDays === null ? null : ago(exitedAtDays),
        exitCode: exitedAtDays === null ? null : 0,
        transcriptPath,
      } as never,
      { id, createdAt: ts, updatedAt: ts },
    );
  };
  await session('a0000000-0000-4000-8000-0000000000d1', spec.terminals.closedOldDays, null);
  await session(
    'a0000000-0000-4000-8000-0000000000d2',
    spec.terminals.closedOldDays,
    spec.explicitTranscriptPath,
  );
  await session('a0000000-0000-4000-8000-0000000000d3', spec.terminals.closedOldDays, null);
  await session('a0000000-0000-4000-8000-0000000000d4', spec.terminals.closedRecentDays, null);
  await session('a0000000-0000-4000-8000-0000000000d5', null, null);

  // Mount-index: one linked file, two orphans.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  for (const [name, schema] of [
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
  ] as Array<[string, unknown]>) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }

  // [40319484] `gcOrphanedFileRow` runs inside every content-addressed rewrite
  // and deletes from `doc_mount_blobs` unconditionally, so a mount index that
  // lacks the table now throws `no such table` on the SECOND write to any path.
  // v4's blobs repository creates it lazily from hand-written DDL
  // (`doc-mount-blobs.repository.ts:113`); copied verbatim so the fixture carries
  // exactly the table a real instance has (it is in `fresh_schema.json` too).
  midb.exec(`
    CREATE TABLE IF NOT EXISTS "doc_mount_blobs" (
      "id" TEXT PRIMARY KEY,
      "fileId" TEXT NOT NULL,
      "sha256" TEXT NOT NULL,
      "sizeBytes" INTEGER NOT NULL,
      "storedMimeType" TEXT NOT NULL,
      "data" BLOB NOT NULL,
      "createdAt" TEXT NOT NULL,
      "updatedAt" TEXT NOT NULL,
      FOREIGN KEY ("fileId") REFERENCES "doc_mount_files" ("id") ON DELETE CASCADE
    )
  `);
  midb.exec(
    'CREATE UNIQUE INDEX IF NOT EXISTS "idx_doc_mount_blobs_fileId" ON "doc_mount_blobs" ("fileId")'
  );
  const dmf = midb.prepare(
    `INSERT INTO doc_mount_files (id, sha256, fileSizeBytes, fileType, source, createdAt, updatedAt)
     VALUES (?, ?, 16, 'markdown', 'database', ?, ?)`,
  );
  dmf.run('20000000-0000-4000-8000-0000000000f1', 'a'.repeat(64), ts, ts);
  dmf.run('20000000-0000-4000-8000-0000000000f2', 'b'.repeat(64), ts, ts);
  dmf.run('20000000-0000-4000-8000-0000000000f3', 'c'.repeat(64), ts, ts);
  midb
    .prepare(
      `INSERT INTO doc_mount_file_links (id, fileId, mountPointId, relativePath, fileName, lastModified, createdAt, updatedAt)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
    )
    .run(
      '20000000-0000-4000-8000-0000000000f4',
      '20000000-0000-4000-8000-0000000000f1',
      '20000000-0000-4000-8000-0000000000f9',
      'kept.md',
      'kept.md',
      ts,
      ts,
      ts,
    );

  await closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(`built maintenance-ops fixture: main=${outMain} mount=${outMount}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`maintenance-ops fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
