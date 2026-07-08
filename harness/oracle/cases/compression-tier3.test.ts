/**
 * @jest-environment node
 *
 * Tier-3 (mocked-completion) ORACLE for the context-compression service half
 * (v4 `applyContextCompression`, lib/chat/context/compression.ts:191, driving
 * `compressConversationHistory`, lib/memory/cheap-llm-tasks/compression-tasks.ts).
 *
 * `applyContextCompression` makes NO DB writes of its own, so the RESULT diff is
 * result-object equivalence. But `executeCheapLLMTask`'s `sendToProvider` fires
 * `logLLMCall` after every successful provider call (W4.7e3), so this oracle now
 * runs against a REAL DB with `logLLMCall` UN-mocked and dumps the `llm_logs`
 * sibling table (`CONTEXT_COMPRESSION` rows). The real DB stack is wired back in
 * past `jest.setup` per [[jest-real-db-oracle]]; the ONLY seams pinned are the
 * model call, the API-key acquisition, and (indirectly) the provider factory.
 *
 * Stubbed seams:
 *   - `createLLMProvider` — a provider whose `sendMessage` resolves each call by
 *     matching the request messages against the CURRENT corpus case's completion
 *     rules (keyed on `provider|model`), RECORDS the exact canned key it answered
 *     (as `kind:"canned"` rows), and returns the rule's fixed content (empty or
 *     non-empty) or throws the rule's error. Re-provided via `jest.doMock`; the
 *     per-case rules are read from a mutable `currentCall`.
 *   - `getApiKeyForCheapLLMSelection` → a constant (key management is host-side).
 *
 * `logLLMCall` runs REAL against the real llm-logs DB.
 *
 * Emits, per run: `kind:"result"` rows (the `ContextCompressionResult`),
 * `kind:"canned"` rows (the recorded completion keys), and one `kind:"llmlogs"`
 * row (the dumped `llm_logs` table, id/createdAt/updatedAt placeholdered).
 *
 * Runs under v4's JEST (not tsx) because `createLLMProvider` is a module export
 * only `jest.doMock` can replace. The file lives in the v5 harness tree; v4's jest
 * resolves it via an extra `--roots`, with `@/` mapped to v4. (jest ignores
 * `.claude/` worktree paths, so stage a copy of this file + the fixture to /tmp.)
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-compression-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/compression-tier3.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/compression-tier3.json"  "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-compression.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- compression-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';

// The differential test pepper (shared with the Rust harness side).
const TEST_PEPPER_BASE64 = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

interface CompletionRule {
  provider: string;
  model: string;
  response?: string;
  error?: string;
  usage?: { promptTokens: number; completionTokens: number; totalTokens: number };
}

interface ProfileSpec {
  id: string;
  provider: string;
  modelName: string;
  baseUrl: string | null;
  isDangerousCompatible: boolean;
  parameters: Record<string, unknown> | null;
}

interface CallSpec {
  name: string;
  messages: Array<{ role: 'user' | 'assistant' | 'system'; content: string }>;
  systemPrompt: string;
  windowSize: number;
  compressionTargetTokens: number;
  characterName: string;
  userName: string;
  userId: string;
  selection: {
    provider: string;
    modelName: string;
    baseUrl?: string;
    connectionProfileId?: string;
    isLocal: boolean;
    profileParameters?: Record<string, unknown>;
  };
  dangerSettings: { mode: string; uncensoredTextProfileId?: string } | null;
  availableProfiles: ProfileSpec[] | null;
  completionRules: CompletionRule[];
}

interface Spec {
  calls: CallSpec[];
}

// Per-case provider control (set before each `applyContextCompression`).
let currentCall: CallSpec | null = null;

const cannedRecorded = new Map<
  string,
  {
    provider: string;
    model: string;
    temperature: number | null;
    messages: Array<{ role: string; content: string }>;
    response: string | null;
    failure: string | null;
    usage: CompletionRule['usage'] | null;
  }
>();

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'compression-tier3.json'), 'utf8')
  ) as Spec;
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  // Fresh throwaway DBs (main + mount-index) + a fresh llm-logs DB we read back.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-compression-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER_BASE64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'data', 'mount-index.db');
  process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'data', 'llm-logs.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  // Un-mock the logger service so real llm_logs rows land.
  jest.doMock('@/lib/services/llm-logging.service', () =>
    jest.requireActual('@/lib/services/llm-logging.service')
  );

  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return {
      __esModule: true,
      ...actual,
      getApiKeyForCheapLLMSelection: jest.fn(async () => 'test-key'),
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
          const call = currentCall;
          if (!call) throw new Error('no current call set');
          const rule = call.completionRules.find(
            (r) => r.provider === provider && r.model === params.model
          );
          if (!rule) {
            throw new Error(`no completion rule for provider=${provider} model=${params.model}`);
          }
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
          const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!cannedRecorded.has(key)) {
            cannedRecorded.set(key, {
              provider,
              model: params.model,
              temperature: params.temperature ?? null,
              messages,
              response: rule.error !== undefined ? null : rule.response ?? '',
              failure: rule.error ?? null,
              usage: rule.error !== undefined ? null : rule.usage ?? null,
            });
          }
          if (rule.error !== undefined) {
            throw new Error(rule.error);
          }
          return { content: rule.response ?? '', finishReason: 'stop', usage: rule.usage };
        },
      }),
    };
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawLLMLogsDatabase } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const { applyContextCompression } = await import('@/lib/chat/context/compression');

  await initializeDatabase();

  const lines: string[] = [];

  for (const call of spec.calls) {
    currentCall = call;
    // v4 builds `uncensoredFallback` from `dangerSettings && availableProfiles`;
    // `isDangerousChat` is NOT part of `ContextCompressionOptions`.
    const result = await applyContextCompression(
      call.messages as never,
      call.systemPrompt,
      {
        enabled: true,
        windowSize: call.windowSize,
        compressionTargetTokens: call.compressionTargetTokens,
        systemPromptTargetTokens: 1500,
        selection: call.selection as never,
        userId: call.userId,
        characterName: call.characterName,
        userName: call.userName,
        ...(call.dangerSettings ? { dangerSettings: call.dangerSettings as never } : {}),
        ...(call.availableProfiles
          ? { availableProfiles: call.availableProfiles as never }
          : {}),
      } as never
    );
    lines.push(JSON.stringify({ kind: 'result', call: call.name, result }));
  }
  currentCall = null;

  // `sendToProvider` fires `logLLMCall` fire-and-forget (`.catch`, not awaited),
  // so let the pending writes settle before reading. better-sqlite3 is
  // synchronous, so a short flush is ample.
  await new Promise((r) => setTimeout(r, 300));

  // Read the real llm_logs rows through the LLM-LOGS handle directly, BEFORE
  // closeDatabase() (the backend disconnect closes the llm-logs client). Mint
  // id/createdAt/updatedAt placeholdered; the rest is byte-exact.
  const lldb = getRawLLMLogsDatabase();
  if (!lldb) throw new Error('llm-logs DB handle unavailable (degraded open?)');
  const columns = (lldb.pragma('table_info(llm_logs)') as Array<{ name: string }>).map(
    (c) => c.name
  );
  const rawRows = lldb.prepare('SELECT * FROM llm_logs').all() as Array<
    Record<string, unknown>
  >;
  const rows = rawRows
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
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

  await closeDatabase();

  for (const entry of cannedRecorded.values()) lines.push(JSON.stringify({ kind: 'canned', ...entry }));
  lines.push(JSON.stringify({ kind: 'llmlogs', columns, rows }));

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`compression oracle wrote ${outPath} (${rows.length} llm_logs rows)\n`);
}

test('compression tier-3 oracle', async () => {
  await main();
});
