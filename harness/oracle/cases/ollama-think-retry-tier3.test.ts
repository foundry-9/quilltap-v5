/**
 * @jest-environment node
 *
 * Tier-3 (mocked-fetch) ORACLE for the Ollama retry-without-`think` salvage
 * (P4.D78, v4 `d9c5a1c7`). Drives v4's REAL `OllamaProvider.streamMessage` AND
 * `OllamaProvider.sendMessage` (`plugins/dist/qtap-plugin-ollama/provider.ts`)
 * with ONLY `global.fetch` mocked below it — so v4's own `!response.ok` →
 * `isThinkRejection` → `delete requestBody.think` → re-send path runs end to
 * end, on both methods.
 *
 * Why a SEPARATE oracle (not the request-envelope or stream-decoder corpora):
 * those record ONE fetch of a successful call. The salvage is a SECOND fetch
 * whose body differs from the first by one deleted key, so it can only be
 * observed by a mock that fails the first attempt — the same shape as the
 * `openai-chaining-fallback-tier3` precedent.
 *
 * Four arms per method, matching the Rust composition quartet:
 *   1. rejected-then-succeeded (the retry drops `think`, the answer arrives);
 *   2. rejected twice (v4 throws with the SECOND attempt's text);
 *   3. a think-UNRELATED error (no retry at all);
 *   4. the `think: false` default body STILL retries — v4's guard is
 *      `'think' in requestBody`, key presence, not truthiness. (Arm 1 already
 *      sends `think: false`, so this is recorded as part of arm 1's
 *      fingerprint rather than a fifth case.)
 *
 * Run from the v4 server checkout under Node 24 (jest is needed for the module
 * mock; the /tmp mirror is because jest ignores `.claude/` paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5=~/source/quilltap-v5
 *   TMPO=/tmp/qt-ollama-think-retry-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5/harness/oracle/cases/ollama-think-retry-tier3.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-ollama-think-retry.ndjson $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/cases" -- "ollama-think-retry-tier3\.test\.ts$"
 * Run the Rust diff:
 *   QT_ORACLE_OLLAMA_THINK_RETRY=/tmp/oracle-ollama-think-retry.ndjson cargo test -p quilltap-harness --test ollama_think_retry_tier3_equivalence
 */

import * as fs from 'fs';
import { join } from 'node:path';

interface Attempt {
  /** Whether the request body still carried the `think` key. */
  hasThink: boolean;
  /** The value of `think`, when present. */
  think: unknown;
}

interface Arm {
  arm: string;
  method: 'stream' | 'send';
  attempts: Attempt[];
  /** Concatenated visible content (streaming) or `response.content` (send). */
  content: string | null;
  /** The thrown message, when v4 failed. */
  error: string | null;
}

const OK_STREAM_LINES = [
  { model: 'qwen3:8b', message: { role: 'assistant', content: 'ok' }, done: false },
  {
    model: 'qwen3:8b',
    message: { role: 'assistant', content: '' },
    done: true,
    prompt_eval_count: 1,
    eval_count: 1,
  },
];

const OK_SEND_BODY = {
  model: 'qwen3:8b',
  message: { role: 'assistant', content: 'ok' },
  done: true,
  prompt_eval_count: 1,
  eval_count: 1,
};

const THINK_ERROR = '{"error":"\\"qwen3:8b\\" does not support disabling thinking"}';
const OTHER_ERROR = '{"error":"model \\"nope\\" not found"}';

function okStreamResponse() {
  const body = OK_STREAM_LINES.map((l) => JSON.stringify(l)).join('\n') + '\n';
  const bytes = new TextEncoder().encode(body);
  let served = false;
  return {
    ok: true,
    status: 200,
    body: {
      getReader: () => ({
        read: async () =>
          served ? { done: true, value: undefined } : ((served = true), { done: false, value: bytes }),
        releaseLock: () => {},
      }),
    },
    text: async () => '',
  };
}

function okSendResponse() {
  return { ok: true, status: 200, json: async () => OK_SEND_BODY, text: async () => '' };
}

function failure(status: number, text: string) {
  return { ok: false, status, text: async () => text };
}

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const providerPath = join(process.cwd(), 'plugins/dist/qtap-plugin-ollama/provider');
  const { OllamaProvider } = (await import(providerPath)) as {
    OllamaProvider: new (baseUrl: string) => {
      streamMessage: (params: unknown, apiKey: string) => AsyncGenerator<{ content?: string }>;
      sendMessage: (params: unknown, apiKey: string) => Promise<{ content: string }>;
    };
  };

  const params = {
    model: 'qwen3:8b',
    messages: [{ role: 'user', content: 'hi' }],
    temperature: 0.7,
    maxTokens: 1024,
  };

  const rows: Arm[] = [];
  const original = global.fetch;

  /** Run one arm with a queue of canned responses; record every request body. */
  async function run(
    arm: string,
    method: 'stream' | 'send',
    queue: Array<() => unknown>
  ): Promise<void> {
    const attempts: Attempt[] = [];
    let i = 0;
    global.fetch = (async (_url: string, init: { body: string }) => {
      const body = JSON.parse(init.body);
      attempts.push({ hasThink: 'think' in body, think: body.think ?? null });
      const next = queue[Math.min(i, queue.length - 1)];
      i += 1;
      return next();
    }) as unknown as typeof fetch;

    const provider = new OllamaProvider('http://localhost:11434');
    let content: string | null = null;
    let error: string | null = null;
    try {
      if (method === 'stream') {
        let text = '';
        for await (const chunk of provider.streamMessage(params, '')) text += chunk.content ?? '';
        content = text;
      } else {
        content = (await provider.sendMessage(params, '')).content;
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      global.fetch = original;
    }
    rows.push({ arm, method, attempts, content, error });
  }

  for (const method of ['stream', 'send'] as const) {
    const ok = method === 'stream' ? okStreamResponse : okSendResponse;
    await run('rejected-then-succeeded', method, [() => failure(400, THINK_ERROR), ok]);
    await run('rejected-twice', method, [
      () => failure(400, THINK_ERROR),
      () => failure(500, 'still thinking about it'),
    ]);
    await run('non-think-error', method, [() => failure(404, OTHER_ERROR), ok]);
  }

  fs.writeFileSync(outPath, rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
  process.stderr.write(`ollama think-retry oracle wrote ${rows.length} row(s) → ${outPath}\n`);
}

test('ollama think-retry tier-3 oracle', async () => {
  await main();
});
