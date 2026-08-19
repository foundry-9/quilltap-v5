/**
 * Generator: the nine built-in provider manifests (wave 4 / W4.7a).
 *
 * Transcribes v4's registered provider plugin metadata into a declarative JSON
 * manifest per provider (the 'gen-tool-catalog.mjs' precedent — transcription,
 * not re-derivation). Every field the v4 provider-registry convenience getters
 * read is pulled directly off the built plugin object, so the manifest is a
 * faithful copy of v4's runtime data; the port then answers the getters from
 * these manifests and the 'provider_registry_equivalence' differential proves
 * they agree byte-for-byte with v4.
 *
 * The 'streamDecoder' / 'requestTransform' / 'endpoints' / 'auth' / 'baseUrl'
 * fields are NOT carried on the plugin metadata object (they live in the
 * provider's imperative code, ported by W4.7b/c). They are supplied here from
 * the fixed AUGMENTATION table below: the decoder/transform enum values are the
 * closed sets from 'docs/developer/porting/provider-manifest.md' ("The five
 * stream decoders" + "the request-transform hooks"); the endpoints/auth/baseUrl
 * are transcribed from each provider plugin bundle. W4.7b/c
 * refine endpoints/auth against recorded wire fixtures; they are not
 * differential-checked here (v4 exposes no registry getter for them).
 *
 * The 'imageGenerationModels' field (P4.6p) is emitted for exactly the providers
 * whose plugin declares `capabilities.imageGeneration` — google, grok, openai,
 * openrouter, z_ai. Three of them expose the list on the built plugin object
 * (`plugin.getImageGenerationModels().map(m => m.id)` — openai, google,
 * openrouter); grok and z-ai carry NO such getter, and their only declaration is
 * `supportedModels` in the `image-provider.ts` source that ships inside
 * `plugins/dist/<dir>/`. Those two are read from that source at generation time
 * (an array literal on grok's class; a module-local const on z-ai's), and an
 * unrecognized shape is a LOUD non-zero exit naming the provider — so the
 * generator stays self-updating rather than holding a second copy that rots.
 * Repaired 2026-08-05 by P4.39; P4.6p had added the field to the committed
 * manifests by hand and never taught the generator, so running the recipe below
 * silently DELETED it from those five files (`#[serde(default)]` Rust-side, so
 * the only symptom was a blank `defaultModels` in `imageProfileList`). The regen
 * recipe below is safe to run straight into the manifest dir again.
 *
 * Repaired AGAIN 2026-08-15 by P4.D78, same class: P4.47 (B) corrected google's
 * `auth` to the `x-goog-api-key` HEADER in the committed manifest and never
 * taught the AUGMENTATION table below, so the first regen after it would have
 * reverted the fix. **When a hand-fix lands in a committed manifest, fix this
 * table in the same commit** — the augmentation fields are exactly the ones no
 * v4 getter can re-derive, so nothing else will catch the drift.
 *
 * Regen recipe (after a v4 provider-metadata drift):
 *   cd ~/source/quilltap-server
 *   node ~/source/quilltap-v5/harness/oracle/providers/gen-provider-manifests.mjs \
 *     ~/source/quilltap-v5/crates/quilltap-core/src/provider_manifest/manifests
 *   git -C ~/source/quilltap-v5 status --short   # expect clean on no drift
 *   (then re-run `provider_registry_equivalence` to confirm)
 *
 * The generator MUST run from the quilltap-server checkout root (it loads the
 * built plugins/dist bundles via createRequire).
 */

import { readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join } from 'node:path';

const nodeRequire = createRequire(join(process.cwd(), 'noop.js'));

const [, , outDir] = process.argv;
if (!outDir) {
  console.error('usage: gen-provider-manifests.mjs <out-manifests-dir>');
  console.error('  (run from the quilltap-server checkout root)');
  process.exit(1);
}

/**
 * The nine built-in providers, in registry-registration order (the order the
 * plugin dirs are loaded), with their 'plugins/dist' directory name and the
 * decoder/transform/wire augmentation that does not live on the metadata object.
 */
const PROVIDERS = [
  {
    dir: 'qtap-plugin-anthropic',
    aug: {
      baseUrl: 'https://api.anthropic.com/v1',
      endpoints: { chat: '/messages', models: '/models' },
      auth: { kind: 'header', header: 'x-api-key', extra: { 'anthropic-version': '2023-06-01' } },
      streamDecoder: 'anthropic-sse',
      requestTransform: 'anthropic',
    },
  },
  {
    dir: 'qtap-plugin-openai',
    aug: {
      baseUrl: 'https://api.openai.com/v1',
      endpoints: { chat: '/responses', models: '/models' },
      auth: { kind: 'bearer' },
      streamDecoder: 'responses-api-sse',
      requestTransform: 'openai',
    },
  },
  {
    dir: 'qtap-plugin-google',
    aug: {
      baseUrl: 'https://generativelanguage.googleapis.com/v1beta',
      endpoints: { chat: '/models', models: '/models' },
      // P4.47 (B), re-applied here by P4.D78: v4 builds
      // `new GoogleGenAI({apiKey, httpOptions})` and the SDK sets
      // `X-Goog-Api-Key` on every request, leaving the url untouched (the only
      // `?key=` in the whole plugin is the unused Live/WebSocket path). P4.47
      // corrected the COMMITTED manifest but not this table, so re-running the
      // recipe silently reverted the fix — the P4.39 `imageGenerationModels`
      // class of rot. Pinned in both directions by
      // `request_builder_google_wire_equivalence`.
      auth: { kind: 'header', header: 'x-goog-api-key' },
      streamDecoder: 'google-parts',
      requestTransform: 'google',
    },
  },
  {
    dir: 'qtap-plugin-grok',
    aug: {
      baseUrl: 'https://api.x.ai/v1',
      endpoints: { chat: '/responses', models: '/models' },
      auth: { kind: 'bearer' },
      streamDecoder: 'responses-api-sse',
      requestTransform: 'none',
    },
  },
  {
    dir: 'qtap-plugin-deepseek',
    aug: {
      baseUrl: 'https://api.deepseek.com',
      endpoints: { chat: '/chat/completions', models: '/models' },
      auth: { kind: 'bearer' },
      streamDecoder: 'chat-completions-sse',
      requestTransform: 'deepseek',
    },
  },
  {
    dir: 'qtap-plugin-z-ai',
    aug: {
      baseUrl: 'https://api.z.ai/api/paas/v4',
      endpoints: { chat: '/chat/completions', models: '/models' },
      auth: { kind: 'bearer' },
      streamDecoder: 'chat-completions-sse',
      requestTransform: 'none',
    },
  },
  {
    dir: 'qtap-plugin-openrouter',
    aug: {
      baseUrl: 'https://openrouter.ai/api/v1',
      endpoints: { chat: '/chat/completions', models: '/models' },
      auth: { kind: 'bearer' },
      streamDecoder: 'chat-completions-sse',
      requestTransform: 'none',
    },
  },
  {
    dir: 'qtap-plugin-ollama',
    aug: {
      baseUrl: 'http://localhost:11434',
      endpoints: { chat: '/api/chat', models: '/api/tags' },
      auth: { kind: 'none' },
      streamDecoder: 'ollama-ndjson',
      requestTransform: 'none',
    },
  },
  {
    dir: 'qtap-plugin-openai-compatible',
    aug: {
      baseUrl: 'http://localhost:8080/v1',
      endpoints: { chat: '/chat/completions', models: '/models' },
      auth: { kind: 'bearer' },
      streamDecoder: 'chat-completions-sse',
      requestTransform: 'none',
    },
  },
];

/** v4 registry defaults (provider-registry.ts convenience getters). */
const DEFAULT_CHARS_PER_TOKEN = 3.5;
const DEFAULT_CONTEXT_WINDOW = 8192;
const DEFAULT_TOOL_FORMAT = 'openai';

/** Load a built plugin's 'plugin' object. */
function loadPlugin(dir) {
  const mod = nodeRequire(join(process.cwd(), 'plugins', 'dist', dir, 'index.js'));
  const plugin = mod.plugin || mod.default?.plugin || mod.default;
  if (!plugin || !plugin.metadata) {
    throw new Error(`plugin ${dir} has no .plugin export with metadata`);
  }
  return plugin;
}

/** Die with a named, non-zero-exit complaint — never emit a silently stale field. */
function fail(message) {
  console.error(`gen-provider-manifests: ${message}`);
  process.exit(1);
}

/**
 * Parse a plain string-array literal (`['a', 'b']`). Anything that is not a flat
 * list of quoted, escape-free strings fails loudly rather than being coerced.
 */
function parseStringArrayLiteral(literal, where) {
  const strings = /'([^'\\]*)'|"([^"\\]*)"/g;
  const items = [];
  let m;
  while ((m = strings.exec(literal)) !== null) items.push(m[1] ?? m[2]);
  const residue = literal.replace(strings, '').replace(/[[\],\s]/g, '');
  if (residue !== '') {
    fail(`${where}: not a plain string-array literal (leftover ${JSON.stringify(residue)})`);
  }
  if (items.length === 0) fail(`${where}: empty string-array literal`);
  return items;
}

/**
 * Read a plugin's image model list out of the `image-provider.ts` source that
 * ships in its dist dir — the only declaration grok and z-ai have (no
 * `getImageGenerationModels` getter on either built plugin object).
 *
 * Two shapes are recognized, matching what those two sources carry today:
 *   readonly supportedModels = ['a', 'b'];          (grok)
 *   readonly supportedModels = SUPPORTED_MODELS;    (z-ai, + a module-local const)
 * Anything else exits non-zero naming the provider.
 */
function readSupportedModelsFromSource(dir) {
  const srcPath = join(process.cwd(), 'plugins', 'dist', dir, 'image-provider.ts');
  let src;
  try {
    src = readFileSync(srcPath, 'utf-8');
  } catch {
    fail(`${dir}: declares imageGeneration but has no getter and no ${srcPath}`);
  }

  const decl = /^\s*readonly\s+supportedModels\s*(?::[^=]*)?=\s*([^;]+);/m.exec(src);
  if (!decl) fail(`${dir}: no 'readonly supportedModels = …;' in image-provider.ts`);
  let rhs = decl[1].trim();

  if (!rhs.startsWith('[')) {
    // A bare identifier — resolve the one module-local `const NAME = [...]`.
    if (!/^[A-Za-z_$][\w$]*$/.test(rhs)) {
      fail(`${dir}: unrecognized supportedModels shape ${JSON.stringify(rhs)}`);
    }
    const constDecl = new RegExp(`^\\s*const\\s+${rhs}\\s*(?::[^=]*)?=\\s*(\\[[^\\]]*\\])`, 'm').exec(src);
    if (!constDecl) fail(`${dir}: supportedModels names '${rhs}', but no 'const ${rhs} = [...]' in image-provider.ts`);
    rhs = constDecl[1];
  }

  return parseStringArrayLiteral(rhs, `${dir} supportedModels`);
}

/**
 * The image-generation model ids for a provider, or null when the plugin does
 * not declare the capability (the four text-only built-ins carry no such key).
 * The built plugin's getter is authoritative where it exists — for google and
 * openrouter it deliberately differs from the source's `supportedModels` order.
 */
function resolveImageGenerationModels(plugin, dir) {
  const declaresCapability = !!plugin.capabilities.imageGeneration;
  const hasGetter = typeof plugin.getImageGenerationModels === 'function';

  if (!declaresCapability) {
    if (hasGetter) fail(`${dir}: exposes getImageGenerationModels but capabilities.imageGeneration is false`);
    return null;
  }

  const ids = hasGetter
    ? plugin.getImageGenerationModels().map((m) => m.id)
    : readSupportedModelsFromSource(dir);

  if (!Array.isArray(ids) || ids.length === 0) fail(`${dir}: resolved an empty image-generation model list`);
  for (const id of ids) {
    if (typeof id !== 'string' || id === '') fail(`${dir}: non-string image-generation model id ${JSON.stringify(id)}`);
  }
  return ids;
}

/**
 * The plugin's connection-profile options schema, or null when it declares none
 * (P4.D83, v4 `93ed8abf`). Getter-derived, so it is EXTRACTED rather than
 * transcribed — `byte-exact-static-data-transcription`: ship the generator, not
 * a hand-copied tree that rots on v4's next field.
 *
 * v4's route wraps the call in try/catch and falls back to `undefined`; here a
 * THROW is a loud non-zero exit instead, because a generator that silently emits
 * `null` for a provider whose schema merely failed to build would bake the
 * failure into a committed file.
 */
function resolveOptionsSchema(plugin, dir) {
  if (typeof plugin.getProviderOptionsSchema !== 'function') return null;
  let schema;
  try {
    schema = plugin.getProviderOptionsSchema();
  } catch (err) {
    fail(`${dir}: getProviderOptionsSchema threw — ${err instanceof Error ? err.message : String(err)}`);
  }
  if (schema === undefined || schema === null) return null;
  if (typeof schema !== 'object' || !Array.isArray(schema.groups)) {
    fail(`${dir}: getProviderOptionsSchema returned ${JSON.stringify(schema)}, not { groups: [...] }`);
  }
  return schema;
}

/**
 * The committed manifests carry `imageGenerationModels` on ONE line where every
 * other array is pretty-printed. Stringify through a sentinel so the field keeps
 * its schema position and the inline rendering is exact.
 */
const INLINE_SENTINEL = '@@IMAGE_GENERATION_MODELS@@';

function serializeManifest(manifest) {
  const inlineIds = manifest.imageGenerationModels;
  if (inlineIds === null) {
    // Absent, not null: the four text-only providers have no such key at all.
    delete manifest.imageGenerationModels;
    return JSON.stringify(manifest, null, 2) + '\n';
  }

  manifest.imageGenerationModels = INLINE_SENTINEL;
  const json = JSON.stringify(manifest, null, 2);
  const needle = JSON.stringify(INLINE_SENTINEL);
  if (!json.includes(needle)) fail(`${manifest.id}: inline sentinel never reached the output`);
  const inline = `[${inlineIds.map((id) => JSON.stringify(id)).join(', ')}]`;
  return json.replace(needle, inline) + '\n';
}

/**
 * Build the manifest object for a provider. The field ORDER here is the manifest
 * schema order (mirrored by the Rust serde structs); we do NOT sort keys.
 */
function buildManifest(provider) {
  const plugin = loadPlugin(provider.dir);
  const meta = plugin.metadata;
  const cfg = plugin.config || {};
  const models = plugin.getModelInfo ? plugin.getModelInfo() : [];

  // pricing: the STATIC fallback tier — whatever pricing rows the plugin's
  // getModelInfo declares (currently none on any built-in; W4.7e brings the live
  // fetcher). Faithful transcription: emit only models that carry a pricing row.
  const pricing = {};
  for (const m of models) {
    if (m.pricing) pricing[m.id] = { input: m.pricing.input, output: m.pricing.output };
  }

  const aug = provider.aug;

  return {
    schemaVersion: 1,
    id: meta.providerName,
    displayName: meta.displayName,
    description: meta.description,
    abbreviation: meta.abbreviation,
    colors: meta.colors,
    legacyNames: meta.legacyNames ?? [],
    auth: aug.auth,
    baseUrl: aug.baseUrl,
    endpoints: aug.endpoints,
    streamDecoder: aug.streamDecoder,
    requestTransform: aug.requestTransform,
    toolFormat: plugin.toolFormat ?? DEFAULT_TOOL_FORMAT,
    capabilities: {
      chat: !!plugin.capabilities.chat,
      imageGeneration: !!plugin.capabilities.imageGeneration,
      embeddings: !!plugin.capabilities.embeddings,
      webSearch: !!plugin.capabilities.webSearch,
      toolUse: !!plugin.capabilities.toolUse,
    },
    optionsSchema: resolveOptionsSchema(plugin, provider.dir),
    configRequirements: {
      requiresApiKey: !!cfg.requiresApiKey,
      // v4 bug 81: `acceptsApiKey` is OPTIONAL — omitted means "the same answer
      // as `requiresApiKey`", and v4's own manifest.json omits it for every
      // plugin but OpenAI-Compatible. Emitted only when the plugin config
      // declares it, so the eight non-declaring manifests stay byte-identical
      // and the Rust `Option<bool>` keeps the fallback in one place.
      ...(cfg.acceptsApiKey !== undefined ? { acceptsApiKey: !!cfg.acceptsApiKey } : {}),
      requiresBaseUrl: !!cfg.requiresBaseUrl,
      apiKeyLabel: cfg.apiKeyLabel ?? null,
      baseUrlLabel: cfg.baseUrlLabel ?? null,
      baseUrlPlaceholder: cfg.baseUrlPlaceholder ?? null,
      baseUrlDefault: cfg.baseUrlDefault ?? null,
    },
    messageFormat: {
      supportsNameField: !!plugin.messageFormat?.supportsNameField,
      supportedRoles: plugin.messageFormat?.supportedRoles ?? [],
      maxNameLength: plugin.messageFormat?.maxNameLength ?? null,
    },
    cheapModels: plugin.cheapModels
      ? {
          defaultModel: plugin.cheapModels.defaultModel,
          recommendedModels: plugin.cheapModels.recommendedModels ?? [],
        }
      : null,
    attachment: {
      supportsAttachments: !!plugin.attachmentSupport?.supportsAttachments,
      supportedMimeTypes: plugin.attachmentSupport?.supportedMimeTypes ?? [],
      description: plugin.attachmentSupport?.description ?? '',
      notes: plugin.attachmentSupport?.notes ?? null,
      maxFileSize: plugin.attachmentSupport?.maxFileSize ?? null,
      maxBase64Size: plugin.attachmentSupport?.maxBase64Size ?? null,
      maxFiles: plugin.attachmentSupport?.maxFiles ?? null,
    },
    charsPerToken: plugin.charsPerToken ?? DEFAULT_CHARS_PER_TOKEN,
    defaultContextWindow: plugin.defaultContextWindow ?? DEFAULT_CONTEXT_WINDOW,
    imageGenerationModels: resolveImageGenerationModels(plugin, provider.dir),
    fallbackModels: models.map((m) => m.id),
    pricing,
  };
}

// Build all nine BEFORE writing any: a loud failure (an unrecognized
// `supportedModels` shape, say) must not leave a half-overwritten manifest dir.
const rendered = PROVIDERS.map((provider) => {
  const manifest = buildManifest(provider);
  return { id: manifest.id, path: join(outDir, `${manifest.id.toLowerCase()}.json`), text: serializeManifest(manifest) };
});

for (const { id, path, text } of rendered) {
  writeFileSync(path, text, 'utf-8');
  console.error(`wrote ${path} (${id})`);
}
console.error(`generated ${rendered.length} manifests`);
