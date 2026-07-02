/**
 * SEED fixture builder — the user-identity resolver (v4
 * `lib/services/chat-message/user-identity-resolver.service.ts`
 * `resolveUserIdentity`), ported as
 * `quilltap-core::services::user_identity_resolver`.
 *
 * Builds a two-database fixture (main + an empty-but-valid mount-index):
 *   - Users: seeded via v4's REAL `repos.users.create` (ids + timestamps pinned).
 *     One has a NULL `name` (the default-fallback case).
 *   - Characters: seeded SLIM (null vault mount) via a thin subclass exposing the
 *     protected `_create`, so `repos.characters.findById` overlays nothing and
 *     reads back the slim row (name + the null-mount defaults). Some are
 *     user-controlled, one is llm-controlled.
 *   - Chats: created via v4's REAL `repos.chats.create` (ids + participants +
 *     timestamps pinned). No messages — the resolver reads only participants.
 *
 * Both the oracle (`user-identity.ts`) and the Rust port then run the SAME op
 * sequence (a READ path — `resolveUserIdentity` writes nothing) on a COPY of this
 * fixture, so the resolved identity objects compare exactly.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-useridentity-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-useridentity-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-user-identity-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface UserSeed {
  id: string;
  username: string;
  name: string | null;
}
interface CharSeed {
  id: string;
  userId: string;
  name: string;
  controlledBy: 'llm' | 'user';
}
interface ChatSeed {
  id: string;
  userId: string;
  title: string;
  participants: Array<Record<string, unknown>>;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  users: UserSeed[];
  characters: CharSeed[];
  chats: ChatSeed[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'user-identity.json');
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-useridentity-fixture-build-'));
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
  const { UsersRepository } = await import('@/lib/database/repositories/users.repository');
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

  // Users.
  const usersRepo = new UsersRepository();
  for (const u of spec.users) {
    await usersRepo.create(
      { username: u.username, name: u.name } as never,
      { id: u.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

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
      { userId: c.userId, name: c.name, controlledBy: c.controlledBy },
      { id: c.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  // Chats.
  const chatRepo = new ChatsRepository();
  for (const c of spec.chats) {
    await chatRepo.create(
      { userId: c.userId, title: c.title, participants: c.participants } as never,
      { id: c.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  await closeDatabase();
  await closeMountIndexSQLiteClient();
  process.stderr.write(
    `built user-identity fixture: main=${out} mount=${outMount} ` +
      `(${spec.users.length} users, ${spec.characters.length} characters, ${spec.chats.length} chats)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`user-identity fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
