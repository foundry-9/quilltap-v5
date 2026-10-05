/**
 * @jest-environment node
 *
 * ORACLE for the dangerous-content provider-routing matrix — since v4
 * `8bd080267` (#73, P4.D225) the two thin pre-flight wrappers in
 * `lib/services/dangerous-content/provider-routing.service.ts`
 * (`resolveProviderForDangerousContent`, `resolveImageProviderForDangerousContent`)
 * AND the understudies they delegate to
 * (`lib/services/dangerous-content/understudy.ts`:
 * `resolveUncensoredTextUnderstudy`, `resolveUncensoredImageUnderstudy`), with
 * EVERY line the `ConciergeUnderstudy` and `DangerousContentProviderRouting`
 * loggers write recorded per case.
 *
 * RETIRED rows (P4.D225): v4 DELETED `resolveUncensoredImageProfileForReroute`
 * and `isImageModerationError` at `8bd080267`; importing them at the pin fails
 * (the red-first: this case as it stood could not run there). Their spec rows
 * (`rerouteCases`, `imgErrors`) stay in the committed JSON and are filtered out
 * on BOTH sides — the `retiring-a-v4-deleted-methods-oracle-row-keeps-the-
 * fixture` rule.
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

interface Case { id: string; user: 'A' | 'B'; originalProfileId?: string; currentProfileId?: string; originalApiKey?: string; /** v4 `3b463d6b1` (#76): the stored `conciergeSettings` + the chat the policy resolves WITH. */ concierge: Record<string, unknown>; chat?: { conciergeMode: string }; turnAttachmentMimeTypes?: string[] }
interface UnderstudyCase { id: string; user: 'A' | 'B'; uncensoredTextProfileId?: string | null; uncensoredImageProfileId?: string | null; exclude: string[]; turnAttachmentMimeTypes?: string[]; filterProviders?: string[]; failLookup?: boolean }
interface Spec {
  testPepperBase64: string;
  userA: string;
  userB: string;
  apiKeys: Record<string, string>;
  throwingApiKeys: string[];
  textCases: Case[];
  imageCases: Case[];
  textUnderstudyCases: UnderstudyCase[];
  imageUnderstudyCases: UnderstudyCase[];
}

interface RecordedLog { service: string | null; level: string; message: string; bag: Record<string, unknown> }
const LOGGED_SERVICES = new Set(['ConciergeUnderstudy', 'DangerousContentProviderRouting']);
// v4's fallback-read ERRORs (`base.repository.ts` / `safe-query.ts`).
const REPOSITORY_FALLBACK_MESSAGES = new Set([
  'Error finding entity by ID',
  'Error finding all entities',
  'Error finding API key by ID and user ID',
]);

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
  try {
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
  // P4.D225: every line the two routing loggers write, per case. Other
  // services (the DB stack's) are recorded too and filtered out below.
  const logs: RecordedLog[] = [];
  jest.doMock('@/lib/logger', () => {
    const recorder = (service: string | null): Record<string, unknown> => {
      const record = (level: string) => (message: string, bag?: Record<string, unknown>) => {
        if (service && LOGGED_SERVICES.has(service)) {
          logs.push({ service, level, message, bag: JSON.parse(JSON.stringify(bag ?? {})) });
        } else if (!service && REPOSITORY_FALLBACK_MESSAGES.has(message)) {
          // P4.124: the repository's own fallback-read line (`safeQuery`
          // logs through the root logger), so the failed read's line is in
          // the comparand, not only the resolver's.
          logs.push({ service: 'Repository', level, message, bag: JSON.parse(JSON.stringify(bag ?? {})) });
        }
      };
      const self: Record<string, unknown> = {
        debug: record('debug'), info: record('info'), warn: record('warn'),
        error: record('error'), trace: record('trace'),
      };
      self.child = (ctx: Record<string, unknown>) =>
        recorder(typeof ctx?.service === 'string' ? ctx.service : service);
      return self;
    };
    return {
      __esModule: true,
      LogLevel: { ERROR: 'error', WARN: 'warn', INFO: 'info', DEBUG: 'debug', TRACE: 'trace' },
      logger: recorder(null),
    };
  });

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
  const understudy = await import('@/lib/services/dangerous-content/understudy');
  const { resolveConciergeSettings } = await import('@/lib/services/dangerous-content/resolver.service');

  await initializeDatabase();
  const repos = getRepositories();
  const uid = (u: 'A' | 'B') => (u === 'A' ? spec.userA : spec.userB);

  // Canned key seam: patch the singleton connections repo's key lookup.
  (repos.connections as any).findApiKeyByIdAndUserId = async (id: string, _userId: string) => {
    // P4.D225: a key lookup that THROWS — the understudy's `decryptKey` WARNs
    // and answers null.
    if ((spec.throwingApiKeys ?? []).includes(id)) throw new Error('canned key lookup failure');
    const kv = spec.apiKeys[id];
    if (!kv) return null;
    return { id, userId: _userId, label: 'canned', provider: 'OPENAI', key_value: kv, isActive: true, createdAt: '2020-01-01T00:00:00.000Z', updatedAt: '2020-01-01T00:00:00.000Z' };
  };

  const lines: string[] = [];
  const takeLogs = () => logs.splice(0).map(({ service, level, message, bag }) => ({ service, level, message, bag }));

  for (const c of spec.textCases) {
    const original = await repos.connections.findById(c.originalProfileId!);
    if (!original) throw new Error(`text case ${c.id}: original ${c.originalProfileId} not found`);
    const policy = resolveConciergeSettings({ conciergeSettings: c.concierge as never }, (c.chat ?? null) as never);
    // `turnAttachmentMimeTypes` is v4's fifth parameter (`a1d88aa3a`, bug 106).
    // A case that omits it takes v4's `[]` default, so every pre-existing row
    // is byte-identical.
    const r = await routing.resolveProviderForDangerousContent(
      original as any, c.originalApiKey!, policy, uid(c.user), c.turnAttachmentMimeTypes ?? []
    );
    lines.push(JSON.stringify({ kind: 'text', id: c.id, rerouted: r.rerouted, profile: profileSubset(r.connectionProfile), apiKey: r.apiKey, reason: r.reason, logs: takeLogs() }));
  }

  takeLogs();
  for (const c of spec.imageCases) {
    const original = await repos.imageProfiles.findById(c.originalProfileId!);
    if (!original) throw new Error(`image case ${c.id}: original ${c.originalProfileId} not found`);
    const policy = resolveConciergeSettings({ conciergeSettings: c.concierge as never }, (c.chat ?? null) as never);
    const r = await routing.resolveImageProviderForDangerousContent(original as any, c.originalApiKey!, policy, uid(c.user));
    lines.push(JSON.stringify({ kind: 'image', id: c.id, rerouted: r.rerouted, profile: profileSubset(r.imageProfile), apiKey: r.apiKey, reason: r.reason, logs: takeLogs() }));
  }

  // `rerouteCases` / `imgErrors`: RETIRED (see the header) — not driven.

  // P4.D225: the understudies, driven directly (exclusion, the courier skip,
  // the caller's filter on the explicit pick and the scan, the throwing key,
  // the swallowed lookup failure).
  // P4.124: the failure is planted UNDER v4's real `findAll` — the collection
  // read throws — so its `safeQuery` fallback runs (the repository's ERROR,
  // then `[]`). Stubbing `findAll` itself to throw (the pre-P4.124 plant)
  // tested a throw real v4 cannot raise: the resolver's own catch is
  // unreachable on a database failure (v5's pool arm keeps its line).
  const withFailingLookup = async <T>(fail: boolean | undefined, which: 'connections' | 'imageProfiles', f: () => Promise<T>): Promise<T> => {
    if (!fail) return f();
    const repo = (repos as any)[which];
    const getCollection = repo.getCollection;
    repo.getCollection = async () => { throw new Error('canned lookup failure'); };
    try { return await f(); } finally { repo.getCollection = getCollection; }
  };
  takeLogs();
  for (const c of spec.textUnderstudyCases) {
    // The understudy reads only the policy's desk (v4 `3b463d6b1`, #76).
    const conciergePolicy = resolveConciergeSettings({ conciergeSettings: { enabled: true, uncensoredTextProfileId: c.uncensoredTextProfileId ?? undefined } as never });
    const filter = c.filterProviders ? (p: any) => c.filterProviders!.includes(p.provider) : undefined;
    const r = await withFailingLookup(c.failLookup, 'connections', () =>
      understudy.resolveUncensoredTextUnderstudy({
        userId: uid(c.user), conciergePolicy, exclude: c.exclude,
        turnAttachmentMimeTypes: c.turnAttachmentMimeTypes ?? [], filter,
      }));
    lines.push(JSON.stringify({ kind: 'textUnderstudy', id: c.id, result: r ? { profile: profileSubset(r.profile), apiKey: r.apiKey } : null, logs: takeLogs() }));
  }
  for (const c of spec.imageUnderstudyCases) {
    const conciergePolicy = resolveConciergeSettings({ conciergeSettings: { enabled: true, uncensoredImageProfileId: c.uncensoredImageProfileId ?? undefined } as never });
    const r = await withFailingLookup(c.failLookup, 'imageProfiles', () =>
      understudy.resolveUncensoredImageUnderstudy({ userId: uid(c.user), conciergePolicy, exclude: c.exclude }));
    lines.push(JSON.stringify({ kind: 'imageUnderstudy', id: c.id, result: r ? { profile: profileSubset(r.profile), apiKey: r.apiKey } : null, logs: takeLogs() }));
  }

  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`danger-routing oracle wrote ${outPath}\n`);
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

test('danger-routing oracle', async () => {
  await main();
});
