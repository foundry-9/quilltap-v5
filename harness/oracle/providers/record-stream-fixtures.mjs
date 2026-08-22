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
 * NDJSON line shape (one per case): { decoder, provider, case, chunks: [<v4 StreamChunk>...] }
 * where each StreamChunk is the exact object the v4 generator yielded (its keys
 * omitted-when-absent, per v4). Node 24 (see [[oracle-node-abi-gotcha]] — no DB
 * here, but standardise the Node).
 */

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

async function main() {
  const args = parseArgs();
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
    } finally {
      globalThis.fetch = origFetch;
    }

    lines.push(
      JSON.stringify({
        decoder: c.decoder,
        provider: c.provider,
        case: c.case,
        error,
        chunks,
      })
    );
  }

  writeFileSync(outPath, lines.join('\n') + '\n');
  console.error(`wrote ${providerCases.length} case(s) for ${provider} → ${outPath}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
