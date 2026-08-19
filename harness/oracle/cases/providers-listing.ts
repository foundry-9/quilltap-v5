/**
 * P4.6d providers-listing ORACLE (tier-1) — the `GET /api/v1/providers` LLM rows.
 *
 * Drives v4's REAL provider plugins (loaded the runtime way, the
 * `provider-registry.ts` precedent) and emits, per LLM provider, the exact row the
 * route builds from `plugin.metadata` + `plugin.capabilities` + `plugin.config`
 * (the pure transform in `app/api/v1/providers/route.ts`). The Rust
 * `settings::provider_list` answers from the W4.7a manifests; the differential
 * diffs the manifest-covered fields per provider, normalizing away the ONE field
 * the manifest deliberately lacks (`icon`) and the search providers (no
 * search-provider manifest is ported) — both documented absences. `optionsSchema`
 * has been a comparand since P4.D83, and `configRequirements` is the plugin's
 * whole `config` object since P4.D93 (v4 bug 81's `acceptsApiKey`).
 *
 * Run (Node 24, from the v4 checkout):
 *   cd ~/source/quilltap-server
 *   npx tsx <worktree>/harness/oracle/cases/providers-listing.ts \
 *     > /tmp/oracle-providers-listing.ndjson
 */

import { createRequire } from 'node:module';
import { join } from 'node:path';

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
    });
  }
  process.stdout.write(JSON.stringify({ providers: rows, count: rows.length }) + '\n');
}

main();
