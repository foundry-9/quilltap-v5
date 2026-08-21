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
}
interface Spec {
  testPepperBase64: string;
  chatId: string;
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

  // streamMessage: scripted per-case sequences popped in call order + RECORD the
  // canned key. buildTools stays REAL (its slate is invisible; the instructions +
  // modelSupportsNativeTools it feeds are what matter).
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
            yield { done: true, rawResponse: chunk.rawResponse };
          } else {
            yield { content: chunk.content };
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

    const result = await runBrahmaQuery({
      repos,
      userId: call.userId,
      chatId: spec.chatId,
      question: call.question,
    });

    lines.push(JSON.stringify({ kind: 'result', call: call.name, result }));
  }

  for (const row of cannedRows) lines.push(JSON.stringify({ kind: 'cannedStream', ...row }));

  closeMountIndexSQLiteClient();
  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`brahma-console oracle wrote ${outPath}\n`);
}

test('brahma-console tier-3 oracle', async () => {
  await main();
});
