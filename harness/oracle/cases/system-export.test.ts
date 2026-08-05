/**
 * @jest-environment node
 *
 * P4.9G4 `.qtap` EXPORT ORACLE: drives v4's REAL `createNdjsonStream` /
 * `streamExportRecords` (`lib/export/ndjson-writer.ts`), `previewExport`
 * (`lib/export/quilltap-export-service.ts:63`), and the route-level
 * `handleExportEntities` (`app/api/v1/system/tools/route.ts:440`) over a FRESH
 * copy of the committed `system-data-*` fixture family per case, emitting the
 * NDJSON **line by line** so the Rust writer can be diffed record-for-record.
 *
 * ── WHAT IS NORMALIZED ───────────────────────────────────────────────────────
 * Only `manifest.createdAt` (wall clock) — and only on the envelope, where v4
 * stamps it. `appVersion` is NOT normalized: the Rust differential passes v4's
 * own `package.json` version (emitted here as `_meta.appVersion`) so the line is
 * byte-identical. Every other byte of every other line is exact.
 *
 * ── THE CASE MATRIX ──────────────────────────────────────────────────────────
 * All FIFTEEN entity types × `scope: all` (`7189a968`), plus `scope: selected` and
 * `includeMemories` variants where they change behaviour, plus the unknown-type
 * throw and the `previewExport` / `export-entities` surfaces.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-sysexport-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/system-export.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/system-data.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$V5W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *   QT_ORACLE_OUT=/tmp/oracle-system-export.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- system-export
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

const LORIAN = 'a1000000-0000-4000-8000-000000000001';
const CHAT_1 = 'c1000000-0000-4000-8000-000000000001';
const TAG_1 = 'a5000000-0000-4000-8000-000000000001';

function applyMocks(userId: string): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  // [P4.D46] jest.setup.ts globally mocks the storage manager with canned
  // bytes — `streamFiles` must read REAL bytes (mount-blob keys hit the mount
  // DB; the fixture's legacy disk key has no bytes anywhere, which is the
  // `_bytesMissing` arm).
  jest.doMock('@/lib/file-storage/manager', () =>
    jest.requireActual('@/lib/file-storage/manager'),
  );
  // [P4.D46] The plugin registry is EMPTY under jest (loadPlugins never runs),
  // but production has every bundled plugin registered — and the plugin-config
  // redaction resolves manifests through it. Serve the REAL manifest.json
  // straight from plugins/dist so redaction sees production's data; anything
  // else (an npm-installed plugin, the fixture's synthetic name) resolves
  // null, the withhold-everything arm.
  jest.doMock('@/lib/plugins/registry', () => {
    const actual = jest.requireActual('@/lib/plugins/registry');
    const rfs = require('node:fs');
    const rpath = require('node:path');
    return {
      __esModule: true,
      ...actual,
      getPlugin: (name: string) => {
        const manifestPath = rpath.join(process.cwd(), 'plugins', 'dist', name, 'manifest.json');
        if (!rfs.existsSync(manifestPath)) return null;
        return { manifest: JSON.parse(rfs.readFileSync(manifestPath, 'utf8')) };
      },
    };
  });
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: userId } }),
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

/** Drain a `ReadableStream<Uint8Array>` into the array of NDJSON lines. */
async function drain(stream: ReadableStream<Uint8Array>): Promise<string[]> {
  const reader = stream.getReader();
  const chunks: Uint8Array[] = [];
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    if (value) chunks.push(value);
  }
  const total = chunks.reduce((n, c) => n + c.byteLength, 0);
  const merged = new Uint8Array(total);
  let off = 0;
  for (const c of chunks) {
    merged.set(c, off);
    off += c.byteLength;
  }
  const text = new TextDecoder('utf-8').decode(merged);
  return text.split('\n').filter((l) => l.length > 0);
}

interface ExportOpts {
  type: string;
  scope: 'all' | 'selected';
  selectedIds?: string[];
  includeMemories?: boolean;
}

interface CaseSpec {
  name: string;
  run: () => Promise<Record<string, unknown>>;
}

function buildCases(spec: Spec): CaseSpec[] {
  const streamCase = (name: string, options: ExportOpts): CaseSpec => ({
    name,
    run: async () => {
      const { createNdjsonStream } = await import('@/lib/export/ndjson-writer');
      const lines = await drain(createNdjsonStream(spec.userId, options as never));
      return { kind: 'ndjson', options, lines };
    },
  });

  const previewCase = (name: string, options: ExportOpts): CaseSpec => ({
    name,
    run: async () => {
      const { previewExport } = await import('@/lib/export/quilltap-export-service');
      const preview = await previewExport(spec.userId, options as never);
      return { kind: 'preview', options, body: preview };
    },
  });

  const entitiesCase = (name: string, type: string): CaseSpec => ({
    name,
    run: async () => {
      const route = await import('@/app/api/v1/system/tools/route');
      const resp = (await (route as never as { GET: (r: unknown) => Promise<Response> }).GET(
        mockRequest(
          `http://localhost/api/v1/system/tools?action=export-entities&type=${type}`,
          'GET',
          {},
        ),
      )) as unknown as { status: number; json: () => Promise<unknown> };
      return { kind: 'entities', type, status: resp.status, body: await resp.json() };
    },
  });

  // The fifteen `7189a968` types, in v4's declaration order.
  const TYPES = [
    'characters',
    'chats',
    'roleplay-templates',
    'prompt-templates',
    'connection-profiles',
    'image-profiles',
    'embedding-profiles',
    'tags',
    'projects',
    'groups',
    'document-stores',
    'files',
    'provider-models',
    'plugin-configs',
    'instance-settings',
  ];

  const cases: CaseSpec[] = [];

  // 1. Every type, scope 'all', memories OFF.
  for (const type of TYPES) {
    cases.push(streamCase(`stream_${type}_all`, { type, scope: 'all', includeMemories: false }));
  }

  // 2. The two memory-bearing types with memories ON.
  cases.push(
    streamCase('stream_characters_all_memories', {
      type: 'characters',
      scope: 'all',
      includeMemories: true,
    }),
  );
  cases.push(
    streamCase('stream_chats_all_memories', {
      type: 'chats',
      scope: 'all',
      includeMemories: true,
    }),
  );

  // 3. scope 'selected' — a hit, an empty list, and a miss (nonexistent id).
  cases.push(
    streamCase('stream_characters_selected_one', {
      type: 'characters',
      scope: 'selected',
      selectedIds: [LORIAN],
      includeMemories: true,
    }),
  );
  cases.push(
    streamCase('stream_characters_selected_empty', {
      type: 'characters',
      scope: 'selected',
      selectedIds: [],
      includeMemories: false,
    }),
  );
  cases.push(
    streamCase('stream_tags_selected_missing', {
      type: 'tags',
      scope: 'selected',
      selectedIds: ['ffffffff-ffff-4fff-8fff-ffffffffffff', TAG_1],
      includeMemories: false,
    }),
  );
  cases.push(
    streamCase('stream_chats_selected_one', {
      type: 'chats',
      scope: 'selected',
      selectedIds: [CHAT_1],
      includeMemories: false,
    }),
  );

  // 4. The unknown-type throw (resolveExportIds :650 / streamExportRecords :714).
  cases.push({
    name: 'stream_unknown_type_throws',
    run: async () => {
      const { createNdjsonStream } = await import('@/lib/export/ndjson-writer');
      try {
        await drain(
          createNdjsonStream(spec.userId, {
            type: 'nonsense',
            scope: 'all',
            includeMemories: false,
          } as never),
        );
        return { kind: 'throw', error: null };
      } catch (err) {
        return { kind: 'throw', error: err instanceof Error ? err.message : String(err) };
      }
    },
  });

  // 5. previewExport — every type at scope 'all', plus the memory + selected arms.
  for (const type of TYPES) {
    cases.push(previewCase(`preview_${type}_all`, { type, scope: 'all', includeMemories: false }));
  }
  cases.push(
    previewCase('preview_characters_memories', {
      type: 'characters',
      scope: 'all',
      includeMemories: true,
    }),
  );
  cases.push(
    previewCase('preview_chats_memories', {
      type: 'chats',
      scope: 'all',
      includeMemories: true,
    }),
  );
  cases.push(
    previewCase('preview_characters_selected', {
      type: 'characters',
      scope: 'selected',
      selectedIds: [LORIAN],
      includeMemories: true,
    }),
  );
  cases.push({
    name: 'preview_unknown_type_throws',
    run: async () => {
      const { previewExport } = await import('@/lib/export/quilltap-export-service');
      try {
        await previewExport(spec.userId, {
          type: 'nonsense',
          scope: 'all',
          includeMemories: false,
        } as never);
        return { kind: 'throw', error: null };
      } catch (err) {
        return { kind: 'throw', error: err instanceof Error ? err.message : String(err) };
      }
    },
  });

  // 6. The ?action=export-entities route — the switch is exhaustive at
  //    `7189a968` (groups + document-stores no longer 400), plus one bogus value.
  for (const type of TYPES) {
    cases.push(entitiesCase(`entities_${type}`, type));
  }
  cases.push(entitiesCase('entities_bogus', 'nonsense'));

  return cases;
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string; llm: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec.userId);

  const work = mkdtempSync(join(scratch, 'sd-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  const llmWork = join(work, 'llm.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  copyFileSync(fixtures.llm, llmWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = llmWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();

  try {
    const out = await c.run();
    return { name: c.name, ...out };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'system-data.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_SD_MAIN ?? '',
    mount: process.env.QT_FIXTURE_SD_MOUNT ?? '',
    llm: process.env.QT_FIXTURE_SD_LLM ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-sysexport-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const appVersion = (
    JSON.parse(fs.readFileSync(join(process.cwd(), 'package.json'), 'utf8')) as {
      version: string;
    }
  ).version;

  const outLines: string[] = [
    JSON.stringify({ name: '_meta', kind: 'meta', appVersion, userId: spec.userId }),
  ];
  for (const c of buildCases(spec)) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(
    `system-export oracle wrote ${outPath} (${outLines.length - 1} cases, appVersion=${appVersion})\n`,
  );
}

test('system-export oracle', async () => {
  await main();
});
