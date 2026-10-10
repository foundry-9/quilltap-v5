/**
 * @jest-environment node
 *
 * Tier-2 ORACLE (P4.D268): v4's frozen memory archive at `01a83539d` —
 * `lib/memory/frozen-archive-cache.ts` (`3f7320138` F5's (character, chat) key,
 * generation + size + presence freshness, the LRU bound of 64;
 * `d58548051`'s digests-first composition) and the job dispatcher's REAL
 * `invalidateFrozenArchivesForCompletedJob` (`job-dispatcher.ts:671-695`),
 * over a REAL repository on a COPY of the committed `memories-{main,mount}.db`
 * pair.
 *
 * The copy gets v4's REAL `add-memory-tiers-v1` (idempotent on the widened
 * pair — §R.3's belt), then the spec's `plantSql` (explicit uuids and
 * timestamps; the Rust side runs the SAME statements). The spec's `ops` run in
 * order against ONE process-global cache (reset at case start). Each op emits
 * `{ i, kind, returned, lines }`: the archive's ids in ORDER for a `get`, and
 * every `[FrozenArchive] …` / `Invalidated frozen memory archives after job`
 * line logged during the op (a transport spliced into the singleton logger's
 * SHARED transport array, so the dispatcher's `jobs:dispatcher` child is
 * captured too; the child's `module` context and the logger's
 * `service`/`environment` are dropped — v5 has no such fields).
 *
 * Only `Date` is frozen (constructor + `now`, the `almanack-routes` idiom) —
 * `calculateEffectiveWeight` reads the wall clock; the Rust side passes the
 * same `nowMs`. `../processor-host` is mocked exactly as v4's own
 * `job-dispatcher-frozen-archive.test.ts` mocks it.
 *
 * Run (Node 24, from the v4 checkout or a pinned worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-frozen-archive-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/frozen-archive-tier2.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/frozen-archive-tier2.json" "$TMPO/fixtures/"
 *   cp "$V5W/harness/oracle/fixtures/apply-memory-tiers-migration.ts" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   TZ=UTC QT_FIXTURE_MEM_MAIN=$V5W/crates/quilltap-web/tests/fixtures/memories-main.db \
 *   QT_FIXTURE_MEM_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/memories-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-frozen-archive-tier2.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- frozen-archive-tier2
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { applyMemoryTiersMigration } from '../fixtures/apply-memory-tiers-migration';

interface Op {
  kind: 'get' | 'invalidate' | 'invalidateAll' | 'hook';
  characterId?: string;
  chatId?: string;
  generation?: number;
  size?: number;
  present?: string[];
  jobId?: string;
  type?: string;
  payload?: Record<string, unknown>;
}
interface Spec {
  testPepperBase64: string;
  nowMs: number;
  plantSql: string[];
  ops: Op[];
}

const CAPTURED_PREFIXES = ['[FrozenArchive] ', 'Invalidated frozen memory archives after job', 'Failed to invalidate frozen memory archives after job'];

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'frozen-archive-tier2.json'), 'utf8'),
  ) as Spec;
  const fixtureMain = process.env.QT_FIXTURE_MEM_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_MEM_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_MEM_MAIN and QT_FIXTURE_MEM_MOUNT must point at the committed pair');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-frozen-archive-oracle-'));
  try {
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'main.db');
    const mountWork = join(scratch, 'mount.db');
    copyFileSync(fixtureMain, mainWork);
    copyFileSync(fixtureMount, mountWork);

    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'debug';

    await applyMemoryTiersMigration(mainWork, { pepperBase64: spec.testPepperBase64 });
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

    jest.resetModules();
    const cipherDriverPath = require('node:path').join(
      process.cwd(),
      'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
    );
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
    // As v4's own job-dispatcher-frozen-archive.test.ts does.
    jest.doMock('@/lib/background-jobs/host/processor-host', () => ({
      sendToChild: jest.fn(() => true),
      notifyChild: jest.fn(),
    }));

    // The lines: one transport spliced into the singleton's SHARED array (the
    // dispatcher's child logger writes through the same array).
    const { logger } = await import('@/lib/logger');
    let captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
    const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
    internal.transports.splice(0, internal.transports.length, {
      write: (d: unknown) => {
        const data = d as { level: string; message: string; context?: Record<string, unknown> };
        if (!CAPTURED_PREFIXES.some((p) => data.message.startsWith(p))) return;
        const { service: _s, environment: _e, module: _m, ...fields } = data.context ?? {};
        captured.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
      },
    });

    const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
    await initializeDatabase();
    for (const sql of spec.plantSql) await rawQuery(sql);

    const cache = await import('@/lib/memory/frozen-archive-cache');
    const { invalidateFrozenArchivesForCompletedJob } = await import(
      '@/lib/background-jobs/host/job-dispatcher'
    );

    // Freeze ONLY Date (constructor + now).
    const RealDate = Date;
    (global as { Date: DateConstructor }).Date = class extends RealDate {
      constructor(...args: unknown[]) {
        if (args.length === 0) super(spec.nowMs);
        // @ts-expect-error variadic forwarding
        else super(...args);
      }
      static now(): number {
        return spec.nowMs;
      }
    } as DateConstructor;

    cache.resetFrozenArchiveCacheForTests();
    const lines: string[] = [];
    try {
      for (const [i, op] of spec.ops.entries()) {
        captured = [];
        let returned: string[] | null = null;
        switch (op.kind) {
          case 'get': {
            const options: { size?: number; presentCharacterIds?: string[] } = {};
            if (op.size !== undefined) options.size = op.size;
            if (op.present !== undefined) options.presentCharacterIds = op.present;
            const archive = await cache.getOrComputeFrozenArchive(
              op.characterId as string,
              op.chatId as string,
              op.generation as number,
              options,
            );
            returned = archive.map((m) => m.id);
            break;
          }
          case 'invalidate':
            cache.invalidateFrozenArchive(op.characterId as string);
            break;
          case 'invalidateAll':
            cache.invalidateAllFrozenArchives();
            break;
          case 'hook':
            await invalidateFrozenArchivesForCompletedJob({
              id: op.jobId,
              type: op.type,
              payload: op.payload,
            } as never);
            break;
          default:
            throw new Error(`unknown op ${(op as Op).kind}`);
        }
        lines.push(JSON.stringify({ i, kind: op.kind, returned, lines: captured }));
      }
    } finally {
      (global as { Date: DateConstructor }).Date = RealDate;
    }

    await closeDatabase();
    fs.writeFileSync(outPath, lines.join('\n') + '\n');
    process.stderr.write(`frozen-archive-tier2 oracle wrote ${outPath} (${lines.length} ops)\n`);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

test('frozen-archive tier-2 oracle', async () => {
  await main();
});
