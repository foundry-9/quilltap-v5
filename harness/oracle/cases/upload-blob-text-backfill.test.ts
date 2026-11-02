/**
 * @jest-environment node
 *
 * ORACLE (P4.D354, v4 `40fab1c54`, bugs 260 / 261) for the uploaded-blob text
 * backfill: v4's REAL `backfillUploadBlobText` (`lib/startup/
 * backfill-upload-blob-text.ts`) over a fresh copy of the pre-bug-260 pair the
 * builder minted (`build-upload-blob-text-backfill-fixture.ts`), run TWICE —
 * the second run is the steady state (idempotence: an empty worklist).
 *
 * Emits ONE NDJSON line per run: the result tallies, the captured
 * `Startup:UploadBlobTextBackfill` lines (+ any other warn/error), and the two
 * partitions' dumps (the mount-write family's SQL — links / blobs minus
 * `data` / chunks / files / folders / points / documents + `background_jobs`).
 * A THIRD run follows a raw delete of the bytes-missing link (identical on
 * both sides), so the empty-worklist DEBUG line is exercised too.
 *
 * The PDF converter is doMocked to answer `''` — v4's reader cannot load its
 * ESM `pdfjs-dist` build under jest, and `''` is exactly what v4's converter
 * answers for an unreadable PDF (its own failures are swallowed into `''`) and
 * what v5's refusing seam answers: the R-1 carve's v4 shape.
 *
 * The job host is HELD OFF (`__quilltapJobHost.shuttingDown`): the backfill
 * enqueues `EMBEDDING_GENERATE` jobs and v4's runner would otherwise claim them
 * mid-case (memory note `v4-oracle-job-host-and-log-capture`). The garbage
 * `.PDF` goes through v4's REAL pdfjs reader, which answers `''` on it — the
 * same text v5's refusing seam answers (the R-1 carve's v4 shape).
 *
 * Run (Node 24). STAGE outside `.claude/` (jest ignores those paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>
 *   STAGE=/tmp/qt-oracle-stage-ubtb
 *   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
 *   cp $W/harness/oracle/cases/upload-blob-text-backfill.test.ts $STAGE/harness/oracle/cases/
 *   cp $W/harness/oracle/fixtures/mounts.json $STAGE/harness/oracle/fixtures/
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_UBTB_MAIN=/tmp/qt-ubtb-main.db QT_FIXTURE_UBTB_MOUNT=/tmp/qt-ubtb-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-upload-blob-text-backfill-fixture.ts
 *   QT_FIXTURE_UBTB_MAIN=/tmp/qt-ubtb-main.db QT_FIXTURE_UBTB_MOUNT=/tmp/qt-ubtb-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-ubtb.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "upload-blob-text-backfill\.test\.ts$"
 */

import * as fs from 'fs';
import { copyFileSync, existsSync, mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const CAPTURED = new Set([
  'No unextracted text blobs; nothing to backfill',
  'Extracting text from uploaded blobs',
  'Blob bytes missing; skipping text backfill',
  'Chunking a backfilled blob failed; its text is still readable',
  'Queueing embeddings for a backfilled store failed',
  'Uploaded-blob text backfill finished',
  'Uploaded-blob text backfill failed',
  'Failed to list unextracted blobs',
  'Failed to read blob data',
]);

const MOUNT_TABLES: Record<string, string> = {
  points:
    'SELECT id, name, basePath, mountType, storeType, includePatterns, excludePatterns, enabled, ' +
    'lastScannedAt, scanStatus, lastScanError, conversionStatus, conversionError, fileCount, ' +
    'chunkCount, totalSizeBytes, createdAt, updatedAt FROM doc_mount_points ORDER BY id',
  files:
    'SELECT id, sha256, fileSizeBytes, fileType, source, createdAt, updatedAt ' +
    'FROM doc_mount_files ORDER BY sha256, source',
  links:
    'SELECT id, fileId, mountPointId, relativePath, fileName, folderId, originalFileName, ' +
    'originalMimeType, description, descriptionUpdatedAt, conversionStatus, conversionError, ' +
    'plainTextLength, extractedText, extractedTextSha256, extractionStatus, extractionError, ' +
    'chunkCount, allowEmbed, allowCharacterRead, allowCharacterWrite, lastModified, createdAt, ' +
    'updatedAt, linkGroupId FROM doc_mount_file_links ORDER BY mountPointId, relativePath',
  documents:
    'SELECT id, fileId, content, contentSha256, plainTextLength, createdAt, updatedAt ' +
    'FROM doc_mount_documents ORDER BY contentSha256',
  folders:
    'SELECT id, mountPointId, parentId, name, path, createdAt, updatedAt ' +
    'FROM doc_mount_folders ORDER BY mountPointId, path',
  chunks:
    'SELECT c.id, c.linkId, c.mountPointId, c.chunkIndex, c.content, c.tokenCount, ' +
    'c.headingContext, (CASE WHEN c.embedding IS NULL THEN NULL ELSE length(c.embedding) END) ' +
    'AS embeddingLength, c.createdAt, c.updatedAt, l.relativePath AS sortPath ' +
    'FROM doc_mount_chunks c LEFT JOIN doc_mount_file_links l ON l.id = c.linkId ' +
    'ORDER BY c.mountPointId, sortPath, c.chunkIndex',
  blobs:
    'SELECT id, fileId, sha256, sizeBytes, storedMimeType, length(data) AS dataLength, ' +
    'createdAt, updatedAt FROM doc_mount_blobs ORDER BY sha256',
};
const JOBS_SQL =
  'SELECT id, userId, type, status, payload, priority, attempts, maxAttempts, lastError, ' +
  'scheduledAt, startedAt, completedAt, createdAt, updatedAt FROM background_jobs ORDER BY payload';

interface LogLine {
  level: string;
  message: string;
  fields: Record<string, unknown>;
}

test('upload-blob-text-backfill oracle', async () => {
  const here = dirname(fileURLToPath(new URL(`file://${__filename}`).href));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'mounts.json'), 'utf8'),
  ) as { testPepperBase64: string };
  const fixMain = process.env.QT_FIXTURE_UBTB_MAIN ?? '';
  const fixMount = process.env.QT_FIXTURE_UBTB_MOUNT ?? '';
  if (!existsSync(fixMain) || !existsSync(fixMount)) {
    throw new Error('QT_FIXTURE_UBTB_MAIN / QT_FIXTURE_UBTB_MOUNT must point at the built pair');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ubtb-oracle-'));
  try {
    mkdirSync(join(scratch, 'data'), { recursive: true });
    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'debug';
    (globalThis as Record<string, unknown>).__quilltapJobHost = {
      child: null, spawning: false, shuttingDown: true, childCrashed: false,
      restartTimestamps: [], signalHandlersInstalled: false, invalidationListeners: new Set(),
    };

    jest.resetModules();
    const cipherDriverPath = require('node:path').join(
      process.cwd(),
      'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
    );
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    for (const m of [
      '@/lib/database/manager',
      '@/lib/database/repositories',
      '@/lib/repositories/factory',
      '@/lib/embedding/vector-store',
      '@/lib/embedding/embedding-service',
      '@/lib/mount-index/embedding-scheduler',
      '@/lib/doc-edit/reindex-file',
      '@/lib/mount-index/store-file',
      '@/lib/mount-index/converters',
    ]) {
      jest.doMock(m, () => jest.requireActual(m));
    }

    jest.doMock('@/lib/mount-index/converters/pdf-converter', () => ({
      __esModule: true,
      convertPdfBufferToText: async () => '',
      convertPdfToText: async () => '',
    }));

    const work = mkdtempSync(join(scratch, 'run-'));
    const mainWork = join(work, 'main.db');
    const mountWork = join(work, 'mount.db');
    copyFileSync(fixMain, mainWork);
    copyFileSync(fixMount, mountWork);
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

    const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
    const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
    await initializeDatabase();

    const { logger } = await import('@/lib/logger');
    let captured: LogLine[] = [];
    const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
    internal.transports.splice(0, internal.transports.length, {
      write: (d: unknown) => {
        const data = d as {
          level: string;
          message: string;
          context?: Record<string, unknown>;
          error?: { message: string };
        };
        const { service: _s, module: _m, environment: _e, ...fields } = (data.context ??
          {}) as Record<string, unknown>;
        if (data.error) fields.error = data.error.message;
        const line = JSON.parse(
          JSON.stringify({ level: data.level, message: data.message, fields }),
        ) as LogLine;
        if (CAPTURED.has(data.message) || data.level === 'warn' || data.level === 'error') {
          captured.push(line);
        }
      },
    });

    const dump = (): Record<string, unknown> => {
      const midb = getRawMountIndexDatabase() as never as {
        prepare: (s: string) => { all: () => unknown[] };
      };
      const main = getRawDatabase() as never as { prepare: (s: string) => { all: () => unknown[] } };
      const out: Record<string, unknown> = {};
      for (const [name, sql] of Object.entries(MOUNT_TABLES)) out[name] = midb.prepare(sql).all();
      out.jobs = main.prepare(JOBS_SQL).all();
      return out;
    };
    const drain = async (): Promise<void> => {
      for (let i = 0; i < 60; i++) await new Promise((r) => setImmediate(r));
    };

    const { backfillUploadBlobText } = await import('@/lib/startup/backfill-upload-blob-text');
    const lines: string[] = [];
    try {
      for (const run of ['first', 'steady', 'emptied']) {
        if (run === 'emptied') {
          (getRawMountIndexDatabase() as never as {
            prepare: (s: string) => { run: (...a: unknown[]) => unknown };
          })
            .prepare("DELETE FROM doc_mount_file_links WHERE relativePath = 'uploads/gone.md'")
            .run();
        }
        captured = [];
        const result = await backfillUploadBlobText();
        await drain();
        lines.push(JSON.stringify({ run, result, log: captured, tables: dump() }));
      }
    } finally {
      await closeDatabase();
      closeMountIndexSQLiteClient();
    }
    fs.writeFileSync(outPath, lines.join('\n') + '\n');
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}, 180000);
