/**
 * @jest-environment node
 *
 * Tier-3 (mocked-completion + mocked-embedding) ORACLE for the Carina
 * memory-extraction job handler (v4 `handleCarinaMemoryExtraction`,
 * lib/background-jobs/handlers/carina-memory-extraction.ts).
 *
 * Drives v4's REAL `handleCarinaMemoryExtraction` over the committed corpus
 * (harness/oracle/fixtures/carina-memory-extraction-tier3.json) against the REAL
 * two-database fixture, mocking ONLY the model/infra seams the Rust port injects:
 *
 *   - `createLLMProvider` — the returned provider's `sendMessage` resolves each
 *     call to a corpus rule keyed on (pass, CONTEXT-footer label prefix), RECORDS
 *     the exact `provider|model|temperature|messages` canned key it answered
 *     (emitted as `kind:"canned"` rows), and returns the rule's fixed
 *     content + usage. The Rust side replays exactly those recorded entries
 *     through `CannedCompletionProvider`, so a prompt/selection/temperature
 *     divergence surfaces as a canned-miss. Only SELF passes occur (the synthetic
 *     transcript carries no user-controlled character → OTHER self-skips).
 *   - `generateEmbeddingForUser` — the corpus's canned vectors keyed by the exact
 *     `${summary}\n\n${content}` text (as in the memory-gate oracle).
 *   - `getApiKeyForCheapLLMSelection` → a constant (key management is host-side).
 *   - `logLLMCall` → no-op.
 *   - `getMemoryExtractionLimits` (@/lib/instance-settings) → the corpus limits
 *     (v4's instance-settings read; the Rust port injects the same value).
 *   - `estimateMessageCost` (@/lib/services/cost-estimation.service) → a canned
 *     cost, so the `MEMORY_EXTRACTION` system event + the chats cost aggregate
 *     match the Rust `CarinaCostEstimator`.
 *
 * Everything else — the handler, the transcript reconstruction, selection, the
 * extraction passes, the memory gate, the repositories, the danger resolver — is
 * v4's REAL code against the REAL fixture DBs.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=${V5W:-$HOME/source/quilltap-v5}   # the v5 checkout (or your worktree)
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-carina-mem-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-carina-mem-mount.db \
 *     $N/npx tsx $W/harness/oracle/fixtures/build-carina-memory-extraction-fixture.ts
 *   mkdir -p /tmp/carina-mem-oracle/cases /tmp/carina-mem-oracle/fixtures
 *   cp $W/harness/oracle/cases/carina-memory-extraction-tier3.test.ts /tmp/carina-mem-oracle/cases/
 *   cp $W/harness/oracle/fixtures/carina-memory-extraction-tier3.json /tmp/carina-mem-oracle/fixtures/
 *   QT_FIXTURE_CARINA_MEM_MAIN=/tmp/qt-carina-mem-main.db QT_FIXTURE_CARINA_MEM_MOUNT=/tmp/qt-carina-mem-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-carina-mem.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 --roots "$PWD" --roots "/tmp/carina-mem-oracle/cases" -- carina-memory-extraction-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

// Inlined canonicalizer (same as the other tier-2/3 oracles): BLOBs → hex,
// nulls explicit, rows sorted by `orderBy` code-unit string order.
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
  pass: 'self' | 'other';
  namePrefix: string;
  response: string;
  usage: { promptTokens: number; completionTokens: number; totalTokens: number };
}
interface CaseSpec {
  name: string;
  chatId: string;
  carinaMessageId: string;
  answererId: string;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  connectionProfileId: string;
  estimatedCostUsd: number;
  memoryExtractionLimits: {
    enabled: boolean;
    maxPerHour: number;
    softStartFraction: number;
    softFloor: number;
  };
  cannedEmbeddings: Record<string, number[]>;
  completionRules: CompletionRule[];
  cases: CaseSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'carina-memory-extraction-tier3.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_CARINA_MEM_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_CARINA_MEM_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error(
      'QT_FIXTURE_CARINA_MEM_MAIN / QT_FIXTURE_CARINA_MEM_MOUNT must point at the seed fixtures'
    );
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-carina-mem-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'carina-mem-main.db');
  const workMount = join(scratch, 'carina-mem-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // Recorded canned-completion entries, deduped by the exact call key. The key
  // format MUST match `quilltap_core::model::completion::canned_completion_key`.
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

  // Restore the real DB stack past jest.setup's global mocks, and pin the
  // model/infra seams (see file header). doMock is not hoisted, so it runs now
  // and its factories can close over `spec`.
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
    const { EmbeddingError } = actual;
    return {
      __esModule: true,
      ...actual,
      generateEmbeddingForUser: async (text: string) => {
        const vec = spec.cannedEmbeddings[text];
        if (!vec) {
          throw new EmbeddingError(
            `no canned embedding registered for input (${text.length} chars)`
          );
        }
        return {
          embedding: new Float32Array(vec),
          model: 'canned',
          dimensions: vec.length,
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
          const m = system.match(/\nCONTEXT\n(SUBJECT|OBSERVER): ([^\n]*)\n/);
          if (!m) throw new Error('canned completion: no CONTEXT footer label in system prompt');
          const pass = m[1] === 'SUBJECT' ? 'self' : 'other';
          const label = m[2];
          const rule = spec.completionRules.find(
            (r) => r.pass === pass && label.startsWith(r.namePrefix)
          );
          if (!rule) {
            throw new Error(`no completion rule for pass=${pass} label=${label}`);
          }
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
  jest.doMock('@/lib/services/llm-logging.service', () => {
    const actual = jest.requireActual('@/lib/services/llm-logging.service');
    return {
      __esModule: true,
      ...actual,
      logLLMCall: async () => undefined,
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
      estimateMessageCost: async () => ({
        cost: spec.estimatedCostUsd,
        source: 'registry',
        modelPricing: null,
      }),
    };
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { handleCarinaMemoryExtraction } = await import(
    '@/lib/background-jobs/handlers/carina-memory-extraction'
  );

  await initializeDatabase();

  const lines: string[] = [];

  for (const call of spec.cases) {
    const job = {
      id: `job-${call.name}`,
      userId: spec.userId,
      type: 'CARINA_MEMORY_EXTRACTION',
      payload: {
        chatId: call.chatId,
        carinaMessageId: call.carinaMessageId,
        answererId: call.answererId,
        connectionProfileId: spec.connectionProfileId,
      },
    };
    await handleCarinaMemoryExtraction(job as never);
    // Let the gate's fire-and-forget housekeeping probes settle before the next case.
    await new Promise((resolve) => setTimeout(resolve, 30));
    lines.push(JSON.stringify({ kind: 'ran', call: call.name }));
  }

  for (const entry of cannedRecorded.values()) {
    lines.push(JSON.stringify({ kind: 'canned', ...entry }));
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

  closeMountIndexSQLiteClient();
  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`carina-memory-extraction oracle wrote ${outPath}\n`);
}

test('carina-memory-extraction tier-3 oracle', async () => {
  await main();
});
