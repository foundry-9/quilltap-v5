/**
 * Tier-3 (mocked-`streamMessage`) ORACLE for the initial-greeting core (P4.4
 * unit 2, sub-unit 5). Drives v4's REAL `generateGreetingMessage`
 * (lib/chat/initial-greeting.ts) with ONLY the streaming provider + `logLLMCall`
 * mocked — DB-free. Records the request `messages` v4 passes to `streamMessage`
 * (proving the augmented prompt bytes) and emits the `{content,
 * contentFilterDetected}` result. The Rust port
 * (services::initial_greeting::generate_greeting_message) registers a canned
 * stream keyed by the RECORDED messages and must reproduce the result exactly —
 * a prompt-byte divergence misses the canned key and surfaces.
 *
 * Runs under v4's JEST (not tsx) because `createLLMProvider` is a module export
 * only `jest.doMock` can replace. jest ignores `.claude/` worktree paths, so
 * stage a copy of this file + the fixture to /tmp.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-greeting-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/initial-greeting.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/initial-greeting.json"  "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-greeting.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- initial-greeting
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import type { LLMStreamStalledError as LLMStreamStalledErrorType } from '@/lib/llm/stream-watchdog';

/**
 * P4.D190 — the class the CURRENT module registry holds. The `it()` below
 * calls `jest.resetModules()` before importing `generateGreetingMessage`, so
 * the watchdog it wraps the stream with is a FRESH copy of
 * `@/lib/llm/stream-watchdog`; a class captured by a top-level `import` here
 * is from the previous generation and `instanceof` against it is false, which
 * would silently take the watchdog's non-stall branch.
 */
let StalledError: typeof LLMStreamStalledErrorType;

interface CaseSpec {
  id: string;
  systemPrompt: string;
  characterName: string;
  provider: string;
  model: string;
  temperature: number | null;
  memories: Array<{ aboutCharacterName: string; summary: string }>;
  project: { name: string; description?: string | null; instructions?: string | null } | null;
  recentConversationsBlock: string | null;
  cannedContent: string;
  cannedUsage: { promptTokens: number; completionTokens: number; totalTokens: number } | null;
  /**
   * P4.D79: the CUMULATIVE `reasoningContent` values a thinking model emits —
   * each chunk carries the full thinking-so-far, not a delta, which is why v4
   * ASSIGNS rather than concatenates. Emitted as its own chunks before the
   * content, the way the real providers interleave them.
   */
  cannedReasoning?: string[];
  /**
   * P4.D190 (§C.4): pose a stalled provider. The mock throws v4's REAL
   * `LLMStreamStalledError` from inside the generator, so `withStallWatchdog`
   * sees it as its own class (sets `stalled`, logs, rethrows) and
   * `generateGreetingMessage` logs it onto the `llm_logs` row and rethrows —
   * the whole path a silent provider actually takes. `chunksReceived` content
   * chunks are yielded first, so the count on the error agrees with what the
   * consumer saw.
   */
  stall?: { budgetMs: number; chunksReceived: number };
  /**
   * P4.D190 (§C.4): pose an ORDINARY provider failure — a plain `Error`, which
   * the ladder must NOT read as a silence. The arm that proves the port does
   * not over-classify.
   */
  error?: string;
  /**
   * P4.D324 (v4 `4fbdad186`): the sections that ride after the instruction
   * in the user message (a whitespace-only one is dropped), and the request's
   * `chatId` (absent → `undefined`, which v4's logger omits).
   */
  trailingContextSections?: string[];
  chatId?: string;
  /**
   * P4.D348 (v4 `eb2d87788`): a terminal `done` chunk carrying the
   * `rawResponse` the row's `finishReason` is read from, plus `cacheUsage` /
   * `rawProviderUsage`. Absent → the stream ends without `done`, so the row
   * logs `finishReason: null` (the silence leg).
   */
  cannedDone?: { rawResponse?: unknown; cacheUsage?: unknown; rawProviderUsage?: unknown };
}

/**
 * P4.D348: the `llm_logs` row v4 writes, as the `logLLMCall` argument
 * projected onto the cells `eb2d87788` touched (and their neighbours). The
 * mock replaces only the write; the arguments are v4's own.
 */
function projectLogCall(p: Record<string, any>) {
  return {
    type: p.type,
    chatId: p.chatId ?? null,
    characterId: p.characterId ?? null,
    provider: p.provider,
    modelName: p.modelName,
    connectionProfileId: p.connectionProfileId ?? null,
    content: p.response?.content ?? null,
    finishReason: p.response?.finishReason ?? null,
    error: p.response?.error ?? null,
    usage: p.usage ?? null,
    cacheUsage: p.cacheUsage ?? null,
    rawProviderUsage: p.rawProviderUsage ?? null,
  };
}
const logLLMCallMock = jest.fn(async (..._args: unknown[]) => undefined);

const here = dirname(fileURLToPath(import.meta.url));
const spec = JSON.parse(
  fs.readFileSync(join(here, '..', 'fixtures', 'initial-greeting.json'), 'utf8'),
) as { cases: CaseSpec[] };

let currentCase: CaseSpec | null = null;
const recorded = new Map<string, unknown>();

it('generates greetings (records prompt bytes + result)', async () => {
  jest.resetModules();
  // Same registry generation as `generateGreetingMessage`'s own import.
  StalledError = (await import('@/lib/llm/stream-watchdog')).LLMStreamStalledError;

  jest.doMock('@/lib/services/llm-logging.service', () => {
    const actual = jest.requireActual('@/lib/services/llm-logging.service');
    return { __esModule: true, ...actual, logLLMCall: logLLMCallMock };
  });

  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (provider: string, _baseUrl?: string) => ({
        streamMessage: async function* streamMessage(
          params: { messages: Array<{ role: string; content: string }>; model: string; temperature?: number },
          _apiKey: string,
        ) {
          const c = currentCase;
          if (!c) throw new Error('no current case');
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
          const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!recorded.has(c.id)) {
            recorded.set(c.id, { provider, model: params.model, temperature: params.temperature ?? null, messages });
          }
          void key;
          for (const r of c.cannedReasoning ?? []) yield { reasoningContent: r };
          if (c.stall) {
            // The chunks the consumer had already seen before the silence, so
            // the count carried on the error is the count it observed.
            for (let i = 0; i < c.stall.chunksReceived; i++) yield { content: `chunk-${i} ` };
            throw new StalledError(
              c.stall.budgetMs,
              c.stall.chunksReceived,
              provider,
              params.model,
            );
          }
          if (c.error) throw new Error(c.error);
          if (c.cannedContent) yield { content: c.cannedContent };
          if (c.cannedUsage) yield { usage: c.cannedUsage };
          if (c.cannedDone) yield { done: true, ...c.cannedDone };
        },
      }),
    };
  });

  const { generateGreetingMessage } = await import('@/lib/chat/initial-greeting');
  // P4.D324: the trailing-sections DEBUG, captured off the SAME registry
  // generation's logger.
  const { logger } = await import('@/lib/logger');
  let greetingDebug: Array<{ message: string; context: unknown }> = [];
  const realDebug = logger.debug.bind(logger);
  jest.spyOn(logger, 'debug').mockImplementation(((message: string, context?: unknown) => {
    if (message.startsWith('[Greeting Generation] Appending trailing context sections')) {
      greetingDebug.push({ message, context: context ?? null });
    }
    return realDebug(message, context as never);
  }) as never);

  const lines: string[] = [];
  for (const c of spec.cases) {
    currentCase = c;
    greetingDebug = [];
    logLLMCallMock.mockClear();
    // P4.D190: v4 rethrows a stream failure out of `generateGreetingMessage`
    // (after logging it), so a case can REJECT. Record the rejection's `name`
    // — the byte v4's ladder reads with `instanceof` and v5 reads as
    // `StreamError::v4_name()` — beside its message.
    let result: Awaited<ReturnType<typeof generateGreetingMessage>> | null = null;
    let error: { name: string; message: string } | null = null;
    try {
      result = await generateGreetingMessage({
        systemPrompt: c.systemPrompt,
        characterName: c.characterName,
        provider: c.provider,
        modelName: c.model,
        apiKey: 'test-key',
        temperature: c.temperature ?? undefined,
        participantMemories: c.memories.length > 0 ? c.memories : undefined,
        projectContext: c.project,
        recentConversationsBlock: c.recentConversationsBlock ?? undefined,
        characterId: 'char-1',
        // P4.D348: a user (so the row is written) and the route's profile id.
        userId: 'user-1',
        connectionProfileId: 'profile-1',
        ...(c.chatId ? { chatId: c.chatId } : {}),
        ...(c.trailingContextSections ? { trailingContextSections: c.trailingContextSections } : {}),
      });
    } catch (err) {
      error = {
        name: err instanceof Error ? err.name : 'Error',
        message: err instanceof Error ? err.message : String(err),
      };
    }
    lines.push(
      JSON.stringify({
        id: c.id,
        request: recorded.get(c.id),
        result: result
          ? {
              content: result.content,
              reasoningContent: result.reasoningContent,
              contentFilterDetected: result.contentFilterDetected,
            }
          : null,
        error,
        greetingDebug,
        logCalls: logLLMCallMock.mock.calls.map((call) => projectLogCall(call[0] as Record<string, any>)),
      }),
    );
  }

  const out = process.env.QT_ORACLE_OUT;
  if (!out) throw new Error('QT_ORACLE_OUT must be set');
  fs.writeFileSync(out, lines.join('\n') + '\n');
  expect(lines.length).toBe(spec.cases.length);
});
