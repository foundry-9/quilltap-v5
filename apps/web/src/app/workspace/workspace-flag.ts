/**
 * The tabbed-workspace feature flag (port of v4 `lib/config/feature-flags.ts`,
 * baseline `b8b12695`).
 *
 * **Default ON** — the workspace is v5's post-login landing surface (m6 F1), the
 * legacy per-surface routes redirect into it, and the old routes still render
 * their views when the flag is off.
 *
 * **v5 divergence (documented):** v4 gates this with a build-time env var
 * (`NEXT_PUBLIC_WORKSPACE_TABS`); v5 ships one binary with no per-deploy build
 * env, so the supported opt-out becomes a per-browser localStorage key with
 * identical `!== '0'` semantics. Read ONCE at bootstrap and cached (v4 evaluates
 * its flag at module load); a stale value can't drift mid-session.
 *
 * @module workspace/workspace-flag
 */

export const WORKSPACE_TABS_KEY = 'quilltap.workspace.tabs';

let cached: boolean | null = null;

export function isWorkspaceTabsEnabled(): boolean {
  if (cached !== null) return cached;
  let enabled = true;
  try {
    enabled = localStorage.getItem(WORKSPACE_TABS_KEY) !== '0';
  } catch {
    // Storage unavailable — default ON.
  }
  cached = enabled;
  return enabled;
}

/** Test-only: clear the module-level cache between cases. */
export function resetWorkspaceTabsFlagCache(): void {
  cached = null;
}
