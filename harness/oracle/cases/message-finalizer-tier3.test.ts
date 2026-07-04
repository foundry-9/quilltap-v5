/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the message finalizer (v4 `finalizeMessageResponse` /
 * `calculateNextSpeaker`, lib/services/chat-message/message-finalizer.service.ts).
 *
 * Drives v4's REAL `finalizeMessageResponse` over the committed corpus
 * (harness/oracle/fixtures/message-finalizer-tier3.json) against the REAL two-DB
 * fixture (main + mount-index — the next-speaker talkativeness reads go through
 * the character vault), with ONLY the out-of-scope subsystems stubbed to match
 * the Rust injected seams:
 *
 *   - `triggerAsyncCompression` (compression-cache subsystem) → records the call;
 *   - `detectAndConvertRngPatterns` → returns `[]` (no patterns);
 *   - `runCarinaMarkupQuery` → for a corpus `carina` case, emits the fixed
 *     `carinaAnswer` event over the controller (mirroring v4's real runner's
 *     `onPosted`), and writes NOTHING to the DB (the carina posting path is
 *     separately verified by `carina_runner_tier3_equivalence`); otherwise a
 *     no-op. The Rust seam emits the SAME fixed message.
 *   - `estimateMessageCost` → records the forward args, returns a null cost (so
 *     `trackMessageTokenUsage`'s aggregate write is skipped both sides — the
 *     cost seam is recorded, not diffed as a DB write);
 *   - `logLLMCall` → no-op.
 *
 * Everything else — the content cleaning, anchor/reasoning re-basing, the
 * confirmation skip gates, the message persistence + image link, the updatedAt
 * bump, next-speaker selection, the done event, and the memory-extraction /
 * danger-classification enqueues — is v4's REAL code against the REAL fixture DBs.
 *
 * The SSE controller is a recording controller: it decodes each `data: {…}` frame
 * to JSON, so the emitted event trace can be diffed against the Rust RecordingSink.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-mf-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-mf-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-message-finalizer-fixture.ts
 *   QT_FIXTURE_MF_MAIN=/tmp/qt-mf-main.db QT_FIXTURE_MF_MOUNT=/tmp/qt-mf-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-message-finalizer.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- message-finalizer-tier3
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

interface CarinaSpec {
  characterName: string;
  whisper: boolean;
  answererParticipantId: string;
  answererCharacterId: string;
  answererName: string;
  messageId: string;
  answer: string;
  askerParticipantId: string;
}
interface CallSpec {
  name: string;
  chatId: string;
  responderCharacter: string;
  responderParticipant: string;
  userParticipantId: string | null;
  isMultiCharacter: boolean;
  preGeneratedAssistantMessageId: string;
  fullResponse: string;
  usage: { promptTokens: number; completionTokens: number; totalTokens: number };
  reasoningContent: string | null;
  reasoningSegments: Array<{ anchorOffset: number; content: string; seq: number }>;
  normalizeRewroteBody: boolean;
  generatedImageIds: string[];
  toolMessages?: Array<Record<string, unknown>>;
  compression: { enabled: boolean; selection: boolean; systemPrompt: boolean };
  chatSettings: boolean;
  participantCharacters: string[];
  carina?: CarinaSpec;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  chatSettings: { id: string };
  calls: CallSpec[];
}

// The recorded carina message emitted for a `carina` case (both sides emit this
// EXACT object as the `carinaAnswer` frame). Built here so the fixture drives it.
function carinaMessage(spec: CarinaSpec): Record<string, unknown> {
  return {
    id: spec.messageId,
    type: 'message',
    role: 'ASSISTANT',
    content: spec.answer,
    participantId: spec.answererParticipantId,
    systemSender: 'carina',
  };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'message-finalizer-tier3.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_MF_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_MF_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_MF_MAIN / QT_FIXTURE_MF_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mf-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'mf-main.db');
  const workMount = join(scratch, 'mf-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // Recorded seam calls (compression triggers + cost tracks) across the whole run.
  const compressionTriggers: Array<{ chatId: string; participantId: string | undefined }> = [];
  const costTracks: Array<Record<string, unknown>> = [];
  // The carina message for the current call (steered per-call for the mock).
  let currentCarina: CarinaSpec | undefined;

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

  // Out-of-scope subsystem seams → recorders / no-ops (matching the Rust seams).
  jest.doMock('@/lib/services/chat-message/compression-cache.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/compression-cache.service');
    return {
      __esModule: true,
      ...actual,
      triggerAsyncCompression: (opts: { chatId: string; participantId?: string }) => {
        compressionTriggers.push({ chatId: opts.chatId, participantId: opts.participantId });
      },
    };
  });
  jest.doMock('@/lib/services/chat-message/rng-pattern-detector.service', () => {
    const actual = jest.requireActual('@/lib/services/chat-message/rng-pattern-detector.service');
    return {
      __esModule: true,
      ...actual,
      detectAndConvertRngPatterns: () => [],
    };
  });
  jest.doMock('@/lib/services/cost-estimation.service', () => {
    const actual = jest.requireActual('@/lib/services/cost-estimation.service');
    return {
      __esModule: true,
      ...actual,
      estimateMessageCost: async (
        provider: string,
        modelName: string,
        promptTokens: number,
        completionTokens: number,
        userId: string
      ) => {
        costTracks.push({ provider, modelName, promptTokens, completionTokens, userId });
        // Null cost → the aggregate write is skipped (the cost seam is recorded,
        // not diffed as a DB write; the Rust seam records the same args).
        return { cost: null, source: 'unavailable' };
      },
    };
  });
  jest.doMock('@/lib/services/carina/markup-runner', () => {
    const actual = jest.requireActual('@/lib/services/carina/markup-runner');
    return {
      __esModule: true,
      ...actual,
      runCarinaMarkupQuery: async (opts: {
        text: string;
        onPosted?: (msg: unknown) => void;
      }) => {
        // Mirror the Rust seam boundary exactly: run the REAL markup parse (the
        // ported runner does), and only when markup parses does the injected
        // query seam fire - here emitting the fixed message via onPosted
        // (mirroring v4's real runner's onPosted). No DB write (the posting
        // path is separately verified by carina_runner_tier3_equivalence).
        const { parseCarinaQuery } = jest.requireActual('@/lib/chat/carina-parser');
        const parsed = parseCarinaQuery(opts.text);
        if (!parsed) return;
        if (!currentCarina) {
          throw new Error('carina markup parsed but the corpus call has no carina spec');
        }
        if (parsed.characterName !== currentCarina.characterName) {
          throw new Error('carina parse mismatch: ' + parsed.characterName);
        }
        opts.onPosted?.(carinaMessage(currentCarina));
      },
    };
  });
  // Keep the in-process job runner OFF: the enqueued PENDING rows are state
  // under test, and the Rust port defers the runner the same way.
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });
  jest.doMock('@/lib/services/llm-logging.service', () => {
    const actual = jest.requireActual('@/lib/services/llm-logging.service');
    return { __esModule: true, ...actual, logLLMCall: async () => undefined };
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

  // Decode a recording SSE controller: each enqueued Uint8Array is one or more
  // `data: {json}\n\n` frames; capture the parsed JSON objects in order.
  const decoder = new TextDecoder();
  function makeController(sink: unknown[]): {
    enqueue: (u: Uint8Array) => void;
    close: () => void;
    error: () => void;
  } {
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

  const chatSettings = spec.chatSettings.id
    ? await repos.chatSettings.findById(spec.chatSettings.id)
    : null;

  for (const call of spec.calls) {
    currentCarina = call.carina;

    const chat = await repos.chats.findById(call.chatId);
    if (!chat) throw new Error(`chat not found: ${call.chatId}`);
    const character = await repos.characters.findById(call.responderCharacter);
    if (!character) throw new Error(`character not found: ${call.responderCharacter}`);

    const characterParticipant = (chat.participants as Array<{ id: string; status?: string }>).find(
      (p) => p.id === call.responderParticipant
    );
    if (!characterParticipant) {
      throw new Error(`responder participant not found: ${call.responderParticipant}`);
    }

    const participantCharacters = new Map<string, unknown>();
    for (const cid of call.participantCharacters) {
      const c = await repos.characters.findById(cid);
      if (c) participantCharacters.set(cid, c);
    }

    // For normalizeRewroteBody cases the fullResponse is a content-block wrapper
    // (```json {"content": …} ```) that v4's normalizeContentBlockFormat extracts;
    // otherwise the fullResponse is the plain streamed text.
    const eventSink: unknown[] = [];
    const controller = makeController(eventSink);

    const streaming = {
      fullResponse: call.fullResponse,
      effectiveProfile: {
        id: 'profile-mf-0000-0000-000000000001',
        provider: 'ANTHROPIC',
        modelName: 'claude-sonnet',
        baseUrl: null,
      },
      effectiveApiKey: 'test-key',
      usage: call.usage,
      cacheUsage: null,
      attachmentResults: null,
      rawResponse: null,
      thoughtSignature: undefined,
      reasoningContent: call.reasoningContent ?? undefined,
      reasoningSegments:
        call.reasoningSegments.length > 0 ? call.reasoningSegments : undefined,
      hasStartedStreaming: true,
    };

    const compression = {
      existingMessages: [],
      content: '',
      builtContext: {
        originalSystemPrompt: call.compression.systemPrompt ? 'the original system prompt' : undefined,
      },
      compressionEnabled: call.compression.enabled,
      cheapLLMSelection: call.compression.selection
        ? {
            provider: 'ANTHROPIC',
            modelName: 'claude-haiku',
            connectionProfileId: 'profile-mf-0000-0000-000000000001',
            isLocal: false,
          }
        : null,
      contextCompressionSettings: {
        enabled: call.compression.enabled,
        windowSize: 10,
        compressionTargetTokens: 1000,
        systemPromptTargetTokens: 500,
      },
      allProfiles: [],
    };

    const triggers = {
      dangerSettings: { mode: 'DETECT_ONLY' },
      chatSettings: call.chatSettings ? chatSettings : null,
      participantCharacters,
      resolvedIdentity: { name: character.name, description: '', characterId: character.id },
    };

    const result = await finalizeMessageResponse({
      repos,
      chatId: call.chatId,
      userId: spec.userId,
      chat: chat as never,
      character: character as never,
      characterParticipant: characterParticipant as never,
      userParticipantId: call.userParticipantId,
      isMultiCharacter: call.isMultiCharacter,
      isContinueMode: false,
      generatedImagePaths: call.generatedImageIds.map((id) => ({
        id,
        filename: 'generated.png',
        filepath: '/tmp/generated.png',
        mimeType: 'image/png',
        size: 1024,
      })) as never,
      // W4.1c c.3: inject the tool slate so v4's finalizer saves TOOL rows
      // (dormant for every pre-existing case; `tool-save` exercises it).
      toolMessages: (call.toolMessages ?? []) as never,
      preGeneratedAssistantMessageId: call.preGeneratedAssistantMessageId,
      connectionProfile: streaming.effectiveProfile as never,
      controller: controller as never,
      encoder,
      streaming: streaming as never,
      compression: compression as never,
      triggers: triggers as never,
    });

    lines.push(JSON.stringify({ kind: 'result', call: call.name, result }));
    lines.push(JSON.stringify({ kind: 'events', call: call.name, events: eventSink }));
  }

  // Let the fire-and-forget background triggers settle before dumping.
  await new Promise((resolve) => setTimeout(resolve, 200));

  for (const t of compressionTriggers) {
    lines.push(JSON.stringify({ kind: 'compression', ...t }));
  }
  for (const c of costTracks) {
    lines.push(JSON.stringify({ kind: 'cost', ...c }));
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
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('background_jobs', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('files', 'id')) }));

  closeMountIndexSQLiteClient();
  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`message-finalizer oracle wrote ${outPath}\n`);
}

test('message-finalizer tier-3 oracle', async () => {
  await main();
});
