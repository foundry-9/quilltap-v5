/**
 * @jest-environment node
 *
 * Tier-3 (mocked cheap-LLM + mocked-embedding) ORACLE for the fold-time episode
 * pass (v4 `runFoldEpisodePass`, lib/memory/fold-episode-pass.ts).
 *
 * Drives v4's REAL `runFoldEpisodePass` over the committed corpus
 * (harness/oracle/fixtures/fold-episode-tier3.json) against a REAL fixture DB
 * pair, with only the two model calls pinned: `createLLMProvider` answers the
 * per-run canned episode JSON (RECORDING the exact
 * `provider|model|temperature|messages` key it answered, which the Rust side
 * replays through `CannedCompletionProvider` — so a prompt-byte divergence
 * surfaces as a canned-miss), and `generateEmbeddingForUser` answers the
 * corpus's canned vectors keyed by the exact `buildMemoryEmbeddingText` text.
 * Everything downstream — the gate, the fragment linking, the writes — is v4's
 * real code, and the three affected tables are structural-diffed against
 * `quilltap_core::services::fold_episode_pass` by the Rust harness.
 *
 * Run (Node 24, from the v4 checkout — cp the cases to a /tmp mirror; jest
 * ignores .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; W=<v5 worktree> ; M=/tmp/qt-d14-oracle
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-fold-episode-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-fold-episode-mount.db \
 *     $N/npx tsx $W/harness/oracle/fixtures/build-fold-episode-fixture.ts
 *   QT_FIXTURE_FOLD_EPISODE_MAIN=/tmp/qt-fold-episode-main.db \
 *   QT_FIXTURE_FOLD_EPISODE_MOUNT=/tmp/qt-fold-episode-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-fold-episode.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$M/cases" -- fold-episode-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

// Inlined from ../lib/tier2.ts (jest can't resolve the `.js` ESM specifier).
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

interface Usage {
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
}
interface RunSpec {
  name: string;
  chatId: string;
  timelineMode: 'realtime' | 'narrative';
  projectId: string | null;
  inAutonomousRoom: boolean;
  episodeResponse: string;
  usage: Usage;
  windowMessages: Array<{
    id: string;
    role: string;
    content: string;
    participantId: string | null;
    createdAt: string | null;
  }>;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  runs: RunSpec[];
  profile: {
    id: string;
    provider: string;
    modelName: string;
    baseUrl: string | null;
    isLocal: boolean;
    parameters: unknown;
  };
  cannedEmbeddings: Record<string, number[]>;
  cannedFailures: string[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'fold-episode-tier3.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_FOLD_EPISODE_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_FOLD_EPISODE_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error(
      'QT_FIXTURE_FOLD_EPISODE_MAIN / QT_FIXTURE_FOLD_EPISODE_MOUNT must point at the seed fixtures'
    );
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-fold-episode-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'fold-episode-main.db');
  const workMount = join(scratch, 'fold-episode-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'fold-episode-llm-logs.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // The current run's canned response (one completion call per pass).
  let currentRun: RunSpec | null = null;
  const cannedRecorded = new Map<
    string,
    {
      provider: string;
      model: string;
      temperature: number | null;
      messages: Array<{ role: string; content: string }>;
      response: string;
      usage: Usage;
    }
  >();
  const missingEmbeddings: string[] = [];

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
          missingEmbeddings.push(text);
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
          if (!currentRun) throw new Error('canned completion: no active run');
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
          const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!cannedRecorded.has(key)) {
            cannedRecorded.set(key, {
              provider,
              model: params.model,
              temperature: params.temperature ?? null,
              messages,
              response: currentRun.episodeResponse,
              usage: currentRun.usage,
            });
          }
          return {
            content: currentRun.episodeResponse,
            finishReason: 'stop',
            usage: currentRun.usage,
          };
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

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { runFoldEpisodePass } = await import('@/lib/memory/fold-episode-pass');

  await initializeDatabase();

  const selection = {
    provider: spec.profile.provider,
    modelName: spec.profile.modelName,
    baseUrl: spec.profile.baseUrl,
    connectionProfileId: spec.profile.id,
    isLocal: spec.profile.isLocal,
    profileParameters: spec.profile.parameters ?? undefined,
  };

  const lines: string[] = [];
  for (const run of spec.runs) {
    currentRun = run;
    const result = await runFoldEpisodePass({
      chatId: run.chatId,
      userId: spec.userId,
      windowMessages: run.windowMessages as never,
      cheapLLM: selection as never,
      timelineMode: run.timelineMode,
      projectId: run.projectId,
      inAutonomousRoom: run.inAutonomousRoom,
    });
    lines.push(JSON.stringify({ kind: 'result', run: run.name, result }));
  }
  currentRun = null;

  // Let the gate's fire-and-forget `void maybeEnqueueHousekeeping(...)` settle.
  await new Promise((resolve) => setTimeout(resolve, 200));

  for (const entry of cannedRecorded.values()) {
    lines.push(JSON.stringify({ kind: 'canned', ...entry }));
  }
  if (missingEmbeddings.length > 0) {
    // Loud: a missing canned embedding silently turns every write into
    // SKIP_EMBEDDING_FAILED on BOTH sides, which would pass a hollow diff.
    for (const text of missingEmbeddings) {
      process.stderr.write(`MISSING CANNED EMBEDDING: ${JSON.stringify(text)}\n`);
    }
    throw new Error(`${missingEmbeddings.length} canned embedding(s) missing — see stderr`);
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
  process.stderr.write(`fold-episode oracle wrote ${outPath}\n`);
}

test('fold-episode tier-3 oracle', async () => {
  await main();
});
