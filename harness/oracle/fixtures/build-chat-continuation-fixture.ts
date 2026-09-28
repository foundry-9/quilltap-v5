/**
 * Tier-2 SEED fixture builder — `applyChatContinuation` (Continue Elsewhere).
 *
 * Bakes, through v4's REAL repositories with every id and timestamp pinned:
 *   - four SOURCE chats (the previous chapters) and four DESTINATION chats (the
 *     ones a continuation creates), each with CHARACTER participants carrying a
 *     `characterId` — which is the only thing `buildParticipantIdMap` keys on,
 *     so no `characters` row and no vault are needed;
 *   - each source chat's messages, seeded with DISTINCT `createdAt` stamps
 *     because `getMessages` sorts by them and a tie leaves the replay order to
 *     whatever SQLite's scan happens to hand back — which is not a thing two
 *     implementations must agree on;
 *   - each chat's turn-state columns, written AFTER the messages: `addMessages`
 *     runs `computeSpokenThisCycleAfterMessage` / `computeCycleOrderAfterMessage`
 *     per row, so a rotation set at create time would be overwritten by the seed
 *     itself. It is also what makes the ORDERING of the port measurable — the
 *     replay mutates the destination's rotation and `replicateTurnState`
 *     overwrites it afterwards, so a swap of those two steps must redden.
 *
 * The whole point of the fixture is that both sides start from identical bytes
 * and mint their own ids and clocks from there; the diff remaps them.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-continuation-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-continuation-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-chat-continuation-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharacterSpec {
  id: string;
  name: string;
  controlledBy: 'llm' | 'user';
  /** Absent → `spec.userId`. */
  userId?: string;
  /** A DANGLING vault pointer makes v4's overlaid `findById` throw. */
  characterDocumentMountPointId?: string;
}
interface ChatSpec {
  id: string;
  title: string;
  participants: Array<{
    id: string;
    character: string;
    controlledBy?: 'llm' | 'user';
    status?: string;
  }>;
  messages: Array<Record<string, unknown>>;
  columns: Record<string, unknown>;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  /** P4.D233: slim character rows (absent before — every seat dangled). */
  characters?: CharacterSpec[];
  chats: ChatSpec[];
  /** P4.D233: raw SQL run LAST (the poisoned-post trigger). */
  triggers?: string[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'chat-continuation-tier2.json'), 'utf8'),
  ) as Spec;

  const mainOut = process.env.QT_FIXTURE_OUT;
  const mountOut = process.env.QT_FIXTURE_MOUNT_OUT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_OUT and QT_FIXTURE_MOUNT_OUT must point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-continuation-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery, ensureCollection } = await import(
    '@/lib/database/manager'
  );
  const { CharactersRepository } = await import(
    '@/lib/database/repositories/characters.repository'
  );
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  await initializeDatabase();
  const repos = getRepositories();

  // The mount index is opened by the repository factory and by the Rust engine's
  // `DbPaths`; nothing here writes to it, but an empty encrypted file has to
  // exist or either side's open fails.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  midb.exec('CREATE TABLE IF NOT EXISTS "doc_mount_points" ("id" TEXT PRIMARY KEY)');

  // P4.D233 (v4 `acadcc7cd`, bug 171): the left-behind notice reads each
  // absentee through `characters.findById` and resolves the persona through
  // `findUserControlled`, so the characters table and a few SLIM rows (null
  // vault mount → the overlay passes them through) join the seed. Seeded via
  // the real protected `_create`, as the user-identity builder does. The seats
  // of the original four chats (`characterA`/`characterB`) stay row-less on
  // purpose: a missing character is skipped silently.
  await ensureCollection('characters', CharacterSchema);
  class CharactersSqlRepo extends CharactersRepository {
    async createSlim(data: unknown, options: unknown) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      return (this as any)._create(data, options);
    }
  }
  const charRepo = new CharactersSqlRepo();
  for (const c of spec.characters ?? []) {
    await charRepo.createSlim(
      {
        userId: c.userId ?? spec.userId,
        name: c.name,
        controlledBy: c.controlledBy,
        ...(c.characterDocumentMountPointId
          ? { characterDocumentMountPointId: c.characterDocumentMountPointId }
          : {}),
      },
      { id: c.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp },
    );
  }

  for (const chat of spec.chats) {
    await repos.chats.create(
      {
        userId: spec.userId,
        title: chat.title,
        participants: chat.participants.map((p) => ({
          id: p.id,
          type: 'CHARACTER',
          characterId: p.character,
          controlledBy: p.controlledBy ?? 'llm',
          displayOrder: 0,
          isActive: (p.status ?? 'active') === 'active',
          status: p.status ?? 'active',
          createdAt: spec.seedTimestamp,
          updatedAt: spec.seedTimestamp,
        })),
      } as never,
      { id: chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp } as never,
    );

    if (chat.messages.length > 0) {
      await repos.chats.addMessages(
        chat.id,
        chat.messages.map((m, i) => ({
          type: 'message',
          attachments: [],
          ...m,
          // DISTINCT per message: `getMessages` orders by `createdAt`, and a tie
          // is unordered.
          createdAt: `2026-01-01T00:00:${String(i * 3).padStart(2, '0')}.000Z`,
        })) as never,
      );
    } else {
      await repos.chats.getMessageCount(chat.id);
    }

    // The turn-state columns go on LAST, over whatever `addMessages` computed.
    const cols = Object.entries(chat.columns);
    const sets = ['updatedAt = ?', 'lastMessageAt = ?', ...cols.map(([k]) => `"${k}" = ?`)];
    const vals: unknown[] = [
      spec.seedTimestamp,
      chat.messages.length > 0 ? spec.seedTimestamp : null,
      ...cols.map(([, v]) => (typeof v === 'boolean' ? (v ? 1 : 0) : (v as unknown))),
      chat.id,
    ];
    await rawQuery(`UPDATE chats SET ${sets.join(', ')} WHERE id = ?`, vals);
  }

  for (const sql of spec.triggers ?? []) {
    await rawQuery(sql);
  }

  await closeDatabase();
  closeMountIndexSQLiteClient();
  rmSync(scratch, { recursive: true, force: true });
  // eslint-disable-next-line no-console
  console.log(`wrote ${mainOut} + ${mountOut} (${spec.chats.length} chats)`);
}

void main();
