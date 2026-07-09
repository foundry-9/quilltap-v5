import { existsSync, readFileSync, rmSync } from 'node:fs';

import { ARTIFACTS_DIR, PID_FILE } from './support/env';

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
  rmSync(ARTIFACTS_DIR, { recursive: true, force: true });
}
