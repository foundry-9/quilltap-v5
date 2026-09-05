/**
 * @jest-environment node
 *
 * P4.9I2A tier-1 ORACLE — v4's REAL `resolveHelpContentForUrl` /
 * `resolveAllHelpContentForUrl` / `matchUrlPattern`
 * (`lib/help-chat/context-resolver.ts`) over the committed corpus
 * (`fixtures/help-context-resolver.json`: named document SETS + `{set, url}`
 * rows), with `@/lib/help-search` mocked to the corpus document list — the
 * same mock shape as v4's own `__tests__/unit/lib/help-chat/context-resolver.test.ts`.
 *
 * Emits per url row:
 *   { kind: 'resolve', set, url, primary: {matchType, id} | null, all: [{matchType, id}…] }
 *
 * Run (Node 24, from the v4 checkout — mirror to /tmp; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TMPO=/tmp/qt-help-resolver-oracle; rm -rf $TMPO; mkdir -p $TMPO/cases $TMPO/fixtures
 *   cp $V5W/harness/oracle/cases/help-context-resolver.test.ts $TMPO/cases/
 *   cp $V5W/harness/oracle/fixtures/help-context-resolver.json $TMPO/fixtures/
 *   QT_ORACLE_OUT=/tmp/oracle-help-resolver.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots $TMPO/cases -- help-context-resolver
 */

import * as fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

interface Doc { id: string; slug: string; title: string; path: string; url: string; content: string }
interface Spec { sets: Record<string, Doc[]>; urls: Array<{ set: string; url: string }> }

let currentDocs: Doc[] = [];
const mockHelpSearch = {
  isLoaded: () => true,
  loadFromDatabase: async () => undefined,
  listDocuments: async () => currentDocs.map(({ id, slug, title, path, url }) => ({ id, slug, title, path, url })),
  getDocument: async (idOrSlug: string) => currentDocs.find((d) => d.id === idOrSlug || d.slug === idOrSlug) || null,
};

jest.mock('@/lib/logger', () => ({
  logger: { child: () => ({ debug: jest.fn(), info: jest.fn(), warn: jest.fn(), error: jest.fn() }) },
}));
jest.mock('@/lib/help-search', () => ({ getHelpSearch: () => mockHelpSearch }));

import { resolveHelpContentForUrl, resolveAllHelpContentForUrl } from '@/lib/help-chat/context-resolver';

test('help-context-resolver oracle', async () => {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'help-context-resolver.json'), 'utf8')) as Spec;
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  // The context carries title/content/url/matchType; the doc id is recovered
  // by (url, content) — every corpus doc has a unique content string.
  const idOf = (ctx: { content: string; url: string } | null) =>
    ctx ? currentDocs.find((d) => d.content === ctx.content && d.url === ctx.url)?.id ?? '<unknown>' : null;

  const lines: string[] = [];
  for (const row of spec.urls) {
    currentDocs = spec.sets[row.set];
    const primary = await resolveHelpContentForUrl(row.url);
    const all = await resolveAllHelpContentForUrl(row.url);
    lines.push(JSON.stringify({
      kind: 'resolve',
      set: row.set,
      url: row.url,
      primary: primary ? { matchType: primary.matchType, id: idOf(primary) } : null,
      all: all.map((c) => ({ matchType: c.matchType, id: idOf(c) })),
    }));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`help-context-resolver oracle wrote ${outPath} (${lines.length} rows)\n`);
});
