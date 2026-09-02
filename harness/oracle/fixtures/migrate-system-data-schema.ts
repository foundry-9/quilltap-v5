/**
 * P4.70 fixture MIGRATOR — bring the committed `system-data-*` family up to the
 * baseline schema vintage, in place, through v4's OWN migration code.
 *
 * ## Why a migrator rather than a rebuild
 *
 * The order asked for a rebuild through `build-system-data-fixture.ts` so the
 * DDL would be production-identical, AND for every pre-existing row to be
 * byte-preserved. Measured at P4.70, those two requirements are incompatible:
 *
 *   * the builder does not pin the two character-vault mount-point ids — v4's
 *     real `characters.create` mints them — so a rebuild produces new UUIDs;
 *   * `extend-system-data-archive-substrate.ts:88-89` HARD-CODES those two ids
 *     (`c5e2a72d-…`, `60d9f423-…`), and so does
 *     `harness/oracle/cases/system-import-execute.test.ts:230`. A rebuild +
 *     extend therefore writes the substrate under a mount point that no longer
 *     exists — and does it SILENTLY, landing plausible row counts;
 *   * six committed restore archives and the sha256-guarded
 *     `harness/oracle/fixtures/uuid-remap-corpus.json` are projections of this
 *     family's exact ids.
 *
 * So the family is migrated where it lies. That is not a second-best: it is
 * what a real instance gets. v4 has no hand-written migration files — its
 * migration path is `compareSchemas` + `generateAlterStatements`
 * (`lib/database/schema-translator.ts:477,528`), which emits
 * `ALTER TABLE "<t>" ADD COLUMN "<name>" <type> <constraints>` using the SAME
 * `mapToSQLiteType` + `generateColumnConstraints` that `generateDDL` uses. The
 * migrated column is therefore character-identical to the freshly-created one,
 * and this script calls that code rather than transcribing its output.
 *
 * ## What it closes (measured against `fresh_schema.json` at `6d2a50382`)
 *
 *   main   `connection_profiles`  multiCharacterPrefill, fallbackProfileId, allowTierFallback
 *   main   `characters`           archivedAt, archiveFileId, archivedAvatarFileId
 *   main   `chat_settings`        composerEmoji, composerUnicode, smartTypographySettings
 *   llm    `llm_logs`             connectionProfileId, imageProfileId
 *
 * Only `connection_profiles` was FATAL, and only on the write path: v5's
 * `ConnectionProfilesRepository::create` names `fallbackProfileId` /
 * `allowTierFallback` unconditionally and v4's `insertOne` names
 * `allowTierFallback` (its Zod default makes the key always present), so EVERY
 * connection-profile import threw on both sides and the family's import leg had
 * measured nothing since v4 bug 68. The other eight columns read tolerantly on
 * both sides; they are closed here so the family stops rotting.
 *
 * Idempotent: a column that already exists is skipped, so re-running is a
 * no-op. Adding a future table is one row in `TABLES` below.
 *
 * Run (Node 24, from the v4 checkout — or a pinned worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; W=<the v5 worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/migrate-system-data-schema.ts
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import type { z } from 'zod';

interface Spec {
  testPepperBase64: string;
}

/** `[partition, collection name, the module + export its schema lives in]`. */
const TABLES: Array<{
  partition: 'main' | 'llm';
  collection: string;
  load: () => Promise<z.ZodType>;
}> = [
  {
    partition: 'main',
    collection: 'connection_profiles',
    load: async () => (await import('@/lib/schemas/profile.types')).ConnectionProfileSchema,
  },
  {
    partition: 'main',
    collection: 'characters',
    load: async () => (await import('@/lib/schemas/character.types')).CharacterSchema,
  },
  {
    partition: 'main',
    collection: 'chat_settings',
    load: async () => (await import('@/lib/schemas/settings.types')).ChatSettingsSchema,
  },
  {
    partition: 'llm',
    collection: 'llm_logs',
    load: async () => (await import('@/lib/schemas/llm-log.types')).LLMLogSchema,
  },
];

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, 'system-data.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_SD_MAIN ?? '';
  const mountOut = process.env.QT_FIXTURE_SD_MOUNT ?? '';
  const llmOut = process.env.QT_FIXTURE_SD_LLM ?? '';
  for (const [k, v] of Object.entries({ mainOut, mountOut, llmOut })) {
    if (!v) throw new Error(`${k} env var is required`);
    if (!existsSync(v)) throw new Error(`${k} does not exist: ${v} (migrate, do not create)`);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-sd-migrate-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.SQLITE_LLM_LOGS_PATH = llmOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawLLMLogsDatabase } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const { extractSchemaMetadata, compareSchemas, generateAlterStatements } = await import(
    '@/lib/database/schema-translator'
  );

  await initializeDatabase();

  let added = 0;
  for (const { partition, collection, load } of TABLES) {
    const db = partition === 'main' ? getRawDatabase() : getRawLLMLogsDatabase();
    if (!db) throw new Error(`no raw handle for the ${partition} partition`);

    const current = (
      db.prepare(`PRAGMA table_info("${collection}")`).all() as Array<{
        name: string;
        type: string;
        notnull: number;
      }>
    ).map(c => ({ name: c.name, type: c.type, nullable: c.notnull === 0 }));
    if (current.length === 0) {
      throw new Error(`${collection} is absent from the ${partition} partition`);
    }

    const metadata = extractSchemaMetadata(collection, await load());
    const diff = compareSchemas(metadata, current);
    // ONLY added columns are applied. A removed or retyped field would mean the
    // fixture has drifted in a way an ALTER cannot express, and silently
    // ignoring that is how a fixture rots — so say so and stop.
    if (diff.removedFields.length > 0 || diff.modifiedFields.length > 0) {
      throw new Error(
        `${collection}: the fixture disagrees with the schema in a way ADD COLUMN cannot fix ` +
          `(removed: ${diff.removedFields.join(', ') || 'none'}; ` +
          `modified: ${diff.modifiedFields.map(m => m.field).join(', ') || 'none'})`
      );
    }
    if (diff.addedFields.length === 0) {
      console.log(`  ${partition}/${collection}: already current (${current.length} columns)`);
      continue;
    }
    for (const statement of generateAlterStatements(collection, diff)) {
      db.exec(statement);
      console.log(`  ${partition}/${collection}: ${statement}`);
      added += 1;
    }
  }

  await closeDatabase();
  console.log(`migrated system-data fixtures: ${added} column(s) added`);
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});
