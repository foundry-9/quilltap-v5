/**
 * @jest-environment node
 *
 * P4.6z `systemBrowseDirectory` route ORACLE: drives v4's REAL
 * `app/api/v1/system/browse-directory/route.ts` GET handler over the committed
 * `browse-fs-tree/` fixture, emitting `{name, status, body}` per case so the
 * Rust port (`api::system::browse_directory`) can be diffed byte-for-byte.
 *
 * The route is DB-FREE (it reads the host filesystem directly), so the ONLY
 * seam mocked is the auth context middleware — `createContextHandler` becomes a
 * pass-through that calls the real inner handler with an empty context (the
 * handler never reads it). The fixture tree is read-only, so both sides point at
 * the SAME committed directory; every absolute occurrence of the tree root is
 * sentinel-rewritten to `<BASE>` on both sides for checkout/machine independence
 * (the P4.6v "fs basePath sentinel rewrite" gotcha).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-browse-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/browse-directory.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_BROWSE_FS_TREE=$V5W/crates/quilltap-web/tests/fixtures/browse-fs-tree \
 *   QT_ORACLE_OUT=/tmp/oracle-browse-directory.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- browse-directory
 * Then diff:
 *   QT_ORACLE_BROWSE_DIR=/tmp/oracle-browse-directory.ndjson \
 *     cargo test -p quilltap-web --test browse_directory_equivalence
 */

import * as fs from 'fs';

const SENTINEL = '<BASE>';

function mockRequest(url: string): unknown {
  return {
    method: 'GET',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
  };
}

function applyMocks(): void {
  // The route's ONLY non-fs dependency is `createContextHandler`. Replace it
  // with a pass-through so the REAL inner handler runs without the DB/session
  // `buildRequestContext` (which browse-directory never consults).
  jest.doMock('@/lib/api/middleware', () => ({
    __esModule: true,
    createContextHandler:
      (handler: (req: unknown, ctx: unknown) => Promise<unknown>) => (req: unknown) =>
        handler(req, {}),
  }));
}

async function loadRoute(): Promise<{ GET: (req: unknown) => Promise<unknown> }> {
  return (await import('@/app/api/v1/system/browse-directory/route')) as never;
}

/** Rewrite every occurrence of the absolute tree root → the sentinel. */
function sentinelize(value: unknown, treeRoot: string): unknown {
  if (typeof value === 'string') {
    return value.split(treeRoot).join(SENTINEL);
  }
  if (Array.isArray(value)) {
    return value.map((v) => sentinelize(v, treeRoot));
  }
  if (value && typeof value === 'object') {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value)) {
      out[k] = sentinelize(v, treeRoot);
    }
    return out;
  }
  return value;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

async function main(): Promise<void> {
  const treeRoot = process.env.QT_BROWSE_FS_TREE;
  if (!treeRoot) throw new Error('QT_BROWSE_FS_TREE must point at the committed browse-fs-tree');
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  process.env.LOG_LEVEL = 'error';

  const B = 'http://localhost/api/v1/system/browse-directory';
  const q = (p: string) => `${B}?path=${encodeURIComponent(p)}`;

  // Every case browses at/under the tree root so the response's absolute paths
  // are all sentinel-rewritable (the `parent` of `root` is the tree root itself).
  const cases: Array<{ name: string; path: string }> = [
    // Listing + hidden-dir skip + dir-only filter + localeCompare sort
    // (apple/Banana/cherry: byte order is Banana,apple,cherry; localeCompare is
    // apple,Banana,cherry — the case that distinguishes them). `.hidden` skipped,
    // `zeta.txt` (a file) excluded.
    { name: 'list-root', path: `${treeRoot}/root` },
    // Nested listing + parent computation (parent of cherry = <BASE>/root).
    { name: 'nested-cherry', path: `${treeRoot}/root/cherry` },
    // A leaf directory with a single child.
    { name: 'deep', path: `${treeRoot}/root/cherry/deep` },
    // Nonexistent path → 400 text.
    { name: 'nonexistent', path: `${treeRoot}/root/nope` },
    // A file, not a directory → 400 text.
    { name: 'file-not-dir', path: `${treeRoot}/root/zeta.txt` },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    jest.resetModules();
    applyMocks();
    const route = await loadRoute();
    const { status, body } = await respond(await route.GET(mockRequest(q(c.path))));
    outLines.push(
      JSON.stringify({ name: c.name, status, body: sentinelize(body, treeRoot) }),
    );
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`browse-directory oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('browse-directory oracle', async () => {
  await main();
});
