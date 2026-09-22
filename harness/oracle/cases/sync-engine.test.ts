/**
 * @jest-environment node
 *
 * Tier-2 ORACLE — `syncMountPoint` end to end against a real database-backed
 * store (real SQLite, real repositories, the real write chokepoints) and a real
 * directory, over the shared scenario corpus
 * `harness/oracle/fixtures/sync-engine-scenarios.json`.
 *
 * The corpus is DATA, read by this driver and by the Rust one, so the two sides
 * run the same script rather than two transcriptions of it. It covers the ground
 * of v4's own `engine.integration.test.ts` and the properties a planner test
 * cannot reach — above all **a second run is a no-op**, which is the whole
 * contract: if the appliers and the walks disagree about one timestamp or one
 * trailing newline, the sync oscillates for ever and nobody notices until it has
 * been running nightly for a month.
 *
 * ## The one declared seam: the post-write re-index
 *
 * v4's own integration test mocks `reindexSingleFile` and
 * `reindexLinkGroupSiblings`, and this driver mocks them the same way — the real
 * hook drags in the chunker and the embedding scheduler, none of which is what
 * the sync is being measured on. v5 has no mock seam there and runs the real
 * hook, so `doc_mount_chunks` and the links' `chunkCount` are EXCLUDED from the
 * comparand on both sides, and the Rust family carries its own assertion that
 * the sync issues no chunk SQL of its own. What the sync does to chunks is
 * `reindexAfterDatabaseWrite`'s behaviour, which is P4.D209's differential, not
 * this one's.
 *
 * Every clock in the corpus is explicit, so nothing here is normalized except
 * `elapsedMs` (wall time), the target path (a per-run tmpdir), and the
 * manifest's `lastSyncAt` (the one field the engine mints from `now`).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores
 * `.claude/` paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-sync-engine-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/sync-engine.test.ts"                "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/sync-engine-scenarios.json"      "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-sync-engine.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- sync-engine
 */

import { promises as fs } from 'fs';
import fsSync from 'fs';
import os from 'os';
import path from 'path';
import { createHash } from 'crypto';
import { fileURLToPath } from 'url';

function loadDriver(): any {
  const here = process.cwd();
  try {
    return require(
      path.join(here, 'packages', 'quilltap', 'node_modules', 'better-sqlite3-multiple-ciphers'),
    );
  } catch {
    return require(path.join(here, 'node_modules', 'better-sqlite3'));
  }
}
const Database = loadDriver();

jest.mock('@/lib/repositories/factory');
jest.mock('@/lib/mount-index/db-store-events', () => ({
  emitDocumentWritten: jest.fn(),
  emitDocumentDeleted: jest.fn(),
  emitDocumentMoved: jest.fn(),
}));
jest.mock('@/lib/doc-edit/reindex-file', () => ({
  reindexSingleFile: jest.fn().mockResolvedValue(undefined),
}));
jest.mock('@/lib/mount-index/link-groups', () => ({
  reindexLinkGroupSiblings: jest.fn().mockResolvedValue(0),
}));
jest.mock('@/lib/mount-index/character-vault', () => ({
  getArchivedCharacterVaultMountPointIds: jest.fn().mockResolvedValue([]),
}));

import { logger } from '@/lib/logger';
import { DocMountFileLinksRepository } from '@/lib/database/repositories/doc-mount-file-links.repository';
import { DocMountFilesRepository } from '@/lib/database/repositories/doc-mount-files.repository';
import { DocMountFoldersRepository } from '@/lib/database/repositories/doc-mount-folders.repository';
import { DocMountDocumentsRepository } from '@/lib/database/repositories/doc-mount-documents.repository';
import { DocMountBlobsRepository } from '@/lib/database/repositories/doc-mount-blobs.repository';
import { DocMountChunksRepository } from '@/lib/database/repositories/doc-mount-chunks.repository';
import { deleteDatabaseFolder } from '@/lib/mount-index/database-store';
import { syncMountPoint, SyncRefusedError } from '@/lib/mount-index/sync';
import { ManifestMismatchError } from '@/lib/mount-index/sync/manifest';
import type { DocMountPoint } from '@/lib/schemas/mount-index.types';

const getRepositoriesMock = jest.requireMock('@/lib/repositories/factory')
  .getRepositories as jest.Mock;
const archivedMock = jest.requireMock('@/lib/mount-index/character-vault')
  .getArchivedCharacterVaultMountPointIds as jest.Mock;

const MOUNT_ID = 'aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee';

interface Corpus {
  constants: Record<string, string>;
  scenarios: Array<{
    id: string;
    store?: Record<string, unknown>;
    steps: Array<Record<string, any>>;
  }>;
}

function corpusPath(): string {
  const here = path.dirname(fileURLToPath(import.meta.url));
  return path.join(here, '..', 'fixtures', 'sync-engine-scenarios.json');
}

const shaOf = (b: Buffer) => createHash('sha256').update(b).digest('hex');

const rows: string[] = [];
const emit = (row: unknown) => rows.push(JSON.stringify(row));

async function run(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  const corpus: Corpus = JSON.parse(fsSync.readFileSync(corpusPath(), 'utf-8'));
  const K = corpus.constants;

  /** A corpus token (`T1`, `PNG`, …) resolved against the constants block. */
  const clock = (token: string | undefined): string | undefined =>
    token === undefined ? undefined : (K[token] ?? token);
  const bytesOf = (token: string): Buffer =>
    Buffer.from(K[`${token}_BASE64`] ?? token, 'base64');

  for (const scenario of corpus.scenarios) {
    jest.spyOn(logger, 'warn').mockImplementation(() => {});
    jest.spyOn(logger, 'debug').mockImplementation(() => {});
    jest.spyOn(logger, 'info').mockImplementation(() => {});

    const db = new Database(':memory:');
    db.pragma('foreign_keys = ON');
    (globalThis as Record<string, unknown>).__quilltapMountIndexDatabase = db;
    (globalThis as Record<string, unknown>).__quilltapMountIndexDegraded = false;

    const links = new DocMountFileLinksRepository();
    const blobs = new DocMountBlobsRepository();
    const documents = new DocMountDocumentsRepository();
    const folders = new DocMountFoldersRepository();
    const repos = {
      docMountFileLinks: links,
      docMountFiles: new DocMountFilesRepository(),
      docMountFolders: folders,
      docMountDocuments: documents,
      docMountBlobs: blobs,
      docMountChunks: new DocMountChunksRepository(),
    };
    getRepositoriesMock.mockReturnValue(repos);
    // Mint the tables.
    await repos.docMountFiles.findBySha256('seed');
    await repos.docMountFolders.findByMountPointId('seed');
    await repos.docMountDocuments.findByFileId('seed');
    await repos.docMountBlobs.findByFileId('seed');
    await repos.docMountChunks.findByLinkId('seed');

    const over = scenario.store ?? {};
    archivedMock.mockResolvedValue(over.archived ? [MOUNT_ID] : []);
    const store = {
      id: MOUNT_ID,
      name: 'Lore',
      basePath: (over.basePath as string) ?? '',
      mountType: (over.mountType as string) ?? 'database',
      storeType: (over.storeType as string) ?? 'documents',
      includePatterns: [],
      excludePatterns: ['node_modules'],
      enabled: true,
      scanStatus: (over.scanStatus as string) ?? 'idle',
      conversionStatus: (over.conversionStatus as string) ?? 'idle',
      fileCount: 0,
      chunkCount: 0,
      totalSizeBytes: 0,
      createdAt: '2026-01-01T00:00:00.000Z',
      updatedAt: '2026-01-01T00:00:00.000Z',
    } as unknown as DocMountPoint;

    const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'qt-sync-engine-'));
    const reports: unknown[] = [];
    let failure: string | null = null;

    try {
      for (const step of scenario.steps) {
        const abs = (p: string) => path.join(dir, p);
        switch (step.op) {
          case 'seedDoc': {
            const content = step.content as string;
            await links.linkDocumentContent({
              mountPointId: MOUNT_ID,
              relativePath: step.path,
              fileName: path.posix.basename(step.path),
              folderId: null,
              fileType: 'markdown',
              content,
              contentSha256: shaOf(Buffer.from(content, 'utf-8')),
              plainTextLength: content.length,
              fileSizeBytes: Buffer.byteLength(content, 'utf-8'),
              lastModified: clock(step.lastModified),
              createdAt: clock(step.createdAt),
            });
            break;
          }
          case 'seedBlob': {
            const data = bytesOf(step.bytes);
            await links.linkBlobContent({
              mountPointId: MOUNT_ID,
              relativePath: step.path,
              fileName: path.posix.basename(step.path),
              folderId: null,
              fileType: 'blob',
              originalFileName: path.posix.basename(step.path),
              originalMimeType: 'image/png',
              storedMimeType: 'image/png',
              sha256: shaOf(data),
              data,
              lastModified: clock(step.lastModified),
              createdAt: clock(step.createdAt),
              ...(step.description !== undefined ? { description: step.description } : {}),
            });
            break;
          }
          case 'seedFolder': {
            const { ensureFolderPath } = await import('@/lib/mount-index/folder-paths');
            await ensureFolderPath(MOUNT_ID, step.path);
            break;
          }
          case 'setDescription': {
            const link = await links.findByMountPointAndPath(MOUNT_ID, step.path);
            const blob = await blobs.findByFileId(link!.fileId);
            await blobs.updateDescription(blob!.id, step.text, link!.id);
            break;
          }
          case 'deleteLink': {
            const link = await links.findByMountPointAndPath(MOUNT_ID, step.path);
            await links.deleteWithGC(link!.id);
            break;
          }
          case 'deleteFolder': {
            await deleteDatabaseFolder(MOUNT_ID, step.path);
            break;
          }
          case 'writeDisk':
          case 'writeDiskBytes': {
            const body =
              step.op === 'writeDisk'
                ? Buffer.from(step.body as string, 'utf-8')
                : bytesOf(step.bytes);
            await fs.mkdir(path.dirname(abs(step.path)), { recursive: true });
            await fs.writeFile(abs(step.path), body);
            const when = new Date(clock(step.mtime)!);
            await fs.utimes(abs(step.path), when, when);
            break;
          }
          case 'rmDisk': {
            await fs.rm(abs(step.path), { force: true, recursive: true });
            break;
          }
          case 'symlinkDisk': {
            await fs.symlink(step.target, abs(step.path));
            break;
          }
          case 'sync': {
            const options = {
              targetPath: dir,
              dryRun: false,
              direction: 'both',
              prefer: 'newer',
              propagateDeletes: true,
              useManifest: true,
              ...(step.options ?? {}),
            };
            try {
              const report = await syncMountPoint(store, options as never);
              reports.push(normalizeReport(report, dir));
            } catch (err) {
              if (!step.expectRefusal && !(err instanceof SyncRefusedError) &&
                  !(err instanceof ManifestMismatchError)) {
                throw err;
              }
              reports.push({
                refused: true,
                name: (err as Error).name,
                code: (err as any).code ?? null,
                message: (err as Error).message,
              });
            }
            break;
          }
          default:
            throw new Error(`unknown corpus op \`${step.op}\``);
        }
      }
    } catch (err) {
      failure = err instanceof Error ? `${err.name}: ${err.message}` : String(err);
    }

    // ONE normalizer per scenario, so the id remap is scenario-scoped and the
    // same token means the same row across the report, the store and the disk.
    const scrub = makeNormalizers(K);
    emit(
      scrub({
        id: scenario.id,
        reports,
        failure,
        store: failure ? null : await dumpStore(links, documents, blobs, folders),
        disk: failure ? null : await dumpDisk(dir),
      }),
    );

    jest.restoreAllMocks();
    try {
      db.close();
    } catch {
      /* ignore */
    }
    (globalThis as Record<string, unknown>).__quilltapMountIndexDatabase = undefined;
    await fs.rm(dir, { recursive: true, force: true });
  }

  fsSync.writeFileSync(outPath, rows.join('\n') + '\n');
  process.stderr.write(`sync-engine oracle wrote ${outPath} (${rows.length} rows)\n`);
}

/**
 * Two normalizers, applied identically on both sides.
 *
 * `normalizeClocks` replaces any ISO-8601 instant that is NOT one of the
 * corpus's own constants with `<minted>`. The corpus fixes every clock it cares
 * about, so what survives here is exactly the propagation the rules are about;
 * what is replaced is `now` — a folder row's `updatedAt` from `ensureFolderPath`,
 * a blob's `descriptionUpdatedAt` from the seed — which two processes can never
 * agree on.
 *
 * `remapIds` turns minted UUIDs into `<id-1>`, `<id-2>`, … in first-seen order,
 * which keeps the IDENTITY relationships (the same link named twice is the same
 * token) while dropping the values, exactly as the tier-2 remap machinery does.
 */
function makeNormalizers(constants: Record<string, string>) {
  const known = new Set(Object.values(constants));
  const ISO = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z$/;
  const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
  const ids = new Map<string, string>();
  const scrub = (v: unknown): unknown => {
    if (typeof v === 'string') {
      if (ISO.test(v) && !known.has(v)) return '<minted>';
      if (UUID.test(v)) {
        if (!ids.has(v)) ids.set(v, `<id-${ids.size + 1}>`);
        return ids.get(v);
      }
      return v;
    }
    if (Array.isArray(v)) return v.map(scrub);
    if (v && typeof v === 'object') {
      const out: Record<string, unknown> = {};
      for (const [k, val] of Object.entries(v as Record<string, unknown>)) out[k] = scrub(val);
      return out;
    }
    return v;
  };
  return scrub;
}

/** The target path is a per-run tmpdir and `elapsedMs` is wall time. */
function normalizeReport(report: any, dir: string): unknown {
  return {
    storeId: report.storeId,
    storeName: report.storeName,
    targetPath: report.targetPath === dir ? '<target>' : report.targetPath,
    dryRun: report.dryRun,
    actions: report.actions,
    summary: report.summary,
    warnings: (report.warnings as string[]).map((w) => w.split(dir).join('<target>')),
    elapsedMs: '<elapsed>',
  };
}

/**
 * The store's rows, in a stable order.
 *
 * `chunkCount` and `doc_mount_chunks` are OUT by declared seam (see the header):
 * the re-index hook is mocked here and real on v5's side.
 */
async function dumpStore(
  links: DocMountFileLinksRepository,
  documents: DocMountDocumentsRepository,
  blobs: DocMountBlobsRepository,
  folders: DocMountFoldersRepository,
): Promise<unknown> {
  const all = await links.findByMountPointId(MOUNT_ID);
  all.sort((a, b) => (a.relativePath < b.relativePath ? -1 : a.relativePath > b.relativePath ? 1 : 0));
  const linkRows = [];
  for (const link of all) {
    const doc = await documents.findByFileId(link.fileId);
    const blob = await blobs.findByFileId(link.fileId);
    const data = blob ? await blobs.readDataByFileId(link.fileId) : null;
    linkRows.push({
      relativePath: link.relativePath,
      fileName: link.fileName,
      fileType: link.fileType,
      sha256: link.sha256,
      fileSizeBytes: link.fileSizeBytes,
      description: link.description ?? '',
      lastModified: link.lastModified,
      createdAt: link.createdAt,
      originalMimeType: link.originalMimeType ?? null,
      storedMimeType: blob?.storedMimeType ?? null,
      // What the store actually holds, hashed rather than inlined.
      contentSha256: doc
        ? createHash('sha256').update(Buffer.from(doc.content, 'utf-8')).digest('hex')
        : data
          ? createHash('sha256').update(data).digest('hex')
          : null,
      kind: doc ? 'document' : blob ? 'blob' : 'none',
    });
  }
  const folderRows = (await folders.findByMountPointId(MOUNT_ID))
    .map((f) => f.path)
    .filter((p) => p)
    .sort();
  return { links: linkRows, folders: folderRows };
}

/** The directory tree: every path, its kind, and its bytes' sha. */
async function dumpDisk(dir: string): Promise<unknown> {
  const out: Array<Record<string, unknown>> = [];
  async function walk(rel: string): Promise<void> {
    const entries = await fs.readdir(path.join(dir, rel || '.'), { withFileTypes: true });
    entries.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
    for (const entry of entries) {
      const relPath = rel ? `${rel}/${entry.name}` : entry.name;
      if (entry.isSymbolicLink()) {
        out.push({ path: relPath, kind: 'symlink' });
        continue;
      }
      if (entry.isDirectory()) {
        out.push({ path: relPath, kind: 'dir' });
        await walk(relPath);
        continue;
      }
      const bytes = await fs.readFile(path.join(dir, relPath));
      const stat = await fs.stat(path.join(dir, relPath));
      const row: Record<string, unknown> = {
        path: relPath,
        kind: 'file',
        sha256: createHash('sha256').update(bytes).digest('hex'),
        sizeBytes: bytes.length,
        lastModified: stat.mtime.toISOString(),
      };
      // The manifest is the one file whose CONTENT is part of the contract, and
      // its `lastSyncAt` is the one field the engine mints from `now`.
      if (relPath === '.quilltap-sync.json') {
        delete row.sha256;
        delete row.sizeBytes;
        delete row.lastModified;
        try {
          const parsed = JSON.parse(bytes.toString('utf-8'));
          parsed.lastSyncAt = '<lastSyncAt>';
          row.manifest = parsed;
        } catch {
          // A manifest that does not parse is a corpus state in its own right
          // (the run refused, or ignored it) — reported, not thrown on.
          row.manifestRaw = bytes.toString('utf-8');
        }
      }
      // A sidecar's text is contractual too, and short.
      if (relPath.toLowerCase().endsWith('.description.md')) {
        row.text = bytes.toString('utf-8');
      }
      out.push(row);
    }
  }
  await walk('');
  return out;
}

test('sync-engine oracle', async () => {
  await run();
});
