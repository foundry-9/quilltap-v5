/**
 * Tier-2 SEED fixture builder — the enclave lifecycle service (U4.3; v4
 * `lib/services/chat-message/autonomous-room.service.ts` +
 * `lib/background-jobs/handlers/autonomous-run-start.ts` /
 * `autonomous-room-announce.ts`), ported as `quilltap-core::enclave::{announce,
 * lifecycle}`.
 *
 * Builds a two-database fixture (main + an empty-but-valid mount-index):
 *   - Characters seeded SLIM (null vault mount) via a thin subclass exposing the
 *     real protected `_create` — the banner's overlaid `characters.findById`
 *     resolves to the slim row (no vault linked → overlay no-op).
 *   - Chats created via v4's REAL `repos.chats.create` with the autonomous
 *     run-state / schedule / budget columns seeded per the spec (ids +
 *     timestamps pinned to 2020-era sentinels). No messages are seeded — the
 *     `chat_messages` table is materialized empty (getMessageCount trick) so
 *     both sides start from a clean banner surface.
 *   - `chat_settings` rows for userA (destructiveToolPolicy opt_in_per_room) and
 *     userB (always_refuse); userC deliberately has NO row (banks the
 *     `?? DEFAULT_FRESHNESS_WINDOW_MS` fallback).
 *   - `background_jobs` materialized empty (the enqueues under test insert).
 *
 * Both the oracle (`cases/enclave-lifecycle-tier2.ts`) and the Rust port then
 * run the SAME op sequence on a COPY of this fixture.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-enclave-lifecycle-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-enclave-lifecycle-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-enclave-lifecycle-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharSeed {
  id: string;
  name: string;
  controlledBy: 'llm' | 'user';
}
interface ChatSeed {
  id: string;
  title: string;
  participants: Array<Record<string, unknown>>;
  [column: string]: unknown;
}
interface SettingsSeed {
  id: string;
  userId: string;
  autonomousRoomSettings: Record<string, unknown>;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  users: Record<string, string>;
  chatSettings: SettingsSeed[];
  characters: CharSeed[];
  chats: ChatSeed[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'enclave-lifecycle-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-enclave-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');
  const { CharactersRepository } = await import(
    '@/lib/database/repositories/characters.repository'
  );
  const { ChatSettingsRepository } = await import(
    '@/lib/database/repositories/chat-settings.repository'
  );
  const { CharacterSchema, ChatSettingsSchema, BackgroundJobSchema } = await import(
    '@/lib/schemas/types'
  );
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chat_settings', ChatSettingsSchema);
  await ensureCollection('background_jobs', BackgroundJobSchema);

  // Materialize the (empty) mount-index content tables so the port can open the
  // partition and the banner's vault overlay resolves cleanly (no character is
  // linked to a vault).
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) {
      midb.exec(sql);
    }
  }

  // [40319484] `gcOrphanedFileRow` runs inside every content-addressed rewrite
  // and deletes from `doc_mount_blobs` unconditionally, so a mount index that
  // lacks the table now throws `no such table` on the SECOND write to any path.
  // v4's blobs repository creates it lazily from hand-written DDL
  // (`doc-mount-blobs.repository.ts:113`); copied verbatim so the fixture carries
  // exactly the table a real instance has (it is in `fresh_schema.json` too).
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
    'CREATE UNIQUE INDEX IF NOT EXISTS "idx_doc_mount_blobs_fileId" ON "doc_mount_blobs" ("fileId")'
  );

  const TS = spec.seedTimestamp;

  // Slim characters (null vault mount) via the real protected `_create`.
  class CharactersSqlRepo extends CharactersRepository {
    async createSlim(data: unknown, options: unknown) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      return (this as any)._create(data, options);
    }
  }
  const charRepo = new CharactersSqlRepo();
  for (const c of spec.characters) {
    await charRepo.createSlim(
      { userId: spec.users.userA, name: c.name, controlledBy: c.controlledBy },
      { id: c.id, createdAt: TS, updatedAt: TS }
    );
  }

  // Per-user chat settings (the freshness default + destructive-tool policy).
  const settingsRepo = new ChatSettingsRepository();
  for (const s of spec.chatSettings) {
    await settingsRepo.create(
      { userId: s.userId, autonomousRoomSettings: s.autonomousRoomSettings } as never,
      { id: s.id, createdAt: TS, updatedAt: TS }
    );
  }

  // Chats via the REAL create — every seed column (incl. the autonomous
  // run-state / schedule / budget set) passes straight through the schema.
  const chatRepo = new ChatsRepository();
  for (const c of spec.chats) {
    const { id, title, participants, ...columns } = c;
    await chatRepo.create(
      {
        userId: spec.users.userA,
        title,
        participants: participants.map((p) => ({
          type: 'CHARACTER',
          status: 'active',
          createdAt: TS,
          updatedAt: TS,
          ...p,
        })),
        ...columns,
      } as never,
      { id, createdAt: TS, updatedAt: TS }
    );
  }

  // Force the (empty) chat_messages table into existence so both sides can
  // INSERT banner rows into the copied fixture.
  await chatRepo.getMessageCount(spec.chats[0].id);

  await closeDatabase();
  await closeMountIndexSQLiteClient();
  process.stderr.write(
    `built enclave-lifecycle fixture: main=${out} mount=${outMount} ` +
      `(${spec.chats.length} chats, ${spec.characters.length} characters, ` +
      `${spec.chatSettings.length} settings rows)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`enclave-lifecycle fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
