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
 *   applyMemoryTiersMigration(dbPath, { pepperBase64?, ledger? })
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
}

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
  let report: string;
  utils.closeSQLite();
  try {
    if (opts.ledger === false) {
      if (!(await addMemoryTiersMigration.shouldRun())) {
        report = `${addMemoryTiersMigration.id} not needed`;
      } else {
        const result = await addMemoryTiersMigration.run();
        if (!result?.success) throw new Error(`add-memory-tiers-v1 failed: ${result?.error}`);
        report = `+RAN ${result.id} (${result.message})`;
      }
    } else {
      const { MigrationRunner } = await import(join(v4Root, 'migrations/index.ts'));
      const runner = new MigrationRunner();
      (runner as unknown as { migrations: unknown[] }).migrations = [addMemoryTiersMigration];
      const result = await runner.runMigrations();
      if (!result.success) throw new Error(`the runner failed: ${JSON.stringify(result)}`);
      report = result.migrationsRun
        ? `+RAN ${result.results[0].id} (${result.results[0].message})`
        : `${addMemoryTiersMigration.id} skipped (ledger or not needed)`;
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
  return report;
}
