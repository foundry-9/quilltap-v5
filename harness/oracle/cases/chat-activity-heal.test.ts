/**
 * @jest-environment node
 *
 * P4.D140 heal ORACLE — v4's REAL `recompute-chat-last-message-at-v1` migration
 * (`migrations/scripts/recompute-chat-last-message-at.ts`, `735d9408c`) plus its
 * REAL ledger write (`migrations/state.ts` `recordCompletedMigration`), driven
 * over the shared spec `harness/oracle/fixtures/chat-activity-heal.json`.
 *
 * Both sides build the same migration-vintage `chats` + `chat_messages` tables
 * (v4's own integration-test schema) from the spec and follow the RUNNER's exact
 * sequence: `loadMigrationState()` → `isMigrationCompleted()` → `shouldRun()` →
 * (only if true) `run()` → `recordCompletedMigration()`. The case dumps the chat
 * rows, the `migrations_state` row, the `migrations_metadata` rows and the
 * `MigrationResult`, so the v5 boot heal — which must interoperate with this very
 * ledger row in BOTH directions — can be diffed cell-for-cell.
 *
 * Two scenarios:
 *   - `mixed` — every shape from v4's integration suite in ONE instance (the
 *     walk-back past a Staff announcement, the whisper kept, the six
 *     clears-to-NULL arms, the already-correct row left alone, the never-stamped
 *     row filled, a chat with no messages at all) PLUS the `''`-systemSender
 *     seam, where the SQL mirror (`IS NULL`) and the in-memory predicate (JS
 *     truthiness) deliberately disagree. The migration is SQL, so SQL's answer is
 *     the one to reproduce — measured, not assumed. A second `shouldRun()` after
 *     the pass proves the rewrite is its own fixed point, and a second
 *     `isMigrationCompleted()` proves the runner would skip a re-run.
 *   - `no-drift` — a correct instance. v4's runner SKIPS and writes NO ledger
 *     row (the pass is retried, cheaply, every boot). `noDriftRunMessage` calls
 *     `run()` directly on the untouched DB — as v4's own integration test does —
 *     purely to pin that branch's sentence; it writes nothing.
 *
 * Harnessing follows v4's own integration test (real driver by absolute path past
 * the jest `better-sqlite3` mock; the `migrations/lib` logger/progress/
 * database-utils mocks over one shared in-memory DB — with `executeSQLite`/
 * `querySQLite` added so `state.ts` runs against the same DB).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores
 * .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-chat-activity-heal-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/chat-activity-heal.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/chat-activity-heal.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-chat-activity-heal.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "chat-activity-heal\.test\.ts$"
 */

import { describe, it, expect, jest } from '@jest/globals';
import * as fs from 'fs';
import path from 'path';

function loadDriver() {
  try {
    return require(path.join(
      process.cwd(),
      'packages',
      'quilltap',
      'node_modules',
      'better-sqlite3-multiple-ciphers'
    ));
  } catch {
    return require(path.join(process.cwd(), 'node_modules', 'better-sqlite3'));
  }
}
const Database = loadDriver();
type DatabaseInstance = ReturnType<typeof Database>;

let testDb: DatabaseInstance = null as unknown as DatabaseInstance;

jest.mock('@/migrations/lib/logger', () => ({
  logger: {
    info: jest.fn(),
    debug: jest.fn(),
    warn: jest.fn(),
    error: jest.fn(),
    child: jest.fn().mockReturnValue({
      info: jest.fn(),
      debug: jest.fn(),
      warn: jest.fn(),
      error: jest.fn(),
    }),
  },
}));

jest.mock('@/migrations/lib/progress', () => ({
  reportProgress: jest.fn(),
}));

jest.mock('@/migrations/lib/database-utils', () => ({
  isSQLiteBackend: () => true,
  sqliteTableExists: (name: string) =>
    (
      testDb
        .prepare(`SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?`)
        .get(name) as unknown
    ) !== undefined,
  getSQLiteDatabase: () => testDb,
  getSQLiteTableColumns: (name: string) =>
    testDb.prepare(`PRAGMA table_info(${name})`).all() as Array<{ name: string }>,
  executeSQLite: (sql: string) => {
    testDb.exec(sql);
  },
  querySQLite: (sql: string) => testDb.prepare(sql).all(),
}));

interface SpecChat {
  id: string;
  lastMessageAt: string | null;
}
interface SpecMessage {
  chatId: string;
  createdAt: string;
  type?: string;
  role?: string | null;
  systemSender?: string | null;
  customAnnouncer?: string | null;
}
interface Scenario {
  name: string;
  chats: SpecChat[];
  messages: SpecMessage[];
}

/** v4's own integration-test schema: the migration-vintage reduced tables. */
function buildScenario(s: Scenario): DatabaseInstance {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE chats (
      id TEXT PRIMARY KEY,
      lastMessageAt TEXT,
      createdAt TEXT NOT NULL,
      updatedAt TEXT NOT NULL
    );
    CREATE TABLE chat_messages (
      id TEXT PRIMARY KEY,
      chatId TEXT NOT NULL,
      type TEXT DEFAULT 'message',
      role TEXT,
      systemSender TEXT DEFAULT NULL,
      customAnnouncer TEXT DEFAULT NULL,
      createdAt TEXT NOT NULL
    );
  `);
  const chat = db.prepare(
    'INSERT INTO chats (id, lastMessageAt, createdAt, updatedAt) VALUES (?, ?, ?, ?)'
  );
  for (const c of s.chats) {
    chat.run(c.id, c.lastMessageAt, '2026-01-01T00:00:00.000Z', '2026-12-31T00:00:00.000Z');
  }
  const msg = db.prepare(
    'INSERT INTO chat_messages (id, chatId, type, role, systemSender, customAnnouncer, createdAt) VALUES (?, ?, ?, ?, ?, ?, ?)'
  );
  let seq = 0;
  for (const m of s.messages) {
    msg.run(
      `m${++seq}`,
      m.chatId,
      m.type ?? 'message',
      m.role === undefined ? 'ASSISTANT' : m.role,
      m.systemSender ?? null,
      m.customAnnouncer ?? null,
      m.createdAt
    );
  }
  return db;
}

describe('chat-activity-heal oracle', () => {
  it('runs the real migration + ledger over each scenario and dumps the result', async () => {
    const out = process.env.QT_ORACLE_OUT;
    if (!out) throw new Error('set QT_ORACLE_OUT');

    const spec = JSON.parse(
      fs.readFileSync(path.join(__dirname, '..', 'fixtures', 'chat-activity-heal.json'), 'utf8')
    ) as { scenarios: Scenario[] };

    const { recomputeChatLastMessageAtMigration } = await import(
      '@/migrations/scripts/recompute-chat-last-message-at'
    );
    const { loadMigrationState, isMigrationCompleted, recordCompletedMigration } = await import(
      '@/migrations/state'
    );

    const lines: string[] = [];
    for (const scenario of spec.scenarios) {
      testDb = buildScenario(scenario);

      // The runner's exact sequence.
      let state = await loadMigrationState();
      const completedBefore = isMigrationCompleted(
        state,
        'recompute-chat-last-message-at-v1'
      );
      const shouldRun = await recomputeChatLastMessageAtMigration.shouldRun();

      let result: Record<string, unknown> | null = null;
      let shouldRunAfter: boolean | null = null;
      let skippedOnRerun: boolean | null = null;
      let noDriftRunMessage: string | null = null;

      if (shouldRun) {
        const r = await recomputeChatLastMessageAtMigration.run();
        expect(r.success).toBe(true);
        result = { id: r.id, success: r.success, itemsAffected: r.itemsAffected, message: r.message };
        state = await recordCompletedMigration(state, r);
        // The rewrite is its own fixed point, and the runner would skip a re-run.
        shouldRunAfter = await recomputeChatLastMessageAtMigration.shouldRun();
        const reloaded = await loadMigrationState();
        skippedOnRerun = isMigrationCompleted(reloaded, 'recompute-chat-last-message-at-v1');
      } else {
        // v4's runner writes NOTHING here. `run()` is called only to pin the
        // no-op branch's sentence — it makes no UPDATE (drifted.length === 0).
        const r = await recomputeChatLastMessageAtMigration.run();
        noDriftRunMessage = r.message;
      }

      const chats = testDb.prepare('SELECT id, lastMessageAt FROM chats ORDER BY id').all();
      const ledgerExists =
        (testDb
          .prepare(`SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?`)
          .get('migrations_state') as unknown) !== undefined;
      const ledger = ledgerExists
        ? testDb.prepare('SELECT * FROM migrations_state ORDER BY id').all()
        : [];
      const metadata = ledgerExists
        ? testDb.prepare('SELECT * FROM migrations_metadata ORDER BY key').all()
        : [];

      lines.push(
        JSON.stringify({
          scenario: scenario.name,
          completedBefore,
          shouldRun,
          shouldRunAfter,
          skippedOnRerun,
          noDriftRunMessage,
          result,
          chats,
          ledger,
          metadata,
        })
      );
      testDb.close();
    }

    fs.writeFileSync(out, lines.join('\n') + '\n');
  });
});
