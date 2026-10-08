/**
 * @jest-environment node
 *
 * P4.D256 WARDROBE-WEAR-HISTORY ORACLE (helper level): drives v4's REAL
 * `lib/wardrobe/wear-history.ts` exports — `attachWear`, `resolveWearers`,
 * `buildWearHistoryPayload` (v4 `3ee3b1342`) — over a FRESH copy of the
 * committed `wardrobe-routes-{main,mount}.db` pair per case, and records each
 * result with the lines v4 logged while it ran. The route-level wear-history
 * cases (the 404-before-ledger order) live in the route corpora; this family
 * pins the helpers themselves: the pre-round shape (no ledger table — v4's
 * `findSummaries` / `findHistory` fallbacks), the planted ledger, the three
 * wearer labels, the avatar leg, a deleted last-worn chat, and R-F's
 * measurement (a corrupt `characters` row: does `findByIdRaw` throw — the
 * `Could not read wearer` WARN — or answer `null`?).
 *
 * The corpus is `wardrobe-wear-history.json#cases`, shared with the Rust
 * differential (`wardrobe_wear_history_equivalence`). A case with `ledger:
 * true` gets `wardrobe_wear_stats` from v4's OWN migration statements
 * (`WARDROBE_WEAR_STATS_DDL`, what `add-wardrobe-wear-stats-table-v1` runs);
 * the v5 side runs `test_support::ensure_wear_ledger_on`. `plants` are raw
 * SQL run on the main copy (both sides), BEFORE the database initializes.
 *
 * Each row: `{ name, out, logs }` — `logs` every WARN / ERROR and the two
 * wear-history DEBUG lines, `{ level, message, fields: [[k, String(v)]] }`
 * with `error` omitted (each driver's own text) and `undefined` dropped.
 *
 * Run (Node 24, from the pin; cp to a /tmp mirror because jest ignores
 * .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-wwh-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/wardrobe-wear-history.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/wardrobe-wear-history.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server   # (or the pinned worktree if v4 has drifted)
 *   QT_FIXTURE_WROUTES_MAIN=$V5W/crates/quilltap-web/tests/fixtures/wardrobe-routes-main.db \
 *   QT_FIXTURE_WROUTES_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/wardrobe-routes-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-wardrobe-wear-history.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- wardrobe-wear-history
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CaseEntry {
  name: string;
  kind: 'attachWear' | 'resolveWearers' | 'buildWearHistoryPayload';
  ledger?: boolean;
  plants?: Array<{ sql: string; params: Array<string | number | null> }>;
  items?: unknown[];
  wearers?: Array<{ characterId: string | null }>;
  avatars?: boolean;
  itemId?: string;
}

interface Spec {
  testPepperBase64: string;
  cases: CaseEntry[];
}

/** The DEBUG lines this family compares (every WARN / ERROR is compared). */
const DEBUG_MESSAGES = new Set([
  'Attached wear summaries to wardrobe read',
  'Built wear history',
]);

type LogRow = { level: string; message: string; fields: Array<[string, string]> };

function plantMain(spec: Spec, c: CaseEntry, mainWork: string, ddl: readonly string[]): void {
  if (!c.ledger && !(c.plants && c.plants.length)) return;
  const { createRequire } = require('node:module');
  const nodeRequire = createRequire(join(process.cwd(), 'noop.js'));
  const Database = nodeRequire(
    join(process.cwd(), 'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'),
  );
  const conn = new Database(mainWork);
  conn.pragma(`key = "x'${Buffer.from(spec.testPepperBase64, 'base64').toString('hex')}'"`);
  try {
    if (c.ledger) for (const statement of ddl) conn.exec(statement);
    for (const p of c.plants ?? []) conn.prepare(p.sql).run(...p.params);
  } finally {
    conn.close();
  }
}

async function runCase(
  spec: Spec,
  c: CaseEntry,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  const cipherDriverPath = join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

  const work = mkdtempSync(join(scratch, 'wwh-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  const { WARDROBE_WEAR_STATS_DDL } = (await import(
    '@/lib/database/backends/sqlite/wardrobe-wear-stats-ddl'
  )) as { WARDROBE_WEAR_STATS_DDL: readonly string[] };
  plantMain(spec, c, mainWork, WARDROBE_WEAR_STATS_DDL);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { closeDatabase, initializeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();

  const { Logger } = await import('@/lib/logger');
  const logs: LogRow[] = [];
  let armed = false;
  const originals: Record<string, unknown> = {};
  for (const level of ['error', 'warn', 'debug'] as const) {
    const original = Logger.prototype[level];
    originals[level] = original;
    Logger.prototype[level] = function (
      this: unknown,
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      if (armed && (level !== 'debug' || DEBUG_MESSAGES.has(message))) {
        const fields: Array<[string, string]> = [];
        for (const [k, v] of Object.entries(context ?? {})) {
          if (k === 'error' || v === undefined) continue;
          fields.push([k, String(v)]);
        }
        logs.push({ level, message, fields });
      }
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }

  try {
    const { getRepositories } = await import('@/lib/repositories/factory');
    const repos = getRepositories();
    const wh = (await import('@/lib/wardrobe/wear-history')) as {
      attachWear: (items: unknown[], repos: unknown) => Promise<unknown[]>;
      resolveWearers: (
        w: Array<{ characterId: string | null }>,
        repos: unknown,
        opts: { avatars?: boolean },
      ) => Promise<unknown[]>;
      buildWearHistoryPayload: (itemId: string, repos: unknown) => Promise<unknown>;
    };
    armed = true;
    let out: unknown;
    switch (c.kind) {
      case 'attachWear':
        out = await wh.attachWear(c.items ?? [], repos);
        break;
      case 'resolveWearers':
        out = await wh.resolveWearers(c.wearers ?? [], repos, { avatars: c.avatars });
        break;
      case 'buildWearHistoryPayload':
        out = await wh.buildWearHistoryPayload(c.itemId ?? '', repos);
        break;
      default:
        throw new Error(`unknown kind ${(c as CaseEntry).kind}`);
    }
    armed = false;
    return { name: c.name, out, logs };
  } finally {
    for (const level of ['error', 'warn', 'debug'] as const) {
      Logger.prototype[level] = originals[level] as never;
    }
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'wardrobe-wear-history.json'), 'utf8'),
  ) as Spec;
  const fixtures = {
    main: process.env.QT_FIXTURE_WROUTES_MAIN ?? '',
    mount: process.env.QT_FIXTURE_WROUTES_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wwh-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];
  for (const c of spec.cases) {
    outLines.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(
    `wardrobe-wear-history oracle wrote ${outPath} (${outLines.length} rows)\n`,
  );
}

test('wardrobe-wear-history oracle', async () => {
  await main();
});

const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
