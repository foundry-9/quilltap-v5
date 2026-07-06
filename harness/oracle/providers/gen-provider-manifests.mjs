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
 * Regen recipe (after a v4 provider-metadata drift):
 *   cd ~/source/quilltap-server
 *   node ~/source/quilltap-v5/harness/oracle/providers/gen-provider-manifests.mjs \
 *     ~/source/quilltap-v5/crates/quilltap-core/src/provider_manifest/manifests
 *   (then re-run the differential to confirm)
 *
 * The generator MUST run from the quilltap-server checkout root (it loads the
 * built plugins/dist bundles via createRequire).
 */

import { writeFileSync } from 'node:fs';
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
      auth: { kind: 'query', param: 'key' },
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
    configRequirements: {
      requiresApiKey: !!cfg.requiresApiKey,
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
    fallbackModels: models.map((m) => m.id),
    pricing,
  };
}

for (const provider of PROVIDERS) {
  const manifest = buildManifest(provider);
  const filename = `${manifest.id.toLowerCase()}.json`;
  const outPath = join(outDir, filename);
  writeFileSync(outPath, JSON.stringify(manifest, null, 2) + '\n', 'utf-8');
  console.error(`wrote ${outPath} (${manifest.id})`);
}
console.error(`generated ${PROVIDERS.length} manifests`);
