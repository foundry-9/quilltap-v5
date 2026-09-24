/**
 * @jest-environment node
 *
 * Tier-3 (mocked model boundaries) ORACLE for the Brahma one-shot console
 * (v4 `runBrahmaQuery`, lib/services/brahma-console/one-shot.service.ts).
 *
 * Drives v4's REAL `runBrahmaQuery` over the committed corpus
 * (harness/oracle/fixtures/brahma-console-tier3.json) against the REAL main-DB
 * fixture, mocking ONLY the model boundaries the Rust port injects as seams:
 *
 *   - `streamMessage`: per-case scripted sequences popped in call order + RECORD
 *     the exact `provider|model|temperature|messages` canned key answered (so the
 *     Rust `QueuedStreamingProvider` replays them — a system-prompt / threading /
 *     selection divergence surfaces as a canned-miss on BOTH sides). `buildTools`
 *     stays REAL: its slate is invisible to the diff, but `tools.length > 0` and
 *     `modelSupportsNativeTools` steer the instructions + detection path, so both
 *     sides must agree (the provider registry is initialized so buildTools is fully
 *     real; the ANTHROPIC + fictional model makes `checkModelSupportsTools` return
 *     `true` deterministically, matching the Rust injected value).
 *   - `detectToolCallsInResponse`: canned by the raw response's `marker` field
 *     (`processToolCalls` + `executeToolCallWithContext` + every handler stay REAL
 *     — `run_sql` runs a real SELECT; its byte-exact result threads into the
 *     continuation, proven by the continuation canned key).
 *
 * The console NEVER persists (no chat rows written), so there are NO table dumps —
 * only the `runBrahmaQuery` result + the recorded canned streams. `runBrahmaQuery`
 * is called with the per-case userId (four users exercise the profile / api-key
 * branches).
 *
 * P4.D216 (v4 `d1c06cd9d` — the loop moved into the shared
 * `lib/services/agent-loop/one-shot-loop.ts`):
 *   - **Log lines.** `createServiceLogger` is wrapped so the two services this
 *     family reaches — `OneShotToolLoop` (the eight loop lines under the
 *     caller's label) and `BrahmaOneShot` (the no-profile debug) — are RECORDED
 *     per case (`{ level, message, context }`); every other service logs as
 *     before. The Rust side captures its tracing events structurally and diffs
 *     them line for line, so a missing, extra, reordered or re-fielded line is
 *     a red.
 *   - **Loop-direct arms** (`spec.loopCases`) drive v4's REAL
 *     `runOneShotToolLoop` itself — `runBrahmaQuery` passes no `signal`, no
 *     `onReasoning` and no `logType`, so the abort seam, the reasoning
 *     callback, the usage sum, the default label and the log type are only
 *     reachable from here. Each arm records the result (`{ ok, answer,
 *     toolsExecuted, usage }` / `{ ok: false, detail }` / `{ threw }`), the
 *     `onReasoning` calls, the log lines, and — per stream call that reaches its
 *     terminal chunk (where v4's real `streamMessage` writes its `llm_logs` row,
 *     BEFORE yielding it) — the `logType` that row would carry (`opts.logType ??
 *     'CHAT_MESSAGE'`, the destructure default). Two trip hooks: a chunk whose
 *     `reasoningContent` equals the arm's `abortOnReasoning` aborts from inside
 *     `onReasoning` (the NEXT chunk is the mid-stream break); a raw-response
 *     marker equal to `abortOnMarker` aborts from inside detection (after the
 *     mid-stream check — the loop-top check is the between-turns break).
 *
 * Run from the v4 server checkout under Node 24 (jest ignores `.claude/` worktree
 * paths, so mirror the oracle + spec to /tmp — see header of the Rust test):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}   # the v5 checkout (or your worktree)
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-brahma-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-brahma-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-brahma-console-fixture.ts
 *   mkdir -p /tmp/brahma-oracle/cases /tmp/brahma-oracle/fixtures
 *   cp $V5W/harness/oracle/cases/brahma-console-tier3.test.ts /tmp/brahma-oracle/cases/
 *   cp $V5W/harness/oracle/fixtures/brahma-console-tier3.json /tmp/brahma-oracle/fixtures/
 *   QT_FIXTURE_BRAHMA_MAIN=/tmp/qt-brahma-main.db QT_FIXTURE_BRAHMA_MOUNT=/tmp/qt-brahma-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-brahma.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 --roots "$PWD" --roots "/tmp/brahma-oracle/cases" -- brahma-console-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { createRequire } from 'node:module';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ChunkSpec {
  content?: string;
  done?: boolean;
  rawResponse?: unknown;
  /** P4.D216: a reasoning delta (cumulative, as providers send it). */
  reasoningContent?: string;
  /** P4.D216: usage on the terminal chunk. */
  usage?: { promptTokens: number; completionTokens: number; totalTokens: number };
  /** A scripted mid-stream provider throw (P4.79's `stream_error_mid_turn`). */
  error?: string;
  /** P4.114: a Gemini-style thought signature on the chunk (`""` included). */
  thoughtSignature?: string;
}
interface CaseSpec {
  name: string;
  userId: string;
  question: string;
  streams: ChunkSpec[][];
  /** P4.D60 (Bug 47): per-case Brahma turn budget written to instance_settings
   * before the case runs (absent = the default 50). A small budget forces the
   * forced-final turn to exercise the salvage. */
  maxAgentTurns?: number;
  /** P4.114: run v4's REAL `streamMessage` for this case (the provider is
   * scripted one level down, at `createLLMProvider`) — so its per-chunk
   * `normalizeContentBlockFormat` runs. */
  realStreamMessage?: boolean;
}
/** P4.D216: an arm that drives `runOneShotToolLoop` directly. */
interface LoopCaseSpec {
  name: string;
  userId: string;
  chatId: string;
  userMessage: string;
  maxAgentTurns: number;
  logLabel?: string;
  logType?: string;
  abortOnReasoning?: string;
  abortOnMarker?: string;
  streams: ChunkSpec[][];
}
interface Spec {
  testPepperBase64: string;
  chatId: string;
  loopProfile: { id: string; name: string; provider: string; modelName: string };
  loopSystemPrompt: string;
  loopCases: LoopCaseSpec[];
  detection: Record<
    string,
    Array<{ name: string; arguments: Record<string, unknown>; callId?: string }>
  >;
  cases: CaseSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'brahma-console-tier3.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_BRAHMA_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_BRAHMA_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_BRAHMA_MAIN / QT_FIXTURE_BRAHMA_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-brahma-oracle-'));
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

  let currentCase: { name: string; streams: ChunkSpec[][]; realStreamMessage?: boolean } =
    spec.cases[0];
  let streamCallIndex = 0;
  // P4.D216: the loop-direct arms' trip hooks and per-call log-type record.
  let currentAbort: AbortController | null = null;
  let currentAbortOnMarker: string | undefined;
  let loggedTypes: string[] = [];
  const logLines: Array<{ level: string; message: string; context: unknown }> = [];
  const cannedRows: Array<{
    provider: string;
    model: string;
    temperature: number | null;
    messages: Array<{ role: string; content: string }>;
    sequences: ChunkSpec[][];
    /** P4.114: every message's `thoughtSignature` as v4 passed it (null = absent). */
    thoughtSignatures: Array<string | null>;
  }> = [];

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  // jest.setup globally mocks provider-validation to a partial (no requiresApiKey);
  // the console reads requiresApiKey, so restore the REAL module.
  jest.doMock('@/lib/plugins/provider-validation', () =>
    jest.requireActual('@/lib/plugins/provider-validation')
  );
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

  // P4.D216: record the two services' log lines; every other logger is real.
  jest.doMock('@/lib/logging/create-logger', () => {
    const actual = jest.requireActual('@/lib/logging/create-logger');
    const RECORDED = new Set(['OneShotToolLoop', 'BrahmaOneShot']);
    return {
      __esModule: true,
      ...actual,
      createServiceLogger: (serviceName: string) => {
        if (!RECORDED.has(serviceName)) return actual.createServiceLogger(serviceName);
        const rec = (level: string) => (message: string, context?: unknown) => {
          logLines.push({ level, message, context: context ?? null });
        };
        return { debug: rec('debug'), info: rec('info'), warn: rec('warn'), error: rec('error') };
      },
    };
  });

  // P4.114: `@/lib/llm` (globally mocked by jest.setup — five bare jest.fns)
  // re-mocked with the SAME five members, `createLLMProvider` answering a
  // scripted provider ONLY while a `realStreamMessage` case streams (undefined
  // otherwise, exactly as before). v4's REAL `streamMessage` then runs over it:
  // the stall watchdog, the per-chunk `normalizeContentBlockFormat`, the log
  // call (a no-op under jest.setup's logging mock).
  let realProviderSeq: ChunkSpec[] | null = null;
  jest.doMock('@/lib/llm', () => ({
    createLLMProvider: jest.fn(async () =>
      realProviderSeq
        ? {
            streamMessage: async function* () {
              for (const chunk of realProviderSeq ?? []) {
                if (chunk.done) yield { content: '', done: true, rawResponse: chunk.rawResponse, usage: chunk.usage };
                else yield { content: chunk.content ?? '', done: false };
              }
            },
          }
        : undefined
    ),
    createImageProvider: jest.fn(),
    getAllAvailableProviders: jest.fn(() => []),
    getAllAvailableImageProviders: jest.fn(() => []),
    isProviderFromPlugin: jest.fn(() => true),
  }));

  // streamMessage: scripted per-case sequences popped in call order + RECORD the
  // canned key. buildTools stays REAL (its slate is invisible; the instructions +
  // modelSupportsNativeTools it feeds are what matter).
  jest.doMock('@/lib/services/chat-message/streaming.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/streaming.service');
    return {
      __esModule: true,
      ...actual,
      streamMessage: async function* (opts: {
        messages: Array<{ role: string; content: string; thoughtSignature?: string }>;
        connectionProfile: { provider: string; modelName: string };
        modelParams?: { temperature?: number };
        logType?: string;
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
          thoughtSignatures: opts.messages.map((m) => m.thoughtSignature ?? null),
        });
        if (currentCase.realStreamMessage) {
          // P4.114: v4's REAL generator over the scripted provider.
          realProviderSeq = seq;
          try {
            for await (const chunk of actual.streamMessage(opts as never)) {
              if ((chunk as { done?: boolean }).done) loggedTypes.push(opts.logType ?? 'CHAT_MESSAGE');
              yield chunk;
            }
          } finally {
            realProviderSeq = null;
          }
          return;
        }
        for (const chunk of seq) {
          // A scripted mid-stream throw: v4's `for await` propagates it out of
          // `runBrahmaQuery` itself (no internal try/catch there) — the same
          // shape the `p4.9i2` §3 review pinned in the help loop.
          if (chunk.error) throw new Error(chunk.error);
          if (chunk.done) {
            // v4's real `streamMessage` writes its `llm_logs` row HERE — as the
            // terminal chunk passes through, before it is yielded — typed by
            // the `logType = 'CHAT_MESSAGE'` destructure default (P4.D216).
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

  // detectToolCallsInResponse: canned by the raw response marker. processToolCalls
  // + executeToolCallWithContext + every handler stay REAL.
  jest.doMock('@/lib/services/chat-message/tool-execution.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/tool-execution.service');
    return {
      __esModule: true,
      ...actual,
      detectToolCallsInResponse: (raw: unknown) => {
        const marker = (raw as { marker?: string } | null)?.marker;
        // P4.D216: the between-turns trip — detection runs AFTER the loop's
        // mid-stream abort check, so the next check is the loop top.
        if (marker && marker === currentAbortOnMarker) currentAbort?.abort();
        return (marker && spec.detection[marker]) || [];
      },
    };
  });

  // Initialize the REAL provider registry so buildTools reshapes/answers the real
  // way (ANTHROPIC + a fictional model → checkModelSupportsTools true, no fetch).
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

  // Fully evaluate provider-validation BEFORE the `@/lib/tools` barrel (pulled in
  // by requireActual(streaming.service)) can cache a mid-cycle partial with
  // `requiresApiKey` undefined — the barrel circular-init gotcha.
  await import('@/lib/plugins/provider-validation');

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { runBrahmaQuery } = await import('@/lib/services/brahma-console/one-shot.service');
  const oneShotLoop = await import('@/lib/services/agent-loop/one-shot-loop').catch(() => null);
  const { setBrahmaConsoleSettings } = await import('@/lib/instance-settings');

  await initializeDatabase();
  const repos = getRepositories();

  const lines: string[] = [];

  for (const call of spec.cases) {
    currentCase = call;
    streamCallIndex = 0;

    // Per-case budget override (only the Bug-47 salvage cases set it); the
    // committed fixture has no instance_settings table, so create it first — the
    // absent setting resolves to the default 50 for every other case.
    if (call.maxAgentTurns !== undefined) {
      rawQuery(
        'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
      );
      await setBrahmaConsoleSettings({ maxAgentTurns: call.maxAgentTurns });
    }

    // `runBrahmaQuery` has NO try/catch of its own — a scripted mid-stream throw
    // propagates straight out of the awaited call (v4's real caller,
    // `answerAsBrahma`, wraps the whole thing and converts any thrown error into
    // `{ok: false, error: {kind: 'llm-failed', detail}}`; the ENGINE's own return
    // shape on this path is therefore undefined in v4, so the oracle records it
    // in the same `{ok, detail}` shape `runBrahmaQuery`'s OWN failure returns
    // already use — matching v5's `run_brahma_query`, which never throws).
    let result: { ok: boolean; answer?: string; detail?: string };
    try {
      result = await runBrahmaQuery({
        repos,
        userId: call.userId,
        chatId: spec.chatId,
        question: call.question,
      });
    } catch (err) {
      result = { ok: false, detail: err instanceof Error ? err.message : String(err) };
    }

    lines.push(JSON.stringify({ kind: 'result', call: call.name, result }));
    lines.push(JSON.stringify({ kind: 'logs', call: call.name, lines: logLines.splice(0) }));
  }

  // P4.D216: the loop-direct arms (v4 `d1c06cd9d`'s `runOneShotToolLoop`; absent
  // at an older pin, where nothing is emitted for them).
  const loopCannedRows: typeof cannedRows = [];
  if (oneShotLoop) {
    const mainCanned = cannedRows.length;
    for (const lc of spec.loopCases ?? []) {
      currentCase = lc;
      streamCallIndex = 0;
      loggedTypes = [];
      currentAbort = new AbortController();
      currentAbortOnMarker = lc.abortOnMarker;
      const reasoningCalls: string[] = [];
      const controller = currentAbort;
      let result: unknown;
      try {
        result = await oneShotLoop.runOneShotToolLoop({
          repos,
          userId: lc.userId,
          chatId: lc.chatId,
          connectionProfile: spec.loopProfile as never,
          apiKey: 'unused-by-the-canned-stream',
          systemPrompt: spec.loopSystemPrompt,
          userMessage: lc.userMessage,
          tools: { tools: [], modelSupportsNativeTools: true },
          toolContext: {
            chatId: lc.chatId,
            userId: lc.userId,
            operatorSurface: true,
            pendingWardrobeAnnouncements: new Set<string>(),
          } as never,
          maxAgentTurns: lc.maxAgentTurns,
          signal: controller.signal,
          ...(lc.logType ? { logType: lc.logType as never } : {}),
          ...(lc.logLabel ? { logLabel: lc.logLabel } : {}),
          statusContext: { characterName: 'The Host', characterId: '' },
          onReasoning: (r: string) => {
            reasoningCalls.push(r);
            if (lc.abortOnReasoning !== undefined && r === lc.abortOnReasoning) controller.abort();
          },
        });
      } catch (err) {
        result = { threw: err instanceof Error ? err.message : String(err) };
      }
      lines.push(
        JSON.stringify({
          kind: 'loopResult',
          call: lc.name,
          result,
          reasoningCalls,
          loggedTypes,
          lines: logLines.splice(0),
        })
      );
    }
    loopCannedRows.push(...cannedRows.splice(mainCanned));
  }

  for (const row of cannedRows) lines.push(JSON.stringify({ kind: 'cannedStream', ...row }));
  for (const row of loopCannedRows) lines.push(JSON.stringify({ kind: 'loopCannedStream', ...row }));

  closeMountIndexSQLiteClient();
  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`brahma-console oracle wrote ${outPath}\n`);
}

test('brahma-console tier-3 oracle', async () => {
  await main();
});
