/**
 * P4.106 item 10 — builds the committed `qtap-import-two-link-blob.qtap`
 * through v4's REAL exporter (`createNdjsonStream`, type `document-stores`).
 *
 * The row the P4.D209 lane record left OPEN (status-log "What is OPEN under the
 * order at lane close"): a `.qtap` whose document-store section holds TWO blob
 * entries with the SAME bytes / `sha256` at two `relativePath`s, the SECOND
 * carrying `extractedText`. After import the second link must hold the text and
 * the first must be NULL — the bug-157 repoint (`document_stores.rs` →
 * `Some(&created.link_id)`) had no corpus row that could tell it from the
 * deleted fallback.
 *
 * The store is built in a throwaway instance through v4's REAL repositories:
 * one database-backed store, `docMountBlobs.create` twice over the same PNG
 * bytes (the second write dedups onto the first blob, minting a second link),
 * then `updateExtractedText(blob, …, secondLinkId)` — the per-link sidecar.
 * `normalizeImages: false` on both writes, so the archived bytes ARE the PNG
 * (a decodable 1×1) and the import's byte-fidelity rule is visible: the
 * imported bytes must equal the bundle's, and `image/png` must stay `image/png`.
 *
 * The output is v4's exporter bytes, verbatim (NDJSON). Built ONCE at the
 * `f45a517a9` pin and committed; it is read by both sides of
 * `qtap_import_equivalence` through each side's REAL NDJSON loader.
 *
 * Run (Node 24, from the (pinned) v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_BUNDLE_OUT=/tmp/qtap-import-two-link-blob.qtap \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-qtap-import-two-link-blob.ts
 *   cp /tmp/qtap-import-two-link-blob.qtap ~/source/quilltap-v5/harness/oracle/fixtures/
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { createHash } from 'node:crypto';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

/** A decodable 1×1 RGBA PNG. */
const PNG_BASE64 =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';
const STORE_ID = '1e000000-0000-4000-8000-0000000000a1';
const TS = '2026-09-01T00:00:00.000Z';
const EXTRACTED = 'A lamplighter on a ladder, one pane lit.';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'qtap-import-tier2.json'), 'utf8')) as {
    testPepperBase64: string;
  };
  const out = process.env.QT_BUNDLE_OUT;
  if (!out) throw new Error('QT_BUNDLE_OUT must point at the .qtap to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-two-link-blob-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'mount.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    ProjectDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  const { DocMountBlobsRepository } = await import(
    '@/lib/database/repositories/doc-mount-blobs.repository'
  );
  await new DocMountBlobsRepository().findByFileId('00000000-0000-4000-8000-000000000000');

  const repos = getRepositories();
  await repos.docMountPoints.create(
    {
      name: 'Lamplighter Plates',
      basePath: '',
      mountType: 'database',
      storeType: 'documents',
      includePatterns: [],
      excludePatterns: [],
      enabled: true,
    } as never,
    { id: STORE_ID, createdAt: TS, updatedAt: TS } as never,
  );

  const data = Buffer.from(PNG_BASE64, 'base64');
  const sha256 = createHash('sha256').update(data).digest('hex');
  const write = (relativePath: string) =>
    repos.docMountBlobs.create({
      mountPointId: STORE_ID,
      relativePath,
      originalFileName: relativePath.split('/').pop()!,
      originalMimeType: 'image/png',
      storedMimeType: 'image/png',
      sha256,
      data,
      normalizeImages: false,
    });
  const first = await write('plates/first.png');
  const second = await write('plates/second.png');
  if (first.id !== second.id) {
    throw new Error(`expected ONE deduped blob, got ${first.id} and ${second.id}`);
  }
  if (first.linkId === second.linkId) throw new Error('expected TWO links');
  await repos.docMountBlobs.updateExtractedText(
    second.id,
    {
      extractedText: EXTRACTED,
      extractedTextSha256: createHash('sha256').update(EXTRACTED).digest('hex'),
      extractionStatus: 'converted',
      extractionError: null,
    } as never,
    second.linkId,
  );

  const { createNdjsonStream } = await import('@/lib/export/ndjson-writer');
  const stream = createNdjsonStream('00000000-0000-4000-8000-000000000001', {
    type: 'document-stores',
    scope: 'selected',
    selectedIds: [STORE_ID],
  } as never);
  const reader = stream.getReader();
  const chunks: Buffer[] = [];
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    if (value) chunks.push(Buffer.from(value));
  }
  const bytes = Buffer.concat(chunks);
  writeFileSync(out, bytes);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(
    `built ${out} (${bytes.length} bytes; blob ${sha256.slice(0, 12)}…, two links)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`two-link-blob bundle build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
