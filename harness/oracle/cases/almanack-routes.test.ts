/**
 * @jest-environment node
 *
 * P4.37 Almanack tier-2 ORACLE: drives v4's REAL `generateAlmanackData` +
 * `renderAlmanackMarkdown` and the REAL `system/tools` route handlers
 * (`capabilities-report-generate|list|get|delete` + the `download=true` leg)
 * over FRESH copies of the committed `almanack-*` fixture family, and emits per
 * case the report data, the rendered markdown, the route envelopes, the
 * progress-frame replay and the persisted-row dumps. The Rust differential
 * (`almanack_tier2_equivalence`) diffs each — see its header for the
 * normalization contract.
 *
 * ── DETERMINISM ──────────────────────────────────────────────────────────────
 *  - The CLOCK is frozen at `spec.pinnedNowIso` via jest fake timers faking
 *    ONLY `Date` (every timer API is in `doNotFake`), so `generatedAt`, the
 *    report filename and every repo-stamped timestamp are pinned; the Rust side
 *    pins `ctx.now_ms` to the same instant.
 *  - The fixture DBs are copied to `<scratch>/data/quilltap*.db` — the SAME
 *    paths `lib/paths` derives from QUILLTAP_DATA_DIR — so phase 1's stat block
 *    sees the databases that are actually open.
 *  - The backups directory is seeded from `spec.backups` (pinned names+sizes);
 *    the Rust side seeds an identical directory.
 *  - `getHasUserPassphrase` is MOCKED to `true`; the Rust side pins the same.
 *  - v4's plugin system is INITIALIZED (`initializePlugins()`), so phase 2's
 *    registry-derived lists are the real bundled-plugin inventory, exactly as
 *    in production. (The theme registry is whatever plugin init leaves behind —
 *    the theme LIST is a normalized divergence either way.)
 *  - The runtime-environment block (uptime, free memory, …) is nondeterministic
 *    per call; the differential splices the oracle's values into the Rust
 *    context for the data cases and line-normalizes it for the route flow.
 *
 * ── CASES ────────────────────────────────────────────────────────────────────
 *  data_exact / data_approximate — `generateAlmanackData` + render over the
 *    migrated / legacy llm-logs variant (the §2 attribution probe's two arms).
 *  flow — ONE work copy: generate (with progressId) → progress replay →
 *    persisted dumps → list → get → get(download) → get(missing) → delete →
 *    delete(missing) → list-after-delete.
 *  generate_untracked — the optional-progressId zod arm on a fresh copy.
 *
 * Run (Node 24, from the PINNED v4 worktree; jest ignores .claude/ so the case
 * is copied to a /tmp mirror):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>
 *   TMPO=/tmp/qt-almanack-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$W/harness/oracle/cases/almanack-routes.test.ts" "$TMPO/cases/"
 *   cp "$W/harness/oracle/fixtures/almanack.json" "$TMPO/fixtures/"
 *   cd /tmp/qt-v4-pin-p437-f7f1a956   # or ~/source/quilltap-server when clean at baseline
 *   QT_FIXTURE_ALM_MAIN=$W/crates/quilltap-web/tests/fixtures/almanack-main.db \
 *   QT_FIXTURE_ALM_MOUNT=$W/crates/quilltap-web/tests/fixtures/almanack-mount.db \
 *   QT_FIXTURE_ALM_LLM=$W/crates/quilltap-web/tests/fixtures/almanack-llmlogs.db \
 *   QT_FIXTURE_ALM_LLM_LEGACY=$W/crates/quilltap-web/tests/fixtures/almanack-llmlogs-legacy.db \
 *   QT_ORACLE_OUT=/tmp/oracle-almanack.ndjson \
 *     TZ=UTC $N/npx jest --silent --watchman=false --testTimeout=600000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- almanack-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface BackupSpec {
  filename: string;
  sizeBytes: number;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  pinnedNowIso: string;
  backups: BackupSpec[];
}

const BASELINE = 'f7f1a956';
const PROGRESS_ID = 'fd000000-0000-4000-8000-000000000001';

function applyMocks(userId: string): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: userId } }),
  }));
  // jest.setup mocks the uploads bridge (phantom storage key) AND the file
  // storage manager (canned 'mock file content', no-op delete); the report
  // write/read/delete must all be REAL so the dumps and the download bytes
  // mean anything.
  jest.doMock('@/lib/file-storage/user-uploads-bridge', () =>
    jest.requireActual('@/lib/file-storage/user-uploads-bridge'),
  );
  jest.doMock('@/lib/file-storage/manager', () =>
    jest.requireActual('@/lib/file-storage/manager'),
  );
  // The passphrase flag is host knowledge — pinned `true` on both sides.
  jest.doMock('@/lib/startup/dbkey', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/startup/dbkey'),
    getHasUserPassphrase: () => true,
  }));
  jest.doMock('@/lib/startup/startup-state', () => {
    const actual = jest.requireActual('@/lib/startup/startup-state');
    return {
      __esModule: true,
      ...actual,
      startupState: {
        ...actual.startupState,
        isReady: () => true,
        waitForReady: async () => true,
        isPepperResolved: () => true,
        getPepperState: () => 'resolved',
        getPhase: () => 'ready',
        isLockedMode: () => false,
      },
    };
  });
}

function mockRequest(url: string, method: string, body: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

async function respondJson(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  if (resp.status === 204) return { status: 204, body: null };
  return { status: resp.status, body: await resp.json() };
}

const TOOLS_ROUTE = '@/app/api/v1/system/tools/route';
const B = 'http://localhost/api/v1';

interface Ctx {
  spec: Spec;
  scratch: string;
  fixtures: { main: string; mount: string; llm: string; llmLegacy: string };
}

/** Copy the fixture family onto the constant lib/paths locations. */
function layFixtures(ctx: Ctx, llmVariant: 'migrated' | 'legacy'): void {
  const dataDir = join(ctx.scratch, 'data');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    for (const name of ['quilltap.db', 'quilltap-mount-index.db', 'quilltap-llm-logs.db']) {
      const p = join(dataDir, name + suffix);
      if (existsSync(p)) rmSync(p);
    }
  }
  copyFileSync(ctx.fixtures.main, join(dataDir, 'quilltap.db'));
  copyFileSync(ctx.fixtures.mount, join(dataDir, 'quilltap-mount-index.db'));
  copyFileSync(
    llmVariant === 'legacy' ? ctx.fixtures.llmLegacy : ctx.fixtures.llm,
    join(dataDir, 'quilltap-llm-logs.db'),
  );
  process.env.SQLITE_PATH = join(dataDir, 'quilltap.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = join(dataDir, 'quilltap-mount-index.db');
  process.env.SQLITE_LLM_LOGS_PATH = join(dataDir, 'quilltap-llm-logs.db');
}

/** Open the fixture DBs and run `fn`, then close every partition client. */
async function withDatabases<T>(
  ctx: Ctx,
  llmVariant: 'migrated' | 'legacy',
  fn: () => Promise<T>,
): Promise<T> {
  jest.resetModules();
  applyMocks(ctx.spec.userId);
  layFixtures(ctx, llmVariant);

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  await initializeDatabase();
  // The real bundled-plugin inventory, exactly as production sees it.
  const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
  await initializePlugins();
  try {
    return await fn();
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    closeLLMLogsSQLiteClient();
  }
}

/** Dump the persisted report rows (main files row + the mount diagnostics/ set). */
async function dumpPersisted(): Promise<Record<string, unknown[]>> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const mainDb = getRawDatabase();
  const midb = getRawMountIndexDatabase();
  if (!mainDb || !midb) throw new Error('raw handles unavailable');
  const filesRows = mainDb
    .prepare('SELECT * FROM "files" WHERE "folderPath" = \'/reports\' ORDER BY "originalFilename"')
    .all();
  const links = midb
    .prepare(
      "SELECT * FROM doc_mount_file_links WHERE relativePath LIKE 'diagnostics/%' ORDER BY relativePath",
    )
    .all();
  const files = midb
    .prepare(
      'SELECT f.* FROM doc_mount_files f WHERE f.id IN ' +
        "(SELECT fileId FROM doc_mount_file_links WHERE relativePath LIKE 'diagnostics/%') ORDER BY f.id",
    )
    .all();
  const documents = midb
    .prepare(
      'SELECT d.* FROM doc_mount_documents d WHERE d.fileId IN ' +
        "(SELECT fileId FROM doc_mount_file_links WHERE relativePath LIKE 'diagnostics/%') ORDER BY d.id",
    )
    .all();
  const blobs = midb
    .prepare(
      'SELECT b.* FROM doc_mount_blobs b WHERE b.fileId IN ' +
        "(SELECT fileId FROM doc_mount_file_links WHERE relativePath LIKE 'diagnostics/%') ORDER BY b.id",
    )
    .all();
  return {
    files_rows: filesRows as unknown[],
    mount_links: links as unknown[],
    mount_files: files as unknown[],
    mount_documents: documents as unknown[],
    mount_blobs: blobs as unknown[],
  };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'almanack.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_ALM_MAIN ?? '',
    mount: process.env.QT_FIXTURE_ALM_MOUNT ?? '',
    llm: process.env.QT_FIXTURE_ALM_LLM ?? '',
    llmLegacy: process.env.QT_FIXTURE_ALM_LLM_LEGACY ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-almanack-oracle-'));
  mkdirSync(join(scratch, 'data', 'backups'), { recursive: true });
  for (const b of spec.backups) {
    writeFileSync(join(scratch, 'data', 'backups', b.filename), Buffer.alloc(b.sizeBytes, 0x51));
  }
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // Freeze ONLY Date — every timer API stays real so async DB work proceeds.
  jest.useFakeTimers({
    now: new Date(spec.pinnedNowIso),
    doNotFake: [
      'hrtime',
      'nextTick',
      'performance',
      'queueMicrotask',
      'requestAnimationFrame',
      'cancelAnimationFrame',
      'requestIdleCallback',
      'cancelIdleCallback',
      'setImmediate',
      'clearImmediate',
      'setInterval',
      'clearInterval',
      'setTimeout',
      'clearTimeout',
    ],
  });

  const ctx: Ctx = { spec, scratch, fixtures };
  const lines: Record<string, unknown>[] = [];
  const emit = (name: string, payload: Record<string, unknown>) =>
    lines.push({ name, baseline: BASELINE, ...payload });

  // ── The two data cases ─────────────────────────────────────────────────────
  for (const [name, variant] of [
    ['data_exact', 'migrated'],
    ['data_approximate', 'legacy'],
  ] as Array<[string, 'migrated' | 'legacy']>) {
    await withDatabases(ctx, variant, async () => {
      const { generateAlmanackData } = await import('@/lib/tools/almanack');
      const { renderAlmanackMarkdown } = await import('@/lib/tools/almanack/render');
      const data = await generateAlmanackData(spec.userId);
      const markdown = renderAlmanackMarkdown(data as never);
      emit(name, { data, markdown });
    });
  }

  // ── The route flow (one work copy, generate through delete) ────────────────
  await withDatabases(ctx, 'migrated', async () => {
    const route = (await import(TOOLS_ROUTE)) as Record<
      string,
      (...a: unknown[]) => Promise<unknown>
    >;
    const get = (qs: string) =>
      route.GET(mockRequest(`${B}/system/tools?${qs}`, 'GET', {}));
    const post = (action: string, body: unknown) =>
      route.POST(mockRequest(`${B}/system/tools?action=${action}`, 'POST', body));

    const genResp = await respondJson(
      await post('capabilities-report-generate', { progressId: PROGRESS_ID }),
    );
    emit('generate_route', genResp as never);

    // The progress replay (the operation-progress bus buffers frames; strip ts).
    const { subscribeOperationProgress } = await import('@/lib/progress/operation-progress');
    const seen: unknown[] = [];
    const sub = subscribeOperationProgress(PROGRESS_ID, () => {});
    for (const ev of sub.replay) {
      const { ts: _ts, ...rest } = ev as Record<string, unknown>;
      seen.push(rest);
    }
    sub.unsubscribe();
    emit('progress_frames', { frames: seen });

    emit('persisted_after_generate', { tables: await dumpPersisted() });

    const reportId = (genResp.body as { reportId?: string }).reportId ?? '';
    emit('list_route', (await respondJson(await get('action=capabilities-report-list'))) as never);
    emit(
      'get_route',
      (await respondJson(
        await get(`action=capabilities-report-get&reportId=${reportId}`),
      )) as never,
    );

    // The download leg returns raw markdown + attachment headers. jest.setup's
    // MockNextResponse keeps the constructor body verbatim on `.body`
    // (a Uint8Array here), so the bytes are read straight off it.
    const dl = (await get(
      `action=capabilities-report-get&reportId=${reportId}&download=true`,
    )) as { status: number; body: Uint8Array; headers: Headers };
    emit('get_download_route', {
      status: dl.status,
      contentType: dl.headers.get('Content-Type'),
      contentDisposition: dl.headers.get('Content-Disposition'),
      bodyText: Buffer.from(dl.body).toString('utf-8'),
    });

    emit(
      'get_missing_route',
      (await respondJson(
        await get('action=capabilities-report-get&reportId=fe000000-0000-4000-8000-000000000001'),
      )) as never,
    );
    emit(
      'delete_route',
      (await respondJson(await post('capabilities-report-delete', { reportId }))) as never,
    );
    emit(
      'delete_missing_route',
      (await respondJson(await post('capabilities-report-delete', { reportId }))) as never,
    );
    emit(
      'list_after_delete',
      (await respondJson(await get('action=capabilities-report-list'))) as never,
    );
    emit('persisted_after_delete', { tables: await dumpPersisted() });
  });

  // ── The untracked-generate arm (no progressId in the body) ─────────────────
  await withDatabases(ctx, 'migrated', async () => {
    const route = (await import(TOOLS_ROUTE)) as Record<
      string,
      (...a: unknown[]) => Promise<unknown>
    >;
    const genResp = await respondJson(
      await route.POST(
        mockRequest(`${B}/system/tools?action=capabilities-report-generate`, 'POST', {}),
      ),
    );
    emit('generate_untracked', genResp as never);
  });

  fs.writeFileSync(outPath, lines.map((l) => JSON.stringify(l)).join('\n') + '\n');
  process.stderr.write(`almanack oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('almanack-routes oracle', async () => {
  await main();
});
