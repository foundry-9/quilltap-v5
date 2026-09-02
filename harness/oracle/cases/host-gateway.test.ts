/**
 * @jest-environment node
 *
 * P4.71 HOST-GATEWAY ORACLE (tier 1, exact) — drives v4's REAL
 * `lib/host-rewrite.ts` (v4 `6d2a50382`): `isVMEnvironment()`,
 * `resolveHostGateway()` (module-private — reached through the public rewrite,
 * which is how production reaches it too) and `rewriteLocalhostUrl()`.
 *
 * The module is impure twice over: it reads `process.env.QUILLTAP_HOST_IP` and
 * calls `isDockerEnvironment()` from `@/lib/paths`, and it caches the resolved
 * gateway in a module-level `let` for the life of the process. Both are handled
 * here rather than reimplemented:
 *   - `@/lib/paths` is `jest.doMock`ed ONCE with `isDockerEnvironment` reading a
 *     mutable flag this file owns. (A `doMock` survives `jest.resetModules()`,
 *     so re-mocking per row would be both unnecessary and a way to get it
 *     wrong — see the `jest-domock-survives-resetmodules` note.)
 *   - `@/lib/logger` is mocked with a recorder, so the four log lines v4 emits —
 *     their level, message bytes and context bag — are part of the comparand
 *     rather than something the Rust port merely claims to have transcribed.
 *   - `jest.resetModules()` + a fresh `require` per row clears the cache, so
 *     every row measures the uncached path.
 *
 * What it emits, per case:
 *   - `rewrite` — the full matrix {env unset, '', an IP, a hostname} × {docker
 *     true/false} × six URLs: `isVMEnvironment()`, the returned string, and
 *     every log line emitted during the call, in order. The URL set covers the
 *     three localhost spellings v4 knows, a non-localhost host, an unparseable
 *     string, and an upper-case LOCALHOST (WHATWG lowercases the host, so it
 *     rewrites — the arm a naive `===` port fails).
 *   - `cache` — two rewrites of the same localhost URL on a freshly-reset
 *     module, per environment: v4 resolves (and logs the resolution) exactly
 *     once, so the second call carries only the debug line.
 *
 * ⚠ ORDER IS PART OF THE CONTRACT. v4 returns before resolving for a
 * non-localhost or unparseable URL, so those rows must show NO resolution line
 * even in an environment that would have resolved happily. That silence is
 * what the corpus pins.
 *
 * Run (Node 24, from the v4 checkout — copied to a /tmp mirror; jest ignores
 * `.claude/` venues):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-host-gateway-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/host-gateway.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-host-gateway.ndjson \
 *     $N/npx jest --silent --watchman=false \
 *       --roots "$PWD" --roots "$TMPO/cases" -- host-gateway
 */

import * as fs from 'fs';

// ── the injected environment ──────────────────────────────────────────────

/** What the mocked `isDockerEnvironment()` answers for the row being run. */
let dockerFlag = false;

/** Every line the mocked logger recorded during the row being run. */
interface LogLine {
  level: 'info' | 'warn' | 'debug' | 'error';
  message: string;
  /** v4's per-call context bag, or null when the call passed none. */
  context: Record<string, unknown> | null;
}
let logLines: LogLine[] = [];
/** The context v4 passes to `logger.child(...)` — asserted once, not per row. */
let childContext: Record<string, unknown> | null = null;

jest.doMock('@/lib/paths', () => ({
  ...jest.requireActual('@/lib/paths'),
  isDockerEnvironment: () => dockerFlag,
}));

jest.doMock('@/lib/logger', () => {
  const record =
    (level: LogLine['level']) =>
    (message: string, context?: Record<string, unknown>) => {
      logLines.push({ level, message, context: context ?? null });
    };
  const child = (ctx: Record<string, unknown>) => {
    childContext = ctx;
    return {
      info: record('info'),
      warn: record('warn'),
      debug: record('debug'),
      error: record('error'),
      child,
    };
  };
  return {
    logger: {
      info: record('info'),
      warn: record('warn'),
      debug: record('debug'),
      error: record('error'),
      child,
    },
  };
});

// ── the matrix ────────────────────────────────────────────────────────────

/** `null` = the var is DELETED (v4's `undefined`). */
const ENVS: { name: string; hostIp: string | null }[] = [
  { name: 'env-unset', hostIp: null },
  // v4's `!!process.env.X` / `if (envIP)` — an empty string is falsy, i.e. the
  // var may as well be unset. The arm that catches a Rust port comparing
  // `Option::is_some`.
  { name: 'env-empty', hostIp: '' },
  { name: 'env-ip', hostIp: '10.0.0.5' },
  { name: 'env-hostname', hostIp: 'my-host' },
];

const URLS: { name: string; url: string }[] = [
  { name: 'ollama', url: 'http://localhost:11434' },
  { name: 'lmstudio-127', url: 'http://127.0.0.1:1234/v1' },
  // Bracketed IPv6 loopback with a path, query AND fragment — the arm that
  // proves the re-serialization keeps everything after the authority.
  { name: 'ipv6-full', url: 'http://[::1]:8080/x?y=1#f' },
  { name: 'remote', url: 'https://api.openai.com/v1' },
  { name: 'unparseable', url: 'not a url' },
  // WHATWG lowercases the host, so this DOES rewrite.
  { name: 'uppercase-host', url: 'http://LOCALHOST:11434' },
];

/** Set/delete `QUILLTAP_HOST_IP` for a row. */
function applyEnv(hostIp: string | null): void {
  if (hostIp === null) {
    delete process.env.QUILLTAP_HOST_IP;
  } else {
    process.env.QUILLTAP_HOST_IP = hostIp;
  }
}

/** A freshly-cached copy of v4's real module (the `let` cache starts empty). */
function freshModule(): typeof import('@/lib/host-rewrite') {
  jest.resetModules();
  logLines = [];
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  return require('@/lib/host-rewrite');
}

describe('host-gateway oracle', () => {
  it('emits the corpus', () => {
    const outPath = process.env.QT_ORACLE_OUT;
    if (!outPath) {
      throw new Error('set QT_ORACLE_OUT to the NDJSON path');
    }
    const out = fs.createWriteStream(outPath);
    const write = (row: unknown) => out.write(`${JSON.stringify(row)}\n`);

    for (const env of ENVS) {
      for (const u of URLS) {
        applyEnv(env.hostIp);
        const mod = freshModule();
        const isVM = mod.isVMEnvironment();
        // `isVMEnvironment()` emits nothing, so the lines below belong to the
        // rewrite alone.
        const before = logLines.length;
        const rewritten = mod.rewriteLocalhostUrl(u.url);
        write({
          name: `${env.name}/docker-false/${u.name}`,
          kind: 'rewrite',
          hostIp: env.hostIp,
          isDocker: false,
          url: u.url,
          isVM,
          rewritten,
          logs: logLines.slice(before),
        });
      }
    }

    dockerFlag = true;
    for (const env of ENVS) {
      for (const u of URLS) {
        applyEnv(env.hostIp);
        const mod = freshModule();
        const isVM = mod.isVMEnvironment();
        const before = logLines.length;
        const rewritten = mod.rewriteLocalhostUrl(u.url);
        write({
          name: `${env.name}/docker-true/${u.name}`,
          kind: 'rewrite',
          hostIp: env.hostIp,
          isDocker: true,
          url: u.url,
          isVM,
          rewritten,
          logs: logLines.slice(before),
        });
      }
    }
    dockerFlag = false;

    // ── the cache: v4 resolves once per process ──
    for (const isDocker of [false, true]) {
      dockerFlag = isDocker;
      for (const env of ENVS) {
        applyEnv(env.hostIp);
        const mod = freshModule();
        const first = mod.rewriteLocalhostUrl('http://localhost:11434');
        const firstLogs = logLines.slice();
        const second = mod.rewriteLocalhostUrl('http://localhost:11434');
        const secondLogs = logLines.slice(firstLogs.length);
        write({
          name: `${env.name}/docker-${isDocker}/cache`,
          kind: 'cache',
          hostIp: env.hostIp,
          isDocker,
          url: 'http://localhost:11434',
          first,
          firstLogs,
          second,
          secondLogs,
        });
      }
    }
    dockerFlag = false;

    // The child-logger context, recorded once: v4's
    // `logger.child({ module: 'host-rewrite' })`.
    write({ name: 'child-context', kind: 'child_context', context: childContext });

    out.end();
    expect(true).toBe(true);
  });
});
