import { spawn, spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, rmSync, writeFileSync, openSync } from 'node:fs';
import { resolve } from 'node:path';

import { makeDbKeyFile } from './support/dbkey';
import {
  ARTIFACTS_DIR,
  BASE_URL,
  cliBinary,
  E2E_PASSPHRASE,
  FIXTURE_USER,
  FIXTURES_DIR,
  INSTANCE_DATA_DIR,
  INSTANCE_DIR,
  PID_FILE,
  PORT,
  SERVER_LOG,
  SINGLE_USER_ID,
  spaDir,
  TEST_PEPPER,
  webBinary,
} from './support/env';

/**
 * Playwright global setup: build a passphrase-LOCKED copy of the committed
 * chat-send fixture and launch the REAL axum server against it + the built SPA.
 * The fixture is COPIED (never the committed original is mutated).
 *
 * Prerequisites (documented in apps/web/README.md): `cargo build -p quilltap-web
 * -p quilltap-cli` and `npm run build` (the SPA dist) — this setup fails loud
 * with guidance if either is missing.
 */
export default async function globalSetup(): Promise<void> {
  const web = webBinary();
  const cli = cliBinary();
  if (!existsSync(web) || !existsSync(cli)) {
    throw new Error(
      `Missing Rust binaries. Build them first:\n  cargo build -p quilltap-web -p quilltap-cli\n(looked for ${web} and ${cli})`,
    );
  }
  const dist = spaDir();
  if (!existsSync(resolve(dist, 'index.html'))) {
    throw new Error(`Missing SPA build at ${dist}. Build it first:\n  npm run build`);
  }

  // Fresh instance dir from the committed fixture (copy, never mutate original).
  rmSync(ARTIFACTS_DIR, { recursive: true, force: true });
  mkdirSync(INSTANCE_DATA_DIR, { recursive: true });
  copyFileSync(resolve(FIXTURES_DIR, 'chat-send-main.db'), resolve(INSTANCE_DATA_DIR, 'quilltap.db'));
  copyFileSync(resolve(FIXTURES_DIR, 'chat-send-mount.db'), resolve(INSTANCE_DATA_DIR, 'quilltap-mount-index.db'));

  // Lock the instance: a user-passphrase .dbkey wrapping the test pepper (and NO
  // env pepper when we launch → the server boots `needs-passphrase`). Written
  // BEFORE the migrations so the CLI can unlock via the passphrase (this also
  // exercises the Node-generated .dbkey against the real reader).
  writeFileSync(resolve(INSTANCE_DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));

  // Bring the fixture schema/data up to what the engine reads (mirrors the Rust
  // test harness `common::materialize_fixture_instance`): the `turnSkippingEnabled`
  // column (v4 add-turn-skipping-field-v1) and the user-id rewrite to the engine's
  // SINGLE_USER_ID (so `listChats` — filtered by that id — sees the chats). The
  // CLI unlocks the .dbkey via QUILLTAP_DB_PASSPHRASE.
  runCliWrite(cli, 'ALTER TABLE chats ADD COLUMN turnSkippingEnabled INTEGER;', { allowFail: true });
  runCliWrite(cli, `UPDATE chats SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`);

  // Launch the real server (no env pepper → locked) serving the built SPA.
  const logFd = openSync(SERVER_LOG, 'w');
  const child = spawn(
    web,
    ['--host', '127.0.0.1', '--port', String(PORT), '--data-dir', INSTANCE_DIR, '--spa-dir', dist],
    { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
  );
  child.unref();
  writeFileSync(PID_FILE, String(child.pid));

  await waitForHealth();
}

function runCliWrite(cli: string, sql: string, opts: { allowFail?: boolean } = {}): void {
  // The CLI `--data-dir` is the INSTANCE dir (it appends `/data` — resolve.rs);
  // it unlocks the .dbkey via QUILLTAP_DB_PASSPHRASE.
  const res = spawnSync(cli, ['db', '--data-dir', INSTANCE_DIR, '--write', sql], {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  if (res.status !== 0 && !opts.allowFail) {
    throw new Error(`CLI migration failed (${sql}):\n${res.stdout}\n${res.stderr}`);
  }
}

/** The server env WITHOUT any inherited pepper (so it boots locked). */
function withoutPepper(): NodeJS.ProcessEnv {
  const env = { ...process.env };
  delete env['ENCRYPTION_MASTER_PEPPER'];
  return env;
}

/** Poll `/health` until the server answers (423 locked is "ready" for the e2e). */
async function waitForHealth(): Promise<void> {
  const deadline = Date.now() + 30_000;
  let lastErr = '';
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${BASE_URL}/health`);
      if (res.status === 423 || res.status === 200) {
        return;
      }
      lastErr = `health status ${res.status}`;
    } catch (e) {
      lastErr = e instanceof Error ? e.message : String(e);
    }
    await sleep(300);
  }
  throw new Error(`server did not become ready within 30s (${lastErr}); see ${SERVER_LOG}`);
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}
