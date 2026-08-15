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
}

const here = dirname(fileURLToPath(import.meta.url));
const spec = JSON.parse(
  fs.readFileSync(join(here, '..', 'fixtures', 'initial-greeting.json'), 'utf8'),
) as { cases: CaseSpec[] };

let currentCase: CaseSpec | null = null;
const recorded = new Map<string, unknown>();

it('generates greetings (records prompt bytes + result)', async () => {
  jest.resetModules();

  jest.doMock('@/lib/services/llm-logging.service', () => {
    const actual = jest.requireActual('@/lib/services/llm-logging.service');
    return { __esModule: true, ...actual, logLLMCall: jest.fn(async () => undefined) };
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
          if (c.cannedContent) yield { content: c.cannedContent };
          if (c.cannedUsage) yield { usage: c.cannedUsage };
        },
      }),
    };
  });

  const { generateGreetingMessage } = await import('@/lib/chat/initial-greeting');

  const lines: string[] = [];
  for (const c of spec.cases) {
    currentCase = c;
    const result = await generateGreetingMessage({
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
    });
    lines.push(
      JSON.stringify({
        id: c.id,
        request: recorded.get(c.id),
        result: {
          content: result.content,
          reasoningContent: result.reasoningContent,
          contentFilterDetected: result.contentFilterDetected,
        },
      }),
    );
  }

  const out = process.env.QT_ORACLE_OUT;
  if (!out) throw new Error('QT_ORACLE_OUT must be set');
  fs.writeFileSync(out, lines.join('\n') + '\n');
  expect(lines.length).toBe(spec.cases.length);
});
