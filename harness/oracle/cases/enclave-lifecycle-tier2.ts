/**
 * Tier-2 oracle case — the enclave lifecycle service (U4.3; v4
 * `lib/services/chat-message/autonomous-room.service.ts`
 * `startAutonomousRoomManually` / `pauseAutonomousRoom` / `stopAutonomousRoom` /
 * `resumeAutonomousRoom` / `updateAutonomousRoomSettings` /
 * `reconcileAutonomousRunsAtStartup` / `reconcileFailedAutonomousTurn`, plus
 * `beginAutonomousRun` from `lib/background-jobs/handlers/autonomous-run-start.ts`).
 *
 * These read/write the DB via `getRepositories()` (no LLM anywhere), so — like
 * the memory-delete / turn-orchestrator cases — this runs directly under the tsx
 * real-DB path (no jest).
 *
 * INJECTION:
 *   - The global `Date` is swapped for a stepped mock, RE-ANCHORED before each
 *     op at `spec.baseMs + opIndex * spec.stepPerOpMs` and advancing +1 ms per
 *     read within the op ([[chain-depth-frozen-clock-artifact]]). Each op's
 *     FIRST clock read therefore lands exactly on its per-op anchor — those
 *     first-read-derived values (runStartedAt / scheduleLastRunAt /
 *     runPausedAt / the resume accumulation arithmetic / stop's runEndedAt /
 *     the cron anchors) are PINNED in the diff; later-read minted timestamps
 *     are placeholdered by the harness (see the Rust test).
 *   - `crypto.randomUUID` is NOT monkey-patched: v4 imports the destructured
 *     binding (`import { randomUUID } from 'node:crypto'`), which an in-process
 *     patch cannot reach under ESM/tsx. Minted UUIDs are handled by the shared
 *     first-seen remap in the harness instead (payload.runId ↔ chats.currentRunId
 *     relationships verify through the shared map).
 *   - The v4 job-processor host is neutralized (`childCrashed: true` pre-seeded
 *     into the HMR-global state) so `ensureProcessorRunning()` inside
 *     `enqueueJob` can never fork a child that would claim the enqueued turn
 *     jobs before the dump (the tsx analogue of the jest ensureProcessorRunning
 *     no-op pin).
 *   - Croner runs REAL here (TZ pinned to UTC before any import); the Rust side
 *     injects a canned closure whose triples are pre-verified against croner.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   TZ=UTC \
 *   QT_FIXTURE_ENCLAVE=/tmp/qt-enclave-lifecycle-main.db \
 *   QT_FIXTURE_ENCLAVE_MOUNT=/tmp/qt-enclave-lifecycle-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/enclave-lifecycle-tier2.ts > /tmp/oracle-enclave-lifecycle.ndjson
 */

process.env.TZ = 'UTC';

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind:
    | 'manualStart'
    | 'pause'
    | 'stop'
    | 'resume'
    | 'updateSettings'
    | 'begin'
    | 'reconcileFailed'
    | 'reconcileStartup';
  chatId?: string | null;
  userId?: string;
  runId?: string | null;
  reason?: string;
  patch?: Record<string, unknown>;
  note?: string;
}
interface Spec {
  testPepperBase64: string;
  baseMs: number;
  stepPerOpMs: number;
  ops: Op[];
}

const RealDate = Date;
let mockClock = 0;
function installClock(anchorMs: number): void {
  mockClock = anchorMs;
  const next = () => mockClock++;
  const MockDate = class extends RealDate {
    constructor(...args: unknown[]) {
      if (args.length === 0) {
        super(next());
      } else {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        super(...(args as [any]));
      }
    }
    static now(): number {
      return next();
    }
  } as DateConstructor;
  globalThis.Date = MockDate;
}
function restoreClock(): void {
  globalThis.Date = RealDate;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'enclave-lifecycle-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_ENCLAVE;
  const fixtureMount = process.env.QT_FIXTURE_ENCLAVE_MOUNT;
  if (!fixture || !existsSync(fixture) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error(
      'QT_FIXTURE_ENCLAVE and QT_FIXTURE_ENCLAVE_MOUNT must point at the seed fixtures',
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-enclave-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'enclave-main.db');
  const workMount = join(scratch, 'enclave-mount.db');
  copyFileSync(fixture, work);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // Neutralize the job-processor host BEFORE any service import: enqueueJob's
  // ensureProcessorRunning() would otherwise fork a child that claims the
  // enqueued AUTONOMOUS_ROOM_TURN jobs before the dump. `childCrashed: true` is
  // the host's own permanent give-up latch — every ensureProcessorRunning call
  // returns immediately.
  (globalThis as Record<string, unknown>).__quilltapJobHost = {
    child: null,
    spawning: false,
    shuttingDown: false,
    childCrashed: true,
    restartTimestamps: [],
    signalHandlersInstalled: true,
    invalidationListeners: new Set(),
  };

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const {
    startAutonomousRoomManually,
    pauseAutonomousRoom,
    stopAutonomousRoom,
    resumeAutonomousRoom,
    updateAutonomousRoomSettings,
    reconcileAutonomousRunsAtStartup,
    reconcileFailedAutonomousTurn,
  } = await import('@/lib/services/chat-message/autonomous-room.service');
  const { beginAutonomousRun } = await import(
    '@/lib/background-jobs/handlers/autonomous-run-start'
  );

  await initializeDatabase();

  const results: Array<Record<string, unknown>> = [];

  for (let i = 0; i < spec.ops.length; i++) {
    const op = spec.ops[i];
    const anchor = spec.baseMs + i * spec.stepPerOpMs;
    installClock(anchor);
    try {
      switch (op.kind) {
        case 'manualStart': {
          const r = await startAutonomousRoomManually(op.chatId as string, op.userId as string);
          results.push({
            op: 'manualStart',
            chatId: op.chatId,
            ok: r.ok,
            runId: r.ok ? r.runId : null,
            jobId: r.ok ? r.jobId : null,
            reason: r.ok ? null : r.reason,
            message: r.ok ? null : r.message,
          });
          break;
        }
        case 'pause': {
          const r = await pauseAutonomousRoom(op.chatId as string);
          results.push({
            op: 'pause',
            chatId: op.chatId,
            ok: r.ok,
            message: r.message ?? null,
          });
          break;
        }
        case 'stop': {
          const r = await stopAutonomousRoom(op.chatId as string);
          results.push({
            op: 'stop',
            chatId: op.chatId,
            ok: r.ok,
            message: r.message ?? null,
          });
          break;
        }
        case 'resume': {
          const r = await resumeAutonomousRoom(op.chatId as string, op.userId as string);
          results.push({
            op: 'resume',
            chatId: op.chatId,
            ok: r.ok,
            runId: r.ok ? r.runId : null,
            jobId: r.ok ? r.jobId : null,
            reason: r.ok ? null : r.reason,
            message: r.ok ? null : r.message,
          });
          break;
        }
        case 'updateSettings': {
          const r = await updateAutonomousRoomSettings(
            op.chatId as string,
            op.userId as string,
            (op.patch ?? {}) as never,
          );
          results.push({
            op: 'updateSettings',
            chatId: op.chatId,
            ok: r.ok,
            clampedDestructive: r.ok ? r.clampedDestructive : null,
            reason: r.ok ? null : r.reason,
            message: r.ok ? null : r.message,
          });
          break;
        }
        case 'begin': {
          const nowIso = new RealDate(anchor).toISOString();
          const r = await beginAutonomousRun({
            chatId: op.chatId as string,
            userId: op.userId as string,
            runId: op.runId as string,
            nowIso,
            onEnqueueFailure: 'error',
          });
          results.push({
            op: 'begin',
            chatId: op.chatId,
            ok: r.ok,
            runId: r.ok ? r.runId : null,
            jobId: r.ok ? r.jobId : null,
            reason: r.ok ? null : r.reason,
            message: r.ok ? null : r.message,
            hasCause: r.ok ? false : r.cause !== undefined,
          });
          break;
        }
        case 'reconcileFailed': {
          await reconcileFailedAutonomousTurn(
            { chatId: op.chatId ?? undefined, runId: op.runId ?? undefined },
            op.reason as string,
          );
          results.push({ op: 'reconcileFailed', chatId: op.chatId ?? null, done: true });
          break;
        }
        case 'reconcileStartup': {
          const r = await reconcileAutonomousRunsAtStartup();
          results.push({ op: 'reconcileStartup', reconciledCount: r.reconciledCount });
          break;
        }
      }
    } finally {
      restoreClock();
    }
  }

  async function dump(table: string): Promise<unknown> {
    const columns = (
      (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy: 'id' });
  }

  const dumps = {
    chats: await dump('chats'),
    background_jobs: await dump('background_jobs'),
    chat_messages: await dump('chat_messages'),
  };

  await closeDatabase();
  await closeMountIndexSQLiteClient();

  process.stdout.write(JSON.stringify({ case: 'enclave-lifecycle-tier2', results, dumps }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`enclave-lifecycle oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
