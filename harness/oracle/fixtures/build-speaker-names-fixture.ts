/**
 * Tier-2 fixture builder for the speaker-name resolver (P4.D212; v4
 * `lib/chat/speaker-names.ts`, `e7821606f`, bug 161).
 *
 * Bakes the seats both differential sides resolve: the characters via v4's
 * REAL `repos.characters.create` (pinned ids, vaults provisioned in the
 * mount-index DB) and the chats via v4's REAL `repos.chats.create` (pinned
 * ids). Two shapes v4's SCHEMAS forbid are asked of v4's real write path
 * FIRST, and planted by direct SQL on the built fixture only if that write
 * refuses — so every plant is a measured necessity, not an assumption (the
 * resolver must survive both shapes on a real instance whatever the write
 * path allows):
 *
 *   - the EMPTY-NAME character. MEASURED at `a2db63da7`: `characters.create`
 *     ACCEPTS `name: ''` (the create path does not enforce the slim schema's
 *     `min(1)`), so no plant is needed — the fallback (placeholder name, then
 *     `UPDATE characters SET name = ''`) stays for a v4 that starts refusing;
 *   - the seats with NO `characterId` / an EMPTY one
 *     (`ChatParticipantBaseSchema.characterId` is a required `UUIDSchema`) —
 *     if `chats.create` refuses, created with a valid id and the stored
 *     `participants` JSON rewritten (key deleted / set to `""`). Either way
 *     v4's `chats.findById` answers NULL for that chat (its read validates),
 *     which is why the spec reads it `raw`.
 *
 * Both the oracle and the Rust test then READ a COPY of this one fixture.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SPEAKER_NAMES_MAIN=/tmp/qt-speaker-names-main.db \
 *   QT_FIXTURE_SPEAKER_NAMES_MOUNT=/tmp/qt-speaker-names-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-speaker-names-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharacterSpec {
  id: string;
  name: string;
  controlledBy: string;
  plantEmptyName?: boolean;
}
interface SeatSpec {
  id: string;
  characterId: string;
  controlledBy: string;
  status: string;
  plant?: 'deleteCharacterId' | 'emptyCharacterId';
}
interface ChatSpec {
  name: string;
  id: string;
  participants: SeatSpec[];
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
  const spec = JSON.parse(readFileSync(join(here, 'speaker-names.json'), 'utf8')) as Spec;

  const outMain = process.env.QT_FIXTURE_SPEAKER_NAMES_MAIN;
  const outMount = process.env.QT_FIXTURE_SPEAKER_NAMES_MOUNT;
  if (!outMain || !outMount) {
    throw new Error(
      'QT_FIXTURE_SPEAKER_NAMES_MAIN and QT_FIXTURE_SPEAKER_NAMES_MOUNT must point at the .db files to write'
    );
  }
  for (const base of [outMain, outMount]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = base + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-speaker-names-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { CharacterSchema } = await import('@/lib/schemas/types');
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
  await ensureCollection('characters', CharacterSchema);

  // The vault scaffold `characters.create` runs needs the mount-index store
  // tables (the characters-read builder's set).
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
  // [40319484] v4's blobs repository creates this lazily from hand-written DDL
  // (`doc-mount-blobs.repository.ts`); copied verbatim, as the sibling builders do.
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

  const repos = getRepositories();
  const planted: string[] = [];

  // The empty-name character: ASK v4's real create for `name: ''` first, and
  // plant only if it refuses — so the plant is a measured necessity, not an
  // assumption. `emptyNameVia` records which path the fixture took.
  let emptyNameVia = 'n/a';
  for (const c of spec.characters) {
    if (c.plantEmptyName) {
      try {
        await repos.characters.create(
          { name: '', userId: spec.userId, controlledBy: c.controlledBy } as never,
          { id: c.id }
        );
        emptyNameVia = 'create accepted name ""';
        continue;
      } catch (err) {
        emptyNameVia = `create refused name "" (${String((err as Error)?.message ?? err).split('\n')[0].slice(0, 80)}…); planted`;
      }
    }
    await repos.characters.create(
      { name: c.name, userId: spec.userId, controlledBy: c.controlledBy } as never,
      { id: c.id }
    );
  }

  // A chat carrying planted seats: ASK v4's real create for the planted shape
  // first (key absent / `""`), and fall back to a valid create + plant only if
  // it refuses. `seatPlantVia` records which path each such chat took.
  const seatPlantVia: string[] = [];
  const plantedChats = new Set<string>();
  const seat = (p: SeatSpec, planted: boolean): Record<string, unknown> => {
    const out: Record<string, unknown> = {
      id: p.id,
      type: 'CHARACTER',
      characterId: p.characterId,
      controlledBy: p.controlledBy,
      status: p.status,
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    };
    if (planted && p.plant === 'deleteCharacterId') delete out.characterId;
    if (planted && p.plant === 'emptyCharacterId') out.characterId = '';
    return out;
  };
  for (const chat of spec.chats) {
    const opts = { id: chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp };
    if (chat.participants.some((p) => p.plant)) {
      try {
        await repos.chats.create(
          {
            userId: spec.userId,
            title: chat.name,
            participants: chat.participants.map((p) => seat(p, true)),
          } as never,
          opts
        );
        seatPlantVia.push(`${chat.name}: create accepted the planted seats`);
        continue;
      } catch (err) {
        seatPlantVia.push(
          `${chat.name}: create refused the planted seats (${String((err as Error)?.message ?? err)
            .replace(/\s+/g, ' ')
            .slice(0, 80)}…); planted`
        );
        plantedChats.add(chat.id);
      }
    }
    await repos.chats.create(
      {
        userId: spec.userId,
        title: chat.name,
        participants: chat.participants.map((p) => seat(p, false)),
      } as never,
      opts
    );
  }

  closeMountIndexSQLiteClient();

  // ---- Plants: the shapes v4's own schemas refuse to write. ----
  const db = getRawDatabase?.();
  if (!db) throw new Error('main DB handle unavailable');

  for (const c of spec.characters) {
    if (!c.plantEmptyName || emptyNameVia.startsWith('create accepted')) continue;
    const r = db.prepare('UPDATE characters SET name = ? WHERE id = ?').run('', c.id);
    if (r.changes !== 1) throw new Error(`empty-name plant touched ${r.changes} rows`);
    planted.push(`characters[${c.id}].name=''`);
  }

  for (const chat of spec.chats) {
    if (!plantedChats.has(chat.id)) continue;
    const seatPlants = chat.participants
      .map((p, i) => [i, p.plant] as const)
      .filter(([, plant]) => plant);
    if (seatPlants.length === 0) continue;
    const row = db.prepare('SELECT participants FROM chats WHERE id = ?').get(chat.id) as
      | { participants: string }
      | undefined;
    if (!row || typeof row.participants !== 'string') {
      throw new Error(`chat ${chat.id}: participants column is not plain JSON text`);
    }
    const stored = JSON.parse(row.participants) as Array<Record<string, unknown>>;
    if (stored.length !== chat.participants.length) {
      throw new Error(`chat ${chat.id}: stored ${stored.length} seats, spec has ${chat.participants.length}`);
    }
    for (const [i, plant] of seatPlants) {
      if (plant === 'deleteCharacterId') delete stored[i].characterId;
      else stored[i].characterId = '';
      planted.push(`chats[${chat.name}].participants[${i}]:${plant}`);
    }
    db.prepare('UPDATE chats SET participants = ? WHERE id = ?').run(
      JSON.stringify(stored),
      chat.id
    );
  }

  await closeDatabase();
  process.stderr.write(
    `built speaker-names fixture: ${outMain} + ${outMount} (${spec.characters.length} characters, ` +
      `${spec.chats.length} chats; empty name: ${emptyNameVia}; seats: ${seatPlantVia.join(' | ')}; planted: ${planted.join(', ') || 'nothing'})\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`speaker-names fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
