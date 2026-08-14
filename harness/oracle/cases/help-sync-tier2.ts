/**
 * P4.1b oracle case — the help-docs disk sync (v4 `lib/help/help-doc-sync.ts`
 * `syncHelpDocs`).
 *
 * Copies the pre-seeded fixture DB, `process.chdir`s into the committed
 * fixture root (`fixtures/help-sync/`, which contains `help/`) BEFORE
 * importing help-doc-sync — its `HELP_DIR = join(process.cwd(), 'help')` is
 * captured at module load — then drives v4's REAL `syncHelpDocs()` and emits:
 *   - a `summary` line: the returned `HelpDocSyncResult` with `changedIds`
 *     replaced by the corresponding row PATHS, sorted (walk order is
 *     readdir-dependent; paths are the stable identity);
 *   - a `help_docs` line: the full table (embedding as hex) ordered by path.
 * The Rust side copies the SAME fixture, walks the SAME committed help tree,
 * runs `sync_help_docs`, and diffs both lines (minted ids/timestamps
 * normalized sentinel-aware; the seeded 2020 sentinel must SURVIVE on the
 * unchanged row).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_HELP_MAIN=/tmp/qt-help-sync-main.db \
 *     $N/node --import tsx $V5/harness/oracle/cases/help-sync-tier2.ts > /tmp/oracle-help-sync.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedSentinel: string;
  helpDocChunkSeed: Array<{ id: string }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const fixtureRoot = join(here, '..', 'fixtures', 'help-sync');
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'help-sync.json'), 'utf8'),
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_HELP_MAIN;
  if (!fixtureMain || !existsSync(fixtureMain)) {
    throw new Error('QT_FIXTURE_HELP_MAIN must point at the seed fixture');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-help-sync-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'help-sync-main.db');
  copyFileSync(fixtureMain, workMain);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // HELP_DIR is captured at module load from process.cwd() — chdir FIRST.
  process.chdir(fixtureRoot);

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { syncHelpDocs } = await import('@/lib/help/help-doc-sync');

  await initializeDatabase();

  const result = await syncHelpDocs();

  const rows = (await rawQuery<Array<Record<string, unknown>>>(
    'SELECT id, title, path, url, content, contentHash, hex(embedding) AS embeddingHex, createdAt, updatedAt FROM help_docs ORDER BY path ASC',
    [],
  )) ?? [];

  // changedIds → paths (sorted) — walk order is not the differential's concern.
  const pathById = new Map<string, string>();
  for (const r of rows) pathById.set(String(r.id), String(r.path));
  const changedPaths = result.changedIds
    .map((id) => pathById.get(id) ?? `<unknown:${id}>`)
    .sort();

  process.stdout.write(
    JSON.stringify({
      kind: 'summary',
      totalOnDisk: result.totalOnDisk,
      created: result.created,
      updated: result.updated,
      unchanged: result.unchanged,
      deleted: result.deleted,
      failed: result.failed,
      chunksWritten: result.chunksWritten,
      changedPaths,
    }) + '\n',
  );
  process.stdout.write(JSON.stringify({ kind: 'help_docs', rows }) + '\n');

  // P4.D77 (v4 `24633026`): the section chunks the sync wrote. Chunk ids and
  // timestamps are minted on BOTH sides, so the row identity here is
  // (doc PATH, chunkIndex) — the pair the UNIQUE constraint uses. `embedding`
  // must be NULL on every row: `replaceForDoc` writes them empty and the
  // HELP_DOC job is what fills them.
  const chunkRows =
    (await rawQuery<Array<Record<string, unknown>>>(
      'SELECT c.id AS id, c.docId AS docId, c.chunkIndex AS chunkIndex, c.heading AS heading, ' +
        'c.content AS content, hex(c.embedding) AS embeddingHex, c.updatedAt AS updatedAt ' +
        'FROM help_doc_chunks c',
      [],
    )) ?? [];
  const seededChunkIds = new Set(spec.helpDocChunkSeed.map((c) => c.id));
  const chunks = chunkRows
    .map((r) => ({
      id: seededChunkIds.has(String(r.id)) ? String(r.id) : '<minted>',
      docPath: pathById.get(String(r.docId)) ?? `<unknown:${String(r.docId)}>`,
      chunkIndex: Number(r.chunkIndex),
      heading: r.heading == null ? null : String(r.heading),
      content: String(r.content),
      hasEmbedding: String(r.embeddingHex ?? '') !== '',
      updatedAt: String(r.updatedAt) === spec.seedSentinel ? '<sentinel>' : '<ts>',
    }))
    .sort((a, b) =>
      a.docPath < b.docPath ? -1 : a.docPath > b.docPath ? 1 : a.chunkIndex - b.chunkIndex,
    );
  process.stdout.write(JSON.stringify({ kind: 'help_doc_chunks', rows: chunks }) + '\n');

  // The prune's cascade: the retired doc's status rows must be gone, the
  // surviving doc's must remain (deleteByEntity is scoped AND a deleteMany).
  const statusRows =
    (await rawQuery<Array<Record<string, unknown>>>(
      'SELECT id, entityType, entityId, profileId, status FROM embedding_status ORDER BY id ASC',
      [],
    )) ?? [];
  process.stdout.write(JSON.stringify({ kind: 'embedding_status', rows: statusRows }) + '\n');

  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`help-sync oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
