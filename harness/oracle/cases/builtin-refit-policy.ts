/**
 * Tier-1 + tier-2 oracle — P4.D292: v4's REAL `builtin-refit-policy.ts`
 * (`4ae0c4ca6`, at the `83334b9a5` pin) and the REAL `scheduleRefit` debounce
 * of `embedding-job-scheduler.ts`.
 *
 * (a) TIER 1 — `evaluateRefitNeed` / `driftThreshold` over a grid: `fittedAt`
 *     null / `''` / set; `textShapeVersion` null / 0 / 0.5 / 1 / 2; totals at
 *     the drift bar ±1.
 * (b) TIER 2 — `checkBuiltinRefit` / `enqueueRefitIfDue` over planted DBs.
 *     Every scenario is a list of SQL statements run on BOTH sides (each
 *     starts by clearing the four tables and re-creating `tfidf_vocabularies`
 *     in one of v4's two shapes — generateDDL's `REAL DEFAULT 0` in schema
 *     order, or the migration's `INTEGER DEFAULT 0` appended — R-R), then ONE
 *     op through v4's real code; recorded: the result object, the captured
 *     lines, and the `background_jobs` rows the op left.
 * (c) TIER 2 — the debounce: `globalThis.setTimeout` / `clearTimeout` are
 *     replaced by a manual virtual clock BEFORE the scheduler is imported (the
 *     module reads the globals at call time — jest's fake timers do the same
 *     thing), so each scenario is a list of `schedule` calls at virtual times
 *     and `advance` checkpoints; v5 replays it through `schedule_refit` /
 *     `fire_due_refits` with the same virtual `now_ms`.
 *
 * v4's job host is held off (`__quilltapJobHost.shuttingDown`) — the policy's
 * `enqueueEmbeddingRefit` would otherwise spawn it and claim the row.
 *
 * Run (Node 24, from the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -rf /tmp/qt-builtin-refit-policy /tmp/oracle-builtin-refit-policy.ndjson
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_BUILTIN_REFIT_DIR=/tmp/qt-builtin-refit-policy \
 *     $N/npx tsx $V5W/harness/oracle/cases/builtin-refit-policy.ts \
 *     > /tmp/oracle-builtin-refit-policy.ndjson
 */

import { copyFileSync, existsSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

(globalThis as Record<string, unknown>).__quilltapJobHost = {
  child: null,
  spawning: false,
  shuttingDown: true,
  childCrashed: false,
  restartTimestamps: [],
  signalHandlersInstalled: false,
  invalidationListeners: new Set(),
};

// ---- (c)'s virtual clock, installed before any module reads the globals ----
let virtualNow = 0;
let nextTimer = 1;
const timers = new Map<number, { at: number; fn: () => unknown }>();
const realSetTimeout = globalThis.setTimeout;
(globalThis as Record<string, unknown>).setTimeout = ((fn: () => unknown, ms: number) => {
  const id = nextTimer++;
  timers.set(id, { at: virtualNow + (ms ?? 0), fn });
  return id as unknown as NodeJS.Timeout;
}) as unknown;
(globalThis as Record<string, unknown>).clearTimeout = ((id: number) => {
  timers.delete(id);
}) as unknown;
/** Fire every timer due at or before `to` (in due order), awaiting each. */
async function advanceTo(to: number): Promise<number> {
  let fired = 0;
  for (;;) {
    const due = [...timers.entries()].filter(([, t]) => t.at <= to).sort((a, b) => a[1].at - b[1].at || a[0] - b[0]);
    if (due.length === 0) break;
    const [id, t] = due[0];
    timers.delete(id);
    virtualNow = t.at;
    await t.fn();
    fired++;
  }
  virtualNow = to;
  return fired;
}

const PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const USER = 'e2920000-0000-4000-8000-0000000000aa';
const OTHER_USER = 'e2920000-0000-4000-8000-0000000000ab';
const CHAR = 'e2920000-0000-4000-8000-0000000000c1';
const CHAR2 = 'e2920000-0000-4000-8000-0000000000c2';
const P_TF = 'e2920000-0000-4000-8000-0000000000e1';
const P_OA = 'e2920000-0000-4000-8000-0000000000e2';
const T0 = '2026-01-01T00:00:00.000Z';
const FITTED = '2026-06-01T00:00:00.000Z';
const OLD = '2026-02-01T00:00:00.000Z';
const NEW = '2026-07-01T00:00:00.000Z';

const TV_REAL = `CREATE TABLE "tfidf_vocabularies" (
  "id" TEXT PRIMARY KEY NOT NULL,
  "profileId" TEXT NOT NULL,
  "userId" TEXT NOT NULL,
  "vocabulary" TEXT NOT NULL,
  "idf" TEXT NOT NULL,
  "avgDocLength" REAL NOT NULL,
  "vocabularySize" REAL NOT NULL,
  "includeBigrams" INTEGER DEFAULT 1,
  "fittedAt" TEXT NOT NULL,
  "textShapeVersion" REAL DEFAULT 0,
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL
)`;
const TV_PRE = `CREATE TABLE "tfidf_vocabularies" (
  "id" TEXT PRIMARY KEY NOT NULL,
  "profileId" TEXT NOT NULL,
  "userId" TEXT NOT NULL,
  "vocabulary" TEXT NOT NULL,
  "idf" TEXT NOT NULL,
  "avgDocLength" REAL NOT NULL,
  "vocabularySize" REAL NOT NULL,
  "includeBigrams" INTEGER DEFAULT 1,
  "fittedAt" TEXT NOT NULL,
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL
)`;
const TV_INTEGER = [TV_PRE, `ALTER TABLE "tfidf_vocabularies" ADD COLUMN "textShapeVersion" INTEGER DEFAULT 0`];

function reset(shape: 'real' | 'integer'): string[] {
  return [
    'DELETE FROM "memories"',
    'DELETE FROM "background_jobs"',
    'DELETE FROM "embedding_profiles"',
    'DROP TABLE IF EXISTS "tfidf_vocabularies"',
    ...(shape === 'real' ? [TV_REAL] : TV_INTEGER),
  ];
}
function profile(id: string, provider: string, isDefault: boolean, userId = USER): string {
  return `INSERT INTO "embedding_profiles" ("id","userId","name","provider","modelName","isDefault","createdAt","updatedAt") VALUES ('${id}','${userId}','${provider} profile','${provider}','m',${isDefault ? 1 : 0},'${T0}','${T0}')`;
}
/** A vocabulary row; `version` undefined = the column's DEFAULT (the INSERT does not name it). */
function vocabulary(profileId: string, fittedAt: string, version?: number): string {
  const cols = `"id","profileId","userId","vocabulary","idf","avgDocLength","vocabularySize","includeBigrams","fittedAt"${version === undefined ? '' : ',"textShapeVersion"'},"createdAt","updatedAt"`;
  const vals = `'e2920000-0000-4000-8000-0000000000d1','${profileId}','${USER}','[]','[]',1.5,0,1,'${fittedAt}'${version === undefined ? '' : `,${version}`},'${T0}','${T0}'`;
  return `INSERT INTO "tfidf_vocabularies" (${cols}) VALUES (${vals})`;
}
/** `count` memories for `characterId` created at `createdAt`, ids from `base`. */
function memories(characterId: string, count: number, createdAt: string, base: number): string {
  return `INSERT INTO "memories" ("id","characterId","content","summary","createdAt","updatedAt")
    SELECT printf('e2920000-0000-4000-8000-%012d', n + ${base}), '${characterId}', 'content ' || n, 'summary ' || n, '${createdAt}', '${createdAt}'
    FROM (WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM c WHERE n < ${count}) SELECT n FROM c)`;
}
function job(id: string, type: string, status: string): string {
  return `INSERT INTO "background_jobs" ("id","userId","type","status","payload","priority","attempts","maxAttempts","scheduledAt","createdAt","updatedAt") VALUES ('${id}','${USER}','${type}','${status}','{"profileId":"${P_TF}"}',-1,0,3,'${T0}','${T0}','${T0}')`;
}

interface Scenario {
  name: string;
  sql: string[];
  op: { kind: 'check' } | { kind: 'enqueue'; profileId: string; userId: string; force: boolean; source: string };
}

const BUILTIN_DEFAULT = profile(P_TF, 'BUILTIN', true);
const SCENARIOS: Scenario[] = [
  { name: 'no_default_profile', sql: [...reset('real'), profile(P_TF, 'BUILTIN', false)], op: { kind: 'check' } },
  { name: 'neural_default', sql: [...reset('real'), profile(P_OA, 'OPENAI', true), profile(P_TF, 'BUILTIN', false)], op: { kind: 'check' } },
  { name: 'unfitted_enqueues', sql: [...reset('real'), BUILTIN_DEFAULT, memories(CHAR, 3, OLD, 0)], op: { kind: 'check' } },
  { name: 'unfitted_empty_fitted_at', sql: [...reset('real'), BUILTIN_DEFAULT, vocabulary(P_TF, '', 1), memories(CHAR, 3, OLD, 0)], op: { kind: 'check' } },
  { name: 'unfitted_nothing_to_fit', sql: [...reset('real'), BUILTIN_DEFAULT], op: { kind: 'check' } },
  { name: 'text_shape_real_default', sql: [...reset('real'), BUILTIN_DEFAULT, vocabulary(P_TF, FITTED), memories(CHAR, 3, OLD, 0)], op: { kind: 'check' } },
  { name: 'text_shape_integer_default', sql: [...reset('integer'), BUILTIN_DEFAULT, vocabulary(P_TF, FITTED), memories(CHAR, 3, OLD, 0)], op: { kind: 'check' } },
  { name: 'current_integer_one', sql: [...reset('integer'), BUILTIN_DEFAULT, vocabulary(P_TF, FITTED, 1), memories(CHAR, 3, OLD, 0)], op: { kind: 'check' } },
  { name: 'current_real_one', sql: [...reset('real'), BUILTIN_DEFAULT, vocabulary(P_TF, FITTED, 1), memories(CHAR, 3, OLD, 0)], op: { kind: 'check' } },
  {
    name: 'drift_exactly_at_the_bar',
    sql: [...reset('real'), BUILTIN_DEFAULT, vocabulary(P_TF, FITTED, 1), memories(CHAR, 225, OLD, 0), memories(CHAR2, 25, NEW, 1000)],
    op: { kind: 'check' },
  },
  {
    name: 'drift_one_below_the_bar',
    sql: [...reset('real'), BUILTIN_DEFAULT, vocabulary(P_TF, FITTED, 1), memories(CHAR, 226, OLD, 0), memories(CHAR2, 24, NEW, 1000)],
    op: { kind: 'check' },
  },
  {
    name: 'drift_new_at_fitted_at_counts',
    sql: [...reset('real'), BUILTIN_DEFAULT, vocabulary(P_TF, FITTED, 1), memories(CHAR, 25, FITTED, 0)],
    op: { kind: 'check' },
  },
  {
    name: 'held_by_pending_refit',
    sql: [...reset('real'), BUILTIN_DEFAULT, memories(CHAR, 3, OLD, 0), job('e2920000-0000-4000-8000-0000000000f1', 'EMBEDDING_REFIT', 'PENDING')],
    op: { kind: 'check' },
  },
  {
    name: 'held_by_processing_reindex_unforced',
    sql: [...reset('real'), BUILTIN_DEFAULT, memories(CHAR, 3, OLD, 0), job('e2920000-0000-4000-8000-0000000000f2', 'EMBEDDING_REINDEX_ALL', 'PROCESSING')],
    op: { kind: 'enqueue', profileId: P_TF, userId: USER, force: false, source: 'scheduleRefit' },
  },
  {
    name: 'forced_past_processing_reindex',
    sql: [...reset('real'), BUILTIN_DEFAULT, vocabulary(P_TF, FITTED, 1), memories(CHAR, 3, OLD, 0), job('e2920000-0000-4000-8000-0000000000f2', 'EMBEDDING_REINDEX_ALL', 'PROCESSING')],
    op: { kind: 'enqueue', profileId: P_TF, userId: USER, force: true, source: 'scheduleRefit' },
  },
  {
    name: 'forced_held_by_pending_reindex',
    sql: [
      ...reset('real'),
      BUILTIN_DEFAULT,
      vocabulary(P_TF, FITTED, 1),
      job('e2920000-0000-4000-8000-0000000000f2', 'EMBEDDING_REINDEX_ALL', 'PROCESSING'),
      job('e2920000-0000-4000-8000-0000000000f3', 'EMBEDDING_REINDEX_ALL', 'PENDING'),
    ],
    op: { kind: 'enqueue', profileId: P_TF, userId: USER, force: true, source: 'scheduleRefit' },
  },
  {
    name: 'forced_on_an_empty_corpus',
    sql: [...reset('real'), BUILTIN_DEFAULT, vocabulary(P_TF, FITTED, 1)],
    op: { kind: 'enqueue', profileId: P_TF, userId: USER, force: true, source: 'scheduleRefit' },
  },
  {
    name: 'current_unforced_enqueue',
    sql: [...reset('real'), BUILTIN_DEFAULT, vocabulary(P_TF, FITTED, 1), memories(CHAR, 3, OLD, 0)],
    op: { kind: 'enqueue', profileId: P_TF, userId: USER, force: false, source: 'scheduleRefit' },
  },
  {
    name: 'instance_default_owned_by_another_user',
    sql: [...reset('real'), profile(P_TF, 'BUILTIN', true, OTHER_USER), memories(CHAR, 3, OLD, 0)],
    op: { kind: 'check' },
  },
  {
    name: 'vocabulary_read_fails',
    sql: [...reset('real'), BUILTIN_DEFAULT, memories(CHAR, 3, OLD, 0), 'DROP TABLE "tfidf_vocabularies"'],
    op: { kind: 'check' },
  },
  // The three repository reads' own ERRORs (`Error counting the memory corpus`,
  // `Error finding active background jobs by type`, `Error finding the instance
  // default embedding profile`) are reached only inside the strict scope: the
  // base FALLBACKS under them answer first. Each row below fails the inner read
  // and pins the outer line SILENT (and, for the jobs table, the check's WARN).
  {
    name: 'memories_read_fails',
    sql: [...reset('real'), BUILTIN_DEFAULT, vocabulary(P_TF, FITTED, 1), 'ALTER TABLE "memories" RENAME TO "memories_gone"'],
    op: { kind: 'check' },
  },
  {
    name: 'jobs_read_fails',
    sql: [...reset('real'), BUILTIN_DEFAULT, memories(CHAR, 3, OLD, 0), 'ALTER TABLE "background_jobs" RENAME TO "background_jobs_gone"'],
    op: { kind: 'check' },
  },
  {
    name: 'profiles_read_fails',
    sql: [...reset('real'), BUILTIN_DEFAULT, memories(CHAR, 3, OLD, 0), 'ALTER TABLE "embedding_profiles" RENAME TO "embedding_profiles_gone"'],
    op: { kind: 'check' },
  },
];

/** Each scenario's renamed table is put back before the next one's reset. */
const RESTORE = [
  'ALTER TABLE "memories_gone" RENAME TO "memories"',
  'ALTER TABLE "background_jobs_gone" RENAME TO "background_jobs"',
  'ALTER TABLE "embedding_profiles_gone" RENAME TO "embedding_profiles"',
];

async function main(): Promise<void> {
  const dir = process.env.QT_FIXTURE_BUILTIN_REFIT_DIR;
  if (!dir) throw new Error('QT_FIXTURE_BUILTIN_REFIT_DIR must name the dir for base.db');
  mkdirSync(dir, { recursive: true });
  const work = join(dir, 'work.db');
  for (const p of [work, join(dir, 'base.db')]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) if (existsSync(p + suffix)) rmSync(p + suffix);
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-builtin-refit-policy-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  const CAPTURED = new Set([
    'Assessed the TF-IDF vocabulary',
    'TF-IDF refit due but there are no memories to fit',
    'TF-IDF refit due but one is already in hand; not enqueueing another',
    'TF-IDF vocabulary refit enqueued',
    'TF-IDF refit check failed',
    '[EmbeddingScheduler] Refit check scheduled (debounced)',
    '[EmbeddingScheduler] Refit not needed; the vocabulary is current',
    '[EmbeddingScheduler] Refit job scheduled (debounced)',
    '[EmbeddingScheduler] Failed to schedule refit job',
    'Error finding TF-IDF vocabulary by profile ID',
    'Error counting the memory corpus',
    'Error counting entities',
    'Error finding active background jobs by type',
    'Error finding the instance default embedding profile',
    'Error finding entities by filter',
    'Error finding entity by filter',
  ]);
  const { logger } = await import('@/lib/logger');
  let captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown>; error?: { message?: string } };
      if (!CAPTURED.has(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      if (data.error?.message !== undefined) fields.error = data.error.message;
      captured.push({ level: data.level, message: data.message, fields });
    },
  });

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { MemorySchema, BackgroundJobSchema, EmbeddingProfileSchema, TfidfVocabularySchema } = await import('@/lib/schemas/types');
  const ensure = async () => {
    await initializeDatabase();
    await ensureCollection('memories', MemorySchema);
    await ensureCollection('background_jobs', BackgroundJobSchema);
    await ensureCollection('embedding_profiles', EmbeddingProfileSchema);
    await ensureCollection('tfidf_vocabularies', TfidfVocabularySchema);
  };
  await ensure();
  await closeDatabase();
  copyFileSync(work, join(dir, 'base.db'));
  await ensure();

  const policy = await import('@/lib/embedding/builtin-refit-policy');
  const { scheduleRefit } = await import('@/lib/embedding/embedding-job-scheduler');

  // A scenario that renamed the jobs table dumps `[]` (it has none to show).
  const jobsDump = async () => {
    try {
      return (await rawQuery(
        `SELECT "id","userId","type","status","payload","priority","attempts","maxAttempts","lastError","startedAt","completedAt" FROM "background_jobs" ORDER BY "createdAt", "id"`,
      )) as Array<Record<string, unknown>>;
    } catch {
      return [];
    }
  };
  const plant = async (sql: string[]) => {
    for (const s of sql) await rawQuery(s);
  };

  // ---- (a) tier 1 ----
  const tier1: Array<Record<string, unknown>> = [];
  for (const fittedAt of [null, '', FITTED]) {
    for (const textShapeVersion of [null, 0, 0.5, 1, 2]) {
      for (const totalMemories of [0, 24, 25, 249, 250, 251, 10000]) {
        const bar = policy.driftThreshold(totalMemories);
        for (const newSinceFit of [bar - 1, bar, bar + 1]) {
          const input = { fittedAt, textShapeVersion, newSinceFit, totalMemories };
          tier1.push({ input, threshold: bar, reason: policy.evaluateRefitNeed(input) });
        }
      }
    }
  }
  process.stdout.write(JSON.stringify({ kind: 'tier1', rows: tier1 }) + '\n');

  // ---- (b) tier 2 ----
  const restore = async () => {
    for (const r of RESTORE) {
      try {
        await rawQuery(r);
      } catch {
        // not renamed by the last scenario
      }
    }
  };
  for (const sc of SCENARIOS) {
    await restore();
    await plant(sc.sql);
    captured = [];
    const op = sc.op;
    const result =
      op.kind === 'check'
        ? await policy.checkBuiltinRefit()
        : await policy.enqueueRefitIfDue({ id: op.profileId, userId: op.userId }, { force: op.force, source: op.source });
    process.stdout.write(
      JSON.stringify({ kind: 'scenario', name: sc.name, sql: sc.sql, op, result, lines: captured, jobs: await jobsDump() }) + '\n',
    );
  }

  // ---- (d) the DATA tick's step 10 through v4's REAL runScheduledMaintenance ----
  // Only `builtinRefit` and the refit job are compared: the other sweeps meet
  // tables this case never builds (their failures are each side's own).
  const { runScheduledMaintenance } = await import('@/lib/background-jobs/scheduled-maintenance');
  for (const [name, sql] of [
    ['maintenance_builtin_drift', [...reset('real'), BUILTIN_DEFAULT, vocabulary(P_TF, FITTED, 1), memories(CHAR, 30, NEW, 0)]],
    ['maintenance_neural_default', [...reset('real'), profile(P_OA, 'OPENAI', true), memories(CHAR, 30, NEW, 0)]],
  ] as Array<[string, string[]]>) {
    await restore();
    await plant(sql);
    captured = [];
    const summary = await runScheduledMaintenance();
    const refitJobs = (await jobsDump()).filter((j) => j.type === 'EMBEDDING_REFIT');
    process.stdout.write(
      JSON.stringify({
        kind: 'maintenance',
        name,
        sql,
        builtinRefit: summary.builtinRefit,
        lines: captured,
        jobs: refitJobs,
      }) + '\n',
    );
  }

  // ---- (c) the debounce ----
  type Step = { at: number; schedule: { userId: string; profileId?: string; force?: boolean } } | { at: number; advance: true };
  const DEBOUNCE: Array<{ name: string; sql: string[]; steps: Step[] }> = [
    {
      name: 'three_calls_collapse_to_one_check',
      sql: [...reset('real'), BUILTIN_DEFAULT, vocabulary(P_TF, FITTED, 1), memories(CHAR, 3, OLD, 0)],
      steps: [
        { at: 0, schedule: { userId: USER } },
        { at: 2000, schedule: { userId: USER } },
        { at: 4000, schedule: { userId: USER } },
        { at: 8999, advance: true },
        { at: 9000, advance: true },
        { at: 20000, advance: true },
      ],
    },
    {
      name: 'forced_then_unforced_stays_forced',
      sql: [...reset('real'), BUILTIN_DEFAULT, vocabulary(P_TF, FITTED, 1), memories(CHAR, 3, OLD, 0)],
      steps: [
        { at: 0, schedule: { userId: USER, profileId: P_TF, force: true } },
        { at: 1000, schedule: { userId: USER, profileId: P_TF } },
        { at: 5999, advance: true },
        { at: 6000, advance: true },
      ],
    },
    {
      name: 'drift_refits_once',
      sql: [...reset('real'), BUILTIN_DEFAULT, vocabulary(P_TF, FITTED, 1), memories(CHAR, 30, NEW, 0)],
      steps: [
        { at: 0, schedule: { userId: USER } },
        { at: 5000, advance: true },
      ],
    },
    {
      name: 'due_but_held_logs_nothing_from_the_scheduler',
      sql: [...reset('real'), BUILTIN_DEFAULT, memories(CHAR, 3, OLD, 0), job('e2920000-0000-4000-8000-0000000000f1', 'EMBEDDING_REFIT', 'PENDING')],
      steps: [
        { at: 0, schedule: { userId: USER } },
        { at: 5000, advance: true },
      ],
    },
    {
      name: 'neural_and_missing_profiles_schedule_nothing',
      sql: [...reset('real'), profile(P_OA, 'OPENAI', true), profile(P_TF, 'BUILTIN', false)],
      steps: [
        { at: 0, schedule: { userId: USER, force: true } },
        { at: 0, schedule: { userId: USER, profileId: 'e2920000-0000-4000-8000-0000000000ee', force: true } },
        { at: 0, schedule: { userId: OTHER_USER } },
        { at: 10000, advance: true },
      ],
    },
    {
      name: 'explicit_builtin_profile_while_the_default_is_neural',
      sql: [...reset('real'), profile(P_OA, 'OPENAI', true), profile(P_TF, 'BUILTIN', false), memories(CHAR, 3, OLD, 0)],
      steps: [
        { at: 0, schedule: { userId: USER, profileId: P_TF } },
        { at: 5000, advance: true },
      ],
    },
  ];
  for (const sc of DEBOUNCE) {
    timers.clear();
    virtualNow = 0;
    await restore();
    await plant(sc.sql);
    const steps: Array<Record<string, unknown>> = [];
    for (const step of sc.steps) {
      captured = [];
      if ('schedule' in step) {
        virtualNow = step.at;
        await scheduleRefit(step.schedule.userId, step.schedule.profileId, step.schedule.force ? { force: true } : {});
        steps.push({ ...step, pending: timers.size, lines: captured });
      } else {
        const fired = await advanceTo(step.at);
        steps.push({ ...step, fired, pending: timers.size, lines: captured });
      }
    }
    process.stdout.write(JSON.stringify({ kind: 'debounce', name: sc.name, sql: sc.sql, steps, jobs: await jobsDump() }) + '\n');
  }

  await closeDatabase();
  (globalThis as Record<string, unknown>).setTimeout = realSetTimeout;
}

main().then(
  () => process.exit(0),
  (e) => {
    process.stderr.write(`builtin-refit-policy oracle failed: ${e instanceof Error ? e.stack : String(e)}\n`);
    process.exit(1);
  },
);
