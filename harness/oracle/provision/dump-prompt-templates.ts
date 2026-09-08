/**
 * Data-module generator for the built-in prompt templates (P4.83).
 *
 * Emits `crates/quilltap-core/src/services/builtin_prompt_templates.json` — the
 * verbatim 21 "Sample Prompts" v4's `PromptTemplatesRepository.seedSamplePrompts()`
 * inserts on the first listing. The bytes come from v4's REAL plugin module
 * (`plugins/dist/qtap-plugin-default-system-prompts/index.js`, whose `loadPrompts()`
 * reads the 21 `prompts/*.md` files) fed through v4's REAL
 * `systemPromptRegistry`, so there is no hand transcription of the (large,
 * whitespace-sensitive) prompt bodies — and, crucially, no re-derivation of the
 * registry's DISPLAY NAME, which is what the seeded row's `name` column carries.
 *
 * ⚠ The seeded `name` is NOT the filename. `system-prompt-registry.ts`'s
 * `loadPromptsFromPlugin` computes
 *   `displayName = `${modelHint} ${category[0] + category.slice(1).toLowerCase()}``
 * and stores it as `LoadedSystemPrompt.name`; the seeder writes `prompt.name`.
 * So `MODERN_GENERAL.md` becomes the row **`MODERN General`**, and the registry
 * id (`default-system-prompts/MODERN_GENERAL`) is what the seed log's `promptId`
 * field carries. Both are emitted here.
 *
 * v5 has no plugin system and no `prompts/` directory, so this table IS the
 * catalogue: a v5-only instance seeds from it. A v4 commit that edits, adds or
 * removes a prompt `.md` is therefore a RE-VENDOR obligation — the tripwire is
 * `crates/quilltap-harness/tests/builtin_prompt_templates_guard.rs`, which
 * re-reads the checkout's `.md` files and fails the gate when they disagree.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/npx tsx ~/source/quilltap-v5/harness/oracle/provision/dump-prompt-templates.ts
 */

import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { writeFileSync } from 'node:fs';

const require = createRequire(import.meta.url);

interface DumpedPrompt {
  /** The registry id — the seed log's `promptId` (`default-system-prompts/<FILE>`). */
  promptId: string;
  /** The registry DISPLAY name — the `prompt_templates.name` column. */
  name: string;
  content: string;
  modelHint: string;
  category: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const out =
    process.env.QT_PROMPT_TEMPLATES_OUT ??
    join(
      here,
      '..',
      '..',
      '..',
      'crates',
      'quilltap-core',
      'src',
      'services',
      'builtin_prompt_templates.json',
    );

  process.env.LOG_LEVEL = 'error';

  const pluginPath =
    process.env.QT_SYSTEM_PROMPT_PLUGIN ??
    join(
      process.cwd(),
      'plugins',
      'dist',
      'qtap-plugin-default-system-prompts',
      'index.js',
    );

  // The REAL plugin module: its top-level `loadPrompts()` reads the 21 .md files
  // in `readdirSync(...).sort()` order (JS default sort — UTF-16 code units).
  const mod = require(pluginPath);
  const plugin = mod.plugin ?? mod.default?.plugin;
  if (!plugin) {
    throw new Error(`no plugin export at ${pluginPath}`);
  }

  // The REAL registry: `initialize` is what computes each prompt's display name
  // and `default-system-prompts/<NAME>` id. `getAll()` preserves Map insertion
  // order = the plugin's array order = the sorted filenames = the SEED order.
  const { systemPromptRegistry, initializeSystemPromptRegistry } = await import(
    '@/lib/plugins/system-prompt-registry'
  );
  await initializeSystemPromptRegistry([plugin]);

  const prompts: DumpedPrompt[] = systemPromptRegistry.getAll().map((p) => ({
    promptId: p.id,
    name: p.name,
    content: p.content,
    modelHint: p.modelHint,
    category: p.category,
  }));

  if (prompts.length === 0) {
    throw new Error('the registry loaded no prompts — refusing to write an empty catalogue');
  }

  writeFileSync(out, JSON.stringify(prompts, null, 2) + '\n');
  process.stderr.write(`wrote ${prompts.length} built-in prompt templates → ${out}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`dump-prompt-templates failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
