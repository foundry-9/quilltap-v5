/**
 * @jest-environment node
 *
 * Tier-3 (mocked model boundaries) ORACLE for the Scenario Builder service
 * (P4.D217 — v4 `d1c06cd9d`, `runScenarioBuilder` in
 * lib/services/scenario-builder/scenario-builder.service.ts), ported to
 * quilltap_core::services::scenario_builder::run_scenario_builder.
 *
 * Cloned from `brahma-console-tier3.test.ts`. Drives v4's REAL
 * `runScenarioBuilder` over the committed corpus
 * (harness/oracle/fixtures/scenario-builder-tier3.json) against the REAL
 * doc-opacity two-partition fixture (built by its own unchanged builder),
 * mocking ONLY the model boundaries the Rust port injects as seams:
 *
 *   - `streamMessage`: per-case scripted sequences popped in call order, and
 *     per call RECORDED: the canned key (`provider|model|temperature|
 *     messages`), the `logType` (every call must carry `SCENARIO_BUILDER`),
 *     and the NAMES of the tools passed (the slate is not in the key).
 *   - `detectToolCallsInResponse`: canned by the raw response's `marker`.
 *
 * Everything else is REAL: `buildTools` (the registry initialized),
 * `buildOneShotToolInstructions`, `runOneShotToolLoop`, `processToolCalls`,
 * `executeToolCallWithContext`, every handler (`search`, `doc_grep`,
 * `doc_read_file` over the pre-built pool), and `resolveScenarioBuilderMountPool`.
 * `isWebSearchConfigured` is REAL too — driven per case through
 * `SERPER_API_KEY`. The `controller` is a collector: each `data: …\n\n`
 * payload is decoded back into its object, in order.
 *
 * Determinism: `Date` is faked to `nowEpochMs` (timers stay real) with
 * `TZ=nowTz`, so the system prompt's `## Now` line is fixed; `crypto.
 * randomUUID` answers `syntheticChatId` ONCE per chat-less case (the service's
 * first act is minting its synthetic chat id), then falls through to the real
 * one. v5 is handed the same instant and id.
 *
 * Trip hooks (the Brahma recipe's, re-pointed): a `reasoning` FRAME equal to
 * the case's `abortOnReasoning` aborts from inside the controller (the loop's
 * next chunk is the mid-stream break); a raw-response marker equal to
 * `abortOnMarker` aborts from inside detection (the loop top is the
 * between-turns break).
 *
 * Emits per case: { kind: 'case', name, frames, streams: [{ logType,
 * toolNames }], loggedTypes, lines } — `loggedTypes` being the type of each
 * row v4's real `streamMessage` would have written (one per call that reaches
 * its terminal chunk) and at the end one { kind: 'cannedStream', … } per
 * served call. `lines` are the `ScenarioBuilder`, `OneShotToolLoop` and
 * `ScenarioBuilderMountPool` services' log lines.
 *
 * Run (Node 24, from the v4 checkout or a PINNED worktree; stage outside
 * `.claude/`, which v4's jest ignores):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>
 *   STAGE=/tmp/qt-oracle-stage-sb-tier3
 *   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
 *   cp $W/harness/oracle/cases/scenario-builder-tier3.test.ts $STAGE/harness/oracle/cases/
 *   cp $W/harness/oracle/fixtures/scenario-builder-tier3.json $STAGE/harness/oracle/fixtures/
 *   cd ~/source/quilltap-server
 *   rm -f /tmp/qt-sbt3-main.db /tmp/qt-sbt3-mount.db
 *   QT_FIXTURE_DOPA_MAIN=/tmp/qt-sbt3-main.db QT_FIXTURE_DOPA_MOUNT=/tmp/qt-sbt3-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-doc-opacity-fixture.ts
 *   QT_FIXTURE_SBT3_MAIN=/tmp/qt-sbt3-main.db QT_FIXTURE_SBT3_MOUNT=/tmp/qt-sbt3-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-scenario-builder-tier3.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=240000 \
 *       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "scenario-builder-tier3\.test\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { createRequire } from 'node:module';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ChunkSpec {
  content?: string;
  done?: boolean;
  rawResponse?: unknown;
  reasoningContent?: string;
  usage?: { promptTokens: number; completionTokens: number; totalTokens: number };
  error?: string;
  /** P4.114: a Gemini-style thought signature on the chunk (`""` included). */
  thoughtSignature?: string;
}
interface CaseSpec {
  name: string;
  userId: string;
  mode: 'real' | 'in-world';
  location: string;
  time: string;
  details: string;
  projectId: string | null;
  characterIds: string[];
  chat?: { id: string; scenarioText?: string | null; contextSummary?: string | null };
  priorDraft?: string;
  revision?: string;
  webSearchConfigured: boolean;
  profile: Record<string, unknown>;
  abortOnReasoning?: string;
  abortOnMarker?: string;
  streams: ChunkSpec[][];
}
interface Spec {
  testPepperBase64: string;
  nowEpochMs: number;
  nowTz: string;
  syntheticChatId: string;
  profileBase: Record<string, unknown>;
  detection: Record<string, Array<{ name: string; arguments: Record<string, unknown>; callId?: string }>>;
  cases: CaseSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'scenario-builder-tier3.json'), 'utf8'),
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_SBT3_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_SBT3_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_SBT3_MAIN / QT_FIXTURE_SBT3_MOUNT must point at the doc-opacity builder output');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-sbt3-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'main.db');
  const workMount = join(scratch, 'mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  process.env.TZ = spec.nowTz;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  let currentCase: CaseSpec = spec.cases[0];
  let streamCallIndex = 0;
  let currentAbort: AbortController | null = null;
  let streamRecords: Array<{ logType: string; toolNames: string[] }> = [];
  // v4's real `streamMessage` writes its `llm_logs` row as the TERMINAL chunk
  // passes through, before yielding it — typed `opts.logType ?? 'CHAT_MESSAGE'`.
  let loggedTypes: string[] = [];
  const logLines: Array<{ service: string; level: string; message: string; context: unknown }> = [];
  const cannedRows: Array<{
    provider: string;
    model: string;
    temperature: number | null;
    messages: Array<{ role: string; content: string }>;
    sequences: ChunkSpec[][];
    thoughtSignatures: Array<string | null>;
  }> = [];

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/plugins/provider-validation', () =>
    jest.requireActual('@/lib/plugins/provider-validation'),
  );
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () => jest.requireActual('@/lib/embedding/vector-store'));
  jest.doMock('@/lib/embedding/embedding-service', () =>
    jest.requireActual('@/lib/embedding/embedding-service'),
  );

  jest.doMock('@/lib/logging/create-logger', () => {
    const actual = jest.requireActual('@/lib/logging/create-logger');
    const RECORDED = new Set(['ScenarioBuilder', 'OneShotToolLoop', 'ScenarioBuilderMountPool']);
    return {
      __esModule: true,
      ...actual,
      createServiceLogger: (serviceName: string) => {
        if (!RECORDED.has(serviceName)) return actual.createServiceLogger(serviceName);
        const rec = (level: string) => (message: string, context?: unknown) => {
          logLines.push({ service: serviceName, level, message, context: context ?? null });
        };
        return { debug: rec('debug'), info: rec('info'), warn: rec('warn'), error: rec('error') };
      },
    };
  });

  jest.doMock('@/lib/services/chat-message/streaming.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/streaming.service');
    return {
      __esModule: true,
      ...actual,
      streamMessage: async function* (opts: {
        messages: Array<{ role: string; content: string; thoughtSignature?: string }>;
        connectionProfile: { provider: string; modelName: string };
        modelParams?: { temperature?: number };
        tools?: Array<{ name?: string; function?: { name?: string } }>;
        logType?: string;
      }) {
        const seq = currentCase.streams[streamCallIndex];
        streamCallIndex += 1;
        if (!seq) throw new Error(`no scripted stream #${streamCallIndex - 1} for case ${currentCase.name}`);
        streamRecords.push({
          logType: opts.logType ?? 'CHAT_MESSAGE',
          toolNames: (opts.tools ?? []).map((t) => t.name ?? t.function?.name ?? '?'),
        });
        cannedRows.push({
          provider: opts.connectionProfile.provider,
          model: opts.connectionProfile.modelName,
          temperature: opts.modelParams?.temperature ?? null,
          messages: opts.messages.map((m) => ({ role: m.role, content: m.content })),
          sequences: [seq],
          // P4.114: every message's signature as v4 threaded it (null = absent).
          thoughtSignatures: opts.messages.map((m) => m.thoughtSignature ?? null),
        });
        for (const chunk of seq) {
          if (chunk.error) throw new Error(chunk.error);
          if (chunk.done) {
            loggedTypes.push(opts.logType ?? 'CHAT_MESSAGE');
            yield {
              done: true,
              rawResponse: chunk.rawResponse,
              usage: chunk.usage,
              ...(chunk.thoughtSignature !== undefined ? { thoughtSignature: chunk.thoughtSignature } : {}),
            };
          } else if (chunk.reasoningContent !== undefined) {
            yield { reasoningContent: chunk.reasoningContent };
          } else {
            yield {
              content: chunk.content,
              ...(chunk.thoughtSignature !== undefined ? { thoughtSignature: chunk.thoughtSignature } : {}),
            };
          }
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
        if (marker && marker === currentCase.abortOnMarker) currentAbort?.abort();
        return (marker && spec.detection[marker]) || [];
      },
    };
  });

  {
    const nodeRequire = createRequire(join(process.cwd(), 'noop.js'));
    const PLUGIN_DIRS = ['anthropic', 'openai', 'google', 'grok', 'deepseek', 'z-ai', 'openrouter', 'ollama', 'openai-compatible'];
    const { initializeProviderRegistry } = await import('@/lib/plugins/provider-registry');
    const providers = PLUGIN_DIRS.map((d) => {
      const m = nodeRequire(join(process.cwd(), 'plugins', 'dist', `qtap-plugin-${d}`, 'index.js'));
      return m.plugin || m.default?.plugin || m.default;
    });
    await initializeProviderRegistry(providers);
  }
  await import('@/lib/plugins/provider-validation');

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { runScenarioBuilder } = await import('@/lib/services/scenario-builder/scenario-builder.service');
  // The CJS module object — the transpiled service reads `crypto_1.randomUUID`
  // off it at call time, so a spy here is what the service sees.
  const cryptoMod = require('crypto') as typeof import('crypto');

  await initializeDatabase();
  const repos = getRepositories();

  // Only Date is faked — timers stay real (the loop awaits real promises).
  jest.useFakeTimers({
    now: spec.nowEpochMs,
    doNotFake: [
      'hrtime', 'nextTick', 'performance', 'queueMicrotask', 'requestAnimationFrame',
      'cancelAnimationFrame', 'requestIdleCallback', 'cancelIdleCallback', 'setImmediate',
      'clearImmediate', 'setInterval', 'clearInterval', 'setTimeout', 'clearTimeout',
    ],
  });

  const lines: string[] = [];
  const decoder = new TextDecoder();
  try {
    for (const c of spec.cases) {
      currentCase = c;
      streamCallIndex = 0;
      streamRecords = [];
      loggedTypes = [];
      currentAbort = new AbortController();
      const abort = currentAbort;
      if (c.webSearchConfigured) process.env.SERPER_API_KEY = 'oracle-configured';
      else delete process.env.SERPER_API_KEY;

      const frames: unknown[] = [];
      const controller = {
        enqueue: (data: Uint8Array) => {
          const text = decoder.decode(data);
          for (const block of text.split('\n\n')) {
            if (!block.startsWith('data: ')) continue;
            const frame = JSON.parse(block.slice('data: '.length));
            frames.push(frame);
            if (
              c.abortOnReasoning !== undefined &&
              (frame as { reasoning?: string }).reasoning === c.abortOnReasoning
            ) {
              abort.abort();
            }
          }
        },
      };

      const spy = c.chat
        ? null
        : jest.spyOn(cryptoMod, 'randomUUID').mockReturnValueOnce(spec.syntheticChatId as never);
      await runScenarioBuilder(
        {
          repos,
          userId: c.userId,
          connectionProfile: { ...spec.profileBase, ...c.profile } as never,
          apiKey: 'unused-by-the-canned-stream',
          input: {
            mode: c.mode,
            location: c.location,
            time: c.time,
            details: c.details,
            projectId: c.projectId,
            characterIds: c.characterIds,
            chat: c.chat ?? null,
            priorDraft: c.priorDraft ?? null,
            revision: c.revision ?? null,
          },
        },
        controller as never,
        abort.signal,
      );
      spy?.mockRestore();

      lines.push(
        JSON.stringify({
          kind: 'case',
          name: c.name,
          frames,
          streams: streamRecords,
          loggedTypes,
          lines: logLines.splice(0),
        }),
      );
    }
  } finally {
    jest.useRealTimers();
    closeMountIndexSQLiteClient();
    await closeDatabase();
    rmSync(scratch, { recursive: true, force: true });
  }
  for (const row of cannedRows) lines.push(JSON.stringify({ kind: 'cannedStream', ...row }));
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`scenario-builder tier-3 oracle wrote ${outPath} (${spec.cases.length} cases)\n`);
}

test('scenario-builder tier-3 oracle', async () => {
  await main();
});
