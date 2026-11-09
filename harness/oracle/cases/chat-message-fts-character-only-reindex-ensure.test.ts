/**
 * @jest-environment node
 *
 * P4.D363 — the character-only re-index ORACLE: v4's REAL
 * `reindex-chat-message-fts-character-only-v1` migration (`migrations/scripts/
 * reindex-chat-message-fts-character-only.ts`, `902c6efe9`) plus its REAL
 * ledger (`migrations/state.ts`) and its REAL DDL module
 * (`lib/database/backends/sqlite/chat-message-fts.ts`), driven over the shared
 * spec `harness/oracle/fixtures/chat-message-fts-character-only-reindex-ensure.json`
 * at `624d0d25c`. The `scenario-seeded-summary-heal.test.ts` harnessing, whole.
 *
 * Each scenario builds the reduced `chat_messages` (v4's own reduced DDL, with
 * `systemSender` / `customAnnouncer`), v4's ensure (unless `noFts`), the
 * `damage` SQL (a legacy `_ai`, a dropped trigger, …), the `seed` rows — so a
 * legacy trigger INDEXES the Staff rows, as on a pre-round instance — and the
 * `afterSeed` SQL. Then the RUNNER's exact sequence: `loadMigrationState()` →
 * `isMigrationCompleted()` → `shouldRun()` → `run()` →
 * `recordCompletedMigration()`. A throw from `run()` is recorded as `error`
 * (v4's runner lists it as failed and stops the boot — the migration is not
 * `resumable`); a throw from `shouldRun()` as `shouldRunError` (the runner
 * skips it).
 *
 * `createFirst` (the coordinator's ruling, 2026-11-09): v4's REAL
 * `create-chat-message-fts-v1` is taken through the same runner sequence
 * FIRST — its `dependsOn` order — on a base with no FTS objects (a fresh
 * instance), and its own path / ledger row recorded as `create`. The question
 * it answers: does v4 leave the reindex stamped on a fresh instance?
 *
 * Paths: `already-completed`, `not-applicable` (no `chat_messages` —
 * `sqliteTableExists` false, which is `shouldRun` false), `not-needed`
 * (`shouldRun` false with the table present), `ran`, `threw`.
 *
 * Comparands: `path`, `shouldRun`, `error`, the result's `itemsAffected` /
 * `message`, the `chat_messages_fts_map` rows (by `ftsId`), the five objects'
 * `sqlite_master` text AFTER, `migrations_state` / `migrations_metadata`
 * (`completedAt` / `quilltapVersion` / `lastChecked` normalized — the id is
 * the key), and `logs`: the migration's two INFO lines (the mocked
 * `migrations/lib/logger`) and the DDL module's debug lines (a spy on the bare
 * `@/lib/logger`), in order.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores
 * .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-fts-reindex-ensure-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/chat-message-fts-character-only-reindex-ensure.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/chat-message-fts-character-only-reindex-ensure.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-fts-reindex-ensure.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "chat-message-fts-character-only-reindex-ensure\.test\.ts$"
 */

import { describe, it, jest } from '@jest/globals';
import * as fs from 'fs';
import path from 'path';

// Real binding by absolute root path — a bare or nested require resolves to
// the jest mock, which returns empty result sets.
// eslint-disable-next-line @typescript-eslint/no-var-requires
const Database = require(path.join(process.cwd(), 'node_modules', 'better-sqlite3'));

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let testDb: any = null;

interface LogLine {
  source: 'migration' | 'module';
  level: string;
  message: string;
  context: unknown;
}
const mockLines: LogLine[] = [];

jest.mock('@/migrations/lib/logger', () => {
  const at = (level: string) => (message: string, context?: unknown) =>
    mockLines.push({ source: 'migration', level, message, context: context ?? null });
  const l = { info: at('info'), debug: at('debug'), warn: at('warn'), error: at('error') };
  return { logger: { ...l, child: () => l } };
});

jest.mock('@/migrations/lib/progress', () => ({
  reportProgress: jest.fn(),
}));

jest.mock('@/migrations/lib/database-utils', () => ({
  isSQLiteBackend: () => true,
  sqliteTableExists: (name: string) =>
    testDb.prepare(`SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?`).get(name) !==
    undefined,
  getSQLiteDatabase: () => testDb,
  getSQLiteTableColumns: (name: string) => testDb.prepare(`PRAGMA table_info(${name})`).all(),
  executeSQLite: (sql: string) => {
    testDb.exec(sql);
  },
  querySQLite: (sql: string) => testDb.prepare(sql).all(),
}));

const MIGRATION_ID = 'reindex-chat-message-fts-character-only-v1';

interface Seed {
  id: string;
  role: string;
  content: string;
  systemSender?: string;
  customAnnouncer?: string;
}
interface Scenario {
  name: string;
  priorLedger?: boolean;
  noChatMessages?: boolean;
  noFts?: boolean;
  damage?: string[];
  seed: Seed[];
  afterSeed?: string[];
  createFirst?: boolean;
}

function errMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

describe('P4.D363 chat-message FTS character-only reindex oracle', () => {
  it('runs the real migration + ledger over each scenario', async () => {
    const out = process.env.QT_ORACLE_OUT;
    if (!out) throw new Error('set QT_ORACLE_OUT');
    const spec = JSON.parse(
      fs.readFileSync(
        path.join(__dirname, '..', 'fixtures', 'chat-message-fts-character-only-reindex-ensure.json'),
        'utf8',
      ),
    ) as { chatMessagesDdl: string; scenarios: Scenario[] };

    const { reindexChatMessageFtsCharacterOnlyMigration: migration } = await import(
      '@/migrations/scripts/reindex-chat-message-fts-character-only'
    );
    const { createChatMessageFtsMigration: createMigration } = await import(
      '@/migrations/scripts/create-chat-message-fts'
    );
    const { loadMigrationState, isMigrationCompleted, recordCompletedMigration } = await import(
      '@/migrations/state'
    );
    const { ensureChatMessageFtsSchema, chatMessageFtsObjectNames } = await import(
      '@/lib/database/backends/sqlite/chat-message-fts'
    );
    const { registerTextCodecFunction } = await import(
      '@/lib/database/backends/sqlite/text-codec-function'
    );
    const { logger } = await import('@/lib/logger');
    for (const level of ['debug', 'info', 'warn', 'error'] as const) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (jest.spyOn(logger as any, level) as any).mockImplementation(
        (message: string, context?: unknown) => {
          mockLines.push({ source: 'module', level, message, context: context ?? null });
        },
      );
    }

    const tableExists = (name: string) =>
      testDb.prepare(`SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?`).get(name) !==
      undefined;

    const lines: string[] = [];
    for (const scenario of spec.scenarios) {
      testDb = new Database(':memory:');
      registerTextCodecFunction(testDb);
      if (!scenario.noChatMessages) {
        testDb.exec(spec.chatMessagesDdl);
        if (!scenario.noFts) ensureChatMessageFtsSchema(testDb);
        for (const sql of scenario.damage ?? []) testDb.exec(sql);
        const ins = testDb.prepare(
          'INSERT INTO "chat_messages" ("id","chatId","type","role","content","systemSender","customAnnouncer","createdAt") VALUES (?,?,?,?,?,?,?,?)',
        );
        for (const r of scenario.seed) {
          ins.run(
            r.id,
            'chat-1',
            'message',
            r.role,
            r.content,
            r.systemSender ?? null,
            r.customAnnouncer ?? null,
            `2026-01-01T00:00:${r.id.slice(-2)}.000Z`,
          );
        }
        for (const sql of scenario.afterSeed ?? []) testDb.exec(sql);
      }

      if (scenario.priorLedger) {
        const prior = await loadMigrationState();
        await recordCompletedMigration(prior, {
          id: MIGRATION_ID,
          success: true,
          itemsAffected: 2,
          message: 'Removed 2 Staff messages from the search index',
          durationMs: 0,
          timestamp: '2026-11-01T00:00:00.000Z',
        });
      }
      // The ruling's measurement: v4's create migration FIRST, the runner's
      // sequence (ledger → shouldRun → run → record).
      let create: Record<string, unknown> | null = null;
      if (scenario.createFirst) {
        const st = await loadMigrationState();
        if (isMigrationCompleted(st, createMigration.id)) {
          create = { path: 'already-completed' };
        } else if (!(await createMigration.shouldRun())) {
          create = { path: 'not-needed' };
        } else {
          const r = await createMigration.run();
          await recordCompletedMigration(st, r);
          create = { path: 'ran', itemsAffected: r.itemsAffected, message: r.message };
        }
      }
      mockLines.length = 0;

      // The runner's exact sequence.
      let state = await loadMigrationState();
      let pathTaken: string;
      let shouldRun: boolean | null = null;
      let shouldRunError: string | null = null;
      let error: string | null = null;
      let result: Record<string, unknown> | null = null;
      if (isMigrationCompleted(state, MIGRATION_ID)) {
        pathTaken = 'already-completed';
      } else {
        try {
          shouldRun = await migration.shouldRun();
        } catch (err) {
          shouldRunError = errMessage(err);
        }
        if (shouldRunError !== null) {
          pathTaken = 'should-run-threw';
        } else if (!shouldRun) {
          pathTaken = tableExists('chat_messages') ? 'not-needed' : 'not-applicable';
        } else {
          try {
            const r = await migration.run();
            result = { itemsAffected: r.itemsAffected, message: r.message };
            state = await recordCompletedMigration(state, r);
            pathTaken = 'ran';
          } catch (err) {
            error = errMessage(err);
            pathTaken = 'threw';
          }
        }
      }
      const logs = [...mockLines];

      const map = tableExists('chat_messages_fts_map')
        ? testDb.prepare('SELECT "ftsId", "messageId" FROM "chat_messages_fts_map" ORDER BY "ftsId"').all()
        : null;
      const names = chatMessageFtsObjectNames();
      const master = testDb
        .prepare(
          `SELECT name, type, sql FROM sqlite_master WHERE name IN (${names.map(() => '?').join(',')}) ORDER BY name`,
        )
        .all(...names);
      const ledgerExists = tableExists('migrations_state');
      const ledger = ledgerExists
        ? testDb
            .prepare('SELECT id, itemsAffected, message FROM migrations_state ORDER BY id')
            .all()
        : null;
      const metadata = tableExists('migrations_metadata')
        ? testDb.prepare('SELECT key FROM migrations_metadata ORDER BY key').all()
        : null;

      lines.push(
        JSON.stringify({
          scenario: scenario.name,
          path: pathTaken,
          create,
          shouldRun,
          shouldRunError,
          error,
          result,
          logs,
          map,
          master,
          ledger,
          metadata,
        }),
      );
      testDb.close();
    }

    fs.writeFileSync(out, lines.join('\n') + '\n');
  });
});
