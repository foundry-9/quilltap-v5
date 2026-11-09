/**
 * Run v4's REAL migration modules against one encrypted SQLite file (P4.D225 —
 * the migrator's module-running arm, and the per-run fixture builders' widen).
 *
 * Why the real modules and not the ADD-COLUMN twin: the chain after P4.D225
 * needs migrations the twin cannot express (#75's and #76's JavaScript
 * backfills, #76's DROP). Driving v4's own `shouldRun()` / `run()` is the only
 * shape that carries all of them, and for an ALTER-only migration (P4.D225's
 * `add-chat-refusal-ledger-v1`) it is simply the same statement from its
 * source.
 *
 * Mechanics that bite:
 *
 * - v4's migration connection (`migrations/lib/database-utils.ts`
 *   `getSQLiteDatabase`) reads `SQLITE_PATH` + `ENCRYPTION_MASTER_PEPPER` from
 *   the environment, takes the instance lock under the DATA dir, and opens the
 *   file with `journal_mode = WAL` — which is PERSISTED in the file header. A
 *   fixture must come back in the journal mode it went in with, so the mode is
 *   read first and restored after (and any `-wal`/`-shm` residue removed).
 * - The connection is a module-level singleton: `closeSQLite()` before and
 *   after, and the utils module is imported by the SAME absolute URL the
 *   migration modules resolve, so both see one instance.
 * - The environment is saved and restored, so a builder that set its own
 *   `SQLITE_PATH` for `initializeDatabase` is left as it was.
 *
 * Run with cwd = the v4 checkout (or a pinned worktree): every module path is
 * resolved against it, and `better-sqlite3` is required from it (v4 aliases it
 * to the sqleet/ChaCha20 build).
 */

import { existsSync, mkdtempSync, rmSync, statSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

export interface V4MigrationRef {
  /** Path under the v4 root, e.g. `migrations/scripts/add-chat-refusal-ledger.ts`. */
  file: string;
  /** The exported `Migration` object's name. */
  exportName: string;
}

export interface RunV4MigrationsOptions {
  dbPath: string;
  pepperBase64: string;
  migrations: V4MigrationRef[];
  /** Report what `shouldRun()` says; run nothing. */
  reportOnly?: boolean;
  /**
   * A migration whose FILE is absent at this pin is skipped and reported
   * rather than thrown — how a builder stays runnable at a baseline pin that
   * predates the migration (and so predates the columns, as v4 there does).
   */
  allowMissing?: boolean;
  /**
   * P4.D288 (O-5): run the modules through v4's REAL `MigrationRunner`,
   * NARROWED to exactly this list (registration order as given), instead of
   * calling `shouldRun()` / `run()` directly — so `migrations_state` is
   * consulted first and STAMPED for every migration that runs, exactly as a
   * real v4 boot records it. Ignored under `reportOnly`.
   */
  runner?: boolean;
  /**
   * P4.D341 — the llm-logs SIBLING file a migration that opens the logs
   * database itself reads (`SQLITE_LLM_LOGS_PATH`; v4's
   * `add-llm-logs-storyteller-round-id-column-v1` opens it directly with its
   * own `journal_mode = WAL` connection). Set for the run, restored after, and
   * its header's journal mode put back like `dbPath`'s.
   */
  llmLogsPath?: string;
}

/**
 * P4.D288 — the twelve migrations of the `83334b9a5` span, in v4's
 * registration order (`migrations/scripts/index.ts:1319-1340`): the two DATA
 * rewrites (slots 4 and 5) included. The keystone widening runs them through
 * the narrowed runner (`runner: true`) under `TZ=UTC QUILLTAP_TIMEZONE=UTC`.
 */
export const P4D288_SPAN_MIGRATIONS: V4MigrationRef[] = [
  // `add-memory-tiers-v1` is P4.D265's (pre-span), but its `shouldRun` holds on
  // any instance without its ledger row through the chat watermark column the
  // span's cursor migration drops — the narrowed runner must SEE that re-run
  // (the `83334b9a5` unification's review: v5 stamps it in the settled-chain
  // arm and after the move on the other shapes).
  { file: 'migrations/scripts/add-memory-tiers-v1.ts', exportName: 'addMemoryTiersMigration' },
  { file: 'migrations/scripts/add-memory-superseded-at-v1.ts', exportName: 'addMemorySupersededAtMigration' },
  { file: 'migrations/scripts/add-chat-scenario-source-v1.ts', exportName: 'addChatScenarioSourceMigration' },
  { file: 'migrations/scripts/add-episode-extraction-watermark-v1.ts', exportName: 'addEpisodeExtractionWatermarkMigration' },
  { file: 'migrations/scripts/recompute-reinforced-importance-v1.ts', exportName: 'recomputeReinforcedImportanceMigration' },
  { file: 'migrations/scripts/rezone-legacy-memory-occurred-at-v1.ts', exportName: 'rezoneLegacyMemoryOccurredAtMigration' },
  { file: 'migrations/scripts/add-memories-superseded-by-index-v1.ts', exportName: 'addMemoriesSupersededByIndexMigration' },
  { file: 'migrations/scripts/move-memory-maintenance-settings-v1.ts', exportName: 'moveMemoryMaintenanceSettingsMigration' },
  { file: 'migrations/scripts/retire-memory-vector-entries-v1.ts', exportName: 'retireMemoryVectorEntriesMigration' },
  { file: 'migrations/scripts/add-memory-extraction-cursors-v1.ts', exportName: 'addMemoryExtractionCursorsMigration' },
  { file: 'migrations/scripts/drop-inter-character-memory-jobs-v1.ts', exportName: 'dropInterCharacterMemoryJobsMigration' },
  { file: 'migrations/scripts/add-tfidf-text-shape-version-v1.ts', exportName: 'addTfidfTextShapeVersionMigration' },
  { file: 'migrations/scripts/drop-housekeeping-consolidation-jobs-v1.ts', exportName: 'dropHousekeepingConsolidationJobsMigration' },
];

/**
 * P4.D304 — the two 4.11 `chat_settings` migrations, in v4's registration
 * order (`migrations/scripts/index.ts:909`, `:911` at the `25b8fab6a` pin —
 * appended right after the span's slot 12): triggers, then slash. NOTE the
 * file names carry NO `-v1` suffix, unlike the ids. Kept beside — never inside
 * — `P4D288_SPAN_MIGRATIONS`, which the widener's `--p4d288-span` flag reads
 * by name.
 */
export const CHAT_SETTINGS_4_11_MIGRATIONS: V4MigrationRef[] = [
  { file: 'migrations/scripts/add-custom-tool-triggers-field.ts', exportName: 'addCustomToolTriggersFieldMigration' },
  // P4.D341 (v4 `839575d89`, the TWENTIETH D23 move): registered in the MIDDLE
  // of the 4.11 pair at the `236c04e35` pin (`index.ts:921-925`).
  { file: 'migrations/scripts/add-custom-tool-intervention-field.ts', exportName: 'addCustomToolInterventionFieldMigration' },
  { file: 'migrations/scripts/add-composer-slash-commands-field.ts', exportName: 'addComposerSlashCommandsFieldMigration' },
];

/**
 * P4.D315 — the chat wardrobe history's three migrations, in v4's
 * registration order (`migrations/scripts/index.ts:916-919` at the
 * `e8e17b2d9` pin — after `CHAT_SETTINGS_4_11_MIGRATIONS`): the table, the
 * `actorToolName` column, the one-time seed. These file names DO carry `-v1`.
 * Kept beside — never inside — the lists above.
 */
export const CHAT_WARDROBE_HISTORY_MIGRATIONS: V4MigrationRef[] = [
  { file: 'migrations/scripts/add-chat-wardrobe-history-table-v1.ts', exportName: 'addChatWardrobeHistoryTableMigration' },
  { file: 'migrations/scripts/add-chat-wardrobe-history-tool-name-v1.ts', exportName: 'addChatWardrobeHistoryToolNameMigration' },
  { file: 'migrations/scripts/seed-chat-wardrobe-history-v1.ts', exportName: 'seedChatWardrobeHistoryMigration' },
];

/**
 * P4.D324 — the Inventory's two column migrations, in v4's registration order
 * (`migrations/scripts/index.ts:924-926` at the `322a5488e` pin — after
 * `CHAT_WARDROBE_HISTORY_MIGRATIONS`, before the terminal geometry):
 * `chats.equippedInventory`, then `characters.canManageInventory`. Both
 * column-gated `addColumnIfMissing` appends (`TEXT DEFAULT NULL` / `INTEGER
 * DEFAULT NULL`); v5's ensures are UNSTAMPED. Kept beside — never inside —
 * the lists above.
 */
export const INVENTORY_MIGRATIONS: V4MigrationRef[] = [
  { file: 'migrations/scripts/add-chat-equipped-inventory-v1.ts', exportName: 'addChatEquippedInventoryMigration' },
  { file: 'migrations/scripts/add-character-inventory-flag-v1.ts', exportName: 'addCharacterInventoryFlagMigration' },
];

// === P4.D337 (v4 a6292f169, the FIFTEENTH D23 move) ===
/**
 * P4.D337 — bug 255's terminal geometry migration, registered LAST at the
 * `322a5488e` pin (`migrations/scripts/index.ts:928`, after the Inventory
 * pair): `terminal_sessions.cols` / `rows`, two column-gated
 * `addColumnIfMissing` appends (`INTEGER DEFAULT NULL`, `cols` then `rows`).
 * The file name carries `-v1`; its `dependsOn` (`add-terminal-sessions-table`)
 * does not. v5's ensure is UNSTAMPED.
 */
export const TERMINAL_GEOMETRY_MIGRATIONS: V4MigrationRef[] = [
  {
    file: 'migrations/scripts/add-terminal-session-geometry-v1.ts',
    exportName: 'addTerminalSessionGeometryMigration',
  },
];
// === end P4.D337 ===

// === P4.D341 (v4 8cb584eff / b15fe9404 / 778b97b2b — the 17th, 18th, 19th and 21st D23 moves) ===
/**
 * P4.D341 — the LAST four migrations of the `236c04e35` registry
 * (`migrations/scripts/index.ts:942-946`, after the terminal geometry), in
 * registration order: the memory keystone (column + partial index), the
 * Storyteller mode (three columns + the `storyteller_rounds` table), the
 * llm-logs round id (the logs FILE, opened by the module itself from
 * `SQLITE_LLM_LOGS_PATH` — pass `llmLogsPath`), the roleplay-template
 * unmarked pair. The intervention step (the 20th move) rides
 * `CHAT_SETTINGS_4_11_MIGRATIONS`. The llm-logs FILE name carries no `-v1`.
 * Every one of v5's five ensures is UNSTAMPED.
 */
export const KEYSTONE_236_MIGRATIONS: V4MigrationRef[] = [
  { file: 'migrations/scripts/add-memory-keystone-v1.ts', exportName: 'addMemoryKeystoneMigration' },
  { file: 'migrations/scripts/add-storyteller-mode-v1.ts', exportName: 'addStorytellerModeMigration' },
  {
    file: 'migrations/scripts/add-llm-logs-storyteller-round-id-column.ts',
    exportName: 'addLLMLogsStorytellerRoundIdColumnMigration',
  },
  {
    file: 'migrations/scripts/add-roleplay-template-unmarked-fields-v1.ts',
    exportName: 'addRoleplayTemplateUnmarkedFieldsMigration',
  },
];
// === end P4.D341 ===

/**
 * P4.D315 — the `e8e17b2d9` round's ONE shared fixture plant (§R.3, R-S).
 *
 * v4's fixtures are built from `generateDDL` (`initializeDatabase` runs no
 * migrations) and `ChatWardrobeHistoryRepository` is a plain class, so NO
 * committed v4 fixture carries `chat_wardrobe_history` at the pin — and v4's
 * `createBackup` reads it UNGUARDED (`backup-service.ts:291`). Every case that
 * reaches a settle, a history read or `createBackup` plants the table on its
 * PER-RUN copy through this helper (never a rebake of a committed pair), beside
 * the v5 side's `test_support::ensure_chat_wardrobe_history_on`. Consumers:
 * P4.D315's tier-2 and GET cases; P4.D316's recording / tools / rollback
 * cases (the recording survey §4.3's table); P4.D317's backup / restore /
 * export / import cases.
 *
 * Runs v4's REAL `CHAT_WARDROBE_HISTORY_DDL` statements (the table at its
 * final 11-column shape + both UNIQUE indexes) — never `ensureCollection`.
 * Given a better-sqlite3 handle it `exec`s them on it; given none it runs them
 * through v4's `rawQuery` (`@/lib/database/manager`, a DYNAMIC import so a
 * case that never calls this loads clean) on the initialised per-run database
 * — the `plantWardrobeWear` idiom (`cases/system-backup.test.ts`). Idempotent
 * (`IF NOT EXISTS` is in v4's text) and self-checking: it throws unless
 * `sqlite_master` holds the table afterwards.
 */
export async function plantChatWardrobeHistoryTable(db?: {
  exec(sql: string): unknown;
  prepare(sql: string): { get(...params: unknown[]): unknown };
}): Promise<void> {
  const { CHAT_WARDROBE_HISTORY_DDL } = await import(
    '@/lib/database/backends/sqlite/chat-wardrobe-history-ddl'
  );
  const check = `SELECT sql FROM sqlite_master WHERE name = 'chat_wardrobe_history'`;
  let planted: unknown;
  if (db) {
    for (const statement of CHAT_WARDROBE_HISTORY_DDL) db.exec(statement);
    planted = (db.prepare(check).get() as { sql?: string } | undefined)?.sql;
  } else {
    const { rawQuery } = await import('@/lib/database/manager');
    for (const statement of CHAT_WARDROBE_HISTORY_DDL) await rawQuery(statement);
    planted = ((await rawQuery(check)) as Array<{ sql?: string }>)[0]?.sql;
  }
  if (typeof planted !== 'string' || planted.length === 0) {
    throw new Error('plantChatWardrobeHistoryTable: chat_wardrobe_history is absent after the plant');
  }
}

/**
 * P4.D324 — the `322a5488e` round's Inventory column plant (§R.3, C1).
 *
 * v4 at the pin WRITES `chats.equippedInventory` at every chat create / seat
 * add / merge (`applyInventoryDefaults` — the `{"<charId>":[]}` stamp) and its
 * character reads carry `canManageInventory`; a COMMITTED pair predates both
 * columns, so v4's write is caught (`Failed to write equipped inventory`) and
 * the dumps diverge from v5's. Every case on a committed pair calls this on
 * its PER-RUN copy (never a widened committed pair), beside the v5 side's
 * `test_support::ensure_inventory_columns_on` — the same two clauses the
 * migrations append (`addColumnIfMissing`, `TEXT DEFAULT NULL` / `INTEGER
 * DEFAULT NULL`), each only when the column is absent.
 *
 * Given a better-sqlite3 handle it runs on it; given none it runs through
 * v4's `rawQuery` (`@/lib/database/manager`, a DYNAMIC import) on the
 * initialised per-run database — the `plantChatWardrobeHistoryTable` shape:
 * AWAIT it. Idempotent and self-checking: it throws unless `PRAGMA table_info`
 * names both columns afterwards.
 */
export async function plantInventoryColumns(db?: {
  exec(sql: string): unknown;
  prepare(sql: string): { all(...params: unknown[]): unknown };
}): Promise<void> {
  const PLANTS: Array<[string, string, string]> = [
    ['chats', 'equippedInventory', 'TEXT DEFAULT NULL'],
    ['characters', 'canManageInventory', 'INTEGER DEFAULT NULL'],
  ];
  const columnsOf = async (table: string): Promise<string[]> => {
    const sql = `PRAGMA table_info("${table}")`;
    const rows = db
      ? (db.prepare(sql).all() as Array<{ name: string }>)
      : ((await (await import('@/lib/database/manager')).rawQuery(sql)) as Array<{ name: string }>);
    return rows.map((r) => r.name);
  };
  for (const [table, column, decl] of PLANTS) {
    const before = await columnsOf(table);
    if (before.length === 0) throw new Error(`plantInventoryColumns: no ${table} table`);
    if (before.includes(column)) continue;
    const alter = `ALTER TABLE "${table}" ADD COLUMN "${column}" ${decl}`;
    if (db) db.exec(alter);
    else await (await import('@/lib/database/manager')).rawQuery(alter);
  }
  for (const [table, column] of PLANTS) {
    if (!(await columnsOf(table)).includes(column)) {
      throw new Error(`plantInventoryColumns: ${table}.${column} is absent after the plant`);
    }
  }
}

// === P4.D341 ===
/**
 * P4.D341 — the `236c04e35` round's per-case plant of the five moves'
 * EXISTING-instance shape (§R.3), the `plantInventoryColumns` role for a case
 * whose per-run copy predates the round (a committed pair the lane did not
 * widen, or a base a builder made before the pin).
 *
 * Unlike the Inventory plant it does not spell the clauses: it runs v4's REAL
 * migration modules — the intervention step (`CHAT_SETTINGS_4_11_MIGRATIONS`'
 * middle entry) then `KEYSTONE_236_MIGRATIONS`, in registration order —
 * through {@link runV4Migrations}' direct arm (no ledger rows: a plant is not
 * a boot), so the partial index, the `storyteller_rounds` table in the
 * migration's text and the llm-logs column + index all land exactly as v4
 * makes them. `llmLogsPath` names the per-run llm-logs sibling (omit it for a
 * main-only copy: v4's round-id migration then finds no file and does
 * nothing). Close every connection on both files first. Idempotent (each
 * `shouldRun` is structural) and self-checking: it throws unless the main
 * file's `memories` (when present) carries `keystone` + the index afterwards,
 * and the sibling's `llm_logs` (when present) carries `storytellerRoundId`.
 * The v5 side's twins are `test_support::ensure_keystone_schema_on` /
 * `ensure_llm_logs_storyteller_round_id_on`.
 */
export async function plantKeystoneColumns(
  mainPath: string,
  llmLogsPath: string | undefined,
  pepperBase64: string,
): Promise<string[]> {
  const report = await runV4Migrations({
    dbPath: mainPath,
    pepperBase64,
    llmLogsPath,
    migrations: [
      CHAT_SETTINGS_4_11_MIGRATIONS.find((m) => m.exportName === 'addCustomToolInterventionFieldMigration')!,
      ...KEYSTONE_236_MIGRATIONS,
    ],
  });
  const Database = createRequire(process.cwd() + '/')('better-sqlite3');
  const check = (path: string, probe: (db: any) => string | null) => {
    const db = new Database(path, { readonly: true });
    try {
      db.pragma(`key = "x'${keyHex(pepperBase64)}'"`);
      const problem = probe(db);
      if (problem) throw new Error(`plantKeystoneColumns: ${problem} after the plant (${path})`);
    } finally {
      db.close();
    }
  };
  const hasTable = (db: any, t: string) =>
    !!db.prepare(`SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?`).get(t);
  const cols = (db: any, t: string) =>
    (db.prepare(`PRAGMA table_info("${t}")`).all() as Array<{ name: string }>).map((c) => c.name);
  check(mainPath, (db) => {
    if (hasTable(db, 'memories')) {
      if (!cols(db, 'memories').includes('keystone')) return 'memories.keystone is absent';
      if (!db.prepare(`SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = 'idx_memories_keystone'`).get()) {
        return 'idx_memories_keystone is absent';
      }
    }
    if (hasTable(db, 'chats') && hasTable(db, 'chat_messages') && !hasTable(db, 'storyteller_rounds')) {
      return 'storyteller_rounds is absent';
    }
    return null;
  });
  if (llmLogsPath && existsSync(llmLogsPath)) {
    check(llmLogsPath, (db) =>
      hasTable(db, 'llm_logs') && !cols(db, 'llm_logs').includes('storytellerRoundId')
        ? 'llm_logs.storytellerRoundId is absent'
        : null,
    );
  }
  return report;
}
// === end P4.D341 ===

// === P4.D362 (v4 544c7ab1e / db6190b44 / d439f6dc9 — the 22nd / 23rd D23 moves + the Anthropic strip) ===
/**
 * P4.D362 — the three `624d0d25c` migrations v5's
 * `db::schema_4_12_repair::run_4_12_ensures` re-homes, in v4's registration
 * order (`migrations/scripts/index.ts:950-960`, after the unmarked pair):
 * the STAMPED Anthropic sampling strip, then the two UNSTAMPED column moves.
 * The FTS character-only reindex v4 registers between the strip and the
 * established-fact column is P4.D363's — EXCLUDED here; P4.D363 appends its own
 * block after this one.
 */
export const PIN_624_MIGRATIONS: V4MigrationRef[] = [
  {
    file: 'migrations/scripts/strip-anthropic-sampling-parameters-v1.ts',
    exportName: 'stripAnthropicSamplingParametersMigration',
  },
  { file: 'migrations/scripts/add-established-fact-field-v1.ts', exportName: 'addEstablishedFactFieldMigration' },
  {
    file: 'migrations/scripts/add-profile-max-images-per-request-v1.ts',
    exportName: 'addProfileMaxImagesPerRequestMigration',
  },
];

/**
 * P4.D362 — the round's per-case fixture plant (R-L1): the two COLUMN
 * migrations of {@link PIN_624_MIGRATIONS} (NOT the strip — a plant is not a
 * boot) through {@link runV4Migrations}' direct arm, no ledger rows, onto a
 * PER-RUN copy of a pre-round main file (never a committed pair). Needed
 * wherever a case WRITES a connection profile on a pre-column pair (v4's
 * `_create` always names `maxImagesPerRequest` — Zod's default 4 — so v4
 * itself fails `no column named maxImagesPerRequest` otherwise) or plants an
 * established-fact message. Close every connection on the file first.
 * Idempotent (each `shouldRun` is structural) and self-checking: it throws
 * unless each present table carries its column afterwards. The v5 side's twin
 * is `test_support::ensure_4_12_columns_on`.
 */
export async function plant412Columns(mainPath: string, pepperBase64: string): Promise<string[]> {
  const report = await runV4Migrations({
    dbPath: mainPath,
    pepperBase64,
    migrations: PIN_624_MIGRATIONS.filter((m) => m.exportName !== 'stripAnthropicSamplingParametersMigration'),
  });
  const Database = createRequire(process.cwd() + '/')('better-sqlite3');
  const db = new Database(mainPath, { readonly: true });
  try {
    db.pragma(`key = "x'${keyHex(pepperBase64)}'"`);
    const hasTable = (t: string) =>
      !!db.prepare(`SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?`).get(t);
    const cols = (t: string) =>
      (db.prepare(`PRAGMA table_info("${t}")`).all() as Array<{ name: string }>).map((c) => c.name);
    for (const [t, c] of [
      ['chat_messages', 'isEstablishedFact'],
      ['connection_profiles', 'maxImagesPerRequest'],
    ]) {
      if (hasTable(t) && !cols(t).includes(c)) {
        throw new Error(`plant412Columns: ${t}.${c} is absent after the plant (${mainPath})`);
      }
    }
  } finally {
    db.close();
  }
  return report;
}
// === end P4.D362 ===

const ENV_KEYS = ['SQLITE_PATH', 'ENCRYPTION_MASTER_PEPPER', 'QUILLTAP_DATA_DIR', 'SQLITE_LLM_LOGS_PATH'] as const;

function keyHex(pepperBase64: string): string {
  return Buffer.from(pepperBase64, 'base64').toString('hex');
}

function readJournalMode(Database: any, dbPath: string, pepperBase64: string): string {
  const db = new Database(dbPath, { readonly: true });
  try {
    db.pragma(`key = "x'${keyHex(pepperBase64)}'"`);
    return String(db.pragma('journal_mode', { simple: true }));
  } finally {
    db.close();
  }
}

export async function runV4Migrations(opts: RunV4MigrationsOptions): Promise<string[]> {
  const v4Root = process.cwd();
  const requireFromV4 = createRequire(v4Root + '/');
  const Database = requireFromV4('better-sqlite3');
  const report: string[] = [];

  const utilsPath = join(v4Root, 'migrations/lib/database-utils.ts');
  if (!existsSync(utilsPath)) throw new Error(`not a v4 checkout (no ${utilsPath})`);

  const savedEnv = Object.fromEntries(ENV_KEYS.map((k) => [k, process.env[k]]));
  const scratch = mkdtempSync(join(tmpdir(), 'qt-v4-migrations-'));
  const originalMode = readJournalMode(Database, opts.dbPath, opts.pepperBase64);
  const logsOriginalMode =
    opts.llmLogsPath && existsSync(opts.llmLogsPath)
      ? readJournalMode(Database, opts.llmLogsPath, opts.pepperBase64)
      : null;

  process.env.SQLITE_PATH = opts.dbPath;
  if (opts.llmLogsPath) process.env.SQLITE_LLM_LOGS_PATH = opts.llmLogsPath;
  else delete process.env.SQLITE_LLM_LOGS_PATH;
  process.env.ENCRYPTION_MASTER_PEPPER = opts.pepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;

  const utils = await import(pathToFileURL(utilsPath).href);
  utils.closeSQLite();
  try {
    if (opts.runner && !opts.reportOnly) {
      const migrations: Array<{ id: string }> = [];
      for (const ref of opts.migrations) {
        const file = join(v4Root, ref.file);
        if (!existsSync(file)) throw new Error(`migration module not found at this pin: ${ref.file}`);
        const migration = (await import(pathToFileURL(file).href))[ref.exportName];
        if (!migration) throw new Error(`${ref.file} exports no migration named ${ref.exportName}`);
        migrations.push(migration);
      }
      const { MigrationRunner } = await import(pathToFileURL(join(v4Root, 'migrations/index.ts')).href);
      const runner = new MigrationRunner();
      (runner as unknown as { migrations: unknown[] }).migrations = migrations;
      const result = await runner.runMigrations();
      if (!result.success) throw new Error(`the runner failed: ${JSON.stringify(result)}`);
      const ran = new Map<string, { message?: string }>(
        (result.results as Array<{ id: string; message?: string }>).map((r) => [r.id, r]),
      );
      for (const m of migrations) {
        const r = ran.get(m.id);
        report.push(r ? `+RAN ${m.id} (${r.message})` : `${m.id} not needed (or already recorded)`);
      }
    }
    for (const ref of opts.runner && !opts.reportOnly ? [] : opts.migrations) {
      const file = join(v4Root, ref.file);
      if (!existsSync(file)) {
        if (opts.allowMissing) {
          report.push(`${ref.file} absent at this pin (skipped)`);
          continue;
        }
        throw new Error(`migration module not found at this pin: ${ref.file}`);
      }
      const mod = await import(pathToFileURL(file).href);
      const migration = mod[ref.exportName];
      if (!migration || typeof migration.shouldRun !== 'function') {
        throw new Error(`${ref.file} exports no migration named ${ref.exportName}`);
      }
      const needed = await migration.shouldRun();
      if (!needed) {
        report.push(`${migration.id} not needed`);
        continue;
      }
      if (opts.reportOnly) {
        report.push(`WOULD RUN ${migration.id}`);
        continue;
      }
      const result = await migration.run();
      if (!result?.success) {
        throw new Error(`${migration.id} failed: ${result?.error ?? JSON.stringify(result)}`);
      }
      report.push(`+RAN ${migration.id} (${result.message})`);
    }
  } finally {
    utils.closeSQLite();
    for (const k of ENV_KEYS) {
      if (savedEnv[k] === undefined) delete process.env[k];
      else process.env[k] = savedEnv[k] as string;
    }
    rmSync(scratch, { recursive: true, force: true });
  }

  // Put the header's journal mode back and sweep WAL residue — in BOTH modes.
  // `reportOnly` writes no migration, but `shouldRun()` still opens v4's
  // migration connection, which sets `journal_mode = WAL` in the header: a dry
  // run over a committed fixture left it byte-changed until P4.D226 measured it
  // (41 of 41 copies differed after a `--report-only` pass).
  // P4.D341: the llm-logs sibling too — v4's round-id migration opens it with
  // its OWN `journal_mode = WAL` connection.
  const restores: Array<[string, string]> = [[opts.dbPath, originalMode]];
  if (opts.llmLogsPath && logsOriginalMode !== null && opts.llmLogsPath !== opts.dbPath) {
    restores.push([opts.llmLogsPath, logsOriginalMode]);
  }
  for (const [path, mode] of restores) {
    const db = new Database(path);
    try {
      db.pragma(`key = "x'${keyHex(opts.pepperBase64)}'"`);
      const now = String(db.pragma('journal_mode', { simple: true }));
      if (now !== mode) db.pragma(`journal_mode = ${mode}`);
    } finally {
      db.close();
    }
    for (const suffix of ['-wal', '-shm']) {
      if (existsSync(path + suffix)) rmSync(path + suffix);
    }
    // An EMPTY rollback journal is residue, not state (a non-empty one would
    // be a hot journal and is left for SQLite to recover).
    const journal = path + '-journal';
    if (existsSync(journal) && statSync(journal).size === 0) rmSync(journal);
  }
  return report;
}

/**
 * v4 `49059fb14` (#74): the Concierge refusal ledger's two `chats` columns.
 * Absent at any pin before it — builders pass `allowMissing: true` so they stay
 * runnable at the baseline (where v4 has no such columns either).
 */
export const ADD_CHAT_REFUSAL_LEDGER: V4MigrationRef = {
  file: 'migrations/scripts/add-chat-refusal-ledger.ts',
  exportName: 'addChatRefusalLedgerMigration',
};

// === P4.D363 (v4 902c6efe9 — the chat-message FTS character-only reindex) ===
/**
 * P4.D363 — the migration v4 registers BETWEEN {@link PIN_624_MIGRATIONS}' strip
 * and its established-fact column (`migrations/scripts/index.ts:956`). A
 * separate ref rather than an element of that array, because
 * {@link plant412Columns} runs every non-strip element as a COLUMN plant — the
 * reindex is not one. The zero-rerun runner list splices it in at v4's
 * registration position.
 */
export const PIN_624_FTS_REINDEX_MIGRATION: V4MigrationRef = {
  file: 'migrations/scripts/reindex-chat-message-fts-character-only.ts',
  exportName: 'reindexChatMessageFtsCharacterOnlyMigration',
};
// === end P4.D363 ===
