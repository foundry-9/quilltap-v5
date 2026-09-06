/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the help-chat **orchestrator** (P4.9I2A; v4
 * `handleHelpChatMessage` + `processHelpResponse` + `triggerAsyncTasks`,
 * lib/services/help-chat/orchestrator.service.ts).
 *
 * Drives v4's REAL `handleHelpChatMessage` over the committed corpus
 * (harness/oracle/fixtures/help-chat-orchestrator-tier3.json) against the
 * committed `help-chat-{main,mount}.db` fixture, mocking ONLY the model
 * boundaries the Rust port injects as seams (each matching the Rust seam):
 *
 *   - `streamMessage`: per-case scripted sequences popped in call order + RECORD
 *     the exact `provider|model|temperature|messages` canned key answered — over
 *     the messages the PLUGINS would send: an id-less `tool` row is filtered out
 *     of the recorded key exactly as every v4 plugin filters it at format time
 *     (`if (m.role === "tool" && !m.toolCallId) return false`), which is where
 *     v5's `to_stream_messages` drops it too (the orchestrator module doc,
 *     divergence 1). `buildTools` stays REAL (ANTHROPIC + a fictional model →
 *     `checkModelSupportsTools` true).
 *   - `detectToolCallsInResponse`: canned by the raw response's `marker`.
 *     `processToolCalls` + `saveToolMessages` + every handler stay REAL —
 *     `help_search` runs the real keyword fallback over the fixture's 17 docs (no
 *     embedding profile → the embedding call fails → keyword search), and
 *     `help_navigate` is pure.
 *   - The async tail: `triggerContextSummaryCheck` no-op'd on both sides (a
 *     cheap-LLM model call behind a host seam); `triggerTurnMemoryExtraction`
 *     stays REAL — its MEMORY_EXTRACTION enqueue is a comparand.
 *     `trackMessageTokenUsage` no-op'd, `estimateMessageCost` canned.
 *
 * Each case sends into its OWN help chat (one DB init; a +200 ms settle drains
 * the fire-and-forget tail). Per case: the decoded SSE frame trace, the chat's
 * persisted messages (stable columns), and the chat's `background_jobs` rows
 * (type, status, payload keys with the minted opener id reduced to presence).
 *
 * Run (Node 24, from the v4 checkout — mirror to /tmp; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   mkdir -p /tmp/help-orch/cases /tmp/help-orch/fixtures
 *   cp $V5W/harness/oracle/cases/help-chat-orchestrator-tier3.test.ts /tmp/help-orch/cases/
 *   cp $V5W/harness/oracle/fixtures/help-chat-orchestrator-tier3.json /tmp/help-orch/fixtures/
 *   QT_FIXTURE_HELP_CHAT_MAIN=$V5W/crates/quilltap-web/tests/fixtures/help-chat-main.db \
 *   QT_FIXTURE_HELP_CHAT_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/help-chat-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-help-orch.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots /tmp/help-orch/cases -- help-chat-orchestrator-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ChunkSpec {
  content?: string;
  done?: boolean;
  usage?: { promptTokens: number; completionTokens: number; totalTokens: number } | null;
  rawResponse?: unknown;
}
interface CaseSpec { name: string; chatId: string; user?: 'A' | 'B'; content: string; streams: ChunkSpec[][] }
interface Spec {
  testPepperBase64: string;
  users: { A: string; B: string };
  detection: Record<string, Array<{ name: string; arguments: Record<string, unknown>; callId?: string }>>;
  cases: CaseSpec[];
}

const MSG_COLS = ['role', 'content', 'participantId', 'provider', 'modelName', 'promptTokens', 'completionTokens', 'tokenCount'] as const;

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'help-chat-orchestrator-tier3.json'), 'utf8')) as Spec;
  const fixtureMain = process.env.QT_FIXTURE_HELP_CHAT_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_HELP_CHAT_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_HELP_CHAT_MAIN / QT_FIXTURE_HELP_CHAT_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-help-orch-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'help-main.db');
  const workMount = join(scratch, 'help-mount.db');
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
  const cannedRows: Array<{ provider: string; model: string; temperature: number | null; messages: Array<{ role: string; content: string }>; sequences: ChunkSpec[][] }> = [];

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(process.cwd(), 'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers');
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/plugins/provider-validation', () => jest.requireActual('@/lib/plugins/provider-validation'));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  // The help_search handler's embedding call must FAIL (no profile) so the real
  // keyword fallback runs — the same path v5's tool takes on this fixture.
  jest.doMock('@/lib/embedding/embedding-service', () => jest.requireActual('@/lib/embedding/embedding-service'));
  // The fixture's 17 `help_docs` ARE the corpus: v4's lazy `ensureHelpDocsSynced`
  // (run by `HelpSearch.loadFromDatabase()` on the first resolve) would otherwise
  // walk cwd/help — the FULL 120-file shipped tree — and re-sync it over them,
  // and the resolver would then see wildcard docs the fixture never had (the
  // first run of this oracle: a third `### Additional Context: Width Toggle
  // Button` in every prompt). v5 reads the table per call with no ensure in the
  // differential venue; the same fixture-fidelity pin the routes oracle uses.
  jest.doMock('@/lib/help/help-doc-sync', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/help/help-doc-sync'),
    ensureHelpDocsSynced: async () => undefined,
  }));
  // `enqueueJob()` resolves the job-child entry relative to cwd — keep the
  // in-process runner out of the dumps (the help-sync-ensure recipe).
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });

  jest.doMock('@/lib/services/chat-message/streaming.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/streaming.service');
    return {
      __esModule: true,
      ...actual,
      streamMessage: async function* (opts: {
        messages: Array<{ role: string; content: string; toolCallId?: string }>;
        connectionProfile: { provider: string; modelName: string };
        modelParams?: { temperature?: number };
      }) {
        const seq = currentCase.streams[streamCallIndex];
        streamCallIndex += 1;
        if (!seq) throw new Error(`no scripted stream #${streamCallIndex - 1} for case ${currentCase.name}`);
        cannedRows.push({
          provider: opts.connectionProfile.provider,
          model: opts.connectionProfile.modelName,
          temperature: opts.modelParams?.temperature ?? null,
          // The plugins' filter (anthropic/openai-compatible/ollama
          // `formatMessages…`, the OpenAI Responses formatter's skip): an
          // id-less tool row never reaches the wire — on NINE of the ten
          // providers. GOOGLE's plugin keeps it (`provider.ts:376`), so the
          // recorded key keeps it there too (the §3 review of the `p4.9i2`
          // unification; the fixture carries no GOOGLE profile yet).
          messages: opts.messages
            .filter((m) => opts.connectionProfile.provider === 'GOOGLE' || !(m.role === 'tool' && !m.toolCallId))
            .map((m) => ({ role: m.role, content: m.content })),
          sequences: [seq],
        });
        for (const chunk of seq) {
          // A scripted mid-stream throw: v4's `for await` propagates it out of
          // `processHelpResponse` (the per-participant `error` frame, no row).
          if (chunk.error) throw new Error(chunk.error);
          if (chunk.done) yield { done: true, usage: chunk.usage ?? undefined, rawResponse: chunk.rawResponse };
          else yield { content: chunk.content };
        }
      },
    };
  });
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
  jest.doMock('@/lib/services/chat-message/memory-trigger.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/memory-trigger.service');
    return { __esModule: true, ...actual, triggerContextSummaryCheck: async () => undefined };
  });
  jest.doMock('@/lib/services/token-tracking.service', () => ({ __esModule: true, trackMessageTokenUsage: async () => undefined }));
  jest.doMock('@/lib/services/cost-estimation.service', () => {
    const actual = jest.requireActual('@/lib/services/cost-estimation.service');
    return { __esModule: true, ...actual, estimateMessageCost: async () => ({ cost: null, source: 'unavailable' }) };
  });

  {
    const nodeRequire = createRequire(join(process.cwd(), 'noop.js'));
    const PLUGIN_DIRS = ['anthropic', 'openai', 'google', 'grok', 'deepseek', 'z-ai', 'openrouter', 'ollama', 'openai-compatible', 'nanogpt'];
    const { initializeProviderRegistry } = await import('@/lib/plugins/provider-registry');
    const providers = PLUGIN_DIRS.map((d) => {
      const m = nodeRequire(join(process.cwd(), 'plugins', 'dist', `qtap-plugin-${d}`, 'index.js'));
      return m.plugin || m.default?.plugin || m.default;
    });
    await initializeProviderRegistry(providers);
  }
  await import('@/lib/plugins/provider-validation');

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { handleHelpChatMessage } = await import('@/lib/services/help-chat/orchestrator.service');
  await initializeDatabase();
  const repos = getRepositories();

  const decoder = new TextDecoder();
  function decodeFrames(bytes: Uint8Array, sink: unknown[]): void {
    for (const line of decoder.decode(bytes).split('\n')) {
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
    const userId = call.user === 'B' ? spec.users.B : spec.users.A;
    const events: unknown[] = [];
    const stream = await handleHelpChatMessage(repos, call.chatId, userId, { content: call.content });
    const reader = stream.getReader();
    // eslint-disable-next-line no-constant-condition
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      if (value) decodeFrames(value, events);
    }
    await new Promise((resolve) => setTimeout(resolve, 200));

    const rawMessages = (await repos.chats.getMessages(call.chatId)) as Array<Record<string, unknown>>;
    const rows = rawMessages
      .filter((m) => m.type === 'message')
      .map((m) => { const out: Record<string, unknown> = {}; for (const col of MSG_COLS) out[col] = m[col] ?? null; return out; });
    const jobRows = (await rawQuery<Array<Record<string, unknown>>>(
      "SELECT type, status, userId, payload FROM background_jobs WHERE json_extract(payload, '$.chatId') = ? ORDER BY rowid",
      [call.chatId],
    )) ?? [];
    const jobs = jobRows.map((r) => {
      const p = typeof r.payload === 'string' ? JSON.parse(r.payload) : r.payload;
      return {
        type: String(r.type), status: String(r.status), userId: String(r.userId),
        payloadKeys: Object.keys(p ?? {}).sort(),
        chatId: p?.chatId ?? null,
        hasTurnOpener: p?.turnOpenerMessageId != null,
        hasAnchor: p?.extractionAnchorMessageId != null,
        connectionProfileId: p?.connectionProfileId ?? null,
      };
    });
    lines.push(JSON.stringify({ kind: 'events', call: call.name, events }));
    lines.push(JSON.stringify({ kind: 'messages', call: call.name, rows }));
    lines.push(JSON.stringify({ kind: 'jobs', call: call.name, jobs }));
  }
  for (const row of cannedRows) lines.push(JSON.stringify({ kind: 'cannedStream', ...row }));

  closeMountIndexSQLiteClient();
  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`help-chat-orchestrator oracle wrote ${outPath} (${spec.cases.length} cases, ${cannedRows.length} canned streams)\n`);
}

test('help-chat-orchestrator tier-3 oracle', async () => {
  await main();
});
