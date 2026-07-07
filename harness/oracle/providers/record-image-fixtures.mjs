/**
 * Image-dialect recorder (W4.7f).
 *
 * Drives a v4 image-provider plugin's REAL `generateImage` with `global.fetch`
 * mocked to a committed wire payload, capturing (a) the exact request the plugin
 * (or its SDK) builds and (b) the outcome — the parsed `ImageGenResponse` OR the
 * thrown error string. The Rust `model::image_dialects` differential
 * (`image_dialects_equivalence`) diffs its `build_image_request` /
 * `parse_image_response` against these rows.
 *
 * Run FROM the plugin directory (imports resolve from the plugin's node_modules;
 * the record-request-envelopes.mjs precedent), Node 24 under `npx tsx`:
 *
 *   cd ~/source/quilltap-server/plugins/dist/qtap-plugin-<name>
 *   node <V5>/harness/oracle/providers/record-image-fixtures.mjs \
 *     --provider <name> --out /tmp/img-<name>.ndjson
 *
 * `regenerate-image-fixtures.sh` drives every provider and concatenates into
 * fixtures/image-dialects/image-dialects.recorded.ndjson.
 *
 * Line shape:
 *   { provider, case, model, style, input, request:{method,url,body},
 *     mode:'wire'|'sdkThrow', wire:{status,body}, outcome:'ok'|'thrown',
 *     images:[{data,url,mimeType,revisedPrompt}]|null, thrown:string|null,
 *     isModeration:bool }
 */

import { writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {};
  for (let i = 0; i < args.length; i += 2) out[args[i].replace(/^--/, '')] = args[i + 1];
  return out;
}

// A verbatim copy of v4 `isImageModerationError`
// (lib/services/dangerous-content/provider-routing.service.ts). Applied to the
// REAL thrown strings so the recorded verdict exercises the keyword matrix incl.
// the documented gaps; the Rust port must classify identically.
function isImageModerationError(message) {
  const m = (message || '').toLowerCase();
  return (
    m.includes('content moderation') ||
    m.includes('content_policy') ||
    m.includes('content policy') ||
    m.includes('safety system') ||
    m.includes('rejected by content') ||
    m.includes('moderation_blocked')
  );
}

const PROVIDERS = {
  openai: {
    style: 'sdk',
    make: async () => new (await import(pathToFileURL(resolve('image-provider.ts')))).OpenAIImageProvider(),
  },
  google: {
    style: 'fetch',
    make: async () => new (await import(pathToFileURL(resolve('image-provider.ts')))).GoogleImagenProvider(),
  },
  grok: {
    style: 'fetch-sdk', // OpenAI SDK, but treated per-status below
    make: async () => new (await import(pathToFileURL(resolve('image-provider.ts')))).GrokImageProvider(),
  },
  openrouter: {
    style: 'fetch',
    make: async () => new (await import(pathToFileURL(resolve('image-provider.ts')))).OpenRouterImageProvider(),
  },
  'z-ai': {
    style: 'sdk',
    make: async () => new (await import(pathToFileURL(resolve('image-provider.ts')))).ZAIImageProvider(),
  },
};

// Providers whose transport is the OpenAI SDK (a non-2xx becomes a thrown Error).
const SDK_PROVIDERS = new Set(['openai', 'grok', 'z-ai']);

function projectImages(images) {
  return (images || []).map((img) => ({
    data: img.data ?? null,
    url: img.url ?? null,
    mimeType: img.mimeType ?? null,
    revisedPrompt: img.revisedPrompt ?? null,
  }));
}

const LONG_REFUSAL = 'X'.repeat(250);

function casesFor(provider) {
  const c = [];
  const add = (name, params, wire) => c.push({ name, params, wire });
  const ok = (status, obj) => ({ status, body: JSON.stringify(obj) });

  if (provider === 'openai') {
    add('happy_b64', { prompt: 'a cat', model: 'dall-e-3', n: 1, size: '1024x1024', quality: 'hd', style: 'natural' },
      ok(200, { created: 1, data: [{ b64_json: 'QUJD', revised_prompt: 'a fluffy cat' }] }));
    add('gpt_image', { prompt: 'a cat', model: 'gpt-image-1', n: 1 },
      ok(200, { created: 1, data: [{ b64_json: 'QUJD' }] }));
    add('url_only', { prompt: 'a cat', model: 'dall-e-3', n: 1 },
      ok(200, { created: 1, data: [{ url: 'https://oai/x.png' }] }));
    add('size_normalize', { prompt: 'a cat', model: 'dall-e-2', n: 2, size: '9999x9999' },
      ok(200, { created: 1, data: [{ b64_json: 'A' }, { b64_json: 'B' }] }));
    add('moderation', { prompt: 'bad', model: 'dall-e-3', n: 1 },
      ok(400, { error: { message: 'Your request was rejected as a result of our safety system.', type: 'image_generation_user_error', code: 'moderation_blocked' } }));
    add('invalid_response', { prompt: 'a cat', model: 'dall-e-3', n: 1 },
      ok(200, { created: 1, foo: 1 }));
  } else if (provider === 'grok') {
    add('happy_b64', { prompt: 'a cat', model: 'grok-imagine-image', n: 1 },
      ok(200, { data: [{ b64_json: 'QUJD', revised_prompt: 'rp' }] }));
    add('pro_resolution', { prompt: 'a cat', model: 'grok-imagine-image-pro', n: 1, aspectRatio: '16:9' },
      ok(200, { data: [{ b64_json: 'X' }] }));
    add('url_only', { prompt: 'a cat', model: 'grok-2-image', n: 1 },
      ok(200, { data: [{ url: 'https://grok/x.jpg' }] }));
    add('moderation', { prompt: 'bad', model: 'grok-imagine-image', n: 1 },
      ok(400, { error: { message: 'Generated image rejected by content moderation.' } }));
  } else if (provider === 'z-ai') {
    add('happy_b64', { prompt: 'a cat', model: 'cogview-4-250304', n: 1 },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    add('url_only', { prompt: 'a cat', model: 'glm-image', n: 1 },
      ok(200, { data: [{ url: 'https://z.ai/x.png' }] }));
    add('both_b64_url', { prompt: 'a cat', model: 'cogview-4-250304', n: 1, quality: 'hd' },
      ok(200, { data: [{ b64_json: 'QUJD', url: 'https://z.ai/x.png', revised_prompt: 'rp' }] }));
    // z-ai has NO moderation handling: a generic 400 just surfaces the SDK message.
    add('generic_error', { prompt: 'x', model: 'glm-image', n: 1 },
      ok(400, { error: { message: 'Bad request' } }));
  } else if (provider === 'google') {
    add('imagen_happy', { prompt: 'a cat', model: 'imagen-4', n: 1, aspectRatio: '3:4', seed: 7 },
      ok(200, { predictions: [{ bytesBase64Encoded: 'QUJD', mimeType: 'image/png' }] }));
    add('imagen_empty_pred_reason', { prompt: 'bad', model: 'imagen-4', n: 1 },
      ok(200, { predictions: [{ raiFilteredReason: 'unsafe content' }] }));
    add('imagen_empty_data_reason', { prompt: 'bad', model: 'imagen-4', n: 1 },
      ok(200, { predictions: [], raiFilteredReason: 'data-level reason' }));
    add('imagen_empty_filtered_reason', { prompt: 'bad', model: 'imagen-4-fast', n: 1 },
      ok(200, { predictions: [], filteredReason: 'fr' }));
    add('imagen_empty_no_reason', { prompt: 'bad', model: 'imagen-4', n: 1 },
      ok(200, { predictions: [] }));
    add('imagen_http_error_msg', { prompt: 'x', model: 'imagen-4', n: 1 },
      ok(500, { error: { message: 'quota exceeded' } }));
    add('imagen_http_error_fallback', { prompt: 'x', model: 'imagen-4', n: 1 },
      ok(503, {}));
    add('gemini_happy', { prompt: 'a cat', model: 'gemini-2.5-flash-image', n: 1, aspectRatio: '3:4' },
      ok(200, { candidates: [{ content: { parts: [{ inlineData: { data: 'QUJD', mimeType: 'image/png' } }, { text: 'here you go' }] } }] }));
    add('gemini_refusal', { prompt: 'bad', model: 'gemini-2.5-flash-image', n: 1 },
      ok(200, { candidates: [{ content: { parts: [{ text: 'I will not create that image.' }] } }] }));
    add('gemini_no_images_default', { prompt: 'x', model: 'gemini-3-pro-image-preview', n: 1 },
      ok(200, { candidates: [{ content: { parts: [] } }] }));
    add('gemini_http_error', { prompt: 'x', model: 'gemini-2.5-flash-image', n: 1 },
      ok(400, { error: { message: 'bad request to gemini' } }));
  } else if (provider === 'openrouter') {
    add('happy_data_uri', { prompt: 'a cat', model: 'google/gemini-2.5-flash-preview-native-image', n: 1 },
      ok(200, { choices: [{ message: { images: [{ image_url: { url: 'data:image/png;base64,QUJD' } }] } }] }));
    add('external_url', { prompt: 'a cat', model: 'google/gemini-2.5-flash-preview-native-image', n: 1 },
      ok(200, { choices: [{ message: { images: [{ image_url: { url: 'https://ext.example/i.jpg' } }] } }] }));
    add('negative_and_style_and_hd', { prompt: 'a cat', model: 'google/gemini-3-pro-image-preview', n: 1, negativePrompt: 'blurry', style: 'photographic', quality: 'hd', aspectRatio: '16:9' },
      ok(200, { choices: [{ message: { images: [{ image_url: { url: 'data:image/webp;base64,QUJD' } }] } }] }));
    add('declined_gap', { prompt: 'bad', model: 'google/gemini-2.5-flash-preview-native-image', n: 1 },
      ok(200, { choices: [{ message: { refusal: 'Sorry, I will not create that.' } }] }));
    add('declined_long_slice', { prompt: 'bad', model: 'google/gemini-2.5-flash-preview-native-image', n: 1 },
      ok(200, { choices: [{ message: { content: LONG_REFUSAL } }] }));
    add('no_images_default', { prompt: 'x', model: 'google/gemini-2.5-flash-preview-native-image', n: 1 },
      ok(200, { choices: [{ message: {} }] }));
    add('http_error', { prompt: 'x', model: 'google/gemini-2.5-flash-preview-native-image', n: 1 },
      { status: 500, body: 'upstream is down' });
  }
  return c;
}

function makeResponse(status, bodyText) {
  return new Response(bodyText, {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

async function main() {
  const args = parseArgs();
  const provider = args.provider;
  const outPath = args.out;
  if (!provider || !outPath || !PROVIDERS[provider]) {
    console.error('usage: --provider <openai|google|grok|openrouter|z-ai> --out <ndjson>');
    process.exit(1);
  }
  const spec = PROVIDERS[provider];
  const lines = [];

  for (const c of casesFor(provider)) {
    let captured = null;
    const origFetch = globalThis.fetch;
    globalThis.fetch = async (url, init) => {
      if (!captured) {
        const u = typeof url === 'string' ? url : (url && url.url) || String(url);
        const method = (init && init.method) || (url && url.method) || 'POST';
        let body = (init && init.body) || (url && url.body) || null;
        if (body && typeof body !== 'string') {
          try { body = new TextDecoder().decode(body); } catch { body = String(body); }
        }
        captured = { method, url: u, body };
      }
      return makeResponse(c.wire.status, c.wire.body);
    };

    let outcome = 'ok';
    let images = null;
    let thrown = null;
    try {
      const inst = await spec.make();
      const params = { ...c.params };
      const res = await inst.generateImage(params, 'test-api-key');
      images = projectImages(res.images);
    } catch (e) {
      outcome = 'thrown';
      thrown = e instanceof Error ? e.message : String(e);
    } finally {
      globalThis.fetch = origFetch;
    }

    // Mode: fetch-style providers parse the status themselves (always 'wire');
    // SDK providers parse only 2xx bodies, a non-2xx being the SDK throw.
    const is2xx = c.wire.status >= 200 && c.wire.status < 300;
    const mode = SDK_PROVIDERS.has(provider) && !is2xx ? 'sdkThrow' : 'wire';

    lines.push(
      JSON.stringify({
        kind: 'dialect',
        provider,
        case: c.name,
        model: c.params.model,
        style: spec.style,
        input: c.params,
        request: captured,
        mode,
        wire: c.wire,
        outcome,
        images,
        thrown,
        isModeration: outcome === 'thrown' ? isImageModerationError(thrown) : false,
      })
    );
  }

  // Also dump the plugin's orientation declarations (v4
  // `getImageGenerationModels` + `getImageProviderConstraints`) so the Rust
  // `image_gen_data::orientation_data_for` transcription is verified against v4.
  try {
    const idx = await import(pathToFileURL(resolve('index.ts')));
    const plug =
      Object.values(idx).find((v) => v && (v.getImageGenerationModels || v.getImageProviderConstraints)) ||
      idx.default;
    const models = plug?.getImageGenerationModels
      ? plug.getImageGenerationModels().map((m) => ({ id: m.id, orientationSupport: m.orientationSupport ?? null }))
      : [];
    const constraint = plug?.getImageProviderConstraints
      ? plug.getImageProviderConstraints()?.orientationSupport ?? null
      : null;
    lines.push(JSON.stringify({ kind: 'orientation', provider, models, providerConstraint: constraint }));
  } catch (e) {
    process.stderr.write(`orientation dump failed for ${provider}: ${e}\n`);
  }

  writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`image-dialects oracle wrote ${outPath} (${lines.length} ${provider} cases)\n`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
