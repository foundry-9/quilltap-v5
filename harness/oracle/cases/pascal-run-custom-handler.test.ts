/**
 * @jest-environment node
 *
 * P4.6ay unit 4 (the handler half) ORACLE: drives v4's REAL
 * `executeRunCustomTool` (lib/tools/handlers/run-custom-handler.ts) over a FRESH
 * copy of the committed `pascal-run-custom-{main,mount}.db` fixture per case,
 * emitting the returned model output + the posted MAIN-db `chat_messages` rows
 * (the Pascal outcome / Prospero error). The Rust port
 * (`tools::run_custom::execute_run_custom_tool`) is diffed against it.
 *
 * Every roll is `min === max` (no draw), so the outcome is deterministic without
 * a shared byte source. Posted rows carry a minted id + createdAt — the Rust
 * diff normalizes both positionally.
 *
 * Only the DB stack is un-mocked past jest.setup (the real cipher binding); the
 * handler's seams (roster/writers/repos) are all REAL — the whole point is the
 * end-to-end DB behavior.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   QT_FIXTURE_PASCAL_MAIN=<v5>/crates/quilltap-web/tests/fixtures/pascal-run-custom-main.db \
 *   QT_FIXTURE_PASCAL_MOUNT=<v5>/crates/quilltap-web/tests/fixtures/pascal-run-custom-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-pascal-run-custom-handler.ndjson npx jest -- pascal-run-custom-handler
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

interface Meta {
  vaultA: string;
  vaultB: string;
  vaultC: string;
}

const CHAT = 'c1000000-0000-4000-8000-000000000001';
const CHAR_A = 'a1000000-0000-4000-8000-00000000000a';
const CHAR_B = 'a1000000-0000-4000-8000-00000000000b';
const CHAR_C = 'a1000000-0000-4000-8000-00000000000c';
const P_A = 'e1000000-0000-4000-8000-00000000000a';

interface CaseSpec {
  name: string;
  characterId: string | null;
  vaultKey: 'vaultA' | 'vaultB' | 'vaultC' | null;
  characterIds: string[];
  input: unknown;
}

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

async function runCase(
  spec: Spec,
  meta: Meta,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

  const work = mkdtempSync(join(scratch, 'pascal-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite');
  const { executeRunCustomTool } = await import('@/lib/tools/handlers/run-custom-handler');

  await initializeDatabase();

  try {
    const context = {
      userId: spec.userId,
      chatId: CHAT,
      characterId: c.characterId,
      characterMountPointId: c.vaultKey ? meta[c.vaultKey] : null,
      characterIds: c.characterIds,
      projectId: null,
      callerParticipantId: P_A,
    };
    const output = await executeRunCustomTool(c.input, context as never);

    const mdb = getRawDatabase();
    if (!mdb) throw new Error('main DB handle unavailable');
    const columns = (
      mdb.prepare(`PRAGMA table_info(chat_messages)`).all() as Array<{ name: string }>
    ).map((col) => col.name);
    const rawRows = mdb.prepare(`SELECT * FROM chat_messages ORDER BY createdAt`).all() as Array<
      Record<string, unknown>
    >;
    const rows = rawRows.map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    });
    return { name: c.name, output, columns, rows };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'pascal-run-custom.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_PASCAL_MAIN ?? '',
    mount: process.env.QT_FIXTURE_PASCAL_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const meta = JSON.parse(fs.readFileSync(fixtures.main + '.meta.json', 'utf8')) as Meta;
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-pascal-handler-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const ALL = [CHAR_A, CHAR_B, CHAR_C];
  const cases: CaseSpec[] = [
    { name: 'gated-hit', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'ansible' } },
    { name: 'gated-miss', characterId: CHAR_B, vaultKey: 'vaultB', characterIds: ALL, input: { tool: 'ansible' } },
    { name: 'gated-empty', characterId: CHAR_C, vaultKey: 'vaultC', characterIds: ALL, input: { tool: 'ansible' } },
    { name: 'public-coin', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'coin' } },
    { name: 'whispered-default', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'whispered' } },
    { name: 'private-override', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'coin', private: true } },
    { name: 'public-override', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'whispered', private: false } },
    { name: 'unknown-tool', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'nonexistent' } },
    { name: 'validation-fail', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { notTool: 1 } },
    { name: 'run-error', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'coin', parameters: { bad: 1 } } },
    // The 616930db consult, end to end. The fixture carries no connection
    // profiles, so BOTH sides take the invoker's `no connection profiles are
    // configured` arm: the run does NOT fail, `pascalMeta.llm` records the
    // rendered prompt + the technical reason + the author's errorMessage as the
    // output, and the table branches to the `ok: false` row. Deterministic
    // without mocking a provider.
    { name: 'llm-consult-no-profiles', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'oracle' } },
    { name: 'llm-consult-private', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'oracle', private: true } },
  ];

  const out = fs.createWriteStream(outPath);
  for (const c of cases) {
    const row = await runCase(spec, meta, c, scratch, fixtures);
    out.write(JSON.stringify(row) + '\n');
  }
  out.end();
  await new Promise((r) => out.on('finish', r));
  rmSync(scratch, { recursive: true, force: true });
}

it('emits the pascal run_custom handler oracle', async () => {
  await main();
}, 120000);
