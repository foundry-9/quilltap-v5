/**
 * Tier-2 oracle case — the document-store WRITE-METADATA contract (P4.D209).
 *
 * Drives v4's REAL `DocMountFileLinksRepository` and `DocMountBlobsRepository`
 * DIRECTLY — `linkDocumentContent`, `linkBlobContent`, `setLinkTimestamps`,
 * `bindLinkGroup`, `updateDescription`, `updateExtractedText` — never through
 * `writeDatabaseDocument`. That distinction is the whole instrument: the
 * database-store writer re-chunks the moment it returns, so it puts `chunkCount`
 * straight back and bug 156 leaves NO trace in a final-state dump. Every
 * byte-preserving writer (file-ops, the sync applier, every in-child
 * `doc_write_file`) calls the repository directly, and that is where the three
 * defects live:
 *
 *   - **bug 155** (`23da0b322`) — `linkBlobContent`'s UPDATE branch blanked
 *     `description` / `extractedText` / `extractionStatus` whenever the caller
 *     omitted them. Omitted now means KEEP; an explicit `''` still clears.
 *   - **bug 156** (`23da0b322`) — repointing a link left the old revision's
 *     chunks answering search while the link went on claiming
 *     `chunkCount > 0, converted`, which is exactly the predicate the rescan
 *     uses to decide it has nothing to do.
 *   - **bug 157** (`0c14fd61f`) — `updateDescription` / `updateExtractedText`
 *     resolved their target with `WHERE fileId = ? LIMIT 1`, so on a blob
 *     carried at several locations the caption landed on an arbitrary one.
 *     `linkId` is required now.
 *
 * Plus seam 4 of `23da0b322`: the optional per-location `lastModified` /
 * `createdAt` on both writers, and `setLinkTimestamps` as their mirror.
 *
 * NORMALIZATION: ids are minted internally and remapped to first-seen tokens by
 * the Rust harness, exactly as `doc-mount-file-links-tier2` does. Timestamps are
 * placeholdered EXCEPT the corpus's `pinnedTimestamps`, which stay literal —
 * without that carve-out "honoured the caller's clock" and "stamped now" are
 * indistinguishable.
 *
 * The fixture is the one `build-doc-mount-file-links-fixture.ts` builds (same
 * seeded store id), so no second builder exists to drift.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server        # or a worktree pinned at the baseline
 *   QT_FIXTURE_OUT=/tmp/qt-dmwm-fixture.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-doc-mount-file-links-fixture.ts
 *   QT_FIXTURE_DOC_MOUNT_WRITE_METADATA=/tmp/qt-dmwm-fixture.db \
 *     $N/npx tsx $V5W/harness/oracle/cases/doc-mount-write-metadata.ts \
 *     > /tmp/oracle-dmwm.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Spec {
  testPepperBase64: string;
  store: { id: string };
  pinnedTimestamps: string[];
  ops: Array<Record<string, unknown>>;
}

/** The corpus distinguishes an ABSENT key from an explicit `null`. */
function has(op: Record<string, unknown>, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(op, key);
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'doc-mount-write-metadata.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_DOC_MOUNT_WRITE_METADATA;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_DOC_MOUNT_WRITE_METADATA must point at the fixture from build-doc-mount-file-links-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-dmwm-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'dmwm-mount-index-work.db');
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

  await initializeDatabase();

  const { getRepositories } = await import('@/lib/repositories/factory');
  const { reindexSingleFile } = await import('@/lib/doc-edit/reindex-file');
  const links = () => getRepositories().docMountFileLinks;
  const blobs = () => getRepositories().docMountBlobs;

  const storeId = spec.store.id;
  const results: Array<Record<string, unknown>> = [];

  const linkAt = async (rel: string) => {
    const l = await links().findByMountPointAndPath(storeId, rel);
    if (!l) throw new Error(`no link at ${rel}`);
    return l;
  };
  const blobAt = async (rel: string) => {
    const b = await blobs().findByMountPointAndPath(storeId, rel);
    if (!b) throw new Error(`no blob at ${rel}`);
    return b;
  };

  for (let i = 0; i < spec.ops.length; i += 1) {
    const op = spec.ops[i];
    const kind = op.kind as string;
    switch (kind) {
      case 'write-doc': {
        const content = op.content as string;
        await links().linkDocumentContent({
          mountPointId: storeId,
          relativePath: op.relativePath as string,
          fileName: (op.relativePath as string).split('/').pop()!,
          folderId: null,
          fileType: 'markdown',
          content,
          contentSha256: (await import('node:crypto'))
            .createHash('sha256')
            .update(content, 'utf-8')
            .digest('hex'),
          plainTextLength: content.length,
          fileSizeBytes: Buffer.byteLength(content, 'utf-8'),
          ...(has(op, 'lastModified') ? { lastModified: op.lastModified as string } : {}),
          ...(has(op, 'createdAt') ? { createdAt: op.createdAt as string } : {}),
        });
        break;
      }
      case 'write-blob': {
        await links().linkBlobContent({
          mountPointId: storeId,
          relativePath: op.relativePath as string,
          fileName: (op.relativePath as string).split('/').pop()!,
          folderId: null,
          originalFileName: op.originalFileName as string,
          originalMimeType: op.originalMimeType as string,
          storedMimeType: op.storedMimeType as string,
          // Advisory only — linkBlobContent recomputes from the bytes.
          sha256: '0'.repeat(64),
          data: Buffer.from(op.dataHex as string, 'hex'),
          // The three metadata fields ride the corpus's key PRESENCE, which is
          // bug 155's whole distinction.
          ...(has(op, 'description') ? { description: op.description as string } : {}),
          ...(has(op, 'extractedText')
            ? { extractedText: op.extractedText as string | null }
            : {}),
          ...(has(op, 'extractedTextSha256')
            ? { extractedTextSha256: op.extractedTextSha256 as string | null }
            : {}),
          ...(has(op, 'extractionStatus')
            ? { extractionStatus: op.extractionStatus as 'none' }
            : {}),
          ...(has(op, 'lastModified') ? { lastModified: op.lastModified as string } : {}),
          ...(has(op, 'createdAt') ? { createdAt: op.createdAt as string } : {}),
        });
        break;
      }
      case 'chunk':
        await reindexSingleFile(storeId, op.relativePath as string, '');
        break;
      case 'bind-group': {
        const source = await linkAt(op.sourcePath as string);
        const dest = await linkAt(op.destPath as string);
        await links().bindLinkGroup(source.id, dest.id);
        break;
      }
      case 'set-timestamps': {
        const link = await linkAt(op.relativePath as string);
        const moved = await links().setLinkTimestamps(link.id, {
          ...(has(op, 'lastModified') ? { lastModified: op.lastModified as string } : {}),
          ...(has(op, 'createdAt') ? { createdAt: op.createdAt as string } : {}),
        });
        results.push({ op: i, kind, path: op.relativePath, moved });
        break;
      }
      case 'set-timestamps-unknown': {
        const moved = await links().setLinkTimestamps(op.linkId as string, {
          lastModified: '2000-01-01T00:00:00.000Z',
        });
        results.push({ op: i, kind, moved });
        break;
      }
      case 'resolve-links': {
        const ids: string[] = [];
        for (const rel of op.paths as string[]) {
          const b = await blobAt(rel);
          ids.push(b.linkId);
        }
        results.push({
          op: i,
          kind,
          paths: op.paths,
          linkIds: ids,
          distinct: new Set(ids).size === ids.length,
        });
        break;
      }
      case 'update-description': {
        const b = await blobAt(op.relativePath as string);
        const updated = await blobs().updateDescription(
          b.id,
          op.description as string,
          b.linkId
        );
        results.push({
          op: i,
          kind,
          path: op.relativePath,
          returnedLinkId: updated?.linkId ?? null,
          returnedDescription: updated?.description ?? null,
        });
        break;
      }
      case 'update-extracted-text': {
        const b = await blobAt(op.relativePath as string);
        const updated = await blobs().updateExtractedText(
          b.id,
          {
            extractedText: (op.extractedText as string | null) ?? null,
            extractedTextSha256: (op.extractedTextSha256 as string | null) ?? null,
            extractionStatus: op.extractionStatus as 'converted',
            extractionError: (op.extractionError as string | null) ?? null,
          },
          b.linkId
        );
        results.push({
          op: i,
          kind,
          path: op.relativePath,
          returnedLinkId: updated?.linkId ?? null,
          returnedExtractedText: updated?.extractedText ?? null,
        });
        break;
      }
      case 'needs-rechunk-probe': {
        // `rescanDatabaseMountPoint`'s own predicate, read back over every link.
        const midb = getRawMountIndexDatabase();
        if (!midb) throw new Error('mount-index DB handle unavailable');
        const rows = midb
          .prepare(
            `SELECT relativePath, chunkCount, conversionStatus FROM doc_mount_file_links
             WHERE mountPointId = ? ORDER BY relativePath`
          )
          .all(storeId) as Array<{
          relativePath: string;
          chunkCount: number;
          conversionStatus: string;
        }>;
        results.push({
          op: i,
          kind,
          needsRechunk: rows
            .filter((r) => r.chunkCount === 0 || r.conversionStatus !== 'converted')
            .map((r) => r.relativePath),
        });
        break;
      }
      default:
        throw new Error(`unknown op kind: ${kind}`);
    }
  }

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable (degraded open?)');
  const dumpTable = (table: string, orderBy: string) => {
    const columns = (midb.pragma(`table_info(${table})`) as Array<{ name: string }>).map(
      (c) => c.name
    );
    const rawRows = midb.prepare(`SELECT * FROM ${table}`).all() as Array<
      Record<string, unknown>
    >;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  const files = dumpTable('doc_mount_files', 'sha256');
  const documents = dumpTable('doc_mount_documents', 'contentSha256');
  const linkRows = dumpTable('doc_mount_file_links', 'relativePath');
  const blobRows = dumpTable('doc_mount_blobs', 'sha256');

  const chunkColumns = (
    midb.pragma('table_info(doc_mount_chunks)') as Array<{ name: string }>
  ).map((c) => c.name);
  const chunkRows = midb
    .prepare(
      "SELECT c.*, COALESCE(l.relativePath, '') || '#' || printf('%05d', CAST(c.chunkIndex AS INTEGER)) AS sortKey \
       FROM doc_mount_chunks c \
       LEFT JOIN doc_mount_file_links l ON l.id = c.linkId"
    )
    .all() as Array<Record<string, unknown>>;
  const chunks = canonicalizeRows({
    table: 'doc_mount_chunks',
    columns: [...chunkColumns, 'sortKey'],
    rawRows: chunkRows,
    orderBy: 'sortKey',
  });

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(
    JSON.stringify({
      case: 'doc-mount-write-metadata',
      results,
      files,
      documents,
      links: linkRows,
      chunks,
      blobs: blobRows,
    }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`doc-mount-write-metadata oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
