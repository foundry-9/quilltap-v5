/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the W4.2 gatekeeper — the CHAT_DANGER_CLASSIFICATION job
 * runner (v4 `handleChatDangerClassification` +
 * `lib/services/dangerous-content/gatekeeper.service.ts`).
 *
 * Drives v4's REAL `handleChatDangerClassification` over a seeded fixture, with
 * ONLY the model/infra seams pinned (the real DB stack wired back in past
 * `jest.setup`, [[jest-real-db-oracle]]):
 *
 *   - `createLLMProvider` → a provider whose `sendMessage` keys the classify call
 *     on the `token=<case>` marker in the content, returns the corpus's canned
 *     response + usage, and RECORDS the exact `provider|model|temperature|messages`
 *     canned key (`kind:"canned"` rows) the Rust `CannedCompletionProvider`
 *     replays ([[tier3-completion-oracle]]).
 *   - `moderationProviderRegistry` → `getDefaultProvider()` returns a fake OPENAI
 *     provider for the moderation-path cases (its `moderate()` returns the corpus's
 *     canned raw `ModerationResult`, or throws for the failure case) and `null`
 *     for the LLM-path cases; `findApiKeyByIdAndUserId` is patched to a canned key
 *     so the real `autoDetectModerationApiKey` succeeds.
 *   - `getApiKeyForCheapLLMSelection` → constant (host-side seam).
 *   - `logLLMCall` / `postConciergeDangerAnnouncement` → no-ops (host / W4.6 seams).
 *
 * Dumps `chats` + `chat_messages`.
 *
 * Run from the v4 checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-danger-gatekeeper.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-danger-gatekeeper-fixture.ts
 *   TMPO=/tmp/qt-danger-oracle  # copy this .test.ts + spec json outside .claude
 *   QT_FIXTURE_GATEKEEPER=/tmp/qt-danger-gatekeeper.db \
 *   QT_ORACLE_OUT=/tmp/oracle-danger-gatekeeper.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/cases" -- danger-gatekeeper
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
function canonicalizeRows(table: string, columns: string[], rawRows: Array<Record<string, unknown>>, orderBy: string) {
  const rows = rawRows
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    })
    .sort((a, b) => (String(a[orderBy] ?? '') < String(b[orderBy] ?? '') ? -1 : String(a[orderBy] ?? '') > String(b[orderBy] ?? '') ? 1 : 0));
  return { table, columns, rows };
}

interface Usage { promptTokens: number; completionTokens: number; totalTokens: number }
interface Case { name: string; chatId: string; user: 'A' | 'B'; connectionProfileId: string }
interface Spec {
  testPepperBase64: string;
  userA: string;
  userB: string;
  llmRules: Record<string, { response: string; usage: Usage }>;
  moderationResults: Record<string, { flagged: boolean; categories: Array<{ category: string; score: number }> } | 'fail'>;
  cases: Case[];
}

// Per-call moderation control (set before each handler invocation).
let currentModeration: { flagged: boolean; categories: Array<{ category: string; score: number }> } | 'fail' | null = null;

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'danger-gatekeeper.json'), 'utf8')
  ) as Spec;

  const fixture = process.env.QT_FIXTURE_GATEKEEPER;
  if (!fixture || !existsSync(fixture)) throw new Error('QT_FIXTURE_GATEKEEPER must point at the seed fixture');
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-danger-gatekeeper-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'gatekeeper-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cannedRecorded = new Map<string, { provider: string; model: string; temperature: number | null; messages: Array<{ role: string; content: string }>; response: string; usage: Usage }>();

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(process.cwd(), 'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers');
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (provider: string, _baseUrl?: string) => ({
        sendMessage: async (params: { messages: Array<{ role: string; content: string }>; model: string; temperature?: number }, _apiKey: string) => {
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
          const user = messages.find((m) => m.role === 'user')?.content ?? '';
          const mt = user.match(/token=(\S+)/);
          if (!mt) throw new Error('canned completion: no token= marker in classify content');
          const rule = spec.llmRules[mt[1]];
          if (!rule) throw new Error(`no llm rule for token ${mt[1]}`);
          const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!cannedRecorded.has(key)) {
            cannedRecorded.set(key, { provider, model: params.model, temperature: params.temperature ?? null, messages, response: rule.response, usage: rule.usage });
          }
          return { content: rule.response, finishReason: 'stop', usage: rule.usage };
        },
      }),
    };
  });

  jest.doMock('@/lib/plugins/moderation-provider-registry', () => ({
    __esModule: true,
    moderationProviderRegistry: {
      isInitialized: () => true,
      getAllProviders: () => [],
      getDefaultProvider: () => {
        if (currentModeration === null) return null;
        return {
          metadata: { providerName: 'OPENAI' },
          moderate: async (_content: string, _key: string) => {
            if (currentModeration === 'fail') throw new Error('moderation provider exploded');
            return currentModeration;
          },
        };
      },
    },
  }));

  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return { __esModule: true, ...actual, getApiKeyForCheapLLMSelection: async () => 'test-key' };
  });
  jest.doMock('@/lib/services/llm-logging.service', () => {
    const actual = jest.requireActual('@/lib/services/llm-logging.service');
    return { __esModule: true, ...actual, logLLMCall: async () => undefined };
  });
  jest.doMock('@/lib/services/concierge-notifications/writer', () => ({
    __esModule: true,
    postConciergeDangerAnnouncement: async () => undefined,
    postConciergeManualAnnouncement: async () => undefined,
  }));

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { handleChatDangerClassification } = await import('@/lib/background-jobs/handlers/chat-danger-classification');

  await initializeDatabase();
  const repos = getRepositories();
  const uid = (u: 'A' | 'B') => (u === 'A' ? spec.userA : spec.userB);

  // Canned key seam so `autoDetectModerationApiKey` succeeds on the moderation path.
  (repos.connections as any).findApiKeyByIdAndUserId = async (id: string, userId: string) => ({
    id, userId, label: 'canned', provider: 'OPENAI', key_value: 'moderation-key', isActive: true, createdAt: '2020-01-01T00:00:00.000Z', updatedAt: '2020-01-01T00:00:00.000Z',
  });

  for (const c of spec.cases) {
    currentModeration = spec.moderationResults[c.name] ?? null;
    const job = { id: `job-${c.name}`, userId: uid(c.user), type: 'CHAT_DANGER_CLASSIFICATION', status: 'PROCESSING', payload: { chatId: c.chatId, connectionProfileId: c.connectionProfileId } };
    await handleChatDangerClassification(job as never);
  }
  currentModeration = null;

  const dumpTable = async (table: string, orderBy: string) => {
    const columns = ((await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>).map((c) => c.name);
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
    return canonicalizeRows(table, columns, rawRows, orderBy);
  };

  const lines: string[] = [];
  for (const entry of cannedRecorded.values()) lines.push(JSON.stringify({ kind: 'canned', ...entry }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chats', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chat_messages', 'chatId')) }));

  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`danger-gatekeeper oracle wrote ${outPath}\n`);
}

test('danger-gatekeeper tier-3 oracle', async () => {
  await main();
});
