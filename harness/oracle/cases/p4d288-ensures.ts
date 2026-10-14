/**
 * Oracle case — P4.D288 (C1-2): v4's REAL span migrations at the `83334b9a5`
 * pin against the starting shapes each of v5's re-homed boot ensures meets, so
 * every ensure is diffed against what v4 itself does to an existing instance.
 *
 * ONE driver, one FAMILY per invocation (`QT_P4D288_ENSURE_FAMILY`), each the
 * oracle of one harness family:
 *
 *   superseded-at       add-memory-superseded-at-v1
 *   scenario-source     add-chat-scenario-source-v1
 *   cursors-chain       add-episode-extraction-watermark-v1 → add-memory-extraction-cursors-v1
 *                       (v4's REAL MigrationRunner narrowed to the two — the ledger
 *                       is consulted and STAMPED, as on a v4 boot)
 *   superseded-by-index add-memories-superseded-by-index-v1
 *   maintenance-move    move-memory-maintenance-settings-v1
 *   vector-retire       retire-memory-vector-entries-v1
 *   retired-jobs        drop-inter-character-memory-jobs-v1 → drop-housekeeping-consolidation-jobs-v1
 *   tfidf-text-shape    add-tfidf-text-shape-version-v1
 *   recompute-importance recompute-reinforced-importance-v1   (runner — STAMPED, R-E)
 *   rezone-occurred-at  rezone-legacy-memory-occurred-at-v1    (runner — STAMPED, R-E)
 *
 * The two DATA rewrites (unit 12) run through the narrowed runner so the
 * stamp's `(id, itemsAffected, message)` is compared, and their recipe pins
 * the zone: `TZ=UTC QUILLTAP_TIMEZONE=UTC` (the rezone's host fallback; the
 * Rust side passes `UTC`).
 *
 * The baseline tables are v4's generateDDL at the pin — the committed
 * `fresh_schema.json`, which `provisioning_equivalence` (1d) holds equal to
 * v4's LIVE dump — with exactly the round's moved lines removed (a pre-round
 * instance), or a real migration's text where the family needs one (the
 * vector mirror from `sqlite-initial-schema-v1`; the cursor table in its
 * migration text). Every base file is ENCRYPTED with the test pepper and
 * written to `$QT_FIXTURE_P4D288_ENSURES_DIR/<family>/<mode>.db`; v4 migrates
 * a COPY and the Rust side runs v5's ensure on its own copy.
 *
 * Comparands (EFFECTS — v4's migration lines are NO-PORT): `sqlite_master`
 * (type, name, tbl_name, sql — every object but the two ledger tables), each
 * family table's `PRAGMA table_info` and EVERY row through SQL `quote()` (type-
 * exact: a REAL `1.0` and an INTEGER `1` differ; a BLOB is `X'…'`), and the
 * `migrations_state` rows `(id, itemsAffected, message)` (the minted
 * `completedAt` / `quilltapVersion` are not compared). v4's report is recorded.
 *
 * Run from the v4 pin under Node 24 (one invocation per family):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   F=superseded-at   # or any family above
 *   rm -rf /tmp/qt-p4d288-ensures/$F /tmp/oracle-p4d288-ensure-$F.ndjson
 *   cd ~/source/quilltap-server
 *   QT_P4D288_ENSURE_FAMILY=$F QT_FIXTURE_P4D288_ENSURES_DIR=/tmp/qt-p4d288-ensures \
 *     $N/npx tsx $V5W/harness/oracle/cases/p4d288-ensures.ts \
 *     > /tmp/oracle-p4d288-ensure-$F.ndjson
 */

import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { runV4Migrations, type V4MigrationRef } from '../lib/v4-migrations';

const PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const KEY_PRAGMA = `key = "x'${Buffer.from(PEPPER, 'base64').toString('hex')}'"`;
const T0 = '2026-01-01T00:00:00.000Z';
const SINGLE_USER = 'ffffffff-ffff-ffff-ffff-ffffffffffff';

const M = {
  supersededAt: { file: 'migrations/scripts/add-memory-superseded-at-v1.ts', exportName: 'addMemorySupersededAtMigration' },
  scenarioSource: { file: 'migrations/scripts/add-chat-scenario-source-v1.ts', exportName: 'addChatScenarioSourceMigration' },
  episode: { file: 'migrations/scripts/add-episode-extraction-watermark-v1.ts', exportName: 'addEpisodeExtractionWatermarkMigration' },
  cursors: { file: 'migrations/scripts/add-memory-extraction-cursors-v1.ts', exportName: 'addMemoryExtractionCursorsMigration' },
  supersededByIndex: { file: 'migrations/scripts/add-memories-superseded-by-index-v1.ts', exportName: 'addMemoriesSupersededByIndexMigration' },
  maintenanceMove: { file: 'migrations/scripts/move-memory-maintenance-settings-v1.ts', exportName: 'moveMemoryMaintenanceSettingsMigration' },
  vectorRetire: { file: 'migrations/scripts/retire-memory-vector-entries-v1.ts', exportName: 'retireMemoryVectorEntriesMigration' },
  dropInter: { file: 'migrations/scripts/drop-inter-character-memory-jobs-v1.ts', exportName: 'dropInterCharacterMemoryJobsMigration' },
  dropHk: { file: 'migrations/scripts/drop-housekeeping-consolidation-jobs-v1.ts', exportName: 'dropHousekeepingConsolidationJobsMigration' },
  tfidf: { file: 'migrations/scripts/add-tfidf-text-shape-version-v1.ts', exportName: 'addTfidfTextShapeVersionMigration' },
  recompute: { file: 'migrations/scripts/recompute-reinforced-importance-v1.ts', exportName: 'recomputeReinforcedImportanceMigration' },
  rezone: { file: 'migrations/scripts/rezone-legacy-memory-occurred-at-v1.ts', exportName: 'rezoneLegacyMemoryOccurredAtMigration' },
} satisfies Record<string, V4MigrationRef>;

const CURSOR_TABLE = `CREATE TABLE IF NOT EXISTS "memory_extraction_cursors" (
          "id" TEXT PRIMARY KEY,
          "chatId" TEXT NOT NULL,
          "pass" TEXT NOT NULL,
          "messageId" TEXT NOT NULL,
          "createdAt" TEXT NOT NULL,
          "updatedAt" TEXT NOT NULL,
          UNIQUE ("chatId", "pass"),
          FOREIGN KEY ("chatId") REFERENCES "chats"("id") ON DELETE CASCADE
        )`;
const LEDGER_DDL = `CREATE TABLE IF NOT EXISTS "migrations_state" (
        "id" TEXT PRIMARY KEY,
        "completedAt" TEXT NOT NULL,
        "quilltapVersion" TEXT NOT NULL,
        "itemsAffected" INTEGER NOT NULL DEFAULT 0,
        "message" TEXT
      );
      CREATE TABLE IF NOT EXISTS "migrations_metadata" (
        "key" TEXT PRIMARY KEY,
        "value" TEXT NOT NULL
      );`;
const VECTOR_INDICES = `CREATE TABLE IF NOT EXISTS "vector_indices" (
      "id" TEXT PRIMARY KEY,
      "characterId" TEXT NOT NULL,
      "version" INTEGER NOT NULL,
      "dimensions" INTEGER NOT NULL,
      "createdAt" TEXT NOT NULL,
      "updatedAt" TEXT NOT NULL
    )`;
const VECTOR_ENTRIES = `CREATE TABLE IF NOT EXISTS "vector_entries" (
      "id" TEXT PRIMARY KEY,
      "characterId" TEXT NOT NULL,
      "embedding" BLOB NOT NULL,
      "createdAt" TEXT NOT NULL
    )`;

interface Family {
  tables: string[];
  migrations: V4MigrationRef[];
  /** Through v4's REAL MigrationRunner (ledger consulted + stamped). */
  runner?: boolean;
  modes: Record<string, (db: any) => void>;
}

function strip(ddl: string, lines: string[]): string {
  let out = ddl;
  for (const line of lines) {
    if (out.split(line).length !== 2) throw new Error(`expected exactly one ${JSON.stringify(line)} in:\n${ddl}`);
    out = out.replace(line, '');
  }
  return out;
}

async function main(): Promise<void> {
  const family = process.env.QT_P4D288_ENSURE_FAMILY;
  const root = process.env.QT_FIXTURE_P4D288_ENSURES_DIR;
  if (!family || !root) throw new Error('QT_P4D288_ENSURE_FAMILY and QT_FIXTURE_P4D288_ENSURES_DIR are required');
  process.env.LOG_LEVEL = 'error';

  const here = dirname(fileURLToPath(import.meta.url));
  const fresh = JSON.parse(
    readFileSync(join(here, '..', '..', '..', 'crates/quilltap-core/src/services/provisioning/fresh_schema.json'), 'utf8'),
  ) as { main: string[] };
  const ddl = (table: string) => {
    const s = fresh.main.find((x) => x.startsWith(`CREATE TABLE "${table}" (`));
    if (!s) throw new Error(`fresh_schema.json carries no ${table}`);
    return s;
  };
  const memoriesNow = ddl('memories');
  const memoriesPre = strip(memoriesNow, [`  "supersededAt" TEXT,\n`]);
  const memoriesPreTiers = strip(memoriesNow, [
    `  "tier" TEXT DEFAULT 'hot',\n`,
    `  "supersededById" TEXT,\n`,
    `  "supersededAt" TEXT,\n`,
    `  "consolidatedFrom" TEXT DEFAULT '[]',\n`,
    `  "consolidatedAt" TEXT,\n`,
  ]);
  const chatsNow = ddl('chats');
  const chatsPre = strip(chatsNow, [`  "scenarioSource" TEXT,\n`, `  "scenarioNotes" TEXT,\n`]);
  const chatsHalf = strip(chatsNow, [`  "scenarioNotes" TEXT,\n`]);

  const memory = (db: any, id: string, extra: Record<string, unknown> = {}) => {
    const cols = ['id', 'characterId', 'content', 'summary', 'createdAt', 'updatedAt', ...Object.keys(extra)];
    const vals = [id, 'c1', 'x', 'y', T0, '2026-02-01T00:00:00.000Z', ...Object.values(extra)];
    db.prepare(`INSERT INTO memories (${cols.join(', ')}) VALUES (${cols.map(() => '?').join(', ')})`).run(...vals);
  };
  const chat = (db: any, id: string, extra: Record<string, unknown> = {}) => {
    const cols = ['id', 'userId', 'title', 'createdAt', 'updatedAt', ...Object.keys(extra)];
    const vals = [id, 'u1', 't', T0, T0, ...Object.values(extra)];
    db.prepare(`INSERT INTO chats (${cols.join(', ')}) VALUES (${cols.map(() => '?').join(', ')})`).run(...vals);
  };
  const ledger = (db: any, ids: string[]) => {
    db.exec(LEDGER_DDL);
    for (const id of ids) {
      db.prepare(
        `INSERT INTO migrations_state (id, completedAt, quilltapVersion, itemsAffected, message) VALUES (?, ?, '4.10.0', 0, 'planted')`,
      ).run(id, T0);
    }
  };
  const settingsRow = (db: any, id: string, userId: string, updatedAt: string, ahk: string | null) => {
    db.prepare(
      `INSERT INTO chat_settings (id, userId, avatarDisplayMode, avatarDisplayStyle, createdAt, updatedAt, autoHousekeepingSettings, memoryExtractionLimits, memoryExtractionConcurrency)
       VALUES (?, ?, 'ALWAYS', 'CIRCULAR', ?, ?, ?, '{"enabled":true}', 3)`,
    ).run(id, userId, T0, updatedAt, ahk);
  };
  const deadSettingsColumns = (db: any) => {
    db.exec(`ALTER TABLE "chat_settings" ADD COLUMN "autoHousekeepingSettings" TEXT`);
    db.exec(`ALTER TABLE "chat_settings" ADD COLUMN "memoryExtractionLimits" TEXT`);
    db.exec(`ALTER TABLE "chat_settings" ADD COLUMN "memoryExtractionConcurrency" INTEGER DEFAULT 1`);
  };
  const f32 = (xs: number[]) => Buffer.from(new Float32Array(xs).buffer);
  const job = (db: any, id: string, type: string, status: string) => {
    db.prepare(
      `INSERT INTO background_jobs (id, userId, type, status, payload, scheduledAt, createdAt, updatedAt) VALUES (?, 'u1', ?, ?, '{}', ?, ?, ?)`,
    ).run(id, type, status, T0, T0, T0);
  };

  const families: Record<string, Family> = {
    'superseded-at': {
      tables: ['memories'],
      migrations: [M.supersededAt],
      modes: {
        'pre-round': (db) => {
          db.exec(memoriesPre);
          memory(db, 'm1', { supersededById: 'd1', tier: 'cold' });
          memory(db, 'm2');
          memory(db, 'd1');
        },
        'no-table': (db) => db.exec('CREATE TABLE "unrelated" ("id" TEXT PRIMARY KEY NOT NULL)'),
        'column-present-unstamped': (db) => {
          db.exec(memoriesNow);
          memory(db, 'm1', { supersededById: 'd1' });
          memory(db, 'm3', { supersededById: 'd1', supersededAt: '2026-03-01T00:00:00.000Z' });
          memory(db, 'd1');
        },
        'already-migrated': (db) => {
          db.exec(memoriesNow);
          memory(db, 'm3', { supersededById: 'd1', supersededAt: '2026-03-01T00:00:00.000Z' });
          memory(db, 'd1');
        },
        'pre-tiers': (db) => {
          db.exec(memoriesPreTiers);
          memory(db, 'm1');
        },
      },
    },
    'scenario-source': {
      tables: ['chats'],
      migrations: [M.scenarioSource],
      modes: {
        'pre-round': (db) => {
          db.exec(chatsPre);
          chat(db, 'c1', { scenarioText: 'A rainy evening.' });
        },
        'no-table': (db) => db.exec('CREATE TABLE "unrelated" ("id" TEXT PRIMARY KEY NOT NULL)'),
        'half': (db) => {
          db.exec(chatsHalf);
          chat(db, 'c1');
        },
        'current': (db) => {
          db.exec(chatsNow);
          chat(db, 'c1');
        },
      },
    },
    'cursors-chain': {
      tables: ['chats', 'memory_extraction_cursors'],
      migrations: [M.episode, M.cursors],
      runner: true,
      modes: {
        'pre-round': (db) => {
          db.exec(chatsNow);
          db.exec(`ALTER TABLE "chats" ADD COLUMN "otherExtractionWatermarkMessageId" TEXT DEFAULT NULL`);
          chat(db, 'c1', { otherExtractionWatermarkMessageId: 'm9', summaryAnchorMessageIds: '["a1","a2"]' });
          chat(db, 'c2', { otherExtractionWatermarkMessageId: '', summaryAnchorMessageIds: '[]' });
          chat(db, 'c3', { summaryAnchorMessageIds: '["x"]' });
          chat(db, 'c4', { summaryAnchorMessageIds: '' });
          chat(db, 'c5', { summaryAnchorMessageIds: 'garbage' });
          chat(db, 'c6', { summaryAnchorMessageIds: '["y",""]' });
          chat(db, 'c7', { otherExtractionWatermarkMessageId: 'm7' });
        },
        'already-migrated': (db) => {
          db.exec(chatsNow);
          db.exec(CURSOR_TABLE);
          chat(db, 'c1', { summaryAnchorMessageIds: '["a1"]' });
          db.prepare(
            `INSERT INTO memory_extraction_cursors VALUES ('c1:other-fold', 'c1', 'other-fold', 'm1', ?, ?)`,
          ).run(T0, T0);
          ledger(db, ['add-episode-extraction-watermark-v1', 'add-memory-extraction-cursors-v1']);
        },
        'half-migrated': (db) => {
          db.exec(chatsNow);
          db.exec(`ALTER TABLE "chats" ADD COLUMN "otherExtractionWatermarkMessageId" TEXT DEFAULT NULL`);
          db.exec(CURSOR_TABLE);
          chat(db, 'c1', { otherExtractionWatermarkMessageId: 'm9', summaryAnchorMessageIds: '["a1"]' });
          chat(db, 'c2', { otherExtractionWatermarkMessageId: 'm5' });
          db.prepare(
            `INSERT INTO memory_extraction_cursors VALUES ('c1:other-fold', 'c1', 'other-fold', 'old', ?, ?)`,
          ).run(T0, T0);
          ledger(db, ['add-episode-extraction-watermark-v1']);
        },
        'mid-chain': (db) => {
          db.exec(chatsNow);
          db.exec(`ALTER TABLE "chats" ADD COLUMN "otherExtractionWatermarkMessageId" TEXT DEFAULT NULL`);
          db.exec(`ALTER TABLE "chats" ADD COLUMN "episodeExtractionWatermarkMessageId" TEXT DEFAULT NULL`);
          chat(db, 'c1', {
            otherExtractionWatermarkMessageId: 'm9',
            episodeExtractionWatermarkMessageId: 'e4',
            summaryAnchorMessageIds: '["a1"]',
          });
          chat(db, 'c2', { episodeExtractionWatermarkMessageId: 'e2' });
          ledger(db, ['add-episode-extraction-watermark-v1']);
        },
        'no-chats': (db) => db.exec('CREATE TABLE "unrelated" ("id" TEXT PRIMARY KEY NOT NULL)'),
        'fresh-v5-unledgered': (db) => {
          db.exec(chatsNow);
          db.exec(CURSOR_TABLE);
          chat(db, 'c1', { summaryAnchorMessageIds: '["a1","a2"]' });
        },
      },
    },
    'superseded-by-index': {
      tables: ['memories'],
      migrations: [M.supersededByIndex],
      modes: {
        'index-absent': (db) => {
          db.exec(memoriesNow);
          memory(db, 'm1', { supersededById: 'd1' });
        },
        'pre-tiers': (db) => db.exec(memoriesPreTiers),
        'index-present': (db) => {
          db.exec(memoriesNow);
          db.exec(
            `CREATE INDEX "idx_memories_supersededById" ON "memories" ("supersededById") WHERE "supersededById" IS NOT NULL`,
          );
        },
        'no-table': (db) => db.exec('CREATE TABLE "unrelated" ("id" TEXT PRIMARY KEY NOT NULL)'),
      },
    },
    'maintenance-move': {
      tables: ['chat_settings', 'instance_settings'],
      migrations: [M.maintenanceMove],
      modes: {
        'single-user': (db) => {
          db.exec(ddl('chat_settings'));
          db.exec(ddl('instance_settings'));
          deadSettingsColumns(db);
          settingsRow(
            db,
            's1',
            SINGLE_USER,
            T0,
            '{"enabled":true,"perCharacterCap":50,"perCharacterCapOverrides":{"c1":200000,"10":300,"2":400},"mergeSimilar":true,"autoMergeSimilarThreshold":0.8}',
          );
          settingsRow(db, 's2', 'u2', '2026-09-01T00:00:00.000Z', '{"enabled":false,"perCharacterCap":900}');
          db.prepare(`INSERT INTO instance_settings (key, value) VALUES ('memoryExtractionConcurrency', '3'), ('other', 'kept')`).run();
        },
        'latest-row': (db) => {
          db.exec(ddl('chat_settings'));
          db.exec(ddl('instance_settings'));
          deadSettingsColumns(db);
          settingsRow(db, 's1', 'u1', '2026-02-01T00:00:00.000Z', '{"enabled":false,"perCharacterCap":800}');
          settingsRow(db, 's2', 'u2', '2026-03-01T00:00:00.000Z', '{"enabled":true,"perCharacterCap":1500}');
        },
        'single-user-null': (db) => {
          db.exec(ddl('chat_settings'));
          db.exec(ddl('instance_settings'));
          deadSettingsColumns(db);
          settingsRow(db, 's1', SINGLE_USER, T0, null);
          settingsRow(db, 's2', 'u2', '2026-09-01T00:00:00.000Z', '{"enabled":true,"perCharacterCap":700}');
        },
        'key-exists': (db) => {
          db.exec(ddl('chat_settings'));
          db.exec(ddl('instance_settings'));
          deadSettingsColumns(db);
          settingsRow(db, 's1', SINGLE_USER, T0, '{"enabled":true,"perCharacterCap":600}');
          db.prepare(`INSERT INTO instance_settings (key, value) VALUES ('memoryMaintenance', '{"enabled":false}')`).run();
        },
        'dead-key-only': (db) => {
          db.exec(ddl('chat_settings'));
          db.exec(ddl('instance_settings'));
          db.prepare(`INSERT INTO instance_settings (key, value) VALUES ('memoryExtractionConcurrency', '2')`).run();
        },
        'garbage-value': (db) => {
          db.exec(ddl('chat_settings'));
          db.exec(ddl('instance_settings'));
          deadSettingsColumns(db);
          settingsRow(db, 's1', SINGLE_USER, T0, 'garbage');
        },
        'no-instance-settings': (db) => {
          db.exec(ddl('chat_settings'));
          deadSettingsColumns(db);
          settingsRow(db, 's1', SINGLE_USER, T0, '{"enabled":true}');
        },
        'nothing-dead': (db) => {
          db.exec(ddl('chat_settings'));
          db.exec(ddl('instance_settings'));
        },
      },
    },
    'vector-retire': {
      tables: ['memories', 'vector_entries', 'vector_indices'],
      migrations: [M.vectorRetire],
      modes: {
        'mirror-two-pages': (db) => {
          db.exec(memoriesNow);
          db.exec(VECTOR_INDICES);
          db.exec(VECTOR_ENTRIES);
          db.prepare(`INSERT INTO vector_indices VALUES ('vi1', 'c1', 1, 4, ?, ?)`).run(T0, T0);
          const entry = db.prepare(`INSERT INTO vector_entries VALUES (?, 'c1', ?, ?)`);
          // 510 rows a memory lacks — the copy crosses the 500-row page.
          for (let i = 0; i < 510; i++) {
            const id = `p${String(i).padStart(4, '0')}`;
            memory(db, id);
            entry.run(id, f32([i, 0.5, -0.25, 1]), T0);
          }
          memory(db, 'has', { embedding: f32([9, 9, 9, 9]) });
          entry.run('has', f32([1, 1, 1, 1]), T0);
          memory(db, 'emptyrow', { embedding: Buffer.alloc(0) });
          entry.run('emptyrow', f32([2, 2, 2, 2]), T0);
          entry.run('orphan', f32([3, 3, 3, 3]), T0);
          memory(db, 'textarr');
          entry.run('textarr', '[0.1, 0.2, 0.3, 0.4]', T0);
          memory(db, 'textobj');
          entry.run('textobj', '{"1":0.6,"0":0.5,"2":0.7,"3":0.8}', T0);
          memory(db, 'emptyblob');
          entry.run('emptyblob', Buffer.alloc(0), T0);
          memory(db, 'textnull');
          entry.run('textnull', 'null', T0);
          memory(db, 'textempty');
          entry.run('textempty', '[]', T0);
          memory(db, 'number');
          entry.run('number', 5, T0);
          // BLOBs copied BYTE-FOR-BYTE (v4 `mirrorBlob` hands the Buffer back
          // as-is): full-precision values a re-quantization would move, and a
          // length that is no multiple of 4.
          memory(db, 'precise');
          entry.run('precise', f32([0.123456789, -1e-7, 3.4e38, 0.1 + 0.2]), T0);
          memory(db, 'oddblob');
          entry.run('oddblob', Buffer.from([1, 2, 3, 4, 5]), T0);
        },
        'indices-only': (db) => {
          db.exec(memoriesNow);
          db.exec(VECTOR_INDICES);
        },
        'neither': (db) => db.exec(memoriesNow),
        'no-memories': (db) => {
          db.exec(VECTOR_ENTRIES);
          db.prepare(`INSERT INTO vector_entries VALUES ('x', 'c1', ?, ?)`).run(f32([1, 2]), T0);
        },
      },
    },
    'retired-jobs': {
      tables: ['background_jobs'],
      migrations: [M.dropInter, M.dropHk],
      modes: {
        'mixed': (db) => {
          db.exec(ddl('background_jobs'));
          job(db, 'j1', 'INTER_CHARACTER_MEMORY', 'PENDING');
          job(db, 'j2', 'INTER_CHARACTER_MEMORY', 'COMPLETED');
          job(db, 'j3', 'MEMORY_HOUSEKEEPING', 'FAILED');
          job(db, 'j4', 'MEMORY_CONSOLIDATION', 'PENDING');
          job(db, 'j5', 'EMBEDDING_GENERATE', 'PENDING');
          job(db, 'j6', 'MEMORY_MAINTENANCE', 'PROCESSING');
        },
        'none-retired': (db) => {
          db.exec(ddl('background_jobs'));
          job(db, 'j5', 'EMBEDDING_GENERATE', 'PENDING');
        },
        'no-table': (db) => db.exec('CREATE TABLE "unrelated" ("id" TEXT PRIMARY KEY NOT NULL)'),
      },
    },
    'tfidf-text-shape': {
      tables: ['tfidf_vocabularies'],
      migrations: [M.tfidf],
      modes: {
        'pre-round': (db) => {
          db.exec(strip(ddl('tfidf_vocabularies'), [`  "textShapeVersion" REAL DEFAULT 0,\n`]));
          db.prepare(
            `INSERT INTO tfidf_vocabularies (id, profileId, userId, vocabulary, idf, avgDocLength, vocabularySize, fittedAt, createdAt, updatedAt)
             VALUES ('v1', 'p1', 'u1', '[]', '[]', 1.5, 2, ?, ?, ?)`,
          ).run(T0, T0, T0);
        },
        'current': (db) => db.exec(ddl('tfidf_vocabularies')),
        'no-table': (db) => db.exec('CREATE TABLE "unrelated" ("id" TEXT PRIMARY KEY NOT NULL)'),
      },
    },
    'recompute-importance': {
      tables: ['memories'],
      migrations: [M.recompute],
      runner: true,
      modes: {
        'counts': (db) => {
          db.exec(memoriesNow);
          // (importance, reinforcementCount, stored reinforcedImportance)
          const rows: Array<[string, number | null, number | null, number | null]> = [
            ['r-null-count', 0.5, null, 0.55],
            ['r-zero', 0.5, 0, 0.5],
            ['r-one', 0.4, 1, 0.45],
            ['r-two', 0.4, 2, 0.4],
            ['r-million', 0.3, 1e6, 0.3],
            ['r-null-stored', 0.6, 3, null],
            ['r-null-importance', null, 4, 0.5],
            ['r-in-band', 0.5, 2, 0.5 + 0.05 + 4e-10],
            ['r-out-of-band', 0.5, 2, 0.5 + 0.05 + 4e-9],
            ['r-exact', 0.5, 2, 0.55],
          ];
          for (const [id, importance, count, stored] of rows) {
            memory(db, id, { importance, reinforcementCount: count, reinforcedImportance: stored });
          }
        },
        'ledgered': (db) => {
          db.exec(memoriesNow);
          memory(db, 'r-one', { importance: 0.4, reinforcementCount: 1, reinforcedImportance: 0.45 });
          ledger(db, ['recompute-reinforced-importance-v1']);
        },
        'zero-rows': (db) => db.exec(memoriesNow),
        'no-table': (db) => db.exec('CREATE TABLE "unrelated" ("id" TEXT PRIMARY KEY NOT NULL)'),
      },
    },
    'rezone-occurred-at': {
      tables: ['memories', 'chat_settings'],
      migrations: [M.rezone],
      runner: true,
      modes: (() => {
        const dates = (db: any) => {
          const rows: Array<[string, string]> = [
            ['z-no-millis', '2026-07-14T00:00:00Z'],
            ['z-millis', '2026-07-14T00:00:00.000Z'],
            ['z-offset', '2026-07-14T00:00:00+02:00'],
            ['z-noon', '2026-07-14T12:00:00.000Z'],
            ['z-pre-1970', '1960-03-01T00:00:00.000Z'],
            ['z-santiago-gap', '2022-09-11T00:00:00.000Z'],
            ['z-havana-gap', '2026-03-08T00:00:00.000Z'],
            ['z-garbage', 'T00:00:00 never'],
            // Where V8's whole `Date.parse` and a strict ISO parser part ways
            // (the zone-less form reads LOCAL time — UTC under the pinned TZ).
            ['z-no-zone', '2026-07-14T00:00:00'],
            ['z-micros', '2026-07-14T00:00:00.000000Z'],
            ['z-plus-zero', '2026-07-14T00:00:00+00:00'],
            ['z-lowercase', '2026-07-14t00:00:00z'],
            ['z-space-zone', '2026-07-14T00:00:00 Z'],
            ['z-half-second', '2026-07-14T00:00:00.5Z'],
          ];
          for (const [id, occurredAt] of rows) memory(db, id, { occurredAt });
          memory(db, 'z-null');
        };
        const settings = (db: any, zones: Array<string | null>) => {
          db.exec(ddl('chat_settings'));
          zones.forEach((tz, i) =>
            db
              .prepare(
                `INSERT INTO chat_settings (id, userId, avatarDisplayMode, avatarDisplayStyle, createdAt, updatedAt, timezone)
                 VALUES (?, ?, 'ALWAYS', 'CIRCULAR', ?, ?, ?)`,
              )
              .run(`s${i}`, `u${i}`, T0, T0, tz),
          );
        };
        return {
          'utc-fallback': (db: any) => {
            db.exec(memoriesNow);
            dates(db);
          },
          'chicago-after-an-invalid-zone': (db: any) => {
            db.exec(memoriesNow);
            settings(db, [null, 'Not/AZone', 'America/Chicago', 'Europe/Paris']);
            dates(db);
          },
          'santiago-dst-gap': (db: any) => {
            db.exec(memoriesNow);
            settings(db, ['America/Santiago']);
            dates(db);
          },
          'havana-dst-gap': (db: any) => {
            db.exec(memoriesNow);
            settings(db, ['America/Havana']);
            dates(db);
          },
          'tokyo-east': (db: any) => {
            db.exec(memoriesNow);
            settings(db, ['Asia/Tokyo']);
            dates(db);
          },
          'ledgered': (db: any) => {
            db.exec(memoriesNow);
            dates(db);
            ledger(db, ['rezone-legacy-memory-occurred-at-v1']);
          },
          'no-table': (db: any) => db.exec('CREATE TABLE "unrelated" ("id" TEXT PRIMARY KEY NOT NULL)'),
        };
      })(),
    },
  };

  const spec = families[family];
  if (!spec) throw new Error(`unknown family ${family}`);
  const outDir = join(root, family);
  mkdirSync(outDir, { recursive: true });
  const Database = createRequire(process.cwd() + '/')('better-sqlite3');
  const sweep = (path: string) => {
    for (const suffix of ['', '-journal', '-wal', '-shm']) if (existsSync(path + suffix)) rmSync(path + suffix);
  };

  const inspect = (path: string) => {
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
    for (const t of spec.tables) {
      if (!db.prepare(`SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?`).get(t)) {
        tables[t] = null;
        continue;
      }
      const info = db.prepare(`PRAGMA table_info("${t}")`).all() as Array<{ name: string }>;
      const cols = info.map((c) => `quote("${c.name}")`).join(', ');
      tables[t] = { info, rows: db.prepare(`SELECT ${cols} FROM "${t}" ORDER BY rowid`).raw().all() };
    }
    const hasLedger = !!db.prepare(`SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'migrations_state'`).get();
    const ledgerRows = hasLedger
      ? db.prepare(`SELECT id, itemsAffected, message FROM migrations_state ORDER BY id`).all()
      : [];
    db.close();
    return { master, tables, ledger: ledgerRows };
  };

  for (const [mode, build] of Object.entries(spec.modes)) {
    const base = join(outDir, `${mode}.db`);
    sweep(base);
    {
      const db = new Database(base);
      db.pragma(KEY_PRAGMA);
      db.pragma('journal_mode = TRUNCATE');
      build(db);
      db.close();
    }
    const scratch = mkdtempSync(join(tmpdir(), 'qt-p4d288-ensure-'));
    const work = join(scratch, 'work.db');
    copyFileSync(base, work);
    let report: string[];
    if (spec.runner) {
      report = await runNarrowedRunner(work, spec.migrations);
    } else {
      report = await runV4Migrations({ dbPath: work, pepperBase64: PEPPER, migrations: spec.migrations });
    }
    const state = inspect(work);
    rmSync(scratch, { recursive: true, force: true });
    process.stdout.write(JSON.stringify({ case: 'p4d288-ensures', family, mode, report, ...state }) + '\n');
  }
  process.exit(0);
}

/**
 * v4's REAL `MigrationRunner`, narrowed to `refs` (registration order) — it
 * consults `migrations_state` before `shouldRun` and records each run (the
 * `wardrobe-wear-stats-ensure.ts` / `apply-memory-tiers-migration.ts`
 * mechanics: the env saved and restored, the journal mode put back, WAL
 * residue swept).
 */
async function runNarrowedRunner(dbPath: string, refs: V4MigrationRef[]): Promise<string[]> {
  const v4Root = process.cwd();
  const Database = createRequire(v4Root + '/')('better-sqlite3');
  const mode = (() => {
    const db = new Database(dbPath, { readonly: true });
    db.pragma(KEY_PRAGMA);
    const m = String(db.pragma('journal_mode', { simple: true }));
    db.close();
    return m;
  })();
  const keys = ['SQLITE_PATH', 'ENCRYPTION_MASTER_PEPPER', 'QUILLTAP_DATA_DIR'] as const;
  const saved = Object.fromEntries(keys.map((k) => [k, process.env[k]]));
  const scratch = mkdtempSync(join(tmpdir(), 'qt-p4d288-runner-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.SQLITE_PATH = dbPath;
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.QUILLTAP_DATA_DIR = scratch;
  const utils = await import(join(v4Root, 'migrations/lib/database-utils.ts'));
  const migrations = [];
  for (const ref of refs) migrations.push((await import(join(v4Root, ref.file)))[ref.exportName]);
  const report: string[] = [];
  utils.closeSQLite();
  try {
    const { MigrationRunner } = await import(join(v4Root, 'migrations/index.ts'));
    const runner = new MigrationRunner();
    (runner as unknown as { migrations: unknown[] }).migrations = migrations;
    const result = await runner.runMigrations();
    if (!result.success) throw new Error(`the runner failed: ${JSON.stringify(result)}`);
    for (const r of result.results) report.push(`+RAN ${r.id} (${r.message})`);
  } finally {
    utils.closeSQLite();
    for (const k of keys) {
      if (saved[k] === undefined) delete process.env[k];
      else process.env[k] = saved[k] as string;
    }
    rmSync(scratch, { recursive: true, force: true });
  }
  const db = new Database(dbPath);
  db.pragma(KEY_PRAGMA);
  if (String(db.pragma('journal_mode', { simple: true })) !== mode) db.pragma(`journal_mode = ${mode}`);
  db.close();
  for (const suffix of ['-wal', '-shm']) if (existsSync(dbPath + suffix)) rmSync(dbPath + suffix);
  const journal = dbPath + '-journal';
  if (existsSync(journal) && statSync(journal).size === 0) rmSync(journal);
  return report;
}

main().catch((err) => {
  process.stderr.write(`p4d288-ensures oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
