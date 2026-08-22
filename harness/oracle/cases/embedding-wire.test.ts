/**
 * @jest-environment node
 *
 * W4.7e `embedding_wire_equivalence` ORACLE — drives v4's REAL plugin embedding
 * providers (`plugins/dist/qtap-plugin-{openai,ollama,openrouter,nanogpt}/
 * embedding-provider.ts`) with `global.fetch` / the `@openrouter/sdk` mocked to
 * committed payloads, recording each provider's built request(s) (method/url/body)
 * and the parsed result (or thrown error message). The Rust `model::embedding_wire`
 * builds the same requests + parses the same bodies.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-embedding-wire.ndjson \
 *     $N/npx jest --silent --watchman=false \
 *       --roots "$PWD" --roots "$V5/harness/oracle/cases" -- embedding-wire
 */

import * as fs from 'fs';

// Per-case fetch script (keyed by URL suffix -> {status, body}); the SDK response.
type FetchResult = { status: number; body: unknown };
let fetchScript: Record<string, FetchResult> = {};
let recorded: Array<{ method: string; url: string; body: unknown }> = [];
let sdkResponse: unknown = null;
let sdkRequestBody: unknown = null;

jest.mock('@openrouter/sdk', () => ({
  __esModule: true,
  OpenRouter: class {
    embeddings = {
      generate: async (args: { requestBody: unknown }) => {
        sdkRequestBody = args.requestBody;
        if (sdkResponse instanceof Error) throw sdkResponse;
        return sdkResponse;
      },
    };
  },
}));

function installFetch() {
  recorded = [];
  // @ts-expect-error override
  global.fetch = jest.fn(async (url: string, init: { method?: string; body?: string }) => {
    const u = String(url);
    const body = init?.body ? JSON.parse(init.body) : undefined;
    recorded.push({ method: init?.method || 'GET', url: u, body });
    const key = Object.keys(fetchScript).find((suffix) => u.endsWith(suffix));
    const res = key ? fetchScript[key] : { status: 404, body: {} };
    return {
      ok: res.status >= 200 && res.status < 300,
      status: res.status,
      statusText: res.status === 404 ? 'Not Found' : res.status === 500 ? 'Internal Server Error' : 'OK',
      json: async () => res.body,
    };
  });
}

async function driveOpenAI(c: Case) {
  const { OpenAIEmbeddingProvider } = await import(
    '@/plugins/dist/qtap-plugin-openai/embedding-provider'
  );
  const p = new OpenAIEmbeddingProvider(c.baseUrl || undefined);
  try {
    const r = await p.generateEmbedding(c.input, c.model, c.apiKey || 'sk-x', c.dimensions ? { dimensions: c.dimensions } : undefined);
    return { result: r };
  } catch (e) {
    return { error: e instanceof Error ? e.message : String(e) };
  }
}

async function driveOllama(c: Case) {
  const { OllamaEmbeddingProvider } = await import(
    '@/plugins/dist/qtap-plugin-ollama/embedding-provider'
  );
  const p = new OllamaEmbeddingProvider(c.baseUrl || undefined);
  try {
    const r = await p.generateEmbedding(c.input, c.model, '', undefined);
    return { result: r };
  } catch (e) {
    return { error: e instanceof Error ? e.message : String(e) };
  }
}

async function driveOpenRouter(c: Case) {
  const { OpenRouterEmbeddingProvider } = await import(
    '@/plugins/dist/qtap-plugin-openrouter/embedding-provider'
  );
  const p = new OpenRouterEmbeddingProvider();
  try {
    const r = await p.generateEmbedding(c.input, c.model, c.apiKey || 'sk-or', c.dimensions ? { dimensions: c.dimensions } : undefined);
    return { result: r };
  } catch (e) {
    return { error: e instanceof Error ? e.message : String(e) };
  }
}

async function driveNanoGPT(c: Case) {
  const { NanoGPTEmbeddingProvider } = await import(
    '@/plugins/dist/qtap-plugin-nanogpt/embedding-provider'
  );
  const p = new NanoGPTEmbeddingProvider(c.baseUrl || undefined);
  try {
    const r = await p.generateEmbedding(c.input, c.model, c.apiKey || 'ng-x', c.dimensions ? { dimensions: c.dimensions } : undefined);
    return { result: r };
  } catch (e) {
    return { error: e instanceof Error ? e.message : String(e) };
  }
}

type Case = {
  id: string;
  kind: 'openai' | 'ollama' | 'openrouter' | 'nanogpt';
  baseUrl?: string;
  model: string;
  input: string;
  dimensions?: number;
  apiKey?: string;
  fetchScript?: Record<string, FetchResult>;
  sdkResponse?: unknown; // for openrouter (array/base64/string/Error-marker)
};

// A base64 of two LE f32s (1.5, -2.25), to bank the decode path.
const f32Bytes = Buffer.from(new Float32Array([1.5, -2.25]).buffer).toString('base64');

const cases: Case[] = [
  {
    id: 'openai-basic',
    kind: 'openai',
    model: 'text-embedding-3-small',
    input: 'hello world',
    fetchScript: { '/embeddings': { status: 200, body: { data: [{ embedding: [0.1, 0.2, 0.3] }], usage: { prompt_tokens: 2, total_tokens: 2 } } } },
  },
  {
    id: 'openai-dimensions',
    kind: 'openai',
    baseUrl: 'https://custom/v1',
    model: 'text-embedding-3-large',
    input: 'hi',
    dimensions: 256,
    fetchScript: { '/embeddings': { status: 200, body: { data: [{ embedding: [1, 2] }] } } },
  },
  {
    id: 'openai-error',
    kind: 'openai',
    model: 'm',
    input: 'x',
    fetchScript: { '/embeddings': { status: 429, body: { error: { message: 'rate limit exceeded' } } } },
  },
  {
    id: 'ollama-embed',
    kind: 'ollama',
    model: 'nomic-embed-text',
    input: 'embed me',
    fetchScript: {
      '/api/show': { status: 200, body: { model_info: { 'nomic.context_length': 4096 } } },
      '/api/embed': { status: 200, body: { embeddings: [[0.5, 0.25, -0.5]] } },
    },
  },
  {
    id: 'ollama-numctx-ceiling',
    kind: 'ollama',
    model: 'big-model',
    input: 'x',
    fetchScript: {
      '/api/show': { status: 200, body: { model_info: { 'llama.context_length': 131072 } } },
      '/api/embed': { status: 200, body: { embeddings: [[1, 2]] } },
    },
  },
  {
    id: 'ollama-numctx-fallback',
    kind: 'ollama',
    model: 'no-ctx',
    input: 'x',
    fetchScript: {
      '/api/show': { status: 200, body: { model_info: {} } },
      '/api/embed': { status: 200, body: { embeddings: [[3, 4]] } },
    },
  },
  {
    id: 'ollama-404-legacy',
    kind: 'ollama',
    model: 'old-model',
    input: 'legacy path',
    fetchScript: {
      '/api/show': { status: 200, body: { model_info: { 'x.context_length': 2048 } } },
      '/api/embed': { status: 404, body: {} },
      '/api/embeddings': { status: 200, body: { embedding: [7, 8, 9] } },
    },
  },
  {
    id: 'ollama-nan',
    kind: 'ollama',
    model: 'nan-model',
    input: 'boom',
    fetchScript: {
      '/api/show': { status: 200, body: { model_info: { 'x.context_length': 1024 } } },
      // A non-numeric element stands in for a non-finite value (JSON can't hold NaN).
      '/api/embed': { status: 200, body: { embeddings: [[1, null, 3]] } },
    },
  },
  {
    id: 'ollama-empty-input',
    kind: 'ollama',
    model: 'm',
    input: '   ',
    fetchScript: {},
  },
  {
    id: 'openrouter-array',
    kind: 'openrouter',
    model: 'openai/text-embedding-3-small',
    input: 'router text',
    dimensions: 512,
    sdkResponse: { model: 'openai/text-embedding-3-small', data: [{ embedding: [0.1, 0.2] }], usage: { promptTokens: 3, totalTokens: 3, cost: 0.0001 } },
  },
  {
    id: 'openrouter-base64',
    kind: 'openrouter',
    model: 'voyage/voyage-3',
    input: 'binary',
    sdkResponse: { model: 'voyage/voyage-3', data: [{ embedding: '__B64__' }], usage: { promptTokens: 1, totalTokens: 1 } },
  },
  {
    id: 'openrouter-error-string',
    kind: 'openrouter',
    model: 'm',
    input: 'x',
    sdkResponse: '__STRING__:overloaded',
  },
  // P4.D101 — NanoGPT's OpenAI-compatible /embeddings route, driven through
  // v4's REAL NanoGPTEmbeddingProvider.
  {
    id: 'nanogpt-basic',
    kind: 'nanogpt',
    model: 'text-embedding-3-small',
    input: 'hello nano',
    fetchScript: { '/embeddings': { status: 200, body: { data: [{ embedding: [0.5, -0.25, 0.125] }], usage: { prompt_tokens: 3, total_tokens: 3 } } } },
  },
  {
    // `dimensions` rides the body only when set.
    id: 'nanogpt-dimensions',
    kind: 'nanogpt',
    model: 'text-embedding-3-large',
    input: 'sized',
    dimensions: 256,
    fetchScript: { '/embeddings': { status: 200, body: { data: [{ embedding: [1, 2] }], usage: { prompt_tokens: 1, total_tokens: 1 } } } },
  },
  {
    // A custom base url replaces the gateway default.
    id: 'nanogpt-custom-base',
    kind: 'nanogpt',
    baseUrl: 'https://proxy.example/v1',
    model: 'BAAI/bge-m3',
    input: 'proxied',
    fetchScript: { '/embeddings': { status: 200, body: { data: [{ embedding: [0.1] }] } } },
  },
  {
    // The error sentence: `NanoGPT embedding failed: <error.error.message>`.
    id: 'nanogpt-error-message',
    kind: 'nanogpt',
    model: 'text-embedding-3-small',
    input: 'boom',
    fetchScript: { '/embeddings': { status: 401, body: { error: { message: 'Invalid API key' } } } },
  },
  {
    // No `error.message` in the body → falls back to the HTTP statusText.
    id: 'nanogpt-error-statustext',
    kind: 'nanogpt',
    model: 'text-embedding-3-small',
    input: 'boom',
    fetchScript: { '/embeddings': { status: 500, body: {} } },
  },
  {
    // usage absent → the result carries no usage block.
    id: 'nanogpt-no-usage',
    kind: 'nanogpt',
    model: 'gemini-embedding-001',
    input: 'bare',
    fetchScript: { '/embeddings': { status: 200, body: { data: [{ embedding: [0.9, 0.8] }] } } },
  },

];

async function run() {
  const rows: unknown[] = [];
  for (const c of cases) {
    jest.resetModules();
    fetchScript = c.fetchScript || {};
    installFetch();
    sdkRequestBody = null;
    // Resolve openrouter sdkResponse markers.
    if (c.kind === 'openrouter') {
      if (c.sdkResponse === '__STRING__:overloaded') sdkResponse = 'overloaded';
      else if (c.sdkResponse && typeof c.sdkResponse === 'object' && (c.sdkResponse as any).data?.[0]?.embedding === '__B64__') {
        sdkResponse = { ...(c.sdkResponse as any), data: [{ embedding: f32Bytes }] };
      } else sdkResponse = c.sdkResponse;
    }

    let out: { result?: unknown; error?: string };
    if (c.kind === 'openai') out = await driveOpenAI(c);
    else if (c.kind === 'ollama') out = await driveOllama(c);
    else if (c.kind === 'nanogpt') out = await driveNanoGPT(c);
    else out = await driveOpenRouter(c);

    rows.push({
      id: c.id,
      kind: c.kind,
      baseUrl: c.baseUrl ?? null,
      model: c.model,
      input: c.input,
      dimensions: c.dimensions ?? null,
      fetchScript: c.fetchScript ?? {},
      sdkResponse: c.kind === 'openrouter' ? sdkResponse : null,
      sdkRequestBody,
      recorded,
      ...out,
    });
  }

  // P4.D101 — the CATALOGUE row: v4's real plugin `getEmbeddingModels()`, so
  // v5's `embedding_models_for("NANOGPT")` is a pinned transcription rather
  // than a hand-copy that rots on v4's next model. (The route-level
  // list-models differential cannot cover this: v4's registry is empty in the
  // jest sandbox, which is why those arms are Rust-side today.)
  for (const [dir, provider] of [
    ['qtap-plugin-nanogpt', 'NANOGPT'],
    ['qtap-plugin-openai', 'OPENAI'],
    ['qtap-plugin-ollama', 'OLLAMA'],
    ['qtap-plugin-openrouter', 'OPENROUTER'],
    ['qtap-plugin-builtin-embeddings', 'BUILTIN'],
  ] as const) {
    const { plugin } = await import(`@/plugins/dist/${dir}/index`);
    rows.push({
      id: `${provider.toLowerCase()}-embedding-catalogue`,
      kind: 'catalogue',
      provider,
      models: plugin.getEmbeddingModels ? plugin.getEmbeddingModels() : null,
      // The shared row shape, so one Rust struct still parses every line.
      baseUrl: null,
      model: '',
      input: '',
      dimensions: null,
      fetchScript: {},
      sdkResponse: null,
      sdkRequestBody: null,
      recorded: [],
    } as never);
  }

  const out = process.env.QT_ORACLE_OUT;
  const text = rows.map((r) => JSON.stringify(r)).join('\n') + '\n';
  if (out) fs.writeFileSync(out, text);
  else process.stdout.write(text);
}

test('emit embedding-wire oracle', async () => {
  await run();
});
