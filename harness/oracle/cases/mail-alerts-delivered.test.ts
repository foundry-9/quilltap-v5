/**
 * @jest-environment node
 *
 * Differential ORACLE (tier-2, real DB) for the deferred mail `alerted` flip —
 * v4 bug 227 (`69d38dd60`): `markMailAlertsDelivered(owed, { chatId, via })`
 * from `lib/post-office/mailbox.ts`, driven through v4's REAL mailbox over the
 * shared Post Office fixture (`build-mail-carina-tools-fixture.ts` — the
 * READER vault's letters: one unalerted, one already alerted, one blank, …),
 * ONE fresh fixture copy per scenario (the flip WRITES).
 *
 * Mirrors `lib/post-office/__tests__/mailbox.test.ts`'s
 * `markMailAlertsDelivered` cases on a real store: nothing owed (`null` and an
 * empty `paths`), every owed letter marked, a vanished letter (`markAlerted:
 * letter no longer present` — still counted marked, v4's `markAlerted`
 * returns normally there), a per-letter write failure (a planted
 * `BEFORE INSERT` trigger that aborts every new document row — v4 stores
 * content by address, so a changed letter is a NEW row: each letter's
 * WARN with the bare SQLite message, `marked` 0, the turn never throws), an
 * unreadable store (a planted column rename: v4's repository fallback answers
 * the read as absent, so each letter takes the NOT_FOUND arm — the fallback's
 * ERROR, then `markAlerted`'s WARN, still counted marked), and an
 * already-alerted letter re-marked. Each record carries:
 *   - `logs` — every DEBUG / WARN / ERROR v4 logged during the call, recorded
 *     off the `Logger` prototype with the context keys in v4's order (the
 *     `mail-tools.test.ts` plant recipe);
 *   - `unalerted` — `collectUnalertedMail(readerVault)`'s paths afterwards;
 *   - `contents` — each touched path's stored content afterwards (null when
 *     absent), byte-for-byte.
 *
 * Paths are the fixture's own (`readerPaths` in the fixture's `.meta.json`);
 * the record carries them so the Rust side replays the same owed list. No
 * minted id surfaces in a comparand, and no clock is read by the flip.
 *
 * Run (Node 24, from the pin; STAGE outside any .claude path — jest ignores
 * `.claude/`):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   STAGE=/tmp/qt-mail-alerts-stage
 *   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
 *   cp $V5W/harness/oracle/cases/mail-alerts-delivered.test.ts $STAGE/harness/oracle/cases/
 *   cp $V5W/harness/oracle/fixtures/mail-carina-tools.json $STAGE/harness/oracle/fixtures/
 *   cd <the v4 pin>
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-mailalerts-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-mailalerts-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-mail-carina-tools-fixture.ts
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-mailalerts-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-mailalerts-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-mail-alerts-delivered.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" \
 *     -- "mail-alerts-delivered\.test\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
}

type Owed = 'null' | { paths: string[] };

interface Scenario {
  label: string;
  via: string;
  owed: Owed;
  /**
   * Planted before the call: `documents` renames the content column (every
   * read fails into the repository fallback); `trigger` aborts every
   * new document row (reads work, the write fails).
   */
  plant?: 'documents' | 'trigger';
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'mail-carina-tools.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_TMP_MAIN;
  const mountFixture = process.env.QT_FIXTURE_TMP_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_TMP_MAIN and QT_FIXTURE_TMP_MOUNT must point at the seeded fixtures');
  }
  const meta = JSON.parse(readFileSync(mainFixture + '.meta.json', 'utf8')) as {
    readerVault: string;
    readerPaths: Record<string, string>;
  };
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );

  const P = meta.readerPaths;
  const scenarios: Scenario[] = [
    { label: 'nothing_owed_null', via: 'finalizer', owed: 'null' },
    { label: 'nothing_owed_empty', via: 'finalizer', owed: { paths: [] } },
    { label: 'all_marked', via: 'finalizer', owed: { paths: [P.unalerted, P.blank] } },
    { label: 'vanished_letter', via: 'preserved-partial', owed: { paths: ['Mail/gone.md', P.unalerted] } },
    { label: 'write_failure', via: 'tool-only', owed: { paths: [P.unalerted, P.blank] }, plant: 'trigger' },
    { label: 'unreadable_store', via: 'tool-only', owed: { paths: [P.unalerted, P.blank] }, plant: 'documents' },
    { label: 'already_alerted', via: 'turn-skip', owed: { paths: [P.alerted] } },
  ];

  const lines: string[] = [];
  for (const sc of scenarios) {
    const scratch = mkdtempSync(join(tmpdir(), 'qt-mailalerts-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'main-work.db');
    const mountWork = join(scratch, 'mount-work.db');
    copyFileSync(mainFixture, mainWork);
    copyFileSync(mountFixture, mountWork);

    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    jest.resetModules();
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

    const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
    const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    await initializeDatabase();

    const { Logger } = await import('@/lib/logger');
    let current: Array<Record<string, unknown>> | null = null;
    const originals: Array<[string, unknown]> = [];
    for (const level of ['debug', 'warn', 'error'] as const) {
      const original = Logger.prototype[level];
      originals.push([level, original]);
      Logger.prototype[level] = function (
        this: unknown,
        message: string,
        context?: Record<string, unknown>,
        ...rest: unknown[]
      ) {
        if (current) {
          const fields: Array<[string, string]> = [];
          for (const [k, v] of Object.entries(context ?? {})) {
            if (typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean') {
              fields.push([k, String(v)]);
            }
          }
          current.push({ level, message, fields });
        }
        return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
      } as never;
    }

    if (sc.plant) {
      const midb = getRawMountIndexDatabase();
      if (!midb) throw new Error('mount-index DB handle unavailable');
      midb.exec(
        sc.plant === 'documents'
          ? 'ALTER TABLE doc_mount_documents RENAME COLUMN content TO content_x'
          : "CREATE TRIGGER qt_plant_no_insert BEFORE INSERT ON doc_mount_documents BEGIN SELECT RAISE(ABORT, 'disk on fire'); END",
      );
    }

    const { markMailAlertsDelivered, collectUnalertedMail } = await import('@/lib/post-office/mailbox');
    const owed = sc.owed === 'null' ? null : { vaultId: meta.readerVault, paths: sc.owed.paths };
    current = [];
    let threw: string | null = null;
    try {
      await markMailAlertsDelivered(owed, { chatId: 'chat-mail', via: sc.via });
    } catch (e) {
      threw = e instanceof Error ? e.message : String(e);
    }
    const logs = current;
    current = null;

    let unalerted: string[] | null = null;
    let contents: Array<string | null> | null = null;
    if (sc.plant !== 'documents') {
      unalerted = (await collectUnalertedMail(meta.readerVault)).map(l => l.path);
      const { readDatabaseDocumentIfExists } = await import('@/lib/mount-index/database-store');
      const touched = sc.owed === 'null' ? [] : sc.owed.paths;
      contents = [];
      for (const path of touched) contents.push(await readDatabaseDocumentIfExists(meta.readerVault, path));
    }

    for (const [level, original] of originals) {
      (Logger.prototype as unknown as Record<string, unknown>)[level] = original;
    }
    closeMountIndexSQLiteClient();
    await closeDatabase();
    rmSync(scratch, { recursive: true, force: true });

    lines.push(
      JSON.stringify({
        label: sc.label,
        via: sc.via,
        vaultId: meta.readerVault,
        owed: sc.owed === 'null' ? null : sc.owed.paths,
        plant: sc.plant ?? null,
        threw,
        logs,
        unalerted,
        contents,
      }),
    );
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`mail-alerts-delivered oracle wrote ${outPath} (${lines.length} scenarios)\n`);
}

test('mail-alerts-delivered oracle', async () => {
  await main();
});
