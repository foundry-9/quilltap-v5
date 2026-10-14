/**
 * Tier-2 ORACLE for v4's REAL `refreshMemoryTimeZone` at the pin
 * (`83334b9a5`, `lib/memory/time/calendar-refresh.ts` over
 * `repos.chatSettings.findAll()`) — P4.D289's stateful memory-calendar zone
 * read (`services::memory_time_zone::refresh_memory_time_zone`).
 *
 * Each scenario builds a FRESH scratch database through v4's real
 * `initializeDatabase` + `ensureCollection('chat_settings', …)` (test pepper
 * from `fixtures/chat-settings-tier2.json`), plants its rows with raw SQL,
 * copies the file to `$QT_FIXTURE_OUT_DIR/<scenario>.db` (the state BEFORE the
 * first refresh — the Rust side opens its own copy of that file), then runs
 * its steps: each step applies an optional SQL plant and calls
 * `refreshMemoryTimeZone(nowMs)`. The cache is reset per scenario.
 * `QUILLTAP_TIMEZONE` is pinned to `America/Santiago` (the env rung, which
 * the Rust side passes as `host_fallback`), the process runs under `TZ=UTC`.
 *
 * Per step it records the effective zone and the captured log lines (every
 * line, the logger's shared transports spliced — child loggers included).
 *
 * Scenarios: no rows; the first row invalid and the second valid; a NULL
 * timezone before a valid case-folded one; the TTL (`+299_999` cached,
 * `+300_000` re-read after a planted change); a row v4's whole-row
 * `ChatSettingsSchema` refuses (`MEMORY_ZONE_ROW_UNVALIDATED` — v4 skips it,
 * v5 does not re-run the schema); a read failure planted ON the `timezone`
 * column (a VIRTUAL `json('not json')`, so both sides' reads throw); and that
 * failure AFTER a successful read (v4's repository fallback answers `[]`, so
 * the cached zone is CLEARED, not kept).
 *
 * Run (Node 24, from the v4 checkout or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT_DIR=/tmp/qt-memory-time-zone-refresh TZ=UTC \
 *     $N/npx tsx "$V5W/harness/oracle/cases/memory-time-zone-refresh-tier2.ts" > /tmp/oracle-memory-time-zone-refresh.ndjson
 */

import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

interface Step {
  /** SQL statements applied before this step's refresh (`'__THROWING_TABLE__'` names the plant below). */
  plant?: string[] | '__THROWING_TABLE__';
  nowMs: number;
}
interface Scenario {
  name: string;
  /** SQL statements run once, before the copy, after v4 created the table. */
  setup: () => Promise<string[]> | string[];
  steps: Step[];
}

const T0 = Date.parse('2026-07-01T12:00:00.000Z');
const TS = '2026-01-01T00:00:00.000Z';
/** A v4-valid row id / user id (`z.uuid()` refuses anything else). */
const uuid = (n: number) => `00000000-0000-4000-8000-${String(n).padStart(12, '0')}`;
const row = (n: number, tz: string | null, avatarDisplayMode?: string) =>
  `INSERT INTO chat_settings (id, userId, createdAt, updatedAt, timezone${avatarDisplayMode ? ', avatarDisplayMode' : ''}) ` +
  `VALUES ('${uuid(n)}', '${uuid(100 + n)}', '${TS}', '${TS}', ${tz === null ? 'NULL' : `'${tz}'`}${avatarDisplayMode ? `, '${avatarDisplayMode}'` : ''})`;

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'chat-settings-tier2.json'), 'utf8')) as {
    testPepperBase64: string;
  };
  const outDir = process.env.QT_FIXTURE_OUT_DIR;
  if (!outDir) throw new Error('QT_FIXTURE_OUT_DIR must name the directory for the scenario .db files');
  mkdirSync(outDir, { recursive: true });

  process.env.LOG_LEVEL = 'debug';
  process.env.QUILLTAP_TIMEZONE = 'America/Santiago';
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  delete process.env.SQLITE_WAL_MODE;
  const scratch = mkdtempSync(join(tmpdir(), 'qt-memory-time-zone-refresh-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.QUILLTAP_DATA_DIR = scratch;
  process.env.SQLITE_PATH = join(scratch, 'boot.db');

  const { logger } = await import('@/lib/logger');
  const captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown>; error?: { message: string } };
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      if (data.error) fields.error = data.error.message;
      captured.push({ level: data.level, message: data.message, fields });
    },
  });

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { ChatSettingsSchema } = await import('@/lib/schemas/settings.types');
  const { refreshMemoryTimeZone } = await import('@/lib/memory/time/calendar-refresh');
  const { resetMemoryTimeZoneCacheForTests } = await import('@/lib/memory/time/calendar');

  // A read failure ON the `timezone` column: the row is written first, then
  // the column is swapped for a VIRTUAL one whose every read throws
  // (`malformed JSON`) — both v4's `SELECT *` and v5's one-column read.
  const throwingTable = (): string[] => [
    `INSERT INTO chat_settings (id, userId, createdAt, updatedAt) VALUES ('${uuid(99)}', '${uuid(199)}', '${TS}', '${TS}')`,
    'ALTER TABLE chat_settings DROP COLUMN "timezone"',
    `ALTER TABLE chat_settings ADD COLUMN "timezone" TEXT GENERATED ALWAYS AS (json('not json')) VIRTUAL`,
  ];

  const scenarios: Scenario[] = [
    { name: 'no-rows', setup: () => [], steps: [{ nowMs: T0 }] },
    {
      name: 'first-invalid-second-valid',
      setup: () => [row(1, 'Mars/Olympus'), row(2, 'Asia/Kolkata')],
      steps: [{ nowMs: T0 }],
    },
    {
      name: 'null-then-folded-link',
      setup: () => [row(1, null), row(2, ''), row(3, 'us/central')],
      steps: [{ nowMs: T0 }],
    },
    {
      name: 'ttl',
      setup: () => [row(1, 'Europe/London')],
      steps: [
        { nowMs: T0 },
        { plant: [`UPDATE chat_settings SET timezone = 'Asia/Kolkata' WHERE id = '${uuid(1)}'`], nowMs: T0 + 299_999 },
        { nowMs: T0 + 300_000 },
      ],
    },
    {
      name: 'row-unvalidated',
      setup: () => [row(1, 'Asia/Kolkata', 'BOGUS')],
      steps: [{ nowMs: T0 }],
    },
    { name: 'read-failure', setup: throwingTable, steps: [{ nowMs: T0 }] },
    {
      name: 'read-failure-after-success',
      setup: () => [row(1, 'Europe/London')],
      steps: [{ nowMs: T0 }, { plant: '__THROWING_TABLE__', nowMs: T0 + 300_000 }],
    },
  ];

  for (const sc of scenarios) {
    const file = join(scratch, `${sc.name}.db`);
    process.env.SQLITE_PATH = file;
    await initializeDatabase();
    await ensureCollection('chat_settings', ChatSettingsSchema);
    const setupSql = await sc.setup();
    for (const stmt of setupSql) await rawQuery(stmt);
    await closeDatabase();
    const out = join(outDir, `${sc.name}.db`);
    for (const suffix of ['', '-journal', '-wal', '-shm']) if (existsSync(out + suffix)) rmSync(out + suffix);
    copyFileSync(file, out);
    await initializeDatabase();
    await ensureCollection('chat_settings', ChatSettingsSchema);

    resetMemoryTimeZoneCacheForTests();
    const steps: unknown[] = [];
    for (const step of sc.steps) {
      const plant = step.plant === '__THROWING_TABLE__' ? throwingTable() : step.plant;
      for (const stmt of plant ?? []) await rawQuery(stmt);
      captured.length = 0;
      const effective = await refreshMemoryTimeZone(step.nowMs);
      steps.push({ plant: plant ?? null, nowMs: step.nowMs, effective, logs: captured.map((c) => ({ ...c })) });
    }
    await closeDatabase();
    process.stdout.write(
      JSON.stringify({ scenario: sc.name, setup: setupSql, hostFallback: process.env.QUILLTAP_TIMEZONE, steps }) + '\n',
    );
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
