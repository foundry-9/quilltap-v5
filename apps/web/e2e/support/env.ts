import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

// Playwright transpiles config/spec TS to CommonJS, so `__dirname` is the
// portable way to anchor paths here (import.meta is unavailable in that mode).
/** Repo root (…/apps/web/e2e/support → up four). */
export const REPO_ROOT = resolve(__dirname, '../../../..');
export const APP_ROOT = resolve(REPO_ROOT, 'apps/web');
export const ARTIFACTS_DIR = resolve(APP_ROOT, 'e2e/.artifacts');
export const INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'instance');
export const INSTANCE_DATA_DIR = resolve(INSTANCE_DIR, 'data');
export const SERVER_LOG = resolve(ARTIFACTS_DIR, 'server.log');
export const PID_FILE = resolve(ARTIFACTS_DIR, 'server.pid');

export const FIXTURES_DIR = resolve(REPO_ROOT, 'crates/quilltap-web/tests/fixtures');

/** The committed fixture's test pepper + user id (harness corpus; synthetic). */
export const TEST_PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
export const FIXTURE_USER = 'e18e05bc-63e8-4539-8a85-719b7a508850';
export const SINGLE_USER_ID = 'ffffffff-ffff-ffff-ffff-ffffffffffff';

/**
 * The fixed port the M4 mock LLM listens on. The fixture's OPENAI_COMPATIBLE
 * profile `baseUrl` is rewritten to this in global setup (pre-server — the CLI
 * write-lock refuses once the server holds the instance); the M4 spec starts
 * the mock on this port in-worker.
 */
export const MOCK_LLM_PORT = 45301;

/** The passphrase the e2e locks the fixture behind, and the wrong one it tries. */
export const E2E_PASSPHRASE = 'open sesame please';
export const E2E_WRONG_PASSPHRASE = 'not the passphrase';

export const PORT = 4319;
export const BASE_URL = `http://127.0.0.1:${PORT}`;

/** The built quilltap-web binary (base-commit build — see the README prereq). */
export function webBinary(): string {
  const release = resolve(REPO_ROOT, 'target/release/quilltap-web');
  const debug = resolve(REPO_ROOT, 'target/debug/quilltap-web');
  return existsSync(release) ? release : debug;
}

/** The built quilltap CLI binary (for the fixture-schema migration). */
export function cliBinary(): string {
  const release = resolve(REPO_ROOT, 'target/release/quilltap');
  const debug = resolve(REPO_ROOT, 'target/debug/quilltap');
  return existsSync(release) ? release : debug;
}

/** The built SPA directory (`--spa-dir` target): dist/quilltap/browser or dist/quilltap. */
export function spaDir(): string {
  const withBrowser = resolve(APP_ROOT, 'dist/quilltap/browser');
  const flat = resolve(APP_ROOT, 'dist/quilltap');
  if (existsSync(resolve(withBrowser, 'index.html'))) {
    return withBrowser;
  }
  return flat;
}
