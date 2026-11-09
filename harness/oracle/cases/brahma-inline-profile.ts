/**
 * P4.D374 ORACLE — v4 `c539c82e0`'s REAL `resolveInlineBrahmaConnectionProfile`
 * (`lib/services/brahma-console/inline-profile.ts`) over a COPY of the
 * `build-brahma-inline-profile-fixture.ts` seed. Per case (in order, ONE copy):
 * write `instance_settings['brahmaConsole']` raw (or delete the row when
 * `setting` is null), resolve for `(userId, chatId)`, and emit
 * `{ name, profileId, lines }` — `profileId` the resolved profile's id (or
 * null), `lines` every `BrahmaInlineProfile` service line (level, message,
 * context in v4's key order), captured off the `Logger` prototype (child
 * loggers share it; the line is matched on the child's `service` binding).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_BIP_MAIN=/tmp/qt-bip-main.db QT_FIXTURE_BIP_MOUNT=/tmp/qt-bip-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/cases/brahma-inline-profile.ts > /tmp/oracle-brahma-inline-profile.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { copyFileSync, mkdtempSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  users: Record<string, string>;
  cases: Array<{ name: string; user: string; chatId: string; setting: string | null }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'brahma-inline-profile.json'), 'utf8'),
  ) as Spec;
  const fixMain = process.env.QT_FIXTURE_BIP_MAIN;
  const fixMount = process.env.QT_FIXTURE_BIP_MOUNT;
  if (!fixMain || !fixMount) throw new Error('QT_FIXTURE_BIP_MAIN / QT_FIXTURE_BIP_MOUNT unset');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-bip-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'main.db');
  const workMount = join(scratch, 'mount.db');
  copyFileSync(fixMain, workMain);
  copyFileSync(fixMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { Logger } = (await import('@/lib/logger')) as unknown as {
    Logger: { prototype: Record<string, (...a: unknown[]) => unknown> };
  };
  let sink: Array<Record<string, unknown>> = [];
  for (const level of ['debug', 'info', 'warn', 'error']) {
    const original = Logger.prototype[level];
    Logger.prototype[level] = function (this: { context?: Record<string, unknown> }, message: unknown, context?: unknown, ...rest: unknown[]) {
      if (this?.context?.service === 'BrahmaInlineProfile') {
        sink.push({ level, message, context: context ?? null });
      }
      return original.call(this, message, context, ...rest);
    };
  }

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { resolveInlineBrahmaConnectionProfile } = await import(
    '@/lib/services/brahma-console/inline-profile'
  );
  await initializeDatabase();
  const repos = getRepositories();

  for (const c of spec.cases) {
    await rawQuery('DELETE FROM "instance_settings" WHERE "key" = ?', ['brahmaConsole']);
    if (c.setting !== null) {
      await rawQuery('INSERT INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
        'brahmaConsole',
        c.setting,
      ]);
    }
    sink = [];
    const profile = (await resolveInlineBrahmaConnectionProfile(
      repos as never,
      spec.users[c.user],
      c.chatId,
    )) as { id?: string } | null;
    process.stdout.write(
      JSON.stringify({ name: c.name, profileId: profile?.id ?? null, lines: sink }) + '\n',
    );
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`brahma-inline-profile oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
