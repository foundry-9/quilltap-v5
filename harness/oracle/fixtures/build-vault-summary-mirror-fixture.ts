/**
 * Tier-2 fixture builder — W4.6b conversation-summary vault mirror + delete sweep
 * (v4 `lib/file-storage/conversation-summary-vault-bridge.ts`).
 *
 * A character spans TWO databases, so this builds TWO fixtures — the shared seed
 * state both differential sides start from:
 *   - the MAIN db (QT_FIXTURE_VSM_MAIN): the slim `characters` table (via v4's own
 *     repos.characters.create, which provisions each vault), the `chats` rows (via
 *     repos.chats.create, both characters as participants), and a materialized
 *     `chat_messages` table (getMessageCount) so the Rust port's delete-sweep
 *     `clear_messages` has a table to hit (the Rust core issues no DDL);
 *   - the MOUNT-INDEX db (QT_FIXTURE_VSM_MOUNT): the document-store tables,
 *     materialized via v4's generated DDL (idempotent CREATE TABLE) — they must
 *     pre-exist because the Rust port never issues DDL. Same recipe as
 *     build-characters-create-fixture.ts.
 *
 * `doc_mount_chunks` IS materialized so v4's post-write `reindexSingleFile` (run by
 * both `repos.characters.create` here and `writeConversationSummaryToVaults` in the
 * oracle) runs cleanly; the differential pins the resulting link chunkCount and
 * excludes that table.
 *
 * Everything is PINNED (character ids, chat ids, participant ids, seed timestamps),
 * and the summary ops (in the oracle / Rust test) pass explicit timestamps too, so
 * the authored file bytes are byte-identical on both sides.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_VSM_MAIN=/tmp/qt-vsm-main.db \
 *   QT_FIXTURE_VSM_MOUNT=/tmp/qt-vsm-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-vault-summary-mirror-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharacterSpec {
  id: string;
  name: string;
  controlledBy: string;
}
interface ParticipantSpec {
  id: string;
  characterId: string;
  controlledBy: string;
}
interface ChatSpec {
  id: string;
  title: string;
  participants: ParticipantSpec[];
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  characters: CharacterSpec[];
  chats: ChatSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'vault-summary-mirror-tier2.json'), 'utf8')
  ) as Spec;

  const mainOut = process.env.QT_FIXTURE_VSM_MAIN;
  const mountOut = process.env.QT_FIXTURE_VSM_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_VSM_MAIN and QT_FIXTURE_VSM_MOUNT must both point at the .db files to write'
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-vsm-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
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
  const repos = getRepositories();
  const TS = spec.seedTimestamp;

  // MOUNT-INDEX db: materialize every store table the create/provision/write path
  // touches, via v4's own generated DDL (idempotent).
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

  // TWO characters, each with a fully provisioned vault (pinned id).
  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        aliases: [],
        controlledBy: c.controlledBy,
      } as never,
      { id: c.id }
    );
  }

  // TWO chats, each carrying BOTH characters as CHARACTER participants (pinned
  // ids/timestamps). One participant is user-controlled per chat.
  for (const chat of spec.chats) {
    const participants = chat.participants.map((p) => ({
      id: p.id,
      type: 'CHARACTER',
      characterId: p.characterId,
      controlledBy: p.controlledBy,
      status: 'active',
      createdAt: TS,
      updatedAt: TS,
    }));
    await repos.chats.create(
      {
        userId: spec.userId,
        title: chat.title,
        participants,
        chatType: 'salon',
      } as never,
      { id: chat.id, createdAt: TS, updatedAt: TS }
    );
    // Force chat_messages into existence (empty) so the Rust delete-sweep's
    // clear_messages has a table (v4 creates it lazily; the Rust core owns no DDL).
    await repos.chats.getMessageCount(chat.id);
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built vault-summary-mirror fixtures: main=${mainOut} mount=${mountOut} ` +
      `(${spec.characters.length} characters, ${spec.chats.length} chats)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`vault-summary-mirror fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
