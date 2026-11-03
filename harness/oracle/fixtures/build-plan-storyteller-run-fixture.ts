/**
 * The plan-storyteller-run fixture builder (P4.D343, item 24) — the
 * test-pepper substrate for `plan_storyteller_run_equivalence` (tier 2).
 *
 * Built entirely through v4's REAL code at the pin, with every id and
 * timestamp pinned, so both differential sides open byte-identical rows:
 *
 *   1. ONE user.
 *   2. Chat S — four LLM CHARACTER seats a / b / c / d, a and b SHARING a
 *      connection profile (the `profileKey` grouping), c on its own, d on
 *      none (`'default'`) — put in storyteller mode through v4's REAL
 *      `repos.chats.setTurnMode(S, 'storyteller')`, with a SEEDED rotation
 *      (`cycleOrderParticipantIds = [c, a, b, d]`, written through v4's REAL
 *      `repos.chats.update` — the one writer `resolveCycleOrder` itself uses),
 *      and two messages: c spoke, then the operator posted. `resolveCycleOrder`
 *      therefore READS the stored cycle (c is the last speaker, a is usable)
 *      and draws nothing.
 *   3. Chat D — three seats with participant talkativeness and NO stored
 *      cycle: `resolveCycleOrder` DRAWS and PERSISTS one (the oracle stubs
 *      `Math.random` with the sequence the Rust side passes as
 *      `DrawSource::sequence`).
 *   4. The `storyteller_rounds` table through v4's REAL `ensureCollection`
 *      (generateDDL's FK-LESS shape — a v5-provisioned instance's), so the
 *      oracle can plant a round naming a chat that does not exist (the 404
 *      gate).
 *
 * NO `storyteller_rounds` rows: the oracle plants one per case.
 *
 * Built at regen time into /tmp (the .db files are never committed). Node 24,
 * from the PINNED v4 worktree:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd /tmp/qt-v4-pin-<pin>     # (the pinned worktree)
 *   TZ=UTC \
 *   QT_FIXTURE_PSR_MAIN=/tmp/qt-psr-main.db \
 *   QT_FIXTURE_PSR_MOUNT=/tmp/qt-psr-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-plan-storyteller-run-fixture.ts
 */

import { join } from 'node:path';
import { mkdtempSync, mkdirSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

// ── Pinned values (the oracle case repeats them verbatim) ──
const TEST_PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const USER = 'f0000000-0000-4000-8000-00000000000f';
const TS = '2026-05-01T00:00:00.000Z';
const TS_C = '2026-05-01T00:01:00.000Z';
const TS_OP = '2026-05-01T00:02:00.000Z';
const CHAT_S = 'c6000000-0000-4000-8000-00000000000a';
const CHAT_D = 'c6000000-0000-4000-8000-00000000000d';
const PROF_1 = '70000000-0000-4000-8000-000000000001';
const PROF_2 = '70000000-0000-4000-8000-000000000002';
const CH = (n: number) => `a6000000-0000-4000-8000-00000000000${n}`;
const P_A = 'e6000000-0000-4000-8000-0000000000aa';
const P_B = 'e6000000-0000-4000-8000-0000000000bb';
const P_C = 'e6000000-0000-4000-8000-0000000000cc';
const P_D = 'e6000000-0000-4000-8000-0000000000dd';
const P_X = 'e6000000-0000-4000-8000-0000000000d1';
const P_Y = 'e6000000-0000-4000-8000-0000000000d2';
const P_Z = 'e6000000-0000-4000-8000-0000000000d3';
const M_C = 'd6000000-0000-4000-8000-0000000000c1';
const M_OP = 'd6000000-0000-4000-8000-0000000000c2';

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(`plan-storyteller-run fixture must be built under TZ=UTC (offset=${offset})`);
  }
  const mainOut = process.env.QT_FIXTURE_PSR_MAIN;
  const mountOut = process.env.QT_FIXTURE_PSR_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_PSR_MAIN and QT_FIXTURE_PSR_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-psr-fixture-'));
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
  const { ChatMetadataSchema, CharacterSchema } = await import('@/lib/schemas/types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { StorytellerRoundSchema } = await import('@/lib/schemas/storyteller.types');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  await initializeDatabase();
  await ensureCollection('users', UserSchema as never);
  await ensureCollection('chats', ChatMetadataSchema as never);
  // The room's characters are read on both sides (v4 `loadRoomCharacters` →
  // `characters.findByIds`); an empty table, so every seat reads unloaded.
  await ensureCollection('characters', CharacterSchema as never);
  await ensureCollection('storyteller_rounds', StorytellerRoundSchema as never);
  const repos = getRepositories();

  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: USER, createdAt: TS, updatedAt: TS } as never,
  );

  const seat = (
    id: string,
    characterId: string,
    displayOrder: number,
    connectionProfileId: string | null,
    talkativeness?: number,
  ) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy: 'llm',
    status: 'active',
    isActive: true,
    displayOrder,
    connectionProfileId,
    hasHistoryAccess: false,
    ...(talkativeness === undefined ? {} : { talkativeness }),
    createdAt: TS,
    updatedAt: TS,
  });
  const mkChat = async (id: string, title: string, participants: object[]) =>
    repos.chats.create(
      { userId: USER, title, chatType: 'salon', projectId: null, participants, tags: [] } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  await mkChat(CHAT_S, 'The Storyteller’s Table', [
    seat(P_A, CH(1), 0, PROF_1),
    seat(P_B, CH(2), 1, PROF_1),
    seat(P_C, CH(3), 2, PROF_2),
    seat(P_D, CH(4), 3, null),
  ]);
  await mkChat(CHAT_D, 'The Drawing Room', [
    seat(P_X, CH(5), 0, PROF_1, 0.2),
    seat(P_Y, CH(6), 1, PROF_1, 0.5),
    seat(P_Z, CH(7), 2, PROF_2, 0.3),
  ]);

  // c spoke, then the operator posted (c is the last speaker).
  await repos.chats.addMessage(CHAT_S, {
    type: 'message',
    id: M_C,
    role: 'ASSISTANT',
    content: 'The candle flickers; Clio clears her throat.',
    participantId: P_C,
    createdAt: TS_C,
    attachments: [],
  } as never);
  await repos.chats.addMessage(CHAT_S, {
    type: 'message',
    id: M_OP,
    role: 'USER',
    content: 'The lamps gutter; who will speak first?',
    createdAt: TS_OP,
    attachments: [],
  } as never);

  // The seeded rotation, through v4's REAL whole-row writer (the one
  // `resolveCycleOrder` persists through).
  await repos.chats.update(CHAT_S, {
    cycleOrderParticipantIds: JSON.stringify([P_C, P_A, P_B, P_D]),
  } as never);

  // v4's REAL patch-only writer; it stamps `updatedAt` with the live clock, so
  // re-pin it afterwards (byte-reproducible rebuilds).
  for (const id of [CHAT_S, CHAT_D]) {
    if (!(await repos.chats.setTurnMode(id, 'storyteller' as never))) {
      throw new Error(`setTurnMode(${id}) wrote nothing`);
    }
  }
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const maindb = getRawDatabase();
  if (!maindb) throw new Error('main DB handle unavailable');
  maindb.prepare('UPDATE "chats" SET "updatedAt" = ?').run(TS);

  closeMountIndexSQLiteClient();
  await closeDatabase();

  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
  }
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`built plan-storyteller-run fixtures: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`plan-storyteller-run fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
