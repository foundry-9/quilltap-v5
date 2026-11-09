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
 *           no `tfidf_vocabularies.textShapeVersion`, no `chat_wardrobe_history`
 *           (P4.D315 — the narrowed runner's last three; `c1` carries an
 *           `equippedOutfit` so the seed is not vacuous), no 4.11 `chat_settings`
 *           toggles (P4.D304: `customToolTriggers` / `composerSlashCommands`,
 *           whose two migrations follow the span in the narrowed runner), the three dead
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
 *           hold back, and the bytes v4 writes for each; P4.D315 adds each
 *           base's `-history-unstamped` twin, the HISTORY rows deleted): dump the state, run v4's
 *           narrowed runner on a COPY, dump again; emit `{phase: 'v4', mode,
 *           ran, before, after}` (`ran` = the ids v4 ran — `+RAN` in its
 *           result; the dumps = `sqlite_master` minus the two ledger tables +
 *           every touched table through `quote()`, and the ledger rows).
 *
 * **P4.D341 (the `236c04e35` round):** the narrowed runner also carries the
 * intervention step (the MIDDLE of `CHAT_SETTINGS_4_11_MIGRATIONS`) and
 * `KEYSTONE_236_MIGRATIONS` LAST (keystone → storyteller → llm-logs round id →
 * unmarked); the planted base takes the five moves back (the memories,
 * chats ×2, message, intervention and template ×2 lines; no
 * `storyteller_rounds` + its createdAt index; no `idx_memories_keystone` /
 * `idx_storyteller_rounds_chat`), and — R-8 — a planted llm-logs SIBLING
 * `planted-llm-logs.db` (the dump's `llmLogs` block minus `storytellerRoundId`
 * and its index) rides beside every main file: the build migrates
 * `v4-migrated-llm-logs.db` with it, the Rust write step leaves a
 * `<mode>.llm-logs.after-v5.db` beside each `<mode>.after-v5.db`, and the v4
 * phase points `SQLITE_LLM_LOGS_PATH` at the copy and dumps it like the main
 * file (`logs`: `sqlite_master` + `llm_logs` through `quote()`).
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

import {
  CHAT_SETTINGS_4_11_MIGRATIONS,
  CHAT_WARDROBE_HISTORY_MIGRATIONS,
  INVENTORY_MIGRATIONS,
  P4D288_SPAN_MIGRATIONS,
  // === P4.D337 ===
  TERMINAL_GEOMETRY_MIGRATIONS,
  // === end P4.D337 ===
  // === P4.D341 ===
  KEYSTONE_236_MIGRATIONS,
  // === end P4.D341 ===
  // === P4.D362 ===
  PIN_624_MIGRATIONS,
  // === end P4.D362 ===
  // === P4.D363 ===
  PIN_624_FTS_REINDEX_MIGRATION,
  // === end P4.D363 ===
} from '../lib/v4-migrations';

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
  // P4.D315: the boot-created history table (absent on the planted base).
  'chat_wardrobe_history',
  // === P4.D337: the geometry columns (stripped from the planted base) ===
  'terminal_sessions',
  // === end P4.D337 ===
  // === P4.D341: the five moves' tables (stripped / absent on the planted base) ===
  'chat_messages',
  'roleplay_templates',
  'storyteller_rounds',
  // === end P4.D341 ===
  // === P4.D362: the strip's rows + the cap column (chat_messages is above) ===
  'connection_profiles',
  // === end P4.D362 ===
  // === P4.D363: the message search index's map (the reindex rebuilds it) ===
  'chat_messages_fts_map',
  // === end P4.D363 ===
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

/** P4.D341 (R-8): the llm-logs sibling's `sqlite_master` + `llm_logs` rows
 *  through `quote()` (`null` when the file is absent). */
function inspectLogs(path: string): Record<string, unknown> | null {
  if (!existsSync(path)) return null;
  const db = new Database(path, { readonly: true });
  db.pragma(KEY_PRAGMA);
  const master = db.prepare(`SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY type, name`).all();
  let rows: unknown = null;
  if (db.prepare(`SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'llm_logs'`).get()) {
    const info = db.prepare(`PRAGMA table_info("llm_logs")`).all() as Array<{ name: string }>;
    rows = db.prepare(`SELECT ${info.map((c) => `quote("${c.name}")`).join(', ')} FROM llm_logs ORDER BY rowid`).raw().all();
  }
  db.close();
  return { master, rows };
}

/** v4's narrowed runner over the twelve on `path` (env saved / restored, the
 *  journal mode put back, WAL residue swept); answers the ids it ran. */
async function runTwelve(path: string, logsPath?: string): Promise<string[]> {
  const v4Root = process.cwd();
  const mode = (() => {
    const db = new Database(path, { readonly: true });
    db.pragma(KEY_PRAGMA);
    const m = String(db.pragma('journal_mode', { simple: true }));
    db.close();
    return m;
  })();
  const keys = ['SQLITE_PATH', 'ENCRYPTION_MASTER_PEPPER', 'QUILLTAP_DATA_DIR', 'SQLITE_LLM_LOGS_PATH'] as const;
  // P4.D341: the llm-logs sibling's journal mode, put back like the main file's
  // (v4's round-id migration opens it with its OWN `journal_mode = WAL`).
  const logsMode =
    logsPath && existsSync(logsPath)
      ? (() => {
          const db = new Database(logsPath, { readonly: true });
          db.pragma(KEY_PRAGMA);
          const m = String(db.pragma('journal_mode', { simple: true }));
          db.close();
          return m;
        })()
      : null;
  const saved = Object.fromEntries(keys.map((k) => [k, process.env[k]]));
  const scratch = mkdtempSync(join(tmpdir(), 'qt-zero-rerun-runner-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.SQLITE_PATH = path;
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.QUILLTAP_DATA_DIR = scratch;
  if (logsPath) process.env.SQLITE_LLM_LOGS_PATH = logsPath;
  else delete process.env.SQLITE_LLM_LOGS_PATH;
  const utils = await import(join(v4Root, 'migrations/lib/database-utils.ts'));
  const migrations = [];
  // P4.D304: the span, then the two 4.11 chat_settings migrations (v4's
  // registration order — both unstamped on v5's side, R-B); P4.D315: then the
  // chat wardrobe history's three (table → tool-name column → seed — v5 stamps
  // the table and the seed as v4's runner would, R-C); P4.D324: then the
  // Inventory's two columns (both unstamped on v5's side); P4.D337: then the
  // terminal geometry (registered LAST at the pin; unstamped on v5's side).
  for (const ref of [
    ...P4D288_SPAN_MIGRATIONS,
    ...CHAT_SETTINGS_4_11_MIGRATIONS,
    ...CHAT_WARDROBE_HISTORY_MIGRATIONS,
    ...INVENTORY_MIGRATIONS,
    // === P4.D337 ===
    ...TERMINAL_GEOMETRY_MIGRATIONS,
    // === end P4.D337 ===
    // === P4.D341: the round's last four, LAST (registration order) ===
    ...KEYSTONE_236_MIGRATIONS,
    // === end P4.D341 ===
    // === P4.D362: the `624d0d25c` three (strip, established fact, image cap), LAST ===
    // === P4.D363: with the FTS reindex spliced in at v4's registration
    // position — after the strip, before the established-fact column ===
    PIN_624_MIGRATIONS[0],
    PIN_624_FTS_REINDEX_MIGRATION,
    ...PIN_624_MIGRATIONS.slice(1),
    // === end P4.D363 ===
    // === end P4.D362 ===
  ]) {
    migrations.push((await import(join(v4Root, ref.file)))[ref.exportName]);
  }
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
  if (logsPath && logsMode !== null) {
    const ldb = new Database(logsPath);
    ldb.pragma(KEY_PRAGMA);
    if (String(ldb.pragma('journal_mode', { simple: true })) !== logsMode) ldb.pragma(`journal_mode = ${logsMode}`);
    ldb.close();
    for (const suffix of ['-wal', '-shm']) if (existsSync(logsPath + suffix)) rmSync(logsPath + suffix);
    const lj = logsPath + '-journal';
    if (existsSync(lj) && statSync(lj).size === 0) rmSync(lj);
  }
  return ran;
}

function buildPlanted(path: string): void {
  const here = dirname(fileURLToPath(import.meta.url));
  const provisioning = join(here, '..', '..', '..', 'crates/quilltap-core/src/services/provisioning');
  const fresh = JSON.parse(readFileSync(join(provisioning, 'fresh_schema.json'), 'utf8')) as {
    main: string[];
    llmLogs: string[];
  };
  const migrationIndexes = JSON.parse(readFileSync(join(provisioning, 'migration_indexes.json'), 'utf8')) as {
    main: string[];
    llmLogs: string[];
  };
  const db = new Database(path);
  db.pragma(KEY_PRAGMA);
  db.pragma('journal_mode = TRUNCATE');
  for (const stmt of fresh.main) {
    if (stmt.includes('"memory_extraction_cursors"')) continue;
    // === P4.D341: no storyteller_rounds (+ its createdAt index) ===
    if (stmt.includes('"storyteller_rounds"')) continue;
    // === end P4.D341 ===
    let s = stmt;
    if (s.startsWith('CREATE TABLE "memories" (')) {
      s = strip(s, [`  "supersededAt" TEXT,\n`, /* P4.D341 */ `  "keystone" INTEGER DEFAULT 0,\n`]);
    }
    // === P4.D341: the Storyteller message column + the template pair ===
    if (s.startsWith('CREATE TABLE "chat_messages" (')) {
      s = strip(s, [`  "storytellerBid" TEXT,\n`, /* P4.D362 */ `  "isEstablishedFact" TEXT,\n`]);
    }
    // === P4.D362: the image cap taken back (pre-d439f6dc9) ===
    if (s.startsWith('CREATE TABLE "connection_profiles" (')) {
      s = strip(s, [`  "maxImagesPerRequest" INTEGER DEFAULT 4,\n`]);
    }
    // === end P4.D362 ===
    if (s.startsWith('CREATE TABLE "roleplay_templates" (')) {
      s = strip(s, [`  "unmarkedStyle" TEXT,\n`, `  "markdownAsterisks" INTEGER DEFAULT 1,\n`]);
    }
    // === end P4.D341 ===
    // P4.D324: the Inventory's two columns taken back too (the planted base is
    // pre-round; v4's runner APPENDS them, v5's ensures do the same).
    if (s.startsWith('CREATE TABLE "chats" (')) {
      s = strip(s, [
        `  "scenarioSource" TEXT,\n`,
        `  "scenarioNotes" TEXT,\n`,
        `  "equippedInventory" TEXT,\n`,
        // P4.D341: the Storyteller pair.
        `  "turnMode" TEXT,\n`,
        `  "storytellerInviteFilter" INTEGER DEFAULT 0,\n`,
      ]);
    }
    if (s.startsWith('CREATE TABLE "characters" (')) s = strip(s, [`  "canManageInventory" INTEGER,\n`]);
    // === P4.D337: the terminal geometry taken back too (pre-a6292f169) ===
    if (s.startsWith('CREATE TABLE "terminal_sessions" (')) s = strip(s, [`  "cols" REAL,\n`, `  "rows" REAL,\n`]);
    // === end P4.D337 ===
    if (s.startsWith('CREATE TABLE "tfidf_vocabularies" (')) s = strip(s, [`  "textShapeVersion" REAL DEFAULT 0,\n`]);
    // P4.D304: the 4.11 chat_settings toggles taken back too.
    if (s.startsWith('CREATE TABLE "chat_settings" (')) {
      s = strip(s, [
        `  "customToolTriggers" INTEGER DEFAULT 0,\n`,
        `  "composerSlashCommands" INTEGER DEFAULT 1,\n`,
        // P4.D341: the intervention toggle (the 4.11 trio's middle).
        `  "customToolIntervention" INTEGER DEFAULT 1,\n`,
      ]);
    }
    db.exec(s);
  }
  for (const stmt of migrationIndexes.main) {
    if (stmt.includes('"idx_memories_supersededById"')) continue;
    // === P4.D341: the two new main-partition migration indexes ===
    if (stmt.includes('"idx_memories_keystone"') || stmt.includes('"idx_storyteller_rounds_chat"')) continue;
    // === end P4.D341 ===
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
  // P4.D315: c1 is DRESSED, so the history seed has a pair to seed (else it
  // would be vacuous on the planted base).
  chat('c1', {
    otherExtractionWatermarkMessageId: 'm9',
    summaryAnchorMessageIds: '["a1","a2"]',
    equippedOutfit: '{"k1":{"top":["i1"],"hair":["i2"]}}',
  });
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
  // === P4.D341: rows the five moves' columns land under ===
  db.prepare(
    `INSERT INTO chat_messages (id, chatId, type, role, content, createdAt) VALUES ('msg1', 'c1', 'message', 'USER', 'hi', ?)`,
  ).run(T0);
  // === end P4.D341 ===
  // === P4.D362: two profiles for the Anthropic strip (one it cleans, one it never examines) ===
  for (const [id, provider, parameters] of [
    ['cp-anth', 'ANTHROPIC', '{"temperature":0.7,"max_tokens":512}'],
    ['cp-oai', 'OPENAI', '{"temperature":0.7}'],
  ]) {
    db.prepare(
      `INSERT INTO connection_profiles (id, userId, name, provider, modelName, parameters, tags, createdAt, updatedAt)
       VALUES (?, 'u1', ?, ?, 'm', ?, '[]', ?, ?)`,
    ).run(id, id, provider, parameters, T0, T0);
  }
  // === end P4.D362 ===
  db.close();

  // === P4.D341 (R-8): the planted llm-logs SIBLING ===
  const logsPath = path.replace(/\.db$/, '-llm-logs.db');
  sweep(logsPath);
  const logs = new Database(logsPath);
  logs.pragma(KEY_PRAGMA);
  logs.pragma('journal_mode = TRUNCATE');
  for (const stmt of fresh.llmLogs) {
    logs.exec(stmt.startsWith('CREATE TABLE "llm_logs" (') ? strip(stmt, [`  "storytellerRoundId" TEXT,\n`]) : stmt);
  }
  for (const stmt of migrationIndexes.llmLogs) {
    if (stmt.includes('"idx_llm_logs_storytellerRoundId"')) continue;
    logs.exec(stmt);
  }
  logs.prepare(
    `INSERT INTO llm_logs (id, userId, type, provider, modelName, request, response, createdAt, updatedAt)
     VALUES ('l1', 'u1', 'CHAT_MESSAGE', 'OPENAI', 'gpt', '{}', '{}', ?, ?)`,
  ).run(T0, T0);
  logs.close();
  // === end P4.D341 ===
}

// === P4.D363 (v4 902c6efe9) ===
/**
 * P4.D363 — the planted base's message search index, as a PRE-ROUND instance
 * carries it: `create-chat-message-fts-v1`'s objects (it ran in 4.10) with the
 * insert trigger as that migration first shipped it (v4's
 * `LEGACY_INSERT_TRIGGER`), the index rebuilt over the existing rows, then a
 * Staff row the legacy trigger INDEXES. Without it v4's narrowed runner throws
 * on the reindex (`run()` counts the map before ensuring it; v4 never meets
 * that because `dependsOn` runs `create-chat-message-fts-v1` first). Through
 * v4's REAL module and codec, so the bytes are v4's.
 */
async function plantPreRoundFts(path: string): Promise<void> {
  const v4Root = process.cwd();
  const fts = await import(join(v4Root, 'lib/database/backends/sqlite/chat-message-fts.ts'));
  const codec = await import(join(v4Root, 'lib/database/backends/sqlite/text-codec-function.ts'));
  const db = new Database(path);
  db.pragma(KEY_PRAGMA);
  codec.registerTextCodecFunction(db);
  const statements = fts.CHAT_MESSAGE_FTS_SCHEMA_STATEMENTS as string[];
  db.exec(statements[0]);
  db.exec(statements[1]);
  db.exec(`CREATE TRIGGER "chat_messages_fts_ai" AFTER INSERT ON "chat_messages"
  WHEN new."type" = 'message' AND new."role" IN ('USER','ASSISTANT') AND new."content" IS NOT NULL
BEGIN
  INSERT INTO "chat_messages_fts_map"("messageId") VALUES (new."id");
  INSERT INTO "chat_messages_fts"(rowid, content)
    VALUES ((SELECT "ftsId" FROM "chat_messages_fts_map" WHERE "messageId" = new."id"),
            qt_text(new."content"));
END`);
  db.exec(statements[3]);
  db.exec(statements[4]);
  fts.rebuildChatMessageFtsIndex(db);
  db.prepare(
    `INSERT INTO chat_messages (id, chatId, type, role, content, systemSender, createdAt) VALUES ('msg2', 'c1', 'message', 'ASSISTANT', 'a recall', 'commonplaceBook', ?)`,
  ).run(T0);
  const indexed = (db.prepare('SELECT COUNT(*) AS n FROM "chat_messages_fts_map"').get() as { n: number }).n;
  if (Number(indexed) !== 2) throw new Error(`plantPreRoundFts: expected 2 indexed rows, found ${indexed}`);
  if ((fts.staleChatMessageFtsTriggers(db) as string[]).join() !== 'chat_messages_fts_ai') {
    throw new Error('plantPreRoundFts: the legacy trigger is not stale to v4');
  }
  db.close();
}
// === end P4.D363 ===

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
    // === P4.D363 ===
    await plantPreRoundFts(planted);
    // === end P4.D363 ===
    copyFileSync(planted, migrated);
    // P4.D341 (R-8): the sibling migrates beside it.
    const migratedLogs = join(dir, 'v4-migrated-llm-logs.db');
    sweep(migratedLogs);
    copyFileSync(join(dir, 'planted-llm-logs.db'), migratedLogs);
    const ran = await runTwelve(migrated, migratedLogs);
    process.stdout.write(JSON.stringify({ phase: 'build', ran, ...inspect(migrated) }) + '\n');
  } else {
    for (const mode of [
      'planted', 'fresh', 'fresh-used', 'v4-migrated',
      'planted-unstamped', 'fresh-unstamped', 'fresh-used-unstamped',
      // P4.D315: each base after v5's chain with the HISTORY ledger rows deleted.
      'planted-history-unstamped', 'fresh-history-unstamped', 'fresh-used-history-unstamped',
    ]) {
      const src = join(dir, `${mode}.after-v5.db`);
      if (!existsSync(src)) throw new Error(`${src} is missing — run the Rust write step first (header)`);
      const scratch = mkdtempSync(join(tmpdir(), 'qt-zero-rerun-'));
      const work = join(scratch, 'work.db');
      copyFileSync(src, work);
      // P4.D341 (R-8): the mode's llm-logs sibling, copied beside it.
      const logsSrc = join(dir, `${mode}.llm-logs.after-v5.db`);
      if (!existsSync(logsSrc)) throw new Error(`${logsSrc} is missing — run the Rust write step first (header)`);
      const logsWork = join(scratch, 'work-llm-logs.db');
      copyFileSync(logsSrc, logsWork);
      const before = { ...inspect(work), logs: inspectLogs(logsWork) };
      const ran = await runTwelve(work, logsWork);
      const after = { ...inspect(work), logs: inspectLogs(logsWork) };
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
