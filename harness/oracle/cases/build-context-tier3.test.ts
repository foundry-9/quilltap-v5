/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the buildContext capstone (v4 `buildContext`,
 * lib/chat/context-manager.ts:477).
 *
 * Drives v4's REAL `buildContext` over the committed corpus
 * (harness/oracle/fixtures/build-context-tier3.json) against a REAL two-database
 * fixture, with ONLY the model seams + the unported feeder / post-office
 * subsystems mocked (to match the Rust `NoopSeams`):
 *
 *   - `generateEmbeddingForUser` → the corpus's canned vectors (keyed by exact
 *     query text) — the same vectors the Rust `CannedEmbeddingProvider` injects.
 *   - `createLLMProvider` → the compression case's canned completion, RECORDING
 *     the exact `provider|model|temperature|messages` key it answered (the Rust
 *     side replays it through `CannedCompletionProvider`).
 *   - `getApiKeyForCheapLLMSelection` → constant; `logLLMCall` → no-op.
 *   - `resolveTieredMountPool` → the responding character's own vault only (no
 *     group / project / global tier) = the Rust `resolve_mount_pool` default.
 *   - `generateMemoryRecap` → `{ content: '' }` (no recap).
 *   - `extractMemorySearchKeywords` → `{ success: false }` (no distillation).
 *   - `getOrComputeFrozenArchive` → `[]` (no archive).
 *   - `getMemoryRecallSettings` → BALANCED / no expand.
 *   - `getCompiledIdentityStack` → null (fresh build).
 *   - the wardrobe live-clothing resolution + the post-office writers
 *     (`postCoreWhisper`/`postCommonplaceWhisper`/`postSuparnaMailWhisper`/
 *     `postHostTimestampAnnouncement`/`postHostOffSceneCharactersAnnouncement`/
 *     `surfaceOperatorMailForChat`/`collectUnalertedMail`) → no-ops returning
 *     "nothing", matching the Rust seam defaults.
 *
 * Everything else — the system-prompt build, budget math, message selection,
 * memory formatters, `searchMemoriesSemantic`, `retrieveKnowledgeForTurn`, the
 * compression service, the token accounting — is v4's REAL code against the REAL
 * fixture DB. Each op's full `BuiltContext` (JSON) is emitted; the Rust harness
 * compares byte-for-byte (only genuinely minted values normalized — none here,
 * the read path mints nothing persistent).
 *
 * Runs under v4's JEST (real-DB-under-jest: resetModules + doMock past
 * jest.setup's global mocks; cipher driver by absolute path). Needs
 * `--watchman=false` with the v5 --roots. Node 24.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-build-context-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-build-context-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-context-tier3-fixture.ts
 *   QT_FIXTURE_BC_MAIN=/tmp/qt-build-context-main.db \
 *   QT_FIXTURE_BC_MOUNT=/tmp/qt-build-context-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-build-context.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- build-context-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Op {
  name: string;
  characterId: string;
  newUserMessage?: string;
  existingMessages?: Array<{ id?: string; role: string; content: string }>;
  existingMessagesRepeat?: { count: number; userContent: string; assistantContent: string };
  chatOverrides?: { summaryAnchorMessageIds?: string[]; contextSummary?: string };
  respondingParticipantId?: string;
  activeUserParticipantId?: string;
  participants?: Array<{
    id: string;
    type: string;
    characterId: string;
    controlledBy: string;
    status: string;
    hasHistoryAccess: boolean;
    createdAt: string;
  }>;
  messagesWithParticipants?: Array<{
    id: string;
    role: string;
    content: string;
    participantId: string;
    createdAt: string;
    targetParticipantIds?: string[];
  }>;
  skipMemories: boolean;
  compressionEnabled: boolean;
  timestampMode: string | null;
  cachedCompression?: {
    compressedHistory: string;
    messageCount: number;
    warnings?: string[];
  };
}
interface CompletionRule {
  op: string;
  kind: string;
  response: string;
  usage?: { promptTokens: number; completionTokens: number; totalTokens: number };
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  characters: Array<{ id: string; name: string }>;
  cannedEmbeddings: Record<string, number[]>;
  completionRules: CompletionRule[];
  chat: { id: string; type: string; compactionGeneration: number };
  ops: Op[];
}

const DEFAULT_USAGE = { promptTokens: 40, completionTokens: 20, totalTokens: 60 };

/** Fixed wall clock so `new Date()` / `Date.now()` inside buildContext (the
 * timestamp whisper) is deterministic. The Rust side injects the same `now_ms`.
 * 2024-06-15T12:00:00.000Z. */
const FIXED_NOW_MS = 1718452800000;

function buildExistingMessages(op: Op): Array<{ id?: string; role: string; content: string }> {
  if (op.existingMessagesRepeat) {
    const out: Array<{ id?: string; role: string; content: string }> = [];
    for (let i = 0; i < op.existingMessagesRepeat.count; i++) {
      out.push({ role: 'user', content: op.existingMessagesRepeat.userContent });
      out.push({ role: 'assistant', content: op.existingMessagesRepeat.assistantContent });
    }
    return out;
  }
  return op.existingMessages ?? [];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'build-context-tier3.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_BC_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_BC_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain)) throw new Error('QT_FIXTURE_BC_MAIN missing');
  if (!fixtureMount || !existsSync(fixtureMount)) throw new Error('QT_FIXTURE_BC_MOUNT missing');
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT missing');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-build-context-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'bc-main.db');
  const workMount = join(scratch, 'bc-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
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
      usage: { promptTokens: number; completionTokens: number; totalTokens: number };
    }
  >();
  let currentOp = '';

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

  // ---- Model seams ----
  jest.doMock('@/lib/embedding/vector-store', () => jest.requireActual('@/lib/embedding/vector-store'));
  jest.doMock('@/lib/embedding/embedding-service', () => {
    const actual = jest.requireActual('@/lib/embedding/embedding-service');
    return {
      __esModule: true,
      ...actual,
      generateEmbeddingForUser: async (text: string) => {
        const vec = spec.cannedEmbeddings[text];
        if (!vec) throw new Error(`no canned embedding for ${JSON.stringify(text)}`);
        return { embedding: new Float32Array(vec), model: 'canned', provider: 'canned', dimensions: vec.length };
      },
    };
  });
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
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
          const rule = spec.completionRules.find((r) => r.op === currentOp);
          if (!rule) throw new Error(`no completion rule for op=${currentOp}`);
          const usage = rule.usage ?? DEFAULT_USAGE;
          const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!cannedRecorded.has(key)) {
            cannedRecorded.set(key, { provider, model: params.model, temperature: params.temperature ?? null, messages, response: rule.response, usage });
          }
          return { content: rule.response, finishReason: 'stop', usage };
        },
      }),
    };
  });
  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return { __esModule: true, ...actual, getApiKeyForCheapLLMSelection: async () => 'test-key' };
  });
  jest.doMock('@/lib/services/llm-logging.service', () => {
    const actual = jest.requireActual('@/lib/services/llm-logging.service');
    return { __esModule: true, ...actual, logLLMCall: async () => undefined };
  });

  // ---- W4.6a: the READ/COMPUTE feeders now run for REAL both sides (the Rust
  //      port closed each seam in-line), so their mocks are DROPPED one-for-one.
  //      The only remaining mocks are the W4.6b whisper-POSTing writers (below).
  //      `resolveTieredMountPool`, `generateMemoryRecap`, `getOrComputeFrozenArchive`,
  //      `getMemoryRecallSettings`, `extractMemorySearchKeywords`,
  //      `resolveCoreWhisperConfig`/`assembleCorePacket` all run real.
  jest.doMock('@/lib/services/system-prompt-compiler/compiler', () => {
    const actual = jest.requireActual('@/lib/services/system-prompt-compiler/compiler');
    return { __esModule: true, ...actual, getCompiledIdentityStack: () => null };
  });

  // ---- W4.6b post-office writers → no-op / recorded (the POSTs stay seamed) ----
  // Round-3 unification (Group 2): the W4.6b whisper writers are now wired LIVE
  // into the Rust spine (`RealBuildContextSeams`), so they run REAL on both sides
  // — no `postCore/postCommonplace/postSuparna/postHost*` overrides, and the
  // mailbox READ (`collectUnalertedMail`/`markAlerted`) runs real too. buildContext
  // diffs only the returned `BuiltContext`; the POSTs (chat_messages rows) are
  // proven end-to-end in `orchestrator_tier3` (which dumps the tables). The
  // operator-mail sweep is a chat-load path, not a buildContext feeder, so it stays
  // mocked to a no-op.
  jest.doMock('@/lib/post-office/surface-operator-mail', () => {
    const actual = jest.requireActual('@/lib/post-office/surface-operator-mail');
    return { __esModule: true, ...actual, surfaceOperatorMailForChat: async () => undefined };
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { buildContext } = await import('@/lib/chat/context-manager');

  await initializeDatabase();
  const repos = getRepositories();

  // Freeze the wall clock: buildContext's timestamp whisper uses `new Date()`.
  // Patch the zero-arg Date and Date.now to FIXED_NOW_MS; the Rust side injects
  // the same now_ms. (buildContext writes no persistent rows we diff, so a
  // frozen clock in the DB layer is harmless here.)
  const RealDate = Date;
  const FakeDate = class extends RealDate {
    constructor(...args: unknown[]) {
      if (args.length === 0) {
        super(FIXED_NOW_MS);
      } else {
        // @ts-expect-error variadic forwarding
        super(...args);
      }
    }
    static now(): number {
      return FIXED_NOW_MS;
    }
  } as DateConstructor;
  (global as { Date: DateConstructor }).Date = FakeDate;

  const lines: string[] = [];

  // A minimal connection profile that triggers the compression budget math.
  const connectionProfile = { maxContext: 8000, maxTokens: 1000, modelClass: null } as never;
  const cheapLLMSelection = {
    provider: 'ANTHROPIC',
    modelName: 'claude-cheap',
    baseUrl: null,
    connectionProfileId: null,
    isLocal: false,
  } as never;

  for (const op of spec.ops) {
    currentOp = op.name;
    const character = await repos.characters.findById(op.characterId);
    if (!character) throw new Error(`character ${op.characterId} not found`);
    const chat = await repos.chats.findById(spec.chat.id);
    if (!chat) throw new Error('chat not found');

    const timestampConfig = op.timestampMode
      ? {
          mode: op.timestampMode,
          format: 'ISO8601',
          autoPrepend: true,
          useFictionalTime: false,
        }
      : null;

    // Per-op chat metadata overrides (summary anchors / contextSummary).
    const effectiveChat = op.chatOverrides ? { ...chat, ...op.chatOverrides } : chat;

    const options: Record<string, unknown> = {
      provider: 'ANTHROPIC',
      modelName: 'claude-test',
      userId: spec.userId,
      character,
      userCharacter: { name: 'Charlie', description: 'The operator.' },
      chat: effectiveChat,
      existingMessages: buildExistingMessages(op),
      newUserMessage: op.newUserMessage,
      skipMemories: op.skipMemories,
      minMemoryImportance: 0.5,
      timestampConfig,
      isInitialMessage: false,
    };
    if (op.participants && op.respondingParticipantId && op.messagesWithParticipants) {
      const respondingParticipant = op.participants.find(
        (p) => p.id === op.respondingParticipantId
      );
      if (!respondingParticipant) throw new Error(`responding participant not in op ${op.name}`);
      const participantCharacters = new Map<string, unknown>();
      for (const p of op.participants) {
        const c = await repos.characters.findById(p.characterId);
        if (!c) throw new Error(`participant character ${p.characterId} not found`);
        participantCharacters.set(p.characterId, c);
      }
      options.respondingParticipant = respondingParticipant;
      options.allParticipants = op.participants;
      options.participantCharacters = participantCharacters;
      options.messagesWithParticipants = op.messagesWithParticipants;
      options.activeUserParticipantId = op.activeUserParticipantId ?? null;
    }
    if (op.compressionEnabled) {
      options.connectionProfile = connectionProfile;
      options.cheapLLMSelection = cheapLLMSelection;
      options.contextCompressionSettings = {
        enabled: true,
        windowSize: 5,
        contextCompressionThreshold: 0.5,
        systemPromptTargetTokens: 1000,
      };
      // W4.4a4: a warm async pre-compression cache. buildContext uses it verbatim
      // (no sync compression call → no canned completion consumed).
      if (op.cachedCompression) {
        options.cachedCompressionResult = {
          compressionApplied: true,
          compressedHistory: op.cachedCompression.compressedHistory,
          warnings: op.cachedCompression.warnings ?? [],
        };
        options.cachedCompressionMessageCount = op.cachedCompression.messageCount;
      }
    }

    const result = await buildContext(options as never);
    lines.push(JSON.stringify({ kind: 'result', op: op.name, result }));
  }

  for (const entry of cannedRecorded.values()) {
    lines.push(JSON.stringify({ kind: 'canned', ...entry }));
  }

  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`build-context oracle wrote ${outPath}\n`);
}

test('build-context tier-3 oracle', async () => {
  await main();
});
