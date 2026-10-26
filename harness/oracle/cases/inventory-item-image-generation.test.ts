/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the Inventory object picture GENERATION (P4.D324; v4
 * `lib/inventory/item-image-generation.ts` `generateInventoryItemImage` at
 * `322a5488e`), ported to `quilltap-core::inventory::item_image_generation`.
 *
 * The wardrobe twin's shape (`wardrobe-item-image-generation.test.ts`), over
 * the wardrobe twin's OWN pair (`build-wardrobe-item-image-generation-fixture.ts`
 * — its profiles, keys, chat settings, characters and project). Seams are the
 * twin's: `createImageProvider` → a RECORDING provider answering one tiny WebP
 * or failing per the scenario's `provider` mode; the REAL provider registry;
 * the REAL `logLLMCall` into a scratch llm-logs DB.
 *
 * Per scenario: a FRESH module graph and fixture copy; the scenario's Inventory
 * object written into its home through v4's REAL `createInventoryInMount` (the
 * writer, so an archived owner's vault can hold one); the scenario's `sql`;
 * optionally the project store's Lantern aesthetic doc (v4's REAL
 * `writeDatabaseDocument`); then v4's REAL `resolveInventoryItemHome` +
 * `generateInventoryItemImage`, the logger spied (`[InventoryItemImage]`
 * lines — the picture write's are the keystone family's). Emitted: the result
 * or typed error, the ordered provider calls `{provider, apiKey, params}`, the
 * main `files` table, the mount-index tables, the `llm_logs` rows (id / ts
 * placeholdered) and the object's pointer after.
 *
 * Run (Node 24, from the v4 checkout — a /tmp mirror, jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-iiig-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/inventory-item-image-generation.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/inventory-item-image-generation-tier3.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WIIG_MAIN=/tmp/qt-iiig-main.db QT_FIXTURE_WIIG_MOUNT=/tmp/qt-iiig-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wardrobe-item-image-generation-fixture.ts
 *   QT_FIXTURE_WIIG_MAIN=/tmp/qt-iiig-main.db QT_FIXTURE_WIIG_MOUNT=/tmp/qt-iiig-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-inventory-item-image-generation.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=600000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "cases/inventory-item-image-generation\.test\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

interface Scenario {
  name: string;
  /** A key of `spec.objects`. */
  object: string;
  imageProfileId?: string | null;
  provider: 'ok' | 'refuseFirst' | 'refuseAll' | 'throw' | 'nodata';
  sql?: string[];
  /** The project store's Lantern aesthetic doc, written before the op. */
  lanternAesthetic?: string;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  characterId: string;
  archivedCharacterId: string;
  projectId: string;
  webp: string;
  lanternAestheticsFilename: string;
  objects: Record<string, { home: 'character' | 'archived' | 'project'; record: Record<string, unknown> }>;
  scenarios: Scenario[];
}

const MOUNT_TABLES: Array<{ key: string; table: string }> = [
  { key: 'points', table: 'doc_mount_points' },
  { key: 'folders', table: 'doc_mount_folders' },
  { key: 'links', table: 'doc_mount_file_links' },
  { key: 'mountFiles', table: 'doc_mount_files' },
  { key: 'documents', table: 'doc_mount_documents' },
  { key: 'blobs', table: 'doc_mount_blobs' },
];
const LOG_PREFIXES = ['[InventoryItemImage]'];
const PLUGIN_DIRS = [
  'anthropic', 'openai', 'google', 'grok', 'deepseek',
  'z-ai', 'openrouter', 'ollama', 'openai-compatible', 'nanogpt',
];
const SAFETY = '400 Your request was rejected as a result of our safety system.';

function sanitize(meta: unknown): unknown {
  if (meta === undefined) return null;
  return JSON.parse(
    // An `undefined` field is DROPPED, as winston drops it (§R.5).
    JSON.stringify(meta, (_k, v) => (v instanceof Error ? v.message : v)),
  );
}

async function runScenario(
  spec: Spec,
  scenario: Scenario,
  scratch: string,
  mainFixture: string,
  mountFixture: string,
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
  jest.doMock('@/lib/repositories/factory', () =>
    jest.requireActual('@/lib/repositories/factory'),
  );
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  jest.doMock('@/lib/embedding/embedding-service', () =>
    jest.requireActual('@/lib/embedding/embedding-service'),
  );
  jest.doMock('@/lib/files/webp-conversion', () =>
    jest.requireActual('@/lib/files/webp-conversion'),
  );
  // jest.setup mocks the logging service globally; the REAL `logLLMCall`
  // lands the WARDROBE_ITEM_IMAGE rows in the scratch llm-logs DB.
  jest.doMock('@/lib/services/llm-logging.service', () =>
    jest.requireActual('@/lib/services/llm-logging.service'),
  );

  const providerCalls: Array<Record<string, unknown>> = [];
  jest.doMock('@/lib/llm/plugin-factory', () => {
    const actual = jest.requireActual('@/lib/llm/plugin-factory');
    return {
      __esModule: true,
      ...actual,
      createImageProvider: (provider: string) => ({
        generateImage: async (params: Record<string, unknown>, apiKey: string) => {
          providerCalls.push({ provider, apiKey, params });
          if (scenario.provider === 'throw') throw new Error('canned provider failure');
          if (scenario.provider === 'refuseAll') throw new Error(SAFETY);
          if (scenario.provider === 'refuseFirst' && providerCalls.length === 1) {
            throw new Error(SAFETY);
          }
          if (scenario.provider === 'nodata') {
            return { images: [{ mimeType: 'image/webp', revisedPrompt: 'nothing drawn' }] };
          }
          return {
            images: [{ data: spec.webp, mimeType: 'image/webp', revisedPrompt: 'a revised coat' }],
          };
        },
      }),
    };
  });

  const work = mkdtempSync(join(scratch, 'sc-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = join(work, 'llm-logs.db');

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawLLMLogsDatabase } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const loggerModule = await import('@/lib/logger');
  const { initializeProviderRegistry } = await import('@/lib/plugins/provider-registry');
  await initializeProviderRegistry(
    PLUGIN_DIRS.map((d) => {
      const m = require(join(process.cwd(), 'plugins', 'dist', `qtap-plugin-${d}`, 'index.js'));
      return m.plugin || m.default?.plugin || m.default;
    }),
  );
  const ii = await import('@/lib/inventory/item-images');
  const gen = await import('@/lib/inventory/item-image-generation');
  const writes = await import('@/lib/database/repositories/vault-overlay/inventory-writes');
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');

  await initializeDatabase();
  const repos = getRepositories();
  const raw = getRawDatabase();
  const midb = getRawMountIndexDatabase();
  if (!raw || !midb) throw new Error('DB handles unavailable');
  const object = spec.objects[scenario.object];
  const ownerId =
    object.home === 'character' ? spec.characterId : object.home === 'archived' ? spec.archivedCharacterId : spec.projectId;
  const scope = object.home === 'project' ? 'project' : 'character';
  const mountPointId =
    object.home === 'project'
      ? ((await repos.projects.findByIdRaw(spec.projectId)) as { officialMountPointId: string }).officialMountPointId
      : ((await repos.characters.findByIdRaw(ownerId)) as { characterDocumentMountPointId: string })
          .characterDocumentMountPointId;
  await writes.createInventoryInMount(
    { mountPointId, scope: scope as never, characterId: scope === 'character' ? ownerId : null },
    object.record as never,
  );
  if (scenario.lanternAesthetic) {
    await writeDatabaseDocument(mountPointId, spec.lanternAestheticsFilename, scenario.lanternAesthetic);
  }
  for (const sql of scenario.sql ?? []) raw.exec(sql);

  const logs: Array<{ level: string; message: string; meta: unknown }> = [];
  const spies: jest.SpyInstance[] = [];
  for (const level of ['debug', 'info', 'warn', 'error'] as const) {
    spies.push(
      jest.spyOn(loggerModule.logger as never, level as never).mockImplementation(((
        message: string,
        meta?: unknown,
      ) => {
        if (typeof message === 'string' && LOG_PREFIXES.some((p) => message.startsWith(p))) {
          logs.push({ level, message, meta: sanitize(meta) });
        }
      }) as never),
    );
  }

  let result: unknown;
  try {
    const home = await ii.resolveInventoryItemHome(repos, spec.userId, scope as never, ownerId, object.record.id as string);
    if (!home) throw new Error('no home');
    const r = await gen.generateInventoryItemImage(repos, {
      userId: spec.userId,
      home,
      imageProfileId: scenario.imageProfileId ?? null,
    });
    result = { ok: true, fileId: r.fileId, prompt: r.prompt, profile: r.profile, rerouted: r.rerouted, trail: r.trail };
  } catch (e) {
    const err = e as Error & { trail?: unknown; refused?: boolean };
    result = {
      ok: false,
      error: err.name,
      message: err.message,
      trail: err.trail ?? null,
      refused: err.refused ?? null,
    };
  }
  for (const s of spies) s.mockRestore();

  try {
    const after = await ii.resolveInventoryItemHome(repos, spec.userId, scope as never, ownerId, object.record.id as string);
    const pointers = { object: after ? (after.record.imageFileId ?? null) : '<gone>' };
    const dump = (db: { prepare: (s: string) => { all: () => unknown[] } }, table: string) => {
      const columns = (db.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>).map(
        (c) => c.name,
      );
      const rows = (db.prepare(`SELECT * FROM ${table} ORDER BY rowid`).all() as Array<
        Record<string, unknown>
      >).map((r) => {
        const out: Record<string, unknown> = {};
        for (const col of columns) out[col] = canonValue(r[col]);
        return out;
      });
      return { table, columns, rows };
    };
    const tables: Record<string, unknown> = {
      files: dump(raw as never, 'files'),
    };
    for (const t of MOUNT_TABLES) tables[t.key] = dump(midb as never, t.table);

    const lldb = getRawLLMLogsDatabase();
    if (!lldb) throw new Error('llm-logs DB handle unavailable');
    const llColumns = (lldb.pragma('table_info(llm_logs)') as Array<{ name: string }>).map(
      (c) => c.name,
    );
    // The table is created on the first logged call; a run that logged
    // nothing has none (the Rust side materializes it empty).
    const llRaw = llColumns.length
      ? (lldb.prepare('SELECT * FROM llm_logs').all() as Array<Record<string, unknown>>)
      : [];
    const llRows = llRaw
      .map((r) => {
        const out: Record<string, unknown> = {};
        for (const col of llColumns) out[col] = canonValue(r[col]);
        out.id = '<id>';
        out.createdAt = '<ts>';
        out.updatedAt = '<ts>';
        return out;
      })
      .sort((a, b) => {
        const sa = JSON.stringify(a);
        const sb = JSON.stringify(b);
        return sa < sb ? -1 : sa > sb ? 1 : 0;
      });

    const count = (table: string): number | null => {
      const exists = raw
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?")
        .get(table);
      if (!exists) return null;
      return (raw.prepare(`SELECT COUNT(*) AS n FROM ${table}`).get() as { n: number }).n;
    };

    return {
      name: scenario.name,
      result,
      providerCalls,
      logs,
      pointers,
      tables,
      llmLogs: { columns: llColumns, rows: llRows },
      conciergeRefusals: count('concierge_refusals'),
      chatMessages: count('chat_messages'),
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(
      join(here, '..', 'fixtures', 'inventory-item-image-generation-tier3.json'),
      'utf8',
    ),
  ) as Spec;
  const mainFixture = process.env.QT_FIXTURE_WIIG_MAIN;
  const mountFixture = process.env.QT_FIXTURE_WIIG_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_WIIG_MAIN and QT_FIXTURE_WIIG_MOUNT must point at the seed fixtures from build-wardrobe-item-image-generation-fixture.ts',
    );
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-iiig-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];
  for (const scenario of spec.scenarios) {
    outLines.push(
      JSON.stringify(await runScenario(spec, scenario, scratch, mainFixture, mountFixture)),
    );
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(
    `inventory-item-image-generation oracle wrote ${outPath} (${outLines.length} scenarios)\n`,
  );
}

test('inventory-item-image-generation tier-3 oracle', async () => {
  await main();
});

const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
