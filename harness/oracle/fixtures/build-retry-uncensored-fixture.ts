/**
 * Fixture builder for the P4.D228 "Try uncensored" tier-3 differential (v4
 * `lib/services/dangerous-content/retry-uncensored.ts` + the two route
 * handlers, NEW at `ce2f1dabf`, #77).
 *
 * A PURPOSE-BUILT fixture (the P4.D200 `doc_opacity` precedent): v4's own unit
 * tests for #77 mock the gate, the repositories and the generators, so none of
 * them is an oracle. Everything here is written through v4's REAL repositories
 * at the pin, so the stored bytes (the `routeTrail` rows in the schema's key
 * order, the TOOL rows' content JSON, the `conciergeSettings` object) are v4's:
 *
 *   - two users — the operator, whose configured desk has one uncensored text
 *     and one uncensored image profile; and a second user with NO uncensored
 *     desk and one uncensored-compatible profile of each kind (the one the
 *     cases exclude), so `no-understudy` is reachable;
 *   - connection profiles: the responder (moderated), the configured desk, a
 *     SAME provider+model twin of the desk, and a second desk on another model;
 *   - image profiles: the chat's painter (moderated), the configured painter,
 *     its same-model twin, and a second painter on another model;
 *   - two characters: Bertie (defaults to the responder profile) and one with
 *     NO default connection profile (v4's `resolveConnectionProfile` throws →
 *     the service's DEBUG line);
 *   - chats: Moderated, Locked, Unmoderated, an exempt `help` chat, and the
 *     second user's two (one with nobody present);
 *   - messages: an ASSISTANT row with a refusal trail, a soft refusal (no
 *     trail), a row the configured desk already answered (the same-model
 *     exclusion), one by the profile-less character, a Lantern staff row, a
 *     USER row; TOOL rows: a refused `generate_image` (an image trail), a soft
 *     one, one the desk drew, one stored WITHOUT milliseconds, an `rng` row,
 *     one with no `arguments`, and one whose content is not JSON.
 *
 * Then v4's own `add-chat-refusal-ledger-v1` migration (the ledger columns
 * the Zod-built `chats` table omits — the refusal-ledger builder's step).
 *
 * Characters are vault-backed in v4, so the builder writes a mount index too
 * (the `build-regenerate-swipe-fixture.ts` arrangement, `doc_mount_blobs`
 * created BEFORE the first character — see that builder's note).
 *
 * Run (Node 24, from a PINNED v4 worktree — the sweep driver's `--v4`):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   QT_FIXTURE_OUT=/tmp/qt-retry-uncensored-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-retry-uncensored-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-retry-uncensored-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  users: Record<string, string>;
  seedTimestamp: string;
  chatSettings: Array<Record<string, unknown> & { id: string; userId: string; conciergeSettings: Record<string, unknown> }>;
  connectionProfiles: Array<Record<string, unknown> & { id: string }>;
  imageProfiles: Array<Record<string, unknown> & { id: string }>;
  characters: Array<Record<string, unknown> & { id: string }>;
  chats: Array<
    Record<string, unknown> & {
      id: string;
      userId: string;
      messages?: Array<Record<string, unknown> & { id: string }>;
    }
  >;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'retry-uncensored.json'), 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  const outMount = process.env.QT_FIXTURE_MOUNT_OUT;
  if (!out || !outMount) {
    throw new Error('QT_FIXTURE_OUT and QT_FIXTURE_MOUNT_OUT must point at the fixture .db files');
  }
  for (const base of [out, outMount]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = base + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-retry-uncensored-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { ConciergeSettingsSchema } = await import('@/lib/schemas/settings.types');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  const repos = getRepositories();

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  // v4's blobs repository's hand-written DDL (`doc-mount-blobs.repository.ts`),
  // BEFORE the first character create (the regenerate-swipe builder's note).
  midb.exec(`
    CREATE TABLE IF NOT EXISTS "doc_mount_blobs" (
      "id" TEXT PRIMARY KEY,
      "fileId" TEXT NOT NULL,
      "sha256" TEXT NOT NULL,
      "sizeBytes" INTEGER NOT NULL,
      "storedMimeType" TEXT NOT NULL,
      "data" BLOB NOT NULL,
      "createdAt" TEXT NOT NULL,
      "updatedAt" TEXT NOT NULL,
      FOREIGN KEY ("fileId") REFERENCES "doc_mount_files" ("id") ON DELETE CASCADE
    )
  `);
  midb.exec(
    'CREATE UNIQUE INDEX IF NOT EXISTS "idx_doc_mount_blobs_fileId" ON "doc_mount_blobs" ("fileId")',
  );
  const ts = spec.seedTimestamp;

  // The route middleware answers "User not found" without a users row.
  for (const [label, userId] of Object.entries(spec.users)) {
    await repos.users.create(
      { username: `retry-${label}`, email: null, name: label } as never,
      { id: userId, createdAt: ts, updatedAt: ts } as never,
    );
  }
  for (const cs of spec.chatSettings) {
    const { id, conciergeSettings, ...rest } = cs;
    await repos.chatSettings.create(
      { ...rest, conciergeSettings: ConciergeSettingsSchema.parse(conciergeSettings) } as never,
      { id, createdAt: ts, updatedAt: ts },
    );
  }
  for (const p of spec.connectionProfiles) {
    const { id, ...data } = p;
    await repos.connections.create(data as never, { id, createdAt: ts, updatedAt: ts });
  }
  for (const p of spec.imageProfiles) {
    const { id, ...data } = p;
    await repos.imageProfiles.create(data as never, { id, createdAt: ts, updatedAt: ts });
  }
  for (const c of spec.characters) {
    const { id, ...data } = c;
    await repos.characters.create(data as never, { id });
  }

  for (const c of spec.chats) {
    const { id, messages: _messages, ...rest } = c;
    await repos.chats.create(rest as never, { id, createdAt: ts, updatedAt: ts });
  }
  // `chats.transcriptVersion` arrives by MIGRATION only (v4 keeps it out of
  // `ChatMetadataSchema`), so a Zod-built table lacks it and every
  // `addMessage` would log "Failed to bump transcript version". Added AFTER the
  // creates (v4's collections are lazy) and BEFORE the messages, so the seed's
  // own bumps are part of the fixture — the chats-messages-ops builder's step.
  const chatCols = ((await rawQuery('PRAGMA table_info(chats)')) as Array<{ name: string }>).map(
    (c) => c.name,
  );
  if (!chatCols.includes('transcriptVersion')) {
    await rawQuery('ALTER TABLE "chats" ADD COLUMN "transcriptVersion" INTEGER DEFAULT 0');
  }
  let nMessages = 0;
  for (const c of spec.chats) {
    for (const m of c.messages ?? []) {
      await repos.chats.addMessage(c.id, { type: 'message', attachments: [], ...m } as never);
      nMessages += 1;
    }
  }
  // Materialize `background_jobs` (lazy in v4) so the queue arms and the
  // oracle's planted job see the table on both sides.
  await repos.backgroundJobs.findPendingForChat(spec.chats[0].id);

  closeMountIndexSQLiteClient();
  await closeDatabase();

  const { runV4Migrations, ADD_CHAT_REFUSAL_LEDGER } = await import('../lib/v4-migrations');
  const migrated = await runV4Migrations({
    dbPath: out,
    pepperBase64: spec.testPepperBase64,
    migrations: [ADD_CHAT_REFUSAL_LEDGER],
  });
  process.stderr.write(`retry-uncensored fixture migrations: ${migrated.join('; ')}\n`);
  process.stderr.write(
    `built retry-uncensored fixture: ${out} (${spec.chats.length} chats, ${nMessages} messages)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`retry-uncensored fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
