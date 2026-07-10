import { Injectable, signal } from '@angular/core';

/**
 * A one-shot flag for the fresh-instance → setup-wizard handoff (v4
 * `app/setup/page.tsx` `navigateAfterSetup`: after minting the key, a
 * profile-less instance is routed into the provider wizard). The setup screen
 * sets the flag on completion; the shell consumes it once on mount and routes to
 * `/settings/wizard?mode=setup`.
 */
@Injectable({ providedIn: 'root' })
export class FirstRunService {
  private readonly pending = signal(false);

  /** Mark that a first-run setup just completed (route to the wizard next). */
  markSetupComplete(): void {
    this.pending.set(true);
  }

  /** Read-and-clear the flag (returns whether the wizard handoff should fire). */
  consume(): boolean {
    const was = this.pending();
    this.pending.set(false);
    return was;
  }
}
