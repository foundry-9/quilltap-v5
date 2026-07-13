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
  MOCK_LLM_PORT,
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
  // The terminal PTY spawns with its cwd at `<data>/files` and transcripts under
  // `<data>/logs/terminals`; the manager doesn't create the cwd (P4.6u), so make
  // them here (a real instance provisions them).
  mkdirSync(resolve(INSTANCE_DATA_DIR, 'files'), { recursive: true });
  mkdirSync(resolve(INSTANCE_DATA_DIR, 'logs'), { recursive: true });
  copyFileSync(resolve(FIXTURES_DIR, 'salon-main.db'), resolve(INSTANCE_DATA_DIR, 'quilltap.db'));
  copyFileSync(resolve(FIXTURES_DIR, 'salon-mount.db'), resolve(INSTANCE_DATA_DIR, 'quilltap-mount-index.db'));

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
  // The Salon fixture predates terminal support; the terminal routes (P4.6u) need
  // the `terminal_sessions` table (the P4.1c DDL, verbatim from the Rust web test
  // harness `common::materialize_fixture_instance`). IF NOT EXISTS keeps it
  // idempotent; this is fixture-schema materialization, NOT a fixture regen.
  runCliWrite(
    cli,
    'CREATE TABLE IF NOT EXISTS terminal_sessions (' +
      'id TEXT PRIMARY KEY, chatId TEXT, label TEXT, shell TEXT, cwd TEXT, ' +
      'startedAt TEXT, exitedAt TEXT, exitCode REAL, transcriptPath TEXT, ' +
      'createdAt TEXT, updatedAt TEXT);',
  );
  // The Salon fixture may predate Document Mode; the P4.6x document dispatch
  // (lane B, wired at unification) reads/writes `chat_documents`. The columns
  // match `quilltap-core::db::chat_documents` (the frozen v4 schema). IF NOT
  // EXISTS keeps it a no-op when the fixture already carries the table — this is
  // schema materialization, NOT a fixture regen (the terminal_sessions precedent).
  runCliWrite(
    cli,
    'CREATE TABLE IF NOT EXISTS chat_documents (' +
      'id TEXT PRIMARY KEY, chatId TEXT, filePath TEXT, scope TEXT, ' +
      'mountPoint TEXT, displayTitle TEXT, isActive INTEGER, ' +
      'createdAt TEXT, updatedAt TEXT);',
  );
  // The Salon fixture predates embedding profiles; the P4.6z Scriptorium scan
  // enqueues mount-chunk embeddings, whose `default_profile_id` read touches
  // `embedding_profiles` (in the MAIN db). With the table absent the read errors
  // and the whole scan fails; an EMPTY table lets the enqueue skip gracefully
  // (no default profile → 0 jobs). IF NOT EXISTS keeps it a no-op when present —
  // schema materialization, NOT a fixture regen (the terminal_sessions precedent).
  runCliWrite(
    cli,
    'CREATE TABLE IF NOT EXISTS embedding_profiles (' +
      'id TEXT PRIMARY KEY NOT NULL, userId TEXT NOT NULL, name TEXT NOT NULL, ' +
      'provider TEXT NOT NULL, apiKeyId TEXT, baseUrl TEXT, modelName TEXT NOT NULL, ' +
      'dimensions REAL, truncateToDimensions REAL, normalizeL2 INTEGER DEFAULT 1, ' +
      "isDefault INTEGER DEFAULT 0, tags TEXT DEFAULT '[]', " +
      'createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);',
  );
  for (const table of ['chats', 'connection_profiles', 'api_keys', 'chat_settings', 'characters', 'tags', 'projects', 'memories']) {
    runCliWrite(cli, `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`, {
      allowFail: true,
    });
  }
  // Point the fixture's OPENAI_COMPATIBLE profile at the M4 mock LLM — this must
  // happen BEFORE the server launches (the CLI write-lock refuses a live holder),
  // so the mock listens on the fixed MOCK_LLM_PORT and the spec starts it there.
  runCliWrite(
    cli,
    `UPDATE connection_profiles SET baseUrl = 'http://127.0.0.1:${MOCK_LLM_PORT}/v1' WHERE provider = 'OPENAI_COMPATIBLE';`,
  );

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
