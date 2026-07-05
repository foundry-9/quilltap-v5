/**
 * Fixture builder for W4.1g `tool_build_equivalence`.
 *
 * `buildTools` is essentially pure — its ONLY DB touch is
 * `repos.pluginConfigs.findByUserId(userId)`, which returns `[]` for a no-plugin
 * instance (and is unused for the built-in slate anyway; the plugin registry is
 * deferred). So the fixture is an EMPTY encrypted instance carrying the
 * `plugin_configs` table DDL, under the throwaway test pepper. The Rust
 * `build_tools` reads the empty table; the jest oracle's `getRepositories()` is
 * mocked away (→ empty map) — both agree on "no plugin tools".
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-tool-build.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-tool-build-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'tool-build.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  }

  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-tool-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { PluginConfigSchema } = await import('@/lib/schemas/plugin-config.types');

  await initializeDatabase();
  await ensureCollection('plugin_configs', PluginConfigSchema);
  await closeDatabase();

  process.stderr.write(`built tool-build fixture: ${out} (empty seed)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
