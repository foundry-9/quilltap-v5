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
 * Line shape (kind:'dialect'):
 *   { provider, case, model, style, input, request:{method,url,body},
 *     mode:'wire'|'sdkThrow', wire:{status,body}, outcome:'ok'|'thrown',
 *     images:[{data,url,mimeType,revisedPrompt}]|null, thrown:string|null,
 *     isModeration:bool }
 *
 * Line shape (kind:'models' — the `ca22ec45` keyed model discovery): drives the
 * plugin's REAL `getAvailableModels(apiKey?)` over a SEQUENCE of canned wire
 * responses, capturing every request the plugin (or its SDK) made:
 *   { provider, case, withKey:bool, supportedModels:[...],
 *     requests:[{method,url,body,headers}], wire:[{status,body,headers}],
 *     outcome:'ok'|'thrown', models:[...]|null, thrown:string|null }
 *
 * Line shape (kind:'download' — the `ca22ec45` Z.AI URL→base64 conversion):
 * a `generateImage` case whose wire body carries a `url`, with the follow-up
 * image download answered by a distinct binary response:
 *   { provider, case, model, input, requests:[{method,url,body,headers}],
 *     wire:[…], download:{status,contentType,bytesBase64}, outcome, images,
 *     thrown }
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
  // P4.D101 — NanoGPT's OpenAI-compatible images route (the OpenAI SDK against
  // its own baseURL, so a non-2xx becomes a thrown SDK Error).
  nanogpt: {
    style: 'sdk',
    make: async () => new (await import(pathToFileURL(resolve('image-provider.ts')))).NanoGPTImageProvider(),
  },
};

// Providers whose transport is the OpenAI SDK (a non-2xx becomes a thrown Error).
const SDK_PROVIDERS = new Set(['openai', 'grok', 'z-ai', 'nanogpt']);

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
  /** A case whose provider makes a FOLLOW-UP image download after the wire call. */
  const addDl = (name, params, wire, download) => c.push({ name, params, wire, download });
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
    // === d8d2890ee (PR #62): GPT Image 2.5 and the full OpenAI parameter set ===
    // One row per case in v4's own `__tests__/unit/openai-image-provider-params.
    // test.ts`, plus the JS-coercion edges its `it` blocks never reach
    // (`readEnum`'s `String(raw)`, `readIntInRange`'s `Number(raw)`, the
    // empty-string-is-unset gate, and `??` vs truthiness on `style`/`quality`).
    // Every one answers the same 200 so the comparand is the REQUEST BODY and
    // the returned `mimeType`, never the wire.
    const okImage = () => ok(200, { created: 1, data: [{ b64_json: 'aGVsbG8=', revised_prompt: 'a revised prompt' }] });

    // -- quality tiers --------------------------------------------------
    add('q25_sunburst_xhigh', { prompt: 'a cat', model: 'gpt-image-2.5-sunburst', quality: 'xhigh' }, okImage());
    add('q25_sunburst_max', { prompt: 'a cat', model: 'gpt-image-2.5-sunburst', quality: 'max' }, okImage());
    add('q25_flare_xhigh', { prompt: 'a cat', model: 'gpt-image-2.5-flare', quality: 'xhigh' }, okImage());
    add('q25_flare_max', { prompt: 'a cat', model: 'gpt-image-2.5-flare', quality: 'max' }, okImage());
    // A dated snapshot rides its family's tiers via the longest-prefix match.
    add('q_dated_snapshot_max', { prompt: 'a cat', model: 'gpt-image-2.5-flare-2026-09-08', quality: 'max' }, okImage());
    // xhigh is 2.5's alone: gpt-image-2 drops it and sends NO quality.
    add('q_drop_xhigh_on_gpt_image_2', { prompt: 'a cat', model: 'gpt-image-2', quality: 'xhigh' }, okImage());
    // The DALL-E spelling on a GPT Image model is dropped, not translated.
    add('q_drop_hd_on_gpt_image', { prompt: 'a cat', model: 'gpt-image-1.5', quality: 'hd' }, okImage());
    add('q_omit_for_gpt_image_unset', { prompt: 'a cat', model: 'gpt-image-2.5-sunburst' }, okImage());
    add('q_dalle3_hd', { prompt: 'a cat', model: 'dall-e-3', quality: 'hd' }, okImage());
    add('q_dalle3_unset_standard', { prompt: 'a cat', model: 'dall-e-3' }, okImage());
    // A tier dall-e-3 does not offer falls back to its historical default.
    add('q_dalle3_bogus_falls_back', { prompt: 'a cat', model: 'dall-e-3', quality: 'max' }, okImage());
    // `!quality` is a TRUTHY test, so an empty string is "unset".
    add('q_empty_string_is_unset', { prompt: 'a cat', model: 'dall-e-3', quality: '' }, okImage());

    // -- size handling ---------------------------------------------------
    add('size_arbitrary_accepted', { prompt: 'a cat', model: 'gpt-image-2.5-sunburst', size: '1536x864' }, okImage());
    add('size_max_documented', { prompt: 'a cat', model: 'gpt-image-2.5-flare', size: '3840x2160' }, okImage());
    // The experimental band is reachable only by an ARBITRARY size: the
    // picker-list check returns FIRST, so `3840x2160` (which is on the list)
    // never gets the experimental DEBUG line. 3840x1280 is past the threshold,
    // /16 on both edges, exactly 3:1, and off the list.
    add('size_experimental_arbitrary', { prompt: 'a cat', model: 'gpt-image-2.5-sunburst', size: '3840x1280' }, okImage());
    // The four `it.each` failure shapes, one per rule in checkArbitrarySize.
    add('size_bad_edge_multiple', { prompt: 'a cat', model: 'gpt-image-2.5-sunburst', size: '1000x1000' }, okImage());
    add('size_bad_aspect', { prompt: 'a cat', model: 'gpt-image-2.5-sunburst', size: '3200x800' }, okImage());
    add('size_bad_edge', { prompt: 'a cat', model: 'gpt-image-2.5-sunburst', size: '3856x2160' }, okImage());
    add('size_bad_pixels', { prompt: 'a cat', model: 'gpt-image-2.5-sunburst', size: '3840x3840' }, okImage());
    // Not-a-size at all: the fifth reason.
    add('size_unparseable', { prompt: 'a cat', model: 'gpt-image-2.5-sunburst', size: 'wide' }, okImage());
    // gpt-image-1.5 takes only the standard list, so an arbitrary size falls back.
    add('size_arbitrary_refused_on_1_5', { prompt: 'a cat', model: 'gpt-image-1.5', size: '1536x864' }, okImage());
    add('size_standard_on_gpt_image_1', { prompt: 'a cat', model: 'gpt-image-1', size: '1024x1536' }, okImage());
    add('size_auto_on_gpt_image', { prompt: 'a cat', model: 'gpt-image-2.5-sunburst', size: 'auto' }, okImage());
    add('size_dalle3_ok', { prompt: 'a cat', model: 'dall-e-3', size: '1792x1024' }, okImage());
    // A GPT Image size on dall-e-3 is not on its list.
    add('size_dalle3_wrong', { prompt: 'a cat', model: 'dall-e-3', size: '1536x1024' }, okImage());
    // `!size` is truthy-tested, so an empty string takes the 1024x1024 default
    // WITHOUT a warning (the fallback log sits past the early return).
    add('size_empty_string', { prompt: 'a cat', model: 'gpt-image-2', size: '' }, okImage());

    // -- GPT Image output controls ---------------------------------------
    add('extras_all_four', { prompt: 'a cat', model: 'gpt-image-2.5-sunburst',
      profileParameters: { background: 'transparent', output_format: 'webp', output_compression: 80, moderation: 'low' } }, okImage());
    // Transparency wins over the requested jpeg; the mimeType follows the FORCE.
    add('extras_force_png_for_transparent_jpeg', { prompt: 'a cat', model: 'gpt-image-2.5-flare',
      profileParameters: { background: 'transparent', output_format: 'jpeg' } }, okImage());
    // webp is already alpha-capable, so nothing is forced.
    add('extras_transparent_with_webp', { prompt: 'a cat', model: 'gpt-image-2',
      profileParameters: { background: 'transparent', output_format: 'webp' } }, okImage());
    // No format at all: the force needs `outputFormat !== undefined`, so the
    // background rides alone and the mimeType stays png.
    add('extras_transparent_without_format', { prompt: 'a cat', model: 'gpt-image-2',
      profileParameters: { background: 'transparent' } }, okImage());
    add('extras_drop_compression_for_png', { prompt: 'a cat', model: 'gpt-image-2.5-sunburst',
      profileParameters: { output_format: 'png', output_compression: 50 } }, okImage());
    // …and with no format at all, the DEBUG says `png (default)`.
    add('extras_compression_without_format', { prompt: 'a cat', model: 'gpt-image-2',
      profileParameters: { output_compression: 50 } }, okImage());
    add('extras_drop_bad_values', { prompt: 'a cat', model: 'gpt-image-2.5-sunburst',
      profileParameters: { background: 'chartreuse', output_format: 'tiff', output_compression: 300, moderation: 'off' } }, okImage());
    add('extras_never_on_dalle', { prompt: 'a cat', model: 'dall-e-3',
      profileParameters: { background: 'transparent', output_format: 'webp', moderation: 'low' } }, okImage());
    // The mimeType follows the requested format on every arm.
    add('mime_webp', { prompt: 'a cat', model: 'gpt-image-2', profileParameters: { output_format: 'webp' } }, okImage());
    add('mime_jpeg', { prompt: 'a cat', model: 'gpt-image-2', profileParameters: { output_format: 'jpeg' } }, okImage());
    add('mime_unset_png', { prompt: 'a cat', model: 'gpt-image-2' }, okImage());
    // `readEnum`/`readIntInRange`'s own gates, which v4's suite never drives:
    // an empty string is "unset" (silently), a non-string enum is String()'d
    // into the warning, `Number('80')` is 80, 0 survives the `!== undefined`
    // test, and a fractional value fails `Number.isInteger`.
    add('extras_empty_strings_are_unset', { prompt: 'a cat', model: 'gpt-image-2',
      profileParameters: { background: '', output_format: '', moderation: '', output_compression: '' } }, okImage());
    add('extras_non_string_enum_values', { prompt: 'a cat', model: 'gpt-image-2',
      profileParameters: { background: 5, output_format: ['webp'], moderation: true } }, okImage());
    add('extras_numeric_string_compression', { prompt: 'a cat', model: 'gpt-image-2',
      profileParameters: { output_format: 'jpeg', output_compression: '80' } }, okImage());
    add('extras_zero_compression', { prompt: 'a cat', model: 'gpt-image-2',
      profileParameters: { output_format: 'webp', output_compression: 0 } }, okImage());
    add('extras_fractional_compression', { prompt: 'a cat', model: 'gpt-image-2',
      profileParameters: { output_format: 'webp', output_compression: 50.5 } }, okImage());
    add('extras_null_values_are_unset', { prompt: 'a cat', model: 'gpt-image-2',
      profileParameters: { background: null, output_compression: null } }, okImage());

    // -- family-specific parameters --------------------------------------
    add('style_not_on_dalle2', { prompt: 'a cat', model: 'dall-e-2' }, okImage());
    add('style_dropped_on_gpt_image', { prompt: 'a cat', model: 'gpt-image-2.5-sunburst', style: 'natural' }, okImage());
    // `params.style ?? 'vivid'` is NULLISH, not truthy: an empty string rides.
    add('style_empty_string_rides', { prompt: 'a cat', model: 'dall-e-3', style: '' }, okImage());
    add('n_capped_on_dalle3', { prompt: 'a cat', model: 'dall-e-3', n: 4 }, okImage());
    add('n_kept_on_sunburst', { prompt: 'a cat', model: 'gpt-image-2.5-sunburst', n: 4 }, okImage());
    // Math.max(requested, 1) raises a zero as well as capping a too-large one.
    add('n_zero_raised_to_one', { prompt: 'a cat', model: 'gpt-image-2', n: 0 }, okImage());
    // An unrecognised model is forwarded untouched — size, quality AND n.
    add('unknown_model_forwarded', { prompt: 'a cat', model: 'gpt-image-3-supernova', size: '4096x4096', quality: 'ludicrous' }, okImage());
    add('unknown_model_uncapped_n', { prompt: 'a cat', model: 'gpt-image-3-supernova', n: 50 }, okImage());
    // An unknown NON-gpt-image id still gets `response_format` and `style`,
    // because `isGptImageModel` falls back to the bare id prefix.
    add('unknown_non_gpt_model', { prompt: 'a cat', model: 'mystery-painter-9', size: '4096x4096' }, okImage());
    // No model at all: `requestParams.model = params.model` is the RAW value, so
    // the key is absent from the wire while `?? 'dall-e-3'` drove validation.
    add('no_model_at_all', { prompt: 'a cat', n: 1 }, okImage());
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
    // `ca22ec45`: Z.AI answers with URLs (valid ~30 days) while every Quilltap
    // consumer reads only base64, so the provider now DOWNLOADS each one. These
    // rows script a distinct binary answer for the follow-up GET.
    addDl('url_only', { prompt: 'a cat', model: 'glm-image', n: 1 },
      ok(200, { data: [{ url: 'https://z.ai/x.png' }] }),
      { status: 200, contentType: 'image/webp; charset=binary', bytes: 'AAECA/7/' });
    // b64_json present => NO download at all (and the mimeType stays image/png).
    add('both_b64_url', { prompt: 'a cat', model: 'cogview-4-250304', n: 1, quality: 'hd' },
      ok(200, { data: [{ b64_json: 'QUJD', url: 'https://z.ai/x.png', revised_prompt: 'rp' }] }));
    // A non-`image/` content type must NOT override the image/png default.
    addDl('url_only_non_image_ctype', { prompt: 'a cat', model: 'glm-image', n: 1 },
      ok(200, { data: [{ url: 'https://z.ai/y.bin' }] }),
      { status: 200, contentType: 'application/octet-stream', bytes: 'AAEC' });
    // A failed download throws with the HTTP status in the sentence.
    addDl('url_only_download_404', { prompt: 'a cat', model: 'glm-image', n: 1 },
      ok(200, { data: [{ url: 'https://z.ai/missing.png' }] }),
      { status: 404, contentType: 'text/plain', bytes: '' });
    // Neither field: the entry is rejected before any download is attempted.
    add('entry_without_data_or_url', { prompt: 'a cat', model: 'glm-image', n: 1 },
      ok(200, { data: [{ revised_prompt: 'rp' }] }));
    // z-ai has NO moderation handling: a generic 400 just surfaces the SDK message.
    add('generic_error', { prompt: 'x', model: 'glm-image', n: 1 },
      ok(400, { error: { message: 'Bad request' } }));
  } else if (provider === 'nanogpt') {
    // The b64 PIN: `response_format: 'b64_json'` rides EVERY request, including
    // the gpt-image-1.5 id that the OpenAI plugin deliberately exempts.
    add('happy_b64', { prompt: 'a cat', model: 'hidream', n: 1 },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    add('b64_pin_on_gpt_image_id', { prompt: 'a cat', model: 'gpt-image-1.5', n: 1 },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    // No model → hidream, NanoGPT's own server-side default made explicit.
    add('default_model', { prompt: 'a cat', n: 1 },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    // size rides VERBATIM and only when supplied; seed only when set.
    add('size_and_seed', { prompt: 'a cat', model: 'flux-2-pro', n: 1, size: '832x1248', seed: 42 },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    // A size v4 never validates — it is cast, not normalized.
    add('unvalidated_size', { prompt: 'a cat', model: 'recraft-v3', n: 1, size: '99x1' },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    // URL-only entries download into base64 (the same seam Z.AI uses).
    addDl('url_only', { prompt: 'a cat', model: 'hidream', n: 1 },
      ok(200, { data: [{ url: 'https://nano-gpt.com/x.png' }] }),
      { status: 200, contentType: 'image/webp; charset=binary', bytes: 'AAECA/7/' });
    // b64 present => NO download, mimeType stays image/png.
    add('both_b64_url', { prompt: 'a cat', model: 'hidream', n: 1 },
      ok(200, { data: [{ b64_json: 'QUJD', url: 'https://nano-gpt.com/x.png', revised_prompt: 'rp' }] }));
    // A non-`image/` content type must NOT override the default.
    addDl('url_only_non_image_ctype', { prompt: 'a cat', model: 'hidream', n: 1 },
      ok(200, { data: [{ url: 'https://nano-gpt.com/y.bin' }] }),
      { status: 200, contentType: 'application/octet-stream', bytes: 'AAEC' });
    // A failed download carries NanoGPT's own sentence + the HTTP status.
    addDl('url_only_download_404', { prompt: 'a cat', model: 'hidream', n: 1 },
      ok(200, { data: [{ url: 'https://nano-gpt.com/missing.png' }] }),
      { status: 404, contentType: 'text/plain', bytes: '' });
    // Neither field → NanoGPT's own rejection sentence.
    add('entry_without_data_or_url', { prompt: 'a cat', model: 'hidream', n: 1 },
      ok(200, { data: [{ revised_prompt: 'rp' }] }));
    // === 84f33ce94 + 648d5c8aa: the LoRA dialects and the passthrough bag ===
    // The flat model-specific controls the OpenAI SDK forwards verbatim.
    add('lora_extra_body_fields', { prompt: 'a cat', model: 'hidream', n: 1, guidanceScale: 3.5, steps: 28, negativePrompt: 'blurry' },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    // The passthrough allow-list: four keys ride, a blank is "unset" and is
    // SKIPPED, and anything off the list never reaches the wire.
    add('lora_passthrough_allowlist', { prompt: 'a cat', model: 'hidream', n: 1,
      profileParameters: { num_inference_steps: 20, guidance_scale: 2, steps: '', strength: 0.6, not_allowed: 'x', hf_api_token: 'tok', lora_preset: 'p' } },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    // indexed: lora_url_N / lora_scale_N; a scale-less adapter writes only the url.
    add('lora_indexed_two', { prompt: 'a cat', model: 'flux-2-dev-lora', n: 1,
      loras: [{ source: 'owner/one', scale: 0.8 }, { source: 'https://x.test/two.safetensors' }] },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    // The plugin caps too (the unknown-family safety net), with its OWN sentence.
    add('lora_indexed_over_cap', { prompt: 'a cat', model: 'flux-2-klein-4b', n: 1,
      loras: [{ source: 'a/1' }, { source: 'a/2' }, { source: 'a/3' }, { source: 'a/4' }] },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    // A longest-prefix child lands on its family's dialect.
    add('lora_indexed_prefix_child', { prompt: 'a cat', model: 'flux-2-dev-lora-image-to-image', n: 1,
      loras: [{ source: 'a/1', scale: 1.5 }] },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    // weights: lora_weights / lora_scale, and hf_api_token ONLY beside weights.
    add('lora_weights_with_token', { prompt: 'a cat', model: 'pruna-ai/p-image/edit-lora', n: 1,
      loras: [{ source: 'owner/gated', scale: 0.5 }],
      profileParameters: { hf_api_token: 'hf_secret', lora_preset: 'ignored-here' } },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    add('lora_weights_blank_token', { prompt: 'a cat', model: 'pruna-ai/p-image/text-to-image-lora', n: 1,
      loras: [{ source: 'owner/w' }], profileParameters: { hf_api_token: '' } },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    // BUG 110's opposite rule: a credential with no weights to fetch stays home.
    add('lora_weights_token_without_weights', { prompt: 'a cat', model: 'pruna-ai/p-image/edit-lora', n: 1,
      loras: [], profileParameters: { hf_api_token: 'hf_secret' } },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    // url: lora_url / lora_strength, plus the preset.
    add('lora_url_with_preset', { prompt: 'a cat', model: 'flux-lora', n: 1,
      loras: [{ source: 'https://fal.test/w.safetensors', scale: 1.2 }],
      profileParameters: { lora_preset: 'anime' } },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    // BUG 110 itself: a configured preset with NO adapter beside it. Before the
    // fix the empty-list early return threw it away in silence; after it, the
    // family is resolved FIRST and the preset stands alone.
    add('lora_preset_without_adapters', { prompt: 'a cat', model: 'flux-lora', n: 1,
      loras: [], profileParameters: { lora_preset: 'anime' } },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    // …and with no `loras` key at all, which is how every non-LoRA call site
    // reaches this code (v4's suite passed `undefined` here — the blind spot).
    add('lora_preset_no_loras_key', { prompt: 'a cat', model: 'flux-lora', n: 1,
      profileParameters: { lora_preset: 'anime' } },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    // A known family with nothing configured still REPORTS its dialect —
    // "nothing was configured" and "nothing could be spelled" are different
    // diagnoses. (Visible in the log fields, not the body: the body is bare.)
    add('lora_known_family_empty', { prompt: 'a cat', model: 'z-image-turbo-lora', n: 1, loras: [] },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    // An unknown family writes NOTHING — not the adapters, not the preset.
    add('lora_unknown_family_drops', { prompt: 'a cat', model: 'hidream', n: 1,
      loras: [{ source: 'a/1' }, { source: 'a/2' }],
      profileParameters: { lora_preset: 'anime', hf_api_token: 'tok' } },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    add('lora_unknown_family_empty', { prompt: 'a cat', model: 'recraft-v3', n: 1, loras: [],
      profileParameters: { lora_preset: 'anime' } },
      ok(200, { data: [{ b64_json: 'QUJD' }] }));
    // BUG 111: the wrapped generate call logs the composed body's key NAMES at
    // error before rethrowing the SDK error UNCHANGED.
    add('lora_request_failed', { prompt: 'a cat', model: 'flux-lora', n: 1,
      loras: [{ source: 'https://fal.test/w.safetensors', scale: 1.2 }],
      profileParameters: { lora_preset: 'anime', num_inference_steps: 20 } },
      ok(400, { error: { message: 'try a different prompt or image' } }));
    add('generic_error', { prompt: 'x', model: 'hidream', n: 1 },
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
    // The `ca22ec45` routing widening: a live-fetched `gemini*` id that is NOT in
    // GEMINI_IMAGE_MODELS. The pre-widening predicate routed it to the Imagen
    // `predict` endpoint (which serves only imagen-*); it must now build a
    // `:generateContent` request and parse the Gemini candidates shape.
    add('gemini_live_fetched_id', { prompt: 'a cat', model: 'gemini-2.0-flash-preview-image-generation', n: 1 },
      ok(200, { candidates: [{ content: { parts: [{ inlineData: { data: 'QUJD', mimeType: 'image/png' } }] } }] }));
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

/**
 * A `@openrouter/sdk`-schema-valid `Model` row. Every field `Model$inboundSchema`
 * marks required is present, so the SDK's zod parse succeeds and the recorded
 * outcome measures v4's DISCOVERY rather than the SDK's validator.
 */
function orModel(id, extra = {}) {
  return {
    id,
    canonical_slug: id,
    name: id,
    created: 1,
    context_length: 8192,
    default_parameters: null,
    per_request_limits: null,
    supported_voices: null,
    supported_parameters: [],
    links: { details: `https://openrouter.ai/${id}` },
    pricing: { prompt: '0', completion: '0' },
    top_provider: { is_moderated: false },
    architecture: { input_modalities: ['text'], modality: 'text->text', output_modalities: ['text'] },
    ...extra,
  };
}

/** A schema-valid `ModelsListResponse` page (one page; `links.next` is null). */
function orListPage(models) {
  return { data: models, links: { next: null }, total_count: models.length };
}

/**
 * The `ca22ec45` keyed model-discovery cases. Each drives the plugin's REAL
 * `getAvailableModels(apiKey?)`; `responses` is the SEQUENCE the mocked fetch
 * answers (google and openrouter page, so a case may consume more than one).
 */
function modelCasesFor(provider) {
  const c = [];
  const add = (name, withKey, responses) => c.push({ name, withKey, responses });
  const j = (status, obj) => ({ status, body: JSON.stringify(obj) });

  // Every provider: no key => the curated static list, ZERO requests.
  add('models_static', false, []);

  if (provider === 'openai') {
    // The /v1/models filter is /^(dall-e|gpt-image)/ then .sort().
    add('models_live', true, [
      j(200, { object: 'list', data: [
        { id: 'gpt-4o' },
        { id: 'dall-e-3' },
        { id: 'gpt-image-1-mini' },
        { id: 'text-embedding-3-small' },
        { id: 'gpt-image-1' },
        { id: 'dall-e-2' },
      ] }),
    ]);
    add('models_live_empty', true, [
      j(200, { object: 'list', data: [{ id: 'gpt-4o' }, { id: 'o3' }] }),
    ]);
    add('models_http_error', true, [
      j(401, { error: { message: 'Incorrect API key provided: test-****-key.', type: 'invalid_request_error', code: 'invalid_api_key' } }),
    ]);
    // A non-JSON error body: measures the SDK's message FALLBACK, which the port
    // has to reproduce because the host wire hands back the raw status + body
    // where v4 gets a thrown `APIError`.
    add('models_http_error_bare', true, [{ status: 400, body: 'service unavailable' }]);
  } else if (provider === 'nanogpt') {
    // The filter is the capability FLAG, strictly `=== true`, not the id — the
    // listing also carries edit-only and upscale-only entries. The curated six
    // are then UNIONED in and the whole thing sorted, so this arm has no
    // empty-throw.
    add('models_live', true, [
      j(200, { data: [
        { id: 'flux-2-pro', capabilities: { image_generation: true } },
        { id: 'some-upscaler', capabilities: { image_generation: false } },
        { id: 'edit-only-model', capabilities: { image_edit: true } },
        { id: 'aurora-x', capabilities: { image_generation: true } },
        { id: 'no-capabilities-block' },
        { id: 'truthy-not-true', capabilities: { image_generation: 1 } },
      ] }),
    ]);
    // Nothing passes the filter: the union still answers the curated six.
    add('models_live_none_pass', true, [
      j(200, { data: [{ id: 'x', capabilities: { image_generation: false } }] }),
    ]);
    // A malformed payload (no `data` array) behaves as an empty page.
    add('models_live_no_data', true, [j(200, { notdata: [] })]);
    // A raw fetch, so a non-ok status is NanoGPT's OWN sentence, not an SDK error.
    add('models_http_error', true, [
      j(500, { error: { message: 'upstream exploded' } }),
    ]);
  } else if (provider === 'google') {
    add('models_live', true, [
      j(200, { models: [
        { name: 'models/imagen-4.0-generate-001', supportedGenerationMethods: ['predict'] },
        { name: 'models/gemini-2.0-flash-preview-image-generation', supportedGenerationMethods: ['generateContent', 'countTokens'] },
        { name: 'models/gemini-2.5-flash', supportedGenerationMethods: ['generateContent'] },
        { name: 'models/veo-3.0-generate-001', supportedGenerationMethods: ['predictLongRunning'] },
        { name: 'models/text-embedding-004', supportedGenerationMethods: ['embedContent'] },
        { name: 'models/imagen-3.0-fast-generate-001', supportedGenerationMethods: ['predict'] },
        { name: 'models/imagen-4.0-ultra-generate-001' },
        { name: '' },
      ] }),
    ]);
    // Two pages: the do/while continues while nextPageToken is present.
    add('models_paged', true, [
      j(200, { models: [{ name: 'models/imagen-4.0-generate-001', supportedGenerationMethods: ['predict'] }], nextPageToken: 'page-2' }),
      j(200, { models: [{ name: 'models/gemini-2.5-flash-image', supportedGenerationMethods: ['generateContent'] }] }),
    ]);
    add('models_http_error', true, [{ status: 403, body: 'forbidden' }]);
    add('models_empty', true, [
      j(200, { models: [{ name: 'models/gemini-2.5-flash', supportedGenerationMethods: ['generateContent'] }] }),
    ]);
  } else if (provider === 'grok') {
    // The documented top-level key has shifted between `models` and `data`;
    // both are accepted, and every non-empty alias joins the id set.
    // `grok-imagine-image` appears BOTH as an id and as a later row's alias, and
    // `grok-2-image` twice over, so the Set's dedup is a measured comparand and
    // not an accident of the payload.
    add('models_live_models_key', true, [
      j(200, { models: [
        { id: 'grok-imagine-image', aliases: ['grok-image', ''] },
        { id: 'grok-2-image-1212', aliases: ['grok-2-image', 'grok-2-image'] },
        { id: 'grok-2-image', aliases: ['grok-imagine-image'] },
        { aliases: ['orphan-alias'] },
        {},
      ] }),
    ]);
    add('models_live_data_key', true, [
      j(200, { data: [{ id: 'grok-imagine-image-pro' }, { id: 'grok-imagine-image' }] }),
    ]);
    add('models_http_error', true, [{ status: 500, body: 'boom' }]);
    add('models_empty', true, [j(200, { models: [] })]);
  } else if (provider === 'z-ai') {
    // IMAGE_GEN_MODEL_PATTERN = /^(cogview|glm-image)/i, UNIONED with the two
    // static ids — so this list can never come back empty and never throws.
    add('models_live_union', true, [
      j(200, { object: 'list', data: [
        { id: 'glm-4.6' },
        { id: 'cogview-3' },
        { id: 'GLM-Image-X' },
        { id: 'glm-4.6v' },
        { id: 'cogview-4-250304' },
      ] }),
    ]);
    add('models_live_none_matching', true, [
      j(200, { object: 'list', data: [{ id: 'glm-4.6' }] }),
    ]);
    add('models_http_error', true, [
      j(401, { error: { message: 'invalid api key', code: '1002' } }),
    ]);
    add('models_http_error_bare', true, [{ status: 400, body: 'service unavailable' }]);
  } else if (provider === 'openrouter') {
    // ⚠ The payloads here are SCHEMA-VALID `ModelsListResponse` bodies, because
    // `@openrouter/sdk` zod-parses the page before the plugin sees it — a thin
    // hand-rolled row fails with "Response validation failed" and would measure
    // the SDK's validator instead of v4's discovery.
    //
    // These four rows carry, between them, EVERY signal v4's discovery reads:
    // (A) `architecture.output_modalities` (the one field genuinely in
    // `Model$inboundSchema`), (B) model-level `output_modalities`, (C)
    // `architecture.outputModality` (singular), (D)
    // `supported_generation_methods`. The recorded v4 answer is the finding: the
    // SDK's `z.object` STRIPS (B)/(C)/(D) as unknown keys and REMAPS (A) to
    // `architecture.outputModalities` (plural) — so all three of v4's arms
    // read `undefined` and discovery throws even on a maximally-image payload.
    add('models_live_every_signal', true, [j(200, orListPage([
      orModel('arch/image-out', { architecture: { input_modalities: ['text'], modality: 'text->image', output_modalities: ['image', 'text'] } }),
      orModel('wire/output-modalities', { output_modalities: ['image', 'text'] }),
      orModel('wire/arch-output-modality', { architecture: { input_modalities: ['text'], modality: 'text->text', output_modalities: ['text'], outputModality: 'text+image' } }),
      orModel('wire/gen-methods', { supported_generation_methods: ['image'] }),
      orModel('plain/text-only'),
    ]))]);
    add('models_empty_page', true, [j(200, orListPage([]))]);
    // 401 (not a 5xx): the SDK retries 5xx with backoff, which would make the
    // scripted response sequence — and the recorded request count — a timing
    // artifact rather than a contract.
    add('models_http_error', true, [
      j(401, { error: { message: 'No auth credentials found', code: 401 } }),
    ]);
  }
  return c;
}

/**
 * Normalize whatever header carrier `fetch` was handed into a plain lowercase
 * object, so an SDK-built `Request` and a raw `{headers}` init record alike.
 */
function headersOf(url, init) {
  const src = (init && init.headers) || (url && url.headers) || null;
  const out = {};
  if (!src) return out;
  if (typeof src.forEach === 'function' && !Array.isArray(src)) {
    src.forEach((v, k) => { out[String(k).toLowerCase()] = String(v); });
    return out;
  }
  if (Array.isArray(src)) {
    for (const [k, v] of src) out[String(k).toLowerCase()] = String(v);
    return out;
  }
  for (const [k, v] of Object.entries(src)) out[String(k).toLowerCase()] = String(v);
  return out;
}

/**
 * The scripted answer to a follow-up image download: raw bytes (given as base64
 * in the case so the fixture stays diffable) under an explicit content type.
 */
function makeBinaryResponse(dl) {
  const bytes = Buffer.from(dl.bytes ?? '', 'base64');
  const headers = {};
  if (dl.contentType) headers['content-type'] = dl.contentType;
  return new Response(dl.status === 200 ? bytes : '', { status: dl.status, headers });
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
    console.error('usage: --provider <openai|google|grok|openrouter|z-ai|nanogpt> --out <ndjson>');
    process.exit(1);
  }
  const spec = PROVIDERS[provider];
  const lines = [];

  for (const c of casesFor(provider)) {
    let captured = null;
    const followUps = [];
    const origFetch = globalThis.fetch;
    globalThis.fetch = async (url, init) => {
      const u = typeof url === 'string' ? url : (url && url.url) || String(url);
      // The generate call is a POST built by the plugin/SDK; a follow-up image
      // download is a BARE `fetch(url)` with no init at all, which is a GET.
      const method = (init && init.method) || (url && url.method) || (captured ? 'GET' : 'POST');
      let body = (init && init.body) || (url && url.body) || null;
      if (body && typeof body !== 'string') {
        try { body = new TextDecoder().decode(body); } catch { body = String(body); }
      }
      if (!captured) {
        captured = { method, url: u, body };
        return makeResponse(c.wire.status, c.wire.body);
      }
      // A follow-up request: the `ca22ec45` Z.AI image download. Recorded with
      // its headers so the "bare fetch, no headers" contract is measurable.
      followUps.push({ method, url: u, body: body ?? null, headers: headersOf(url, init) });
      if (!c.download) {
        throw new Error(`recorder: case ${c.name} made an unscripted follow-up request to ${u}`);
      }
      return makeBinaryResponse(c.download);
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
        ...(c.download ? { download: c.download } : {}),
        downloadRequests: followUps,
        outcome,
        images,
        thrown,
        isModeration: outcome === 'thrown' ? isImageModerationError(thrown) : false,
      })
    );
  }

  // === ca22ec45: the keyed model-discovery rows ===
  for (const c of modelCasesFor(provider)) {
    const requests = [];
    let served = 0;
    const origFetch = globalThis.fetch;
    globalThis.fetch = async (url, init) => {
      const u = typeof url === 'string' ? url : (url && url.url) || String(url);
      const method = (init && init.method) || (url && url.method) || 'GET';
      let body = (init && init.body) || (url && url.body) || null;
      if (body && typeof body !== 'string') {
        try { body = new TextDecoder().decode(body); } catch { body = String(body); }
      }
      requests.push({ method, url: u, body: body ?? null, headers: headersOf(url, init) });
      // Deliberately NOT a repeating last-response: a plugin (or SDK) that
      // keeps asking past the scripted sequence is a runaway page loop, and it
      // must fail loudly here rather than hang the recorder.
      const r = c.responses[served];
      served += 1;
      if (!r) throw new Error(`recorder: case ${c.name} requested more responses than scripted (${u})`);
      return makeResponse(r.status, r.body);
    };

    let outcome = 'ok';
    let models = null;
    let thrown = null;
    let supportedModels = null;
    try {
      const inst = await spec.make();
      supportedModels = [...inst.supportedModels];
      models = await inst.getAvailableModels(c.withKey ? 'test-api-key' : undefined);
    } catch (e) {
      outcome = 'thrown';
      thrown = e instanceof Error ? e.message : String(e);
    } finally {
      globalThis.fetch = origFetch;
    }

    lines.push(
      JSON.stringify({
        kind: 'models',
        provider,
        case: c.name,
        withKey: c.withKey,
        supportedModels,
        requests,
        wire: c.responses,
        outcome,
        models,
        thrown,
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
