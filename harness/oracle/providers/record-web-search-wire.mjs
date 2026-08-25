/**
 * Serper web-search-wire recorder (W4.7f).
 *
 * Drives v4's REAL Serper plugin `executeSearch` + `formatResults`
 * (qtap-plugin-search-serper/index.ts) with `global.fetch` mocked, capturing the
 * request, the mapped `SearchOutput`, and the formatted strings. The Rust
 * `tools::web_search` differential (`web_search_wire_equivalence`) diffs
 * `build_serper_request` / `map_serper_results` / `serper_plugin_error` /
 * `format_web_search_results` against these rows.
 *
 * The env-var FALLBACK error set (`executeSerperFallback`) lives in the main-app
 * handler, not the plugin, and is verified in the tier-3 `web_search_tool` regen.
 *
 * P4.59 added two things the registered arm needs now that it is LIVE: the
 * request HEADERS (the plugin sends a `User-Agent` the legacy fallback does not),
 * and the plugin's SECOND fetch site — `validateApiKey`, which the API-keys
 * screen's Test button reaches through `searchProviderRegistry.validateApiKey`.
 *
 * Run FROM the serper plugin directory (Node 24, tsx), TZ=UTC (formatResults'
 * `toLocaleDateString`):
 *   cd ~/source/quilltap-server/plugins/dist/qtap-plugin-search-serper
 *   TZ=UTC node <V5>/harness/oracle/providers/record-web-search-wire.mjs --out /tmp/ws.ndjson
 */

import { writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {};
  for (let i = 0; i < args.length; i += 2) out[args[i].replace(/^--/, '')] = args[i + 1];
  return out;
}

function makeResponse(status, statusText, bodyText) {
  return new Response(bodyText, { status, statusText, headers: { 'content-type': 'application/json' } });
}

const ORG = (i) => ({ title: `Result ${i}`, link: `https://ex/${i}`, snippet: `snippet ${i}`, date: i === 1 ? '2026-06-15T00:00:00.000Z' : undefined });

const SEARCH_CASES = [
  {
    name: 'kg_unshift_under_capacity', query: 'cats', maxResults: 5,
    wire: { status: 200, statusText: 'OK', body: JSON.stringify({ organic: [ORG(1), ORG(2)], knowledgeGraph: { title: 'Cat', description: 'A cat is a feline.', source: { name: 'wiki', link: 'https://wiki/cat' } } }) },
  },
  {
    name: 'kg_at_capacity_no_unshift', query: 'cats', maxResults: 2,
    wire: { status: 200, statusText: 'OK', body: JSON.stringify({ organic: [ORG(1), ORG(2)], knowledgeGraph: { title: 'Cat', description: 'A cat is a feline.', source: { link: 'https://wiki/cat' } } }) },
  },
  {
    name: 'kg_no_title_no_source', query: 'dogs', maxResults: 5,
    wire: { status: 200, statusText: 'OK', body: JSON.stringify({ organic: [], knowledgeGraph: { description: 'A dog.' } }) },
  },
  {
    name: 'no_kg', query: 'birds', maxResults: 5,
    wire: { status: 200, statusText: 'OK', body: JSON.stringify({ organic: [ORG(1)] }) },
  },
  { name: 'error_401', query: 'x', maxResults: 5, wire: { status: 401, statusText: 'Unauthorized', body: 'nope' } },
  { name: 'error_403', query: 'x', maxResults: 5, wire: { status: 403, statusText: 'Forbidden', body: 'nope' } },
  { name: 'error_429', query: 'x', maxResults: 5, wire: { status: 429, statusText: 'Too Many Requests', body: 'slow down' } },
  { name: 'error_500', query: 'x', maxResults: 5, wire: { status: 500, statusText: 'Internal Server Error', body: 'boom' } },
  { name: 'network_error', query: 'x', maxResults: 5, networkError: 'socket hang up' },
];

const FORMAT_CASES = [
  { name: 'iso_date', results: [{ title: 'A', url: 'https://a', snippet: 'sa', publishedDate: '2026-06-15T00:00:00.000Z' }] },
  { name: 'freeform_invalid_date', results: [{ title: 'B', url: 'https://b', snippet: 'sb', publishedDate: '2 days ago' }] },
  { name: 'no_date', results: [{ title: 'C', url: 'https://c', snippet: 'sc' }] },
  { name: 'multi', results: [{ title: 'A', url: 'https://a', snippet: 'sa', publishedDate: '2020-12-31T23:00:00.000Z' }, { title: 'B', url: 'https://b', snippet: 'sb' }] },
  { name: 'empty', results: [] },
];

/**
 * `validateApiKey` cases — the plugin's second fetch site. It POSTs a fixed
 * minimal search (`{q: 'test', num: 1}`) and answers `response.ok`; any throw is
 * caught and answers false.
 */
const VALIDATE_CASES = [
  { name: 'validate_ok', wire: { status: 200, statusText: 'OK', body: '{}' } },
  { name: 'validate_401', wire: { status: 401, statusText: 'Unauthorized', body: 'nope' } },
  { name: 'validate_429', wire: { status: 429, statusText: 'Too Many Requests', body: 'slow' } },
  { name: 'validate_500', wire: { status: 500, statusText: 'Internal Server Error', body: 'boom' } },
  { name: 'validate_network_error', networkError: 'socket hang up' },
];

async function main() {
  const args = parseArgs();
  const outPath = args.out;
  if (!outPath) {
    console.error('usage: --out <ndjson>');
    process.exit(1);
  }
  const mod = await import(pathToFileURL(resolve('index.ts')));
  const plugin = mod.plugin;
  const lines = [];

  for (const c of SEARCH_CASES) {
    let captured = null;
    const origFetch = globalThis.fetch;
    globalThis.fetch = async (url, init) => {
      const u = typeof url === 'string' ? url : (url && url.url) || String(url);
      const method = (init && init.method) || 'POST';
      let body = (init && init.body) || null;
      if (body && typeof body !== 'string') body = new TextDecoder().decode(body);
      if (!captured) {
        // P4.59: headers too. The plugin sends `User-Agent` (the legacy
        // fallback in the main-app handler does not), and once the provider is
        // registered that header is on the real wire.
        const headers = {};
        const raw = (init && init.headers) || {};
        for (const [k, v] of Object.entries(raw)) headers[k] = String(v);
        captured = { method, url: u, body, headers };
      }
      if (c.networkError) throw new Error(c.networkError);
      return makeResponse(c.wire.status, c.wire.statusText, c.wire.body);
    };
    let output;
    try {
      output = await plugin.executeSearch(c.query, c.maxResults, 'test-key', undefined);
    } finally {
      globalThis.fetch = origFetch;
    }
    lines.push(JSON.stringify({
      kind: 'search',
      case: c.name,
      query: c.query,
      maxResults: c.maxResults,
      request: captured,
      wire: c.wire ?? null,
      networkError: c.networkError ?? null,
      output: { success: output.success, results: output.results ?? null, error: output.error ?? null },
    }));
  }

  for (const c of VALIDATE_CASES) {
    let captured = null;
    const origFetch = globalThis.fetch;
    globalThis.fetch = async (url, init) => {
      const u = typeof url === 'string' ? url : (url && url.url) || String(url);
      const method = (init && init.method) || 'POST';
      let body = (init && init.body) || null;
      if (body && typeof body !== 'string') body = new TextDecoder().decode(body);
      if (!captured) {
        const headers = {};
        const raw = (init && init.headers) || {};
        for (const [k, v] of Object.entries(raw)) headers[k] = String(v);
        captured = { method, url: u, body, headers };
      }
      if (c.networkError) throw new Error(c.networkError);
      return makeResponse(c.wire.status, c.wire.statusText, c.wire.body);
    };
    let valid;
    try {
      valid = await plugin.validateApiKey('test-key', undefined);
    } finally {
      globalThis.fetch = origFetch;
    }
    lines.push(JSON.stringify({
      kind: 'validate',
      case: c.name,
      request: captured,
      wire: c.wire ?? null,
      networkError: c.networkError ?? null,
      valid,
    }));
  }

  for (const c of FORMAT_CASES) {
    const formatted = plugin.formatResults(c.results);
    lines.push(JSON.stringify({ kind: 'format', case: c.name, results: c.results, formatted }));
  }

  writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`web-search-wire oracle wrote ${outPath} (${lines.length} rows)\n`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
