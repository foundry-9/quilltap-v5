/**
 * @jest-environment node
 *
 * ORACLE for the W4.2 dangerous-content provider-routing matrix (v4
 * `lib/services/dangerous-content/provider-routing.service.ts`):
 * `resolveProviderForDangerousContent`, `resolveImageProviderForDangerousContent`,
 * `resolveUncensoredImageProfileForReroute`, `isImageModerationError`.
 *
 * Drives the REAL functions over a baked `connection_profiles` / `image_profiles`
 * fixture. API-key decryption is a canned seam on BOTH sides: this oracle
 * monkey-patches the singleton `repos.connections.findApiKeyByIdAndUserId` to
 * return `{ key_value }` from the spec's `apiKeys` map (or null when absent) —
 * the Rust port injects the same map as its `ApiKeyResolver`. No `api_keys`
 * rows are seeded. The real DB stack is wired back in past `jest.setup`
 * ([[jest-real-db-oracle]]).
 *
 * Run from the v4 checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-danger-routing.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-danger-routing-fixture.ts
 *   TMPO=/tmp/qt-danger-oracle  # copy this .test.ts + the spec json outside .claude
 *   QT_FIXTURE_ROUTING=/tmp/qt-danger-routing.db \
 *   QT_ORACLE_OUT=/tmp/oracle-danger-routing.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/cases" -- danger-routing
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Case { id: string; user: 'A' | 'B'; originalProfileId?: string; currentProfileId?: string; originalApiKey?: string; mode: string; uncensoredTextProfileId?: string | null; uncensoredImageProfileId?: string | null; turnAttachmentMimeTypes?: string[] }
interface Spec {
  testPepperBase64: string;
  userA: string;
  userB: string;
  apiKeys: Record<string, string>;
  textCases: Case[];
  imageCases: Case[];
  rerouteCases: Case[];
  imgErrors: string[];
}

function profileSubset(p: any): unknown {
  return { id: p.id, name: p.name, provider: p.provider, modelName: p.modelName, baseUrl: p.baseUrl ?? null };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'danger-routing.json'), 'utf8')
  ) as Spec;

  const fixture = process.env.QT_FIXTURE_ROUTING;
  if (!fixture || !existsSync(fixture)) throw new Error('QT_FIXTURE_ROUTING must point at the seed fixture');
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-danger-routing-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'routing-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  // The provider registry, initialized with the ten real dist plugins.
  //
  // LOAD-BEARING since `a1d88aa3a`: the scan's carry test runs through
  // `profileCanReceiveAttachment` → `providerCanTransportImages`, which prefers
  // the live registry and only falls back to the client-safe static mirror when
  // the registry is DOWN. v5's manifests are baked, so it always answers the
  // registry tier; an uninitialized oracle would answer the mirror and the two
  // could disagree for exactly the providers bug 97 was about. (Same finding as
  // the `fallback-engine` oracle's registry block — [[jest-oracle-empty-provider-registry]].)
  const nodeRequire = require;
  const PLUGIN_DIRS = [
    'anthropic', 'openai', 'google', 'grok', 'deepseek',
    'z-ai', 'openrouter', 'ollama', 'openai-compatible', 'nanogpt',
  ];
  const { initializeProviderRegistry } = await import('@/lib/plugins/provider-registry');
  await initializeProviderRegistry(
    PLUGIN_DIRS.map((d) => {
      const m = nodeRequire(join(process.cwd(), 'plugins', 'dist', `qtap-plugin-${d}`, 'index.js'));
      return m.plugin || m.default?.plugin || m.default;
    }),
  );

  const routing = await import('@/lib/services/dangerous-content/provider-routing.service');

  await initializeDatabase();
  const repos = getRepositories();
  const uid = (u: 'A' | 'B') => (u === 'A' ? spec.userA : spec.userB);

  // Canned key seam: patch the singleton connections repo's key lookup.
  (repos.connections as any).findApiKeyByIdAndUserId = async (id: string, _userId: string) => {
    const kv = spec.apiKeys[id];
    if (!kv) return null;
    return { id, userId: _userId, label: 'canned', provider: 'OPENAI', key_value: kv, isActive: true, createdAt: '2020-01-01T00:00:00.000Z', updatedAt: '2020-01-01T00:00:00.000Z' };
  };

  const lines: string[] = [];

  for (const c of spec.textCases) {
    const original = await repos.connections.findById(c.originalProfileId!);
    if (!original) throw new Error(`text case ${c.id}: original ${c.originalProfileId} not found`);
    const settings = { mode: c.mode, uncensoredTextProfileId: c.uncensoredTextProfileId ?? undefined } as any;
    // `turnAttachmentMimeTypes` is v4's fifth parameter (`a1d88aa3a`, bug 106).
    // A case that omits it takes v4's `[]` default, so every pre-existing row
    // is byte-identical.
    const r = await routing.resolveProviderForDangerousContent(
      original as any, c.originalApiKey!, settings, uid(c.user), c.turnAttachmentMimeTypes ?? []
    );
    lines.push(JSON.stringify({ kind: 'text', id: c.id, rerouted: r.rerouted, profile: profileSubset(r.connectionProfile), apiKey: r.apiKey, reason: r.reason }));
  }

  for (const c of spec.imageCases) {
    const original = await repos.imageProfiles.findById(c.originalProfileId!);
    if (!original) throw new Error(`image case ${c.id}: original ${c.originalProfileId} not found`);
    const settings = { mode: c.mode, uncensoredImageProfileId: c.uncensoredImageProfileId ?? undefined } as any;
    const r = await routing.resolveImageProviderForDangerousContent(original as any, c.originalApiKey!, settings, uid(c.user));
    lines.push(JSON.stringify({ kind: 'image', id: c.id, rerouted: r.rerouted, profile: profileSubset(r.imageProfile), apiKey: r.apiKey, reason: r.reason }));
  }

  for (const c of spec.rerouteCases) {
    const settings = { mode: c.mode, uncensoredImageProfileId: c.uncensoredImageProfileId ?? undefined } as any;
    const r = await routing.resolveUncensoredImageProfileForReroute(c.currentProfileId!, settings, uid(c.user));
    lines.push(JSON.stringify({ kind: 'reroute', id: c.id, result: r ? { profile: profileSubset(r.profile), apiKey: r.apiKey } : null }));
  }

  for (const msg of spec.imgErrors) {
    lines.push(JSON.stringify({ kind: 'imgerr', message: msg, out: routing.isImageModerationError(msg) }));
  }

  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`danger-routing oracle wrote ${outPath}\n`);
}

test('danger-routing oracle', async () => {
  await main();
});
