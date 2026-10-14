/**
 * C1 §13 (P4.D265) — run v4's REAL `add-memory-tiers-v1` (`d58548051`,
 * `migrations/scripts/add-memory-tiers-v1.ts#addMemoryTiersMigration`) on ONE
 * database file: the four `memories` tier columns, `idx_memories_character_tier`,
 * the NULL-tier heal and `chats.otherExtractionWatermarkMessageId`.
 *
 * The v4-side twin of v5's `test_support::ensure_memory_tiers_columns` — and,
 * like it, for BUILDER outputs and in-case PLANTS only: every committed
 * `*-main.db` pair was WIDENED in place by P4.D265 (§R.3), and the migration is
 * idempotent, so applying it to a widened copy is harmless.
 *
 *   applyMemoryTiersMigration(dbPath, { pepperBase64?, ledger?, chain? })
 *
 * - `pepperBase64` — the file's test pepper (an ENCRYPTED copy, keyed exactly
 *   as `lib/v4-migrations.ts` keys it: `PRAGMA key = "x'<hex>'"`); omit it for
 *   a PLAIN unencrypted file (v4's migration connection then sets no key).
 * - `ledger` (default `true`) — run through v4's REAL `MigrationRunner`
 *   narrowed to this one migration (the `wardrobe-wear-stats-ensure.ts`
 *   mechanics), which consults and STAMPS `migrations_state`, exactly as a v4
 *   boot does. `ledger: false` calls the migration's `shouldRun()` / `run()`
 *   directly — no ledger row — for a family whose comparand would otherwise
 *   see the v4-only `migrations_state` table.
 * - `chain` (default `true`, P4.D288 at the `83334b9a5` pin) — follow the
 *   tiers step with v4's two migrations that consume what it adds, in v4's
 *   registration order: `add-episode-extraction-watermark-v1` (`c2795733f`)
 *   and `add-memory-extraction-cursors-v1` (`ac6348c5f`, which copies both
 *   chat watermark columns into `memory_extraction_cursors` and DROPS them).
 *   At the pin `add-memory-tiers-v1` still ADDS
 *   `chats.otherExtractionWatermarkMessageId`, so without the chain every
 *   caller would re-add a column a real v4 instance no longer carries.
 *   `chain: false` runs the tiers step alone (`memory-tiers-ensure.ts`,
 *   which compares the chain itself).
 *
 * Mechanics that bite (shared with `lib/v4-migrations.ts`): v4's migration
 * connection is a module singleton reading `SQLITE_PATH` /
 * `ENCRYPTION_MASTER_PEPPER` / `QUILLTAP_DATA_DIR` from the environment (saved
 * and restored here, so a caller's own `initializeDatabase` settings survive),
 * it takes the instance lock under the data dir (a scratch dir here), and it
 * opens WAL — PERSISTED in the header — so the file's journal mode is read
 * first and restored after, and `-wal` / `-shm` residue is removed.
 * `closeSQLite()` runs before and after. Call it while the file is NOT open in
 * the caller's own v4 connection (before `initializeDatabase()`).
 *
 * Run with cwd = the v4 checkout or a pinned worktree (the module and
 * `better-sqlite3` resolve against it). The v4 modules are imported by ABSOLUTE
 * PATH, not a `file://` URL: the same specifier resolves under tsx (ESM) and
 * under jest (whose `import()` becomes a `require`) — a jest case stages this
 * file beside itself (`$TMPO/fixtures/`) and imports
 * `../fixtures/apply-memory-tiers-migration`.
 */

import { existsSync, mkdirSync, mkdtempSync, rmSync, statSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

export interface ApplyMemoryTiersOptions {
  pepperBase64?: string;
  ledger?: boolean;
  chain?: boolean;
}

/** P4.D288: the two migrations the chain runs after the tiers step. */
const CHAIN: Array<{ file: string; exportName: string }> = [
  { file: 'migrations/scripts/add-episode-extraction-watermark-v1.ts', exportName: 'addEpisodeExtractionWatermarkMigration' },
  { file: 'migrations/scripts/add-memory-extraction-cursors-v1.ts', exportName: 'addMemoryExtractionCursorsMigration' },
];

const ENV_KEYS = ['SQLITE_PATH', 'ENCRYPTION_MASTER_PEPPER', 'QUILLTAP_DATA_DIR'] as const;

/** v4's migration report: `+RAN <id> (<message>)`, `<id> not needed`, or the
 *  runner's `skipped (ledger)`. */
export async function applyMemoryTiersMigration(
  dbPath: string,
  opts: ApplyMemoryTiersOptions = {},
): Promise<string> {
  const v4Root = process.cwd();
  const Database = createRequire(v4Root + '/')('better-sqlite3');
  const keyPragma = opts.pepperBase64
    ? `key = "x'${Buffer.from(opts.pepperBase64, 'base64').toString('hex')}'"`
    : null;
  const openRaw = (readonly: boolean) => {
    const db = new Database(dbPath, readonly ? { readonly: true } : {});
    if (keyPragma) db.pragma(keyPragma);
    return db;
  };
  const originalMode = (() => {
    const db = openRaw(true);
    try {
      return String(db.pragma('journal_mode', { simple: true }));
    } finally {
      db.close();
    }
  })();

  const saved = Object.fromEntries(ENV_KEYS.map((k) => [k, process.env[k]]));
  const scratch = mkdtempSync(join(tmpdir(), 'qt-memory-tiers-apply-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.SQLITE_PATH = dbPath;
  if (opts.pepperBase64) process.env.ENCRYPTION_MASTER_PEPPER = opts.pepperBase64;
  else delete process.env.ENCRYPTION_MASTER_PEPPER;
  process.env.QUILLTAP_DATA_DIR = scratch;

  const utils = await import(join(v4Root, 'migrations/lib/database-utils.ts'));
  const { addMemoryTiersMigration } = await import(
    join(v4Root, 'migrations/scripts/add-memory-tiers-v1.ts')
  );
  type Migration = {
    id: string;
    shouldRun: () => Promise<boolean>;
    run: () => Promise<{ success: boolean; id: string; message?: string; error?: string }>;
  };
  const migrations: Migration[] = [addMemoryTiersMigration];
  if (opts.chain !== false) {
    for (const m of CHAIN) migrations.push((await import(join(v4Root, m.file)))[m.exportName]);
  }
  const reports: string[] = [];
  utils.closeSQLite();
  try {
    if (opts.ledger === false) {
      for (const m of migrations) {
        if (!(await m.shouldRun())) {
          reports.push(`${m.id} not needed`);
          continue;
        }
        const result = await m.run();
        if (!result?.success) throw new Error(`${m.id} failed: ${result?.error}`);
        reports.push(`+RAN ${result.id} (${result.message})`);
      }
    } else {
      const { MigrationRunner } = await import(join(v4Root, 'migrations/index.ts'));
      const runner = new MigrationRunner();
      (runner as unknown as { migrations: unknown[] }).migrations = migrations;
      const result = await runner.runMigrations();
      if (!result.success) throw new Error(`the runner failed: ${JSON.stringify(result)}`);
      const ran = new Map<string, { id: string; message?: string }>(
        (result.results as Array<{ id: string; message?: string }>).map((r) => [r.id, r]),
      );
      for (const m of migrations) {
        const r = ran.get(m.id);
        reports.push(r ? `+RAN ${r.id} (${r.message})` : `${m.id} skipped (ledger or not needed)`);
      }
    }
  } finally {
    utils.closeSQLite();
    for (const k of ENV_KEYS) {
      if (saved[k] === undefined) delete process.env[k];
      else process.env[k] = saved[k] as string;
    }
    rmSync(scratch, { recursive: true, force: true });
  }

  // The header's journal mode back; WAL / empty-journal residue swept.
  const db = openRaw(false);
  try {
    if (String(db.pragma('journal_mode', { simple: true })) !== originalMode) {
      db.pragma(`journal_mode = ${originalMode}`);
    }
  } finally {
    db.close();
  }
  for (const suffix of ['-wal', '-shm']) {
    if (existsSync(dbPath + suffix)) rmSync(dbPath + suffix);
  }
  const journal = dbPath + '-journal';
  if (existsSync(journal) && statSync(journal).size === 0) rmSync(journal);
  return reports.join('; ');
}
