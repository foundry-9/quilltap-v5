/**
 * Tier-2 oracle case — `doc_mount_blobs` (the binary byte-store, build step 8).
 *
 * Drives v4's REAL `DocMountBlobsRepository.upsertByFileId` over a fixed corpus
 * of (fileId, bytes, advisory sha, mime) ops, then dumps `doc_mount_blobs` with
 * the `data` BLOB rendered as hex (`canonicalizeRows` → `canonValue` hexes a
 * Buffer). Proves the byte-store mutation: insert by fileId, overwrite-in-place
 * on a repeat fileId, and the **sha-recompute rule** (the stored `sha256` is
 * `sha256(data)`, never the caller's advisory value — every op passes an all-zero
 * advisory sha, so a row that stored it would diverge).
 *
 * NORMALIZATION (done by the Rust harness on both dumps): `upsertByFileId` mints
 * `id` + timestamps, so `id` → first-seen token and timestamps → placeholder;
 * `fileId` is the pinned seeded parent id (left literal), and `sha256` /
 * `sizeBytes` / `storedMimeType` / `data` are deterministic content (compared
 * directly).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DOC_MOUNT_BLOBS=/tmp/qt-blobs-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/doc-mount-blobs-tier2.ts \
 *     > /tmp/oracle-blobs.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  fileId: string;
  dataHex: string;
  sha256: string;
  storedMimeType: string;
}
interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'doc-mount-blobs-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_DOC_MOUNT_BLOBS;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_DOC_MOUNT_BLOBS must point at the fixture from build-doc-mount-blobs-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-blobs-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'blobs-mount-index-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { DocMountBlobsRepository } = await import(
    '@/lib/database/repositories/doc-mount-blobs.repository'
  );

  await initializeDatabase();

  const blobs = new DocMountBlobsRepository();
  for (const op of spec.ops) {
    await blobs.upsertByFileId({
      fileId: op.fileId,
      sha256: op.sha256,
      storedMimeType: op.storedMimeType,
      data: Buffer.from(op.dataHex, 'hex'),
    });
  }

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable (degraded open?)');
  const columns = (
    midb.pragma('table_info(doc_mount_blobs)') as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = midb
    .prepare('SELECT * FROM doc_mount_blobs')
    .all() as Array<Record<string, unknown>>;
  const dump = canonicalizeRows({
    table: 'doc_mount_blobs',
    columns,
    rawRows,
    orderBy: 'fileId',
  });

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'doc-mount-blobs-tier2', ...dump }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`doc-mount-blobs-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
