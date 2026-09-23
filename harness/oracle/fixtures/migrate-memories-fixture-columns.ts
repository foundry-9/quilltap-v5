/**
 * P4.52 — bring a committed fixture pair up to v4's current schema vintage
 * (measured at v4 `b8449b3e`; extended at v4 `f4ad2c8d1` by P4.D179 with
 * `chat_settings.impersonationVoiceRewrite`, and at the same unification with
 * the two P4.D171 columns — at which point the script was also pointed at the
 * `post-office-*` and `help-chat-*` main partitions, whose staleness had put
 * FOUR families silently red on `main`; see below. P4.89 added the
 * `brahma-{main,mount}` pair and the `--report-only` gap measurement. P4.94
 * measured and widened FIVE more pairs at v4 `1fefadb9a` — `salon-*`,
 * `chat-gallery-*`, `images-*`, `courier-images-*`, `pascal-run-custom-*` —
 * and with them added five migration rows, the `extraSql` slot for v4's
 * index statements, and the second test pepper; see "The P4.94 measurement"
 * below).
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
 * ## The P4.94 measurement (five pairs, v4 `1fefadb9a`)
 *
 * Measured the P4.89 way — v4's REAL `extractSchemaMetadata` + `compareSchemas`
 * + `generateAlterStatements` over the live repository registry (41
 * collections: `getRepositories()` plus the two SECONDARY ones the container
 * does not expose as a `collectionName`, `chat_messages` via
 * `ChatMessageRowSchema` and `api_keys` via `ApiKeySchema`), per repo-backed
 * table, against each fixture's `PRAGMA table_info`:
 *
 *   salon-main.db            chat_messages.routeTrail,
 *                            chat_settings.impersonationVoiceRewrite,
 *                            chats.cycleOrderParticipantIds,
 *                            connection_profiles.fallbackProfileId + allowTierFallback,
 *                            files.generationKey
 *   salon-mount.db           (nothing)
 *   salon-llm-logs.db        llm_logs.connectionProfileId + imageProfileId
 *   chat-gallery-main.db     files.generationKey
 *   chat-gallery-mount.db    (nothing)
 *   images-main.db           chat_messages.routeTrail,
 *                            chat_settings.impersonationVoiceRewrite,
 *                            chats.cycleOrderParticipantIds,
 *                            files.generationKey
 *   images-mount.db          (nothing)
 *   courier-images-main.db   characters.archivedAt/archiveFileId/archivedAvatarFileId,
 *                            chat_messages.routeTrail,
 *                            chat_settings.composerEmoji/composerUnicode/
 *                              impersonationVoiceRewrite/smartTypographySettings,
 *                            chats.cycleOrderParticipantIds,
 *                            connection_profiles.multiCharacterPrefill +
 *                              fallbackProfileId + allowTierFallback,
 *                            files.generationKey
 *   courier-images-mount.db  (nothing)
 *   courier-images-llmlogs.db llm_logs.connectionProfileId + imageProfileId
 *   pascal-run-custom-main.db  chat_messages.routeTrail,
 *                            chat_settings.impersonationVoiceRewrite,
 *                            chats.cycleOrderParticipantIds
 *   pascal-run-custom-mount.db (nothing)
 *
 * No removed and no modified field on any partition. Three columns the round
 * did not predict turned up and are now rows below: the P4.D135 fallback pair
 * on `connection_profiles` and the P4.D49 attribution pair on `llm_logs` —
 * BOTH committed llm-logs partitions lag, which no family had ever regenerated
 * across.
 *
 * ⚠ `chats.transcriptVersion` — ADDED, by a correction at the `53294163f`
 * unification (§3 review). The lane left it out on the ground that
 * `compareSchemas` does not list it (true: v4 declares it in neither
 * `ChatMetadataSchema` nor `ChatMetadataBaseSchema`, so a counter inside the
 * schema cannot be rewound by `$set: validated`) "and no v4 write ever names
 * it" — FALSE on both halves: v4 has a REGISTERED migration adding it
 * (`migrations/scripts/add-transcript-version-column-v1.ts:53`,
 * `addColumnIfMissing('chats', 'transcriptVersion', 'INTEGER DEFAULT 0')`,
 * registered at `migrations/scripts/index.ts:793` — one step BEFORE the
 * `files.generationKey` migration at `:795`), and v4 writes it
 * (`lib/database/repositories/chats-messages.ops.ts:277`, `$inc`, wrapped in a
 * fail-soft `safeQuery`). So a real migrated v4 instance that has
 * `generationKey` necessarily has `transcriptVersion` too, and a pair with one
 * but not the other is a vintage no real instance can be in. The row below
 * follows v4's own migration statement; v5's `db/chats_transcript_version_
 * repair.rs` boot ensure and the per-copy `ensure_p4d182_columns` heal become
 * no-ops on these pairs. (The brahma pair, P4.89, still lacks it — the same
 * correction applies there at the next widen.)
 *
 * ⚠ The `salon-long-{main,mount}.db` pair was measured and DELIBERATELY NOT
 * widened: no red names it, and the order confines P4.94 to pairs a red names.
 * Its gap is the union of the above plus the two MANAGED_FIELDS columns
 * (`characters.metadata`, `canChooseOutfit`) this script never adds.
 *
 * ⚠ One pre-existing ordering wrinkle, recorded not repaired: v4 runs the
 * `chat_settings` composer trio (`index.ts:765-771`) BEFORE
 * `connection_profiles.multiCharacterPrefill` (`:773`), and the routeTrail /
 * cycleOrder / impersonationVoiceRewrite trio in the order `:784`, `:786`,
 * `:791`. This list has carried a different order since P4.52 and the
 * already-widened pairs were written in it, so re-ordering would only move the
 * physical column order of pairs widened FROM NOW ON — immaterial to every
 * differential (both engines read the same file) and not worth splitting the
 * two vintages. The new rows below sit at v4's true registration positions
 * relative to their neighbours.
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
 * The five P4.94 pairs (widened in place 2026-09-17 at v4 `1fefadb9a`; the
 * `chats.transcriptVersion` row re-applied at the `53294163f` unification):
 *   F=$W/crates/quilltap-web/tests/fixtures
 *   $N/node --import tsx $W/harness/oracle/fixtures/migrate-memories-fixture-columns.ts \
 *     $F/salon-main.db $F/salon-mount.db $F/salon-llm-logs.db \
 *     $F/chat-gallery-main.db $F/chat-gallery-mount.db \
 *     $F/images-main.db $F/images-mount.db \
 *     $F/courier-images-main.db $F/courier-images-mount.db $F/courier-images-llmlogs.db \
 *     $F/pascal-run-custom-main.db $F/pascal-run-custom-mount.db
 *   (the mount partitions and `salon-long-*` report `current` — measured, not
 *   skipped; `salon-long-*` is deliberately NOT widened, see above)
 *
 * The `attach-file-*` trio (the SIXTH pair, widened in place 2026-09-17 at v4
 * `bcd7e4852` by the `bcd7e4852` round's unification — `attach_mount_file_
 * equivalence` had been RED at both pins because v4's `$inc transcriptVersion`
 * on the Librarian announcement write threw on the pre-`31436bae4` vintage and
 * the route answered 500; the main partition lacked FIVE columns):
 *   $N/node --import tsx $W/harness/oracle/fixtures/migrate-memories-fixture-columns.ts \
 *     $F/attach-file-main.db $F/attach-file-mount.db $F/attach-file-llmlogs.db
 *   (the mount and llm-logs partitions report `current`)
 *
 * `--report-only` (anywhere on argv) reports what WOULD be applied and writes
 * nothing — the dry run that measures a gap before touching a committed file.
 * It opens each target read-only, so it cannot leave `.db-journal` residue.
 *
 * ⚠ Applying (not `--report-only`) leaves a `<fixture>.db-journal` beside each
 * widened file — delete it; it is not part of the fixture.
 *
 * The six P4.103 fixture-vintage-heal pairs (widened in place 2026-09-22 at
 * v4 `f45a517a9` — the seven standing reds this closes are named in
 * `docs/developer/porting/work-orders/p4.103-fixture-vintage-heal.md`; no
 * index-gating surprises this time, every partition already carried its
 * indexes or gained them alongside its columns in one pass):
 *   F=$W/crates/quilltap-web/tests/fixtures
 *   $N/node --import tsx $W/harness/oracle/fixtures/migrate-memories-fixture-columns.ts \
 *     $F/subprompts-main.db $F/subprompts-mount.db \
 *     $F/chat-delete-main.db $F/chat-delete-mount.db $F/chat-delete-llmlogs.db \
 *     $F/character-generators-main.db $F/character-generators-mount.db \
 *     $F/chat-dialogs-main.db $F/chat-dialogs-mount.db \
 *     $F/profile-main.db $F/profile-mount.db \
 *     $F/groups-projects-main.db $F/groups-projects-mount.db \
 *     $F/chat-compressed-main.db
 *   (run a second time, alone, for the llm-logs partition P4.103 also
 *   widened for its indexes — already current on columns, gained
 *   idx_llm_logs_connectionProfileId/imageProfileId on this pass — Tier 2
 *   item 7: `$F/chat-compressed-llmlogs.db`; `chat-delete-llmlogs.db` above
 *   got the same treatment in its own pass)
 *   (every mount partition reports "already current"; `chat-compressed-
 *   mount.db` is not in this list — it is the ONE rebuild the round
 *   authorizes, from `build-chat-compressed-fixture.ts`, not this script)
 */

import { createRequire } from 'node:module';

// Resolve better-sqlite3 (v4 aliases it to better-sqlite3-multiple-ciphers — the
// sqleet/ChaCha20 binding) from the v4 checkout: run this script with cwd there.
const requireFromV4 = createRequire(process.cwd() + '/');
const Database = requireFromV4('better-sqlite3');

/**
 * The committed web fixtures' synthetic test peppers. Most pairs carry the
 * `quilltap-web/tests/common/mod.rs:17` one; the `images-{main,mount}.db` pair
 * is keyed with the `images-collection.json` spec's own `testPepperBase64`
 * instead (measured at P4.94 — the single-pepper assumption made the migrator
 * die `SQLITE_NOTADB` on that pair, which reads exactly like a corrupt file).
 * Each target is opened with the first pepper that works; none working is a
 * loud refusal, never a skip.
 */
const TEST_PEPPERS: { name: string; base64: string }[] = [
  { name: 'web-fixture', base64: 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=' },
  { name: 'images-collection', base64: 'dGVzdC1wZXBwZXItZm9yLWZpeHR1cmVzLW9ubHktMzJieXRl' },
];

/** v4's `add-smart-typography-settings-field` column default, byte-for-byte. */
const DEFAULT_SMART_TYPOGRAPHY_SETTINGS = JSON.stringify({
  displayQuotes: false,
  dashes: true,
  ellipsis: true,
});

/** v4's real migration ALTERs, verbatim, in migration order. */
const MIGRATIONS: {
  table: string;
  column: string;
  sql: string;
  /** Statements v4's own migration issues BESIDE the ALTER (its indexes). */
  extraSql?: string[];
  source: string;
}[] = [
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
    // P4.94 (v4 `0cde7fbc`, the P4.D49 Almanack attribution pair). v4 runs this
    // over the LLM-LOGS partition, not main: `openLlmLogsDbIfPresent()`. Both
    // committed llm-logs partitions this round measures lag it
    // (`salon-llm-logs.db`, `courier-images-llmlogs.db`), and v5's llm_logs
    // INSERT binds all twenty columns, so the gap is fatal on the v5 side of
    // any family that WRITES a log row.
    table: 'llm_logs',
    column: 'connectionProfileId',
    sql: 'ALTER TABLE "llm_logs" ADD COLUMN "connectionProfileId" TEXT DEFAULT NULL',
    extraSql: [
      'CREATE INDEX IF NOT EXISTS "idx_llm_logs_connectionProfileId" ON "llm_logs" ("connectionProfileId")',
    ],
    source: 'migrations/scripts/add-llm-logs-profile-columns.ts:77,85',
  },
  {
    table: 'llm_logs',
    column: 'imageProfileId',
    sql: 'ALTER TABLE "llm_logs" ADD COLUMN "imageProfileId" TEXT DEFAULT NULL',
    extraSql: [
      'CREATE INDEX IF NOT EXISTS "idx_llm_logs_imageProfileId" ON "llm_logs" ("imageProfileId")',
    ],
    source: 'migrations/scripts/add-llm-logs-profile-columns.ts:77,85',
  },
  {
    table: 'connection_profiles',
    column: 'multiCharacterPrefill',
    sql: 'ALTER TABLE "connection_profiles" ADD COLUMN "multiCharacterPrefill" INTEGER DEFAULT 1',
    source: 'migrations/scripts/add-profile-multi-character-prefill-field.ts:65',
  },
  {
    // P4.94 (v4 `ca75d7a0d`, the P4.D135 provider fallback chains). Measured
    // missing on `salon-main.db`, `courier-images-main.db` and
    // `salon-long-main.db`; already present on the pascal pair, which
    // `migrate-pascal-run-custom-columns.ts` (P4.D155) widened for exactly
    // this reason.
    table: 'connection_profiles',
    column: 'fallbackProfileId',
    sql: 'ALTER TABLE "connection_profiles" ADD COLUMN "fallbackProfileId" TEXT',
    source: 'migrations/scripts/add-profile-fallback-fields.ts:31,62',
  },
  {
    // v4 follows the ALTER with
    // `UPDATE "connection_profiles" SET "allowTierFallback" = 0 WHERE
    // "allowTierFallback" IS NULL` (`:69-73`). Measured a no-op here: SQLite's
    // `ADD COLUMN ... DEFAULT 0` back-fills every existing row with 0, so the
    // UPDATE can never match. Recorded rather than transcribed.
    table: 'connection_profiles',
    column: 'allowTierFallback',
    sql: 'ALTER TABLE "connection_profiles" ADD COLUMN "allowTierFallback" INTEGER DEFAULT 0',
    source: 'migrations/scripts/add-profile-fallback-fields.ts:32,62',
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
  {
    // P4.94 (v4 `7fbf8a55b`, the P4.D182 avatar configuration cache). The
    // first of THREE rows in this list whose v4 migration issues a SECOND
    // statement (the two `llm_logs` rows above are the others): an index a Zod
    // field cannot express, so `generateDDL` never emits it and `compareSchemas`
    // — which populates neither `addedIndexes` nor `removedIndexes` — cannot see
    // it either. v4 creates THIS one inside a `run()` gated on the COLUMN being
    // absent; the `llm_logs` pair's indexes are created OUTSIDE their column
    // guard (`add-llm-logs-profile-columns.ts:84`, after the `if` closes at
    // `:82`) — which is why the apply loop below runs `extraSql` whenever the
    // TABLE exists, not only when the column was just added.
    //
    // This is the column behind four of the six ordered reds this widen closes
    // (five of seven with the latent `images_generate_route`; the two pascal
    // families are the P4.D171 `chats` pair): v5's
    // `files` INSERT is a fixed column list that always binds `generationKey`,
    // where v4's insert names only the keys its data object carries — so v4
    // writes happily to a pre-4.10 `files` table and v5 answers
    // `no such column` (`db/files_generation_key_repair.rs`).
    table: 'chats',
    column: 'transcriptVersion',
    sql: 'ALTER TABLE "chats" ADD COLUMN "transcriptVersion" INTEGER DEFAULT 0',
    // `addColumnIfMissing('chats', 'transcriptVersion', 'INTEGER DEFAULT 0')`
    // expands through `migrations/lib/database-utils.ts:354` to exactly this
    // quoting. See the header for why this row exists.
    source: 'migrations/scripts/add-transcript-version-column-v1.ts:53',
  },
  {
    table: 'files',
    column: 'generationKey',
    sql: 'ALTER TABLE "files" ADD COLUMN "generationKey" TEXT',
    extraSql: [
      'CREATE INDEX IF NOT EXISTS "idx_files_generationKey" ON "files" ("generationKey")',
    ],
    source: 'migrations/scripts/add-file-generation-key-column-v1.ts:54,57',
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

  for (const path of paths) {
    // `--report-only` opens read-only so a dry run cannot leave journal residue.
    // The pepper → key in the raw-hex form (KDF skipped), exactly as the rest of
    // the harness opens a test-pepper DB; try each known one in turn.
    let db: InstanceType<typeof Database> | null = null;
    for (const pepper of TEST_PEPPERS) {
      const candidate = new Database(path, reportOnly ? { readonly: true } : {});
      try {
        candidate.pragma(
          `key = "x'${Buffer.from(pepper.base64, 'base64').toString('hex')}'"`,
        );
        candidate.prepare(`SELECT name FROM sqlite_master LIMIT 1`).get();
        db = candidate;
        break;
      } catch {
        candidate.close();
      }
    }
    if (!db) {
      throw new Error(
        `no known test pepper opens ${path} (tried: ${TEST_PEPPERS.map((p) => p.name).join(', ')})`,
      );
    }
    const applied: string[] = [];
    for (const m of MIGRATIONS) {
      const table = db
        .prepare(`SELECT name FROM sqlite_master WHERE type='table' AND name=?`)
        .get(m.table);
      if (!table) continue; // this partition does not carry the table
      const cols = (db.prepare(`PRAGMA table_info("${m.table}")`).all() as { name: string }[]).map(
        (c) => c.name,
      );
      // The ALTER first, when the column is absent (v4's guard) …
      if (!cols.includes(m.column)) {
        if (!reportOnly) db.exec(m.sql);
        applied.push(`${m.table}.${m.column}`);
      }
      // … and the index statements ALWAYS, after it: v4's `llm_logs` migration
      // creates its indexes on every `run()` (`add-llm-logs-profile-
      // columns.ts:84`, after the `if` at `:82` closes), and every statement
      // here is `IF NOT EXISTS`, so a partition carrying the column but not its
      // index still gets the index — the shape v5's own boot ensures produce.
      // ⚠ The order matters: the `53294163f` unification's index-gating fix put
      // the `extraSql` loop BEFORE the ALTER and the guard, which only works on a
      // partition that already carries the column — on one that does not,
      // `CREATE INDEX … ("generationKey")` died `no such column` before the
      // ALTER ran (found by the `bcd7e4852` unification widening
      // `attach-file-main.db`, the sixth pair).
      if (!reportOnly) for (const extra of m.extraSql ?? []) db.exec(extra);
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
