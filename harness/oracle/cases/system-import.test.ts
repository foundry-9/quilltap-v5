/**
 * @jest-environment node
 *
 * P4.9G4 `.qtap` IMPORT-READ ORACLE: drives v4's REAL `loadQtapFromUpload`
 * chain (`peekFormat` → `readNdjsonLines` → `assembleExportFromStream`, the exact
 * composition `app/api/v1/system/tools/route.ts:614` uses) and v4's REAL
 * `previewImport` (`lib/import/quilltap-import/preview.ts:20`) over a FRESH copy
 * of the committed `system-data-*` fixture family per case.
 *
 * ── THE PAYLOADS ARE REAL EXPORTS ────────────────────────────────────────────
 * Each case first runs v4's own `createNdjsonStream` over the fixture to produce
 * genuine `.qtap` bytes, then feeds those bytes back through the reader. So the
 * round-trip is v4-writer → v4-reader, and the Rust differential replays the
 * SAME bytes through its own reader + preview. The emitted `qtapBase64` is what
 * the Rust side reads, so both sides are provably looking at identical input.
 *
 * ── WHAT IS EMITTED PER CASE ─────────────────────────────────────────────────
 *   assembled : the `{manifest, data}` `assembleExportFromStream` produced
 *   preview   : the `previewImport` body over the SAME fixture the export came
 *               from (so every entity conflicts by id — the same-instance
 *               re-import path), or over an EMPTY instance for the
 *               cross-instance/no-conflict arms
 *   error     : for the malformed-input arms, the thrown message
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-sysimport-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/system-import.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/system-data.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$V5W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *   QT_ORACLE_OUT=/tmp/oracle-system-import.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- system-import
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

function applyMocks(userId: string): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  // [P4.D46] The round-trips run v4's REAL writer over the fixture, so the
  // writer's byte reads + manifest resolution must be real too (jest.setup.ts
  // globally mocks both) — same doMock(requireActual) antidote as the export
  // oracle; the registry mock serves the REAL bundled manifest.json files.
  jest.doMock('@/lib/file-storage/manager', () =>
    jest.requireActual('@/lib/file-storage/manager'),
  );
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

/** Drain a `ReadableStream<Uint8Array>` into its raw bytes. */
async function drainBytes(stream: ReadableStream<Uint8Array>): Promise<Buffer> {
  const reader = stream.getReader();
  const chunks: Buffer[] = [];
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    if (value) chunks.push(Buffer.from(value));
  }
  return Buffer.concat(chunks);
}

/** A `ReadableStream` over a fixed buffer — the shape `peekFormat` expects. */
function streamOf(bytes: Buffer): ReadableStream<Uint8Array> {
  return new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(new Uint8Array(bytes));
      controller.close();
    },
  });
}

/**
 * v4 `loadQtapFromUpload` reproduced over a buffer (the route hands it a `File`
 * whose `.stream()` yields exactly these bytes).
 */
async function loadQtap(bytes: Buffer): Promise<unknown> {
  const { peekFormat, readNdjsonLines, collectLegacyJson } = await import(
    '@/lib/import/ndjson-reader'
  );
  const { assembleExportFromStream } = await import('@/lib/import/quilltap-import-stream');
  const { format, stream } = await peekFormat(streamOf(bytes));
  if (format === 'ndjson') {
    return assembleExportFromStream(readNdjsonLines(stream));
  }
  const text = await collectLegacyJson(stream, 450 * 1024 * 1024);
  // v4 `route.ts:626-633` wraps the parse failure; reproduce the wrapper so the
  // differential sees the same message the route would surface.
  try {
    return JSON.parse(text);
  } catch (err) {
    throw new Error(`Invalid JSON: ${err instanceof Error ? err.message : String(err)}`);
  }
}

interface CaseSpec {
  name: string;
  run: (spec: Spec) => Promise<Record<string, unknown>>;
}

/** Export → read back → preview, all against the same live fixture. */
function roundTrip(
  name: string,
  options: { type: string; scope: 'all' | 'selected'; includeMemories?: boolean },
): CaseSpec {
  return {
    name,
    run: async (spec) => {
      const { createNdjsonStream } = await import('@/lib/export/ndjson-writer');
      const bytes = await drainBytes(createNdjsonStream(spec.userId, options as never));
      const assembled = await loadQtap(bytes);
      const { previewImport } = await import('@/lib/import/quilltap-import-service');
      const preview = await previewImport(spec.userId, assembled as never);
      return {
        kind: 'roundtrip',
        qtapBase64: bytes.toString('base64'),
        assembled,
        preview,
      };
    },
  };
}

function buildCases(): CaseSpec[] {
  const cases: CaseSpec[] = [];

  // The fifteen `7189a968` types, in v4's declaration order.
  for (const type of [
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
  ]) {
    cases.push(roundTrip(`roundtrip_${type}`, { type, scope: 'all', includeMemories: false }));
  }
  cases.push(
    roundTrip('roundtrip_characters_memories', {
      type: 'characters',
      scope: 'all',
      includeMemories: true,
    }),
  );
  cases.push(
    roundTrip('roundtrip_chats_memories', {
      type: 'chats',
      scope: 'all',
      includeMemories: true,
    }),
  );

  // The legacy monolithic-JSON leg: hand the reader a `{manifest,data}` object.
  cases.push({
    name: 'legacy_monolith_reads',
    run: async () => {
      const legacy = Buffer.from(
        JSON.stringify({
          manifest: {
            format: 'quilltap-export',
            version: '1.0',
            exportType: 'tags',
            createdAt: '2026-01-01T00:00:00.000Z',
            appVersion: '4.0.0',
            settings: { includeMemories: false, scope: 'all', selectedIds: [] },
            counts: {},
          },
          data: { tags: [{ id: 'zz000000-0000-4000-8000-0000000000ff', name: 'Imported Tag' }] },
        }),
        'utf8',
      );
      const assembled = await loadQtap(legacy);
      return { kind: 'read', qtapBase64: legacy.toString('base64'), assembled };
    },
  });

  // A cross-instance character export: the preview must match by NAME and carry
  // `matchedExistingId`. Built by hand so the ids differ from the fixture's.
  cases.push({
    name: 'preview_cross_instance_name_match',
    run: async (spec) => {
      const payload = {
        manifest: {
          format: 'quilltap-export',
          version: '1.0',
          exportType: 'characters',
          createdAt: '2026-01-01T00:00:00.000Z',
          appVersion: '4.0.0',
          settings: { includeMemories: false, scope: 'all', selectedIds: [] },
          counts: {},
        },
        data: {
          characters: [
            // Same NAME as the fixture's Lorian, different id → name match.
            { id: 'zz000000-0000-4000-8000-000000000001', name: 'Lorian' },
            // Neither id nor name matches → no conflict.
            { id: 'zz000000-0000-4000-8000-000000000002', name: 'Nobody At All' },
          ],
          memories: [],
        },
      };
      const { previewImport } = await import('@/lib/import/quilltap-import-service');
      const preview = await previewImport(spec.userId, payload as never);
      return { kind: 'preview', payload, preview };
    },
  });

  // A COMPLETE two-chunk blob — the arm v4's `c1507f47` fix actually unblocked,
  // and the one this corpus had no case for.
  //
  // Before that commit, `assembleExportFromStream` asked `received.every(...)`
  // over a pre-sized SPARSE array; `every` skips holes, so the blob finalized on
  // chunk 0 and the accumulator was deleted — chunk 1 then hit "received without
  // preceding doc_mount_blob". v4 therefore could not re-import ANY blob larger
  // than its own `BLOB_CHUNK_BYTES`, and the only case here that noticed
  // (`throw_ndjson_truncated_blob`) exercised the SHORT stream, not the complete
  // one. A whole-stream case is what proves the fix restores the round trip
  // rather than merely relocating an error.
  cases.push({
    name: 'read_ndjson_multi_chunk_blob',
    run: async () => {
      const bytes = Buffer.from(
        '{"kind":"__envelope__","format":"qtap-ndjson","version":1,"manifest":{"exportType":"document-stores"}}\n' +
          '{"kind":"doc_mount_blob","data":{"mountPointId":"m","relativePath":"a","sha256":"s","chunkCount":2}}\n' +
          '{"kind":"doc_mount_blob_chunk","mountPointId":"m","sha256":"s","index":0,"total":2,"dataBase64":"AAAA"}\n' +
          '{"kind":"doc_mount_blob_chunk","mountPointId":"m","sha256":"s","index":1,"total":2,"dataBase64":"BBBB"}\n',
        'utf8',
      );
      const assembled = await loadQtap(bytes);
      return { kind: 'read', qtapBase64: bytes.toString('base64'), assembled };
    },
  });

  // [P4.D46 tier 2] The forward-compat rule, asserted from v4's own checklist
  // (`types.ts:48-52`): a reader WARNS-AND-SKIPS a record `kind` it doesn't
  // know, but THROWS on an unknown `manifest.exportType`. The synthetic
  // future-kind line must vanish on both sides with the tags round-tripping
  // untouched; the future exportType must reach `buildExportDataForType`'s
  // throw with its exact message.
  cases.push({
    name: 'read_ndjson_unknown_kind_skips',
    run: async () => {
      const bytes = Buffer.from(
        '{"kind":"__envelope__","format":"qtap-ndjson","version":1,"manifest":{"exportType":"tags"}}\n' +
          '{"kind":"tag","data":{"id":"zz000000-0000-4000-8000-000000000010","name":"Kept"}}\n' +
          '{"kind":"holographic_ledger","data":{"id":"zz000000-0000-4000-8000-000000000011"}}\n' +
          '{"kind":"__footer__","counts":{"tags":1}}\n',
        'utf8',
      );
      const assembled = await loadQtap(bytes);
      return { kind: 'read', qtapBase64: bytes.toString('base64'), assembled };
    },
  });

  // Malformed inputs — the reader's throwing arms (v4's exact messages).
  const throwCase = (name: string, bytes: Buffer): CaseSpec => ({
    name,
    run: async () => {
      try {
        await loadQtap(bytes);
        return { kind: 'throw', qtapBase64: bytes.toString('base64'), error: null };
      } catch (err) {
        return {
          kind: 'throw',
          qtapBase64: bytes.toString('base64'),
          error: err instanceof Error ? err.message : String(err),
        };
      }
    },
  });

  cases.push(
    throwCase(
      'throw_ndjson_missing_envelope',
      Buffer.from('{"kind":"tag","format":"qtap-ndjson","data":{}}\n', 'utf8'),
    ),
  );
  cases.push(
    throwCase(
      'throw_ndjson_bad_line',
      Buffer.from(
        '{"kind":"__envelope__","format":"qtap-ndjson","version":1,"manifest":{"exportType":"tags"}}\nnot json\n',
        'utf8',
      ),
    ),
  );
  cases.push(
    throwCase(
      'throw_ndjson_unknown_export_type',
      Buffer.from(
        '{"kind":"__envelope__","format":"qtap-ndjson","version":1,"manifest":{"exportType":"chrono-ledgers"}}\n',
        'utf8',
      ),
    ),
  );
  cases.push(
    throwCase(
      'throw_ndjson_bad_version',
      Buffer.from(
        '{"kind":"__envelope__","format":"qtap-ndjson","version":2,"manifest":{"exportType":"tags"}}\n',
        'utf8',
      ),
    ),
  );
  cases.push(
    throwCase(
      'throw_ndjson_truncated_blob',
      Buffer.from(
        '{"kind":"__envelope__","format":"qtap-ndjson","version":1,"manifest":{"exportType":"document-stores"}}\n' +
          '{"kind":"doc_mount_blob","data":{"mountPointId":"m","relativePath":"a","sha256":"s","chunkCount":2}}\n' +
          '{"kind":"doc_mount_blob_chunk","mountPointId":"m","sha256":"s","index":0,"total":2,"dataBase64":"AA=="}\n',
        'utf8',
      ),
    ),
  );
  cases.push(throwCase('throw_legacy_bad_json', Buffer.from('{not json at all', 'utf8')));

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
    const out = await c.run(spec);
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-sysimport-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [
    JSON.stringify({ name: '_meta', kind: 'meta', userId: spec.userId }),
  ];
  for (const c of buildCases()) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`system-import oracle wrote ${outPath} (${outLines.length - 1} cases)\n`);
}

test('system-import oracle', async () => {
  await main();
});
