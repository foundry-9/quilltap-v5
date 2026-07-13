/**
 * The committed mount-index file-ops server-surface fixture builder (P4.6v) —
 * the shared test-pepper substrate for the mount-read / mount-list differentials
 * (`mount_read_equivalence`) and later mount-ops units.
 *
 * Bakes, via v4's REAL repos + `storeMountFile` (all ids/timestamps pinned so
 * both differential sides read identical rows — reads diff with no remap):
 *   - MP_DB  — a database/documents mount: two text docs (`notes/intro.md`,
 *              `reference.txt`) + one blob with a description (`images/logo.png`)
 *              + one garbage-PDF blob (`docs/report.pdf`, extraction untouched:
 *              conversionStatus 'pending' / extractionStatus 'none' — the
 *              reindex/scan extraction-state substrate) + three chunks (two with
 *              REAL builtin TF-IDF embeddings, one with a NULL embedding for the
 *              embed-enqueue differential), plus `doc_mount_folders` rows,
 *   - MP_EMPTY — an empty database mount (no files/folders),
 *   - MP_FS  — a filesystem mount, excludePatterns ['node_modules','*.tmp'],
 *   - MP_OBS — an obsidian mount, excludePatterns [].
 * The MAIN db additionally carries the BUILTIN TF-IDF `embedding_profiles` row
 * (EP_BUILTIN, default) + a fitted `tfidf_vocabularies` row over the chunk
 * corpus (the P4.6s fixture precedent) so `mountEmbed`/`mountSemanticSearch`
 * run the real builtin embedding path deterministically on both sides.
 * MP_FS / MP_OBS store the SENTINEL basePath `__MOUNTS_FS_TREE__`; the oracle and
 * the Rust test each rewrite it to their per-side temp COPY of the committed
 * `mounts-fs-tree/` before reading (nothing writes the committed tree).
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MOUNTS_MAIN=$W/crates/quilltap-web/tests/fixtures/mounts-main.db \
 *   QT_FIXTURE_MOUNTS_MOUNT=$W/crates/quilltap-web/tests/fixtures/mounts-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-mounts-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
}

// ── Pinned mount-point ids (shared verbatim with the Rust differential) ──────
const MP_DB = 'b6000000-0000-4000-8000-000000000001';
const MP_EMPTY = 'b6000000-0000-4000-8000-000000000002';
const MP_FS = 'b6000000-0000-4000-8000-000000000003';
const MP_OBS = 'b6000000-0000-4000-8000-000000000004';
const IDX_CHUNK = 'b7000000-0000-4000-8000-000000000001';
const IDX_CHUNK2 = 'b7000000-0000-4000-8000-000000000002';
const IDX_CHUNK3 = 'b7000000-0000-4000-8000-000000000003';
const EP_BUILTIN = 'b8000000-0000-4000-8000-000000000001';

// The sentinel the test sides rewrite to their own fs-tree copy.
const FS_TREE_SENTINEL = '__MOUNTS_FS_TREE__';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'mounts.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_MOUNTS_MAIN;
  const mountOut = process.env.QT_FIXTURE_MOUNTS_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_MOUNTS_MAIN and QT_FIXTURE_MOUNTS_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mounts-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
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
  } = await import('@/lib/schemas/mount-index.types');
  const { EmbeddingProfileSchema } = await import('@/lib/schemas/profile.types');

  await initializeDatabase();

  // Register the provider plugins (incl. the BUILTIN embeddings provider) so the
  // real builtin embedding path resolves at generate time (P4.6s precedent).
  const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
  await initializePlugins();
  await ensureCollection('embedding_profiles', EmbeddingProfileSchema);

  // Materialize the mount-index schema on the fresh fixture. The `data BLOB`
  // table (`doc_mount_blobs`) can't be produced by generateDDL — storeMountFile's
  // blob branch triggers the repo's hand-written DDL lazily.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  // Materialize doc_mount_blobs via the repo's hand-written DDL (a harmless read
  // triggers its lazy table-init) so storeMountFile's blob branch can insert.
  const { DocMountBlobsRepository } = await import(
    '@/lib/database/repositories/doc-mount-blobs.repository'
  );
  await new DocMountBlobsRepository().findByFileId('00000000-0000-4000-8000-000000000000');

  const repos = getRepositories();
  const TS = spec.seedTimestamp;

  // A single user so the MAIN partition is a valid (non-empty) encrypted DB the
  // Db runtime can open — the mount-index read path never touches it otherwise.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  // The BUILTIN default TF-IDF embedding profile (the P4.6s fixture precedent) —
  // `mountEmbed` resolves it for the enqueue payload; `mountSemanticSearch`
  // embeds queries through it.
  await repos.embeddingProfiles.create(
    {
      userId: spec.userId,
      name: 'Built-in TF-IDF',
      provider: 'BUILTIN',
      apiKeyId: null,
      baseUrl: null,
      modelName: 'tfidf-bm25-v1',
      dimensions: null,
      truncateToDimensions: null,
      normalizeL2: true,
      isDefault: true,
      tags: [],
    } as never,
    { id: EP_BUILTIN, createdAt: TS, updatedAt: TS } as never,
  );

  const mkMount = async (
    id: string,
    name: string,
    mountType: string,
    storeType: string,
    basePath: string,
    excludePatterns: string[],
  ) =>
    repos.docMountPoints.create(
      {
        name,
        basePath,
        mountType,
        storeType,
        includePatterns: [],
        excludePatterns,
        enabled: true,
        lastScannedAt: null,
        scanStatus: 'idle',
        lastScanError: null,
        conversionStatus: 'idle',
        conversionError: null,
        fileCount: 0,
        chunkCount: 0,
        totalSizeBytes: 0,
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );

  await mkMount(MP_DB, 'DB Store', 'database', 'documents', '', []);
  await mkMount(MP_EMPTY, 'Empty Store', 'database', 'documents', '', []);
  await mkMount(MP_FS, 'FS Mount', 'filesystem', 'documents', FS_TREE_SENTINEL, [
    'node_modules',
    '*.tmp',
  ]);
  await mkMount(MP_OBS, 'Obsidian Vault', 'obsidian', 'documents', FS_TREE_SENTINEL, []);

  const { storeMountFile } = await import('@/lib/mount-index/store-file');

  // MP_DB text docs.
  await storeMountFile({
    mountPointId: MP_DB,
    relativePath: 'notes/intro.md',
    data: Buffer.from('# Intro\n\nFirst body line.\nSecond body line.\nThird body line.\n', 'utf8'),
    originalMimeType: 'text/markdown',
    transcodeImages: false,
    extractText: false,
    enqueueEmbedding: false,
  } as never);
  await storeMountFile({
    mountPointId: MP_DB,
    relativePath: 'reference.txt',
    data: Buffer.from('reference alpha\nreference beta\n', 'utf8'),
    originalMimeType: 'text/plain',
    transcodeImages: false,
    extractText: false,
    enqueueEmbedding: false,
  } as never);
  // MP_DB blob (with a description).
  await storeMountFile({
    mountPointId: MP_DB,
    relativePath: 'images/logo.png',
    data: Buffer.from([137, 80, 78, 71, 13, 10, 26, 10, 0, 1, 2, 3, 4, 5, 6, 7]),
    originalMimeType: 'image/png',
    description: 'The company logo.',
    transcodeImages: false,
    extractText: false,
    enqueueEmbedding: false,
  } as never);

  // MP_DB garbage-PDF blob — extraction deliberately untouched at build time
  // (extractText:false), leaving conversionStatus 'pending' / extractionStatus
  // 'none': the reindex / scan extraction-state substrate (both sides fail the
  // extraction at runtime — v4's pdf-parse on garbage vs v5's refusing seam —
  // with the error TEXT normalized by the differential).
  await storeMountFile({
    mountPointId: MP_DB,
    relativePath: 'docs/report.pdf',
    data: Buffer.from('not really a pdf, honest', 'utf8'),
    originalMimeType: 'application/pdf',
    transcodeImages: false,
    extractText: false,
    enqueueEmbedding: false,
  } as never);

  // Chunks: one on the intro doc, two on reference.txt (pinned ids). IDX_CHUNK +
  // IDX_CHUNK2 get REAL builtin TF-IDF embeddings below; IDX_CHUNK3 keeps a NULL
  // embedding (the mountEmbed enqueue target).
  const linkIdFor = (rel: string): string => {
    const row = midb
      .prepare(
        'SELECT id FROM doc_mount_file_links WHERE mountPointId = ? AND relativePath = ? LIMIT 1',
      )
      .get(MP_DB, rel) as { id: string } | undefined;
    if (!row) throw new Error(`link not found after storeMountFile: ${rel}`);
    return row.id;
  };
  const introLinkId = linkIdFor('notes/intro.md');
  const refLinkId = linkIdFor('reference.txt');
  const insertChunk = midb.prepare(
    'INSERT INTO doc_mount_chunks (id, linkId, mountPointId, chunkIndex, content, tokenCount, headingContext, embedding, createdAt, updatedAt) ' +
      'VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)',
  );
  insertChunk.run(IDX_CHUNK, introLinkId, MP_DB, 0, 'First body line.', 4, 'Intro', null, TS, TS);
  insertChunk.run(IDX_CHUNK2, refLinkId, MP_DB, 0, 'reference alpha', 4, null, null, TS, TS);
  insertChunk.run(IDX_CHUNK3, refLinkId, MP_DB, 1, 'reference beta', 4, null, null, TS, TS);

  // Fit the BUILTIN vocabulary over the chunk corpus, then embed IDX_CHUNK +
  // IDX_CHUNK2 through v4's REAL builtin path (the P4.6s recipe) — deterministic
  // TF-IDF vectors both differential sides re-read.
  const plugin = await import('@/plugins/dist/qtap-plugin-builtin-embeddings');
  const TfIdfVectorizer = (plugin as { TfIdfVectorizer: new (b: boolean) => any }).TfIdfVectorizer;
  const vectorizer = new TfIdfVectorizer(true); // include bigrams (the refit default)
  vectorizer.fitCorpus(['First body line.', 'reference alpha', 'reference beta']);
  const state = vectorizer.getState();
  if (!state) throw new Error('TF-IDF fit produced no state');
  await repos.tfidfVocabularies.upsertByProfileId(EP_BUILTIN, {
    profileId: EP_BUILTIN,
    userId: spec.userId,
    vocabulary: JSON.stringify(state.vocabulary),
    idf: JSON.stringify(state.idf),
    avgDocLength: state.avgDocLength,
    vocabularySize: state.vocabularySize,
    includeBigrams: state.includeBigrams,
    fittedAt: state.fittedAt,
  } as never);
  const { generateEmbeddingForUser } = await import('@/lib/embedding/embedding-service');
  for (const [chunkId, content] of [
    [IDX_CHUNK, 'First body line.'],
    [IDX_CHUNK2, 'reference alpha'],
  ] as const) {
    const result = await generateEmbeddingForUser(content, spec.userId, EP_BUILTIN);
    await repos.docMountChunks.updateEmbedding(chunkId, result.embedding);
  }

  // (storeMountFile's ensureFolderPath already created the `notes` + `images` +
  // `docs` doc_mount_folders rows on MP_DB — the list route merges those.)

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(`built mounts fixture: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`mounts fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
