/**
 * P4.28 fixture builder — **the migration-vintage instance**.
 *
 * Every other committed fixture, and every provisioned instance in the harness,
 * gets its schema from v4's `generateDDL` (v5 mirrors it byte-for-byte in
 * `services/provisioning/fresh_schema.json`, D23). **A real long-lived instance
 * does not.** Its tables were created by `migrations/scripts/*` — often years
 * ago, at a different DDL — and grown since by `ALTER TABLE ADD COLUMN`. The two
 * shapes are NOT the same, and the differences are not cosmetic:
 *
 *   - `conversation_annotations` carries `UNIQUE("chatId","messageIndex",
 *     "characterName")` (`sqlite-initial-schema.ts:188`,
 *     `create-conversation-tables.ts:68`); `generateDDL` emits no such index.
 *     The oldest DDL adds `FOREIGN KEY("chatId") REFERENCES "chats"("id")
 *     ON DELETE CASCADE` on top.
 *   - `doc_mount_file_links` carries TWO foreign keys — `fileId` →
 *     `doc_mount_files`, `mountPointId` → `doc_mount_points`, both
 *     `ON DELETE CASCADE` — plus the UNIQUE `(mountPointId, relativePath)`
 *     index (`add-doc-mount-file-links.ts:168`). `generateDDL` emits none of
 *     them.
 *   - `doc_mount_documents` and `doc_mount_blobs` FK their `fileId` to
 *     `doc_mount_files` the same way; `doc_mount_chunks` FKs `linkId`.
 *   - Column sets differ where a migration was added after an instance's table
 *     was created and the aligner could not safely add the column.
 *
 * v5 opens writable connections with `PRAGMA foreign_keys = ON`, so on a
 * migrated instance those constraints are LIVE — which is why the 2026-08-03
 * Part F walk saw failures (`#57`, `#58`) that no differential in this repo can
 * reproduce: they all provision fresh. This fixture is the substrate that closes
 * that blind spot. It is deliberately EMPTY of user rows: its whole value is the
 * DDL, and an empty instance is the one a restore's `replace` mode arrives at
 * anyway once `deleteUserData` has run.
 *
 * ## How it is built — by replay, not by hand
 *
 * The order offered two options: replay v4's real migration chain, or
 * column-drop a fresh fixture to a documented vintage. **This is the replay**,
 * because it is the only one that cannot quietly encode a guess: the DDL here is
 * whatever `MigrationRunner` actually produces, constraints and all, and it
 * moves when v4's migrations move.
 *
 * What that means in practice: a scratch data dir, keyed with the SAME test
 * pepper the restore family uses, with `MigrationRunner().runMigrations()` run
 * over it from nothing. `sqlite-initial-schema` creates the tables at their
 * ORIGINAL DDL and every later migration ALTERs on top — which is exactly the
 * history a real instance has. (A fresh v4 *install* takes the generateDDL path
 * instead; that is the shape every other fixture already has.)
 *
 * ⚠ Not every table a live instance has appears here: v4 creates most
 * collections lazily on first use (`ensureCollection`). A table the chain never
 * touches is simply absent, and that too is a real vintage: the delete/restore
 * paths must tolerate it (they already do — `if_table` / the guarded truncates).
 *
 * Regenerate (Node 24, from the PINNED v4 worktree — see the lane record):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   PIN=/private/tmp/qt-v4-pin-p4.28-c4d4b0de
 *   cd $PIN
 *   QT_FIXTURE_MV_DIR=$W/crates/quilltap-web/tests/fixtures/migration-vintage \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-migration-vintage-fixture.ts
 */

import { mkdtempSync, mkdirSync, copyFileSync, rmSync, existsSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

/** The pepper the restore/backup family keys its instances with. */
const TEST_PEPPER = '3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=';

/** The three partition files, in `DbPaths` order. */
const DB_FILES = ['quilltap.db', 'quilltap-mount-index.db', 'quilltap-llm-logs.db'];

async function main(): Promise<void> {
  const out = process.env.QT_FIXTURE_MV_DIR;
  if (!out) throw new Error('set QT_FIXTURE_MV_DIR to the committed fixture directory');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-migration-vintage-'));
  // `QUILLTAP_DATA_DIR` is the instance BASE; v4's `getDataDir()` appends
  // `data/`, and the mount-index and llm-logs paths are resolved from THAT —
  // so pointing the env at `<scratch>/data` would put the siblings in
  // `<scratch>/data/data` while `SQLITE_PATH` (an absolute override) stayed
  // put, and only the main partition would land where this script looked.
  const dataDir = join(scratch, 'data');
  mkdirSync(dataDir, { recursive: true });

  // `ENCRYPTION_MASTER_PEPPER` is what makes `getSQLiteDatabase` issue the
  // `PRAGMA key` (v4's `better-sqlite3` dependency is an alias for
  // `better-sqlite3-multiple-ciphers`, so the raw driver speaks sqleet).
  process.env.QUILLTAP_DATA_DIR = scratch;
  process.env.SQLITE_PATH = join(dataDir, 'quilltap.db');
  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;

  const { MigrationRunner } = await import('@/migrations/index');
  const result = await new MigrationRunner().runMigrations();
  if (!result.success) {
    throw new Error(
      `migration chain failed: ${result.error ?? ''} failed=[${result.failed.join(', ')}]`
    );
  }

  // ── The mount-index partition needs a second beat ─────────────────────────
  //
  // The chain only authors `doc_mount_points` and `doc_mount_folders` there
  // (the three `provision-*-mount` migrations do it on their way to seeding the
  // built-in stores). The other five tables are authored at RUNTIME, by
  // `generateDDL` over the mount-index Zod schemas, the first time a live
  // instance touches them — which is what a modern instance has, and what every
  // other committed fixture and `provision_fresh_instance` produce.
  //
  // Exactly ONE of those five differs on an instance old enough to have been
  // through the content-addressing refactor: `doc_mount_file_links`, which
  // `add-doc-mount-file-links` authors with TWO `ON DELETE CASCADE` foreign keys
  // and a UNIQUE `(mountPointId, relativePath)` index that `generateDDL` does not
  // emit. That table is the whole reason this fixture exists, so it is created by
  // running **v4's own migration**, before the generateDDL pass — its step 1 is a
  // `CREATE TABLE IF NOT EXISTS`, so authoring order decides which DDL wins, and
  // its later steps are no-ops here because the pre-refactor tables they rebuild
  // do not exist.
  //
  // ⚠ Recorded limitation, measured rather than assumed: on a real pre-refactor
  // instance `doc_mount_documents` and `doc_mount_chunks` ALSO come out of that
  // migration's rebuild steps carrying `fileId` / `linkId` foreign keys, and this
  // fixture takes the generateDDL shape for them instead — reaching the rebuild
  // would mean hand-writing the pre-refactor tables it upgrades, i.e. exactly the
  // transcription this builder exists to avoid. (`doc_mount_blobs` needs no
  // special handling: generateDDL emits its `fileId` foreign key already.) No v5
  // path is known to depend on the two missing constraints; if one turns up, the
  // honest extension is to import `convert-project-files-to-document-stores`'
  // TABLE_DDL rather than to copy it here.
  {
    // A BARE specifier would resolve from THIS file's directory (the v5 tree),
    // which has no node_modules — the standing oracle trap. Resolve it out of
    // the v4 checkout instead. (`better-sqlite3` is an alias for
    // `better-sqlite3-multiple-ciphers` in v4's package.json, so this is the
    // cipher driver.)
    const Database = (
      await import(join(process.cwd(), 'node_modules', 'better-sqlite3', 'lib', 'index.js'))
    ).default;
    const mountPath = join(dataDir, 'quilltap-mount-index.db');
    const { addDocMountFileLinksMigration } = await import(
      '@/migrations/scripts/add-doc-mount-file-links'
    );
    const linkResult = await addDocMountFileLinksMigration.run();
    if (!linkResult.success) {
      throw new Error(`add-doc-mount-file-links failed: ${linkResult.error ?? linkResult.message}`);
    }

    const { generateDDL } = await import('@/lib/database/schema-translator');
    const {
      DocMountFileSchema,
      DocMountDocumentSchema,
      DocMountChunkSchema,
      ProjectDocMountLinkSchema,
    } = await import('@/lib/schemas/mount-index.types');
    const db = new Database(mountPath);
    db.pragma(`key = "x'${Buffer.from(TEST_PEPPER, 'base64').toString('hex')}'"`);
    try {
      // NOT `doc_mount_file_links` — v4's migration just authored it and a
      // generateDDL `CREATE TABLE` would either collide or (worse) win.
      const ddl: Array<[string, unknown]> = [
        ['doc_mount_files', DocMountFileSchema],
        ['doc_mount_documents', DocMountDocumentSchema],
        ['doc_mount_chunks', DocMountChunkSchema],
        ['project_doc_mount_links', ProjectDocMountLinkSchema],
      ];
      for (const [name, schema] of ddl) {
        for (const sql of generateDDL(name, schema as never)) db.exec(sql);
      }
      // `doc_mount_blobs` has no Zod schema of its own in v4 (its repository
      // hand-writes the DDL); take that same statement, which is also what
      // v5's `fresh_schema.json` carries.
      // The two schema aligners the chain itself runs on every boot
      // (`migrations/lib/mount-index-schema.ts`) — this is how a real instance
      // gains the per-document policy columns on a link table that was authored
      // before they existed. Without them the fixture would be a vintage no
      // instance actually reaches: post-refactor but pre-policy-flags.
      const { alignDocMountPointsSchema, alignDocMountFileLinksSchema } = await import(
        '@/migrations/lib/mount-index-schema'
      );
      alignDocMountPointsSchema(db);
      alignDocMountFileLinksSchema(db);
      db.exec(`CREATE TABLE IF NOT EXISTS "doc_mount_blobs" (
          "id" TEXT PRIMARY KEY,
          "fileId" TEXT NOT NULL,
          "sha256" TEXT NOT NULL,
          "sizeBytes" INTEGER NOT NULL,
          "storedMimeType" TEXT NOT NULL,
          "data" BLOB NOT NULL,
          "createdAt" TEXT NOT NULL,
          "updatedAt" TEXT NOT NULL,
          FOREIGN KEY ("fileId") REFERENCES "doc_mount_files" ("id") ON DELETE CASCADE
        )`);
    } finally {
      db.close();
    }
  }

  const { closeSQLite } = await import('@/migrations/lib/database-utils');
  closeSQLite();

  mkdirSync(out, { recursive: true });
  const copied: string[] = [];
  for (const name of DB_FILES) {
    const src = join(dataDir, name);
    if (!existsSync(src)) continue;
    // A WAL sidecar would carry rows the .db file does not; the runner's
    // `closeSQLite` checkpoints, but be explicit that only the .db is copied.
    copyFileSync(src, join(out, name));
    copied.push(name);
  }
  if (!copied.includes('quilltap.db')) {
    throw new Error(`the chain produced no main database (dir held: ${readdirSync(dataDir)})`);
  }

  console.log(
    `built migration-vintage fixture → ${out} ` +
      `(${result.migrationsRun} migrations run, ${result.migrationsSkipped} skipped; ` +
      `files: ${copied.join(', ')})`
  );
  rmSync(scratch, { recursive: true, force: true });
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
