/**
 * @jest-environment node
 *
 * P4.86 QTAP-SCHEMA ORACLE (tier 1, recorded corpus): drives v4's REAL
 * `validateQtapExport` (`lib/validation/qtap-schema-validator.ts` — ajv 2020
 * with `{allErrors: true, strict: false, validateFormats: true}` + `ajv-formats`
 * over `public/schemas/qtap-export.schema.json`) across a corpus of export
 * documents, and emits the INSTANCE it validated alongside v4's verdict.
 *
 * The corpus (`harness/oracle/fixtures/qtap-schema-validate.json`) is a recipe,
 * not a pile of documents:
 *   - `seeds`     — built here by v4's REAL exporter (`createNdjsonStream`,
 *                   `lib/export/ndjson-writer.ts`) over a FRESH copy of the
 *                   committed `system-data-*` fixture family, folded back into
 *                   the schema's document shape by v4's REAL importer
 *                   (`assembleExportFromStream`, `lib/import/
 *                   quilltap-import-stream.ts`). That fold is the only way a
 *                   whole `QuilltapExport` exists in v4 — the exporter streams
 *                   NDJSON records and the importer reassembles them.
 *   - `mutations` — a seed plus JSON-Pointer `set` / `remove` ops. A pointer
 *                   whose parent is missing THROWS (a silently-skipped op is a
 *                   vacuous arm).
 *   - `literals`  — hand-written shapes no exporter can produce (a non-object
 *                   root, an empty object, …).
 *
 * Because the instance travels in the row, the Rust side validates the exact
 * bytes v4 validated and never rebuilds an export: the family is DB-free on
 * the v5 side. The `dataBase64` payload of every blob/file record is replaced
 * by a short deterministic stand-in (`STUB_BASE64`) BEFORE validation on both
 * sides — the schema asserts only `type: string` + `contentEncoding` there
 * (annotation-only in both engines), the bytes are megabytes of fixture blob,
 * and the substitution is applied to the recorded instance so the comparison
 * stays byte-for-byte honest.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-qtap-schema-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/qtap-schema-validate.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/system-data.json" "$TMPO/fixtures/"
 *   cp "$V5W/harness/oracle/fixtures/qtap-schema-validate.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$V5W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *   QT_ORACLE_OUT=/tmp/oracle-qtap-schema.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- qtap-schema-validate
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

interface ExportOpts {
  type: string;
  scope: 'all' | 'selected';
  selectedIds?: string[];
  includeMemories?: boolean;
}

interface Op {
  op: 'set' | 'remove' | 'dropNulls';
  path: string;
  value?: unknown;
}

interface Corpus {
  seeds: Array<{ name: string; options: ExportOpts }>;
  mutations: Array<{ name: string; from: string; ops: Op[] }>;
  literals: Array<{ name: string; instance: unknown }>;
}

/** The stand-in every `dataBase64` payload is collapsed to (see the header). */
const STUB_BASE64 = 'UXVpbGx0YXA=';

function applyMocks(userId: string): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  // jest.setup.ts globally stubs the storage manager with canned bytes; the
  // exporter must read the fixture's REAL mount-blob bytes (the P4.D46 note).
  jest.doMock('@/lib/file-storage/manager', () =>
    jest.requireActual('@/lib/file-storage/manager'),
  );
  // The plugin registry is EMPTY under jest; plugin-config redaction resolves
  // manifests through it. Serve the REAL manifest.json from plugins/dist so
  // redaction takes production's branch (the P4.D46 note).
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

/** Drain a `ReadableStream<Uint8Array>` into its NDJSON lines. */
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
  return new TextDecoder('utf-8')
    .decode(merged)
    .split('\n')
    .filter((l) => l.length > 0);
}

/** Every `dataBase64` string, wherever it sits, becomes `STUB_BASE64`. */
function stubBlobBytes(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(stubBlobBytes);
  if (value && typeof value === 'object') {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      out[k] = k === 'dataBase64' && typeof v === 'string' ? STUB_BASE64 : stubBlobBytes(v);
    }
    return out;
  }
  return value;
}

function pointerParts(path: string): string[] {
  if (path === '') return [];
  if (!path.startsWith('/')) throw new Error(`not a JSON Pointer: ${path}`);
  return path
    .slice(1)
    .split('/')
    .map((p) => p.replace(/~1/g, '/').replace(/~0/g, '~'));
}

/** Deeply delete every property whose value is `null` (arrays keep length). */
function dropNulls(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(dropNulls);
  if (value && typeof value === 'object') {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      if (v === null) continue;
      out[k] = dropNulls(v);
    }
    return out;
  }
  return value;
}

function applyOp(root: unknown, op: Op): void {
  const parts = pointerParts(op.path);
  if (op.op === 'dropNulls') {
    if (parts.length === 0) throw new Error('dropNulls needs a container pointer, not the root');
    let parent: any = root;
    for (const key of parts.slice(0, -1)) parent = parent[key];
    const last = parts[parts.length - 1];
    if (parent === null || typeof parent !== 'object' || !(last in parent)) {
      throw new Error(`pointer ${op.path} is absent — the arm would be vacuous`);
    }
    parent[last] = dropNulls(parent[last]);
    return;
  }
  if (parts.length === 0) throw new Error('the root pointer is not a mutable target');
  let node: any = root;
  for (const key of parts.slice(0, -1)) {
    if (node === null || typeof node !== 'object') {
      throw new Error(`pointer ${op.path} walks through a non-container at ${key}`);
    }
    node = node[key];
  }
  const last = parts[parts.length - 1];
  if (node === null || typeof node !== 'object') {
    throw new Error(`pointer ${op.path} has no container parent`);
  }
  if (op.op === 'remove') {
    if (!(last in node)) {
      throw new Error(`pointer ${op.path} is already absent — the arm would be vacuous`);
    }
    if (Array.isArray(node)) node.splice(Number(last), 1);
    else delete node[last];
    return;
  }
  // `set` on an EXISTING key is a retype; on a new key it is an addition. Both
  // are intended, but a set that walks into a missing array element is not.
  if (Array.isArray(node) && !(last in node)) {
    throw new Error(`pointer ${op.path} indexes past the array — the arm would be vacuous`);
  }
  node[last] = op.value;
}

async function buildSeeds(
  spec: Spec,
  corpus: Corpus,
  scratch: string,
  fixtures: { main: string; mount: string; llm: string },
): Promise<Map<string, unknown>> {
  const seeds = new Map<string, unknown>();
  for (const seed of corpus.seeds) {
    jest.resetModules();
    applyMocks(spec.userId);

    const work = mkdtempSync(join(scratch, 'sd-'));
    copyFileSync(fixtures.main, join(work, 'main.db'));
    copyFileSync(fixtures.mount, join(work, 'mount.db'));
    copyFileSync(fixtures.llm, join(work, 'llm.db'));
    process.env.SQLITE_PATH = join(work, 'main.db');
    process.env.SQLITE_MOUNT_INDEX_PATH = join(work, 'mount.db');
    process.env.SQLITE_LLM_LOGS_PATH = join(work, 'llm.db');

    const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
    const { closeMountIndexSQLiteClient } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    await initializeDatabase();
    try {
      const { createNdjsonStream } = await import('@/lib/export/ndjson-writer');
      const { assembleExportFromStream } = await import('@/lib/import/quilltap-import-stream');
      const lines = await drain(createNdjsonStream(spec.userId, seed.options as never));
      async function* records(): AsyncGenerator<unknown> {
        for (const line of lines) yield JSON.parse(line);
      }
      const assembled = await assembleExportFromStream(records());
      seeds.set(seed.name, stubBlobBytes(assembled));
    } finally {
      await closeDatabase();
      closeMountIndexSQLiteClient();
      rmSync(work, { recursive: true, force: true });
    }
  }
  return seeds;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'system-data.json'), 'utf8'),
  ) as Spec;
  const corpus = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'qtap-schema-validate.json'), 'utf8'),
  ) as Corpus;

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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-qtap-schema-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const seeds = await buildSeeds(spec, corpus, scratch, fixtures);

  const instances: Array<{ name: string; kind: string; instance: unknown }> = [];
  for (const seed of corpus.seeds) {
    instances.push({ name: seed.name, kind: 'seed', instance: seeds.get(seed.name) });
  }
  // A mutation may derive from a seed OR from an earlier mutation (the
  // corpus's `*_denulled` base), so the pool grows as they are built.
  const pool = new Map<string, unknown>(seeds);
  for (const m of corpus.mutations) {
    if (!pool.has(m.from)) throw new Error(`mutation ${m.name} names an unknown base ${m.from}`);
    const instance = JSON.parse(JSON.stringify(pool.get(m.from)));
    for (const op of m.ops) {
      try {
        applyOp(instance, op);
      } catch (err) {
        throw new Error(
          `mutation ${m.name}: ${err instanceof Error ? err.message : String(err)}`,
        );
      }
    }
    pool.set(m.name, instance);
    instances.push({ name: m.name, kind: 'mutation', instance });
  }
  for (const l of corpus.literals) {
    instances.push({ name: l.name, kind: 'literal', instance: l.instance });
  }

  // v4's REAL validator, imported once with the schema read from `process.cwd()`.
  jest.resetModules();
  const { validateQtapExport } = await import('@/lib/validation/qtap-schema-validator');

  const outLines: string[] = [];
  for (const row of instances) {
    const result = validateQtapExport(row.instance);
    outLines.push(
      JSON.stringify({
        name: row.name,
        kind: row.kind,
        instance: row.instance,
        valid: result.valid,
        errors: result.errors,
      }),
    );
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  const invalid = outLines.length - instances.filter((_, i) => JSON.parse(outLines[i]).valid).length;
  process.stderr.write(
    `qtap-schema oracle wrote ${outPath} (${outLines.length} instances, ${invalid} invalid)\n`,
  );
  rmSync(scratch, { recursive: true, force: true });
}

test('qtap-schema-validate oracle', async () => {
  await main();
});
