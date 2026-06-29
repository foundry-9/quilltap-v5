/**
 * Tier-2 oracle case #4 — the `prompt_templates` repo (Phase-2, repo #4 after
 * `folders`, `tags`, `text_replacement_rules`).
 *
 * Proves what state v4 leaves the database in after a fixed create / update /
 * delete sequence that includes two GUARDED no-ops (a built-in row that
 * `update`/`delete` must refuse to mutate), so the Rust port can be diffed
 * against it (structural DB diff). Widens the tier-2 surface past
 * `text_replacement_rules`: the first JSON ARRAY column (`tags`), several
 * nullable string columns, and the built-in read-only guard.
 *
 * Flow (mirrors text-replacement-rules-tier2.ts, with expectNoop handling):
 *   1. copy the SEED-ONLY fixture to a fresh working copy;
 *   2. open the copy through v4's real `initializeDatabase()` and run the op
 *      sequence from `prompt-templates-tier2.json` via the real
 *      `PromptTemplatesRepository`. Ops flagged `expectNoop` MUST report
 *      not-modified — `update` returns `null`, `delete` returns `false` — or the
 *      fixture is mis-designed and we abort (v4 is the oracle of truth);
 *   3. close, then dump the table canonically (RAW on-disk cells via `rawQuery`:
 *      booleans as 0/1, the `tags` array as its JSON text) and emit it as one
 *      NDJSON row.
 *
 * NORMALIZATION SPEC: none. Every id and timestamp is pinned on both sides.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`), AFTER
 * building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_PROMPT_TEMPLATES=/tmp/qt-prompt-templates-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/prompt-templates-tier2.ts \
 *     > /tmp/oracle-prompt-templates.ndjson
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
  expectNoop?: boolean;
  data?: Record<string, unknown>;
  options?: { id: string; createdAt: string; updatedAt: string };
}

interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'prompt-templates-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_PROMPT_TEMPLATES;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_PROMPT_TEMPLATES must point at the seed fixture from build-prompt-templates-fixture.ts'
    );
  }

  // Work on a fresh copy so the shared seed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-prompt-templates-oracle-'));
  // v4 nests working files under `<dataDir>/data/` (instance lock, sibling DBs).
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'prompt-templates-work.db');
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
  const { PromptTemplatesRepository } = await import(
    '@/lib/database/repositories/prompt-templates.repository'
  );

  await initializeDatabase();
  const repo = new PromptTemplatesRepository();

  for (const op of spec.ops) {
    if (op.kind === 'create') {
      await repo.create(op.data as never, op.options);
    } else if (op.kind === 'update') {
      const result = await repo.update(op.id as string, op.data as never);
      if (op.expectNoop && result !== null) {
        throw new Error(
          `expectNoop update (${op.id}) modified the row — guard failed in oracle`
        );
      }
    } else {
      const result = await repo.delete(op.id as string);
      if (op.expectNoop && result !== false) {
        throw new Error(
          `expectNoop delete (${op.id}) removed the row — guard failed in oracle`
        );
      }
    }
  }

  // Read RAW on-disk state through v4's own connected backend. table_info gives
  // schema column order; SELECT * gives the persisted rows (booleans as 0/1, the
  // tags array as its JSON text).
  const columns = (
    (await rawQuery('PRAGMA table_info(prompt_templates)')) as Array<{
      name: string;
    }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM prompt_templates')) as Array<
    Record<string, unknown>
  >;

  await closeDatabase();

  const dump = canonicalizeRows({
    table: 'prompt_templates',
    columns,
    rawRows,
    orderBy: 'id',
  });

  process.stdout.write(
    JSON.stringify({ case: 'prompt-templates-tier2', ...dump }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`prompt-templates-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
