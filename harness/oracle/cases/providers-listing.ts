/**
 * P4.6d providers-listing ORACLE (tier-1) — the `GET /api/v1/providers` LLM rows.
 *
 * Drives v4's REAL provider plugins (loaded the runtime way, the
 * `provider-registry.ts` precedent) and emits, per LLM provider, the exact row the
 * route builds from `plugin.metadata` + `plugin.capabilities` + `plugin.config`
 * (the pure transform in `app/api/v1/providers/route.ts`). The Rust
 * `settings::provider_list` answers from the W4.7a manifests; the differential
 * diffs the manifest-covered fields per provider, normalizing away the fields the
 * manifest deliberately lacks (`icon`, `optionsSchema`) and the search providers
 * (no search-provider manifest is ported) — both documented absences.
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
    // The exact route transform (minus icon/optionsSchema, normalized away).
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
      configRequirements: {
        requiresApiKey: cfg.requiresApiKey,
        requiresBaseUrl: cfg.requiresBaseUrl,
        // Only the non-null labels (the manifest omits null labels).
        ...(cfg.apiKeyLabel != null ? { apiKeyLabel: cfg.apiKeyLabel } : {}),
        ...(cfg.baseUrlLabel != null ? { baseUrlLabel: cfg.baseUrlLabel } : {}),
        ...(cfg.baseUrlPlaceholder != null ? { baseUrlPlaceholder: cfg.baseUrlPlaceholder } : {}),
        ...(cfg.baseUrlDefault != null ? { baseUrlDefault: cfg.baseUrlDefault } : {}),
      },
    });
  }
  process.stdout.write(JSON.stringify({ providers: rows, count: rows.length }) + '\n');
}

main();
