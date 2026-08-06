/**
 * @jest-environment node
 *
 * Tier-3 (mocked-SDK) ORACLE for the OpenAI conversation-chaining FALLBACK
 * (dogfood finding #69). Drives v4's REAL `OpenAIProvider.streamMessage`
 * (`plugins/dist/qtap-plugin-openai/provider.ts:511-614`) with ONLY the `openai`
 * SDK's `responses.create` mocked to throw ONCE (the chained attempt) then return
 * a streaming async-iterable (the full-input attempt) — so v4's real
 * `try/catch` → "conversation chaining failed, falling back to full input"
 * fallback runs end to end.
 *
 * Why a SEPARATE oracle (not the `primary-stream-tier3` family): that family
 * mocks `createLLMProvider().streamMessage` wholesale, ABOVE the provider — so
 * v4's fallback code never runs there. The fallback lives INSIDE the provider,
 * so exercising it needs the REAL provider with the SDK mocked BELOW it. This
 * oracle instantiates `OpenAIProvider` directly (a default-constructed
 * `TextProvider`), bypassing the plugin registry.
 *
 * The Rust side (`primary_stream_tier3_equivalence.rs`
 * `openai_chaining_fallback_tier3_matches_oracle`) drives the REAL
 * `WireStreamingProvider::stream_message("OPENAI", …)` with a fail-then-succeed
 * transport serving the SAME Responses-API SSE on the second call, and diffs:
 *   - the recovered CHUNK SEQUENCE (content / done / usage), and
 *   - the fallback fingerprint (two provider attempts, the first chained, the
 *     retry NOT chained).
 * The byte proof that the retry drops `previous_response_id` and carries the full
 * input lives in the unit + wire tests (`streaming_provider.rs` tests +
 * `tool_wire_call_site.rs::chaining_fallback_retry_bytes_match_a_nonchained_build`);
 * this tier-3 proves turn-level outcome parity against v4's REAL fallback.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5=~/source/quilltap-v5
 *   TMPO=/tmp/qt-openai-fallback-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5/harness/oracle/cases/openai-chaining-fallback-tier3.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-openai-fallback.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/cases" -- openai-chaining-fallback-tier3
 * Run the Rust diff:
 *   QT_ORACLE_OPENAI_FALLBACK=/tmp/oracle-openai-fallback.ndjson \
 *     cargo test -p quilltap-harness --test primary_stream_tier3_equivalence \
 *       openai_chaining_fallback_tier3_matches_oracle -- --nocapture
 */

import * as fs from 'fs';
import { join } from 'node:path';

interface RawUsage {
  promptTokens?: number;
  completionTokens?: number;
  totalTokens?: number;
}
interface RawChunk {
  content?: string;
  done?: boolean;
  usage?: RawUsage;
}

// The Responses-API `response.completed` usage the mocked SDK reports on the
// full-input (recovered) attempt — the numbers the Rust SSE fixture mirrors.
const USAGE = { input_tokens: 10, output_tokens: 2, total_tokens: 12 };

// The conversation: a system turn (→ instructions), then a multi-turn exchange —
// so the chained attempt sends only the last user message and the full-input
// retry sends the whole thing (asserted by the wire test; here it just makes the
// two attempts non-trivially different).
const MESSAGES = [
  { role: 'system', content: 'You are Byron.' },
  { role: 'user', content: 'Roll a die.' },
  { role: 'assistant', content: 'I rolled a 4.' },
  { role: 'user', content: 'Roll again.' },
];

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  let createCount = 0;
  const seen: Array<Record<string, unknown>> = [];

  jest.resetModules();
  jest.doMock('openai', () => {
    class MockOpenAI {
      responses = {
        create: async (params: Record<string, unknown>) => {
          createCount += 1;
          seen.push(params);
          if (createCount === 1) {
            // The chained attempt: OpenAI cannot resolve `previous_response_id`
            // (routine on `store: false` responses). v4 catches this pre-stream.
            throw new Error('400 previous_response_not_found');
          }
          // The full-input retry succeeds: a Responses-API event stream.
          async function* gen() {
            yield { type: 'response.output_text.delta', delta: 'Hi' };
            yield {
              type: 'response.completed',
              response: {
                id: 'r1',
                model: 'gpt-4o',
                output_text: 'Hi',
                output: [],
                usage: USAGE,
              },
            };
          }
          return gen();
        },
      };
    }
    return { __esModule: true, default: MockOpenAI };
  });

  const providerPath = join(process.cwd(), 'plugins/dist/qtap-plugin-openai/provider');
  const { OpenAIProvider } = (await import(providerPath)) as {
    OpenAIProvider: new () => {
      streamMessage: (params: unknown, apiKey: string) => AsyncGenerator<RawChunk>;
    };
  };
  const provider = new OpenAIProvider();

  const params = {
    messages: MESSAGES,
    model: 'gpt-4o',
    temperature: 0.7,
    maxTokens: 1024,
    previousResponseId: 'resp_dead',
  };

  // Collect the recovered chunk sequence, normalized to (content, done, usage).
  const chunks: Array<{ content: string; done: boolean; usage: RawUsage | null }> = [];
  for await (const chunk of provider.streamMessage(params, 'test-key')) {
    chunks.push({
      content: chunk.content ?? '',
      done: chunk.done ?? false,
      usage: chunk.usage
        ? {
            promptTokens: chunk.usage.promptTokens ?? 0,
            completionTokens: chunk.usage.completionTokens ?? 0,
            totalTokens: chunk.usage.totalTokens ?? 0,
          }
        : null,
    });
  }

  const line = JSON.stringify({
    kind: 'fallback',
    sdkCreateCount: createCount,
    firstChained: seen[0]?.previous_response_id === 'resp_dead',
    secondChained: seen[1]?.previous_response_id !== undefined,
    chunks,
  });
  fs.writeFileSync(outPath, line + '\n');
  process.stderr.write(`openai chaining-fallback oracle wrote ${outPath}\n`);
}

test('openai chaining-fallback tier-3 oracle', async () => {
  await main();
});
