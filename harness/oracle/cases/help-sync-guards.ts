/**
 * P4.d6 oracle case — the help-doc sync's PRUNE GUARD (v4 `551f090b`).
 *
 * The prune is the difference between "delete rows whose file is gone" and
 * "empty the table". Its guard is v4's `if (files.length === 0) return result`,
 * and the exact boundary is worth pinning against v4's REAL code rather than
 * against v4's own comment, which claims the guard is "produced at least one
 * READABLE file" — a stronger claim than the code makes.
 *
 * Three scenarios, each against a FRESH copy of the help-sync fixture DB (whose
 * three seeded help_docs rows are the thing at risk):
 *   - `missing-dir`      — the cwd has no help/ at all (v4's existsSync guard);
 *   - `no-markdown`      — help/ exists but holds zero .md files;
 *   - `only-empty-files` — help/ holds ONE whitespace-only .md. The walk is
 *                          non-empty, so the guard does NOT fire, but the file
 *                          contributes no `pathsOnDisk` entry — so every row is
 *                          unreachable and the table IS emptied. This is the
 *                          case that separates v4's code from v4's comment.
 *
 * `HELP_DIR` is captured at module load from `process.cwd()`, so a scenario per
 * PROCESS is the only honest way to drive this — hence the env var and the
 * three appending invocations below.
 *
 * Emits one line per run:
 *   { kind: 'guard', scenario, totalOnDisk, deleted, failed, helpDocIds }
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<this worktree>
 *   cd ~/source/quilltap-server
 *   : > /tmp/oracle-help-sync-guards.ndjson
 *   for s in missing-dir no-markdown only-empty-files; do
 *     QT_FIXTURE_HELP_MAIN=/tmp/qt-help-sync-main.db QT_HELP_SYNC_SCENARIO=$s \
 *       $N/node --import tsx $V5/harness/oracle/cases/help-sync-guards.ts \
 *       >> /tmp/oracle-help-sync-guards.ndjson
 *   done
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
}

const SCENARIOS = ['missing-dir', 'no-markdown', 'only-empty-files'] as const;

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const scenario = process.env.QT_HELP_SYNC_SCENARIO ?? '';
  if (!(SCENARIOS as readonly string[]).includes(scenario)) {
    throw new Error(
      `QT_HELP_SYNC_SCENARIO must be one of ${SCENARIOS.join('|')} (got ${scenario || '<unset>'})`,
    );
  }

  const scenarioRoot = join(here, '..', 'fixtures', 'help-sync-guards', scenario);
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'help-sync.json'), 'utf8'),
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_HELP_MAIN;
  if (!fixtureMain || !existsSync(fixtureMain)) {
    throw new Error('QT_FIXTURE_HELP_MAIN must point at the seed fixture');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-help-guards-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'help-sync-main.db');
  copyFileSync(fixtureMain, workMain);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // HELP_DIR is captured at module load from process.cwd() — chdir FIRST.
  process.chdir(scenarioRoot);

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { syncHelpDocs } = await import('@/lib/help/help-doc-sync');

  await initializeDatabase();

  const result = await syncHelpDocs();

  const rows =
    (await rawQuery<Array<Record<string, unknown>>>(
      'SELECT id FROM help_docs ORDER BY id ASC',
      [],
    )) ?? [];

  process.stdout.write(
    JSON.stringify({
      kind: 'guard',
      scenario,
      totalOnDisk: result.totalOnDisk,
      deleted: result.deleted,
      failed: result.failed,
      // P4.D77: a refused prune must also leave the section chunks alone. Zero
      // on every guard scenario — nothing is sliced when nothing changed.
      chunksWritten: result.chunksWritten,
      helpDocIds: rows.map((r) => String(r.id)),
    }) + '\n',
  );

  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`help-sync-guards oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
