/**
 * @jest-environment node
 *
 * Differential ORACLE for the W4.1d5 `search_web` tool, REGENERATED for W4.7f to
 * verify the real `WebSearchProvider` (Serper wire) behind a canned transport.
 * Drives v4's REAL `executeWebSearchTool` + `formatWebSearchResults`:
 *   - provider-path cases mock `searchProviderRegistry.getDefaultProvider()` to a
 *     Serper provider whose `executeSearch` returns the REAL Serper strings the
 *     plugin yields (byte-verified against the real plugin in the tier-1
 *     `web_search_wire_equivalence`); the Rust side reproduces them via
 *     `RealWebSearchProvider` over a canned wire.
 *   - fallback-path cases leave the registry empty + set `SERPER_API_KEY` + mock
 *     `global.fetch`, driving the REAL `executeSerperFallback` (the handler's own
 *     env-var path — its DISTINCT error strings, previously untested at tier-3).
 * Emits one NDJSON line per case: { label, resultJson, formatted }.
 *
 * TZ MUST be UTC (the built-in formatter's `publishedDate`).
 *
 * Run (Node 24, from the v4 checkout; STAGE outside any .claude path):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; STAGE=/tmp/qt-oracle-stage
 *   cd ~/source/quilltap-server
 *   TZ=UTC QT_ORACLE_OUT=/tmp/oracle-web-search-tool.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- web-search-tool
 */

import * as fs from 'fs';

interface Result {
  title: string;
  url: string;
  snippet: string;
  publishedDate?: string;
}
type Outcome =
  | { kind: 'provider'; apiKeyPresent: boolean; success: boolean; results: Result[]; error?: string | null }
  | { kind: 'missing_key' }
  | { kind: 'fallback'; wire: { status: number; statusText: string; body: string } }
  | { kind: 'not_configured' };

let outcome: Outcome;

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const r = (title: string, publishedDate?: string): Result => ({
    title,
    url: `https://example.com/${title.replace(/\s+/g, '-').toLowerCase()}`,
    snippet: `A snippet about ${title}.`,
    publishedDate,
  });

  interface Case {
    label: string;
    args: unknown;
    outcome: Outcome;
  }
  const cases: Case[] = [
    // --- provider path (Serper plugin registered) ---
    { label: 'provider_success', args: { query: 'latest AI news' }, outcome: { kind: 'provider', apiKeyPresent: true, success: true, results: [r('Quantum leap', '2026-06-15T00:00:00.000Z'), r('More news')] } },
    { label: 'provider_success_maxresults', args: { query: 'tokyo weather', maxResults: 3 }, outcome: { kind: 'provider', apiKeyPresent: true, success: true, results: [r('Sunny in Tokyo', '2020-12-31T23:00:00.000Z')] } },
    // Lenient numbers (P4.d5 tier 2): a model quoting its number must reach the
    // SAME wire request as the bare one — `maxResults` is an llmNumber field, so
    // the parse replaces it and `num: 3` (not `"3"`) goes out. Both sides key the
    // canned transport on the exact request body, so a string leaking through to
    // the wire cannot pass.
    { label: 'lenient_quoted_maxresults', args: { query: 'tokyo weather', maxResults: '3' }, outcome: { kind: 'provider', apiKeyPresent: true, success: true, results: [r('Sunny in Tokyo', '2020-12-31T23:00:00.000Z')] } },
    // Refused, not coerced: `true` would become 1 under z.coerce.number().
    { label: 'lenient_true_refused', args: { query: 'tokyo weather', maxResults: true }, outcome: { kind: 'not_configured' } },
    { label: 'provider_no_results', args: { query: 'obscure' }, outcome: { kind: 'provider', apiKeyPresent: true, success: true, results: [] } },
    { label: 'provider_error_401', args: { query: 'x' }, outcome: { kind: 'provider', apiKeyPresent: true, success: false, results: [], error: 'Invalid Serper API key. Please check your API key in Settings > API Keys.' } },
    { label: 'provider_error_429', args: { query: 'x' }, outcome: { kind: 'provider', apiKeyPresent: true, success: false, results: [], error: 'Serper API rate limit exceeded. Please try again later or upgrade your plan at serper.dev.' } },
    { label: 'provider_error_500', args: { query: 'x' }, outcome: { kind: 'provider', apiKeyPresent: true, success: false, results: [], error: 'Serper API error: 500 Internal Server Error - boom' } },
    { label: 'provider_failure_no_error', args: { query: 'x' }, outcome: { kind: 'provider', apiKeyPresent: true, success: false, results: [], error: null } },
    { label: 'missing_api_key', args: { query: 'needs a key' }, outcome: { kind: 'missing_key' } },
    // --- fallback path (no plugin registered, SERPER_API_KEY set) — REAL handler ---
    { label: 'fallback_success', args: { query: 'fallback query' }, outcome: { kind: 'fallback', wire: { status: 200, statusText: 'OK', body: JSON.stringify({ organic: [{ title: 'F1', link: 'https://f/1', snippet: 's1', date: '2026-01-02T00:00:00.000Z' }], knowledgeGraph: { title: 'KG', description: 'kg desc', source: { link: 'https://kg' } } }) } } },
    { label: 'fallback_401', args: { query: 'x' }, outcome: { kind: 'fallback', wire: { status: 401, statusText: 'Unauthorized', body: 'nope' } } },
    { label: 'fallback_429', args: { query: 'x' }, outcome: { kind: 'fallback', wire: { status: 429, statusText: 'Too Many Requests', body: 'slow' } } },
    { label: 'fallback_500', args: { query: 'x' }, outcome: { kind: 'fallback', wire: { status: 500, statusText: 'Internal Server Error', body: 'boom' } } },
    { label: 'not_configured', args: { query: 'nothing' }, outcome: { kind: 'not_configured' } },
    // --- validation ---
    { label: 'validation_empty', args: { query: '   ' }, outcome: { kind: 'not_configured' } },
    { label: 'validation_nonobject', args: 'nope', outcome: { kind: 'not_configured' } },
  ];

  const lines: string[] = [];

  for (const c of cases) {
    jest.resetModules();
    outcome = c.outcome;
    delete process.env.SERPER_API_KEY;

    const origFetch = globalThis.fetch;
    if (outcome.kind === 'fallback') {
      process.env.SERPER_API_KEY = 'env-key';
      const w = outcome.wire;
      globalThis.fetch = (async () =>
        new Response(w.body, { status: w.status, statusText: w.statusText, headers: { 'content-type': 'application/json' } })) as typeof fetch;
    }

    jest.doMock('@/lib/logger', () => {
      const l = { child: jest.fn(), debug: jest.fn(), info: jest.fn(), warn: jest.fn(), error: jest.fn() };
      l.child.mockReturnValue(l);
      return { logger: l };
    });

    jest.doMock('@/lib/plugins/search-provider-registry', () => ({
      searchProviderRegistry: {
        isSearchConfigured: () => outcome.kind === 'provider' || outcome.kind === 'missing_key',
        getDefaultProvider: () => {
          if (outcome.kind !== 'provider' && outcome.kind !== 'missing_key') return undefined;
          return {
            metadata: { providerName: 'SERPER', displayName: 'Serper Web Search' },
            config: { requiresApiKey: true },
            executeSearch: async () => {
              if (outcome.kind !== 'provider') throw new Error('unexpected executeSearch');
              return { success: outcome.success, results: outcome.results, error: outcome.error ?? undefined };
            },
          };
        },
      },
    }));

    jest.doMock('@/lib/repositories/user-scoped', () => ({
      getUserRepositories: () => ({
        connections: {
          getAllApiKeys: async () =>
            outcome.kind === 'provider' && outcome.apiKeyPresent
              ? [{ provider: 'SERPER', isActive: true, key_value: 'k' }]
              : [],
        },
      }),
    }));

    const { executeWebSearchTool, formatWebSearchResults } = await import(
      '@/lib/tools/handlers/web-search-handler'
    );
    const out = await executeWebSearchTool(c.args, { userId: 'user-1' });
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
