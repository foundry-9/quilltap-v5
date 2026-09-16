/**
 * P4.52 — bring a committed fixture pair up to v4's current schema vintage
 * (measured at v4 `b8449b3e`; extended at v4 `f4ad2c8d1` by P4.D179 with
 * `chat_settings.impersonationVoiceRewrite`, and at the same unification with
 * the two P4.D171 columns — at which point the script was also pointed at the
 * `post-office-*` and `help-chat-*` main partitions, whose staleness had put
 * FOUR families silently red on `main`; see below. P4.89 added the
 * `brahma-{main,mount}` pair and the `--report-only` gap measurement).
 *
 * Despite the file name this is the generic fixture-vintage migrator — the
 * TARGET LIST is the argv paths, and the recipes at the foot of this comment
 * are the standing set.
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
 *   chat_settings        composerEmoji, composerUnicode, smartTypographySettings,
 *                        impersonationVoiceRewrite (P4.D179, v4 `f4ad2c8d1`)
 *   connection_profiles  multiCharacterPrefill
 *   chats                cycleOrderParticipantIds (P4.D171, v4 `78b381a96`)
 *   chat_messages        routeTrail                (P4.D171, v4 `78b381a96`)
 *
 * P4.89 re-measured the `brahma-{main,mount}` pair at v4 `ffb6b3119` with v4's
 * OWN `extractSchemaMetadata` + `compareSchemas` + `generateAlterStatements`
 * over the live repository registry (41 collections incl. the two secondary
 * ones — `chat_messages` via `ChatMessageRowSchema`, `api_keys` via
 * `ApiKeySchema` — which the repo container does not expose as a
 * `collectionName`). Result: MAIN needs exactly the two P4.D171 columns,
 * `chats.cycleOrderParticipantIds` and `chat_messages.routeTrail`; every other
 * repo-backed table on both partitions reports `current`, with no removed and
 * no modified field anywhere. The MOUNT partition needs NOTHING. The pair has
 * no `characters` table at all, so the two MANAGED_FIELDS exclusions below
 * never arise for it.
 *
 * ⚠ One measured disagreement, recorded not resolved: for `routeTrail`
 * `generateAlterStatements` emits a bare `... ADD COLUMN "routeTrail" TEXT`
 * where v4's migration emits `TEXT DEFAULT NULL`. The two are semantically
 * identical in SQLite (an absent default IS NULL) and differ only in the
 * `sqlite_master` text; the migration form wins here for the same reason as
 * `multiCharacterPrefill` below — a real instance gets the column from the
 * migration, and that is the shape v5 must read.
 *
 * The two P4.D171 rows were added at the `f4ad2c8d1` unification. v4's OWN
 * jest side dies on them too — `_update` writes `$set: validated`, so any case
 * that WRITES a chat or a message on a pre-`78b381a96` pair recorded v4's
 * `no such column` as the expected value, and a v5-side heal on the per-case
 * copy could only make v5 diverge from a red oracle. Widening the committed
 * pair is the only fix that reaches BOTH engines; the v5-side
 * `ensure_p4d171_columns` heals the announcer family carries stay as harmless
 * no-ops.
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
 * the .db files named EXPLICITLY — name every pair to widen:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server          # or a PINNED v4 worktree
 *   $N/node --import tsx $W/harness/oracle/fixtures/migrate-memories-fixture-columns.ts \
 *     $W/crates/quilltap-web/tests/fixtures/memories-main.db \
 *     $W/crates/quilltap-web/tests/fixtures/memories-mount.db
 *
 * The brahma pair (P4.89, widened in place 2026-09-16 at v4 `ffb6b3119`):
 *   $N/node --import tsx $W/harness/oracle/fixtures/migrate-memories-fixture-columns.ts \
 *     $W/crates/quilltap-web/tests/fixtures/brahma-main.db \
 *     $W/crates/quilltap-web/tests/fixtures/brahma-mount.db
 *
 * `--report-only` (anywhere on argv) reports what WOULD be applied and writes
 * nothing — the dry run that measures a gap before touching a committed file.
 * It opens each target read-only, so it cannot leave `.db-journal` residue.
 *
 * ⚠ Applying (not `--report-only`) leaves a `<fixture>.db-journal` beside each
 * widened file — delete it; it is not part of the fixture.
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
  {
    // P4.D179 (v4 `686954937`, 4.10). Same class, same cause: `updateForUser`'s
    // `$set: validated` names it, so a fixture that predates it made v4's own
    // regen die on `no such column: impersonationVoiceRewrite` — which is how
    // this row was found (the P4.D179 regen batch, not inspection).
    table: 'chat_settings',
    column: 'impersonationVoiceRewrite',
    sql: `ALTER TABLE "chat_settings" ADD COLUMN "impersonationVoiceRewrite" INTEGER DEFAULT 0`,
    source: 'migrations/scripts/add-impersonation-voice-rewrite-field.ts:64',
  },
  {
    // P4.D171 (v4 `78b381a96`) — `addColumnIfMissing('chats',
    // 'cycleOrderParticipantIds', "TEXT DEFAULT '[]'")`, rendered by
    // `migrations/lib/database-utils.ts:354` as the ALTER below.
    table: 'chats',
    column: 'cycleOrderParticipantIds',
    sql: `ALTER TABLE "chats" ADD COLUMN "cycleOrderParticipantIds" TEXT DEFAULT '[]'`,
    source: 'migrations/scripts/add-cycle-order-column-v1.ts:47',
  },
  {
    // P4.D171 (v4 `78b381a96`) — `addColumnIfMissing('chat_messages',
    // 'routeTrail', 'TEXT DEFAULT NULL')`.
    table: 'chat_messages',
    column: 'routeTrail',
    sql: `ALTER TABLE "chat_messages" ADD COLUMN "routeTrail" TEXT DEFAULT NULL`,
    source: 'migrations/scripts/add-route-trail-message-column-v1.ts:49',
  },
];

function main(): void {
  const argv = process.argv.slice(2);
  const reportOnly = argv.includes('--report-only');
  const paths = argv.filter((a) => a !== '--report-only');
  if (paths.length === 0) {
    throw new Error(
      'usage: migrate-memories-fixture-columns.ts [--report-only] <fixture.db> [<fixture.db> …]',
    );
  }

  // The pepper → key in the raw-hex form (KDF skipped), exactly as the rest of
  // the harness opens a test-pepper DB.
  const hex = Buffer.from(TEST_PEPPER, 'base64').toString('hex');

  for (const path of paths) {
    // `--report-only` opens read-only so a dry run cannot leave journal residue.
    const db = new Database(path, reportOnly ? { readonly: true } : {});
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
      if (!reportOnly) db.exec(m.sql);
      applied.push(`${m.table}.${m.column}`);
    }
    db.close();
    const verb = reportOnly ? 'WOULD ADD ' : '+';
    process.stderr.write(
      applied.length
        ? `${path}: ${verb}${applied.join(` ${verb}`)}\n`
        : `${path}: already current\n`,
    );
  }
}

main();
