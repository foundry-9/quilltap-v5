/**
 * The storyteller-rounds fixture builder (P4.D343) — the test-pepper substrate
 * for `storyteller_rounds_tier2` (tier 2).
 *
 * Built entirely through v4's REAL code at the pin, with every id and
 * timestamp pinned, so both differential sides open byte-identical rows:
 *
 *   1. ONE user.
 *   2. Chat A — two CHARACTER seats — put in storyteller mode through v4's
 *      REAL `repos.chats.setTurnMode(A, 'storyteller')`, plus one operator
 *      post (the rounds' `triggerMessageId`).
 *   3. Chat B — one seat, left in rotation (`turnMode` NULL).
 *   (`chat_informs` is made empty by v4's `ensureCollection` — a live
 *   instance always has it, and v5's chat delete reads it.)
 *   4. The `storyteller_rounds` table made by v4's REAL migration
 *      `add-storyteller-mode-v1` (`run()`, its own keyed connection) — the
 *      MIGRATED-instance shape: the `chatId` FOREIGN KEY with `ON DELETE
 *      CASCADE` and `idx_storyteller_rounds_chat`. That is the shape v4's
 *      chat delete leans on (its delete names no rounds; the FK sweeps them).
 *      The Rust side ALSO measures v5's explicit sweep (contract C-2) on a
 *      copy rebuilt FK-less (the generateDDL shape of a v5-provisioned
 *      instance) — see the test's header.
 *
 * NO `storyteller_rounds` rows: the oracle's ops create them.
 *
 * Built at regen time into /tmp (the .db files are never committed). Node 24,
 * from the PINNED v4 worktree:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server     # (the pinned worktree)
 *   TZ=UTC \
 *   QT_FIXTURE_SR_MAIN=/tmp/qt-sr-main.db \
 *   QT_FIXTURE_SR_MOUNT=/tmp/qt-sr-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-storyteller-rounds-fixture.ts
 */

import { join } from 'node:path';
import { mkdtempSync, mkdirSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

// ── Pinned values (the oracle case repeats them verbatim; the Rust test reads
//    them back from the oracle rows; nothing here is minted) ──
const TEST_PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const USER = 'f0000000-0000-4000-8000-00000000000f';
const TS = '2026-05-01T00:00:00.000Z';
const CHAT_A = 'c5000000-0000-4000-8000-00000000000a';
const CHAT_B = 'c5000000-0000-4000-8000-00000000000b';
const CHAR_1 = 'a5000000-0000-4000-8000-000000000001';
const CHAR_2 = 'a5000000-0000-4000-8000-000000000002';
const CHAR_3 = 'a5000000-0000-4000-8000-000000000003';
const P_A1 = 'e5000000-0000-4000-8000-0000000000a1';
const P_A2 = 'e5000000-0000-4000-8000-0000000000a2';
const P_B1 = 'e5000000-0000-4000-8000-0000000000b1';
const M_A1 = 'd5000000-0000-4000-8000-0000000000a1';

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(`storyteller-rounds fixture must be built under TZ=UTC (offset=${offset})`);
  }
  const mainOut = process.env.QT_FIXTURE_SR_MAIN;
  const mountOut = process.env.QT_FIXTURE_SR_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_SR_MAIN and QT_FIXTURE_SR_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-sr-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'llm-logs.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { ChatMetadataSchema } = await import('@/lib/schemas/types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  await initializeDatabase();
  await ensureCollection('users', UserSchema as never);
  await ensureCollection('chats', ChatMetadataSchema as never);
  // A live instance always carries `chat_informs`; v5's chat delete sweeps it
  // by read (v4 leans on its FK), so a fixture without it would log a
  // `no such table` the real delete never sees.
  const { ChatInformSchema } = await import('@/lib/schemas/chat-inform.types');
  await ensureCollection('chat_informs', ChatInformSchema as never);
  const repos = getRepositories();

  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: USER, createdAt: TS, updatedAt: TS } as never,
  );

  const seat = (id: string, characterId: string, displayOrder: number) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy: 'llm',
    status: 'active',
    isActive: true,
    displayOrder,
    connectionProfileId: null,
    hasHistoryAccess: false,
    createdAt: TS,
    updatedAt: TS,
  });
  const mkChat = async (id: string, title: string, participants: object[]) =>
    repos.chats.create(
      { userId: USER, title, chatType: 'salon', projectId: null, participants, tags: [] } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  await mkChat(CHAT_A, 'The Storyteller’s Table', [seat(P_A1, CHAR_1, 0), seat(P_A2, CHAR_2, 1)]);
  await mkChat(CHAT_B, 'The Rotation Room', [seat(P_B1, CHAR_3, 0)]);

  // The operator post that opens the rounds (and makes `chat_messages`, which
  // the migration's `run()` widens).
  await repos.chats.addMessage(CHAT_A, {
    type: 'message',
    id: M_A1,
    role: 'USER',
    content: 'The lamps gutter; who will speak first?',
    createdAt: TS,
    attachments: [],
  } as never);

  // v4's REAL patch-only writer; it stamps `updatedAt` with the live clock, so
  // re-pin it afterwards (byte-reproducible rebuilds).
  if (!(await repos.chats.setTurnMode(CHAT_A, 'storyteller' as never))) {
    throw new Error('setTurnMode(A) wrote nothing');
  }
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const maindb = getRawDatabase();
  if (!maindb) throw new Error('main DB handle unavailable');
  maindb.prepare('UPDATE "chats" SET "updatedAt" = ?').run(TS);

  closeMountIndexSQLiteClient();
  await closeDatabase();

  // The table, by v4's REAL migration (the migrated-instance shape, FK and all).
  const { addStorytellerModeMigration } = await import(
    '@/migrations/scripts/add-storyteller-mode-v1'
  );
  const result = await addStorytellerModeMigration.run();
  if (!result.success) throw new Error(`migration failed: ${result.message}`);
  const { closeSQLite } = await import('@/migrations/lib/database-utils');
  closeSQLite();

  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
  }
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`built storyteller-rounds fixtures: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`storyteller-rounds fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
