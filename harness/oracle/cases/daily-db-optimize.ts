/**
 * ORACLE for P4.D259 — v4 `f5e953a3f`'s PHASE 0.75 daily backup + optimize
 * pass (`lib/startup/daily-db-optimize.ts`) and the never-ported physical
 * backups it calls (`lib/database/backends/sqlite/physical-backup.ts`: the
 * three `VACUUM INTO` backups, the 24-hour gate, the retention policy), driven
 * through v4's REAL code over a COPY of a committed fixture trio.
 *
 * Every row builds a fresh instance under its own scratch base dir
 * (`QUILLTAP_DATA_DIR`, so v4's `getDataDir()` is `<base>/data`), copies the
 * row's databases in as `quilltap.db` / `quilltap-llm-logs.db` /
 * `quilltap-mount-index.db`, applies the row's PLANT (recorded in the row so
 * the Rust side rebuilds it byte-for-byte), then runs the row's ACTIONS, each
 * with the global `Date` frozen at the action's `nowMs` (`new Date()` and
 * `Date.now()` answer it; every other constructor form is the real one):
 *
 *   - `pass`      — `runDailyDbOptimize(new Date(nowMs))`, then the
 *                   migration layer's `closeSQLite()` + the instance lock's
 *                   release (the main handle is the cached, lock-holding one).
 *   - `phase2`    — backend.ts `connect()`'s startup backup EXACTLY as it
 *                   chains it (`backend.ts:561-568, 580-584, 605-610`): the
 *                   main backup `.then(applyRetentionPolicy).catch(<root
 *                   ERROR>)`, then the LLM-logs and mount-index backups each
 *                   `.catch(<root ERROR>)`, called synchronously in that order
 *                   on fresh keyed handles; awaited until every promise has
 *                   settled. A partition whose file is absent is skipped (v4's
 *                   client answers no connection).
 *   - `retention` — `applyRetentionPolicy()` alone.
 *   - `optimizeReadonly` — `optimizeDatabase(db, 'main')` on a READ-ONLY
 *                   keyed handle of the main copy (the stop-at-first-failure
 *                   arm through a real SQLite refusal).
 *
 * Recorded per action: every line (level, message, the merged fields — the
 * child's `module` first, as v4's `log()` merges it) whose module is
 * `startup:daily-db-optimize` or `database:physical-backup`, plus the four
 * root-logger lines this feature owns (backend.ts's three startup-backup
 * ERRORs and instrumentation.ts's PHASE 0.75 ERROR). The base dir is
 * normalized to `<base>` inside every string field. Recorded per row after
 * the actions: the state file's BYTES (or `null` / `"<dir>"`), the
 * `data/backups/` listing (sorted; each NEW backup's table count when opened
 * with the key — proves the copy is encrypted under the same key), and per
 * database `page_count`, `freelist_count`, the `sqlite_stat1` rows and the
 * `sqlite_stat4` row count (v4's driver is built with `SQLITE_ENABLE_STAT4`,
 * so its `ANALYZE` repopulates `sqlite_stat4`; the count is what lets the
 * Rust side tell a build difference from a layout one).
 *
 * Tier-1 rows (`stamp`) record `localDateStamp(new Date(ms))`.
 *
 * `TZ` is load-bearing (v4's stamp, filenames, parsers and Phase-4 year are
 * the process's LOCAL zone): the family runs the corpus TWICE, `TZ=UTC` and
 * `TZ=America/Chicago`, and every row carries its `tz`. This is a `tsx`
 * script, not a jest case, because v4's `jest.config.ts` pins `TZ=UTC` before
 * forking its workers (and a `process.env.TZ` assignment inside a jest worker
 * never reaches the real process) — measured: a jest run "under Chicago"
 * recorded UTC instants.
 *
 * Run (Node 24, from the v4 checkout or a PINNED worktree). The fixture env
 * vars name the committed trio; the sweep driver copies them to /tmp first.
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DDO_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-main.db \
 *     QT_FIXTURE_DDO_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-mount.db \
 *     QT_FIXTURE_DDO_LLMLOGS=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-llmlogs.db \
 *     TZ=UTC QT_ORACLE_OUT=/tmp/oracle-daily-db-optimize.ndjson \
 *     $N/npx tsx $V5W/harness/oracle/cases/daily-db-optimize.ts
 *   QT_FIXTURE_DDO_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-main.db \
 *     QT_FIXTURE_DDO_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-mount.db \
 *     QT_FIXTURE_DDO_LLMLOGS=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-llmlogs.db \
 *     TZ=America/Chicago QT_ORACLE_OUT=/tmp/oracle-daily-db-optimize-chicago.ndjson \
 *     $N/npx tsx $V5W/harness/oracle/cases/daily-db-optimize.ts
 */

import * as fs from 'node:fs';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

// The frozen clock: `new Date()` / `Date.now()` answer `fakeNow`; every other
// constructor form (and `Date.UTC`) is the real one. Installed before any v4
// module loads.
const RealDate = Date;
let fakeNow = RealDate.now();
class FrozenDate extends RealDate {
  constructor(...args: unknown[]) {
    if (args.length === 0) super(fakeNow);
    else super(...(args as [number]));
  }
  static now(): number {
    return fakeNow;
  }
}
globalThis.Date = FrozenDate as unknown as DateConstructor;

/** The committed trio's test pepper (`chat-delete-web.json`) — never a real instance's. */
const TEST_PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const DAY = 86_400_000;
const HOUR = 3_600_000;


/** The deleted-rows plant: a table filled then emptied, so every file has a non-zero freelist. */
const PLANT_SQL = [
  'CREATE TABLE qt_optimize_plant (id INTEGER PRIMARY KEY, v TEXT)',
  "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 400) INSERT INTO qt_optimize_plant (v) SELECT printf('%.300c%d', 'p', i) FROM n",
  'DELETE FROM qt_optimize_plant',
];

const GARBAGE = { len: 4608, mul: 131, add: 17 };

type Db = 'main' | 'llmLogs' | 'mountIndex';
const FILE_OF: Record<Db, string> = {
  main: 'quilltap.db',
  llmLogs: 'quilltap-llm-logs.db',
  mountIndex: 'quilltap-mount-index.db',
};

type Action =
  | { op: 'pass'; nowMs: number }
  | { op: 'phase2'; nowMs: number }
  | { op: 'retention'; nowMs: number }
  | { op: 'optimizeReadonly'; nowMs: number };

/** A planted file under `data/backups/`, named by its LOCAL timestamp. */
type BackupPlant = { prefix: string; ageMs: number; name?: string };

interface Plant {
  /** Which databases exist (the rest are absent). */
  dbs: Db[];
  /** Databases that get PLANT_SQL. */
  deleted: Db[];
  /** A database replaced by the garbage pattern (v4 cannot open it). */
  garbage?: Db;
  /** The state file: `absent`, a literal string, or `dir` (a DIRECTORY at the path). */
  state: { kind: 'absent' } | { kind: 'text'; text: string } | { kind: 'dir' };
  /** `data/backups` shape. */
  backups: { kind: 'absent' } | { kind: 'file' } | { kind: 'tree'; files: string[] };
}

interface RowSpec {
  name: string;
  plant: Omit<Plant, 'state' | 'backups'> & {
    state: Plant['state'] | { kind: 'stamps'; entries: [string, string][] };
    backups: { kind: 'absent' } | { kind: 'file' } | { kind: 'tree'; plants: BackupPlant[]; extra?: string[] };
  };
  actions: { op: Action['op']; at: number }[];
}

type Line = { level: string; message: string; fields: Record<string, unknown> };

const OWNED_MODULES = new Set(['startup:daily-db-optimize', 'database:physical-backup']);
const OWNED_ROOT = new Set([
  'Startup physical backup or retention policy failed',
  'LLM logs startup physical backup failed',
  'Mount index startup physical backup failed',
  'Daily database optimize failed — continuing startup',
]);

function garbageBytes(): Buffer {
  const b = Buffer.alloc(GARBAGE.len);
  for (let i = 0; i < b.length; i++) b[i] = (i * GARBAGE.mul + GARBAGE.add) & 255;
  return b;
}

/** v4's own filename layout, read back from a LOCAL Date (the generator is private in v4). */
function localStamp(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}`;
}

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  const fixtures: Record<Db, string> = {
    main: process.env.QT_FIXTURE_DDO_MAIN ?? '',
    mountIndex: process.env.QT_FIXTURE_DDO_MOUNT ?? '',
    llmLogs: process.env.QT_FIXTURE_DDO_LLMLOGS ?? '',
  };
  for (const [k, p] of Object.entries(fixtures)) {
    if (!p || !fs.existsSync(p)) throw new Error(`fixture ${k} missing: ${p}`);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-daily-db-optimize-oracle-'));
  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  delete process.env.SQLITE_PATH;
  delete process.env.SQLITE_LLM_LOGS_PATH;
  delete process.env.SQLITE_MOUNT_INDEX_PATH;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // `better-sqlite3` is v4's alias for `better-sqlite3-multiple-ciphers`.
  const Database = createRequire(join(process.cwd(), 'package.json'))('better-sqlite3');

  const optimize = await import('@/lib/startup/daily-db-optimize');
  const physical = await import('@/lib/database/backends/sqlite/physical-backup');
  const dbUtils = await import('@/migrations/lib/database-utils');
  const lock = await import('@/lib/database/backends/sqlite/instance-lock');
  const { logger, Logger } = await import('@/lib/logger');

  let sink: Line[] | null = null;
  let baseToken: string | null = null;
  const normalize = (v: unknown): unknown =>
    typeof v === 'string' && baseToken !== null ? v.split(baseToken).join('<base>') : v;
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const original = (Logger.prototype as unknown as Record<string, unknown>)[level];
    (Logger.prototype as unknown as Record<string, unknown>)[level] = function (
      this: { context?: Record<string, unknown> },
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      if (sink) {
        const merged: Record<string, unknown> = { ...(this.context ?? {}), ...(context ?? {}) };
        const owned =
          (typeof merged.module === 'string' && OWNED_MODULES.has(merged.module)) ||
          (merged.module === undefined && OWNED_ROOT.has(message));
        if (owned) {
          const fields: Record<string, unknown> = {};
          for (const [k, v] of Object.entries(merged)) {
            // winston drops an `undefined` field; so does the capture.
            if (v === undefined) continue;
            fields[k] = normalize(v);
          }
          sink.push({ level, message, fields });
        }
      }
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    };
  }

  const keyHex = Buffer.from(TEST_PEPPER, 'base64').toString('hex');
  const openKeyed = (path: string, readonly = false) => {
    const db = new Database(path, readonly ? { readonly: true, fileMustExist: true } : {});
    db.pragma(`key = "x'${keyHex}'"`);
    return db;
  };

  const settle = () => new Promise<void>((r) => setImmediate(r));

  const outLines: string[] = [];
  const out = (row: unknown) => outLines.push(JSON.stringify(row));

  try {
    out({ kind: 'recipe', plantSql: PLANT_SQL, garbage: GARBAGE, pepper: TEST_PEPPER });
    {
      const tz = process.env.TZ ?? '';
      if (tz !== 'UTC' && tz !== 'America/Chicago') {
        throw new Error(`run under TZ=UTC or TZ=America/Chicago (got ${tz || 'unset'})`);
      }
      // The rows' clock: noon LOCAL on 2026-10-08, in this zone.
      const NOW = new Date(2026, 9, 8, 12, 0, 0).getTime();
      const TODAY = '2026-10-08';
      const YESTERDAY = '2026-10-07';

      // ---- tier-1: the local calendar date -------------------------------
      const stampInstants = [
        new Date(2026, 0, 5, 23, 59).getTime(), // v4's own test rows
        new Date(2026, 11, 31, 0, 1).getTime(),
        Date.UTC(2026, 9, 9, 3, 30, 0), // UTC 10-09, Chicago 10-08
        Date.UTC(2026, 9, 8, 23, 30, 0),
        Date.UTC(2027, 0, 1, 2, 0, 0), // UTC 2027, Chicago 2026
        Date.UTC(2024, 1, 29, 12, 0, 0),
      ];
      for (const ms of stampInstants) {
        out({ kind: 'stamp', tz, nowMs: ms, stamp: optimize.localDateStamp(new Date(ms)) });
      }

      const retentionTree: BackupPlant[] = [
        ...[1, 6, 8, 10, 15, 29, 45, 50, 60, 400, 410, 800].map((d) => ({ prefix: 'quilltap-', ageMs: d * DAY })),
        ...[1, 8, 10].map((d) => ({ prefix: 'quilltap-mount-index-', ageMs: d * DAY })),
        { prefix: 'quilltap-llm-logs-', ageMs: 800 * DAY },
      ];

      const specs: RowSpec[] = [
        {
          name: 'fresh',
          plant: { dbs: ['main', 'llmLogs', 'mountIndex'], deleted: ['main', 'llmLogs', 'mountIndex'], state: { kind: 'absent' }, backups: { kind: 'absent' } },
          actions: [{ op: 'pass', at: NOW }, { op: 'phase2', at: NOW + 5_000 }],
        },
        {
          name: 'again-same-day',
          plant: { dbs: ['main', 'llmLogs', 'mountIndex'], deleted: ['main'], state: { kind: 'absent' }, backups: { kind: 'absent' } },
          actions: [{ op: 'pass', at: NOW }, { op: 'pass', at: NOW + 3 * HOUR }],
        },
        {
          name: 'one-stale',
          plant: {
            dbs: ['main', 'llmLogs', 'mountIndex'], deleted: ['llmLogs'],
            state: { kind: 'stamps', entries: [['main', TODAY], ['llm-logs', YESTERDAY], ['mount-points', TODAY]] },
            backups: { kind: 'absent' },
          },
          actions: [{ op: 'pass', at: NOW }],
        },
        {
          name: 'key-order',
          plant: {
            dbs: ['main', 'llmLogs', 'mountIndex'], deleted: [],
            state: { kind: 'stamps', entries: [['mount-points', TODAY], ['llm-logs', YESTERDAY]] },
            backups: { kind: 'absent' },
          },
          actions: [{ op: 'pass', at: NOW }],
        },
        {
          name: 'llm-absent',
          plant: { dbs: ['main', 'mountIndex'], deleted: ['mountIndex'], state: { kind: 'absent' }, backups: { kind: 'absent' } },
          actions: [{ op: 'pass', at: NOW }, { op: 'phase2', at: NOW + 5_000 }],
        },
        {
          name: 'main-only',
          plant: { dbs: ['main'], deleted: [], state: { kind: 'absent' }, backups: { kind: 'absent' } },
          actions: [{ op: 'pass', at: NOW }],
        },
        ...[
          ['state-corrupt', '{not json'],
          ['state-array', '[1,2]'],
          ['state-string', '"2026-10-08"'],
          ['state-null', 'null'],
          ['state-unknown-keys', '{"bogus":"x","main":5,"mount-points":"2026-10-08"}'],
          ['state-empty-file', ''],
        ].map(([name, text]) => ({
          name,
          plant: { dbs: ['main', 'llmLogs', 'mountIndex'] as Db[], deleted: [] as Db[], state: { kind: 'text' as const, text }, backups: { kind: 'absent' as const } },
          actions: [{ op: 'pass' as const, at: NOW }],
        })),
        {
          name: 'state-is-dir',
          plant: { dbs: ['main', 'llmLogs', 'mountIndex'], deleted: [], state: { kind: 'dir' }, backups: { kind: 'absent' } },
          actions: [{ op: 'pass', at: NOW }],
        },
        {
          name: 'gate',
          plant: {
            dbs: ['main', 'llmLogs', 'mountIndex'], deleted: [], state: { kind: 'absent' },
            backups: {
              kind: 'tree',
              plants: [
                { prefix: 'quilltap-', ageMs: 23 * HOUR },
                { prefix: 'quilltap-', ageMs: 30 * HOUR },
                { prefix: 'quilltap-llm-logs-', ageMs: 25 * HOUR },
                { prefix: 'quilltap-llm-logs-', ageMs: 49 * HOUR + 30 * 60_000 },
              ],
            },
          },
          actions: [{ op: 'phase2', at: NOW }],
        },
        {
          name: 'retention',
          plant: {
            dbs: ['main'], deleted: [], state: { kind: 'absent' },
            backups: { kind: 'tree', plants: retentionTree, extra: ['notes.txt', 'quilltap-2026-10-08T1200.db', 'quilltap-backup.db'] },
          },
          actions: [{ op: 'retention', at: NOW }],
        },
        {
          name: 'retention-after-pass',
          plant: {
            dbs: ['main', 'llmLogs', 'mountIndex'], deleted: [], state: { kind: 'absent' },
            backups: { kind: 'tree', plants: retentionTree },
          },
          actions: [{ op: 'pass', at: NOW }, { op: 'phase2', at: NOW + 5_000 }],
        },
        {
          name: 'backups-is-file',
          plant: { dbs: ['main', 'llmLogs', 'mountIndex'], deleted: [], state: { kind: 'absent' }, backups: { kind: 'file' } },
          actions: [{ op: 'pass', at: NOW }, { op: 'phase2', at: NOW + 5_000 }, { op: 'retention', at: NOW + 6_000 }],
        },
        {
          name: 'llm-garbage',
          plant: { dbs: ['main', 'llmLogs', 'mountIndex'], deleted: [], garbage: 'llmLogs', state: { kind: 'absent' }, backups: { kind: 'absent' } },
          actions: [{ op: 'pass', at: NOW }],
        },
        {
          name: 'day-boundary-late',
          plant: {
            dbs: ['main', 'llmLogs', 'mountIndex'], deleted: [],
            state: { kind: 'stamps', entries: [['main', TODAY], ['llm-logs', TODAY], ['mount-points', TODAY]] },
            backups: { kind: 'absent' },
          },
          actions: [
            { op: 'pass', at: new Date(2026, 9, 8, 23, 59, 30).getTime() },
            { op: 'pass', at: new Date(2026, 9, 9, 0, 0, 30).getTime() },
          ],
        },
        {
          name: 'optimize-readonly',
          plant: { dbs: ['main'], deleted: ['main'], state: { kind: 'absent' }, backups: { kind: 'absent' } },
          actions: [{ op: 'optimizeReadonly', at: NOW }],
        },
      ];

      for (const spec of specs) {
        const base = mkdtempSync(join(scratch, `${spec.name}-`));
        const data = join(base, 'data');
        fs.mkdirSync(data, { recursive: true });
        process.env.QUILLTAP_DATA_DIR = base;
        baseToken = base;

        // ---- the plant ---------------------------------------------------
        for (const d of spec.plant.dbs) {
          const path = join(data, FILE_OF[d]);
          if (spec.plant.garbage === d) {
            fs.writeFileSync(path, garbageBytes());
            continue;
          }
          fs.copyFileSync(fixtures[d], path);
          if (spec.plant.deleted.includes(d)) {
            const db = openKeyed(path);
            db.pragma('journal_mode = truncate');
            for (const s of PLANT_SQL) db.exec(s);
            db.close();
          }
        }
        const statePath = join(data, 'db-optimize-state.json');
        let stateText: string | null = null;
        const st = spec.plant.state;
        if (st.kind === 'text') stateText = st.text;
        if (st.kind === 'stamps') stateText = JSON.stringify(Object.fromEntries(st.entries), null, 2) + '\n';
        if (stateText !== null) fs.writeFileSync(statePath, stateText);
        if (st.kind === 'dir') fs.mkdirSync(statePath);

        const backupsDir = join(data, 'backups');
        const backupNames: string[] = [];
        const bk = spec.plant.backups;
        if (bk.kind === 'file') fs.writeFileSync(backupsDir, 'not a directory');
        if (bk.kind === 'tree') {
          fs.mkdirSync(backupsDir);
          for (const p of bk.plants) backupNames.push(`${p.prefix}${localStamp(spec.actions[0].at - p.ageMs)}.db`);
          for (const n of bk.extra ?? []) backupNames.push(n);
          for (const n of backupNames) fs.writeFileSync(join(backupsDir, n), `plant ${n}`);
        }

        // ---- the actions -------------------------------------------------
        const actions: unknown[] = [];
        for (const a of spec.actions) {
          fakeNow = a.at;
          sink = [];
          let result: unknown = null;
          if (a.op === 'pass') {
            try {
              await optimize.runDailyDbOptimize(new Date(a.at));
            } catch (error) {
              // instrumentation.ts:409-417 — never reached by the corpus (the
              // pass catches everything), recorded if it ever is.
              logger.error('Daily database optimize failed — continuing startup', {
                context: 'instrumentation.register',
                error: error instanceof Error ? error.message : String(error),
              });
            }
            dbUtils.closeSQLite();
            lock.releaseInstanceLock(join(data, 'quilltap.lock'));
          } else if (a.op === 'phase2') {
            const handles: { close: () => void }[] = [];
            const open = (d: Db) => {
              const p = join(data, FILE_OF[d]);
              if (!fs.existsSync(p)) return null;
              const h = openKeyed(p);
              handles.push(h);
              return h;
            };
            const pending: Promise<unknown>[] = [];
            const mainDb = open('main')!;
            pending.push(
              physical.createPhysicalBackup(mainDb)
                .then(() => physical.applyRetentionPolicy())
                .catch((error: unknown) => {
                  logger.error('Startup physical backup or retention policy failed', {
                    error: error instanceof Error ? error.message : String(error),
                  });
                }),
            );
            const llm = open('llmLogs');
            if (llm) {
              pending.push(
                physical.createLLMLogsPhysicalBackup(llm).catch((error: unknown) => {
                  logger.error('LLM logs startup physical backup failed', {
                    error: error instanceof Error ? error.message : String(error),
                  });
                }),
              );
            }
            const mount = open('mountIndex');
            if (mount) {
              pending.push(
                physical.createMountIndexPhysicalBackup(mount).catch((error: unknown) => {
                  logger.error('Mount index startup physical backup failed', {
                    error: error instanceof Error ? error.message : String(error),
                  });
                }),
              );
            }
            await Promise.all(pending);
            await settle();
            for (const h of handles) h.close();
          } else if (a.op === 'retention') {
            await physical.applyRetentionPolicy();
          } else if (a.op === 'optimizeReadonly') {
            const ro = openKeyed(join(data, FILE_OF.main), true);
            const r = optimize.optimizeDatabase(ro, 'main');
            ro.close();
            result = {
              ok: r.ok,
              steps: r.steps.map((s) => ({ name: s.name, ok: s.ok, ...(s.error !== undefined ? { error: s.error } : {}) })),
            };
          }
          actions.push({ op: a.op, nowMs: a.at, lines: sink, result });
          sink = null;
        }

        // ---- the outcome -------------------------------------------------
        let stateAfter: string | null = null;
        if (fs.existsSync(statePath)) {
          stateAfter = fs.statSync(statePath).isDirectory() ? '<dir>' : fs.readFileSync(statePath, 'utf8');
        }
        let backupsAfter: unknown = null;
        if (fs.existsSync(backupsDir) && fs.statSync(backupsDir).isDirectory()) {
          backupsAfter = fs.readdirSync(backupsDir).sort().map((name) => {
            const p = join(backupsDir, name);
            let tables: number | null = null;
            if (!backupNames.includes(name)) {
              const db = openKeyed(p, true);
              tables = (db.prepare("SELECT count(*) AS n FROM sqlite_master WHERE type = 'table'").get() as { n: number }).n;
              db.close();
            }
            return { name, tables };
          });
        } else if (fs.existsSync(backupsDir)) {
          backupsAfter = '<file>';
        }
        const dbs: Record<string, unknown> = {};
        for (const d of spec.plant.dbs) {
          if (spec.plant.garbage === d) continue;
          const db = openKeyed(join(data, FILE_OF[d]), true);
          const pageCount = db.pragma('page_count', { simple: true });
          const freelist = db.pragma('freelist_count', { simple: true });
          const hasStat = (db.prepare("SELECT count(*) AS n FROM sqlite_master WHERE name = 'sqlite_stat1'").get() as { n: number }).n > 0;
          const stat1 = hasStat
            ? db.prepare('SELECT tbl, idx, stat FROM sqlite_stat1 ORDER BY tbl, idx').all()
            : null;
          const hasStat4 = (db.prepare("SELECT count(*) AS n FROM sqlite_master WHERE name = 'sqlite_stat4'").get() as { n: number }).n > 0;
          const stat4Rows = hasStat4
            ? (db.prepare('SELECT count(*) AS n FROM sqlite_stat4').get() as { n: number }).n
            : null;
          db.close();
          dbs[d] = { pageCount, freelist, stat1, stat4Rows };
        }

        out({
          kind: 'row',
          tz,
          name: spec.name,
          plant: {
            dbs: spec.plant.dbs,
            deleted: spec.plant.deleted,
            garbage: spec.plant.garbage ?? null,
            state: st.kind === 'dir' ? { kind: 'dir' } : stateText === null ? { kind: 'absent' } : { kind: 'text', text: stateText },
            backups: bk.kind === 'tree' ? { kind: 'tree', files: backupNames } : { kind: bk.kind },
          },
          actions,
          stateAfter,
          backupsAfter,
          dbs,
        });
        baseToken = null;
      }
    }
  } finally {
    delete process.env.QUILLTAP_DATA_DIR;
    rmSync(scratch, { recursive: true, force: true });
  }

  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
}

main().then(
  () => process.exit(0),
  (error) => {
    console.error(error);
    process.exit(1);
  },
);

