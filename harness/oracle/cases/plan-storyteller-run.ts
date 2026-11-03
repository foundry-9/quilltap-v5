/**
 * Oracle: `plan_storyteller_run_equivalence` (P4.D343, item 24 — tier 2).
 *
 * Drives v4's REAL `planStorytellerRun` + `runStorytellerRound`
 * (`lib/storyteller/round.service.ts`) over a COPY of the fixture pair built
 * by `build-plan-storyteller-run-fixture.ts`, with `runTurn` CANNED (the case
 * spec's outcome per participant). Every v4 module underneath is real: the
 * rounds repository, the chats repository, `loadRoomCharacters`,
 * `calculateTurnStateFromHistory`, `resolveCycleOrder` (which may DRAW and
 * PERSIST — `Math.random` is replaced by the case's draw sequence, answering
 * the last value once exhausted and `0` when empty: `DrawSource::sequence`'s
 * semantics), and the turn-control registry (the stop is a REAL
 * `stopActiveTurns` fired from inside the canned turn, under a REAL
 * `runWithTurnControl`).
 *
 * The cases run IN ORDER on ONE database copy (the Rust side replays them in
 * the same order on its own copy). Each case plants its round through v4's
 * REAL `StorytellerRoundsRepository.create` (pinned id / createdAt), plans,
 * optionally runs, then dumps the round row and both chats' rotation cells.
 *
 * Emits one NDJSON row per case: `{ i, name, input, plan, run, drawsUsed,
 * roundRow, chats, logs }`. The first row is a header carrying the pinned
 * timestamps (the Rust side masks every other `updatedAt` / `closedAt` /
 * `chosenAt` — v4 stamps them with the live clock).
 *
 * Run (Node 24, from the PINNED v4 worktree — see the Rust test's header):
 *   TZ=UTC QT_FIXTURE_PSR_MAIN=/tmp/qt-psr-main.db QT_FIXTURE_PSR_MOUNT=/tmp/qt-psr-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/cases/plan-storyteller-run.ts > /tmp/oracle-plan-storyteller-run.ndjson
 */

import { join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

const TEST_PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const TS = '2026-05-01T00:00:00.000Z';
const CHAT_S = 'c6000000-0000-4000-8000-00000000000a';
const CHAT_D = 'c6000000-0000-4000-8000-00000000000d';
const CHAT_GONE = 'c6000000-0000-4000-8000-0000000000ff';
const CH = (n: number) => `a6000000-0000-4000-8000-00000000000${n}`;
const P_A = 'e6000000-0000-4000-8000-0000000000aa';
const P_B = 'e6000000-0000-4000-8000-0000000000bb';
const P_C = 'e6000000-0000-4000-8000-0000000000cc';
const P_D = 'e6000000-0000-4000-8000-0000000000dd';
const P_X = 'e6000000-0000-4000-8000-0000000000d1';
const P_Y = 'e6000000-0000-4000-8000-0000000000d2';
const P_Z = 'e6000000-0000-4000-8000-0000000000d3';
/** Seats that left the room (a card on the round, no participant row). */
const P_GONE1 = 'e6000000-0000-4000-8000-0000000000f1';
const P_GONE2 = 'e6000000-0000-4000-8000-0000000000f2';
const M_OP = 'd6000000-0000-4000-8000-0000000000c2';
const R = (n: number) => `56000000-0000-4000-8000-0000000000${String(n).padStart(2, '0')}`;
const R_MISSING = '56000000-0000-4000-8000-0000000000ff';
const ASKED = '2026-05-02T10:00:01.000Z';
const ANSWERED = '2026-05-02T10:00:09.000Z';
const T = (n: number) => `2026-05-02T11:${String(n).padStart(2, '0')}:00.000Z`;
const PINNED_TS = [TS, ASKED, ANSWERED, ...Array.from({ length: 40 }, (_, n) => T(n))];

const NAMES: Record<string, [string, string]> = {
  [P_A]: [CH(1), 'Aria'],
  [P_B]: [CH(2), 'Bea'],
  [P_C]: [CH(3), 'Clio'],
  [P_D]: [CH(4), 'Dot'],
  [P_X]: [CH(5), 'Xan'],
  [P_Y]: [CH(6), 'Yew'],
  [P_Z]: [CH(7), 'Zed'],
  [P_GONE1]: [CH(8), 'Gone One'],
  [P_GONE2]: [CH(9), 'Gone Two'],
};

/** An answered card (v4 `cardFromOutcome`'s full shape). */
function bid(pid: string, tier: string | null, extra: Record<string, unknown> = {}) {
  const [characterId, characterName] = NAMES[pid];
  return {
    participantId: pid,
    characterId,
    characterName,
    status: 'bid',
    statedTier: tier,
    tier,
    tierRaised: false,
    tools: tier === 'lookup' ? ['doc_read_file'] : [],
    plan: `${characterName} means to ${tier ?? 'wait'}.`,
    reason: null,
    talkativeness: 0.5,
    silent: false,
    askedAt: ASKED,
    answeredAt: ANSWERED,
    ...extra,
  };
}

type Outcome = { spoke: boolean; yielded: boolean; messageId: string | null };
type Case = {
  name: string;
  round: { id: string; chatId: string; status: string; bids: unknown[] } | null;
  plan: {
    chatId: string;
    roundId: string;
    participantIds: string[];
    order: 'sequential' | 'simultaneous';
    keepOrder?: boolean;
  };
  draws?: number[];
  run?: { outcomes: Record<string, Outcome>; stopDuring?: string };
};

const spoke = (id: string): Outcome => ({ spoke: true, yielded: false, messageId: id });
const SILENT: Outcome = { spoke: false, yielded: false, messageId: null };
const M = (n: number) => `d6000000-0000-4000-8000-0000000001${String(n).padStart(2, '0')}`;
const simple3 = () => [bid(P_A, 'simple'), bid(P_B, 'simple'), bid(P_C, 'simple')];

const CASES: Case[] = [
  // 7d7e38d2f row 1: the rotation (stored [c, a, b, d]) orders [a, b, c] → [c, a, b];
  // run with a non-speaking middle turn (spokenSince accumulates only speakers).
  {
    name: 'rotation-order-and-spoken-since',
    round: { id: R(1), chatId: CHAT_S, status: 'open', bids: simple3() },
    plan: { chatId: CHAT_S, roundId: R(1), participantIds: [P_A, P_B, P_C], order: 'sequential' },
    run: { outcomes: { [P_C]: spoke(M(1)), [P_A]: SILENT, [P_B]: spoke(M(2)) } },
  },
  // 7d7e38d2f row 2: keepOrder keeps [b, a, c] and never consults the rotation —
  // on chat D (no stored cycle), so a consult would DRAW + PERSIST a cycle.
  {
    name: 'keep-order-never-consults-rotation',
    round: { id: R(2), chatId: CHAT_D, status: 'open', bids: [bid(P_X, 'simple'), bid(P_Y, 'act'), bid(P_Z, 'lookup')] },
    plan: { chatId: CHAT_D, roundId: R(2), participantIds: [P_Y, P_X, P_Z], order: 'sequential', keepOrder: true },
    draws: [0.9, 0.1, 0.5],
    run: { outcomes: { [P_Y]: spoke(M(3)), [P_X]: spoke(M(4)), [P_Z]: spoke(M(5)) } },
  },
  // 7d7e38d2f row 3: simultaneous + keepOrder over [c(act), d(lookup), a(act), b(simple)]
  // → [d, b] concurrent, then the Act tail [c, a] in the operator's order. PLAN only
  // (the concurrent group is the host's).
  {
    name: 'simultaneous-keep-order-concurrent-first',
    round: { id: R(3), chatId: CHAT_S, status: 'open', bids: [bid(P_A, 'act'), bid(P_B, 'simple'), bid(P_C, 'act'), bid(P_D, 'lookup')] },
    plan: { chatId: CHAT_S, roundId: R(3), participantIds: [P_C, P_D, P_A, P_B], order: 'simultaneous', keepOrder: true },
  },
  // The same selection through the rotation (no keepOrder): concurrent first,
  // the tail in rotation order.
  {
    name: 'simultaneous-rotation',
    round: { id: R(4), chatId: CHAT_S, status: 'open', bids: [bid(P_A, 'act'), bid(P_B, 'simple'), bid(P_C, 'act'), bid(P_D, 'lookup')] },
    plan: { chatId: CHAT_S, roundId: R(4), participantIds: [P_B, P_D, P_A, P_C], order: 'simultaneous' },
  },
  // Simultaneous with no Simple / Lookup seat: nothing concurrent, the whole
  // run is the sequential tail (Tier 1 runs it).
  {
    name: 'simultaneous-all-act-runs-sequential',
    round: { id: R(5), chatId: CHAT_S, status: 'open', bids: [bid(P_A, 'act'), bid(P_C, 'act')] },
    plan: { chatId: CHAT_S, roundId: R(5), participantIds: [P_A, P_C], order: 'simultaneous' },
    run: { outcomes: { [P_C]: spoke(M(6)), [P_A]: spoke(M(7)) } },
  },
  // The rotation DRAWN and PERSISTED (chat D has no cycle; the draw sequence
  // is pinned both sides).
  {
    name: 'rotation-drawn-and-persisted',
    round: { id: R(6), chatId: CHAT_D, status: 'open', bids: [bid(P_X, 'simple'), bid(P_Y, 'simple'), bid(P_Z, 'simple')] },
    plan: { chatId: CHAT_D, roundId: R(6), participantIds: [P_X, P_Y, P_Z], order: 'sequential' },
    draws: [0.9, 0.1, 0.5],
    run: { outcomes: { [P_X]: spoke(M(8)), [P_Y]: spoke(M(9)), [P_Z]: spoke(M(10)) } },
  },
  // Chosen ids NOT in the cycle (departed seats) rank after it, by chosen index.
  {
    name: 'unknown-to-cycle-ranked-by-chosen-index',
    round: { id: R(7), chatId: CHAT_S, status: 'open', bids: [bid(P_A, 'simple'), bid(P_C, 'simple'), bid(P_GONE1, 'simple'), bid(P_GONE2, 'simple')] },
    plan: { chatId: CHAT_S, roundId: R(7), participantIds: [P_GONE2, P_C, P_GONE1, P_A], order: 'sequential' },
  },
  // A duplicate participant id is de-duplicated (first-seen order).
  {
    name: 'duplicate-participant-deduplicated',
    round: { id: R(8), chatId: CHAT_S, status: 'open', bids: simple3() },
    plan: { chatId: CHAT_S, roundId: R(8), participantIds: [P_B, P_A, P_B, P_A], order: 'sequential', keepOrder: true },
    run: { outcomes: { [P_B]: spoke(M(11)), [P_A]: spoke(M(12)) } },
  },
  // The yielded ending: the second turn yields; the third never runs.
  {
    name: 'yielded-ending',
    round: { id: R(9), chatId: CHAT_S, status: 'open', bids: simple3() },
    plan: { chatId: CHAT_S, roundId: R(9), participantIds: [P_A, P_B, P_C], order: 'sequential' },
    run: { outcomes: { [P_C]: spoke(M(13)), [P_A]: { spoke: true, yielded: true, messageId: M(14) }, [P_B]: spoke(M(15)) } },
  },
  // The stopped ending: the operator stops during the first turn.
  {
    name: 'stopped-ending',
    round: { id: R(10), chatId: CHAT_S, status: 'open', bids: simple3() },
    plan: { chatId: CHAT_S, roundId: R(10), participantIds: [P_A, P_B, P_C], order: 'sequential' },
    run: { outcomes: { [P_C]: spoke(M(16)), [P_A]: spoke(M(17)), [P_B]: spoke(M(18)) }, stopDuring: P_C },
  },
  // A Destroy bid alone plans (and runs) fine; its ceiling is `destroy`.
  {
    name: 'destroy-alone',
    round: { id: R(11), chatId: CHAT_S, status: 'open', bids: [bid(P_D, 'destroy'), bid(P_A, 'simple')] },
    plan: { chatId: CHAT_S, roundId: R(11), participantIds: [P_D], order: 'sequential' },
    run: { outcomes: { [P_D]: SILENT } },
  },
  // ── Refusals, in v4's gate order ──
  {
    name: 'refuse-round-running',
    round: { id: R(12), chatId: CHAT_S, status: 'running', bids: simple3() },
    plan: { chatId: CHAT_S, roundId: R(12), participantIds: [P_A], order: 'sequential' },
  },
  {
    name: 'refuse-round-of-other-chat',
    round: { id: R(13), chatId: CHAT_D, status: 'open', bids: [bid(P_X, 'simple')] },
    plan: { chatId: CHAT_S, roundId: R(13), participantIds: [P_X], order: 'sequential' },
  },
  {
    name: 'refuse-round-missing-before-empty',
    round: null,
    plan: { chatId: CHAT_S, roundId: R_MISSING, participantIds: [], order: 'sequential' },
  },
  {
    name: 'refuse-empty',
    round: { id: R(14), chatId: CHAT_S, status: 'open', bids: simple3() },
    plan: { chatId: CHAT_S, roundId: R(14), participantIds: [], order: 'sequential' },
  },
  {
    name: 'refuse-not-a-bid-pass',
    round: { id: R(15), chatId: CHAT_S, status: 'open', bids: [bid(P_A, 'simple'), bid(P_B, null, { status: 'pass', reason: 'Nothing to add' })] },
    plan: { chatId: CHAT_S, roundId: R(15), participantIds: [P_A, P_B], order: 'sequential' },
  },
  {
    name: 'refuse-not-a-bid-no-tier',
    round: { id: R(16), chatId: CHAT_S, status: 'open', bids: [bid(P_A, null)] },
    plan: { chatId: CHAT_S, roundId: R(16), participantIds: [P_A], order: 'sequential' },
  },
  {
    name: 'refuse-not-a-bid-no-card',
    round: { id: R(17), chatId: CHAT_S, status: 'open', bids: [bid(P_A, 'simple')] },
    plan: { chatId: CHAT_S, roundId: R(17), participantIds: [P_A, P_C], order: 'sequential' },
  },
  {
    name: 'refuse-not-a-bid-before-destroy',
    round: { id: R(18), chatId: CHAT_S, status: 'open', bids: [bid(P_D, 'destroy'), bid(P_B, null, { status: 'pass' })] },
    plan: { chatId: CHAT_S, roundId: R(18), participantIds: [P_D, P_B], order: 'sequential' },
  },
  {
    name: 'refuse-destroy-with-others',
    round: { id: R(19), chatId: CHAT_S, status: 'open', bids: [bid(P_A, 'simple'), bid(P_D, 'destroy')] },
    plan: { chatId: CHAT_S, roundId: R(19), participantIds: [P_A, P_D], order: 'sequential' },
  },
  {
    name: 'refuse-destroy-before-chat-missing',
    round: { id: R(20), chatId: CHAT_GONE, status: 'open', bids: [bid(P_A, 'simple'), bid(P_D, 'destroy')] },
    plan: { chatId: CHAT_GONE, roundId: R(20), participantIds: [P_A, P_D], order: 'sequential' },
  },
  {
    name: 'refuse-chat-missing',
    round: { id: R(21), chatId: CHAT_GONE, status: 'open', bids: [bid(P_A, 'simple')] },
    plan: { chatId: CHAT_GONE, roundId: R(21), participantIds: [P_A], order: 'sequential' },
  },
  {
    name: 'refuse-chat-missing-keep-order',
    round: { id: R(22), chatId: CHAT_GONE, status: 'open', bids: [bid(P_A, 'simple')] },
    plan: { chatId: CHAT_GONE, roundId: R(22), participantIds: [P_A], order: 'sequential', keepOrder: true },
  },
];

const PINNED = [
  "Storyteller run keeps the operator's order",
  'Storyteller round running',
  'Storyteller round closed',
  '[Turn Manager] Cycle order persisted',
];

async function main(): Promise<void> {
  const mainSrc = process.env.QT_FIXTURE_PSR_MAIN;
  const mountSrc = process.env.QT_FIXTURE_PSR_MOUNT;
  if (!mainSrc || !mountSrc) throw new Error('QT_FIXTURE_PSR_MAIN / QT_FIXTURE_PSR_MOUNT unset');
  const scratch = mkdtempSync(join(tmpdir(), 'qt-psr-oracle-'));
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
  const { planStorytellerRun, runStorytellerRound } = await import('@/lib/storyteller/round.service');
  const { runWithTurnControl, stopActiveTurns } = await import('@/lib/chat/turn-control');
  await initializeDatabase();
  const repos = getRepositories();
  const raw = () => {
    const db = getRawDatabase();
    if (!db) throw new Error('main DB handle unavailable');
    return db;
  };

  const realRandom = Math.random;
  const out: string[] = [JSON.stringify({ op: 'header', pinnedTimestamps: PINNED_TS })];
  for (const [i, c] of CASES.entries()) {
    lines.length = 0;
    if (c.round) {
      await repos.storytellerRounds.create(
        {
          chatId: c.round.chatId,
          triggerMessageId: M_OP,
          status: c.round.status,
          bids: c.round.bids,
          selection: null,
          closedReason: null,
          closedAt: null,
        } as never,
        { id: c.round.id, createdAt: T(i), updatedAt: T(i) },
      );
    }

    // `DrawSource::sequence` semantics: in order, then the last value; `0`
    // when the case pins none.
    const seq = c.draws ?? [];
    let drawsUsed = 0;
    Math.random = () => {
      const v = seq.length === 0 ? 0 : seq[Math.min(drawsUsed, seq.length - 1)];
      drawsUsed += 1;
      return v;
    };

    let plan: unknown;
    let planned: Awaited<ReturnType<typeof planStorytellerRun>> | null = null;
    try {
      planned = await planStorytellerRun(c.plan);
      plan = {
        round: planned.round,
        turns: planned.turns.map((t) => ({
          participantId: t.participantId,
          characterName: t.characterName,
          concurrent: t.concurrent,
          profileKey: t.profileKey,
          bid: t.bid,
        })),
      };
    } catch (e) {
      const err = e as Error & { status?: number };
      plan = err.name === 'StorytellerRoundError'
        ? { refused: { message: err.message, status: err.status } }
        : { threw: err.message };
    }
    Math.random = realRandom;

    let run: unknown = null;
    if (planned && c.run) {
      const spec = c.run;
      const calls: unknown[] = [];
      const events: unknown[] = [];
      const result = await runWithTurnControl(c.plan.chatId, () =>
        runStorytellerRound({
          chatId: c.plan.chatId,
          round: planned!.round,
          turns: planned!.turns,
          order: c.plan.order,
          runTurn: async (turn, context) => {
            calls.push({ participantId: turn.participantId, context });
            if (spec.stopDuring === turn.participantId) void stopActiveTurns(c.plan.chatId);
            return spec.outcomes[turn.participantId];
          },
          onTurnStart: (turn, depth) => events.push(['start', turn.participantId, depth]),
          onTurnEnd: (turn, depth, outcome) => events.push(['end', turn.participantId, depth, outcome.spoke]),
        }),
      );
      run = { calls, events, ended: result.ended, lastOutcome: result.lastOutcome };
    }

    const roundRow = c.round
      ? raw().prepare('SELECT * FROM "storyteller_rounds" WHERE "id" = ?').get(c.round.id) ?? null
      : null;
    const chats = raw()
      .prepare('SELECT "id", "cycleOrderParticipantIds", "spokenThisCycleParticipantIds" FROM "chats" ORDER BY "id"')
      .all();
    out.push(
      JSON.stringify({ i, name: c.name, input: c, plan, run, drawsUsed, roundRow, chats, logs: [...lines] }),
    );
  }

  await closeDatabase();
  rmSync(scratch, { recursive: true, force: true });
  process.stdout.write(out.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`plan-storyteller-run oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
