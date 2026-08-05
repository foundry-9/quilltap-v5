/**
 * P4.D46 — dump the BUNDLED plugins' config-schema secret keys, so v5's
 * `.qtap` plugin-config redaction can reproduce v4's `resolveSecretConfigKeys`
 * without a plugin runtime.
 *
 * ── WHY THIS EXISTS ──────────────────────────────────────────────────────────
 * v4's export redacts every `password`-typed manifest key from a plugin
 * config, and withholds the WHOLE config (`_redactedKeys: ['*']`) when the
 * manifest cannot be resolved (`ndjson-writer.ts:781-833` at `7189a968`). The
 * lookup is `getPlugin(name)` against the live plugin registry, which in
 * production holds every bundled plugin under `plugins/dist/` (plus any
 * npm-installed ones). v5 has no plugin runtime, so the bundled set is
 * transcribed statically: presence in this table ≡ "manifest resolvable";
 * the value is the manifest's `configSchema` password-typed keys
 * (`configSchema ?? []` — a plugin with no schema resolves to an EMPTY set,
 * which is different from unresolvable). An npm-installed plugin's manifest is
 * NOT resolvable on v5 (it cannot run there either), so those configs export
 * whole-withheld — the safe arm.
 *
 * The checked-in generator + committed JSON is the standing
 * `byte-exact-static-data-transcription` idiom.
 *
 * Regenerate (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx $V5W/harness/oracle/fixtures/dump-plugin-config-schemas.ts \
 *     > $V5W/crates/quilltap-core/src/services/qtap_export/bundled-plugin-secret-keys.json
 */

import { readdirSync, readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';

interface ConfigField {
  key: string;
  type?: string;
}

function main(): void {
  const distDir = join(process.cwd(), 'plugins', 'dist');
  const out: Record<string, string[]> = {};
  for (const name of readdirSync(distDir).sort()) {
    const manifestPath = join(distDir, name, 'manifest.json');
    if (!existsSync(manifestPath)) continue;
    const manifest = JSON.parse(readFileSync(manifestPath, 'utf8')) as {
      configSchema?: ConfigField[];
    };
    const schema = manifest.configSchema ?? [];
    out[name] = schema.filter((f) => f.type === 'password').map((f) => f.key);
  }
  process.stdout.write(JSON.stringify(out, null, 2) + '\n');
}

main();
