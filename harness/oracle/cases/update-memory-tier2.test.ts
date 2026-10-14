/**
 * @jest-environment node
 *
 * Tier-2 ORACLE (mocked embedding) for the HAND EDIT (P4.D290) — v4's REAL
 * `updateMemory` (`lib/memory/admission/update-memory.ts`, `af48f6cc2` Phase
 * B; bug 203) at the `83334b9a5` pin, in the parent, over a copy of the
 * admission seed pair (`build-admission-fixture.ts`) and the corpus's
 * `edits` (`harness/oracle/fixtures/admission-tier3.json`), run in order on
 * ONE copy.
 *
 * Only the model call is pinned: `generateEmbeddingForUser` answers a
 * deterministic hash vector by the call's `lexicalText`, and FAILS a text
 * containing `cannot be embedded`; the exact texts v4's real `embeddingTextFor`
 * produced are emitted (`canned` / `failure`) for the Rust side's provider.
 *
 * Emits per edit: the result, the embedding calls and the lines `memory:
 * update-memory` owns; then the `memories` dump and the holder vaults'
 * `Commonplace/` projection (links + documents with ISO stamps masked — the
 * mirror stamps `updatedAt` with now — and the folders).
 *
 * Recipe: see `update_memory_tier2_equivalence.rs`.
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

type Json = Record<string, any>;

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

function ownContext(ctx: Record<string, unknown> | undefined): Record<string, unknown> {
  const { service: _s, environment: _e, ...rest } = ctx ?? {};
  return rest;
}

function hashVector(text: string, dims: number): number[] {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  const out: number[] = [];
  let x = h || 1;
  for (let i = 0; i < dims; i++) {
    x ^= x << 13; x >>>= 0;
    x ^= x >>> 17;
    x ^= x << 5; x >>>= 0;
    out.push((x % 2000) / 1000 - 1);
  }
  const norm = Math.sqrt(out.reduce((s, y) => s + y * y, 0)) || 1;
  return out.map((y) => y / norm);
}

/** The Commonplace projections (no ids, no clocks). */
const PROJECTIONS = [
  {
    name: 'links',
    sql: `SELECT l.mountPointId, l.relativePath, l.fileName, f.fileType, f.source, d.content, d.plainTextLength
            FROM doc_mount_file_links l JOIN doc_mount_files f ON f.id = l.fileId
            LEFT JOIN doc_mount_documents d ON d.fileId = f.id
           WHERE l.relativePath LIKE 'Commonplace/%' ORDER BY l.relativePath, l.mountPointId`,
  },
  {
    name: 'folders',
    sql: `SELECT f.mountPointId, f.name, f.path FROM doc_mount_folders f WHERE f.path LIKE 'Commonplace%' ORDER BY f.path, f.mountPointId`,
  },
];

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'admission-tier3.json'), 'utf8')) as Json;
  const fixtureMain = process.env.QT_FIXTURE_ADMISSION_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_ADMISSION_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_ADMISSION_MAIN / _MOUNT must point at the build-admission-fixture.ts pair');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-update-memory-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'main.db');
  const workMount = join(scratch, 'mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  delete process.env.QUILLTAP_JOB_CHILD;
  delete process.env.QUILLTAP_TIMEZONE;
  process.env.LOG_LEVEL = 'error';
  (globalThis as Record<string, unknown>).__quilltapJobHost = { shuttingDown: true };

  const FAILURE_MESSAGE = 'canned embedding failure';
  const canned = new Map<string, { text: string; embedding: number[]; provider: string }>();
  const failures = new Map<string, string>();
  let calls: Json[] = [];

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () => jest.requireActual('@/lib/embedding/vector-store'));
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge')
  );
  jest.doMock('@/lib/file-storage/manager', () => jest.requireActual('@/lib/file-storage/manager'));
  jest.doMock('@/lib/embedding/embedding-service', () => {
    const actual = jest.requireActual('@/lib/embedding/embedding-service');
    const { EmbeddingError } = actual;
    return {
      __esModule: true,
      ...actual,
      generateEmbeddingForUser: async (text: string, _userId: string, _profileId?: string, opts?: Json) => {
        const key = opts?.lexicalText ?? text;
        calls.push({ text, lexicalText: opts?.lexicalText ?? null, priority: opts?.priority ?? null });
        if (key.includes('cannot be embedded')) {
          failures.set(text, FAILURE_MESSAGE);
          throw new EmbeddingError(FAILURE_MESSAGE);
        }
        const f32 = new Float32Array(hashVector(key, 4));
        canned.set(text, { text, embedding: Array.from(f32), provider: 'canned' });
        return { embedding: f32, model: 'canned', dimensions: f32.length, provider: 'canned' };
      },
    };
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { updateMemory } = await import('@/lib/memory/admission/update-memory');
  const { memoryIndex } = await import('@/lib/memory/index-store');
  const { resetMemoryTimeZoneCacheForTests } = await import('@/lib/memory/time/calendar');
  const { Logger } = await import('@/lib/logger');

  let sink: Json[] | null = null;
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const proto = Logger.prototype as unknown as Record<string, (...a: unknown[]) => void>;
    const original = proto[level];
    proto[level] = function (this: { context?: Record<string, unknown> }, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
      const merged = { ...ownContext(this.context), ...(context ?? {}) };
      if (sink && merged.module === 'memory:update-memory') {
        sink.push({ level, message, context: JSON.parse(JSON.stringify(merged)) });
      }
      return original.call(this, message, context, ...rest);
    };
  }

  // LR-11: v5 keeps no index cache.
  const idx = memoryIndex as unknown as Record<string, (...a: unknown[]) => unknown>;
  for (const method of ['search', 'shape', 'entries']) {
    const original = idx[method].bind(memoryIndex);
    idx[method] = (...a: unknown[]) => {
      memoryIndex.reloadAll();
      return original(...a);
    };
  }

  await initializeDatabase();
  resetMemoryTimeZoneCacheForTests();

  const lines: string[] = [];
  for (const edit of spec.edits as Json[]) {
    calls = [];
    sink = [];
    const result = await updateMemory(edit.characterId, edit.memoryId, edit.patch, { userId: spec.userId });
    for (let i = 0; i < 10; i++) await new Promise((r) => setImmediate(r));
    lines.push(JSON.stringify({ kind: 'edit', name: edit.name, result, embedCalls: calls, lines: sink }));
    sink = null;
  }
  for (const entry of canned.values()) lines.push(JSON.stringify({ kind: 'canned', ...entry }));
  for (const [text, message] of failures) lines.push(JSON.stringify({ kind: 'failure', text, message }));

  const columns = ((await rawQuery('PRAGMA table_info(memories)')) as Array<{ name: string }>).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM memories ORDER BY content, id')) as Json[];
  const rows = rawRows.map((r) => {
    const out: Json = {};
    for (const c of columns) out[c] = canonValue(r[c]);
    return out;
  });
  lines.push(JSON.stringify({ kind: 'dump', table: 'memories', columns, rows }));
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  for (const { name, sql } of PROJECTIONS) {
    lines.push(JSON.stringify({ kind: 'projection', name, rows: midb.prepare(sql).raw(true).all() }));
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`update-memory oracle wrote ${outPath} (${spec.edits.length} edits)\n`);
}

test('update-memory tier-2 oracle', async () => {
  await main();
});

const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
