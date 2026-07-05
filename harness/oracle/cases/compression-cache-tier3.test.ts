/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the compression cache (v4
 * `lib/services/chat-message/compression-cache.service.ts`
 * `triggerAsyncCompression` / `getCachedCompression` / `invalidateCompressionCache`).
 *
 * Drives v4's REAL cache functions against the REAL fixture DB, mocking ONLY the
 * cheap-LLM (`createLLMProvider`) that `applyContextCompression` calls (recording
 * the canned key so the Rust side replays it) + the API-key acquisition. The
 * durable state under test is the `chats.compressionCache` column; each op records
 * its observable output (the persisted column / the getCachedCompression return).
 *
 * `triggerAsyncCompression` is fire-and-forget (kicks off the compression promise +
 * persists), so the oracle sleeps after it to let the persist settle; the Rust
 * `trigger_async_compression` is awaitable, so its DB effect is deterministic. The
 * wall clock is frozen (the persisted `createdAt` is minted → normalized in the
 * harness).
 *
 * Run (Node 24, from the v4 checkout; `.claude` worktree copy workaround):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; TMPO=/tmp/qt-oracle-run
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_COMPCACHE=/tmp/qt-compcache.db QT_ORACLE_OUT=/tmp/oracle-compcache.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- compression-cache-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CompletionRule {
  provider: string;
  model: string;
  response?: string;
  usage?: { promptTokens: number; completionTokens: number; totalTokens: number };
}
interface OpSpec {
  kind: 'trigger' | 'get' | 'invalidate';
  chatId: string;
  participantId: string | null;
  few?: boolean;
  messageCount?: number;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  frozenNowMs: number;
  systemPrompt: string;
  selection: { provider: string; modelName: string; isLocal: boolean };
  windowSize: number;
  compressionTargetTokens: number;
  systemPromptTargetTokens: number;
  characterName: string;
  userName: string;
  messages: Array<{ role: string; content: string }>;
  fewMessages: Array<{ role: string; content: string }>;
  completionRules: CompletionRule[];
  ops: OpSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'compression-cache-tier3.json'), 'utf8')
  ) as Spec;

  const fixture = process.env.QT_FIXTURE_COMPCACHE;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_COMPCACHE must point at the seed fixture');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-compcache-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'compcache-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cannedRecorded = new Map<
    string,
    { provider: string; model: string; temperature: number | null; messages: unknown[]; response: string; usage: unknown }
  >();

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

  // The cheap-LLM the compression calls — resolve by (provider, model), record the
  // canned key.
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (provider: string) => ({
        sendMessage: async (
          params: { messages: Array<{ role: string; content: string }>; model: string; temperature?: number },
          _apiKey: string
        ) => {
          const rule = spec.completionRules.find((r) => r.provider === provider && r.model === params.model);
          if (!rule) throw new Error(`no completion rule for ${provider}/${params.model}`);
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
          const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!cannedRecorded.has(key)) {
            cannedRecorded.set(key, {
              provider,
              model: params.model,
              temperature: params.temperature ?? null,
              messages,
              response: rule.response ?? '',
              usage: rule.usage ?? null,
            });
          }
          return { content: rule.response ?? '', finishReason: 'stop', usage: rule.usage };
        },
      }),
    };
  });
  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return { __esModule: true, ...actual, getApiKeyForCheapLLMSelection: async () => 'test-key' };
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const {
    triggerAsyncCompression,
    getCachedCompression,
    invalidateCompressionCache,
    clearCompressionCache,
    hashString,
  } = await import('@/lib/services/chat-message/compression-cache.service');

  await initializeDatabase();

  // Freeze the wall clock (the persisted createdAt is minted → normalized).
  const RealDate = Date;
  const FakeDate = class extends RealDate {
    constructor(...args: unknown[]) {
      if (args.length === 0) super(spec.frozenNowMs);
      // @ts-expect-error variadic forwarding
      else super(...args);
    }
    static now(): number {
      return spec.frozenNowMs;
    }
  } as DateConstructor;
  (global as { Date: DateConstructor }).Date = FakeDate;

  const sysHash = hashString(spec.systemPrompt);
  const compressionOptions = {
    enabled: true,
    windowSize: spec.windowSize,
    compressionTargetTokens: spec.compressionTargetTokens,
    systemPromptTargetTokens: spec.systemPromptTargetTokens,
    selection: spec.selection,
    userId: spec.userId,
    characterName: spec.characterName,
    userName: spec.userName,
  };

  const readColumn = async (chatId: string): Promise<unknown> => {
    const rows = (await rawQuery('SELECT compressionCache FROM chats WHERE id = ?', [chatId])) as Array<{
      compressionCache: string | null;
    }>;
    const raw = rows[0]?.compressionCache ?? null;
    return raw == null ? null : JSON.parse(raw);
  };

  const lines: string[] = [];

  for (const op of spec.ops) {
    const participantId = op.participantId ?? undefined;
    if (op.kind === 'trigger') {
      triggerAsyncCompression({
        chatId: op.chatId,
        participantId,
        messages: (op.few ? spec.fewMessages : spec.messages) as never,
        systemPrompt: spec.systemPrompt,
        compressionOptions: compressionOptions as never,
      });
      // Fire-and-forget: let the compression promise + persist settle.
      await new Promise((r) => setTimeout(r, 200));
      lines.push(JSON.stringify({ kind: 'op', name: op.kind, chatId: op.chatId, column: await readColumn(op.chatId) }));
    } else if (op.kind === 'get') {
      // Clear the in-memory cache so the read exercises the DB fallback.
      clearCompressionCache();
      const resp = await getCachedCompression(op.chatId, op.messageCount as number, participantId, sysHash);
      lines.push(JSON.stringify({ kind: 'op', name: op.kind, chatId: op.chatId, return: resp ?? null }));
    } else {
      invalidateCompressionCache(op.chatId, participantId);
      await new Promise((r) => setTimeout(r, 100));
      lines.push(JSON.stringify({ kind: 'op', name: op.kind, chatId: op.chatId, column: await readColumn(op.chatId) }));
    }
  }

  (global as { Date: DateConstructor }).Date = RealDate;

  for (const entry of cannedRecorded.values()) lines.push(JSON.stringify({ kind: 'canned', ...entry }));

  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`compression-cache oracle wrote ${outPath}\n`);
}

test('compression-cache tier-3 oracle', async () => {
  await main();
});
