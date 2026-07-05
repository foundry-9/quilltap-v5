/**
 * @jest-environment node
 *
 * Differential ORACLE for the W4.1d5 `search_web` tool. DB-FREE: the whole search
 * boundary (`searchProviderRegistry` + the API-key lookup + the Serper fallback)
 * is jest-mocked (the injected seam the Rust port also mocks). Drives v4's REAL
 * `executeWebSearchTool` + `formatWebSearchResults` over a canned-outcome corpus,
 * emitting one NDJSON line per case: { label, resultJson, formatted }.
 *
 * TZ MUST be UTC (the built-in formatter's `publishedDate` uses
 * `toLocaleDateString()` — the Rust port reproduces it in UTC).
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
  | { kind: 'provider'; requiresApiKey: boolean; apiKeyPresent: boolean; success: boolean; results: Result[]; error?: string | null }
  | { kind: 'missing_key'; displayName: string }
  | { kind: 'not_configured' };

// Mutable holders the mocks read (set per case before the dynamic import).
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
    { label: 'success_with_dates', args: { query: 'latest AI news' }, outcome: { kind: 'provider', requiresApiKey: false, apiKeyPresent: false, success: true, results: [r('Quantum leap', '2026-06-15T00:00:00.000Z'), r('More news')] } },
    { label: 'success_maxresults', args: { query: 'tokyo weather', maxResults: 3 }, outcome: { kind: 'provider', requiresApiKey: false, apiKeyPresent: false, success: true, results: [r('Sunny in Tokyo', '2020-12-31T23:00:00.000Z')] } },
    { label: 'success_no_results', args: { query: 'obscure query' }, outcome: { kind: 'provider', requiresApiKey: false, apiKeyPresent: false, success: true, results: [] } },
    { label: 'provider_failure', args: { query: 'fail me' }, outcome: { kind: 'provider', requiresApiKey: false, apiKeyPresent: false, success: false, results: [], error: 'quota exceeded' } },
    { label: 'provider_failure_no_error', args: { query: 'fail silently' }, outcome: { kind: 'provider', requiresApiKey: false, apiKeyPresent: false, success: false, results: [], error: null } },
    { label: 'missing_api_key', args: { query: 'needs a key' }, outcome: { kind: 'missing_key', displayName: 'Serper' } },
    { label: 'not_configured', args: { query: 'nothing configured' }, outcome: { kind: 'not_configured' } },
    { label: 'validation_empty', args: { query: '   ' }, outcome: { kind: 'not_configured' } },
    { label: 'validation_nonobject', args: 'nope', outcome: { kind: 'not_configured' } },
  ];

  const lines: string[] = [];

  for (const c of cases) {
    jest.resetModules();
    outcome = c.outcome;
    delete process.env.SERPER_API_KEY; // so `not_configured` truly falls through

    jest.doMock('@/lib/logger', () => {
      const l = { child: jest.fn(), debug: jest.fn(), info: jest.fn(), warn: jest.fn(), error: jest.fn() };
      l.child.mockReturnValue(l);
      return { logger: l };
    });

    jest.doMock('@/lib/plugins/search-provider-registry', () => ({
      searchProviderRegistry: {
        isSearchConfigured: () => outcome.kind !== 'not_configured',
        getDefaultProvider: () => {
          if (outcome.kind === 'not_configured') return undefined;
          const requiresApiKey =
            outcome.kind === 'missing_key' ? true : (outcome as { requiresApiKey: boolean }).requiresApiKey;
          const displayName = outcome.kind === 'missing_key' ? outcome.displayName : 'Serper';
          return {
            metadata: { providerName: 'serper', displayName },
            config: { requiresApiKey },
            // No `formatResults` → the built-in formatter is exercised.
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
          // `missing_key` → no active key; `provider` with apiKeyPresent → a key.
          getAllApiKeys: async () =>
            outcome.kind === 'provider' && outcome.apiKeyPresent
              ? [{ provider: 'serper', isActive: true, key_value: 'k' }]
              : [],
        },
      }),
    }));

    const { executeWebSearchTool, formatWebSearchResults } = await import(
      '@/lib/tools/handlers/web-search-handler'
    );
    const out = await executeWebSearchTool(c.args, { userId: 'user-1' });
    const formatted = out.success && out.results ? formatWebSearchResults(out.results) : null;

    lines.push(JSON.stringify({ label: c.label, resultJson: JSON.stringify(out), formatted }));
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`web-search-tool oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('web-search-tool oracle', async () => {
  await main();
});
