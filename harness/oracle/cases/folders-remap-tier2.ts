/**
 * Tier-2 oracle case — the `folders` repo MINTED-VALUES (remap) path.
 *
 * Proves v4's resulting DB state after a create sequence that pins NOTHING, so
 * v4 mints its own random UUIDs and wall-clock timestamps. The Rust port mints
 * its own (different) values; the two dumps are reconciled by the harness's
 * normalization (first-seen id remap in natural-key order + timestamp
 * placeholder) — see crates/quilltap-harness/tests/folders_remap_tier2_equivalence.rs.
 *
 * This case therefore emits a RAW dump (no remap, no placeholder), sorted by the
 * natural key `path` — the harness applies the SAME normalization to this dump
 * and to the Rust dump, then diffs, so the normalization is provably consistent.
 *
 * Op forwarding: op[1] carries `parentFromOp: 0`, meaning "set parentFolderId to
 * the id the repo returned for op[0]". Both this oracle and the Rust harness
 * capture each create's id and resolve the reference, so a generated id ends up
 * referencing another generated id (the case the remap exists for).
 *
 * Run from the v4 server checkout under Node 24, AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_FOLDERS_REMAP=/tmp/qt-folders-remap-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/folders-remap-tier2.ts \
 *     > /tmp/oracle-folders-remap.ndjson
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
  kind: 'create';
  data: Record<string, unknown> & { parentFromOp?: number };
}

interface Spec {
  testPepperBase64: string;
  userId: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'folders-remap-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_FOLDERS_REMAP;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_FOLDERS_REMAP must point at the seed fixture from build-folders-remap-fixture.ts'
    );
  }

  // Work on a fresh copy so the shared seed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-folders-remap-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'folders-remap-work.db');
  copyFileSync(fixture, work);

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { FoldersRepository } = await import(
    '@/lib/database/repositories/folders.repository'
  );

  await initializeDatabase();
  const repo = new FoldersRepository();

  // Capture each create's minted id so a later op can reference it.
  const mintedIds: string[] = [];
  for (const op of spec.ops) {
    const data = { ...op.data };
    if (typeof data.parentFromOp === 'number') {
      data.parentFolderId = mintedIds[data.parentFromOp];
      delete data.parentFromOp;
    }
    // No options -> v4 mints id + timestamps.
    const created = await repo.create(data as never);
    mintedIds.push(created.id);
  }

  const columns = (
    (await rawQuery('PRAGMA table_info(folders)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM folders')) as Array<
    Record<string, unknown>
  >;

  await closeDatabase();

  // RAW dump, sorted by the natural key `path` (NOT the random id) so both
  // sides line up row-for-row before the harness remaps ids / placeholders ts.
  const dump = canonicalizeRows({
    table: 'folders',
    columns,
    rawRows,
    orderBy: 'path',
  });

  process.stdout.write(
    JSON.stringify({ case: 'folders-remap-tier2', ...dump }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`folders-remap-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
