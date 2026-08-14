/**
 * P4.D77 fixture EXTENDER — give a committed main-partition fixture the
 * `help_doc_chunks` table and the section rows its family's arms need
 * (v4 `24633026`).
 *
 * ## Why an extender rather than a rebuild
 *
 * Three committed families need this table (`embedding-generate`,
 * `embedding-remainder`, `embedding-profiles`), and none of them needs anything
 * else to move. Rebuilding a family's builder rewrites both its databases
 * wholesale and obliges re-running every sibling that reads them. So we open
 * the committed main database as it is and add ONE collection plus the pinned
 * rows through v4's REAL repository. The `ensureCollection` call is the only
 * DDL issued, and it creates a table that did not exist — it cannot alter an
 * existing one, which is what the extender idiom forbids.
 *
 * ## Which family gets what, and why
 *
 * Each family's own spec carries a `helpDocChunks` array, and each row's
 * `_note` states the arm it exists for. In summary:
 *
 *   - **`embedding-generate-jobs.json`** — the HELP_DOC job's new chunk pass:
 *     a failing chunk FIRST (so a fail-fast port is visible in the chunks
 *     that follow it), an already-embedded chunk whose composed text is a
 *     corpus HIT (so a port that re-embedded it overwrites a distinguishable
 *     vector rather than silently missing the canned map), and an ordinary
 *     chunk last.
 *   - **`embedding-remainder.json`** — the full-scope reindex's chunk wipe: one
 *     embedded and one un-embedded chunk on the ONE help doc the reindex's disk
 *     sync leaves alone (its content hash matches the committed tree), so their
 *     final state is the wipe's doing and not the sync's.
 *   - **`embedding-profiles-tier2.json`** — the reapply's fifth main table: an
 *     8-dimension vector against the Reapply Target profile's trunc=4, so the
 *     table appears in `perTable` with a real `truncated` count.
 *
 * Every id is PINNED, so the committed bytes stay reproducible. Re-running
 * against an already-extended fixture fails loudly on the primary-key conflict
 * rather than silently double-adding.
 *
 * Regenerate (Node 24, from the v4 checkout at the pin) — always against a
 * COPY, then copy over the committed file, then regenerate that family's
 * oracle:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   cd ~/source/quilltap-server
 *   F=$V5W/crates/quilltap-web/tests/fixtures
 *   cp $F/embedding-generate-main.db /tmp/qt-x.db
 *   QT_CHUNK_SPEC=embedding-generate-jobs.json QT_CHUNK_DB=/tmp/qt-x.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/extend-help-doc-chunks.ts
 *   cp /tmp/qt-x.db $F/embedding-generate-main.db
 * …and likewise with `embedding-remainder.json` → `embedding-remainder-main.db`
 * and `embedding-profiles-tier2.json` → `embedding-profiles-main.db`.
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ChunkSeed {
  id: string;
  docId: string;
  chunkIndex: number;
  heading: string | null;
  content: string;
  embedding: number[] | null;
}

interface Spec {
  testPepperBase64: string;
  /** The sentinel both sides recognize as "seeded, not written by the run". */
  seedTimestamp: string;
  helpDocChunks: ChunkSeed[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));

  const specName = process.env.QT_CHUNK_SPEC;
  if (!specName) {
    throw new Error('QT_CHUNK_SPEC must name a spec file in harness/oracle/fixtures');
  }
  const spec = JSON.parse(readFileSync(join(here, specName), 'utf8')) as Spec;
  if (!Array.isArray(spec.helpDocChunks) || spec.helpDocChunks.length === 0) {
    throw new Error(`${specName} carries no helpDocChunks array`);
  }
  if (!spec.seedTimestamp) {
    throw new Error(`${specName} carries no seedTimestamp`);
  }

  const target = process.env.QT_CHUNK_DB;
  if (!target || !existsSync(target)) {
    throw new Error('QT_CHUNK_DB must point at a COPY of the committed main fixture');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chunk-extend-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = target;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { HelpDocChunkSchema } = await import('@/lib/schemas/help-doc-chunk.types');
  const { HelpDocChunksRepository } = await import(
    '@/lib/database/repositories/help-doc-chunks.repository'
  );

  await initializeDatabase();
  await ensureCollection('help_doc_chunks', HelpDocChunkSchema);

  const repo = new HelpDocChunksRepository();
  for (const chunk of spec.helpDocChunks) {
    await repo.create(
      {
        docId: chunk.docId,
        chunkIndex: chunk.chunkIndex,
        heading: chunk.heading,
        content: chunk.content,
        embedding: chunk.embedding,
      } as never,
      { id: chunk.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp },
    );
  }

  await closeDatabase();
  process.stderr.write(
    `extended ${target} from ${specName} with help_doc_chunks ` +
      `(${spec.helpDocChunks.length} rows)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`help-doc-chunk fixture extension failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
