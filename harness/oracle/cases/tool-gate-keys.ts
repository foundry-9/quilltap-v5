/**
 * Oracle case (P4.D340 — v4 `lib/pascal/tool-gate-keys.ts`, `7263105c6` +
 * `a53024d3c`'s any-of walk, at `322a5488e`): `collectToolGateKeys` through
 * v4's REAL discovery code (`listAllCustomTools`) on COPIES of the title-lint
 * pair (`build-inventory-title-lint-fixture.ts`). Each op of
 * `tool-gate-keys.json` runs in its OWN child process on a fresh copy: unless
 * `tools: true`, General's keyed `Tools/measure_twice.tool.json` is deleted
 * first (linted — no line); then the op's plants run — tool files written
 * through v4's REAL `writeDatabaseDocument` inside `withInventoryWritesLinted`
 * (so both sides plant IDENTICAL rows) and raw SQL — and the census is taken,
 * recorded. Per op ONE line: `{op, census: JSON.stringify(census), logs}`.
 *
 * Run (Node 24, from the v4 checkout), AFTER the title-lint fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   QT_FIXTURE_OUT=/tmp/qt-itl-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-itl-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-inventory-title-lint-fixture.ts
 *   LOG_LEVEL=debug QT_FIXTURE_ITL_MAIN=/tmp/qt-itl-main.db QT_FIXTURE_ITL_MOUNT=/tmp/qt-itl-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/cases/tool-gate-keys.ts \
 *     > /tmp/oracle-tool-gate-keys.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';

type Json = any; // eslint-disable-line @typescript-eslint/no-explicit-any

const here = dirname(fileURLToPath(import.meta.url));
const lintSpec = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'inventory-title-lint.json'), 'utf8')) as Json;
const spec = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'tool-gate-keys.json'), 'utf8')) as Json;

async function runChild(index: number): Promise<void> {
  const op = spec.ops[index];
  const fixtureMain = process.env.QT_FIXTURE_ITL_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_ITL_MOUNT;
  if (!fixtureMain || !fixtureMount || !existsSync(fixtureMain) || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_ITL_MAIN / QT_FIXTURE_ITL_MOUNT must point at the title-lint pair');
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-tgk-op-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  copyFileSync(fixtureMain, join(scratch, 'main.db'));
  copyFileSync(fixtureMount, join(scratch, 'mount.db'));
  process.env.ENCRYPTION_MASTER_PEPPER = lintSpec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'mount.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;

  const { Logger } = await import('@/lib/logger');
  const logs: Array<{ level: string; message: string; fields: Array<[string, unknown]> }> = [];
  let recording = false;
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const original = Logger.prototype[level];
    Logger.prototype[level] = function (this: unknown, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
      if (recording) {
        logs.push({ level, message, fields: Object.entries(context ?? {}).filter(([, v]) => v !== undefined) });
      }
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  await initializeDatabase();
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const repos = getRepositories();
  const rawMount = getRawMountIndexDatabase() as Json;
  const { resolveInventoryLocation } = await import('@/lib/inventory/location');
  const { withInventoryWritesLinted } = await import('@/lib/inventory/lint-scope');
  const store = await import('@/lib/mount-index/database-store');
  const { collectToolGateKeys } = await import('@/lib/pascal/tool-gate-keys');

  const mountOf = async (scope: string, id: string | null) =>
    (await resolveInventoryLocation(scope as never, id, repos, lintSpec.userId, { ensure: false }))!.mountPointId;
  const mounts: Record<string, string> = {
    general: await mountOf('general', null),
    project: await mountOf('project', lintSpec.project.id),
    friday: await mountOf('character', lintSpec.friday.id),
  };
  const mp = (s: string) => s.replace(/\{\{mp:(\w+)\}\}/g, (_, k) => mounts[k]);

  if (!op.tools) {
    const gone = await withInventoryWritesLinted(() =>
      store.deleteDatabaseDocument(mounts.general, 'Tools/measure_twice.tool.json'),
    );
    if (!gone) throw new Error('the keyed tool was not there to strip');
  }
  for (const step of op.steps) {
    if (step.do === 'doc-plant') {
      await withInventoryWritesLinted(() => store.writeDatabaseDocument(mp(step.mp), step.path, step.content));
    } else if (step.do === 'sql') {
      const changed = rawMount.prepare(mp(step.sql)).run().changes;
      if (changed === 0) throw new Error(`plant changed no row: ${step.sql}`);
    } else {
      throw new Error(`unknown step ${step.do}`);
    }
  }

  recording = true;
  const census = await collectToolGateKeys();
  recording = false;
  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stdout.write(JSON.stringify({ op: op.name, index, mounts, census: JSON.stringify(census), logs }) + '\n');
  process.exit(0);
}

async function main(): Promise<void> {
  const only = process.env.QT_TGK_OP_INDEX;
  if (only !== undefined) return runChild(Number(only));
  for (let i = 0; i < spec.ops.length; i++) {
    const child = spawnSync(process.execPath, [...process.execArgv, fileURLToPath(import.meta.url)], {
      env: { ...process.env, QT_TGK_OP_INDEX: String(i) },
      encoding: 'utf8',
      maxBuffer: 64 * 1024 * 1024,
    });
    if (child.status !== 0 || !child.stdout.trim()) {
      process.stderr.write(`op #${i} ${spec.ops[i].name} failed:\n${child.stderr}\n`);
      process.exit(1);
    }
    process.stdout.write(child.stdout.trim().split('\n').pop() + '\n');
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`tool-gate-keys oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
