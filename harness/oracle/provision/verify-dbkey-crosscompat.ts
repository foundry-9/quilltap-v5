/**
 * changePassphrase cross-compat (P4.4 unit 1): v4's REAL dbkey code unlocks a
 * `.dbkey` that v5 minted and re-wrapped via `change_passphrase`.
 *
 * The Rust differential (`provisioning_equivalence.rs`,
 * `writes_v5_changed_passphrase_dbkey_for_v4`) writes a `quilltap.dbkey` to
 * `QT_DBKEY_V5_OUT` that wraps the known TEST_PEPPER under the passphrase
 * "beta". This script points v4's real `unlockDbKey` at it and asserts the
 * recovered pepper equals TEST_PEPPER — proving a v5-rewrapped key is
 * v4-compatible. (The reverse — v5 reads any v4-written `.dbkey` — is the
 * Friday-verified read path; `change_passphrase` emits the byte-identical
 * format `save_dbkey`/v4 `saveDbKey` do.)
 *
 * Run from the v4 checkout under Node 24, AFTER the Rust test wrote the key:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_DBKEY_V5_FIXTURE=/tmp/qt-v5-dbkey \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/provision/verify-dbkey-crosscompat.ts
 */

import { existsSync } from 'node:fs';
import { join } from 'node:path';

const TEST_PEPPER = '3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=';

async function main(): Promise<void> {
  const dir = process.env.QT_DBKEY_V5_FIXTURE;
  if (!dir) throw new Error('QT_DBKEY_V5_FIXTURE must point at the base dir (holds data/quilltap.dbkey)');
  // v4 `getDataDir()` is `<QUILLTAP_DATA_DIR>/data`, so the .dbkey lives under data/.
  if (!existsSync(join(dir, 'data', 'quilltap.dbkey'))) {
    throw new Error(`no data/quilltap.dbkey in ${dir} (run the Rust differential first)`);
  }

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

  process.stderr.write('OK: v4 unlocked the v5 change-passphrase .dbkey (pepper matches)\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`dbkey cross-compat FAILED: ${err?.stack ?? err}\n`);
  process.exit(1);
});
