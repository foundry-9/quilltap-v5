/**
 * @jest-environment node
 *
 * Differential ORACLE for the W4.1d5 `search_web` tool.
 *
 * Drives v4's REAL `executeWebSearchTool` + `formatWebSearchResults`. Since
 * P4.59 the provider-path cases drive the REAL machinery all the way down:
 *   - the REAL `searchProviderRegistry`, initialized the way v4's boot
 *     initializes it (`initializeSearchProviderRegistry([plugin])`) with the
 *     REAL built `qtap-plugin-search-serper` bundle — no hand-built stand-in.
 *     `getDefaultProvider`, `config.requiresApiKey`, the plugin's own
 *     `executeSearch` (over a mocked `global.fetch`) and its `formatResults`
 *     all run for real, which is what makes the port's registered arm
 *     comparable at all. (Before P4.59 this case mocked the registry with an
 *     object whose `executeSearch` returned canned output, so the plugin's own
 *     request, error sentences and formatter were never in the loop here —
 *     the `jest-oracle-empty-provider-registry` trap, one level up.)
 *   - the api-key lookup runs v4's OWN predicate: `getAllApiKeys()` is mocked
 *     (it is the repository boundary) but returns a REALISTIC multi-row list,
 *     so `find(k => k.provider === providerName && k.isActive)` is what
 *     decides. Cases cover an active row, an INACTIVE row, a row for another
 *     provider, and an ordering case with two SERPER rows.
 *   - fallback-path cases leave the registry EMPTY + set `SERPER_API_KEY`,
 *     driving the REAL `executeSerperFallback` and its DISTINCT error strings.
 *   - `registered_shortcircuits_env` sets BOTH: v4 takes the provider path, and
 *     the proof is the plugin's 401 sentence rather than the fallback's.
 *
 * ⚠ The registry keeps its state on `globalThis`, so it SURVIVES
 * `jest.resetModules()`. Each case deletes that key first, or a case that must
 * see an empty registry would inherit the previous case's provider.
 *
 * Emits one NDJSON line per case: { label, resultJson, formatted }.
 *
 * TZ MUST be UTC (the formatter's `publishedDate`).
 *
 * Run (Node 24, from the v4 checkout; STAGE outside any .claude path):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; STAGE=/tmp/qt-oracle-stage
 *   cd ~/source/quilltap-server
 *   TZ=UTC QT_ORACLE_OUT=/tmp/oracle-web-search-tool.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- web-search-tool
 */

import * as fs from 'fs';
import { createRequire } from 'node:module';
import { join } from 'node:path';

const nodeRequire = createRequire(join(process.cwd(), 'noop.js'));

/** The REAL built Serper plugin — the same bundle v4's boot loads. */
const serperPlugin = (() => {
  const m = nodeRequire(
    join(process.cwd(), 'plugins', 'dist', 'qtap-plugin-search-serper', 'index.js'),
  );
  const plugin = m.plugin || m.default?.plugin || m.default;
  if (!plugin?.metadata?.providerName) throw new Error('serper plugin has no metadata');
  return plugin;
})();

/** One `api_keys` row as `getAllApiKeys()` returns it (the fields v4 reads). */
interface KeyRow {
  provider: string;
  isActive: boolean;
  key_value: string;
}

/** A canned HTTP reply for the plugin's / the fallback's `fetch`. */
interface Wire {
  status: number;
  statusText: string;
  body: string;
}

interface Case {
  label: string;
  args: unknown;
  /** Register the real Serper plugin into the real registry. */
  registered?: boolean;
  /** Rows `getAllApiKeys()` answers with. */
  apiKeys?: KeyRow[];
  /** `SERPER_API_KEY` for this case. */
  envKey?: string;
  /** The canned wire reply, when the case is expected to reach `fetch`. */
  wire?: Wire;
  /** Make `fetch` throw instead (the plugin's / handler's catch arm). */
  networkError?: string;
  /**
   * Answer with a single organic result whose title is `key:<X-API-KEY>`.
   *
   * WHICH key the lookup chose is otherwise invisible in the tool's output — it
   * travels as a request HEADER and appears in no field either side emits — so a
   * case that merely searches successfully proves only that SOME key was found.
   * Echoing the header into the body is what makes "the FIRST active row, not
   * the stale one" a real comparand on both sides.
   */
  echoKey?: boolean;
}

const ACTIVE: KeyRow = { provider: 'SERPER', isActive: true, key_value: 'db-key' };
const INACTIVE: KeyRow = { provider: 'SERPER', isActive: false, key_value: 'stale-key' };
const OTHER: KeyRow = { provider: 'OPENAI', isActive: true, key_value: 'sk-other' };
const SECOND_ACTIVE: KeyRow = { provider: 'SERPER', isActive: true, key_value: 'second-key' };

const okBody = (organic: unknown[], knowledgeGraph?: unknown): Wire => ({
  status: 200,
  statusText: 'OK',
  body: JSON.stringify(knowledgeGraph ? { organic, knowledgeGraph } : { organic }),
});

const r = (title: string, date?: string) => ({
  title,
  link: `https://example.com/${title.replace(/\s+/g, '-').toLowerCase()}`,
  snippet: `A snippet about ${title}.`,
  ...(date ? { date } : {}),
});

const CASES: Case[] = [
  // --- provider path: the REAL plugin, over the REAL registry ---
  {
    label: 'provider_success',
    args: { query: 'latest AI news' },
    registered: true,
    apiKeys: [ACTIVE],
    wire: okBody([r('Quantum leap', '2026-06-15T00:00:00.000Z'), r('More news')]),
  },
  {
    label: 'provider_success_maxresults',
    args: { query: 'tokyo weather', maxResults: 3 },
    registered: true,
    apiKeys: [ACTIVE],
    wire: okBody([r('Sunny in Tokyo', '2020-12-31T23:00:00.000Z')]),
  },
  // Lenient numbers (P4.d5 tier 2): a model quoting its number must reach the
  // SAME wire request as the bare one — `maxResults` is an llmNumber field, so
  // the parse replaces it and `num: 3` (not `"3"`) goes out. Both sides key the
  // canned transport on the exact request body, so a string leaking through to
  // the wire cannot pass.
  {
    label: 'lenient_quoted_maxresults',
    args: { query: 'tokyo weather', maxResults: '3' },
    registered: true,
    apiKeys: [ACTIVE],
    wire: okBody([r('Sunny in Tokyo', '2020-12-31T23:00:00.000Z')]),
  },
  // Refused, not coerced: `true` would become 1 under z.coerce.number().
  { label: 'lenient_true_refused', args: { query: 'tokyo weather', maxResults: true } },
  {
    label: 'provider_no_results',
    args: { query: 'obscure' },
    registered: true,
    apiKeys: [ACTIVE],
    wire: okBody([]),
  },
  // The knowledgeGraph unshift, through the plugin's own mapping.
  {
    label: 'provider_knowledge_graph',
    args: { query: 'pharos' },
    registered: true,
    apiKeys: [ACTIVE],
    wire: okBody([r('A history of lighthouses')], {
      title: 'Pharos of Alexandria',
      description: 'One of the Seven Wonders of the Ancient World.',
      source: { name: 'Wikipedia', link: 'https://example.com/pharos' },
    }),
  },
  {
    label: 'provider_error_401',
    args: { query: 'x' },
    registered: true,
    apiKeys: [ACTIVE],
    wire: { status: 401, statusText: 'Unauthorized', body: 'nope' },
  },
  {
    label: 'provider_error_429',
    args: { query: 'x' },
    registered: true,
    apiKeys: [ACTIVE],
    wire: { status: 429, statusText: 'Too Many Requests', body: 'slow' },
  },
  {
    label: 'provider_error_500',
    args: { query: 'x' },
    registered: true,
    apiKeys: [ACTIVE],
    wire: { status: 500, statusText: 'Internal Server Error', body: 'boom' },
  },
  {
    label: 'provider_network_error',
    args: { query: 'x' },
    registered: true,
    apiKeys: [ACTIVE],
    networkError: 'socket hang up',
  },
  // --- the api-key predicate: `provider === name && isActive` ---
  { label: 'missing_api_key_no_rows', args: { query: 'needs a key' }, registered: true, apiKeys: [] },
  {
    label: 'missing_api_key_inactive_only',
    args: { query: 'needs a key' },
    registered: true,
    apiKeys: [INACTIVE],
  },
  {
    label: 'missing_api_key_other_provider',
    args: { query: 'needs a key' },
    registered: true,
    apiKeys: [OTHER],
  },
  // The inactive row must be SKIPPED, not merely counted: the active row that
  // follows it is the one whose key goes to the wire — and `echoKey` puts that
  // key in the output so "which one" is actually compared.
  {
    label: 'key_skips_inactive_takes_active',
    args: { query: 'skip the stale one' },
    registered: true,
    apiKeys: [INACTIVE, OTHER, ACTIVE],
    echoKey: true,
  },
  // Two active SERPER rows — `find` takes the FIRST (`db-key`, not `second-key`).
  {
    label: 'key_takes_first_active',
    args: { query: 'first wins' },
    registered: true,
    apiKeys: [ACTIVE, SECOND_ACTIVE],
    echoKey: true,
  },
  // The registered path sends the DB row's key, never the env one — visible
  // only because the header is echoed.
  {
    label: 'key_registered_sends_db_key_not_env',
    args: { query: 'whose key' },
    registered: true,
    apiKeys: [ACTIVE],
    envKey: 'env-key',
    echoKey: true,
  },
  // …and the fallback path sends the env key.
  {
    label: 'key_fallback_sends_env_key',
    args: { query: 'whose key' },
    envKey: 'env-key',
    echoKey: true,
  },
  // --- registration short-circuits the env fallback ---
  // BOTH are configured. v4 takes the provider path, and the tell is the
  // PLUGIN's 401 sentence ('Please check your API key in Settings > API Keys.')
  // rather than the fallback's ('...your SERPER_API_KEY environment
  // variable...').
  {
    label: 'registered_shortcircuits_env',
    args: { query: 'x' },
    registered: true,
    apiKeys: [ACTIVE],
    envKey: 'env-key',
    wire: { status: 401, statusText: 'Unauthorized', body: 'nope' },
  },
  // Registered but keyless, WITH an env key present: v4 still refuses with the
  // MissingApiKey sentence — it never falls back once a provider is registered.
  {
    label: 'registered_keyless_does_not_fall_back',
    args: { query: 'x' },
    registered: true,
    apiKeys: [],
    envKey: 'env-key',
  },
  // --- fallback path (no plugin registered, SERPER_API_KEY set) ---
  {
    label: 'fallback_success',
    args: { query: 'fallback query' },
    envKey: 'env-key',
    wire: okBody(
      [{ title: 'F1', link: 'https://f/1', snippet: 's1', date: '2026-01-02T00:00:00.000Z' }],
      { title: 'KG', description: 'kg desc', source: { link: 'https://kg' } },
    ),
  },
  {
    label: 'fallback_401',
    args: { query: 'x' },
    envKey: 'env-key',
    wire: { status: 401, statusText: 'Unauthorized', body: 'nope' },
  },
  {
    label: 'fallback_429',
    args: { query: 'x' },
    envKey: 'env-key',
    wire: { status: 429, statusText: 'Too Many Requests', body: 'slow' },
  },
  {
    label: 'fallback_500',
    args: { query: 'x' },
    envKey: 'env-key',
    wire: { status: 500, statusText: 'Internal Server Error', body: 'boom' },
  },
  { label: 'not_configured', args: { query: 'nothing' } },
  // --- validation ---
  { label: 'validation_empty', args: { query: '   ' } },
  { label: 'validation_nonobject', args: 'nope' },
];

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const lines: string[] = [];

  for (const c of CASES) {
    jest.resetModules();
    // ⚠ The registry's state lives on globalThis and survives resetModules.
    delete (globalThis as Record<string, unknown>).__quilltapSearchProviderRegistryState;
    delete process.env.SERPER_API_KEY;
    if (c.envKey) process.env.SERPER_API_KEY = c.envKey;

    const origFetch = globalThis.fetch;
    globalThis.fetch = (async (_url: unknown, init: { headers?: Record<string, string> } = {}) => {
      if (c.networkError) throw new Error(c.networkError);
      if (c.echoKey) {
        const sent = (init.headers ?? {})['X-API-KEY'];
        const body = JSON.stringify({
          organic: [
            { title: `key:${sent}`, link: 'https://example.com/key', snippet: 'The key that was sent.' },
          ],
        });
        return new Response(body, {
          status: 200,
          statusText: 'OK',
          headers: { 'content-type': 'application/json' },
        });
      }
      if (!c.wire) throw new Error(`case ${c.label} reached fetch with no canned wire`);
      return new Response(c.wire.body, {
        status: c.wire.status,
        statusText: c.wire.statusText,
        headers: { 'content-type': 'application/json' },
      });
    }) as typeof fetch;

    jest.doMock('@/lib/logger', () => {
      const l = { child: jest.fn(), debug: jest.fn(), info: jest.fn(), warn: jest.fn(), error: jest.fn() };
      l.child.mockReturnValue(l);
      return { logger: l };
    });

    // The repository boundary — the only mock on the provider path. It answers
    // with the case's rows so v4's OWN `find(provider === name && isActive)`
    // predicate is what decides.
    jest.doMock('@/lib/repositories/user-scoped', () => ({
      getUserRepositories: () => ({
        connections: { getAllApiKeys: async () => c.apiKeys ?? [] },
      }),
    }));

    if (c.registered) {
      const registry = await import('@/lib/plugins/search-provider-registry');
      await registry.initializeSearchProviderRegistry([serperPlugin]);
    }

    const { executeWebSearchTool, formatWebSearchResults } = await import(
      '@/lib/tools/handlers/web-search-handler'
    );
    const out = await executeWebSearchTool(c.args, { userId: 'user-1' });
    // `formatWebSearchResults` delegates to the registered plugin's own
    // `formatResults` when one is registered, so this line also proves the
    // plugin's formatter and the handler's built-in one agree byte for byte.
    const formatted = out.success && out.results ? formatWebSearchResults(out.results) : null;
    globalThis.fetch = origFetch;

    lines.push(JSON.stringify({ label: c.label, resultJson: JSON.stringify(out), formatted }));
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`web-search-tool oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('web-search-tool oracle', async () => {
  await main();
});
