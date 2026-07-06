/**
 * Oracle case (wave 4 / W4.7a): the provider-registry convenience getters.
 *
 * Drives v4's REAL `lib/plugins/provider-registry` getters over EVERY registered
 * provider, after initializing the registry the way v4's runtime does — loading
 * the nine built plugins/dist bundles and calling `initializeProviderRegistry`
 * (the same providers[], minus the scan machinery, that
 * `lib/startup/plugin-initialization.ts` feeds it). Emits one NDJSON row per
 * (provider × getter). The Rust side answers from the generated manifests; the
 * `provider_registry_equivalence` differential diffs field-by-field, tier-1
 * exact — including the defaults for absent fields, the legacy-name lookups
 * (v4 does NOT resolve them → null), and a determinism dump (twice).
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/provider-registry.ts \
 *     > /path/to/oracle-provider-registry.ndjson
 */

import { createRequire } from 'node:module';
import { join } from 'node:path';
import {
  initializeProviderRegistry,
  getAllProviders,
  getProvider,
  hasProvider,
  getProviderNames,
  supportsCapability,
  getAttachmentSupport,
  getMessageFormat,
  getCharsPerToken,
  getToolFormat,
  getCheapModelConfig,
  getDefaultContextWindow,
  getModelPricing,
} from '@/lib/plugins/provider-registry';

const nodeRequire = createRequire(join(process.cwd(), 'noop.js'));

/** The nine built-in plugin dirs, in registry-registration order. */
const PLUGIN_DIRS = [
  'anthropic',
  'openai',
  'google',
  'grok',
  'deepseek',
  'z-ai',
  'openrouter',
  'ollama',
  'openai-compatible',
];

/** Capabilities the getter accepts (`keyof DEFAULT_CAPABILITIES`). */
const CAPABILITIES = ['chat', 'imageGeneration', 'embeddings', 'webSearch', 'toolUse'] as const;

type Row = Record<string, unknown> & { kind: string };
const rows: Row[] = [];
const emit = (r: Row) => rows.push(r);

async function main() {
  const providers = PLUGIN_DIRS.map((d) => {
    const m = nodeRequire(join(process.cwd(), 'plugins', 'dist', `qtap-plugin-${d}`, 'index.js'));
    return m.plugin || m.default?.plugin || m.default;
  });
  await initializeProviderRegistry(providers);

  // Registration order + names.
  emit({ kind: 'names', names: getProviderNames() });

  // Every registered provider id, plus a few unknown/legacy ids that must NOT
  // resolve (getProvider is an exact-case Map lookup; legacyNames are metadata).
  const registeredIds = getAllProviders().map((p) => p.metadata.providerName);
  const probeIds = [
    ...registeredIds,
    'anthropic', // wrong case → miss
    'GOOGLE_IMAGEN', // legacy alias → NOT resolved → miss
    'UNKNOWN_PROVIDER',
    '',
  ];

  for (const id of probeIds) {
    const plugin = getProvider(id);
    emit({ kind: 'getProvider', id, found: plugin !== null });
    emit({ kind: 'hasProvider', id, has: hasProvider(id) });

    // Capability getters (default false for unknown).
    for (const cap of CAPABILITIES) {
      emit({ kind: 'supportsCapability', id, capability: cap, out: supportsCapability(id, cap) });
    }

    // Attachment support (null for unknown).
    const att = getAttachmentSupport(id);
    emit({ kind: 'attachmentSupport', id, out: att });

    // Message format (empty default for unknown).
    emit({ kind: 'messageFormat', id, out: getMessageFormat(id) });

    // Chars per token (default 3.5).
    emit({ kind: 'charsPerToken', id, out: getCharsPerToken(id) });

    // Tool format (default 'openai').
    emit({ kind: 'toolFormat', id, out: getToolFormat(id) });

    // Cheap model config (null for unknown / undeclared).
    emit({ kind: 'cheapModelConfig', id, out: getCheapModelConfig(id) });

    // Default context window (default 8192).
    emit({ kind: 'defaultContextWindow', id, out: getDefaultContextWindow(id) });

    // Legacy names (metadata) — proves the field is carried even though lookup
    // does not resolve it.
    emit({ kind: 'legacyNames', id, out: plugin?.metadata.legacyNames ?? [] });
  }

  // Model pricing per (provider × its declared models). Static fallback tier —
  // null on every built-in today, which the Rust manifests reproduce (empty map).
  for (const plugin of getAllProviders()) {
    const id = plugin.metadata.providerName;
    const models = plugin.getModelInfo ? plugin.getModelInfo() : [];
    for (const m of models) {
      emit({ kind: 'modelPricing', id, modelId: m.id, out: getModelPricing(id, m.id) });
    }
    // and an unknown model
    emit({ kind: 'modelPricing', id, modelId: '__no_such_model__', out: getModelPricing(id, '__no_such_model__') });
  }

  // Determinism: dump the names list twice; the Rust side dumps its registration
  // order twice and asserts both agree (a stable-order guard).
  emit({ kind: 'determinism', pass: 1, names: getProviderNames() });
  emit({ kind: 'determinism', pass: 2, names: getProviderNames() });

  for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
}

main().catch((e) => {
  process.stderr.write(String(e?.stack ?? e) + '\n');
  process.exit(1);
});
