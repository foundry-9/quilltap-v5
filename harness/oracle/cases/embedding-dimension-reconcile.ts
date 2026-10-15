/**
 * Tier-2 oracle — P4.D292: v4's REAL `reconcileEmbeddingDimensions`
 * (`lib/startup/reconcile-embedding-dimensions.ts`, NARROWED by `af48f6cc2` —
 * the two vector-mirror arms and their two result keys gone) at the
 * `83334b9a5` pin.
 *
 * Every scenario is SQL run on BOTH sides over copies of `base-main.db` /
 * `base-mount.db` (minimal tables the pass reads, `background_jobs` built by
 * v4's own `ensureCollection`), then ONE pass; recorded: the result object
 * (its KEY SET is v4's narrowed one), the lines (the INFO fires only on
 * `totalMismatched > 0`, the DEBUG on a conforming corpus, the dedupe INFO, the
 * three skips' lines), and the `background_jobs` rows. Raw Float32 vectors are
 * planted as `zeroblob(4 × dims)`.
 *
 * v4's job host is held off (`__quilltapJobHost.shuttingDown`).
 *
 * Run (Node 24, from the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -rf /tmp/qt-embedding-dimension-reconcile /tmp/oracle-embedding-dimension-reconcile.ndjson
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_EDR_DIR=/tmp/qt-embedding-dimension-reconcile \
 *     $N/npx tsx $V5W/harness/oracle/cases/embedding-dimension-reconcile.ts \
 *     > /tmp/oracle-embedding-dimension-reconcile.ndjson
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

const PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const USER = 'e2920000-0000-4000-8000-0000000000aa';
const P = 'e2920000-0000-4000-8000-0000000000e2';
const T0 = '2026-01-01T00:00:00.000Z';
const id = (n: number) => `e2920000-0000-4000-8000-${String(n).padStart(12, '0')}`;
const vec = (dims: number) => `zeroblob(${4 * dims})`;

const MAIN_DDL = [
  'CREATE TABLE IF NOT EXISTS embedding_profiles (id TEXT PRIMARY KEY, userId TEXT, provider TEXT, dimensions REAL, truncateToDimensions REAL, isDefault INTEGER)',
  'CREATE TABLE IF NOT EXISTS memories (id TEXT PRIMARY KEY, characterId TEXT, embedding BLOB)',
  'CREATE TABLE IF NOT EXISTS chats (id TEXT PRIMARY KEY)',
  'CREATE TABLE IF NOT EXISTS conversation_chunks (id TEXT PRIMARY KEY, chatId TEXT, embedding BLOB)',
  'CREATE TABLE IF NOT EXISTS help_docs (id TEXT PRIMARY KEY, embedding BLOB)',
  'CREATE TABLE IF NOT EXISTS embedding_status (id TEXT PRIMARY KEY, entityType TEXT, entityId TEXT, profileId TEXT, status TEXT)',
];
const MOUNT_DDL = [
  'CREATE TABLE IF NOT EXISTS doc_mount_points (id TEXT PRIMARY KEY, enabled INTEGER)',
  'CREATE TABLE IF NOT EXISTS doc_mount_chunks (id TEXT PRIMARY KEY, mountPointId TEXT, embedding BLOB)',
];

const RESET_MAIN = ['memories', 'chats', 'conversation_chunks', 'help_docs', 'embedding_status', 'embedding_profiles', 'background_jobs'].map(
  (t) => `DELETE FROM "${t}"`,
);
const RESET_MOUNT = ['DELETE FROM "doc_mount_points"', 'DELETE FROM "doc_mount_chunks"'];

function profile(provider: string, dimensions: number | null, truncate: number | null = null): string {
  return `INSERT INTO embedding_profiles VALUES ('${P}', '${USER}', '${provider}', ${dimensions ?? 'NULL'}, ${truncate ?? 'NULL'}, 1)`;
}
const CONFORMING = [
  `INSERT INTO memories VALUES ('${id(1)}', 'c1', ${vec(4)})`,
  `INSERT INTO chats VALUES ('${id(30)}')`,
  `INSERT INTO conversation_chunks VALUES ('${id(31)}', '${id(30)}', ${vec(4)})`,
  `INSERT INTO conversation_chunks VALUES ('${id(32)}', '${id(30)}', NULL)`,
  `INSERT INTO help_docs VALUES ('${id(40)}', ${vec(4)})`,
];
const MIXED = [
  `INSERT INTO memories VALUES ('${id(1)}', 'c1', ${vec(4)})`,
  `INSERT INTO memories VALUES ('${id(2)}', 'c1', ${vec(8)})`,
  `INSERT INTO memories VALUES ('${id(3)}', 'c1', NULL)`,
  `INSERT INTO memories VALUES ('${id(4)}', 'c1', ${vec(8)})`,
  `INSERT INTO embedding_status VALUES ('${id(90)}', 'MEMORY', '${id(4)}', '${P}', 'FAILED')`,
  `INSERT INTO memories VALUES ('${id(5)}', NULL, ${vec(8)})`,
  `INSERT INTO chats VALUES ('${id(30)}')`,
  `INSERT INTO conversation_chunks VALUES ('${id(31)}', '${id(30)}', ${vec(8)})`,
  `INSERT INTO conversation_chunks VALUES ('${id(32)}', '${id(30)}', NULL)`,
  `INSERT INTO conversation_chunks VALUES ('${id(33)}', '${id(39)}', ${vec(8)})`,
  `INSERT INTO help_docs VALUES ('${id(40)}', ${vec(8)})`,
  `INSERT INTO help_docs VALUES ('${id(41)}', NULL)`,
];
const MOUNT_MIXED = [
  `INSERT INTO doc_mount_points VALUES ('${id(60)}', 1)`,
  `INSERT INTO doc_mount_points VALUES ('${id(61)}', 0)`,
  `INSERT INTO doc_mount_chunks VALUES ('${id(62)}', '${id(60)}', ${vec(8)})`,
  `INSERT INTO doc_mount_chunks VALUES ('${id(63)}', '${id(61)}', ${vec(8)})`,
  `INSERT INTO doc_mount_chunks VALUES ('${id(64)}', '${id(60)}', ${vec(4)})`,
];
const QUEUED = `INSERT INTO background_jobs ("id","userId","type","status","payload","priority","attempts","maxAttempts","scheduledAt","createdAt","updatedAt") VALUES ('${id(80)}', '${USER}', 'EMBEDDING_REINDEX_ALL', 'PENDING', '{}', -1, 0, 3, '${T0}', '${T0}', '${T0}')`;

const SCENARIOS: Array<{ name: string; main: string[]; mount: string[] }> = [
  { name: 'clean_corpus_debug_only', main: [profile('OPENAI', 4), ...CONFORMING], mount: [] },
  { name: 'mixed_corpus_info_and_enqueue', main: [profile('OPENAI', 4), ...MIXED], mount: MOUNT_MIXED },
  { name: 'truncate_wins_over_dimensions', main: [profile('OPENAI', 8, 4), ...MIXED], mount: [] },
  { name: 'already_queued_deduped', main: [profile('OPENAI', 4), ...MIXED, QUEUED], mount: [] },
  { name: 'builtin_default_skipped', main: [profile('BUILTIN', null), ...MIXED], mount: [] },
  { name: 'no_default_skipped', main: [...MIXED], mount: [] },
  { name: 'no_fixed_dim_warns', main: [profile('OLLAMA', null), ...MIXED], mount: [] },
  { name: 'tables_absent_count_zero', main: [profile('OPENAI', 4), 'DROP TABLE help_docs', 'DROP TABLE chats'], mount: [] },
];
const RESTORE = ['CREATE TABLE IF NOT EXISTS help_docs (id TEXT PRIMARY KEY, embedding BLOB)', 'CREATE TABLE IF NOT EXISTS chats (id TEXT PRIMARY KEY)'];

async function main(): Promise<void> {
  const dir = process.env.QT_FIXTURE_EDR_DIR;
  if (!dir) throw new Error('QT_FIXTURE_EDR_DIR must name the dir for the base DBs');
  mkdirSync(dir, { recursive: true });
  const workMain = join(dir, 'work-main.db');
  const workMount = join(dir, 'work-mount.db');
  for (const p of [workMain, workMount, join(dir, 'base-main.db'), join(dir, 'base-mount.db')]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) if (existsSync(p + suffix)) rmSync(p + suffix);
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-edr-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  const CAPTURED = new Set([
    'Embedding dimension reconcile found non-conforming vectors',
    'Embedding dimension reconcile: corpus conforms',
    'Embedding dimension reconcile failed',
    'Default embedding profile has no fixed dimension; cannot enforce conformance',
    'Mount-chunk dimension check skipped',
    'Mismatched-dim reindex already queued; not enqueueing another',
  ]);
  const { logger } = await import('@/lib/logger');
  let captured: Array<Record<string, unknown>> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      if (!CAPTURED.has(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      captured.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
    },
  });

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { BackgroundJobSchema } = await import('@/lib/schemas/types');
  const ensure = async () => {
    await initializeDatabase();
    await ensureCollection('background_jobs', BackgroundJobSchema);
    for (const s of MAIN_DDL) await rawQuery(s);
    const mount = getRawMountIndexDatabase();
    if (!mount) throw new Error('mount-index DB handle unavailable');
    for (const s of MOUNT_DDL) mount.exec(s);
  };
  await ensure();
  closeMountIndexSQLiteClient();
  await closeDatabase();
  copyFileSync(workMain, join(dir, 'base-main.db'));
  copyFileSync(workMount, join(dir, 'base-mount.db'));
  await ensure();

  const { reconcileEmbeddingDimensions } = await import('@/lib/startup/reconcile-embedding-dimensions');
  for (const sc of SCENARIOS) {
    for (const s of RESTORE) await rawQuery(s);
    for (const s of RESET_MAIN) await rawQuery(s);
    const mount = getRawMountIndexDatabase()!;
    for (const s of RESET_MOUNT) mount.exec(s);
    for (const s of sc.main) await rawQuery(s);
    for (const s of sc.mount) mount.exec(s);
    captured = [];
    const result = await reconcileEmbeddingDimensions();
    const jobs = (await rawQuery(
      `SELECT "id","userId","type","status","payload","priority","attempts","maxAttempts","lastError" FROM "background_jobs" ORDER BY "createdAt", "id"`,
    )) as unknown[];
    process.stdout.write(
      JSON.stringify({
        kind: 'scenario',
        name: sc.name,
        main: [...RESET_MAIN, ...sc.main],
        mount: [...RESET_MOUNT, ...sc.mount],
        result,
        lines: captured,
        jobs,
      }) + '\n',
    );
  }
  closeMountIndexSQLiteClient();
  await closeDatabase();
}

main().then(
  () => process.exit(0),
  (e) => {
    process.stderr.write(`embedding-dimension-reconcile oracle failed: ${e instanceof Error ? e.stack : String(e)}\n`);
    process.exit(1);
  },
);
