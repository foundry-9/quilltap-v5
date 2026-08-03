/**
 * Tier-2 SEED fixture builder — the turn-orchestration decision core (v4
 * `lib/services/chat-message/turn-orchestrator.service.ts` `shouldChainNext` /
 * `persistTurnParticipantId`, and the `actions/turn.ts` `handleTurnAction`
 * mutation core), ported as `quilltap-core::services::turn_orchestrator`.
 *
 * Builds a two-database fixture (main + an empty-but-valid mount-index):
 *   - Characters: seeded SLIM (null vault mount) via a thin subclass exposing the
 *     real protected `_create`, so `repos.characters.findById` overlays nothing
 *     and reads back the slim row (name + the Zod-default talkativeness 0.5). The
 *     per-participant `talkativeness` override on the chat carries the real
 *     weights; the character talkativeness is a constant 0.5.
 *   - Chats: created via v4's REAL `repos.chats.create` (ids + participants +
 *     createdAt/updatedAt pinned). Messages are added via `repos.chats.addMessages`
 *     (message ids + createdAt pinned) — which mints chat-metadata
 *     `lastMessageAt`/`updatedAt`/`spokenThisCycleParticipantIds` as a side effect
 *     — so afterward the builder RESETS the turn columns
 *     (`turnQueue`/`spokenThisCycleParticipantIds`/`lastTurnParticipantId`/
 *     `lastMessageAt`/`isPaused`) and `updatedAt` back to the pinned seed sentinel
 *     via `repos.chats.update(id, { ..., updatedAt: seedTimestamp })`, giving a
 *     fully deterministic starting state.
 *
 * Both the oracle (`turn-orchestrator-tier2.ts`) and the Rust port then run the
 * SAME op sequence on a COPY of this fixture. The orchestrator writes never mint a
 * chat `updatedAt` (v4 `_update` preserves it, and none of these ops pass one), so
 * the differential is ZERO-normalization: the final `chats` dump is compared
 * byte-for-byte and every op's returned decision object is compared field-for-field.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-turnorch-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-turnorch-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-turn-orchestrator-fixture.ts
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
  isPaused: boolean;
  turnQueue: string;
  spokenThisCycleParticipantIds: string;
  participants: Array<Record<string, unknown>>;
  messages: Array<Record<string, unknown>>;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  characters: CharSeed[];
  chats: ChatSeed[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'turn-orchestrator-tier2.json');
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-turnorch-fixture-build-'));
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
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('characters', CharacterSchema);

  // Materialize the (empty) mount-index content tables so the port can open the
  // partition and the vault overlay resolves cleanly (no character is linked).
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
      {
        userId: spec.userId,
        name: c.name,
        controlledBy: c.controlledBy,
      },
      { id: c.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  const repo = new ChatsRepository();
  for (const c of spec.chats) {
    await repo.create(
      {
        userId: spec.userId,
        title: c.title,
        participants: c.participants,
        isPaused: c.isPaused,
        turnQueue: c.turnQueue,
        spokenThisCycleParticipantIds: c.spokenThisCycleParticipantIds,
      } as never,
      { id: c.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );

    if (c.messages.length > 0) {
      // addMessages mints chat metadata as a side effect; reset it below.
      await repo.addMessages(
        c.id,
        c.messages.map((m) => ({
          type: 'message',
          ...m,
          createdAt: spec.seedTimestamp,
        })) as never
      );
    }

    // Reset the turn columns + updatedAt + lastMessageAt to the pinned seed so
    // the starting state is fully deterministic (the ops write these; the diff
    // is zero-normalization from here).
    await repo.update(c.id, {
      turnQueue: c.turnQueue,
      spokenThisCycleParticipantIds: c.spokenThisCycleParticipantIds,
      lastTurnParticipantId: null,
      lastMessageAt: null,
      isPaused: c.isPaused,
      updatedAt: spec.seedTimestamp,
    } as never);
  }

  await closeDatabase();
  await closeMountIndexSQLiteClient();
  process.stderr.write(
    `built turn-orchestrator fixture: main=${out} mount=${outMount} ` +
      `(${spec.chats.length} chats, ${spec.characters.length} characters)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`turn-orchestrator fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
