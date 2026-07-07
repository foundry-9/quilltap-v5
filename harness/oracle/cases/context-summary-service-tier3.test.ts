/**
 * @jest-environment node
 *
 * Tier-3 (mocked-completion) ORACLE for the context-summary service half
 * (v4 `generateContextSummary` / `invalidateContextSummaryIfMessageCovered` /
 * `checkAndGenerateSummaryIfNeeded`, lib/chat/context-summary.ts).
 *
 * Drives v4's REAL context-summary functions over the committed corpus
 * (harness/oracle/fixtures/context-summary-service-tier3.json) against a REAL
 * single main-database fixture, with ONLY these seams stubbed:
 *
 *   - `createLLMProvider` — the returned provider's `sendMessage` resolves each
 *     fold / title / help-title call by (op, promptKind), RECORDS the exact
 *     `provider|model|temperature|messages` canned key it answered (emitted as
 *     `kind: "canned"` rows), and returns the op's fixed content + usage — or
 *     THROWS the op's canned error (the title-failure branch). The Rust side
 *     replays exactly those recorded entries through `CannedCompletionProvider`,
 *     so any prompt/selection/temperature divergence surfaces as a canned-miss.
 *   - `getApiKeyForCheapLLMSelection` → a constant (host-side; the Rust boundary
 *     starts at the provider call).
 *   - `logLLMCall` → no-op (a fire-and-forget llm-logs side channel).
 *   - W4.6b + Round-3 Group 7: `postLibrarianSummaryAnnouncement`,
 *     `createContextSummaryEvent` / `createTitleGenerationEvent`, the vault MIRROR
 *     (`writeConversationSummaryToVaults`) and the relevant-conversations REFRESH
 *     (`refreshRelevantConversationsOnFold`) all run REAL for `generate` ops
 *     (matching the ported `RealContextSummarySeams`) and no-op for the `check`-op
 *     internal fold (`NoopSeams`). `computeConversationStats` is always REAL (pure).
 *     `estimateMessageCost` returns `{ cost: null }` (host-resolved; the Rust seam
 *     passes `None`). The *sweep* of prior Librarian whispers is v4's REAL code.
 *   - `generateEmbeddingForUser` → a canned unit vector (dim 8, [1,0,…]) matching
 *     the fixture's seeded chunk, so the refresh's semantic search scores cosine
 *     1.0 and surfaces the pre-seeded prior summary (a `relevant-conversations`
 *     whisper into `chat_messages`).
 *
 * Everything else — the cheap-LLM selection, the execution pipeline, the fold /
 * title tasks, the turn partition, the gate, the title enqueue, the sweep, the
 * mirror + refresh, the repositories — is v4's REAL code against the REAL two-DB
 * fixture.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-ctxsum-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-ctxsum-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-context-summary-service-fixture.ts
 *   QT_FIXTURE_CTXSUM=/tmp/qt-ctxsum-main.db QT_FIXTURE_CTXSUM_MOUNT=/tmp/qt-ctxsum-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-context-summary-service.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- context-summary-service-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
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

type Kind = 'fold' | 'title' | 'help-title';
interface CompletionRule {
  op: string;
  kind: Kind;
  response?: string;
  fail?: string; // if set, throw this error message instead of answering
  usage?: { promptTokens: number; completionTokens: number; totalTokens: number };
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
interface OpSpec {
  name: string;
  kind: 'generate' | 'check' | 'invalidate';
  chatId: string;
  forceRegenerate?: boolean;
  messageIds?: string[];
}
interface OpsSpec {
  testPepperBase64: string;
  userId: string;
  profiles: ProfileSpec[];
  currentProfileId: string;
  dangerSettings: { mode: string; uncensoredTextProfileId?: string } | null;
  completionRules: CompletionRule[];
  ops: OpSpec[];
}

const DEFAULT_USAGE = { promptTokens: 40, completionTokens: 20, totalTokens: 60 };

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const ops = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'context-summary-service-ops.json'), 'utf8')
  ) as OpsSpec;

  const fixture = process.env.QT_FIXTURE_CTXSUM;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_CTXSUM must point at the seed fixture');
  }
  const fixtureMount = process.env.QT_FIXTURE_CTXSUM_MOUNT;
  if (!fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_CTXSUM_MOUNT must point at the seed mount-index fixture');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ctxsum-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'ctxsum-work.db');
  const workMount = join(scratch, 'ctxsum-work-mount.db');
  copyFileSync(fixture, work);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = ops.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
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
      usage: { promptTokens: number; completionTokens: number; totalTokens: number };
      fail?: string;
    }
  >();

  // The oracle processes ops sequentially. `currentOp` steers the mock's rule
  // lookup so each fold/title call resolves to its op's canned content.
  let currentOp = '';
  // The Librarian re-post + cost events (W4.6b) are driven by the ported
  // `RealContextSummarySeams` ONLY at the `generate_context_summary` entry point.
  // `check_and_generate_summary_if_needed`'s internal fold uses `NoopSeams` (the
  // spine caller is unchanged), so the `check`-op folds must NOT post those rows.
  // Mirror that here: run the real writers only for `generate` ops.
  let currentOpUsesRealSeams = false;

  // Classify the system prompt into a rule kind by its opening line (byte-stable
  // prompt bodies from v4 chat-tasks.ts).
  function classifyPrompt(system: string): Kind | null {
    if (system.startsWith('You are updating an existing summary of an ongoing roleplay conversation.'))
      return 'fold';
    if (system.startsWith('Generate a literary title for this conversation based on the summary provided'))
      return 'title';
    if (
      system.startsWith(
        'Generate a short, practical title for this help/support conversation based on the summary provided'
      )
    )
      return 'help-title';
    return null;
  }

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
  // jest.setup mocks the character-vault bridge to a fixed { mountPointId:
  // 'mock-vault-mount' }; un-mock it so `getCharacterVaultStore` (used by the vault
  // MIRROR + the relevant-conversations search's store check) resolves the REAL
  // minted vault via the real characters/docMountPoints repos ([[jest-real-db-oracle]]).
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge')
  );
  // Side-effect seams the mirror's reindex touches → keep the real modules (no-op
  // enqueue): the mount-index cache invalidation + embedding scheduler.
  jest.doMock('@/lib/mount-index/mount-chunk-cache', () => {
    const actual = jest.requireActual('@/lib/mount-index/mount-chunk-cache');
    return { __esModule: true, ...actual, invalidateMountPoint: () => {} };
  });
  jest.doMock('@/lib/mount-index/embedding-scheduler', () => {
    const actual = jest.requireActual('@/lib/mount-index/embedding-scheduler');
    return { __esModule: true, ...actual, enqueueEmbeddingJobsForMountPoint: async () => 0 };
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
          const kind = classifyPrompt(system);
          if (!kind) throw new Error('canned completion: unrecognized system prompt');
          const rule = ops.completionRules.find((r) => r.op === currentOp && r.kind === kind);
          if (!rule) {
            throw new Error(`no completion rule for op=${currentOp} kind=${kind}`);
          }
          const usage = rule.usage ?? DEFAULT_USAGE;
          const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!cannedRecorded.has(key)) {
            cannedRecorded.set(key, {
              provider,
              model: params.model,
              temperature: params.temperature ?? null,
              messages,
              response: rule.response ?? '',
              usage,
              ...(rule.fail ? { fail: rule.fail } : {}),
            });
          }
          if (rule.fail) {
            // Record the exact key so the Rust CannedCompletionProvider can
            // register the matching failure, then throw.
            throw new Error(rule.fail);
          }
          return { content: rule.response ?? '', finishReason: 'stop', usage };
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
    return { __esModule: true, ...actual, logLLMCall: async () => undefined };
  });
  // W4.6b + Round-3 Group 7: the Librarian re-post, the CONTEXT_SUMMARY /
  // TITLE_GENERATION cost events, the vault MIRROR
  // (`writeConversationSummaryToVaults` + `computeConversationStats`), and the
  // relevant-conversations REFRESH (`refreshRelevantConversationsOnFold`) all run
  // REAL for `generate` ops (matching `RealContextSummarySeams`) and stay no-ops
  // for the `check`-op internal fold (matching the `NoopSeams` the spine caller
  // keeps) — gated on `currentOpUsesRealSeams`. The Librarian *sweep* is always
  // v4's real code. The mirror + refresh write into the fixture's vault
  // (mount-index); the refresh's semantic search surfaces the pre-seeded prior
  // summary and posts a `relevant-conversations` whisper into `chat_messages`.
  jest.doMock('@/lib/services/librarian-notifications/writer', () => {
    const actual = jest.requireActual('@/lib/services/librarian-notifications/writer');
    return {
      __esModule: true,
      ...actual,
      postLibrarianSummaryAnnouncement: async (params: unknown) =>
        currentOpUsesRealSeams ? actual.postLibrarianSummaryAnnouncement(params) : undefined,
    };
  });
  // The vault MIRROR (`writeConversationSummaryToVaults` + `computeConversationStats`)
  // and the relevant-conversations REFRESH (`refreshRelevantConversationsOnFold`)
  // run v4's REAL code for EVERY op — they are NOT wrapped, because a
  // `jest.requireActual` wrapper would rebind their transitive mount-index client to
  // a fresh, disconnected instance (the main-DB writers survive that, but the vault
  // writes silently no-op). Their observable effect is confined to PROVISIONED
  // characters: only `aa…0001` (the `fold_regular` participant) has a vault, and the
  // check-op chats reference unprovisioned characters, so their internal folds mirror
  // nothing — exactly matching the Rust side, where the direct `generate` ops run
  // `RealContextSummarySeams` and the `check`-op internal folds keep `NoopSeams`.
  // The embedding is canned to a fixed unit vector on BOTH sides (dim 8, [1,0,…]),
  // matching the seeded chunk's embedding so the refresh's semantic search scores
  // cosine 1.0 and surfaces the pre-seeded prior summary.
  jest.doMock('@/lib/embedding/embedding-service', () => {
    const actual = jest.requireActual('@/lib/embedding/embedding-service');
    return {
      __esModule: true,
      ...actual,
      generateEmbeddingForUser: async () => ({
        embedding: new Float32Array([1, 0, 0, 0, 0, 0, 0, 0]),
        model: 'canned',
        dimensions: 8,
      }),
    };
  });
  // The estimated cost stays host-resolved; the Rust seam passes `None`, so the
  // fetcher returns null on both sides (the stored `estimatedCostUSD` is null).
  jest.doMock('@/lib/services/cost-estimation.service', () => {
    const actual = jest.requireActual('@/lib/services/cost-estimation.service');
    return { __esModule: true, ...actual, estimateMessageCost: async () => ({ cost: null }) };
  });
  jest.doMock('@/lib/services/system-events.service', () => {
    const actual = jest.requireActual('@/lib/services/system-events.service');
    return {
      __esModule: true,
      ...actual,
      createContextSummaryEvent: async (...args: unknown[]) =>
        currentOpUsesRealSeams ? actual.createContextSummaryEvent(...args) : undefined,
      createTitleGenerationEvent: async (...args: unknown[]) =>
        currentOpUsesRealSeams ? actual.createTitleGenerationEvent(...args) : undefined,
    };
  });
  // Keep the in-process job runner OFF: the enqueued PENDING row is state under
  // test, and the Rust port defers the runner the same way.
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const {
    generateContextSummary,
    invalidateContextSummaryIfMessageCovered,
    checkAndGenerateSummaryIfNeeded,
  } = await import('@/lib/chat/context-summary');

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
    maxContext: null,
  });
  const availableProfiles = ops.profiles.map(mkProfile);
  const currentProfile = availableProfiles.find((p) => p.id === ops.currentProfileId)!;
  const cheapLLMSettings = {
    strategy: 'PROVIDER_CHEAPEST',
    userDefinedProfileId: null,
    defaultCheapProfileId: null,
    fallbackToLocal: false,
  };

  const lines: string[] = [];

  for (const op of ops.ops) {
    currentOp = op.name;
    // Only the direct `generate` entry point wires the real seams (the Rust
    // differential drives `generate_context_summary_with_seams(RealContextSummarySeams)`
    // there); `check`/`invalidate` keep the no-op seams.
    currentOpUsesRealSeams = op.kind === 'generate';
    let result: unknown;
    if (op.kind === 'generate') {
      result = await generateContextSummary({
        userId: ops.userId,
        chatId: op.chatId,
        connectionProfile: currentProfile as never,
        cheapLLMSettings: cheapLLMSettings as never,
        availableProfiles: availableProfiles as never,
        forceRegenerate: op.forceRegenerate ?? false,
      });
    } else if (op.kind === 'invalidate') {
      result = await invalidateContextSummaryIfMessageCovered(op.chatId, op.messageIds ?? []);
    } else {
      // check
      await checkAndGenerateSummaryIfNeeded(
        op.chatId,
        currentProfile.provider as never,
        currentProfile.modelName,
        ops.userId,
        currentProfile as never,
        cheapLLMSettings as never,
        availableProfiles as never,
        { awaitFold: true }
      );
      // checkAndGenerate returns void; reconstruct the observable outcome from
      // the resulting chat + background_jobs state for the diff.
      const chat = await repos.chats.findById(op.chatId);
      const jobs = (await repos.backgroundJobs.findByUserId(ops.userId, 'PENDING')) as Array<{
        type: string;
        payload: { chatId?: string; currentInterchange?: number };
      }>;
      const titleJob = jobs.find(
        (j) => j.type === 'TITLE_UPDATE' && j.payload?.chatId === op.chatId
      );
      result = {
        enqueuedTitleUpdate: !!titleJob,
        currentInterchange: titleJob?.payload?.currentInterchange ?? null,
        chatContextSummary: chat?.contextSummary ?? null,
        chatTitle: chat?.title ?? null,
        chatLastSummaryTurn: chat?.lastSummaryTurn ?? null,
      };
    }
    lines.push(JSON.stringify({ kind: 'result', op: op.name, result }));
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

  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chats', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chat_messages', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('background_jobs', 'payload')) }));

  // Mirror proof (Round-3 Group 7): the SET of (mountPointId, relativePath) present
  // in the vault after the fold. The minted link ids / fileIds / folderIds /
  // timestamps / chunkCount are ignored (v4 reindexes → chunkCount; the Rust write
  // primitive does not — the standard groups/projects treatment); the deterministic
  // relativePath proves the mirror wrote `Conversation Summaries/<title>.md`.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable at dump');
  const linkRows = midb
    .prepare('SELECT mountPointId, relativePath FROM doc_mount_file_links')
    .all() as Array<{ mountPointId: string; relativePath: string }>;
  const linkPaths = linkRows
    .map((r) => `${r.mountPointId}|${r.relativePath}`)
    .sort();
  lines.push(JSON.stringify({ kind: 'mountLinks', paths: linkPaths }));
  closeMountIndexSQLiteClient();

  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`context-summary-service oracle wrote ${outPath}\n`);
}

test('context-summary-service tier-3 oracle', async () => {
  await main();
});
