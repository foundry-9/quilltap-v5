/**
 * @jest-environment node
 *
 * Differential ORACLE for the images-v2 ingest engine (P4.1b; v4
 * `lib/images-v2.ts` `ingestImageBuffer` → `createFile`). Drives v4's REAL
 * ingest over ONE working copy of the seeded two-DB fixture, running the
 * committed op sequence from `image-ingest.json`:
 *   fresh ingest → hash-dedup linkedTo-merge → dedup no-op → (raw blob delete)
 *   → the orphaned-metadata recheck re-ingest → webp passthrough → svg
 *   passthrough → gif convert.
 *
 * SHARP is mocked to a deterministic passthrough (metadata → spec.mockDims,
 * every encode returns the input bytes) — mirrored by the Rust side's
 * `PassthroughPixelCodec`. The WebP POLICY (convertToWebP's mime/filename
 * rewrite, blob-transcode's stored-as-is rules) runs REAL on both sides, so
 * the policy itself is under differential test.
 *
 * Real-DB-under-jest per [[jest-real-db-oracle]]; jest.setup's global mocks of
 * `@/lib/file-storage/manager`, `@/lib/file-storage/user-uploads-bridge`, and
 * `@/lib/files/tag-inheritance` are un-mocked (requireActual) so the real
 * engine runs end to end.
 *
 * Emits NDJSON: one `{kind:'op', ...}` line per op (the returned FileEntry's
 * comparable fields) + one `{kind:'dump', tables:{...}}` line (files + the
 * five mount-index store tables, blob data as hex).
 *
 * Run (Node 24, from the v4 checkout; stage the case OUTSIDE any .claude path
 * — v4's jest ignores /\.claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5 ; TMPO=/tmp/qt-oracle-run
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_INGEST_MAIN=/tmp/qt-ingest-main.db QT_FIXTURE_INGEST_MOUNT=/tmp/qt-ingest-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-image-ingest-fixture.ts
 *   mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp $V5/harness/oracle/cases/image-ingest.test.ts "$TMPO/cases/"
 *   cp $V5/harness/oracle/fixtures/image-ingest.json  "$TMPO/fixtures/"
 *   QT_FIXTURE_INGEST_MAIN=/tmp/qt-ingest-main.db QT_FIXTURE_INGEST_MOUNT=/tmp/qt-ingest-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-image-ingest.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- image-ingest
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SpecOp {
  op: 'ingest' | 'delete_blob_of';
  key: string;
  payload?: string;
  filename?: string;
  mimeType?: string;
  linkedTo?: string[];
  target?: string;
}

interface Spec {
  testPepperBase64: string;
  userId: string;
  uploadsMountPointId: string;
  mockDims: { width: number; height: number };
  payloads: Record<string, string>;
  ops: SpecOp[];
}

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

function canonRows(
  rawRows: Array<Record<string, unknown>>,
  orderBy: string,
): Array<Record<string, unknown>> {
  return rawRows
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const k of Object.keys(r).sort()) out[k] = canonValue(r[k]);
      return out;
    })
    .sort((a, b) => {
      const av = String(a[orderBy] ?? '');
      const bv = String(b[orderBy] ?? '');
      return av < bv ? -1 : av > bv ? 1 : 0;
    });
}

describe('image-ingest oracle', () => {
  it('runs the committed op sequence through the real ingest engine', async () => {
    const here = dirname(fileURLToPath(import.meta.url));
    const spec = JSON.parse(
      fs.readFileSync(join(here, '..', 'fixtures', 'image-ingest.json'), 'utf8'),
    ) as Spec;

    const mainFixture = process.env.QT_FIXTURE_INGEST_MAIN;
    const mountFixture = process.env.QT_FIXTURE_INGEST_MOUNT;
    if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
      throw new Error('QT_FIXTURE_INGEST_MAIN/MOUNT must point at the seeded fixtures');
    }
    const outPath = process.env.QT_ORACLE_OUT;
    if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

    const scratch = mkdtempSync(join(tmpdir(), 'qt-ingest-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'ingest-main.db');
    const mountWork = join(scratch, 'ingest-mount.db');
    copyFileSync(mainFixture, mainWork);
    copyFileSync(mountFixture, mountWork);

    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    const cipherDriverPath = join(
      process.cwd(),
      'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
    );

    jest.resetModules();
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () =>
      jest.requireActual('@/lib/database/repositories'),
    );
    jest.doMock('@/lib/repositories/factory', () =>
      jest.requireActual('@/lib/repositories/factory'),
    );
    // jest.setup mocks the whole file-storage manager + the user-uploads bridge
    // + tag-inheritance — un-mock them: they ARE the unit under test.
    jest.doMock('@/lib/file-storage/manager', () =>
      jest.requireActual('@/lib/file-storage/manager'),
    );
    jest.doMock('@/lib/file-storage/user-uploads-bridge', () =>
      jest.requireActual('@/lib/file-storage/user-uploads-bridge'),
    );
    jest.doMock('@/lib/files/tag-inheritance', () =>
      jest.requireActual('@/lib/files/tag-inheritance'),
    );
    // SHARP → deterministic passthrough (the Rust PassthroughPixelCodec mirror):
    // metadata reports the fixed mockDims; every encode returns the input bytes.
    // CJS-style factory (a bare function module) so BOTH the static
    // `import sharp from 'sharp'` (blob-transcode) and the dynamic
    // `(await import('sharp')).default` (images-v2, webp-conversion) interop to
    // the function — do NOT set __esModule (the [[jest-crypto-randombytes-mock]]
    // default-import gotcha).
    const dims = spec.mockDims;
    // The case file is staged under /tmp (outside the v4 root), so a bare
    // 'sharp' does not resolve from here — mock by the ABSOLUTE package path,
    // which is the same resolved module every v4 importer reaches.
    const sharpPath = join(process.cwd(), 'node_modules/sharp');
    jest.doMock(sharpPath, () => {
      const fake = (buffer: Buffer, _opts?: unknown) => {
        const chain = {
          metadata: async () => ({ width: dims.width, height: dims.height, hasAlpha: false }),
          webp: (_o?: unknown) => chain,
          toBuffer: async () => Buffer.from(buffer),
        };
        return chain;
      };
      return fake;
    });

    const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
    const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    const { getRepositories } = await import('@/lib/repositories/factory');
    const { ingestImageBuffer } = await import('@/lib/images-v2');

    await initializeDatabase();
    const repos = getRepositories();

    const lines: string[] = [];
    const entryByKey: Record<string, Record<string, unknown>> = {};

    for (const op of spec.ops) {
      if (op.op === 'ingest') {
        const entry = (await ingestImageBuffer({
          buffer: Buffer.from(spec.payloads[op.payload!], 'utf-8'),
          originalFilename: op.filename!,
          mimeType: op.mimeType!,
          userId: spec.userId,
          linkedTo: op.linkedTo ?? [],
        })) as unknown as Record<string, unknown>;
        entryByKey[op.key] = entry;
        lines.push(
          JSON.stringify({
            kind: 'op',
            key: op.key,
            entry: {
              id: entry.id,
              sha256: entry.sha256,
              originalFilename: entry.originalFilename,
              mimeType: entry.mimeType,
              size: entry.size,
              width: entry.width ?? null,
              height: entry.height ?? null,
              linkedTo: entry.linkedTo,
              tags: entry.tags,
              source: entry.source,
              category: entry.category,
              storageKey: entry.storageKey,
            },
          }),
        );
      } else if (op.op === 'delete_blob_of') {
        const target = entryByKey[op.target!];
        const storageKey = String(target.storageKey);
        const blobId = storageKey.split(':')[2];
        const deleted = await repos.docMountBlobs.delete(blobId);
        lines.push(JSON.stringify({ kind: 'op', key: op.key, deletedBlob: deleted }));
      }
    }

    // Final dumps.
    const filesRows =
      ((await rawQuery<Array<Record<string, unknown>>>('SELECT * FROM files', [])) ?? []);
    const midb = getRawMountIndexDatabase();
    if (!midb) throw new Error('mount-index DB handle unavailable');
    const mountTable = (name: string): Array<Record<string, unknown>> =>
      midb.prepare(`SELECT * FROM ${name}`).all() as Array<Record<string, unknown>>;

    const dump = {
      kind: 'dump',
      tables: {
        files: canonRows(filesRows, 'sha256'),
        doc_mount_points: canonRows(mountTable('doc_mount_points'), 'name'),
        doc_mount_folders: canonRows(mountTable('doc_mount_folders'), 'path'),
        doc_mount_files: canonRows(mountTable('doc_mount_files'), 'sha256'),
        doc_mount_blobs: canonRows(mountTable('doc_mount_blobs'), 'sha256'),
        doc_mount_file_links: canonRows(mountTable('doc_mount_file_links'), 'relativePath'),
      },
    };
    lines.push(JSON.stringify(dump));

    await closeMountIndexSQLiteClient();
    await closeDatabase();

    fs.writeFileSync(outPath, lines.join('\n') + '\n');
    expect(lines.length).toBe(spec.ops.length + 1);
  });
});
