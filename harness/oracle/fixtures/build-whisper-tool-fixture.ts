/**
 * SEED fixture builder — the whisper tool handler (W4.1d batch 1; v4
 * `lib/tools/handlers/whisper-handler.ts` `executeWhisperTool`).
 *
 * Bakes the seed both differential sides start from, across BOTH databases:
 *   - the participant characters via v4's REAL `repos.characters.create` (pinned
 *     ids, full vault provisioning — the whisper handler resolves names AND
 *     aliases through `repos.characters.findById`, the vault overlay, so aliases
 *     are baked into `properties.json`);
 *   - four multi-character chats via `repos.chats.create` (pinned ids/timestamps,
 *     CHARACTER participants with pinned ids + a mix of active/silent/absent
 *     statuses to exercise `canReceiveWhisper`): three single-target chats for the
 *     success writes (by name, by alias, and a double-encoded/sanitized message)
 *     and one shared chat for the failure ops (which write no row).
 *
 * The oracle then COPIES this seed, runs the whisper ops via v4's REAL
 * `executeWhisperTool`, and dumps `chat_messages` (main DB). The minted whisper
 * message id + createdAt are remap/sentinel-normalized in the diff.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-whisper-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-whisper-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-whisper-tool-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharacterSpec {
  id: string;
  name: string;
  controlledBy: string;
  aliases: string[];
}
interface ParticipantSpec {
  id: string;
  character: string;
  status: string;
}
interface ChatSpec {
  id: string;
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
  const spec = JSON.parse(readFileSync(join(here, 'whisper-tool.json'), 'utf8')) as Spec;

  const outMain = process.env.QT_FIXTURE_OUT;
  const outMount = process.env.QT_FIXTURE_MOUNT_OUT;
  if (!outMain || !outMount) {
    throw new Error('QT_FIXTURE_OUT and QT_FIXTURE_MOUNT_OUT must point at the fixture .db files');
  }
  for (const base of [outMain, outMount]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = base + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-whisper-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
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
    for (const sql of generateDDL(name, schema as never)) {
      midb.exec(sql);
    }
  }

  // Characters (full vault provisioning, pinned ids, aliases baked into
  // properties.json so the overlay surfaces them).
  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        controlledBy: c.controlledBy,
        aliases: c.aliases,
      } as never,
      { id: c.id }
    );
  }

  // Chats + participants (CHARACTER, pinned ids + explicit statuses).
  for (const chat of spec.chats) {
    const participants = chat.participants.map((p) => ({
      id: p.id,
      type: 'CHARACTER',
      characterId: p.character,
      controlledBy: 'llm',
      status: p.status,
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    }));
    await repos.chats.create(
      {
        userId: spec.userId,
        title: `Fixture ${chat.id}`,
        participants,
        chatType: 'salon',
        contextSummary: null,
      } as never,
      { id: chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
    // Force the `chat_messages` collection to materialize (v4 creates it lazily
    // on first access; `chats.create` never touches it). The whisper write + the
    // Rust dump both need the table to exist on the seed.
    await repos.chats.getMessageCount(chat.id);
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built whisper-tool fixture: ${outMain} + ${outMount} ` +
      `(${spec.characters.length} characters, ${spec.chats.length} chats)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`whisper-tool fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
