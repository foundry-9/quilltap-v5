/**
 * changePassphrase cross-compat, BOTH directions (P4.4 unit 1; extended for
 * P4.D68 / v4 4.8.1 bug 60 — one pepper, one file).
 *
 * Direction 1 (`QT_DBKEY_V5_FIXTURE`) — v4 reads v5: v4's REAL dbkey code
 * unlocks a `.dbkey` that v5 minted and re-wrapped via `change_passphrase`.
 * The Rust differential (`provisioning_equivalence.rs`,
 * `writes_v5_changed_passphrase_dbkey_for_v4`) writes a `quilltap.dbkey` to
 * `QT_DBKEY_V5_OUT` that wraps the known TEST_PEPPER under the passphrase
 * "beta". This script points v4's real `unlockDbKey` at it and asserts the
 * recovered pepper equals TEST_PEPPER — proving a v5-rewrapped key is
 * v4-compatible. Since bug 60 it ALSO asserts the v5 rewrap produced exactly
 * one `.dbkey` file (no phantom `quilltap-llm-logs.dbkey`) — re-add the second
 * write in v5 and this script goes red.
 *
 * Direction 2 (`QT_DBKEY_V4_OUT`) — v5 reads v4: drive v4's REAL
 * `storeEnvPepperInDbKey` + `changePassphrase` in a scratch data dir (known
 * TEST_PEPPER via ENCRYPTION_MASTER_PEPPER, wrapped under "alpha", re-wrapped
 * to "beta"), assert v4 itself wrote exactly one `.dbkey` file (at 4.8.1 the
 * `quilltap-llm-logs.dbkey` write is gone — re-add it in v4 and this goes
 * red), and leave the dir for the Rust side
 * (`reads_v4_changed_passphrase_dbkey`, env `QT_DBKEY_V4_FIXTURE`) to unlock
 * with v5's ported reader.
 *
 * Each direction is its own process invocation — v4's dbkey module caches
 * state on `global` and resolves paths from `QUILLTAP_DATA_DIR` at import, so
 * the two must not share a process.
 *
 * Run from the PINNED v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd <pinned v4 worktree>            # e.g. /tmp/qt-v4-pin-<order>-<sha>
 *   # Direction 2 first (writes the v4 fixture the Rust test reads):
 *   QT_DBKEY_V4_OUT=/tmp/qt-v4-dbkey \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/provision/verify-dbkey-crosscompat.ts
 *   # Direction 1 AFTER the Rust test wrote the v5 key:
 *   QT_DBKEY_V5_FIXTURE=/tmp/qt-v5-dbkey \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/provision/verify-dbkey-crosscompat.ts
 */

import { existsSync, readdirSync, mkdirSync, rmSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const TEST_PEPPER = '3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=';

/** The version floor v4's guard writes into `.dbkey` (any semver will do). */
const MIN_SERVER_VERSION = '4.9.0-dev.0';

/**
 * Plant `minServerVersion` exactly as v4's version guard does
 * (`lib/startup/version-guard.ts:243-252` — read, JSON.parse, assign, write back
 * 2-space pretty with mode 0600). Transcribed rather than called because
 * `patchDbKeyFileVersion` is module-private and its public entrance
 * (`storeCurrentVersion`) wants a live SQLite backend.
 */
function plantMinServerVersion(dbkeyPath: string): void {
  const data = JSON.parse(readFileSync(dbkeyPath, 'utf-8'));
  data.minServerVersion = MIN_SERVER_VERSION;
  writeFileSync(dbkeyPath, JSON.stringify(data, null, 2), { mode: 0o600 });
}

function readMinServerVersion(dbkeyPath: string): unknown {
  return JSON.parse(readFileSync(dbkeyPath, 'utf-8')).minServerVersion;
}

/** The one-file contract (bug 60): the data dir holds exactly `quilltap.dbkey`. */
function assertOneDbKeyFile(dataDir: string, side: string): void {
  const dbkeyFiles = readdirSync(dataDir).filter((f) => f.endsWith('.dbkey')).sort();
  if (dbkeyFiles.length !== 1 || dbkeyFiles[0] !== 'quilltap.dbkey') {
    throw new Error(
      `${side}: expected exactly [quilltap.dbkey] in ${dataDir}, got [${dbkeyFiles.join(', ')}]`
    );
  }
}

/** Direction 1 — v4's real dbkey code unlocks the v5-rewrapped key. */
async function v4ReadsV5(dir: string): Promise<void> {
  // v4 `getDataDir()` is `<QUILLTAP_DATA_DIR>/data`, so the .dbkey lives under data/.
  if (!existsSync(join(dir, 'data', 'quilltap.dbkey'))) {
    throw new Error(`no data/quilltap.dbkey in ${dir} (run the Rust differential first)`);
  }

  // One pepper, one file: the v5 rewrap must NOT have minted the phantom
  // per-database key file (v4 4.8.1, bug 60).
  assertOneDbKeyFile(join(dir, 'data'), 'v5 rewrap');

  // dbkey.ts reads the file from getDataDir(); point QUILLTAP_DATA_DIR at the
  // base and clear any env pepper so provisionDbKey takes the .dbkey path.
  process.env.QUILLTAP_DATA_DIR = dir;
  delete process.env.ENCRYPTION_MASTER_PEPPER;
  process.env.LOG_LEVEL = 'error';

  const { provisionDbKey, unlockDbKey, getDbKeyState } = await import('@/lib/startup/dbkey');

  // A passphrase-protected .dbkey resolves to needs-passphrase.
  const state = await provisionDbKey();
  if (state !== 'needs-passphrase') {
    throw new Error(`expected needs-passphrase, got ${state}`);
  }

  // The wrong passphrase fails; "beta" (the v5 re-wrap) succeeds.
  if (unlockDbKey('wrong')) {
    throw new Error('v4 unlocked the v5 key with the WRONG passphrase');
  }
  // unlockDbKey requires the state to still be needs-passphrase after a failure.
  if (getDbKeyState() !== 'needs-passphrase') {
    throw new Error(`state left ${getDbKeyState()} after a failed unlock`);
  }
  if (!unlockDbKey('beta')) {
    throw new Error('v4 could NOT unlock the v5 change-passphrase key with "beta"');
  }

  const recovered = process.env.ENCRYPTION_MASTER_PEPPER;
  if (recovered !== TEST_PEPPER) {
    throw new Error(`recovered pepper mismatch: got ${recovered}`);
  }

  // P4.46: the Rust side planted `minServerVersion` before its re-wrap. v5's
  // read-modify-write must have preserved it AND the result must still be a
  // file v4's real loader opens (asserted above, since we got here).
  const preserved = readMinServerVersion(join(dir, 'data', 'quilltap.dbkey'));
  if (preserved !== MIN_SERVER_VERSION) {
    throw new Error(
      `v5's re-wrap dropped minServerVersion: expected ${MIN_SERVER_VERSION}, got ${String(preserved)}`
    );
  }

  process.stderr.write(
    'OK: v4 unlocked the v5 change-passphrase .dbkey (pepper matches, one file, minServerVersion preserved)\n'
  );
}

/** Direction 2 — v4's REAL changePassphrase writes the fixture v5 reads. */
async function v4WritesForV5(dir: string): Promise<void> {
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(join(dir, 'data'), { recursive: true });

  // Known pepper in, so the Rust side can assert the exact recovered value:
  // env pepper + no .dbkey → needs-vault-storage → storeEnvPepperInDbKey
  // wraps TEST_PEPPER under "alpha" and resolves.
  process.env.QUILLTAP_DATA_DIR = dir;
  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.LOG_LEVEL = 'error';

  const { provisionDbKey, storeEnvPepperInDbKey, changePassphrase } = await import(
    '@/lib/startup/dbkey'
  );

  const state = await provisionDbKey();
  if (state !== 'needs-vault-storage') {
    throw new Error(`expected needs-vault-storage, got ${state}`);
  }
  storeEnvPepperInDbKey('alpha');

  // P4.46: plant the version floor v4's own guard writes, so this run MEASURES
  // what v4's changePassphrase does to a field it does not model.
  const dbkeyPath = join(dir, 'data', 'quilltap.dbkey');
  plantMinServerVersion(dbkeyPath);

  // The real changePassphrase — the unit under test at the 4.8.1 baseline.
  const wrongOld = changePassphrase('wrong', 'beta');
  if (wrongOld.success) {
    throw new Error('v4 changePassphrase accepted a WRONG old passphrase');
  }
  const result = changePassphrase('alpha', 'beta');
  if (!result.success) {
    throw new Error(`v4 changePassphrase failed: ${result.error}`);
  }

  // One pepper, one file (bug 60): at 4.8.1 v4 writes ONLY quilltap.dbkey.
  // Re-adding the quilltap-llm-logs.dbkey write turns this red.
  assertOneDbKeyFile(join(dir, 'data'), 'v4 rewrap');

  // ── The measured, DELIBERATE divergence (P4.46) ────────────────────────────
  // v4's changePassphrase hands writeDbKeyFile a freshly built encryptPepper
  // object (`lib/startup/dbkey.ts:542-548`), so it DROPS minServerVersion. That
  // is survivable in v4 — storeCurrentVersion() rewrites the field on the next
  // startup — and unsurvivable in v5, which has no version guard and would
  // never write it again. v5 therefore preserves unknown fields on purpose.
  // If v4 ever adopts the read-modify-write, this assertion goes red and the
  // divergence retires to a plain equality.
  const afterV4 = readMinServerVersion(dbkeyPath);
  if (afterV4 !== undefined) {
    throw new Error(
      `v4's changePassphrase now PRESERVES minServerVersion (got ${String(afterV4)}) — ` +
        `the P4.46 divergence has converged; retire it and assert equality on both sides`
    );
  }

  // Re-plant so the fixture the Rust side reads carries a v4-authored floor:
  // `reads_v4_changed_passphrase_dbkey` drives v5's change_passphrase over it
  // and asserts the field survives.
  plantMinServerVersion(dbkeyPath);

  process.stderr.write(
    `OK: v4 changePassphrase wrote one .dbkey and DROPPED minServerVersion (measured) → ${dir} ` +
      `(reads_v4_changed_passphrase_dbkey unlocks it under v5 and preserves the re-planted floor)\n`
  );
}

async function main(): Promise<void> {
  const v5Fixture = process.env.QT_DBKEY_V5_FIXTURE;
  const v4Out = process.env.QT_DBKEY_V4_OUT;
  if (v5Fixture && v4Out) {
    throw new Error('set exactly ONE of QT_DBKEY_V5_FIXTURE / QT_DBKEY_V4_OUT per invocation');
  }
  if (v5Fixture) {
    await v4ReadsV5(v5Fixture);
  } else if (v4Out) {
    await v4WritesForV5(v4Out);
  } else {
    throw new Error(
      'set QT_DBKEY_V5_FIXTURE (v4 reads v5) or QT_DBKEY_V4_OUT (v4 writes for v5)'
    );
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`dbkey cross-compat FAILED: ${err?.stack ?? err}\n`);
  process.exit(1);
});
