import { copyFileSync, existsSync, readFileSync, rmSync } from 'node:fs';

import { ARTIFACTS_DIR, PID_FILE, SERVER_LOG } from './support/env';

/** Kill the server the setup launched and clear the copied instance. */
export default async function globalTeardown(): Promise<void> {
  if (existsSync(PID_FILE)) {
    const pid = Number(readFileSync(PID_FILE, 'utf8').trim());
    if (Number.isFinite(pid)) {
      try {
        // The child was `detached` — signal the process group to also reap it.
        process.kill(-pid, 'SIGTERM');
      } catch {
        try {
          process.kill(pid, 'SIGTERM');
        } catch {
          // already gone
        }
      }
    }
  }
  // `E2E_KEEP_SERVER_LOG=<path>` copies the server's log out before the
  // artifacts go — the only way to read a failed beat's server side after
  // the run (the `p4.9i2` unification diagnosed an empty Guide blind).
  const keep = process.env['E2E_KEEP_SERVER_LOG'];
  if (keep && existsSync(SERVER_LOG)) copyFileSync(SERVER_LOG, keep);
  rmSync(ARTIFACTS_DIR, { recursive: true, force: true });
}
