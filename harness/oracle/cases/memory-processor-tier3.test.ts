/**
 * @jest-environment node
 *
 * Tier-3 (mocked-completion + mocked-embedding) ORACLE for the memory processor
 * (v4 `processTurnForMemory`, lib/memory/memory-processor.ts).
 *
 * Drives v4's REAL `processTurnForMemory` over the committed corpus
 * (harness/oracle/fixtures/memory-processor-tier3.json) against the REAL
 * two-database fixture (main + mount-index — the OTHER-pass canon loader reads
 * `Others/<subject>.md` from a character vault), with ONLY the model/infra
 * seams stubbed:
 *
 *   - `createLLMProvider` — the returned provider's `sendMessage` resolves each
 *     call to a corpus rule keyed on (pass, CONTEXT-footer label, model,
 *     autonomous-clause presence), RECORDS the exact
 *     `provider|model|temperature|messages` canned key it answered (emitted as
 *     `kind: "canned"` rows), and returns the rule's fixed content + usage. The
 *     Rust side replays exactly those recorded entries through
 *     `CannedCompletionProvider`, so any prompt/selection/temperature
 *     divergence surfaces as a canned-miss.
 *   - `generateEmbeddingForUser` — the corpus's canned vectors keyed by the
 *     exact `${summary}\n\n${content}` text (as in the memory-gate oracle).
 *   - `getApiKeyForCheapLLMSelection` → a constant (key management is
 *     host-side; the Rust boundary starts at the provider call).
 *   - `logLLMCall` → no-op (a fire-and-forget llm-logs side channel, not part
 *     of this differential's diffed state).
 *
 * Everything else — selection, the execution pipeline, the extraction passes,
 * the canon loads, the rate limiter, the memory gate, the repositories — is
 * v4's REAL code against the REAL fixture DBs.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-memory-processor-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-memory-processor-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-memory-processor-fixture.ts
 *   QT_FIXTURE_PROCESSOR_MAIN=/tmp/qt-memory-processor-main.db \
 *   QT_FIXTURE_PROCESSOR_MOUNT=/tmp/qt-memory-processor-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-memory-processor.ndjson \
 *     $N/npx jest --silent --roots "$PWD" --roots "$V5/harness/oracle/cases" -- memory-processor-tier3
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
  label: string;
  model?: string;
  autonomous?: boolean;
  response: string;
  usage: { promptTokens: number; completionTokens: number; totalTokens: number };
}
interface ProfileSpec {
  id: string;
  provider: string;
  modelName: string;
  baseUrl: string | null;
  isCheap: boolean;
  isDangerousCompatible: boolean;
  parameters: Record<string, unknown> | null;
  maxTokens: number | null;
  modelClass: string | null;
}
interface CallSpec {
  name: string;
  chatId: string;
  projectId: string | null;
  projectDescription: string | null;
  chatContextSummary: string | null;
  cheapLLMSettings: { strategy: string; fallbackToLocal: boolean };
  dangerSettings: { mode: string; uncensoredTextProfileId?: string } | null;
  isDangerousChat: boolean;
  memoryExtractionLimits: {
    enabled: boolean;
    maxPerHour: number;
    softStartFraction: number;
    softFloor: number;
  } | null;
  sourceMessageTimestamp: string | null;
  dryRun: boolean;
  inAutonomousRoom: boolean;
  participantIds: string[];
  transcript: unknown;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  profiles: ProfileSpec[];
  currentProfileId: string;
  cannedEmbeddings: Record<string, number[]>;
  cannedFailures: string[];
  completionRules: CompletionRule[];
  calls: CallSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'memory-processor-tier3.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_PROCESSOR_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_PROCESSOR_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error(
      'QT_FIXTURE_PROCESSOR_MAIN / QT_FIXTURE_PROCESSOR_MOUNT must point at the seed fixtures'
    );
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-memory-processor-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'processor-main.db');
  const workMount = join(scratch, 'processor-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // Recorded canned-completion entries, deduped by the exact call key. The
  // key format MUST match `quilltap_core::model::completion::canned_completion_key`.
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

  // Restore the real DB stack past jest.setup's global mocks, and pin the four
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
        if (spec.cannedFailures.includes(text)) {
          throw new EmbeddingError(`canned failure for input (${text.length} chars)`);
        }
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
          const autonomous = system.includes('autonomous character-to-character');
          const rule = spec.completionRules.find(
            (r) =>
              r.pass === pass &&
              r.label === label &&
              (r.model === undefined || r.model === params.model) &&
              (r.autonomous === undefined || r.autonomous === autonomous)
          );
          if (!rule) {
            throw new Error(
              `no completion rule for pass=${pass} label=${label} model=${params.model} autonomous=${autonomous}`
            );
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

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { processTurnForMemory } = await import('@/lib/memory/memory-processor');

  await initializeDatabase();
  const repos = getRepositories();

  const mkProfile = (p: ProfileSpec) => ({
    id: p.id,
    provider: p.provider,
    modelName: p.modelName,
    baseUrl: p.baseUrl,
    isCheap: p.isCheap,
    isDangerousCompatible: p.isDangerousCompatible,
    parameters: p.parameters ?? undefined,
    maxTokens: p.maxTokens,
    modelClass: p.modelClass,
  });
  const availableProfiles = spec.profiles.map(mkProfile);
  const currentProfile = availableProfiles.find((p) => p.id === spec.currentProfileId)!;

  const lines: string[] = [];

  for (const call of spec.calls) {
    const participantCharacters = new Map<string, unknown>();
    if (call.participantIds.length > 0) {
      const chars = (await repos.characters.findByIds(call.participantIds)) as Array<{
        id: string;
      }>;
      for (const c of chars) participantCharacters.set(c.id, c);
    }

    const result = await processTurnForMemory({
      transcript: call.transcript as never,
      participantCharacters: participantCharacters as never,
      chatId: call.chatId,
      projectId: call.projectId,
      projectDescription: call.projectDescription,
      chatContextSummary: call.chatContextSummary,
      userId: spec.userId,
      connectionProfile: currentProfile as never,
      cheapLLMSettings: call.cheapLLMSettings as never,
      availableProfiles: availableProfiles as never,
      dangerSettings: (call.dangerSettings ?? undefined) as never,
      isDangerousChat: call.isDangerousChat,
      memoryExtractionLimits: (call.memoryExtractionLimits ?? undefined) as never,
      sourceMessageTimestamp: call.sourceMessageTimestamp ?? undefined,
      dryRun: call.dryRun,
      inAutonomousRoom: call.inAutonomousRoom,
    });

    lines.push(JSON.stringify({ kind: 'result', call: call.name, result }));
  }

  // Let the gate's fire-and-forget housekeeping probes settle before closing.
  await new Promise((resolve) => setTimeout(resolve, 100));

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

  closeMountIndexSQLiteClient();
  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`memory-processor oracle wrote ${outPath}\n`);
}

test('memory-processor tier-3 oracle', async () => {
  await main();
});
