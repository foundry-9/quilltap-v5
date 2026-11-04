/**
 * @jest-environment node
 *
 * P4.D356 (bug 269, v4 `4b0d1cb04`) `nanogpt_image_provider_equivalence`
 * ORACLE — drives v4's REAL `NanoGPTImageProvider.generateImage`
 * (`plugins/dist/qtap-plugin-nanogpt/image-provider.ts`) with the OpenAI SDK
 * and `global.fetch` scripted exactly as v4's own test does
 * (`__tests__/unit/plugins/nanogpt-image-sizes.test.ts:95-237`), recording per
 * case:
 *
 *   - `calls`   — every params object handed to `client.images.generate`,
 *                 SNAPSHOTTED at call time (v4's retry mutates the same object
 *                 in place, so a recorded reference would show the square);
 *   - `fetched` — every `fetch` URL during the generate (the catalog GET);
 *   - `outcome` — `{ok: true, images}` or `{error: <message>}`;
 *   - `lines`   — every line the plugin logger took: `{level, message, keys,
 *                 error?}` (key NAMES in v4's order, a key passed as
 *                 `undefined` omitted — the values carry prompts / keys; only
 *                 the `error` text rides, which is the provider's own words).
 *
 * The catalog cache is MODULE state: every case runs in `jest.isolateModules`
 * (a cold module), and a `warm` case fills the cache first through the
 * plugin's own listing (`getAvailableModels`) before recording starts — v4's
 * test reaches the same warm state by ordering its blocks.
 *
 * **The error framing.** The SDK is mocked, so a scripted failure is v4's
 * test's own `Object.assign(new Error(`${status} ${text}`), {status})`; the
 * Rust side posts the same failure as a NON-JSON wire body `text` with
 * `status`, which v5's SDK reconstruction renders `${status} ${text}` (the
 * SDK's `makeMessage` rule for a body that does not parse, proven by the
 * `image-dialects` corpus). A scripted network throw has no status on either
 * side.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-nanogpt-image-provider-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/nanogpt-image-provider.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-nanogpt-image-provider.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "nanogpt-image-provider\.test\.ts$"
 */

import * as fs from 'fs';

type Line = { level: string; message: string; keys: string[]; error?: unknown };
let lines: Line[] = [];
let recording = false;

jest.mock('@quilltap/plugin-utils', () => {
  const actual = jest.requireActual('@quilltap/plugin-utils');
  const make = (): Record<string, unknown> => {
    const log = (level: string) => (message: string, meta?: Record<string, unknown>) => {
      if (!recording) return;
      const keys = Object.keys(meta ?? {}).filter((k) => meta![k] !== undefined);
      lines.push({ level, message, keys, ...(meta && 'error' in meta ? { error: meta.error } : {}) });
    };
    const logger: Record<string, unknown> = {
      debug: log('debug'),
      info: log('info'),
      warn: log('warn'),
      error: log('error'),
    };
    logger.child = () => logger;
    return logger;
  };
  return { ...actual, createPluginLogger: () => make() };
});

type Failure = { status?: number; text: string };
type Outcome = 'ok' | Failure;
let generateScript: Outcome[] = [];
let calls: unknown[] = [];

jest.mock('openai', () => ({
  __esModule: true,
  default: jest.fn().mockImplementation(() => ({
    images: {
      generate: async (params: unknown) => {
        calls.push(JSON.parse(JSON.stringify(params)));
        const next = generateScript.shift();
        if (next === undefined) throw new Error('oracle: generate called past the script');
        if (next === 'ok') return { data: [{ b64_json: 'aGVsbG8=' }] };
        throw next.status === undefined
          ? new Error(next.text)
          : Object.assign(new Error(`${next.status} ${next.text}`), { status: next.status });
      },
    },
    models: { list: jest.fn() },
  })),
}));

/** The catalog GET's scripted answer: an HTTP response, or a network throw. */
type CatalogAnswer = { status: number; text: string } | { throws: string };

const QWEN3 = {
  resolutions: ['1k', '2k'],
  aspect_ratio: ['1:1', '3:2', '16:9', '21:9', '9:16', '2:3', '3:4', '4:5'],
};
const STAR6 = { resolutions: ['1024*1024', '1280*720', '720*1280', '1536*1024', '1024*1536', 'auto'] };
const CATALOG = JSON.stringify({
  data: [
    { id: 'qwen-image-3-uncensored', capabilities: { image_generation: true }, supported_parameters: QWEN3 },
    { id: 'star-six-model', capabilities: { image_generation: true }, supported_parameters: STAR6 },
    { id: 'listed-no-params', capabilities: { image_generation: true }, tags: ['lora'] },
  ],
});

interface Case {
  id: string;
  /** Warm the cache with this listing body before recording; else cold. */
  warm?: string;
  /** The catalog GET's answer when the cache is cold. */
  catalog?: CatalogAnswer;
  params: Record<string, unknown>;
  generate: Outcome[];
}

const P = (extra: Record<string, unknown>) => ({ prompt: 'a lighthouse keeper', model: 'qwen-image-3-uncensored', ...extra });
const UNAVAILABLE: Failure = { status: 503, text: 'Service is temporarily unavailable.' };

const CASES: Case[] = [
  // Cold: the catalog fetch, every way it can answer.
  { id: 'cold-fetch-503', catalog: { status: 503, text: 'busy' }, params: P({ size: '832x1248' }), generate: ['ok'] },
  { id: 'cold-fetch-ok', catalog: { status: 200, text: CATALOG }, params: P({ size: '832x1248' }), generate: ['ok'] },
  { id: 'cold-fetch-throw', catalog: { throws: 'fetch failed' }, params: P({ size: '832x1248' }), generate: ['ok'] },
  { id: 'cold-fetch-not-json', catalog: { status: 200, text: '<html>busy</html>' }, params: P({ size: '832x1248' }), generate: ['ok'] },
  { id: 'cold-fetch-null', catalog: { status: 200, text: 'null' }, params: P({ size: '832x1248' }), generate: ['ok'] },
  { id: 'cold-fetch-data-not-array', catalog: { status: 200, text: '{"data":"nope"}' }, params: P({ size: '832x1248' }), generate: ['ok'] },
  { id: 'cold-fetch-ok-then-503-retry', catalog: { status: 200, text: CATALOG }, params: P({ size: '832x1248' }), generate: [UNAVAILABLE, 'ok'] },
  // Warm: the fit and the retry (v4's quintet + the edges).
  { id: 'warm-portrait-fits', warm: CATALOG, params: P({ size: '832x1248' }), generate: ['ok'] },
  { id: 'warm-503-retries-square', warm: CATALOG, params: P({ size: '832x1248' }), generate: [UNAVAILABLE, 'ok'] },
  { id: 'warm-uncatalogued-size-400', warm: CATALOG, params: P({ model: 'uncatalogued-model', size: '832x1248' }), generate: [{ status: 400, text: 'Model X supports 1K and 2K output.' }, 'ok'] },
  { id: 'warm-400-not-shape', warm: CATALOG, params: P({ model: 'uncatalogued-model', size: '832x1248' }), generate: [{ status: 400, text: 'try a different prompt or image' }] },
  { id: 'warm-already-square-503', warm: CATALOG, params: P({ size: '1024x1024' }), generate: [UNAVAILABLE] },
  { id: 'warm-both-503', warm: CATALOG, params: P({ size: '832x1248' }), generate: [UNAVAILABLE, { status: 503, text: 'still unavailable' }] },
  { id: 'warm-concrete-400-retries-square', warm: CATALOG, params: P({ model: 'star-six-model', size: '832x1248' }), generate: [{ status: 400, text: 'unsupported output size' }, 'ok'] },
  { id: 'warm-network-throw-no-retry', warm: CATALOG, params: P({ size: '832x1248' }), generate: [{ text: 'Connection error.' }] },
  { id: 'warm-500-no-retry', warm: CATALOG, params: P({ size: '832x1248' }), generate: [{ status: 500, text: 'size mismatch' }] },
  { id: 'warm-ratio-only', warm: CATALOG, params: P({ aspectRatio: '9:16' }), generate: ['ok'] },
  { id: 'warm-listed-size-passes', warm: CATALOG, params: P({ size: '2k', aspectRatio: '16:9' }), generate: ['ok'] },
  { id: 'warm-entry-without-params', warm: CATALOG, params: P({ model: 'listed-no-params', size: '832x1248' }), generate: [{ status: 400, text: 'bad resolution' }, 'ok'] },
  { id: 'warm-key-order', warm: CATALOG, params: P({ size: '1248x832', seed: 7, guidanceScale: 3.5, steps: 20, negativePrompt: 'blur' }), generate: ['ok'] },
  { id: 'warm-key-order-retry', warm: CATALOG, params: P({ size: '1248x832', seed: 7, negativePrompt: 'blur' }), generate: [UNAVAILABLE, 'ok'] },
  { id: 'warm-no-size', warm: CATALOG, params: P({}), generate: ['ok'] },
  { id: 'warm-default-model', warm: CATALOG, params: { prompt: 'a lighthouse keeper', size: '832x1248' }, generate: ['ok'] },
];

let fetched: string[] = [];

function installFetch(answer: CatalogAnswer | undefined) {
  // @ts-expect-error override
  global.fetch = jest.fn(async (url: string) => {
    if (recording) fetched.push(String(url));
    if (!answer) throw new Error(`oracle: unscripted fetch ${url}`);
    if ('throws' in answer) throw new TypeError(answer.throws);
    return {
      ok: answer.status >= 200 && answer.status < 300,
      status: answer.status,
      json: async () => JSON.parse(answer.text),
    };
  });
}

const out: string[] = [];

describe('nanogpt image provider oracle', () => {
  const originalFetch = global.fetch;
  afterAll(() => {
    global.fetch = originalFetch;
    fs.writeFileSync(process.env.QT_ORACLE_OUT || '/tmp/oracle-nanogpt-image-provider.ndjson', out.join('\n') + '\n');
  });

  for (const c of CASES) {
    it(c.id, async () => {
      let Provider: new () => {
        generateImage: (p: unknown, k: string) => Promise<{ images: unknown[] }>;
        getAvailableModels: (k?: string) => Promise<string[]>;
      };
      jest.isolateModules(() => {
        Provider = require('@/plugins/dist/qtap-plugin-nanogpt/image-provider').NanoGPTImageProvider;
      });
      lines = [];
      calls = [];
      fetched = [];
      recording = false;
      if (c.warm) {
        installFetch({ status: 200, text: c.warm });
        await new Provider!().getAvailableModels('test-key');
      }
      installFetch(c.catalog);
      generateScript = [...c.generate];
      recording = true;
      let outcome: Record<string, unknown>;
      try {
        const res = await new Provider!().generateImage(c.params, 'test-key');
        outcome = { ok: true, images: res.images.length };
      } catch (e) {
        outcome = { error: e instanceof Error ? e.message : String(e) };
      }
      recording = false;
      out.push(JSON.stringify({ id: c.id, warm: c.warm ?? null, catalog: c.catalog ?? null, params: c.params, generate: c.generate, calls, fetched, outcome, lines }));
    });
  }
});
