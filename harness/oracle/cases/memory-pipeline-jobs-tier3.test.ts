/**
 * @jest-environment node
 *
 * Tier-3 (mocked-completion + recorded-embedding) ORACLE for the memory-pipeline
 * job handlers (P4.6bj units 2–3: v4 `handleMemoryExtraction`,
 * lib/background-jobs/handlers/memory-extraction.ts, and `handleContextSummary`,
 * lib/background-jobs/handlers/context-summary.ts).
 *
 * Drives v4's REAL handlers over the committed corpus
 * (harness/oracle/fixtures/memory-pipeline-jobs-tier3.json) against the REAL
 * two-database fixture, mocking ONLY the model/infra seams the Rust port injects:
 *
 *   - `createLLMProvider` — each call resolves to a corpus rule classified by
 *     the system prompt (extraction SELF/OTHER via the CONTEXT footer label;
 *     the summary fold / fold-episode / title tasks via their fixed opening
 *     lines), RECORDS the exact `provider|model|temperature|messages` canned
 *     key it answered (`kind:"canned"` rows), and returns the rule's content +
 *     usage. The Rust side replays those exact entries through
 *     `CannedCompletionProvider` — a prompt/selection/temperature divergence
 *     surfaces as a canned-miss.
 *   - `generateEmbeddingForUser` — RECORD-and-replay: any text embeds to the
 *     fixed unit vector e0 (dim 8) and the text is recorded
 *     (`kind:"cannedEmbedding"` rows); the Rust side registers exactly the
 *     recorded texts, so a divergent embedding input surfaces as a canned-miss.
 *   - `getApiKeyForCheapLLMSelection` → a constant (key management is host-side).
 *   - `logLLMCall` → the REAL one, wrapped (P4.144): every call's promise is
 *     kept and drained (`Promise.allSettled`) before the `llm_logs` dump — the
 *     orchestrator family's pending-drain shape — and `SQLITE_LLM_LOGS_PATH`
 *     points at a fresh scratch DB. The rows land as `{kind:"llmlogs"}` and the
 *     Rust side diffs them against a per-case `with_logging` executor (the
 *     spine's per-job construction).
 *   - `getMemoryExtractionLimits` → the corpus limits (the Rust port injects the
 *     same value).
 *   - `estimateMessageCost` → `{ cost: null }` (matching the Rust side's canned
 *     `None` — the pricing cascade is host-resolved and separately verified).
 *   - `ensureProcessorRunning` → no-op (the chained CHAT_DANGER_CLASSIFICATION
 *     row is state under test; the Rust side defers the runner the same way).
 *
 * Everything else — the handlers, the transcript builder, selection, the
 * extraction passes, the memory gate, `generateContextSummary` with its REAL
 * Librarian re-post / vault mirror / relevant-conversations refresh / cost
 * events (the Rust side runs `RealContextSummarySeams`; the two CS chats
 * reference unprovisioned characters so the vault arms no-op) — is v4's REAL
 * code against the REAL fixture DBs.
 *
 * Thrown handler errors are caught and recorded on the `ran` row (`error`);
 * the Rust side asserts the same error strings.
 *
 * Run from the v4 server checkout under Node 24 (TZ pinned — the fold's
 * [YYYY-MM-DD] prefixes and the episodic date math are zone-sensitive):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<the v5 worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-mpj-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-mpj-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-memory-pipeline-jobs-fixture.ts
 *   mkdir -p /tmp/qt-mpj-oracle/cases /tmp/qt-mpj-oracle/fixtures
 *   cp $V5/harness/oracle/cases/memory-pipeline-jobs-tier3.test.ts /tmp/qt-mpj-oracle/cases/
 *   cp $V5/harness/oracle/fixtures/memory-pipeline-jobs-tier3.json /tmp/qt-mpj-oracle/fixtures/
 *   TZ=UTC QT_FIXTURE_MPJ_MAIN=/tmp/qt-mpj-main.db QT_FIXTURE_MPJ_MOUNT=/tmp/qt-mpj-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-memory-pipeline-jobs.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 --roots "$PWD" --roots "/tmp/qt-mpj-oracle/cases" -- memory-pipeline-jobs-tier3
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
function canonicalizeRows(opts: {
  table: string;
  columns: string[];
  rawRows: Array<Record<string, unknown>>;
  orderBy?: string;
}): { table: string; columns: string[]; rows: Array<Record<string, unknown>> } {
  const { table, columns, rawRows, orderBy = 'id' } = opts;
  const rows = rawRows
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    })
    .sort((a, b) => {
      const av = String(a[orderBy] ?? '');
      const bv = String(b[orderBy] ?? '');
      return av < bv ? -1 : av > bv ? 1 : 0;
    });
  return { table, columns, rows };
}

interface CompletionRule {
  kind: 'self' | 'other' | 'fold' | 'episode' | 'title';
  namePrefix?: string;
  response: string;
  usage: { promptTokens: number; completionTokens: number; totalTokens: number };
}
interface MeCase {
  name: string;
  chatId: string;
  turnOpenerMessageId: string | null;
  extractionAnchorMessageId?: string;
}
interface CsCase {
  name: string;
  chatId: string;
  forceRegenerate?: boolean;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  connectionProfileId: string;
  memoryExtractionLimits: {
    enabled: boolean;
    maxPerHour: number;
    softStartFraction: number;
    softFloor: number;
  };
  completionRules: CompletionRule[];
  meCases: MeCase[];
  csCases: CsCase[];
}

const EMBED_VECTOR = [1, 0, 0, 0, 0, 0, 0, 0];

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'memory-pipeline-jobs-tier3.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_MPJ_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_MPJ_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_MPJ_MAIN / QT_FIXTURE_MPJ_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mpj-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'mpj-main.db');
  const workMount = join(scratch, 'mpj-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  // P4.144: a fresh llm-logs DB for the un-mocked `logLLMCall`.
  process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'mpj-llm-logs.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cannedRecorded = new Map<
    string,
    {
      provider: string;
      model: string;
      temperature: number | null;
      messages: Array<{ role: string; content: string }>;
      response: string;
      usage: CompletionRule['usage'];
    }
  >();
  const embeddingRecorded = new Set<string>();

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories')
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store')
  );
  jest.doMock('@/lib/embedding/embedding-service', () => {
    const actual = jest.requireActual('@/lib/embedding/embedding-service');
    return {
      __esModule: true,
      ...actual,
      generateEmbeddingForUser: async (text: string) => {
        embeddingRecorded.add(text);
        return {
          embedding: new Float32Array(EMBED_VECTOR),
          model: 'canned',
          dimensions: EMBED_VECTOR.length,
          provider: 'canned',
        };
      },
    };
  });
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (provider: string, _baseUrl?: string) => ({
        sendMessage: async (
          params: {
            messages: Array<{ role: string; content: string }>;
            model: string;
            temperature?: number;
          },
          _apiKey: string
        ) => {
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
          const system = messages.find((m) => m.role === 'system')?.content ?? '';
          let rule: CompletionRule | undefined;
          if (system.startsWith('You are updating an existing summary')) {
            rule = spec.completionRules.find((r) => r.kind === 'fold');
          } else if (system.startsWith('You are consolidating a batch')) {
            rule = spec.completionRules.find((r) => r.kind === 'episode');
          } else if (system.startsWith('Generate a literary title')) {
            rule = spec.completionRules.find((r) => r.kind === 'title');
          } else {
            const m = system.match(/\nCONTEXT\n(SUBJECT|OBSERVER): ([^\n]*)\n/);
            if (!m) throw new Error('canned completion: unclassifiable system prompt');
            const pass = m[1] === 'SUBJECT' ? 'self' : 'other';
            const label = m[2];
            rule = spec.completionRules.find(
              (r) => r.kind === pass && label.startsWith(r.namePrefix ?? '')
            );
          }
          if (!rule) throw new Error('no completion rule for call');
          const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!cannedRecorded.has(key)) {
            cannedRecorded.set(key, {
              provider,
              model: params.model,
              temperature: params.temperature ?? null,
              messages,
              response: rule.response,
              usage: rule.usage,
            });
          }
          return { content: rule.response, finishReason: 'stop', usage: rule.usage };
        },
      }),
    };
  });
  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return {
      __esModule: true,
      ...actual,
      getApiKeyForCheapLLMSelection: async () => 'test-key',
    };
  });
  // P4.144: the REAL `logLLMCall`, every promise kept so the dump can drain
  // them (the cheap-task logging is fire-and-forget; a fixed sleep can lose a
  // row) — `orchestrator-tier3.test.ts`'s wrapper, copied as a pattern.
  const pendingLogs: Array<Promise<unknown>> = [];
  jest.doMock('@/lib/services/llm-logging.service', () => {
    const actual = jest.requireActual('@/lib/services/llm-logging.service');
    return {
      __esModule: true,
      ...actual,
      logLLMCall: (...args: unknown[]) => {
        const p = (actual.logLLMCall as (...a: unknown[]) => Promise<unknown>)(...args);
        pendingLogs.push(p);
        return p;
      },
    };
  });
  jest.doMock('@/lib/instance-settings', () => {
    const actual = jest.requireActual('@/lib/instance-settings');
    return {
      __esModule: true,
      ...actual,
      getMemoryExtractionLimits: async () => spec.memoryExtractionLimits,
    };
  });
  jest.doMock('@/lib/services/cost-estimation.service', () => {
    const actual = jest.requireActual('@/lib/services/cost-estimation.service');
    return {
      __esModule: true,
      ...actual,
      estimateMessageCost: async () => ({ cost: null, source: 'registry', modelPricing: null }),
    };
  });
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { handleMemoryExtraction } = await import(
    '@/lib/background-jobs/handlers/memory-extraction'
  );
  const { handleContextSummary } = await import(
    '@/lib/background-jobs/handlers/context-summary'
  );

  await initializeDatabase();

  const lines: string[] = [];

  for (const call of spec.meCases) {
    const job = {
      id: `job-${call.name}`,
      userId: spec.userId,
      type: 'MEMORY_EXTRACTION',
      payload: {
        chatId: call.chatId,
        turnOpenerMessageId: call.turnOpenerMessageId,
        extractionAnchorMessageId: call.extractionAnchorMessageId ?? null,
        connectionProfileId: spec.connectionProfileId,
      },
    };
    let error: string | null = null;
    try {
      await handleMemoryExtraction(job as never);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
    // Let the gate's fire-and-forget probes settle before the next case.
    await new Promise((resolve) => setTimeout(resolve, 50));
    lines.push(JSON.stringify({ kind: 'ran', call: call.name, error }));
  }

  for (const call of spec.csCases) {
    const job = {
      id: `job-${call.name}`,
      userId: spec.userId,
      type: 'CONTEXT_SUMMARY',
      payload: {
        chatId: call.chatId,
        connectionProfileId: spec.connectionProfileId,
        ...(call.forceRegenerate !== undefined ? { forceRegenerate: call.forceRegenerate } : {}),
      },
    };
    let error: string | null = null;
    try {
      await handleContextSummary(job as never);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
    lines.push(JSON.stringify({ kind: 'ran', call: call.name, error }));
  }

  for (const entry of cannedRecorded.values()) {
    lines.push(JSON.stringify({ kind: 'canned', ...entry }));
  }
  for (const text of embeddingRecorded.values()) {
    lines.push(JSON.stringify({ kind: 'cannedEmbedding', text, vector: EMBED_VECTOR }));
  }

  const dumpTable = async (table: string, orderBy: string) => {
    const columns = (
      (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('memories', 'content')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('vector_indices', 'id')) }));
  lines.push(
    JSON.stringify({ kind: 'table', ...(await dumpTable('vector_entries', 'embedding')) })
  );
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chat_messages', 'content')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chats', 'id')) }));
  lines.push(
    JSON.stringify({ kind: 'table', ...(await dumpTable('background_jobs', 'type')) })
  );

  // P4.144: the `llm_logs` rows the un-mocked `logLLMCall` wrote — every
  // pending call drained first, read through the llm-logs handle BEFORE
  // closeDatabase(); id/createdAt/updatedAt placeholdered, sorted by canonical
  // JSON. v4 creates the table LAZILY, so a run that logged nothing has none:
  // that is loud here (this family's extraction + fold calls always log).
  await Promise.allSettled(pendingLogs);
  const { getRawLLMLogsDatabase } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const lldb = getRawLLMLogsDatabase();
  if (!lldb) throw new Error('llm-logs DB handle unavailable (degraded open?)');
  const hasTable = lldb
    .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'llm_logs'")
    .get();
  if (!hasTable) throw new Error('no llm_logs table — v4 logged no cheap-LLM call at all');
  const llColumns = (lldb.pragma('table_info(llm_logs)') as Array<{ name: string }>).map(
    (c) => c.name
  );
  const llRawRows = lldb.prepare('SELECT * FROM llm_logs').all() as Array<Record<string, unknown>>;
  const llRows = llRawRows
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
  lines.push(JSON.stringify({ kind: 'llmlogs', columns: llColumns, rows: llRows }));

  closeMountIndexSQLiteClient();
  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`memory-pipeline-jobs oracle wrote ${outPath}\n`);
}

test('memory-pipeline-jobs tier-3 oracle', async () => {
  await main();
});

// Remove the OS-temp scratch dir(s) once the oracle has written its NDJSON.
const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
