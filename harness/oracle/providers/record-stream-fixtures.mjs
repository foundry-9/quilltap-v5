/**
 * Stream-decoder fixture recorder (wave 4 / W4.7b).
 *
 * Drives v4's REAL provider plugin `streamMessage` generator over a committed
 * wire transcript and emits the normalised `StreamChunk` sequence as NDJSON —
 * the oracle the Rust `model::decoders` differential (`stream_decoders_equivalence`)
 * diffs against.
 *
 * How it works: it replaces `global.fetch` with a mock returning a `Response`
 * whose body is a `ReadableStream` that emits the transcript bytes in
 * line-aligned chunks (real SSE/NDJSON servers send whole frames per read). All
 * provider SDKs (openai, @anthropic-ai/sdk, @google/genai) resolve `fetch` from
 * `globalThis` by default, and ollama/openrouter use `fetch` directly — so ONE
 * mock covers every provider. The plugin's own SDK/transport parses the wire and
 * hands v4's generator the parsed events; we collect the yielded chunks.
 *
 * Because SDKs resolve `@quilltap/plugin-utils` + the real provider SDK from the
 * PLUGIN's own `node_modules`, run this FROM the plugin directory:
 *
 *   cd ~/source/quilltap-server/plugins/dist/qtap-plugin-<name>
 *   node <V5>/harness/oracle/providers/record-stream-fixtures.mjs \
 *     --provider <name> \
 *     --cases <V5>/harness/oracle/fixtures/streams/<decoder>/cases.json \
 *     --fixtures-dir <V5>/harness/oracle/fixtures/streams/<decoder> \
 *     --out /tmp/oracle-<decoder>.ndjson
 *
 * The `regenerate.sh` sibling drives all providers/decoders in one shot.
 *
 * NDJSON line shape (one per case): { decoder, provider, case, error, chunks: [<v4 StreamChunk>...] }
 * where each StreamChunk is the exact object the v4 generator yielded (its keys
 * omitted-when-absent, per v4). Node 24 (see [[oracle-node-abi-gotcha]] — no DB
 * here, but standardise the Node).
 *
 * P4.122 — a case whose generator THREW also records `thrown` (the fields v4's
 * refusal classifier duck-types off the thrown value — `record-text-errors.mjs`'s
 * `thrownFields`: `message, name, code?, errorCode?, providerReason?, status?`,
 * RAW values) and v4's two verdicts over that same value: `refusal` =
 * `classifyRefusal({ error })`, `trigger` = `classifyFallbackTrigger(error)`.
 * A case that did not throw carries none of the three keys (so every
 * pre-existing row stays byte-identical). The verdicts import v4's REAL modules
 * from `--v4 <checkout>`; since this runs from a PLUGIN dir, tsx must be given
 * the checkout's root tsconfig so their `@/` imports resolve:
 *
 *   npx tsx --tsconfig <V4>/tsconfig.json <this> --v4 <V4> --provider …
 *
 * A throwing row also records `pluginErrorLog` when the plugin logged at ERROR
 * level while the case ran (P4.122 item 6 — `openai-compatible.ts`'s, DeepSeek's
 * and NanoGPT's `… API error in streamMessage` catch lines): `[{ plugin,
 * message, context, error }]`, captured through `@quilltap/plugin-utils`' own
 * host bridge (`globalThis.__quilltap_logger_factory` — the injection point
 * the Quilltap host uses to route plugin logs into its logger), installed
 * BEFORE any plugin module loads so module-level loggers route through it
 * too. Non-error levels are forwarded to the console exactly as the
 * standalone logger printed them. */

import { readFileSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {};
  for (let i = 0; i < args.length; i += 2) {
    const key = args[i].replace(/^--/, '');
    out[key] = args[i + 1];
  }
  return out;
}

// Map a provider key to the plugin dir + a factory that builds the provider.
// The plugin dir is relative to the v4 checkout root.
const PROVIDERS = {
  // chat-completions-sse
  deepseek: {
    dir: 'plugins/dist/qtap-plugin-deepseek',
    async make() {
      const { DeepSeekProvider } = await import(pathToFileURL(resolve('provider.ts')));
      return new DeepSeekProvider();
    },
  },
  'z-ai': {
    dir: 'plugins/dist/qtap-plugin-z-ai',
    async make() {
      const { ZAIProvider } = await import(pathToFileURL(resolve('provider.ts')));
      return new ZAIProvider();
    },
  },
  // P4.D101 — NanoGPT (plugin 1.0.2, v4 bug 87's echo guard included).
  nanogpt: {
    dir: 'plugins/dist/qtap-plugin-nanogpt',
    async make() {
      const { NanoGPTProvider } = await import(pathToFileURL(resolve('provider.ts')));
      return new NanoGPTProvider();
    },
  },
  // P4.D83 (v4 `93ed8abf`): the OPENAI_COMPATIBLE endpoint. Instantiate the
  // SUBCLASS the plugin uses, not the re-exported base — since bug 71 the base
  // declares an empty `profileParamAllowlist` and the subclass supplies the real
  // one. (The stream decoder does not read it, but the two classes are no longer
  // interchangeable and the recorder should not pretend otherwise.)
  'openai-compatible': {
    dir: 'plugins/dist/qtap-plugin-openai-compatible',
    async make() {
      const { OpenAICompatibleEndpointProvider } = await import(
        pathToFileURL(resolve('provider.ts'))
      );
      return new OpenAICompatibleEndpointProvider('http://localhost:8080/v1');
    },
  },
  openrouter: {
    dir: 'plugins/dist/qtap-plugin-openrouter',
    async make() {
      const { OpenRouterProvider } = await import(pathToFileURL(resolve('provider.ts')));
      return new OpenRouterProvider();
    },
  },
  // responses-api-sse
  openai: {
    dir: 'plugins/dist/qtap-plugin-openai',
    async make() {
      const { OpenAIProvider } = await import(pathToFileURL(resolve('provider.ts')));
      return new OpenAIProvider();
    },
  },
  grok: {
    dir: 'plugins/dist/qtap-plugin-grok',
    async make() {
      const { GrokProvider } = await import(pathToFileURL(resolve('provider.ts')));
      return new GrokProvider();
    },
  },
  // anthropic-sse
  anthropic: {
    dir: 'plugins/dist/qtap-plugin-anthropic',
    async make() {
      const { AnthropicProvider } = await import(pathToFileURL(resolve('provider.ts')));
      return new AnthropicProvider();
    },
  },
  // google-parts
  google: {
    dir: 'plugins/dist/qtap-plugin-google',
    async make() {
      const { GoogleProvider } = await import(pathToFileURL(resolve('provider.ts')));
      return new GoogleProvider();
    },
  },
  // ollama-ndjson
  ollama: {
    dir: 'plugins/dist/qtap-plugin-ollama',
    async make() {
      const { OllamaProvider } = await import(pathToFileURL(resolve('provider.ts')));
      return new OllamaProvider('http://localhost:11434');
    },
  },
};

/**
 * Build a mock Response whose body streams `wire` bytes in LINE-ALIGNED chunks
 * (split after each `\n`), the way a real SSE/NDJSON server delivers frames.
 * This is the single chunking v4 observes; the Rust differential replays the
 * same transcript at whole/per-frame/byte chunkings and asserts they all match
 * this recording (except ollama — see the decoder's bug-parity note).
 */
function wireToResponse(wire) {
  const enc = new TextEncoder();
  // Split into line-aligned pieces (keep the trailing `\n` on each).
  const pieces = [];
  let start = 0;
  for (let i = 0; i < wire.length; i++) {
    if (wire[i] === '\n') {
      pieces.push(wire.slice(start, i + 1));
      start = i + 1;
    }
  }
  if (start < wire.length) pieces.push(wire.slice(start));

  let idx = 0;
  const stream = new ReadableStream({
    pull(controller) {
      if (idx >= pieces.length) {
        controller.close();
        return;
      }
      controller.enqueue(enc.encode(pieces[idx]));
      idx++;
    },
  });
  return new Response(stream, {
    status: 200,
    headers: { 'content-type': 'text/event-stream' },
  });
}

// The image recorder's `thrownFields`, as `record-text-errors.mjs` widened it
// with `name` (P4.118) — copied verbatim so the three corpora agree on shape.
function thrownFields(e) {
  if (!(e instanceof Error)) return { message: String(e), name: null };
  const out = { message: e.message, name: typeof e.name === 'string' ? e.name : null };
  if (e.code !== undefined && e.code !== null) out.code = e.code;
  const nested = e.error && typeof e.error === 'object' ? e.error.code : undefined;
  if (nested !== undefined && nested !== null) out.errorCode = nested;
  if (typeof e.providerReason === 'string') out.providerReason = e.providerReason;
  const status =
    typeof e.statusCode === 'number' ? e.statusCode : typeof e.status === 'number' ? e.status : undefined;
  if (status !== undefined) out.status = status;
  return out;
}

// The per-case ERROR-level plugin log (see the header). `null` between cases.
let pluginErrorSink = null;
function captureLogger(prefix, baseContext = {}) {
  const fmt = (context) => {
    const merged = { ...baseContext, ...context };
    const entries = Object.entries(merged)
      .filter(([key]) => key !== 'context')
      .map(([key, value]) => `${key}=${JSON.stringify(value)}`)
      .join(' ');
    return entries ? ` ${entries}` : '';
  };
  return {
    debug: (m, c) => console.debug(`[${prefix}] ${m}${fmt(c)}`),
    info: (m, c) => console.info(`[${prefix}] ${m}${fmt(c)}`),
    warn: (m, c) => console.warn(`[${prefix}] ${m}${fmt(c)}`),
    error: (m, c, e) => {
      console.error(`[${prefix}] ${m}${fmt(c)}`, e ? `\n${e.stack || e.message}` : '');
      if (pluginErrorSink) {
        pluginErrorSink.push({
          plugin: prefix,
          message: m,
          context: { ...baseContext, ...(c ?? {}) },
          error: e instanceof Error ? e.message : null,
        });
      }
    },
    child: (extra) => captureLogger(prefix, { ...baseContext, ...extra }),
  };
}
globalThis.__quilltap_logger_factory = (pluginName) => captureLogger(pluginName);

async function main() {
  const args = parseArgs();
  if (!args.v4) {
    console.error('usage: --v4 <checkout> is required (v4\'s classifier verdicts — P4.122)');
    process.exit(1);
  }
  // v4's classifier + fallback trigger — the REAL modules, from the checkout.
  const { classifyRefusal } = await import(
    pathToFileURL(resolve(args.v4, 'lib/services/dangerous-content/refusal.ts')).href
  );
  const { classifyFallbackTrigger } = await import(
    pathToFileURL(resolve(args.v4, 'lib/llm/fallback/engine.ts')).href
  );
  const provider = args.provider;
  const casesPath = args.cases;
  const fixturesDir = args['fixtures-dir'];
  const outPath = args.out;
  if (!provider || !casesPath || !fixturesDir || !outPath) {
    console.error(
      'usage: --provider <name> --cases <cases.json> --fixtures-dir <dir> --out <ndjson>'
    );
    process.exit(1);
  }

  const spec = PROVIDERS[provider];
  if (!spec) {
    console.error(`unknown provider ${provider}`);
    process.exit(1);
  }

  const cases = JSON.parse(readFileSync(casesPath, 'utf8'));
  const providerCases = cases.filter((c) => c.provider === provider);

  const lines = [];
  for (const c of providerCases) {
    const wire = readFileSync(resolve(fixturesDir, c.wire), 'utf8');

    const origFetch = globalThis.fetch;
    globalThis.fetch = async () => wireToResponse(wire);
    let chunks = [];
    let error = null;
    let thrownValue = null;
    pluginErrorSink = [];
    try {
      const inst = await spec.make();
      const params = {
        messages: c.messages || [{ role: 'user', content: 'hi' }],
        model: c.model,
        temperature: c.temperature,
        maxTokens: c.maxTokens ?? 1024,
        topP: c.topP,
        tools: c.tools,
        webSearchEnabled: c.webSearchEnabled ?? false,
        profileParameters: c.profileParameters,
        stop: c.stop,
        previousResponseId: c.previousResponseId,
        cacheKey: c.cacheKey,
      };
      for await (const chunk of inst.streamMessage(params, 'test-key')) {
        chunks.push(chunk);
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      thrownValue = e;
    } finally {
      globalThis.fetch = origFetch;
    }

    const row = {
      decoder: c.decoder,
      provider: c.provider,
      case: c.case,
      error,
      chunks,
    };
    if (thrownValue !== null) {
      row.thrown = thrownFields(thrownValue);
      row.refusal = classifyRefusal({ error: thrownValue });
      row.trigger = classifyFallbackTrigger(thrownValue);
      if (pluginErrorSink.length > 0) row.pluginErrorLog = pluginErrorSink;
    }
    pluginErrorSink = null;
    lines.push(JSON.stringify(row));
  }

  writeFileSync(outPath, lines.join('\n') + '\n');
  console.error(`wrote ${providerCases.length} case(s) for ${provider} → ${outPath}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
