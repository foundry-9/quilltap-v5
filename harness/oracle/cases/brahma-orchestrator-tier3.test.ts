/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the Brahma Console **orchestrator** (P4.9I1A; v4
 * `handleBrahmaConsoleMessage` + `processBrahmaResponse`,
 * lib/services/brahma-console/orchestrator.service.ts) — the multi-turn,
 * transcript-persisting console.
 *
 * Drives v4's REAL `handleBrahmaConsoleMessage` over the committed corpus
 * (harness/oracle/fixtures/brahma-orchestrator-tier3.json) against the committed
 * `brahma-{main,mount}.db` fixture, mocking ONLY the model boundaries the Rust
 * port injects as seams (each matching the Rust seam exactly):
 *
 *   - `streamMessage`: per-case scripted sequences popped in call order + RECORD
 *     the exact `provider|model|temperature|messages` canned key answered (so the
 *     Rust `QueuedStreamingProvider` replays them — a system-prompt / threading /
 *     tool-mode divergence surfaces as a canned-miss on BOTH sides). `buildTools`
 *     stays REAL (ANTHROPIC + a fictional model → `checkModelSupportsTools` true).
 *   - `detectToolCallsInResponse`: canned by the raw response's `marker`.
 *     `processToolCalls` + `saveToolMessages` + every handler stay REAL — `run_sql`
 *     runs an actual SELECT over the fixture; its byte-exact result threads into the
 *     continuation (proven by the continuation canned key + the persisted TOOL row).
 *   - The async tail (`memory-trigger`), `trackMessageTokenUsage`, and
 *     `estimateMessageCost` are no-op'd / canned: the context-summary drive is a
 *     documented Rust deferral (matching the finalizer), the chat token-aggregate
 *     write is not diffed (pricing-coupled), and memory extraction NEVER fires.
 *
 * Each case sends into its OWN brahma chat, so the persisted transcripts do not
 * contaminate across cases (all on one DB init; the +200ms settle drains stray
 * fire-and-forget promises). Per case, the oracle drains the `ReadableStream`
 * (decoding each `data: {…}` SSE frame — the ordered trace the Rust `RecordingSink`
 * is diffed against) and dumps the chat's persisted messages (projected to the
 * stable columns; minted ids / timestamps dropped).
 *
 * Run (Node 24, from the v4 checkout — the oracle lives under `.claude/`, which
 * jest ignores, so mirror it to /tmp):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}   # the v5 checkout (or your worktree)
 *   cd ~/source/quilltap-server
 *   mkdir -p /tmp/brahma-orch/cases /tmp/brahma-orch/fixtures
 *   cp $V5W/harness/oracle/cases/brahma-orchestrator-tier3.test.ts /tmp/brahma-orch/cases/
 *   cp $V5W/harness/oracle/fixtures/brahma-orchestrator-tier3.json /tmp/brahma-orch/fixtures/
 *   QT_FIXTURE_BRAHMA_MAIN=$V5W/crates/quilltap-web/tests/fixtures/brahma-main.db \
 *   QT_FIXTURE_BRAHMA_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/brahma-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-brahma-orch.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots /tmp/brahma-orch/cases -- brahma-orchestrator-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ChunkSpec {
  content?: string;
  reasoning?: string;
  done?: boolean;
  usage?: { promptTokens: number; completionTokens: number; totalTokens: number } | null;
  rawResponse?: unknown;
}
interface CaseSpec {
  name: string;
  chatId: string;
  content: string;
  streams: ChunkSpec[][];
  /** P4.D60 (Bug 47): per-case Brahma turn budget written to instance_settings
   * before the case runs (default 50 = the fixture's absent-setting value). A
   * small budget forces the forced-final turn quickly to exercise the salvage. */
  maxAgentTurns?: number;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  detection: Record<string, Array<{ name: string; arguments: Record<string, unknown>; callId?: string }>>;
  cases: CaseSpec[];
}

// The stable message columns the diff compares (minted id + createdAt dropped;
// the transcript ORDER — createdAt asc — is deterministic per send).
const MSG_COLS = [
  'role',
  'content',
  'provider',
  'modelName',
  'promptTokens',
  'completionTokens',
  'tokenCount',
  'reasoningContent',
] as const;

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'brahma-orchestrator-tier3.json'), 'utf8'),
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_BRAHMA_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_BRAHMA_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_BRAHMA_MAIN / QT_FIXTURE_BRAHMA_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-brahma-orch-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'brahma-main.db');
  const workMount = join(scratch, 'brahma-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  let currentCase: CaseSpec = spec.cases[0];
  let streamCallIndex = 0;
  const cannedRows: Array<{
    provider: string;
    model: string;
    temperature: number | null;
    messages: Array<{ role: string; content: string }>;
    sequences: ChunkSpec[][];
  }> = [];

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  // jest.setup globally mocks provider-validation to a partial (no requiresApiKey);
  // the console reads requiresApiKey, so restore the REAL module.
  jest.doMock('@/lib/plugins/provider-validation', () =>
    jest.requireActual('@/lib/plugins/provider-validation'),
  );
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

  // streamMessage: scripted per-case sequences popped in call order + RECORD the
  // canned key. buildTools stays REAL.
  jest.doMock('@/lib/services/chat-message/streaming.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/streaming.service');
    return {
      __esModule: true,
      ...actual,
      streamMessage: async function* (opts: {
        messages: Array<{ role: string; content: string }>;
        connectionProfile: { provider: string; modelName: string };
        modelParams?: { temperature?: number };
      }) {
        const seq = currentCase.streams[streamCallIndex];
        streamCallIndex += 1;
        if (!seq) {
          throw new Error(`no scripted stream #${streamCallIndex - 1} for case ${currentCase.name}`);
        }
        cannedRows.push({
          provider: opts.connectionProfile.provider,
          model: opts.connectionProfile.modelName,
          temperature: opts.modelParams?.temperature ?? null,
          messages: opts.messages.map((m) => ({ role: m.role, content: m.content })),
          sequences: [seq],
        });
        for (const chunk of seq) {
          if (chunk.done) {
            yield { done: true, usage: chunk.usage ?? undefined, rawResponse: chunk.rawResponse };
          } else if (chunk.reasoning !== undefined) {
            yield { content: '', reasoningContent: chunk.reasoning };
          } else {
            yield { content: chunk.content };
          }
        }
      },
    };
  });

  // detectToolCallsInResponse: canned by the raw response marker. processToolCalls
  // + saveToolMessages + every handler stay REAL.
  jest.doMock('@/lib/services/chat-message/tool-execution.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/tool-execution.service');
    return {
      __esModule: true,
      ...actual,
      detectToolCallsInResponse: (raw: unknown) => {
        const marker = (raw as { marker?: string } | null)?.marker;
        return (marker && spec.detection[marker]) || [];
      },
    };
  });

  // The async tail: context-summary check no-op (a documented Rust deferral) and
  // memory extraction NEVER fires (the console forms no memories).
  jest.doMock('@/lib/services/chat-message/memory-trigger.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/memory-trigger.service');
    return {
      __esModule: true,
      ...actual,
      triggerContextSummaryCheck: async () => undefined,
      triggerTurnMemoryExtraction: async () => undefined,
    };
  });
  // The chat token-aggregate write is not diffed (pricing-coupled). Cost estimate
  // canned to the Rust `NoCostTracking` shape.
  jest.doMock('@/lib/services/token-tracking.service', () => ({
    __esModule: true,
    trackMessageTokenUsage: async () => undefined,
  }));
  jest.doMock('@/lib/services/cost-estimation.service', () => {
    const actual = jest.requireActual('@/lib/services/cost-estimation.service');
    return {
      __esModule: true,
      ...actual,
      estimateMessageCost: async () => ({ cost: null, source: 'unavailable' }),
    };
  });

  // Initialize the REAL provider registry so buildTools reshapes the real way.
  {
    const nodeRequire = createRequire(join(process.cwd(), 'noop.js'));
    const PLUGIN_DIRS = [
      'anthropic',
      'openai',
      'google',
      'grok',
      'deepseek',
      'z-ai',
      'openrouter',
      'ollama',
      'openai-compatible',
    ];
    const { initializeProviderRegistry } = await import('@/lib/plugins/provider-registry');
    const providers = PLUGIN_DIRS.map((d) => {
      const m = nodeRequire(join(process.cwd(), 'plugins', 'dist', `qtap-plugin-${d}`, 'index.js'));
      return m.plugin || m.default?.plugin || m.default;
    });
    await initializeProviderRegistry(providers);
  }
  // Fully evaluate provider-validation BEFORE the `@/lib/tools` barrel caches a
  // mid-cycle partial with `requiresApiKey` undefined (the barrel circular-init
  // gotcha).
  await import('@/lib/plugins/provider-validation');

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { handleBrahmaConsoleMessage } = await import(
    '@/lib/services/brahma-console/orchestrator.service'
  );
  const { setBrahmaConsoleSettings } = await import('@/lib/instance-settings');

  await initializeDatabase();
  const repos = getRepositories();

  const decoder = new TextDecoder();
  function decodeFrames(bytes: Uint8Array, sink: unknown[]): void {
    const text = decoder.decode(bytes);
    for (const line of text.split('\n')) {
      const t = line.trim();
      if (t.startsWith('data:')) {
        const body = t.slice('data:'.length).trim();
        if (body) sink.push(JSON.parse(body));
      }
    }
  }

  const lines: string[] = [];

  for (const call of spec.cases) {
    currentCase = call;
    streamCallIndex = 0;

    // Per-case budget override (only the Bug-47 salvage cases set it; the
    // committed fixture has no instance_settings table, so create it first — the
    // absent setting resolves to the default 50 for every other case).
    if (call.maxAgentTurns !== undefined) {
      rawQuery(
        'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
      );
      await setBrahmaConsoleSettings({ maxAgentTurns: call.maxAgentTurns });
    }

    const events: unknown[] = [];
    const stream = await handleBrahmaConsoleMessage(repos, call.chatId, spec.userId, {
      content: call.content,
    });
    const reader = stream.getReader();
    // eslint-disable-next-line no-constant-condition
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      if (value) decodeFrames(value, events);
    }

    // Let the fire-and-forget async tail settle before the next case.
    await new Promise((resolve) => setTimeout(resolve, 200));

    const rawMessages = (await repos.chats.getMessages(call.chatId)) as Array<Record<string, unknown>>;
    const rows = rawMessages
      .filter((m) => m.type === 'message')
      .map((m) => {
        const out: Record<string, unknown> = {};
        for (const col of MSG_COLS) out[col] = m[col] ?? null;
        return out;
      });

    lines.push(JSON.stringify({ kind: 'events', call: call.name, events }));
    lines.push(JSON.stringify({ kind: 'messages', call: call.name, rows }));
  }

  for (const row of cannedRows) lines.push(JSON.stringify({ kind: 'cannedStream', ...row }));

  closeMountIndexSQLiteClient();
  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`brahma-orchestrator oracle wrote ${outPath}\n`);
}

test('brahma-orchestrator tier-3 oracle', async () => {
  await main();
});
