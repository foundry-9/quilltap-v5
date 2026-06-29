/**
 * Tier-2 oracle case #3 — the `text_replacement_rules` repo (Phase-2, repo #3
 * after `folders` and `tags`).
 *
 * Proves what state v4 leaves the database in after a fixed create / update /
 * delete sequence that includes two CONFLICTING ops, so the Rust port can be
 * diffed against it (structural DB diff). Widens the tier-2 surface past `tags`:
 * an INTEGER number column (`sortOrder`), two boolean columns, and conflict
 * detection on a duplicate `(fromText, caseSensitive)` pair.
 *
 * Flow (mirrors tags-tier2.ts, plus expectThrow handling):
 *   1. copy the SEED-ONLY fixture to a fresh working copy;
 *   2. open the copy through v4's real `initializeDatabase()` and run the op
 *      sequence from `text-replacement-rules-tier2.json` via the real
 *      `TextReplacementRulesRepository`. Ops flagged `expectThrow` MUST throw a
 *      `TextReplacementRuleConflictError` — if one resolves, the fixture is
 *      mis-designed and we abort (v4 is the oracle of truth);
 *   3. close, then dump the table canonically (RAW on-disk cells via `rawQuery`:
 *      booleans as 0/1, sortOrder as the integer) and emit it as one NDJSON row.
 *
 * NORMALIZATION SPEC: none. Every id and timestamp is pinned on both sides.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`), AFTER
 * building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TRR=/tmp/qt-trr-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/text-replacement-rules-tier2.ts \
 *     > /tmp/oracle-trr.ndjson
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
  kind: 'create' | 'update' | 'delete';
  id?: string;
  expectThrow?: boolean;
  data?: Record<string, unknown>;
  options?: { id: string; createdAt: string; updatedAt: string };
}

interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'text-replacement-rules-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_TRR;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_TRR must point at the seed fixture from build-text-replacement-rules-fixture.ts'
    );
  }

  // Work on a fresh copy so the shared seed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-trr-oracle-'));
  // v4 nests working files under `<dataDir>/data/` (instance lock, sibling DBs).
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'trr-work.db');
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
  const { TextReplacementRulesRepository } = await import(
    '@/lib/database/repositories/text-replacement-rules.repository'
  );

  await initializeDatabase();
  const repo = new TextReplacementRulesRepository();

  for (const op of spec.ops) {
    const run = async (): Promise<void> => {
      if (op.kind === 'create') {
        await repo.create(op.data as never, op.options);
      } else if (op.kind === 'update') {
        await repo.update(op.id as string, op.data as never);
      } else {
        await repo.delete(op.id as string);
      }
    };

    if (op.expectThrow) {
      let threw = false;
      try {
        await run();
      } catch (err) {
        threw = true;
        const name = (err as { name?: string })?.name;
        if (name !== 'TextReplacementRuleConflictError') {
          throw new Error(
            `expectThrow op (${op.kind}) threw the wrong error: ${name ?? err}`
          );
        }
      }
      if (!threw) {
        throw new Error(
          `expectThrow op (${op.kind} ${op.id ?? op.options?.id}) did NOT throw — fixture mis-designed`
        );
      }
    } else {
      await run();
    }
  }

  // Read RAW on-disk state through v4's own connected backend. table_info gives
  // schema column order; SELECT * gives the persisted rows (booleans as 0/1,
  // sortOrder as the integer).
  const columns = (
    (await rawQuery('PRAGMA table_info(text_replacement_rules)')) as Array<{
      name: string;
    }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM text_replacement_rules')) as Array<
    Record<string, unknown>
  >;

  await closeDatabase();

  const dump = canonicalizeRows({
    table: 'text_replacement_rules',
    columns,
    rawRows,
    orderBy: 'id',
  });

  process.stdout.write(
    JSON.stringify({ case: 'text-replacement-rules-tier2', ...dump }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(
    `text-replacement-rules-tier2 oracle failed: ${err?.stack ?? err}\n`
  );
  process.exit(1);
});
