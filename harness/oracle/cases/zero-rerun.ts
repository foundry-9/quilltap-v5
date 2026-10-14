/**
 * Oracle case — P4.D288 unit 13 (R-E's stamp rule, §R.13(a)): after v5's
 * WHOLE span chain, v4's REAL `MigrationRunner` at the `83334b9a5` pin,
 * narrowed to the twelve span migrations, must re-run NOTHING.
 *
 * Two phases (`QT_ZERO_RERUN_PHASE`), around a Rust write step:
 *
 *   build — writes `$QT_FIXTURE_ZERO_RERUN_DIR/planted.db`: a PLANTED pre-round
 *           instance (never a real-instance file) — v4's generateDDL dump
 *           (`fresh_schema.json`) + the migration index family
 *           (`migration_indexes.json`) with the round's moves UNDONE: no
 *           `memories.supersededAt`, no `chats` scenario pair, the OTHER
 *           watermark column, no cursor table, no `idx_memories_supersededById`,
 *           no `tfidf_vocabularies.textShapeVersion`, the three dead
 *           `chat_settings` columns + the dead instance key, the vector mirror
 *           (rows a memory lacks and holds), retired job rows, legacy
 *           UTC-midnight `occurredAt`s and stale `reinforcedImportance`s.
 *           Then a COPY migrated by v4's runner over the twelve →
 *           `v4-migrated.db` (the third mode), and the ledger rows v4 wrote
 *           on it are emitted (`{phase: 'build', ledger}`) — the bytes each v5
 *           stamp must equal.
 *   v4    — for each `<mode>.after-v5.db` the Rust write step left in the dir
 *           (`planted`, `fresh`, `fresh-used`, `v4-migrated`, and each
 *           stamped base's `-unstamped` twin — the same instance after v5's
 *           chain with the span's ledger rows DELETED: which ids the stamps
 *           hold back, and the bytes v4 writes for each): dump the state, run v4's
 *           narrowed runner on a COPY, dump again; emit `{phase: 'v4', mode,
 *           ran, before, after}` (`ran` = the ids v4 ran — `+RAN` in its
 *           result; the dumps = `sqlite_master` minus the two ledger tables +
 *           every touched table through `quote()`, and the ledger rows).
 *
 * Recipe (Node 24, from the round's pin; `TZ=UTC QUILLTAP_TIMEZONE=UTC`):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   D=/tmp/qt-zero-rerun ; rm -rf $D /tmp/oracle-zero-rerun-*.ndjson
 *   cd ~/source/quilltap-server
 *   TZ=UTC QUILLTAP_TIMEZONE=UTC QT_ZERO_RERUN_PHASE=build QT_FIXTURE_ZERO_RERUN_DIR=$D \
 *     $N/npx tsx $V5W/harness/oracle/cases/zero-rerun.ts > /tmp/oracle-zero-rerun-build.ndjson
 *   cd $V5W && QT_ZERO_RERUN_WRITE=1 QT_FIXTURE_ZERO_RERUN_DIR=$D \
 *     cargo test -p quilltap-harness --test zero_migrations_rerun_equivalence -- write_after_v5 --nocapture
 *   cd ~/source/quilltap-server
 *   TZ=UTC QUILLTAP_TIMEZONE=UTC QT_ZERO_RERUN_PHASE=v4 QT_FIXTURE_ZERO_RERUN_DIR=$D \
 *     $N/npx tsx $V5W/harness/oracle/cases/zero-rerun.ts > /tmp/oracle-zero-rerun-v4.ndjson
 *   cd $V5W && QT_ORACLE_ZERO_RERUN_DIR=/tmp QT_FIXTURE_ZERO_RERUN_DIR=$D \
 *     cargo test -p quilltap-harness --test zero_migrations_rerun_equivalence -- --nocapture
 */

import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { P4D288_SPAN_MIGRATIONS } from '../lib/v4-migrations';

const PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const KEY_PRAGMA = `key = "x'${Buffer.from(PEPPER, 'base64').toString('hex')}'"`;
const T0 = '2026-01-01T00:00:00.000Z';
const TOUCHED = [
  'memories',
  'chats',
  'chat_settings',
  'instance_settings',
  'background_jobs',
  'tfidf_vocabularies',
  'memory_extraction_cursors',
  'vector_entries',
  'vector_indices',
];

const Database = createRequire(process.cwd() + '/')('better-sqlite3');

function sweep(path: string): void {
  for (const suffix of ['', '-journal', '-wal', '-shm']) if (existsSync(path + suffix)) rmSync(path + suffix);
}

function strip(ddl: string, lines: string[]): string {
  let out = ddl;
  for (const line of lines) {
    if (out.split(line).length !== 2) throw new Error(`expected exactly one ${JSON.stringify(line)} in:\n${ddl}`);
    out = out.replace(line, '');
  }
  return out;
}

function inspect(path: string): Record<string, unknown> {
  const db = new Database(path, { readonly: true });
  db.pragma(KEY_PRAGMA);
  const master = db
    .prepare(
      `SELECT type, name, tbl_name, sql FROM sqlite_master
        WHERE name NOT IN ('migrations_state', 'migrations_metadata')
          AND tbl_name NOT IN ('migrations_state', 'migrations_metadata')
        ORDER BY type, name`,
    )
    .all();
  const tables: Record<string, unknown> = {};
  for (const t of TOUCHED) {
    if (!db.prepare(`SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?`).get(t)) {
      tables[t] = null;
      continue;
    }
    const info = db.prepare(`PRAGMA table_info("${t}")`).all() as Array<{ name: string }>;
    const cols = info.map((c) => `quote("${c.name}")`).join(', ');
    tables[t] = db.prepare(`SELECT ${cols} FROM "${t}" ORDER BY rowid`).raw().all();
  }
  const hasLedger = !!db.prepare(`SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'migrations_state'`).get();
  const ledger = hasLedger ? db.prepare(`SELECT id, itemsAffected, message FROM migrations_state ORDER BY id`).all() : [];
  db.close();
  return { master, tables, ledger };
}

/** v4's narrowed runner over the twelve on `path` (env saved / restored, the
 *  journal mode put back, WAL residue swept); answers the ids it ran. */
async function runTwelve(path: string): Promise<string[]> {
  const v4Root = process.cwd();
  const mode = (() => {
    const db = new Database(path, { readonly: true });
    db.pragma(KEY_PRAGMA);
    const m = String(db.pragma('journal_mode', { simple: true }));
    db.close();
    return m;
  })();
  const keys = ['SQLITE_PATH', 'ENCRYPTION_MASTER_PEPPER', 'QUILLTAP_DATA_DIR'] as const;
  const saved = Object.fromEntries(keys.map((k) => [k, process.env[k]]));
  const scratch = mkdtempSync(join(tmpdir(), 'qt-zero-rerun-runner-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.SQLITE_PATH = path;
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.QUILLTAP_DATA_DIR = scratch;
  const utils = await import(join(v4Root, 'migrations/lib/database-utils.ts'));
  const migrations = [];
  for (const ref of P4D288_SPAN_MIGRATIONS) migrations.push((await import(join(v4Root, ref.file)))[ref.exportName]);
  const ran: string[] = [];
  utils.closeSQLite();
  try {
    const { MigrationRunner } = await import(join(v4Root, 'migrations/index.ts'));
    const runner = new MigrationRunner();
    (runner as unknown as { migrations: unknown[] }).migrations = migrations;
    const result = await runner.runMigrations();
    if (!result.success) throw new Error(`the runner failed: ${JSON.stringify(result)}`);
    for (const r of result.results as Array<{ id: string }>) ran.push(r.id);
  } finally {
    utils.closeSQLite();
    for (const k of keys) {
      if (saved[k] === undefined) delete process.env[k];
      else process.env[k] = saved[k] as string;
    }
    rmSync(scratch, { recursive: true, force: true });
  }
  const db = new Database(path);
  db.pragma(KEY_PRAGMA);
  if (String(db.pragma('journal_mode', { simple: true })) !== mode) db.pragma(`journal_mode = ${mode}`);
  db.close();
  for (const suffix of ['-wal', '-shm']) if (existsSync(path + suffix)) rmSync(path + suffix);
  const journal = path + '-journal';
  if (existsSync(journal) && statSync(journal).size === 0) rmSync(journal);
  return ran;
}

function buildPlanted(path: string): void {
  const here = dirname(fileURLToPath(import.meta.url));
  const provisioning = join(here, '..', '..', '..', 'crates/quilltap-core/src/services/provisioning');
  const fresh = JSON.parse(readFileSync(join(provisioning, 'fresh_schema.json'), 'utf8')) as { main: string[] };
  const migrationIndexes = JSON.parse(readFileSync(join(provisioning, 'migration_indexes.json'), 'utf8')) as {
    main: string[];
  };
  const db = new Database(path);
  db.pragma(KEY_PRAGMA);
  db.pragma('journal_mode = TRUNCATE');
  for (const stmt of fresh.main) {
    if (stmt.includes('"memory_extraction_cursors"')) continue;
    let s = stmt;
    if (s.startsWith('CREATE TABLE "memories" (')) s = strip(s, [`  "supersededAt" TEXT,\n`]);
    if (s.startsWith('CREATE TABLE "chats" (')) s = strip(s, [`  "scenarioSource" TEXT,\n`, `  "scenarioNotes" TEXT,\n`]);
    if (s.startsWith('CREATE TABLE "tfidf_vocabularies" (')) s = strip(s, [`  "textShapeVersion" REAL DEFAULT 0,\n`]);
    db.exec(s);
  }
  for (const stmt of migrationIndexes.main) {
    if (stmt.includes('"idx_memories_supersededById"')) continue;
    db.exec(stmt);
  }
  db.exec(`ALTER TABLE "chats" ADD COLUMN "otherExtractionWatermarkMessageId" TEXT DEFAULT NULL`);
  db.exec(`ALTER TABLE "chat_settings" ADD COLUMN "autoHousekeepingSettings" TEXT`);
  db.exec(`ALTER TABLE "chat_settings" ADD COLUMN "memoryExtractionLimits" TEXT`);
  db.exec(`ALTER TABLE "chat_settings" ADD COLUMN "memoryExtractionConcurrency" INTEGER DEFAULT 1`);
  db.exec(`CREATE TABLE "vector_indices" (
      "id" TEXT PRIMARY KEY, "characterId" TEXT NOT NULL, "version" INTEGER NOT NULL,
      "dimensions" INTEGER NOT NULL, "createdAt" TEXT NOT NULL, "updatedAt" TEXT NOT NULL)`);
  db.exec(`CREATE TABLE "vector_entries" (
      "id" TEXT PRIMARY KEY, "characterId" TEXT NOT NULL, "embedding" BLOB NOT NULL, "createdAt" TEXT NOT NULL)`);

  const f32 = (xs: number[]) => Buffer.from(new Float32Array(xs).buffer);
  const chat = (id: string, extra: Record<string, unknown>) => {
    const cols = ['id', 'userId', 'title', 'createdAt', 'updatedAt', ...Object.keys(extra)];
    db.prepare(`INSERT INTO chats (${cols.join(', ')}) VALUES (${cols.map(() => '?').join(', ')})`).run(
      id, 'u1', 't', T0, T0, ...Object.values(extra),
    );
  };
  chat('c1', { otherExtractionWatermarkMessageId: 'm9', summaryAnchorMessageIds: '["a1","a2"]' });
  chat('c2', { summaryAnchorMessageIds: '["x"]' });
  const memory = (id: string, extra: Record<string, unknown>) => {
    const cols = ['id', 'characterId', 'content', 'summary', 'createdAt', 'updatedAt', ...Object.keys(extra)];
    db.prepare(`INSERT INTO memories (${cols.join(', ')}) VALUES (${cols.map(() => '?').join(', ')})`).run(
      id, 'k1', 'x', 'y', T0, T0, ...Object.values(extra),
    );
  };
  memory('m1', { importance: 0.4, reinforcementCount: 1, reinforcedImportance: 0.45, occurredAt: '2026-07-14T00:00:00Z' });
  memory('m2', { importance: 0.5, reinforcementCount: 3, reinforcedImportance: 0.6, occurredAt: '2026-07-14T12:00:00.000Z' });
  memory('m3', { tier: 'cold', supersededById: 'd1', reinforcedImportance: 0.5 });
  memory('d1', { source: 'CONSOLIDATED', consolidatedFrom: '["m3"]', reinforcedImportance: 0.5 });
  db.prepare(`INSERT INTO vector_indices VALUES ('vi1', 'k1', 1, 4, ?, ?)`).run(T0, T0);
  db.prepare(`INSERT INTO vector_entries VALUES ('m1', 'k1', ?, ?)`).run(f32([1, 0, 0, 0]), T0);
  db.prepare(`INSERT INTO vector_entries VALUES ('gone', 'k1', ?, ?)`).run(f32([0, 1, 0, 0]), T0);
  db.prepare(
    `INSERT INTO chat_settings (id, userId, avatarDisplayMode, avatarDisplayStyle, createdAt, updatedAt, autoHousekeepingSettings, memoryExtractionLimits, memoryExtractionConcurrency, timezone)
     VALUES ('s1', 'ffffffff-ffff-ffff-ffff-ffffffffffff', 'ALWAYS', 'CIRCULAR', ?, ?, '{"enabled":true,"perCharacterCap":50}', '{"enabled":true}', 3, NULL)`,
  ).run(T0, T0);
  db.prepare(`INSERT INTO instance_settings (key, value) VALUES ('memoryExtractionConcurrency', '3')`).run();
  const job = (id: string, type: string) =>
    db.prepare(
      `INSERT INTO background_jobs (id, userId, type, status, payload, scheduledAt, createdAt, updatedAt) VALUES (?, 'u1', ?, 'PENDING', '{}', ?, ?, ?)`,
    ).run(id, type, T0, T0, T0);
  job('j1', 'INTER_CHARACTER_MEMORY');
  job('j2', 'MEMORY_HOUSEKEEPING');
  job('j3', 'MEMORY_CONSOLIDATION');
  job('j4', 'EMBEDDING_GENERATE');
  db.prepare(
    `INSERT INTO tfidf_vocabularies (id, profileId, userId, vocabulary, idf, avgDocLength, vocabularySize, fittedAt, createdAt, updatedAt)
     VALUES ('v1', 'p1', 'u1', '[]', '[]', 1.5, 2, ?, ?, ?)`,
  ).run(T0, T0, T0);
  db.close();
}

async function main(): Promise<void> {
  const phase = process.env.QT_ZERO_RERUN_PHASE;
  const dir = process.env.QT_FIXTURE_ZERO_RERUN_DIR;
  if (!dir || (phase !== 'build' && phase !== 'v4')) {
    throw new Error('QT_ZERO_RERUN_PHASE (build | v4) and QT_FIXTURE_ZERO_RERUN_DIR are required');
  }
  process.env.LOG_LEVEL = 'error';
  mkdirSync(dir, { recursive: true });
  if (phase === 'build') {
    const planted = join(dir, 'planted.db');
    const migrated = join(dir, 'v4-migrated.db');
    sweep(planted);
    sweep(migrated);
    buildPlanted(planted);
    copyFileSync(planted, migrated);
    const ran = await runTwelve(migrated);
    process.stdout.write(JSON.stringify({ phase: 'build', ran, ...inspect(migrated) }) + '\n');
  } else {
    for (const mode of [
      'planted', 'fresh', 'fresh-used', 'v4-migrated',
      'planted-unstamped', 'fresh-unstamped', 'fresh-used-unstamped',
    ]) {
      const src = join(dir, `${mode}.after-v5.db`);
      if (!existsSync(src)) throw new Error(`${src} is missing — run the Rust write step first (header)`);
      const scratch = mkdtempSync(join(tmpdir(), 'qt-zero-rerun-'));
      const work = join(scratch, 'work.db');
      copyFileSync(src, work);
      const before = inspect(work);
      const ran = await runTwelve(work);
      const after = inspect(work);
      rmSync(scratch, { recursive: true, force: true });
      process.stdout.write(JSON.stringify({ phase: 'v4', mode, ran, before, after }) + '\n');
    }
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`zero-rerun oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
