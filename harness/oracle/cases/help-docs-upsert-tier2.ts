/**
 * Tier-2 oracle case — `help_docs.upsertByPath` (the deferred path-keyed upsert).
 *
 * Proves what state v4 leaves the database in after a fixed sequence of
 * `upsertByPath` calls, so the Rust port can be diffed against it. This is the
 * MINTED-VALUES (remap) form: `upsertByPath` mints its own id + timestamps on the
 * create branch and mints `updatedAt` on the update branch, so the raw dumps
 * cannot match the Rust port's (which mints its own). The harness reconciles them
 * by remapping `id` to first-seen tokens (rows sorted by the natural key `path`)
 * and placeholdering `createdAt`/`updatedAt` — every other column (including the
 * embedding hex/null) is diffed exactly. See
 * crates/quilltap-harness/tests/help_docs_upsert_tier2_equivalence.rs.
 *
 * This case therefore emits a RAW dump (no remap, no placeholder), sorted by
 * `path`, so the harness applies the SAME normalization to this dump and to the
 * Rust dump, then diffs.
 *
 * THE EMBEDDING POINT: op 1 upserts onto seed row `help/aurora.md`, which carries
 * a non-null embedding. `upsertByPath` updates only the four text columns, so the
 * embedding BLOB must survive — the dumped `aurora` row still shows its hex.
 * Ops 2/4 create rows whose embedding is NULL (the create branch never sets it).
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`), AFTER
 * building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_HELP_DOCS_UPSERT=/tmp/qt-hd-upsert-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/help-docs-upsert-tier2.ts \
 *     > /tmp/oracle-hd-upsert.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  copyFileSync,
  existsSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface UpsertData {
  title: string;
  path: string;
  url: string;
  content: string;
  contentHash: string;
}

interface Op {
  kind: 'upsert';
  data: UpsertData;
}

interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'help-docs-upsert-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_HELP_DOCS_UPSERT;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_HELP_DOCS_UPSERT must point at the seed fixture from build-help-docs-upsert-fixture.ts'
    );
  }

  // Work on a fresh copy so the shared seed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-hd-upsert-oracle-'));
  // v4 nests working files under `<dataDir>/data/` (instance lock, sibling DBs).
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'hd-upsert-work.db');
  copyFileSync(fixture, work);

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE; // writable path uses journal_mode = TRUNCATE
  // Keep stdout clean for the NDJSON: v4's console logger sends INFO to stdout.
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { HelpDocsRepository } = await import(
    '@/lib/database/repositories/help-docs.repository'
  );

  await initializeDatabase();
  const repo = new HelpDocsRepository();

  for (const op of spec.ops) {
    // upsertByPath(path, data) — data is the embedding-less Omit shape. v4 mints
    // its own id + timestamps on the create branch (and updatedAt on update).
    await repo.upsertByPath(op.data.path, op.data as never);
  }

  // Read RAW on-disk state through v4's own connected backend. table_info gives
  // schema column order; SELECT * gives the persisted rows (the embedding cell
  // comes back as a raw Buffer -> canonValue hex, or null).
  const columns = (
    (await rawQuery('PRAGMA table_info(help_docs)')) as Array<{
      name: string;
    }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM help_docs')) as Array<
    Record<string, unknown>
  >;

  await closeDatabase();

  // RAW dump, sorted by the natural key `path` (NOT the random id) so both sides
  // line up row-for-row before the harness remaps ids / placeholders ts.
  const dump = canonicalizeRows({
    table: 'help_docs',
    columns,
    rawRows,
    orderBy: 'path',
  });

  process.stdout.write(
    JSON.stringify({ case: 'help-docs-upsert-tier2', ...dump }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`help-docs-upsert-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
