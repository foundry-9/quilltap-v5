/**
 * Tier-2 oracle case — the `conversation_annotations` repo's `upsert` method,
 * in the MINTED-VALUES (remap) form.
 *
 * Proves what state v4 leaves the database in after a fixed sequence of
 * `upsert(input)` calls. `upsert` mints its OWN id (on the create path) and
 * timestamps (create: id+createdAt+updatedAt = the same now; update: a re-minted
 * updatedAt), so NOTHING is pinned on the ops. The Rust port mints its own
 * (different) values; the two dumps are reconciled by the harness's
 * normalization (first-seen `id` remap + timestamp placeholder) — see
 * crates/quilltap-harness/tests/conversation_annotations_upsert_tier2_equivalence.rs.
 *
 * This case therefore emits a RAW dump (no remap, no placeholder), sorted by the
 * natural key `content` (a deterministic INPUT, distinct across every final row).
 * The harness applies the SAME normalization to this dump and to the Rust dump,
 * then diffs, so the normalization is provably consistent.
 *
 * Both upsert paths are exercised by the corpus: a key matching a seed row
 * (UPDATE), a fresh key (CREATE), a key matching a just-created row (UPDATE of a
 * minted row), and another fresh key (CREATE). The nullable `sourceMessageId` is
 * banked both null and non-null.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`), AFTER
 * building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CONVERSATION_ANNOTATIONS_UPSERT=/tmp/qt-ca-upsert-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/conversation-annotations-upsert-tier2.ts \
 *     > /tmp/oracle-ca-upsert.ndjson
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

interface Op {
  kind: 'upsert';
  input: Record<string, unknown>;
}

interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(
    here,
    '..',
    'fixtures',
    'conversation-annotations-upsert-tier2.json'
  );
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_CONVERSATION_ANNOTATIONS_UPSERT;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_CONVERSATION_ANNOTATIONS_UPSERT must point at the seed fixture from build-conversation-annotations-upsert-fixture.ts'
    );
  }

  // Work on a fresh copy so the shared seed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-ca-upsert-oracle-'));
  // v4 nests working files under `<dataDir>/data/` (instance lock, sibling DBs).
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'ca-upsert-work.db');
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
  const { ConversationAnnotationsRepository } = await import(
    '@/lib/database/repositories/conversation-annotations.repository'
  );

  await initializeDatabase();
  const repo = new ConversationAnnotationsRepository();

  for (const op of spec.ops) {
    // v4 `upsert` mints id (create path) + timestamps itself; no options.
    await repo.upsert(op.input as never);
  }

  // Read RAW on-disk state through v4's own connected backend. table_info gives
  // schema column order; SELECT * gives the persisted rows (messageIndex as the
  // number, sourceMessageId null or text). RAW ids/timestamps — the harness
  // normalizes them.
  const columns = (
    (await rawQuery('PRAGMA table_info(conversation_annotations)')) as Array<{
      name: string;
    }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery(
    'SELECT * FROM conversation_annotations'
  )) as Array<Record<string, unknown>>;

  await closeDatabase();

  // RAW dump, sorted by the natural key `content` (NOT the minted id) so both
  // sides line up row-for-row before the harness remaps ids / placeholders ts.
  const dump = canonicalizeRows({
    table: 'conversation_annotations',
    columns,
    rawRows,
    orderBy: 'content',
  });

  process.stdout.write(
    JSON.stringify({ case: 'conversation-annotations-upsert-tier2', ...dump }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(
    `conversation-annotations-upsert-tier2 oracle failed: ${err?.stack ?? err}\n`
  );
  process.exit(1);
});
