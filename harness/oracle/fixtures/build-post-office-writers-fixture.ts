/**
 * Tier-3 SEED fixture builder for the Post Office persona-writer differential
 * (W4.6b — the persona-writer POST functions + the cost/system-event writers).
 *
 * Bakes the seed BOTH differential sides start from, across TWO databases (main +
 * mount-index), via v4's REAL repositories:
 *   - two characters (Ada + Charlie) created through `repos.characters.create`, so
 *     each has a real provisioned vault (the Host `add` announcement reads Ada's
 *     `identity.md` from her vault; `resolveUserCharacterName` reads Charlie's
 *     overlaid name for the `{{user}}` binding);
 *   - one `chats` row with those two as CHARACTER participants — Ada LLM-controlled
 *     (P1) and Charlie user-controlled (P2) — created via `repos.chats.create` with
 *     the id + timestamps pinned to `seedTimestamp`;
 *   - the empty `chat_messages` table materialized (via a `getMessageCount` read) so
 *     the Rust port — which does NOT own DDL — can INSERT into the same table.
 *
 * The op sequence is run on a COPY of this seed on each side.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_POW_MAIN=/tmp/qt-pow-main.db \
 *   QT_FIXTURE_POW_MOUNT=/tmp/qt-pow-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-post-office-writers-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharSeed {
  id: string;
  name: string;
  identity: string;
  description: string;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  chat: { id: string; title: string };
  ada: CharSeed;
  charlie: CharSeed;
  p1Id: string;
  p2Id: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'post-office-writers-tier3.json'), 'utf8')
  ) as Spec;

  const mainOut = process.env.QT_FIXTURE_POW_MAIN;
  const mountOut = process.env.QT_FIXTURE_POW_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_POW_MAIN and QT_FIXTURE_POW_MOUNT must both point at the .db files to write'
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-pow-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();

  // MOUNT-INDEX db: materialize every store table the create/provision/write path
  // touches, via v4's own generated DDL (the Rust port never issues DDL; the store
  // tables must pre-exist). Same recipe as build-characters-create-fixture.ts.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) {
      midb.exec(sql);
    }
  }

  const repos = getRepositories();

  const seedCharacter = async (c: CharSeed): Promise<void> => {
    await repos.characters.create(
      {
        userId: spec.userId,
        name: c.name,
        identity: c.identity,
        description: c.description,
        controlledBy: 'llm',
      } as never,
      { id: c.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  };

  await seedCharacter(spec.ada);
  await seedCharacter(spec.charlie);

  const participants = [
    {
      id: spec.p1Id,
      type: 'CHARACTER',
      characterId: spec.ada.id,
      controlledBy: 'llm',
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    },
    {
      id: spec.p2Id,
      type: 'CHARACTER',
      characterId: spec.charlie.id,
      controlledBy: 'user',
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    },
  ];

  await repos.chats.create(
    {
      userId: spec.userId,
      title: spec.chat.title,
      participants,
    } as never,
    { id: spec.chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
  );

  // Materialize the empty chat_messages table so the Rust port can INSERT.
  await repos.chats.getMessageCount(spec.chat.id);

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built post-office-writers seed fixtures: main=${mainOut} mount=${mountOut}\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`post-office-writers fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
