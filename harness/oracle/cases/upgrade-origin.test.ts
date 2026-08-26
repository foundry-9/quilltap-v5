/**
 * @jest-environment node
 *
 * Differential oracle — the WebSocket upgrade ORIGIN gate (P4.D124, v4
 * `f3892158d`). Drives v4's REAL `authenticateUpgrade`
 * (`lib/realtime/upgrade-auth.ts`) over a fixed (origin, host) corpus and emits
 * each `{ok, reason}`.
 *
 * v4's `checkOrigin` is module-private, so the exported `authenticateUpgrade`
 * is the entrance. Its OTHER two checks are mocked out of the way — a live
 * session and an unlocked instance — because neither ports (v5 has no session
 * auth by design, D2, and the locked case is answered 503 before the upgrade).
 * What survives is exactly the arm v5 implements. This is the DB-free
 * route-guard oracle shape: `jest.doMock`s and no fixture.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   TMPO=/tmp/qt-upgrade-origin-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/upgrade-origin.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-upgrade-origin.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=60000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- upgrade-origin
 */

import * as fs from 'fs';

interface Case {
  name: string;
  origin?: string;
  host?: string;
}

const CASES: Case[] = [
  // 1. No Origin at all — a non-browser client. ALLOWED.
  { name: 'no_origin', host: 'localhost:4319' },
  { name: 'no_origin_no_host' },
  // 2. The opaque `null` origin — a sandboxed/redirected page. ALLOWED.
  { name: 'literal_null_origin', origin: 'null', host: 'localhost:4319' },
  // 3. Same origin, in the shapes a real deployment produces.
  { name: 'same_origin_localhost', origin: 'http://localhost:4319', host: 'localhost:4319' },
  { name: 'same_origin_loopback_ip', origin: 'http://127.0.0.1:4319', host: '127.0.0.1:4319' },
  { name: 'same_origin_https_default_port', origin: 'https://quilltap.example', host: 'quilltap.example' },
  { name: 'same_origin_http_default_port', origin: 'http://quilltap.example', host: 'quilltap.example' },
  // …the DEFAULT-PORT normalization: `new URL('https://x:443').host` is 'x'.
  { name: 'https_explicit_default_port_matches_bare_host', origin: 'https://quilltap.example:443', host: 'quilltap.example' },
  { name: 'http_explicit_default_port_matches_bare_host', origin: 'http://quilltap.example:80', host: 'quilltap.example' },
  // …and a NON-default port is kept, so it must be present in Host too.
  { name: 'explicit_nondefault_port_needs_it_in_host', origin: 'https://quilltap.example:8443', host: 'quilltap.example' },
  // 4. Cross-origin refusals.
  { name: 'cross_origin_different_host', origin: 'http://evil.example', host: 'localhost:4319' },
  { name: 'cross_origin_different_port', origin: 'http://localhost:9999', host: 'localhost:4319' },
  { name: 'cross_origin_subdomain', origin: 'http://sub.quilltap.example', host: 'quilltap.example' },
  // A scheme change alone is NOT cross-origin by this test (host is compared).
  { name: 'scheme_differs_host_matches', origin: 'https://localhost:4319', host: 'localhost:4319' },
  // 5. An Origin with no Host.
  { name: 'origin_without_host', origin: 'http://localhost:4319' },
  // 6. Unparseable origins.
  { name: 'unparseable_origin_bare_word', origin: 'not a url', host: 'localhost:4319' },
  { name: 'unparseable_origin_empty_after_scheme', origin: 'http://', host: 'localhost:4319' },
  { name: 'unparseable_origin_scheme_only', origin: 'http:', host: 'localhost:4319' },
  // 7. An EMPTY Origin header is falsy in v4's `if (!origin)` — allowed.
  { name: 'empty_origin_string', origin: '', host: 'localhost:4319' },
];

function applyMocks(): void {
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    getServerSession: async () => ({ user: { id: 'user-1' } }),
  }));
  jest.doMock('@/lib/startup/startup-state', () => ({
    __esModule: true,
    startupState: { isLockedMode: () => false },
  }));
}

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const rows: string[] = [];
  for (const c of CASES) {
    jest.resetModules();
    applyMocks();
    const { authenticateUpgrade } = await import('@/lib/realtime/upgrade-auth');
    const headers: Record<string, string> = {};
    if (c.origin !== undefined) headers.origin = c.origin;
    if (c.host !== undefined) headers.host = c.host;
    const result = await authenticateUpgrade({ url: '/x', headers } as never);
    rows.push(
      JSON.stringify({
        name: c.name,
        input: { ...(c.origin !== undefined ? { origin: c.origin } : {}), ...(c.host !== undefined ? { host: c.host } : {}) },
        ok: result.ok,
        ...(result.ok ? {} : { reason: (result as { reason: string }).reason }),
      }),
    );
  }
  fs.writeFileSync(outPath, rows.join('\n') + '\n');
}

it('emits the upgrade-origin oracle', async () => {
  await main();
}, 60000);
