/**
 * OpenAI moderation-wire recorder (W4.7f).
 *
 * Drives v4's REAL `moderationPlugin.moderate` (qtap-plugin-openai/
 * moderation-provider.ts) with `global.fetch` mocked to a committed wire payload,
 * capturing the request the plugin builds + the parsed `ModerationResult` OR the
 * thrown error string. The Rust `services::dangerous_content::moderation_wire`
 * differential (`moderation_wire_equivalence`) diffs `build_moderation_request` /
 * `parse_moderation_response` against these rows.
 *
 * Run FROM the openai plugin directory (Node 24, tsx):
 *   cd ~/source/quilltap-server/plugins/dist/qtap-plugin-openai
 *   node <V5>/harness/oracle/providers/record-moderation-wire.mjs --out /tmp/mod.ndjson
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

function makeResponse(status, bodyText) {
  return new Response(bodyText, { status, headers: { 'content-type': 'application/json' } });
}

const CASES = [
  {
    name: 'clean',
    content: 'a friendly hello',
    wire: { status: 200, body: JSON.stringify({ id: 'm1', model: 'omni', results: [{ flagged: false, categories: { sexual: false, hate: false }, category_scores: { sexual: 0.0012, hate: 0.00005 } }] }) },
  },
  {
    name: 'flagged',
    content: 'something bad',
    wire: { status: 200, body: JSON.stringify({ id: 'm2', model: 'omni', results: [{ flagged: true, categories: { sexual: true, violence: false }, category_scores: { sexual: 0.95, violence: 0.02 } }] }) },
  },
  {
    name: 'missing_score_defaults_zero',
    content: 'partial scores',
    wire: { status: 200, body: JSON.stringify({ id: 'm3', model: 'omni', results: [{ flagged: false, categories: { sexual: false, 'self-harm': true }, category_scores: { sexual: 0.01 } }] }) },
  },
  {
    name: 'empty_results',
    content: 'nothing',
    wire: { status: 200, body: JSON.stringify({ id: 'm4', model: 'omni', results: [] }) },
  },
  {
    name: 'http_error',
    content: 'x',
    wire: { status: 429, body: 'rate limit exceeded' },
  },
  {
    name: 'http_error_500',
    content: 'x',
    wire: { status: 500, body: 'internal error' },
  },
  {
    name: 'base_url_override',
    content: 'proxied',
    baseUrl: 'https://proxy.example/',
    wire: { status: 200, body: JSON.stringify({ id: 'm5', model: 'omni', results: [{ flagged: false, categories: { hate: false }, category_scores: { hate: 0.001 } }] }) },
  },
];

async function main() {
  const args = parseArgs();
  const outPath = args.out;
  if (!outPath) {
    console.error('usage: --out <ndjson>');
    process.exit(1);
  }
  const mod = await import(pathToFileURL(resolve('moderation-provider.ts')));
  const plugin = mod.moderationPlugin;
  const lines = [];

  for (const c of CASES) {
    let captured = null;
    const origFetch = globalThis.fetch;
    globalThis.fetch = async (url, init) => {
      if (!captured) {
        const u = typeof url === 'string' ? url : (url && url.url) || String(url);
        const method = (init && init.method) || 'POST';
        let body = (init && init.body) || null;
        if (body && typeof body !== 'string') body = new TextDecoder().decode(body);
        captured = { method, url: u, body };
      }
      return makeResponse(c.wire.status, c.wire.body);
    };

    let outcome = 'ok';
    let result = null;
    let thrown = null;
    try {
      const r = await plugin.moderate(c.content, 'test-key', c.baseUrl);
      result = { flagged: r.flagged, categories: r.categories };
    } catch (e) {
      outcome = 'thrown';
      thrown = e instanceof Error ? e.message : String(e);
    } finally {
      globalThis.fetch = origFetch;
    }

    lines.push(JSON.stringify({
      case: c.name,
      content: c.content,
      baseUrl: c.baseUrl ?? null,
      request: captured,
      wire: c.wire,
      outcome,
      result,
      thrown,
    }));
  }

  writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`moderation-wire oracle wrote ${outPath} (${lines.length} cases)\n`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
