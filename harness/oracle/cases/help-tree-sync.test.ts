/**
 * @jest-environment node
 *
 * P4.9I2A CONTENT ORACLE — the shipped help tree through v4's REAL
 * `ensureHelpDocsSynced()` (`lib/help/help-doc-sync.ts`) on an EMPTY
 * `help_docs` table, with `cwd` = the v4 checkout root so `HELP_DIR =
 * join(process.cwd(), 'help')` is v4's own 120-file tree at the pin.
 *
 * The Rust side (`help_tree_equivalence`) runs `ensure_help_docs_synced` over the
 * EMBEDDED table `quilltap-host` compiled from the vendored `<repo>/help/`, so one
 * diff proves at once: the vendored bytes, the front-matter parser, the H1/`title:`
 * title rule, the `url:` read, the content hash, and the section chunker over
 * every real file. A one-byte edit to ONE vendored file reddens it.
 *
 * Emits one line:
 *   { kind: 'tree', count, walkOrder: [path…], docs: [{path,title,url,content,
 *     contentHash,hasEmbedding}…] (sorted by path), chunks: [{docPath,chunkIndex,
 *     heading,content}…] (sorted by docPath, chunkIndex), jobs: N }
 *
 * `walkOrder` is the `help_docs` rowid order — v4's `readdirSync` order on this
 * machine, which the embedded table must reproduce (the build script mirrors the
 * walker; both read the same directory on the same filesystem).
 *
 * The DB is built inline (v4's own `ensureCollection` for every table the ensure
 * touches + one user; NO embedding profile, so the embedding enqueue backs off
 * and `jobs` is 0 on both sides) — no committed fixture is needed.
 *
 * Run (Node 24, from the v4 checkout — mirror to /tmp; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}   # the v5 checkout (or your worktree)
 *   cd ~/source/quilltap-server
 *   TMPO=/tmp/qt-help-tree-oracle; rm -rf $TMPO; mkdir -p $TMPO/cases
 *   cp $V5W/harness/oracle/cases/help-tree-sync.test.ts $TMPO/cases/
 *   QT_ORACLE_OUT=/tmp/oracle-help-tree.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots $TMPO/cases -- help-tree-sync
 */

import { describe, it } from '@jest/globals';
import { join } from 'node:path';
import * as fs from 'node:fs';
import { tmpdir } from 'node:os';

const PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const USER_ID = 'e18e05bc-63e8-4539-8a85-719b7a508850';
const TS = '2026-05-01T00:00:00.000Z';

describe('help-doc-sync: the shipped tree', () => {
  it('records the synced help_docs + chunks for the whole help/ tree', async () => {
    const outPath = process.env.QT_ORACLE_OUT;
    if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
    if (!fs.existsSync(join(process.cwd(), 'help'))) {
      throw new Error(`no help/ under cwd ${process.cwd()} — run from the v4 checkout root`);
    }

    const scratch = fs.mkdtempSync(join(tmpdir(), 'qt-help-tree-oracle-'));
    fs.mkdirSync(join(scratch, 'data'), { recursive: true });
    const workMain = join(scratch, 'tree-main.db');

    process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
    process.env.SQLITE_PATH = workMain;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    jest.resetModules();
    const cipherDriverPath = join(
      process.cwd(),
      'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
    );
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/repositories/factory', () =>
      jest.requireActual('@/lib/repositories/factory'),
    );
    // `enqueueJob()` resolves the job-child entry relative to cwd; a no-op keeps
    // the in-process runner out of the dumps (the help-sync-ensure recipe).
    jest.doMock('@/lib/background-jobs/processor', () => {
      const actual = jest.requireActual('@/lib/background-jobs/processor');
      return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
    });

    const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import(
      '@/lib/database/manager'
    );
    const { HelpDocSchema } = await import('@/lib/schemas/help-doc.types');
    const { HelpDocChunkSchema } = await import('@/lib/schemas/help-doc-chunk.types');
    const { EmbeddingStatusSchema } = await import('@/lib/schemas/embedding-job.types');
    const { BackgroundJobSchema } = await import('@/lib/schemas/job.types');
    const { UserSchema } = await import('@/lib/schemas/auth.types');
    const { EmbeddingProfileSchema } = await import('@/lib/schemas/profile.types');

    await initializeDatabase();
    await ensureCollection('help_docs', HelpDocSchema);
    await ensureCollection('help_doc_chunks', HelpDocChunkSchema);
    await ensureCollection('embedding_status', EmbeddingStatusSchema);
    await ensureCollection('background_jobs', BackgroundJobSchema);
    await ensureCollection('users', UserSchema);
    await ensureCollection('embedding_profiles', EmbeddingProfileSchema);
    const { getRepositories } = await import('@/lib/repositories/factory');
    await getRepositories().users.create(
      { username: 'friday' } as never,
      { id: USER_ID, createdAt: TS, updatedAt: TS } as never,
    );

    const { ensureHelpDocsSynced } = await import('@/lib/help/help-doc-sync');
    const { logger } = await import('@/lib/logger');
    // The sync is best-effort BY DESIGN (per-file failures count as `failed` and
    // log); fail loudly instead of pinning a harness artifact.
    const swallowed: string[] = [];
    const realError = logger.error.bind(logger);
    (logger as unknown as { error: (...a: unknown[]) => void }).error = (...args: unknown[]) => {
      const msg = String(args[0] ?? '');
      if (msg.includes('[HelpDocSync]')) swallowed.push(JSON.stringify(args));
      return (realError as (...a: unknown[]) => void)(...args);
    };

    await ensureHelpDocsSynced();
    if (swallowed.length > 0) {
      throw new Error(
        `ensureHelpDocsSynced swallowed ${swallowed.length} error(s):\n${swallowed.join('\n')}`,
      );
    }

    const rows =
      (await rawQuery<Array<Record<string, unknown>>>(
        'SELECT rowid AS rid, id, path, title, url, content, contentHash, hex(embedding) AS embeddingHex ' +
          'FROM help_docs ORDER BY rowid ASC',
        [],
      )) ?? [];
    const pathById = new Map<string, string>();
    for (const r of rows) pathById.set(String(r.id), String(r.path));
    const walkOrder = rows.map((r) => String(r.path));
    const docs = rows
      .map((r) => ({
        path: String(r.path),
        title: String(r.title),
        url: String(r.url),
        content: String(r.content),
        contentHash: String(r.contentHash),
        hasEmbedding: String(r.embeddingHex ?? '') !== '',
      }))
      .sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));

    const chunkRows =
      (await rawQuery<Array<Record<string, unknown>>>(
        'SELECT docId, chunkIndex, heading, content FROM help_doc_chunks',
        [],
      )) ?? [];
    const chunks = chunkRows
      .map((r) => ({
        docPath: pathById.get(String(r.docId)) ?? `<unknown:${String(r.docId)}>`,
        chunkIndex: Number(r.chunkIndex),
        heading: r.heading == null ? null : String(r.heading),
        content: String(r.content),
      }))
      .sort((a, b) =>
        a.docPath < b.docPath ? -1 : a.docPath > b.docPath ? 1 : a.chunkIndex - b.chunkIndex,
      );

    const jobRows =
      (await rawQuery<Array<Record<string, unknown>>>(
        'SELECT COUNT(*) AS n FROM background_jobs',
        [],
      )) ?? [];
    const jobs = Number(jobRows[0]?.n ?? 0);

    (logger as unknown as { error: unknown }).error = realError;
    await closeDatabase();

    fs.writeFileSync(
      outPath,
      JSON.stringify({ kind: 'tree', count: docs.length, walkOrder, docs, chunks, jobs }) + '\n',
    );
    process.stderr.write(`help-tree-sync oracle wrote ${outPath} (${docs.length} docs, ${chunks.length} chunks)\n`);
  });
});
