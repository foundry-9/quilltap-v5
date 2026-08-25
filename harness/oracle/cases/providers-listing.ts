/**
 * P4.6d providers-listing ORACLE (tier-1) — the `GET /api/v1/providers` LLM rows.
 *
 * Drives v4's REAL provider plugins (loaded the runtime way, the
 * `provider-registry.ts` precedent) and emits, per LLM provider, the exact row the
 * route builds from `plugin.metadata` + `plugin.capabilities` + `plugin.config`
 * (the pure transform in `app/api/v1/providers/route.ts`). The Rust
 * `settings::provider_list` answers from the W4.7a manifests; the differential
 * diffs the manifest-covered fields per provider, normalizing away the ONE field
 * the manifest deliberately lacks (`icon`) — a documented absence.
 * `optionsSchema` has been a comparand since P4.D83, and `configRequirements` is
 * the plugin's whole `config` object since P4.D93 (v4 bug 81's `acceptsApiKey`).
 *
 * P4.59 appends the SEARCH provider row: the route spreads
 * `[...providerList, ...searchProviderList]`, so it comes AFTER the ten LLM rows,
 * and its shape is materially different — no `capabilities`, no `optionsSchema`,
 * no `thinkingTurnRule`, and a hand-built THREE-key `configRequirements`. The
 * key SET and the key ORDER are both wire-visible under `preserve_order`, so the
 * differential compares the serialized bytes of every row.
 *
 * Run (Node 24, from the v4 checkout):
 *   cd ~/source/quilltap-server
 *   npx tsx <worktree>/harness/oracle/cases/providers-listing.ts \
 *     > /tmp/oracle-providers-listing.ndjson
 */

import { createRequire } from 'node:module';
import { join } from 'node:path';

const nodeRequire = createRequire(join(process.cwd(), 'noop.js'));

/**
 * The ten built-in plugin dirs, in registry-registration order. NanoGPT
 * (P4.D101) is APPENDED, matching the Rust `BUILT_IN_MANIFEST_JSON` order —
 * both this list and that array are compared positionally, so appending
 * leaves all nine pre-existing rows byte-identical.
 */
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
  'nanogpt',
];

/**
 * The bundled SEARCH provider dirs, in registration order. v4 ships exactly one
 * (`enabledByDefault: true`), and the route appends them after the LLM rows.
 */
const SEARCH_PLUGIN_DIRS = ['search-serper'];

function main() {
  const rows: Array<Record<string, unknown>> = [];
  for (const dir of PLUGIN_DIRS) {
    const m = nodeRequire(
      join(process.cwd(), 'plugins', 'dist', `qtap-plugin-${dir}`, 'index.js'),
    );
    const plugin = m.plugin || m.default?.plugin || m.default;
    const md = plugin.metadata;
    const cfg = plugin.config;
    // P4.D83: the plugin's options schema, exactly as the route reads it
    // (`plugin.getProviderOptionsSchema?.() ?? null`). v4 wraps the call in a
    // try/catch that falls back to undefined; no built-in throws, and a throw
    // here should be visible rather than silently recorded as null.
    const optionsSchema = plugin.getProviderOptionsSchema
      ? (plugin.getProviderOptionsSchema() ?? null)
      : null;
    // The exact route transform (minus icon, normalized away).
    rows.push({
      id: md.providerName,
      name: md.providerName,
      displayName: md.displayName,
      description: md.description,
      abbreviation: md.abbreviation,
      colors: md.colors,
      type: 'llm',
      capabilities: {
        chat: !!plugin.capabilities.chat,
        imageGeneration: !!plugin.capabilities.imageGeneration,
        embeddings: !!plugin.capabilities.embeddings,
        webSearch: !!plugin.capabilities.webSearch,
        toolUse: !!plugin.capabilities.toolUse,
      },
      // v4's route spreads `plugin.config` WHOLE (`route.ts:51`), so this does
      // too. It used to hand-pick the six fields the manifest models, which made
      // the comparand blind to any config key v4 adds — precisely how `acceptsApiKey`
      // (bug 81) would have slipped through green. A key the v5 manifest does not
      // carry now shows up as a RED diff, which is the tripwire this family is for.
      configRequirements: cfg,
      optionsSchema,
      // v4 bug 85 (`97d2fcb5`): the declared thinking-turn rule, exactly as
      // the route serves it — `plugin.thinkingTurnRule ?? null`, so the key is
      // ALWAYS present and `null` where the plugin declares none.
      thinkingTurnRule: plugin.thinkingTurnRule ?? null,
    });
  }
  // P4.59: the search providers, appended exactly as the route spreads them
  // (`[...providerList, ...searchProviderList]`). v4 ships exactly one.
  for (const dir of SEARCH_PLUGIN_DIRS) {
    const m = nodeRequire(
      join(process.cwd(), 'plugins', 'dist', `qtap-plugin-${dir}`, 'index.js'),
    );
    const plugin = m.plugin || m.default?.plugin || m.default;
    const md = plugin.metadata;
    // The exact route transform (minus icon, normalized away). Note the search
    // row carries NO `capabilities` key at all — which is how v4's own profile
    // editor keeps it out of the LLM picker (`p.capabilities?.chat`) — and its
    // `configRequirements` is hand-built from three named fields, not the
    // plugin's whole `config`.
    rows.push({
      id: md.providerName,
      name: md.providerName,
      displayName: md.displayName,
      description: md.description,
      abbreviation: md.abbreviation,
      colors: md.colors,
      type: 'search',
      configRequirements: {
        requiresApiKey: plugin.config.requiresApiKey,
        requiresBaseUrl: plugin.config.requiresBaseUrl,
        apiKeyLabel: plugin.config.apiKeyLabel,
      },
    });
  }
  process.stdout.write(JSON.stringify({ providers: rows, count: rows.length }) + '\n');
}

main();
