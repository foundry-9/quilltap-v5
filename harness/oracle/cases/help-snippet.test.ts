/**
 * @jest-environment node
 *
 * P4.9I2A tier-1 ORACLE — the Guide text search: v4's REAL
 * `GET /api/v1/help-docs?action=search&q=` handler (`app/api/v1/help-docs/route.ts`
 * — `buildSnippet` + the match mapper + the title-hits-first sort) driven over
 * the committed corpus (`fixtures/help-snippet.json`: docs + queries) with
 * `@/lib/help-search` mocked to the corpus (no DB) and the request-context
 * middleware collapsed to a passthrough. `buildSnippet` is module-private in v4,
 * so the handler IS the unit.
 *
 * Emits per query: { kind: 'search', q, status, matches }
 *
 * Run (Node 24, from the v4 checkout — mirror to /tmp; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TMPO=/tmp/qt-help-snippet-oracle; rm -rf $TMPO; mkdir -p $TMPO/cases $TMPO/fixtures
 *   cp $V5W/harness/oracle/cases/help-snippet.test.ts $TMPO/cases/
 *   cp $V5W/harness/oracle/fixtures/help-snippet.json $TMPO/fixtures/
 *   QT_ORACLE_OUT=/tmp/oracle-help-snippet.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots $TMPO/cases -- help-snippet
 */

import * as fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

interface Doc { slug: string; title: string; content: string }
interface Spec { docs: Doc[]; queries: string[] }

let currentDocs: Array<Doc & { id: string; path: string; url: string }> = [];
const mockHelpSearch = {
  isLoaded: () => true,
  loadFromDatabase: async () => undefined,
  getAllDocuments: async () => currentDocs,
  listDocuments: async () => currentDocs.map(({ id, slug, title, path, url }) => ({ id, slug, title, path, url })),
  getDocument: async (x: string) => currentDocs.find((d) => d.id === x || d.slug === x) || null,
};

jest.mock('@/lib/help-search', () => ({ getHelpSearch: () => mockHelpSearch }));
jest.mock('@/lib/logging/create-logger', () => ({
  createServiceLogger: () => ({ debug: jest.fn(), info: jest.fn(), warn: jest.fn(), error: jest.fn() }),
}));
// Collapse the request-context middleware to a passthrough: the search handler
// reads nothing from the context. (The `actions` helpers stay REAL.)
jest.mock('@/lib/api/middleware', () => ({
  createContextHandler: (h: (req: unknown, ctx: unknown) => Promise<unknown>) => (req: unknown) =>
    h(req, { user: { id: 'u' }, repos: {} }),
}));

import { GET } from '@/app/api/v1/help-docs/route';

test('help-snippet oracle', async () => {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'help-snippet.json'), 'utf8')) as Spec;
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  currentDocs = spec.docs.map((d, i) => ({ ...d, id: `id-${i}`, path: `help/${d.slug}.md`, url: '/' }));

  const lines: string[] = [];
  for (const q of spec.queries) {
    const url = `http://localhost/api/v1/help-docs?action=search&q=${encodeURIComponent(q)}`;
    const req = { method: 'GET', url, nextUrl: new URL(url), headers: new Headers(), json: async () => ({}) };
    const resp = (await (GET as unknown as (r: unknown) => Promise<{ status: number; json: () => Promise<unknown> }>)(req));
    lines.push(JSON.stringify({ kind: 'search', q, status: resp.status, ...(await resp.json() as object) }));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`help-snippet oracle wrote ${outPath} (${lines.length} rows)\n`);
});
