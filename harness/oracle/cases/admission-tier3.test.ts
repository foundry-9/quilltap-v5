/**
 * @jest-environment node
 *
 * Tier-3 (mocked-embedding) ORACLE for ADMISSION (P4.D290) — v4's REAL
 * `admitRow` / `admitCandidate` / `admitPrepared` (`lib/memory/admission/`,
 * the `83334b9a5` pin) in the PARENT (`QUILLTAP_JOB_CHILD` unset), over the
 * committed corpus (`harness/oracle/fixtures/admission-tier3.json`) and the
 * seed pair `build-admission-fixture.ts` builds.
 *
 * ONLY the model call is pinned: `generateEmbeddingForUser` answers by the
 * call's `lexicalText` (`summary\n\ncontent` — what admission passes beside
 * the anchored text): a candidate's `_vector`, a seed's `reembedVector`, a
 * failure for `_fail` / `reembedFail`, else a deterministic hash vector. The
 * EXACT texts v4's real `embeddingTextFor` produced are emitted (`canned` /
 * `failure` lines) — the Rust side's `CannedEmbeddingProvider` is built from
 * them, never re-derived by hand. `maintenance/triggers` is a recorder
 * (LR-5 — the watermark call is the unifier's); v4's memory-index cache is
 * dropped before every index read (LR-11 — v5 keeps none, and the corpus's
 * vanish trigger deletes rows behind v4's cache).
 *
 * One LEG per invocation (`QT_ADMISSION_LEG=utc|chicago`): the Chicago leg
 * sets the seeded `chat_settings.timezone` first, so the memory calendar —
 * the anchor line, the fallback `when` phrase — moves.
 *
 * Emits NDJSON: one `step` line per step (outcomes, embedding calls, captured
 * lines, watermark calls, rate counts), the `canned` / `failure` map, the
 * tier-1 `contentKey` / `scopeKeys` rows (utc leg), and the `memories` dump.
 *
 * Recipe (from the v4 pin; see the Rust family's header for the whole block).
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

/** The messages admission owns (§R.5 tag ownership): its three modules + the root `[Admission]` lines. */
const OWNED_MODULES = new Set(['memory:admission', 'memory:admission:neighbours', 'memory:admission:apply']);

function ownContext(ctx: Record<string, unknown> | undefined): Record<string, unknown> {
  const { service: _s, environment: _e, ...rest } = ctx ?? {};
  return rest;
}

/** Deterministic unit vector for a text nobody named (FNV-1a over UTF-16 code units + xorshift). */
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

type Json = Record<string, any>;

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'admission-tier3.json'), 'utf8')) as Json;
  const legName = process.env.QT_ADMISSION_LEG;
  const leg = (spec.legs as Json[]).find((l) => l.name === legName);
  if (!leg) throw new Error('QT_ADMISSION_LEG must name a leg (utc | chicago)');
  const fixtureMain = process.env.QT_FIXTURE_ADMISSION_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_ADMISSION_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_ADMISSION_MAIN / _MOUNT must point at the build-admission-fixture.ts pair');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-admission-oracle-'));
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

  // ---- the canned embedding map, by lexical text --------------------------
  const lexical = (r: Json) => `${r.summary}\n\n${r.content}`;
  const byLexical = new Map<string, { embedding: number[]; provider: string }>();
  const failing = new Set<string>();
  const rowsOf = (step: Json): Json[] =>
    step.row ? [step.row] : step.rows ? step.rows : step.candidate ? [{ ...step.candidate, _vector: step._vector }] : [];
  for (const l of spec.legs as Json[]) {
    for (const step of l.steps as Json[]) {
      for (const r of rowsOf(step)) {
        if (r._fail) failing.add(lexical(r));
        else if (r._vector) byLexical.set(lexical(r), { embedding: r._vector, provider: r._provider ?? 'canned' });
      }
    }
  }
  for (const s of spec.seeds as Json[]) {
    if (s.reembedFail) failing.add(lexical(s));
    if (s.reembedVector) byLexical.set(lexical(s), { embedding: s.reembedVector, provider: 'canned' });
  }
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
  jest.doMock('@/lib/embedding/embedding-service', () => {
    const actual = jest.requireActual('@/lib/embedding/embedding-service');
    const { EmbeddingError } = actual;
    return {
      __esModule: true,
      ...actual,
      generateEmbeddingForUser: async (text: string, _userId: string, _profileId?: string, opts?: Json) => {
        const key = opts?.lexicalText ?? text;
        calls.push({ text, lexicalText: opts?.lexicalText ?? null, priority: opts?.priority ?? null });
        if (failing.has(key)) {
          failures.set(text, FAILURE_MESSAGE);
          throw new EmbeddingError(FAILURE_MESSAGE);
        }
        const hit = byLexical.get(key) ?? { embedding: hashVector(key, 4), provider: 'canned' };
        const f32 = new Float32Array(hit.embedding);
        canned.set(text, { text, embedding: Array.from(f32), provider: hit.provider });
        return { embedding: f32, model: 'canned', dimensions: f32.length, provider: hit.provider };
      },
    };
  });
  let watermark: string[][] = [];
  jest.doMock('@/lib/memory/maintenance/triggers', () => ({
    __esModule: true,
    maybeEnqueueMaintenance: async (ids: readonly string[]) => {
      watermark.push([...ids]);
      return [];
    },
  }));

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
  const admit = await import('@/lib/memory/admission/admit');
  const { applyNamePresenceCheck, applyEpisodicFallbackAnchors } = await import('@/lib/memory/admission/prepare');
  const { createDryRunBook } = await import('@/lib/memory/admission/dry-run-book');
  const { countRecentAdmissions, resetRateLedgerForTests } = await import('@/lib/memory/admission/rate-ledger');
  const { MemoryCandidateSchema } = await import('@/lib/memory/candidate');
  const { memoryIndex, embeddingTextFor, lexicalEmbeddingTextFor } = await import('@/lib/memory/index-store');
  const { refreshMemoryTimeZone } = await import('@/lib/memory/time/calendar-refresh');
  const { resetMemoryTimeZoneCacheForTests } = await import('@/lib/memory/time/calendar');
  const { generateEmbeddingForUser } = await import('@/lib/embedding/embedding-service');
  const { runWithJobScope, nextAdmissionKey } = await import('@/lib/background-jobs/child/child-repositories-proxy');
  const { Logger } = await import('@/lib/logger');

  let sink: Json[] | null = null;
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const proto = Logger.prototype as unknown as Record<string, (...a: unknown[]) => void>;
    const original = proto[level];
    proto[level] = function (this: { context?: Record<string, unknown> }, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
      const merged = { ...ownContext(this.context), ...(context ?? {}) };
      const owned = OWNED_MODULES.has(merged.module as string) || (typeof message === 'string' && message.startsWith('[Admission]'));
      if (sink && owned) sink.push({ level, message, context: JSON.parse(JSON.stringify(merged)) });
      return original.call(this, message, context, ...rest);
    };
  }

  // LR-11: v5 keeps no index cache — every read loads the rows. v4's cache
  // stays coherent through its own writers (`retireMemories` evicts), but the
  // corpus's vanish trigger deletes a row behind it, so the cache is dropped
  // before EVERY index read here: both sides measure the same rows.
  const idx = memoryIndex as unknown as Record<string, (...a: unknown[]) => unknown>;
  for (const method of ['search', 'shape', 'entries']) {
    const original = idx[method].bind(memoryIndex);
    idx[method] = (...a: unknown[]) => {
      memoryIndex.reloadAll();
      return original(...a);
    };
  }

  await initializeDatabase();
  admit.resetAdmissionStateForTests();
  resetRateLedgerForTests();
  resetMemoryTimeZoneCacheForTests();
  if (leg.timezone) {
    await rawQuery(`UPDATE chat_settings SET timezone = '${leg.timezone}'`);
  }

  const userId = spec.userId as string;
  const clean = (r: Json): Json => {
    const out: Json = {};
    for (const [k, v] of Object.entries(r)) if (!k.startsWith('_')) out[k] = v;
    return out;
  };
  const settle = async () => {
    for (let i = 0; i < 10; i++) await new Promise((r) => setImmediate(r));
  };

  /** v4 `admitRow`'s preparation (minus `trackActivity` and the retry), then `admitPrepared` with the minted key. */
  const admitKeyed = async (row: Json) => {
    const tz = await refreshMemoryTimeZone();
    const notes: string[] = [];
    let prepared = await applyNamePresenceCheck(row as never, notes);
    prepared = applyEpisodicFallbackAnchors(prepared, tz);
    const { embedding, provider } = await generateEmbeddingForUser(embeddingTextFor(prepared as never, tz), userId, undefined, {
      priority: 'background',
      lexicalText: lexicalEmbeddingTextFor(prepared as never),
    } as never);
    const idempotencyKey = nextAdmissionKey(admit.admissionContentKey(prepared));
    return admit.admitPrepared({
      row: prepared,
      embedding,
      embeddingProvider: provider ?? null,
      userId,
      embeddingAttempts: 1,
      notes,
      idempotencyKey,
    });
  };

  const lines: string[] = [];
  for (const step of leg.steps as Json[]) {
    memoryIndex.reloadAll();
    calls = [];
    watermark = [];
    sink = [];
    let outcomes: unknown[] = [];
    let counts: number[] | undefined;
    if (step.op === 'admit') {
      outcomes = [await admit.admitRow(clean(step.row) as never, {
        userId,
        ...(step.dryRun ? { dryRun: true } : {}),
        ...(step.backfill ? { backfill: true } : {}),
      })];
    } else if (step.op === 'candidate') {
      const candidate = MemoryCandidateSchema.parse(step.candidate);
      outcomes = [await admit.admitCandidate(candidate, step.resolved, { userId })];
    } else if (step.op === 'concurrent') {
      outcomes = await Promise.all((step.rows as Json[]).map((r) => admit.admitRow(clean(r) as never, { userId })));
    } else if (step.op === 'attempt') {
      outcomes = await runWithJobScope(step.jobId, async () => {
        const out: unknown[] = [];
        for (const r of step.rows as Json[]) out.push(await admitKeyed(clean(r)));
        return out;
      });
    } else if (step.op === 'dryRunBook') {
      const book = createDryRunBook(step.retiring);
      for (const r of step.rows as Json[]) {
        outcomes.push(await admit.admitRow(clean(r) as never, { userId, dryRun: true, dryRunBook: book }));
      }
    } else if (step.op === 'rateCounts') {
      counts = [];
      for (const id of step.characterIds as string[]) counts.push(await countRecentAdmissions(id, 0));
    } else {
      throw new Error(`unknown op ${step.op}`);
    }
    await settle();
    lines.push(JSON.stringify({
      kind: 'step', leg: leg.name, name: step.name, outcomes, embedCalls: calls, lines: sink,
      watermarkCalls: watermark, ...(counts ? { counts } : {}),
    }));
    sink = null;
  }

  if (leg.name === 'utc') {
    for (const r of spec.contentKeys as Json[]) {
      lines.push(JSON.stringify({ kind: 'contentKey', row: r, key: admit.admissionContentKey(r as never) }));
    }
    const keys = await runWithJobScope(spec.scopeKeys.jobId, async () =>
      (spec.scopeKeys.contentKeys as string[]).map((k) => nextAdmissionKey(k))
    );
    lines.push(JSON.stringify({ kind: 'scopeKeys', keys }));
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

  closeMountIndexSQLiteClient();
  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`admission oracle (${leg.name}) wrote ${outPath} (${leg.steps.length} steps)\n`);
}

test('admission tier-3 oracle', async () => {
  await main();
});

const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
