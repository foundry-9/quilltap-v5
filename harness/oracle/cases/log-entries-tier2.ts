/**
 * Tier-2 oracle case (P4.D331) — the Log's repository and its one write door,
 * v4 `LogEntriesRepository` + `writeLogEntry` at the `322a5488e` pin, over the
 * llm-logs fixture `build-log-entries-fixture.ts` seeds.
 *
 * Every op of the committed spec (`fixtures/log-entries-tier2.json`) runs
 * through v4's REAL code on a working COPY of the fixture, and one NDJSON row
 * per op records:
 *   - `out` — what the op answered (the entries for a read, the entry for a
 *     create / write, the count for a cleanup), or `error` — the thrown text;
 *   - `lines` — every logger call the op made (a `Logger.prototype` spy:
 *     `LEVEL message key=value …`, an object field rendered as `<key>Json=` +
 *     compact JSON, `null` as `null`);
 *   - `dump` — after a mutating op, the whole table: every column + `hex(message)`.
 *
 * NORMALIZATION: an id the spec did not pin is `<minted>` and its two stamps
 * `<now>`; the dump sorts minted rows LAST, by message hex then id (both sides
 * sort the same way). Reads keep v4's order — newest first is the point.
 *
 * The cleanup ops run under an injected clock: `Date` is swapped for a
 * subclass whose no-argument construction is the spec's `nowIso`, and the
 * process runs in the spec's zone (`TZ`), so `setDate(getDate() - days)` is
 * v4's own host-local arithmetic over a fixed instant.
 *
 * Run from the pinned v4 checkout under Node 24, AFTER building the fixture:
 *   cd ~/source/quilltap-server
 *   TZ=America/Chicago QT_FIXTURE_LOG_ENTRIES=/tmp/qt-log-entries-fixture.db \
 *     npx tsx $V5W/harness/oracle/cases/log-entries-tier2.ts \
 *     > /tmp/oracle-log-entries-tier2.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, rmSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

type Msg = string | { repeat: string; count: number; then?: string };
const expand = (m: Msg): string =>
  typeof m === 'string' ? m : m.repeat.repeat(m.count) + (m.then ?? '');

interface Op {
  id: string;
  kind: 'find' | 'search' | 'create' | 'write' | 'cleanup';
  filter?: Record<string, unknown>;
  pattern?: string;
  flags?: string;
  limit?: number;
  message?: Msg;
  chatId?: string;
  characterId?: string | null;
  options?: { id: string; createdAt: string; updatedAt: string };
  retentionDays?: number | 'NaN';
}

interface Spec {
  testPepperBase64: string;
  tz: string;
  nowIso: string;
  seed: Array<{ id: string }>;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'log-entries-tier2.json'), 'utf8'),
  ) as Spec;
  if (process.env.TZ !== spec.tz) {
    throw new Error(`run under TZ=${spec.tz} (the spec's zone); got ${process.env.TZ}`);
  }

  const fixture = process.env.QT_FIXTURE_LOG_ENTRIES;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_LOG_ENTRIES must point at build-log-entries-fixture.ts output');
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-log-entries-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'llm-logs-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'data', 'mount-index.db');
  process.env.SQLITE_LLM_LOGS_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawLLMLogsDatabase } = await import('@/lib/database/backends/sqlite/llm-logs-client');
  const { LogEntriesRepository } = await import('@/lib/database/repositories/log-entries.repository');
  const { writeLogEntry } = await import('@/lib/log-entries/write');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { Logger } = await import('@/lib/logger');

  // Every logger call, whatever the level filter — the spy sits above it.
  let sink: string[] | null = null;
  const render = (level: string, message: string, context?: Record<string, unknown>) => {
    const parts = [level.toUpperCase(), message];
    for (const [k, v] of Object.entries(context ?? {})) {
      if (v === undefined) continue;
      if (v !== null && typeof v === 'object') parts.push(`${k}Json=${JSON.stringify(v)}`);
      else if (typeof v === 'number') parts.push(`${k}=${JSON.stringify(v)}`);
      else parts.push(`${k}=${v === null ? 'null' : String(v)}`);
    }
    return parts.join(' ');
  };
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const original = Logger.prototype[level];
    Logger.prototype[level] = function (this: unknown, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
      sink?.push(render(level, message, context));
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }

  await initializeDatabase();
  // The write door reaches the container's repository; the reads use a fresh
  // one — the same class, the same connection.
  const repo = new LogEntriesRepository();
  void getRepositories().logEntries;

  const pinned = new Set<string>(spec.seed.map((r) => r.id));
  for (const op of spec.ops) if (op.options?.id) pinned.add(op.options.id);
  const isMinted = (id: unknown) => typeof id === 'string' && !pinned.has(id);
  const maskEntry = (e: Record<string, unknown>) => {
    const m = isMinted(e.id);
    // A clip that splits a surrogate pair leaves a LONE surrogate in the JS
    // string v4 hands back (the stored bytes are U+FFFD — `dump`'s hex); JSON
    // cannot carry it, so the echo is emitted well-formed and the write rows
    // record `outWellFormed` beside it.
    return {
      id: m ? '<minted>' : e.id,
      message: typeof e.message === 'string' ? e.message.toWellFormed() : e.message,
      chatId: e.chatId,
      characterId: e.characterId,
      createdAt: m ? '<now>' : e.createdAt,
      updatedAt: m ? '<now>' : e.updatedAt,
    };
  };
  // A read keeps v4's order, except that a run of rows minted "now" ties on
  // `createdAt` up to clock noise — such a run is sorted by message bytes.
  const settleMintedRuns = (out: Array<Record<string, unknown>>) => {
    const key = (e: Record<string, unknown>) => Buffer.from(String(e.message), 'utf8').toString('hex');
    const result: Array<Record<string, unknown>> = [];
    let run: Array<Record<string, unknown>> = [];
    const flush = () => {
      run.sort((a, b) => (key(a) < key(b) ? -1 : key(a) > key(b) ? 1 : 0));
      result.push(...run);
      run = [];
    };
    for (const e of out) {
      if (e.createdAt === '<now>') run.push(e);
      else {
        flush();
        result.push(e);
      }
    }
    flush();
    return result;
  };
  const maskLine = (line: string, minted: string[]) =>
    minted.reduce((acc, id) => acc.split(id).join('<minted>'), line);

  const RealDate = Date;
  const fixedNow = RealDate.parse(spec.nowIso);
  class FixedDate extends RealDate {
    constructor(...args: unknown[]) {
      if (args.length === 0) super(fixedNow);
      else super(...(args as [number]));
    }
    static now() {
      return fixedNow;
    }
  }

  const lldb = getRawLLMLogsDatabase();
  if (!lldb) throw new Error('llm-logs DB handle unavailable');
  const dump = () => {
    const rows = lldb
      .prepare('SELECT "id", "message", "chatId", "characterId", "createdAt", "updatedAt", hex("message") AS "messageHex" FROM log_entries')
      .all() as Array<Record<string, unknown>>;
    return rows
      .map((r) => ({ ...maskEntry(r), message: undefined, messageHex: r.messageHex }))
      .map(({ message: _m, ...rest }) => rest)
      .sort((a, b) => {
        const am = a.id === '<minted>' ? 1 : 0;
        const bm = b.id === '<minted>' ? 1 : 0;
        if (am !== bm) return am - bm;
        const ah = String(a.messageHex);
        const bh = String(b.messageHex);
        if (ah !== bh) return ah < bh ? -1 : 1;
        return String(a.id) < String(b.id) ? -1 : String(a.id) > String(b.id) ? 1 : 0;
      });
  };

  for (const op of spec.ops) {
    const lines: string[] = [];
    sink = lines;
    const row: Record<string, unknown> = { case: 'log-entries-tier2', op: op.id, kind: op.kind };
    const minted: string[] = [];
    try {
      switch (op.kind) {
        case 'find': {
          const got = await repo.findEntries(op.filter as never);
          row.out = settleMintedRuns(got.map((e) => maskEntry(e as never)));
          break;
        }
        case 'search': {
          const got = await repo.searchMessages(op.pattern!, op.flags!, op.limit!);
          row.out = settleMintedRuns(got.map((e) => maskEntry(e as never)));
          break;
        }
        case 'create': {
          const got = await repo.create(
            { message: expand(op.message!), chatId: op.chatId!, characterId: op.characterId ?? null },
            op.options,
          );
          if (isMinted(got.id)) minted.push(got.id);
          row.out = maskEntry(got as never);
          break;
        }
        case 'write': {
          const got = await writeLogEntry({
            message: expand(op.message!),
            chatId: op.chatId!,
            characterId: op.characterId ?? null,
          });
          if (isMinted(got.id)) minted.push(got.id);
          row.out = maskEntry(got as never);
          row.outHex = Buffer.from(got.message, 'utf8').toString('hex').toUpperCase();
          row.outWellFormed = got.message.isWellFormed();
          break;
        }
        case 'cleanup': {
          const days = op.retentionDays === 'NaN' ? Number.NaN : (op.retentionDays as number);
          globalThis.Date = FixedDate as DateConstructor;
          try {
            row.out = await repo.cleanupOldEntries(days);
          } finally {
            globalThis.Date = RealDate;
          }
          break;
        }
      }
    } catch (error) {
      row.error = error instanceof Error ? error.message : String(error);
    }
    sink = null;
    row.lines = lines.map((l) => maskLine(l, minted));
    if (op.kind === 'create' || op.kind === 'write' || op.kind === 'cleanup') row.dump = dump();
    process.stdout.write(JSON.stringify(row) + '\n');
  }

  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`log-entries-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
