/**
 * Oracle: `storyteller_rounds_tier2` (P4.D343, items 16 + 11 + 28's C-2).
 *
 * Drives v4's REAL `StorytellerRoundsRepository` (`getRepositories()
 * .storytellerRounds`) and `ChatsRepository` (`setTurnMode`, `delete`) over a
 * COPY of the fixture pair built by `build-storyteller-rounds-fixture.ts`, and
 * emits one NDJSON row per op: `{ i, op, input, result, logs }`. The ops and
 * their inputs live HERE only — the Rust side replays each row's `input`, so
 * the corpus has one source of truth. The first row is a header carrying the
 * pinned timestamps (the Rust normaliser keeps those exact and masks every
 * other `updatedAt` / `closedAt` — v4 stamps them with the live clock inside
 * `_update` / `markClosed` / `setTurnMode`).
 *
 * The last rows are `dump` (the whole `storyteller_rounds` table, raw cells,
 * ORDER BY id — JSON columns compared as the stored TEXT — and chat A / B's
 * `turnMode` + `updatedAt` cells), taken before the throwing plant, then the
 * plant's read rows.
 *
 * Log lines are captured by splicing `logger.transports`, keeping only the
 * messages this family pins (`PINNED`), plus v4's backend `Corrupted JSON in
 * column, using default` WARN, which the Rust side drops by name (a backend
 * line, unported by standing convention).
 *
 * Run (Node 24, from the PINNED v4 worktree — see the Rust test's header):
 *   QT_FIXTURE_SR_MAIN=/tmp/qt-sr-main.db QT_FIXTURE_SR_MOUNT=/tmp/qt-sr-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/cases/storyteller-rounds.ts > /tmp/oracle-storyteller-rounds.ndjson
 */

import { join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

const TEST_PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const TS = '2026-05-01T00:00:00.000Z';
const CHAT_A = 'c5000000-0000-4000-8000-00000000000a';
const CHAT_B = 'c5000000-0000-4000-8000-00000000000b';
const CHAT_MISSING = 'c5000000-0000-4000-8000-0000000000ff';
const CHAR_1 = 'a5000000-0000-4000-8000-000000000001';
const CHAR_2 = 'a5000000-0000-4000-8000-000000000002';
const CHAR_3 = 'a5000000-0000-4000-8000-000000000003';
const CHAR_4 = 'a5000000-0000-4000-8000-000000000004';
const P_A1 = 'e5000000-0000-4000-8000-0000000000a1';
const P_A2 = 'e5000000-0000-4000-8000-0000000000a2';
const P_A3 = 'e5000000-0000-4000-8000-0000000000a3';
const P_B1 = 'e5000000-0000-4000-8000-0000000000b1';
const M_A1 = 'd5000000-0000-4000-8000-0000000000a1';

const R1 = '55000000-0000-4000-8000-000000000001';
const R2 = '55000000-0000-4000-8000-000000000002';
const R3 = '55000000-0000-4000-8000-000000000003';
const R4 = '55000000-0000-4000-8000-000000000004';
const R5 = '55000000-0000-4000-8000-000000000005';
const R_MISSING = '55000000-0000-4000-8000-0000000000ff';

const T1 = '2026-05-02T10:00:00.000Z';
const T2 = '2026-05-02T11:00:00.000Z';
const T3 = '2026-05-02T09:00:00.000Z';
const T4 = '2026-05-02T12:00:00.000Z';
const T5 = '2026-05-02T13:00:00.000Z';
const ASKED = '2026-05-02T10:00:01.000Z';
const ANSWERED = '2026-05-02T10:00:09.000Z';
const CHOSEN_1 = '2026-05-02T11:05:00.000Z';
const CHOSEN_2 = '2026-05-02T11:09:00.000Z';
const PINNED_TS = [TS, T1, T2, T3, T4, T5, ASKED, ANSWERED, CHOSEN_1, CHOSEN_2];

/** v4 `round.service.ts` `pendingCard` — every key set, explicit nulls. */
const pending = (participantId: string, characterId: string, characterName: string, talkativeness: number, silent: boolean) => ({
  participantId,
  characterId,
  characterName,
  status: 'pending',
  statedTier: null,
  tier: null,
  tierRaised: false,
  tools: [],
  plan: null,
  reason: null,
  talkativeness,
  silent,
  askedAt: ASKED,
  answeredAt: null,
});
const keptQuiet = (participantId: string, characterId: string, characterName: string, talkativeness: number) => ({
  participantId,
  characterId,
  characterName,
  status: 'kept-quiet',
  statedTier: null,
  tier: null,
  tierRaised: false,
  tools: [],
  plan: null,
  reason: 'Kept quiet this round (talkativeness)',
  talkativeness,
  silent: false,
  askedAt: null,
  answeredAt: null,
});

type Op = { op: string; input: Record<string, unknown> };

const OPS: Op[] = [
  { op: 'create', input: { id: R1, chatId: CHAT_A, triggerMessageId: M_A1, status: 'open', createdAt: T1, bids: [pending(P_A1, CHAR_1, 'Aria', 0.5, false), pending(P_A2, CHAR_2, 'Bea', 1, true)] } },
  { op: 'create', input: { id: R2, chatId: CHAT_A, triggerMessageId: M_A1, status: 'open', createdAt: T2, bids: [keptQuiet(P_A2, CHAR_2, 'Bea', 0.25)] } },
  { op: 'create', input: { id: R3, chatId: CHAT_B, triggerMessageId: M_A1, status: 'open', createdAt: T1, bids: [pending(P_B1, CHAR_3, 'Clio', 0.75, false)] } },
  { op: 'create', input: { id: R4, chatId: CHAT_B, triggerMessageId: null, status: 'open', createdAt: T3, bids: [] } },
  { op: 'findById', input: { id: R1 } },
  { op: 'findById', input: { id: R4 } },
  { op: 'findActiveForChat', input: { chatId: CHAT_A } },
  { op: 'findLatestForChat', input: { chatId: CHAT_A } },
  { op: 'findActiveForChat', input: { chatId: CHAT_B } },
  { op: 'findLatestForChat', input: { chatId: CHAT_MISSING } },
  // Replace an existing seat's card (the answered bid) …
  { op: 'recordBid', input: { roundId: R1, bid: {
    participantId: P_A1, characterId: CHAR_1, characterName: 'Aria', status: 'bid',
    statedTier: 'lookup', tier: 'act', tierRaised: true, tools: ['doc_read_file', 'run_custom:coin'],
    plan: 'Aria reads the docket aloud, then flips the coin.', reason: null, talkativeness: 0.5,
    silent: false, askedAt: ASKED, answeredAt: ANSWERED,
  } } },
  // … and append a new seat (a re-ask of a kept-quiet seat, minimal keys:
  // the defaults fill `tierRaised` / `tools` / `silent`, the optionals stay absent).
  { op: 'recordBid', input: { roundId: R1, bid: {
    participantId: P_A3, characterId: CHAR_4, characterName: 'Dot', status: 'pass',
    reason: 'Nothing to add', talkativeness: 0.1,
  } } },
  // The stored bytes of both cards (the appended one with Zod's defaults
  // filled and its optionals absent).
  { op: 'rawRow', input: { id: R1 } },
  { op: 'markRunning', input: { roundId: R2, selection: { participantIds: [P_A2], order: 'sequential', chosenAt: CHOSEN_1, names: ['Bea'] } } },
  { op: 'findActiveForChat', input: { chatId: CHAT_A } },
  { op: 'markClosed', input: { roundId: R2, reason: 'ran', selection: { participantIds: [P_A2, P_A1], order: null, chosenAt: CHOSEN_2 } } },
  { op: 'findActiveForChat', input: { chatId: CHAT_A } },
  { op: 'findLatestForChat', input: { chatId: CHAT_A } },
  { op: 'recordBid', input: { roundId: R2, bid: pending(P_A1, CHAR_1, 'Aria', 0.5, false) } },
  { op: 'recordBid', input: { roundId: R_MISSING, bid: pending(P_A2, CHAR_2, 'Bea', 1, true) } },
  { op: 'markRunning', input: { roundId: R_MISSING, selection: { participantIds: [], order: null, chosenAt: CHOSEN_1, names: [] } } },
  // Without a selection (undefined → the stored one stays).
  { op: 'markClosed', input: { roundId: R1, reason: 'let-lie' } },
  // With an explicit null selection.
  { op: 'markClosed', input: { roundId: R3, reason: 'superseded', selection: null } },
  { op: 'findActiveForChat', input: { chatId: CHAT_A } },
  { op: 'findById', input: { id: R1 } },
  { op: 'findById', input: { id: R2 } },
  { op: 'create', input: { id: R5, chatId: CHAT_B, triggerMessageId: M_A1, status: 'open', createdAt: T5, bids: [pending(P_B1, CHAR_3, 'Clio', 0.75, false)] } },
  // Corrupt rows, planted raw.
  { op: 'execSql', input: { sql: `UPDATE "storyteller_rounds" SET "bids" = 'not json' WHERE "id" = '${R5}'` } },
  { op: 'findById', input: { id: R5 } },
  { op: 'findActiveForChat', input: { chatId: CHAT_B } },
  { op: 'execSql', input: { sql: `UPDATE "storyteller_rounds" SET "bids" = '[{"participantId":1}]' WHERE "id" = '${R5}'` } },
  { op: 'findById', input: { id: R5 } },
  { op: 'findActiveForChat', input: { chatId: CHAT_B } },
  { op: 'findLatestForChat', input: { chatId: CHAT_B } },
  { op: 'recordBid', input: { roundId: R5, bid: pending(P_B1, CHAR_3, 'Clio', 0.75, false) } },
  { op: 'execSql', input: { sql: `UPDATE "storyteller_rounds" SET "bids" = '[]', "status" = 'bogus' WHERE "id" = '${R5}'` } },
  { op: 'findById', input: { id: R5 } },
  { op: 'findLatestForChat', input: { chatId: CHAT_B } },
  { op: 'markClosed', input: { roundId: R5, reason: 'abandoned' } },
  { op: 'execSql', input: { sql: `UPDATE "storyteller_rounds" SET "bids" = 'null', "status" = 'open' WHERE "id" = '${R5}'` } },
  { op: 'findById', input: { id: R5 } },
  // An unparseable `selection` reads as absent, so `_update` leaves the cell
  // alone (v4 `$set`s only the keys the validated object carries).
  { op: 'execSql', input: { sql: `UPDATE "storyteller_rounds" SET "selection" = 'garbage' WHERE "id" = '${R5}'` } },
  { op: 'recordBid', input: { roundId: R5, bid: keptQuiet(P_A3, CHAR_4, 'Dot', 0.1) } },
  { op: 'rawRow', input: { id: R5 } },
  // The turn mode.
  { op: 'setTurnMode', input: { chatId: CHAT_A, mode: 'rotation' } },
  { op: 'chatCells', input: {} },
  { op: 'setTurnMode', input: { chatId: CHAT_A, mode: 'storyteller' } },
  { op: 'chatCells', input: {} },
  { op: 'setTurnMode', input: { chatId: CHAT_MISSING, mode: 'storyteller' } },
  // The chat delete (v4: the FK cascade sweeps A's rounds).
  { op: 'deleteChat', input: { chatId: CHAT_A } },
  { op: 'findLatestForChat', input: { chatId: CHAT_A } },
  { op: 'dump', input: {} },
  // A read that THROWS (both sides): `bids` rebuilt as a VIRTUAL column whose
  // expression fails, so every SELECT that reaches a row errors.
  { op: 'execSql', input: { sql: `ALTER TABLE "storyteller_rounds" DROP COLUMN "bids"; ALTER TABLE "storyteller_rounds" ADD COLUMN "bids" TEXT AS (json('not json')) VIRTUAL` } },
  { op: 'findActiveForChat', input: { chatId: CHAT_B } },
  { op: 'findLatestForChat', input: { chatId: CHAT_B } },
  { op: 'findById', input: { id: R3 } },
  { op: 'recordBid', input: { roundId: R3, bid: pending(P_B1, CHAR_3, 'Clio', 0.75, false) } },
];

const PINNED = [
  'Storyteller bid dropped — round not open',
  'Error finding active storyteller round',
  'Error finding latest storyteller round',
  'Error finding entity by ID',
  'Error finding entities by filter',
  'Entity not found for update',
  'Error updating entity',
  'Failed to write turn mode',
  'Turn mode write',
  'Corrupted JSON in column, using default',
];

async function main(): Promise<void> {
  const mainSrc = process.env.QT_FIXTURE_SR_MAIN;
  const mountSrc = process.env.QT_FIXTURE_SR_MOUNT;
  if (!mainSrc || !mountSrc) throw new Error('QT_FIXTURE_SR_MAIN / QT_FIXTURE_SR_MOUNT unset');
  const scratch = mkdtempSync(join(tmpdir(), 'qt-sr-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'main.db');
  const mountWork = join(scratch, 'mount.db');
  copyFileSync(mainSrc, mainWork);
  copyFileSync(mountSrc, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'llm-logs.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  const { logger } = await import('@/lib/logger');
  const lines: unknown[] = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      if (!PINNED.includes(data.message)) return;
      const { service: _s, environment: _e, ...fields } = (data.context ?? {}) as Record<string, unknown>;
      lines.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
    },
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  await initializeDatabase();
  const repos = getRepositories();
  const rounds = repos.storytellerRounds;
  const raw = () => {
    const db = getRawDatabase();
    if (!db) throw new Error('main DB handle unavailable');
    return db;
  };

  const out: string[] = [JSON.stringify({ op: 'header', pinnedTimestamps: PINNED_TS })];
  for (const [i, { op, input }] of OPS.entries()) {
    lines.length = 0;
    const a = input as Record<string, any>;
    let result: unknown;
    try {
      switch (op) {
        case 'create':
          result = await rounds.create(
            {
              chatId: a.chatId,
              triggerMessageId: a.triggerMessageId,
              status: a.status,
              bids: a.bids,
              selection: null,
              closedReason: null,
              closedAt: null,
            } as never,
            { id: a.id, createdAt: a.createdAt, updatedAt: a.createdAt },
          );
          break;
        case 'findById':
          result = await rounds.findById(a.id);
          break;
        case 'findActiveForChat':
          result = await rounds.findActiveForChat(a.chatId);
          break;
        case 'findLatestForChat':
          result = await rounds.findLatestForChat(a.chatId);
          break;
        case 'recordBid':
          result = await rounds.recordBid(a.roundId, a.bid);
          break;
        case 'markRunning':
          result = await rounds.markRunning(a.roundId, a.selection);
          break;
        case 'markClosed':
          result = 'selection' in a
            ? await rounds.markClosed(a.roundId, a.reason, a.selection)
            : await rounds.markClosed(a.roundId, a.reason);
          break;
        case 'setTurnMode':
          result = await repos.chats.setTurnMode(a.chatId, a.mode);
          break;
        case 'deleteChat':
          result = await repos.chats.delete(a.chatId);
          break;
        case 'execSql':
          raw().exec(a.sql);
          result = null;
          break;
        case 'rawRow':
          result = raw().prepare('SELECT * FROM "storyteller_rounds" WHERE "id" = ?').get(a.id) ?? null;
          break;
        case 'chatCells':
          result = raw().prepare('SELECT "id", "turnMode", "updatedAt" FROM "chats" ORDER BY "id"').all();
          break;
        case 'dump':
          result = {
            rounds: raw().prepare('SELECT * FROM "storyteller_rounds" ORDER BY "id"').all(),
            chats: raw().prepare('SELECT "id", "turnMode", "updatedAt" FROM "chats" ORDER BY "id"').all(),
          };
          break;
        default:
          throw new Error(`unknown op ${op}`);
      }
    } catch (e) {
      result = { threw: (e as Error).message };
    }
    out.push(JSON.stringify({ i, op, input, result: result === undefined ? null : result, logs: [...lines] }));
  }

  await closeDatabase();
  rmSync(scratch, { recursive: true, force: true });
  process.stdout.write(out.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`storyteller-rounds oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
