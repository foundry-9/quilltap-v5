/**
 * @jest-environment node
 *
 * P4.D269 — tier-3 ORACLE for the F2 anchor-gate probe (v4 `3f7320138`; at
 * the `83334b9a5` pin since P4.D290 — the probe decides through admission's
 * `decideAdmission`, with `lexicalProfile` / `note`): drives v4's REAL
 * `runAnchorGateProbe`
 * (`lib/memory/anchor-gate-probe.ts`) and the REAL `POST
 * /api/v1/memories?action=anchor-gate-probe` route (`handleAnchorGateProbe`,
 * through `createContextHandler`, so the Zod 400 and the 404 / 500 arms are
 * v4's own) over a FRESH copy per case of the committed episodic-recall pair.
 * No v4 test exists for the probe or its route (measured at the pin), so this
 * case drives the real code directly.
 *
 * Nothing is mocked but auth and the startup gate (the memories-routes mock
 * set) and, in ONE case (`failText`), a wrapper around the REAL
 * `generateEmbeddingForUser` that rejects that exact text, and in ONE case
 * (`neural`, LR-14) a scripted NEURAL provider whose exact texts are emitted
 * (`canned`) for the Rust side. Embeddings run REAL
 * through the fixture's fitted BUILTIN TF-IDF profile on both sides —
 * deterministic and free. Each copy gets v4's REAL `add-memory-tiers-v1`
 * (`applyMemoryTiersMigration`, C1 §13), then the corpus's `basePlant` and the
 * case's own `plant` statements (`anchor-gate-probe-cases.json`, read by BOTH
 * sides).
 *
 * v4's log lines are captured by splicing a sink into the singleton logger's
 * shared transports (children included); the `service` / `module` /
 * `environment` child fields are dropped (v5 renders them as the tracing
 * target, never as call-site keys) and `error.message` folds in as `error`.
 * The lines the probe family compares are listed in `CAPTURED`; every other
 * warn/error line lands in `otherLogs` (measurement only, not compared).
 *
 * Emits one NDJSON line per case: `{ name, kind, result | threw | (status,
 * body), logs, otherLogs }`.
 *
 * Run: see the recipe in crates/quilltap-harness/tests/anchor_gate_probe_tier3_equivalence.rs.
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

// C1 §13 (P4.D265): v4's REAL `add-memory-tiers-v1` on the per-case copy —
// staged beside this case (`$STAGE/harness/oracle/fixtures/`, see the recipe).
import { applyMemoryTiersMigration } from '../fixtures/apply-memory-tiers-migration';

interface CaseSpec {
  name: string;
  kind: 'service' | 'route';
  input?: { characterId?: string; limit?: number };
  body?: unknown;
  plant?: string[];
  failText?: string;
  failMessage?: string;
  /** LR-14: a scripted NEURAL provider (see the corpus `$comment`). */
  neural?: boolean;
}
interface Corpus {
  characterId: string;
  userId: string;
  basePlant: string[];
  cases: CaseSpec[];
}

/** Deterministic unit vector for a key (FNV-1a over UTF-16 code units + xorshift). */
function hashVector(key: string, dims: number): number[] {
  let h = 0x811c9dc5;
  for (let i = 0; i < key.length; i++) {
    h ^= key.charCodeAt(i);
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
  const norm = Math.sqrt(out.reduce((acc, y) => acc + y * y, 0)) || 1;
  return out.map((y) => y / norm);
}

/** The lines the family compares (message → compared). */
const CAPTURED = new Set([
  'Probe embedding dimension does not match the store; skipping row',
  'Probe failed for row; continuing',
  'Anchor gate probe complete',
  '[Memories API] Running anchor gate probe',
  '[Memories API] Anchor gate probe failed',
  'Error finding entities by filter',
  'Error finding recent memories',
  'Error finding memories by IDs',
]);

interface LogLine {
  level: string;
  message: string;
  fields: Record<string, unknown>;
}

function mockRequest(url: string, body: unknown): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body),
  };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const corpus = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'anchor-gate-probe-cases.json'), 'utf8'),
  ) as Corpus;
  const fixtureSpec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'episodic-recall.json'), 'utf8'),
  ) as { testPepperBase64: string };

  const mainFixture = process.env.QT_FIXTURE_ER_MAIN;
  const mountFixture = process.env.QT_FIXTURE_ER_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_ER_MAIN and QT_FIXTURE_ER_MOUNT must point at the fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );

  const lines: string[] = [];
  for (const c of corpus.cases) {
    const scratch = mkdtempSync(join(tmpdir(), 'qt-agp-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'main-work.db');
    const mountWork = join(scratch, 'mount-work.db');
    copyFileSync(mainFixture, mainWork);
    copyFileSync(mountFixture, mountWork);
    await applyMemoryTiersMigration(mainWork, { pepperBase64: fixtureSpec.testPepperBase64 });

    process.env.ENCRYPTION_MASTER_PEPPER = fixtureSpec.testPepperBase64;
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    // DEBUG so the route's `Running anchor gate probe` line reaches the sink;
    // the console transport is spliced out below, so nothing reaches stdout.
    process.env.LOG_LEVEL = 'debug';

    jest.resetModules();
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () =>
      jest.requireActual('@/lib/database/repositories'),
    );
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
    jest.doMock('@/lib/embedding/vector-store', () =>
      jest.requireActual('@/lib/embedding/vector-store'),
    );
    // The REAL embedding service; in the `failText` case its one export the
    // probe calls rejects that exact text (and passes every other through).
    const failText = c.failText;
    const failMessage = c.failMessage ?? 'canned embedding failure';
    // LR-14: the scripted neural provider — a vector keyed by the text's
    // SUMMARY (its first `\n\n` part) at the store's width, provider OPENAI;
    // every exact text answered is emitted for the Rust side.
    const neural = c.neural === true;
    let neuralWidth = 0;
    const neuralCanned = new Map<string, number[]>();
    jest.doMock('@/lib/embedding/embedding-service', () => {
      const actual = jest.requireActual('@/lib/embedding/embedding-service');
      if (neural) {
        return {
          __esModule: true,
          ...actual,
          generateEmbeddingForUser: async (text: string) => {
            const f32 = new Float32Array(hashVector(text.split('\n\n')[0], neuralWidth));
            neuralCanned.set(text, Array.from(f32));
            return { embedding: f32, model: 'scripted', dimensions: f32.length, provider: 'OPENAI' };
          },
        };
      }
      if (failText === undefined) return actual;
      return {
        __esModule: true,
        ...actual,
        generateEmbeddingForUser: async (text: string, ...rest: unknown[]) => {
          if (text === failText) throw new Error(failMessage);
          return (actual.generateEmbeddingForUser as (...a: unknown[]) => unknown)(text, ...rest);
        },
      };
    });
    jest.doMock('@/lib/auth/session', () => ({
      __esModule: true,
      ...jest.requireActual('@/lib/auth/session'),
      getServerSession: async () => ({ user: { id: corpus.userId } }),
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

    const { logger } = await import('@/lib/logger');
    const captured: LogLine[] = [];
    const other: LogLine[] = [];
    const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
    internal.transports.splice(0, internal.transports.length, {
      write: (d: unknown) => {
        const data = d as {
          level: string;
          message: string;
          context?: Record<string, unknown>;
          error?: { message: string };
        };
        const {
          service: _s,
          module: _m,
          environment: _e,
          ...fields
        } = (data.context ?? {}) as Record<string, unknown>;
        if (data.error) fields.error = data.error.message;
        // JSON round-trip: an `undefined` field is OMITTED (§R.5).
        const line = JSON.parse(
          JSON.stringify({ level: data.level, message: data.message, fields }),
        ) as LogLine;
        if (CAPTURED.has(data.message)) captured.push(line);
        else if (data.level === 'warn' || data.level === 'error') other.push(line);
      },
    });

    const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
    const { closeMountIndexSQLiteClient } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    await initializeDatabase();
    const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
    await initializePlugins();
    for (const sql of [...corpus.basePlant, ...(c.plant ?? [])]) {
      await rawQuery(sql);
    }
    // LR-11: startup may already have loaded v4's memory-index cache; the
    // plants rewrote the rows behind it, so drop it (v5 keeps none).
    const { memoryIndex } = await import('@/lib/memory/index-store');
    memoryIndex.reloadAll();
    if (neural) {
      const entries = await memoryIndex.entries(corpus.characterId);
      neuralWidth = entries[0]?.embedding.length ?? 4;
    }
    captured.length = 0;
    other.length = 0;

    let rec: Record<string, unknown>;
    if (c.kind === 'service') {
      const { runAnchorGateProbe } = await import('@/lib/memory/anchor-gate-probe');
      const input = {
        characterId: c.input?.characterId ?? corpus.characterId,
        userId: corpus.userId,
        ...(c.input?.limit !== undefined ? { limit: c.input.limit } : {}),
      };
      try {
        rec = { name: c.name, kind: c.kind, result: await runAnchorGateProbe(input) };
      } catch (error) {
        rec = {
          name: c.name,
          kind: c.kind,
          threw: error instanceof Error ? error.message : String(error),
        };
      }
    } else {
      const { POST } = (await import('@/app/api/v1/memories/route')) as {
        POST: (...a: unknown[]) => Promise<{ status: number; json: () => Promise<unknown> }>;
      };
      const res = await POST(
        mockRequest('http://localhost/api/v1/memories?action=anchor-gate-probe', c.body),
      );
      rec = { name: c.name, kind: c.kind, status: res.status, body: await res.json() };
    }
    if (neural) rec.canned = Array.from(neuralCanned, ([text, embedding]) => ({ text, embedding }));
    rec.logs = captured.slice();
    rec.otherLogs = other.slice();

    // Let fire-and-forget promises settle before closing.
    for (let i = 0; i < 10; i++) await new Promise((r) => setImmediate(r));
    await new Promise((r) => setTimeout(r, 50));
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(scratch, { recursive: true, force: true });

    lines.push(JSON.stringify(rec));
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`anchor-gate-probe oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('anchor-gate-probe tier-3 oracle', async () => {
  await main();
}, 240000);
