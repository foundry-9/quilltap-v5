/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the answer-confirmation service (W4.3; v4
 * `finalizeMessageResponse` + `answer-confirmation.service.ts`).
 *
 * Drives v4's REAL `finalizeMessageResponse` over the committed corpus
 * (harness/oracle/fixtures/answer-confirmation-tier3.json) with the
 * answer-confirmation feature ON, against the REAL two-DB fixture, mocking ONLY
 * the model boundary + API keys (the real DB stack wired back in past
 * `jest.setup`, [[jest-real-db-oracle]]):
 *
 *   - `createLLMProvider` → a provider whose `sendMessage` keys the cheap-LLM
 *     call on the `[[CASE:<name>]]` marker in the reply text carried in the user
 *     message, distinguishes the consistency-check from the re-affirmation by the
 *     system prompt, returns the corpus's canned verdict, and RECORDS the exact
 *     `provider|model|temperature|messages` canned key (`kind:"canned"` rows) the
 *     Rust `CannedCompletionProvider` replays ([[tier3-completion-oracle]]). The
 *     recorded keys prove the prompt bytes — the reference-block assembly + the
 *     24K oldest-first truncation + the danger escalation's cheap-profile switch.
 *   - `getApiKeyForCheapLLMSelection` → constant (host-side seam).
 *   - `triggerAsyncCompression` / `estimateMessageCost` /
 *     `runCarinaMarkupQuery` → recorders / no-ops (matching the Rust seams).
 *   - `logLLMCall` runs REAL (W4.10b): the check + re-affirmation each land one
 *     ANSWER_CONFIRMATION `llm_logs` row, dumped + diffed by the Rust harness.
 *
 * Dumps per-call `result` + the ordered event trace + `chats` + `chat_messages`.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-ac-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-ac-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-answer-confirmation-fixture.ts
 *   TMPO=/tmp/qt-ac-oracle; mkdir -p $TMPO/cases $TMPO/fixtures
 *   cp $V5W/harness/oracle/cases/answer-confirmation-tier3.test.ts $TMPO/cases/
 *   cp $V5W/harness/oracle/fixtures/answer-confirmation-tier3.json $TMPO/fixtures/
 *   QT_FIXTURE_AC_MAIN=/tmp/qt-ac-main.db QT_FIXTURE_AC_MOUNT=/tmp/qt-ac-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-answer-confirmation.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- answer-confirmation-tier3
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
function canonicalizeRows(
  table: string,
  columns: string[],
  rawRows: Array<Record<string, unknown>>,
  orderBy: string
) {
  const rows = rawRows
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    })
    .sort((a, b) =>
      String(a[orderBy] ?? '') < String(b[orderBy] ?? '')
        ? -1
        : String(a[orderBy] ?? '') > String(b[orderBy] ?? '')
          ? 1
          : 0
    );
  return { table, columns, rows };
}

interface ToolMessageSpec {
  toolName: string;
  success: boolean;
  content: string;
  arguments: Record<string, unknown>;
}
interface CallSpec {
  name: string;
  chatId: string;
  participantId: string;
  assistantMessageId: string;
  projectId: string | null;
  chatOverride: 'ON' | 'OFF' | null;
  globalEnabled: boolean;
  impersonated: boolean;
  silent: boolean;
  dangerous?: boolean;
  whisper?: string | null;
  whisperRepeat?: { unit: string; times: number };
  toolMessages: ToolMessageSpec[];
  reply: string;
  expectActive: boolean;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  chatSettingsId: string;
  cheapSelection: Record<string, unknown>;
  connectionProfile: {
    id: string;
    provider: string;
    modelName: string;
    baseUrl: string | null;
    parameters: unknown;
  };
  uncensoredProfile: {
    id: string;
    provider: string;
    modelName: string;
    isDangerousCompatible: boolean;
  };
  checkRules: Record<string, string>;
  reaffRules: Record<string, string>;
  calls: CallSpec[];
}

// The re-affirmation system prompt (v4 `buildReaffirmationSystemPrompt`) — used to
// distinguish a re-affirmation call from the consistency check.
const REAFFIRMATION_SYSTEM =
  'You are reconsidering a reply you just drafted, in your own voice, before it is sent. Some of what you wrote appears to conflict with what you actually know or looked up this turn. Respond ONLY with strict JSON.';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'answer-confirmation-tier3.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_AC_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_AC_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_AC_MAIN / QT_FIXTURE_AC_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ac-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'ac-main.db');
  const workMount = join(scratch, 'ac-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  // W4.10b: a fresh llm-logs DB so the un-mocked `logLLMCall` lands the
  // ANSWER_CONFIRMATION rows (check + re-affirmation).
  process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'ac-llm-logs.db');
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
    }
  >();

  function caseFromContent(content: string): string {
    const m = content.match(/\[\[CASE:([^\]]+)\]\]/);
    if (!m) throw new Error('canned completion: no [[CASE:...]] marker in content');
    return m[1];
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
          const user = messages.find((m) => m.role === 'user')?.content ?? '';
          const isReaff = system === REAFFIRMATION_SYSTEM;
          const caseName = caseFromContent(user);
          const rule = isReaff ? spec.reaffRules[caseName] : spec.checkRules[caseName];
          if (rule === undefined) {
            throw new Error(
              `no ${isReaff ? 'reaff' : 'check'} rule for case ${caseName}`
            );
          }
          const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!cannedRecorded.has(key)) {
            cannedRecorded.set(key, {
              provider,
              model: params.model,
              temperature: params.temperature ?? null,
              messages,
              response: rule,
            });
          }
          return { content: rule, finishReason: 'stop', usage: null };
        },
      }),
    };
  });

  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return { __esModule: true, ...actual, getApiKeyForCheapLLMSelection: async () => 'test-key' };
  });
  // W4.10b: run the REAL `logLLMCall` so the check + re-affirmation land
  // ANSWER_CONFIRMATION rows in the llm-logs DB (dumped below).
  jest.doMock('@/lib/services/llm-logging.service', () =>
    jest.requireActual('@/lib/services/llm-logging.service')
  );
  jest.doMock('@/lib/services/chat-message/compression-cache.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/compression-cache.service');
    return {
      __esModule: true,
      ...actual,
      triggerAsyncCompression: () => undefined,
    };
  });
  jest.doMock('@/lib/services/cost-estimation.service', () => {
    const actual = jest.requireActual('@/lib/services/cost-estimation.service');
    return {
      __esModule: true,
      ...actual,
      estimateMessageCost: async () => ({ cost: null, source: 'unavailable' }),
    };
  });
  jest.doMock('@/lib/services/carina/markup-runner', () => {
    const actual = jest.requireActual('@/lib/services/carina/markup-runner');
    return {
      __esModule: true,
      ...actual,
      // No corpus reply carries carina markup; a no-op mirrors the Rust seam.
      runCarinaMarkupQuery: async () => undefined,
    };
  });
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { finalizeMessageResponse } = await import(
    '@/lib/services/chat-message/message-finalizer.service'
  );

  await initializeDatabase();
  const repos = getRepositories();

  const baseChatSettings = await repos.chatSettings.findById(spec.chatSettingsId);
  if (!baseChatSettings) throw new Error('chat settings not found');

  // Recording SSE controller: decode each `data: {…}` frame to JSON in order.
  const decoder = new TextDecoder();
  function makeController(sink: unknown[]) {
    return {
      enqueue: (u: Uint8Array) => {
        const text = decoder.decode(u);
        for (const line of text.split('\n')) {
          const t = line.trim();
          if (t.startsWith('data:')) {
            const body = t.slice('data:'.length).trim();
            if (body) sink.push(JSON.parse(body));
          }
        }
      },
      close: () => {},
      error: () => {},
    };
  }
  const encoder = new TextEncoder();

  const lines: string[] = [];

  const allProfiles = [
    {
      id: spec.uncensoredProfile.id,
      provider: spec.uncensoredProfile.provider,
      modelName: spec.uncensoredProfile.modelName,
      isDangerousCompatible: spec.uncensoredProfile.isDangerousCompatible,
    },
  ];

  for (const call of spec.calls) {
    const chat = await repos.chats.findById(call.chatId);
    if (!chat) throw new Error(`chat not found: ${call.chatId}`);

    const characterParticipant = (
      chat.participants as Array<{ id: string; status?: string; characterId: string }>
    ).find((p) => p.id === call.participantId);
    if (!characterParticipant) throw new Error(`participant not found: ${call.participantId}`);

    const responderChar = await repos.characters.findById(characterParticipant.characterId);
    if (!responderChar) throw new Error('responder character not found');

    const eventSink: unknown[] = [];
    const controller = makeController(eventSink);

    const streaming = {
      fullResponse: call.reply,
      effectiveProfile: {
        id: spec.connectionProfile.id,
        provider: spec.connectionProfile.provider,
        modelName: spec.connectionProfile.modelName,
        baseUrl: spec.connectionProfile.baseUrl,
      },
      effectiveApiKey: 'test-key',
      usage: { promptTokens: 5, completionTokens: 7, totalTokens: 12 },
      cacheUsage: null,
      attachmentResults: null,
      rawResponse: null,
      thoughtSignature: undefined,
      reasoningContent: undefined,
      reasoningSegments: undefined,
      hasStartedStreaming: true,
    };

    const compression = {
      existingMessages: [],
      content: '',
      builtContext: { originalSystemPrompt: undefined },
      compressionEnabled: false,
      cheapLLMSelection: spec.cheapSelection,
      contextCompressionSettings: { enabled: false },
      allProfiles,
    };

    // Per-case global answer-confirmation enabled override (the DB row carries a
    // fixed value; the finalizer reads whatever `triggers.chatSettings` provides).
    const perCaseSettings = {
      ...baseChatSettings,
      answerConfirmationSettings: { enabled: call.globalEnabled },
    };

    const triggers = {
      dangerSettings: {
        mode: 'AUTO_ROUTE',
        uncensoredTextProfileId: spec.uncensoredProfile.id,
      },
      chatSettings: perCaseSettings,
      participantCharacters: new Map<string, unknown>(),
      resolvedIdentity: {
        name: responderChar.name,
        description: '',
        characterId: responderChar.id,
      },
    };

    const result = await finalizeMessageResponse({
      repos,
      chatId: call.chatId,
      userId: spec.userId,
      chat: chat as never,
      character: responderChar as never,
      characterParticipant: characterParticipant as never,
      userParticipantId: null,
      isMultiCharacter: false,
      isContinueMode: false,
      generatedImagePaths: [] as never,
      toolMessages: call.toolMessages.map((tm) => ({
        toolName: tm.toolName,
        success: tm.success,
        content: tm.content,
        arguments: tm.arguments,
      })) as never,
      preGeneratedAssistantMessageId: call.assistantMessageId,
      connectionProfile: {
        id: spec.connectionProfile.id,
        provider: spec.connectionProfile.provider,
        modelName: spec.connectionProfile.modelName,
        baseUrl: spec.connectionProfile.baseUrl,
        parameters: spec.connectionProfile.parameters,
      } as never,
      controller: controller as never,
      encoder,
      streaming: streaming as never,
      compression: compression as never,
      triggers: triggers as never,
    });

    lines.push(JSON.stringify({ kind: 'result', call: call.name, result }));
    lines.push(JSON.stringify({ kind: 'events', call: call.name, events: eventSink }));
  }

  // Let fire-and-forget background triggers settle before dumping.
  await new Promise((resolve) => setTimeout(resolve, 200));

  const dumpTable = async (table: string, orderBy: string) => {
    const columns = (
      (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
    return canonicalizeRows(table, columns, rawRows, orderBy);
  };

  // Read the ANSWER_CONFIRMATION rows through the llm-logs handle directly,
  // BEFORE closeDatabase() (id/createdAt/updatedAt placeholdered).
  const { getRawLLMLogsDatabase } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const lldb = getRawLLMLogsDatabase();
  if (!lldb) throw new Error('llm-logs DB handle unavailable (degraded open?)');
  const llColumns = (
    lldb.pragma('table_info(llm_logs)') as Array<{ name: string }>
  ).map((c) => c.name);
  const llRawRows = lldb.prepare('SELECT * FROM llm_logs').all() as Array<
    Record<string, unknown>
  >;
  const llRows = llRawRows
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of llColumns) out[col] = canonValue(r[col]);
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

  for (const entry of cannedRecorded.values()) lines.push(JSON.stringify({ kind: 'canned', ...entry }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chats', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chat_messages', 'id')) }));
  lines.push(JSON.stringify({ kind: 'llmlogs', columns: llColumns, rows: llRows }));

  closeMountIndexSQLiteClient();
  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`answer-confirmation oracle wrote ${outPath}\n`);
}

test('answer-confirmation tier-3 oracle', async () => {
  await main();
});
