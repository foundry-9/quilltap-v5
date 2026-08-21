/**
 * P4.52 — bring the committed `memories-{main,mount}.db` pair up to v4's
 * current schema vintage (measured at v4 `b8449b3e`).
 *
 * ## Why this exists
 *
 * The pair was baked by `build-memories-web-fixture.ts` against a v4 of some
 * months ago, and v4 has added columns since. v4's `BaseRepository._update`
 * writes `$set: validated` — the WHOLE validated entity — so every schema field
 * carrying a Zod `.default()` is named in the UPDATE, and a column the fixture
 * predates is fatal. That is exactly how `chatSettings.updateForUser` died with
 * `no such column: composerEmoji`, which pinned `housekeeping_config_set` in
 * `memories_routes_equivalence` as a RULED VINTAGE ROW (v4 500 / v5 200) at the
 * `c8a3cf77` unification. This script closes the gap; the ruled row then
 * retires to a plain both-sides equality.
 *
 * ## The measured gap (fixture schema vs v4 `generateDDL` at `b8449b3e`)
 *
 * Main partition, three tables; the mount partition has NO column gap:
 *
 *   characters           archivedAt, archiveFileId, archivedAvatarFileId
 *   chat_settings        composerEmoji, composerUnicode, smartTypographySettings
 *   connection_profiles  multiCharacterPrefill
 *
 * Two further columns generateDDL emits are DELIBERATELY NOT added:
 * `characters.metadata` and `characters.canChooseOutfit`. Both are
 * MANAGED_FIELDS (`lib/database/repositories/vault-overlay/schema.ts:197`) —
 * v4 `delete`s them from the DB row on every create AND update, the vault files
 * are their sole source of truth, and NO v4 migration ever adds them. A real
 * migrated instance therefore does not carry them either; the fixture matching
 * production is the faithful shape, and adding a column v4 never writes could
 * only move a `SELECT *` projection.
 *
 * ## The DDL is v4's own
 *
 * Each statement below is v4's real migration SQL, verbatim, behind v4's own
 * "only if the column is missing" guard — the `migrate-fixtures-pascal-columns`
 * precedent, and for its reason: a REAL instance gets these columns from the
 * migration (appended at the end), which is the shape v5 must read. SQLite's
 * `ADD COLUMN … DEFAULT` back-fills existing rows with the default, so the
 * post-state is byte-identical to what a v4 upgrade produces.
 *
 * One recorded disagreement: `generateDDL` emits `multiCharacterPrefill` as a
 * bare `INTEGER` while v4's migration adds it `INTEGER DEFAULT 1` (the two D23
 * shapes disagreeing, as P4.D79 recorded for this very column). The migration
 * wins here, per the precedent above.
 *
 * Idempotent: a fixture already carrying a column is left untouched.
 *
 * Run from the v4 checkout (it resolves v4's aliased `better-sqlite3` →
 * better-sqlite3-multiple-ciphers, the sqleet/ChaCha20 binding), Node 24, with
 * the .db files named EXPLICITLY — this lane widens the memories pair only:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx $W/harness/oracle/fixtures/migrate-memories-fixture-columns.ts \
 *     $W/crates/quilltap-web/tests/fixtures/memories-main.db \
 *     $W/crates/quilltap-web/tests/fixtures/memories-mount.db
 */

import { createRequire } from 'node:module';

// Resolve better-sqlite3 (v4 aliases it to better-sqlite3-multiple-ciphers — the
// sqleet/ChaCha20 binding) from the v4 checkout: run this script with cwd there.
const requireFromV4 = createRequire(process.cwd() + '/');
const Database = requireFromV4('better-sqlite3');

/** The committed web fixtures' synthetic test pepper (quilltap-web/tests/common/mod.rs:17). */
const TEST_PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';

/** v4's `add-smart-typography-settings-field` column default, byte-for-byte. */
const DEFAULT_SMART_TYPOGRAPHY_SETTINGS = JSON.stringify({
  displayQuotes: false,
  dashes: true,
  ellipsis: true,
});

/** v4's real migration ALTERs, verbatim, in migration order. */
const MIGRATIONS: { table: string; column: string; sql: string; source: string }[] = [
  {
    table: 'characters',
    column: 'archivedAt',
    sql: 'ALTER TABLE "characters" ADD COLUMN "archivedAt" TEXT',
    source: 'migrations/scripts/add-character-archive-fields.ts:50',
  },
  {
    table: 'characters',
    column: 'archiveFileId',
    sql: 'ALTER TABLE "characters" ADD COLUMN "archiveFileId" TEXT',
    source: 'migrations/scripts/add-character-archive-fields.ts:55',
  },
  {
    table: 'characters',
    column: 'archivedAvatarFileId',
    sql: 'ALTER TABLE "characters" ADD COLUMN "archivedAvatarFileId" TEXT',
    source: 'migrations/scripts/add-character-archive-fields.ts:60',
  },
  {
    table: 'connection_profiles',
    column: 'multiCharacterPrefill',
    sql: 'ALTER TABLE "connection_profiles" ADD COLUMN "multiCharacterPrefill" INTEGER DEFAULT 1',
    source: 'migrations/scripts/add-profile-multi-character-prefill-field.ts:65',
  },
  {
    table: 'chat_settings',
    column: 'composerEmoji',
    sql: `ALTER TABLE "chat_settings" ADD COLUMN "composerEmoji" INTEGER DEFAULT 1`,
    source: 'migrations/scripts/add-composer-emoji-field.ts:54',
  },
  {
    table: 'chat_settings',
    column: 'composerUnicode',
    sql: `ALTER TABLE "chat_settings" ADD COLUMN "composerUnicode" INTEGER DEFAULT 1`,
    source: 'migrations/scripts/add-composer-unicode-field.ts:55',
  },
  {
    table: 'chat_settings',
    column: 'smartTypographySettings',
    sql: `ALTER TABLE "chat_settings" ADD COLUMN "smartTypographySettings" TEXT DEFAULT '${DEFAULT_SMART_TYPOGRAPHY_SETTINGS}'`,
    source: 'migrations/scripts/add-smart-typography-settings-field.ts:71',
  },
];

function main(): void {
  const paths = process.argv.slice(2);
  if (paths.length === 0) {
    throw new Error('usage: migrate-memories-fixture-columns.ts <fixture.db> [<fixture.db> …]');
  }

  // The pepper → key in the raw-hex form (KDF skipped), exactly as the rest of
  // the harness opens a test-pepper DB.
  const hex = Buffer.from(TEST_PEPPER, 'base64').toString('hex');

  for (const path of paths) {
    const db = new Database(path);
    db.pragma(`key = "x'${hex}'"`);
    const applied: string[] = [];
    for (const m of MIGRATIONS) {
      const table = db
        .prepare(`SELECT name FROM sqlite_master WHERE type='table' AND name=?`)
        .get(m.table);
      if (!table) continue; // this partition does not carry the table
      const cols = (db.prepare(`PRAGMA table_info("${m.table}")`).all() as { name: string }[]).map(
        (c) => c.name,
      );
      if (cols.includes(m.column)) continue; // v4's guard: already migrated
      db.exec(m.sql);
      applied.push(`${m.table}.${m.column}`);
    }
    db.close();
    process.stderr.write(
      applied.length ? `${path}: +${applied.join(' +')}\n` : `${path}: already current\n`,
    );
  }
}

main();
