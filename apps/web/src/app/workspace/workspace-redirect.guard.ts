/**
 * Old-route → workspace redirect guard (port of v4
 * `lib/navigation/workspace-redirect.ts`, baseline `b8b12695`).
 *
 * When the tabbed workspace is enabled, the legacy per-surface routes redirect
 * into `/workspace` carrying a transient `?open=` intent that the host consumes
 * and strips. When the flag is off this is a no-op and the old routes render
 * their views as before — so the cutover is a single flag flip.
 *
 * ⚠ Two v4 docstrings LIE (m6 F4): they call this dev-only / "flag off (the
 * default)". The flag is ON by default; this guard IS the default shell.
 *
 * v4 does this with a server-component `redirect()` at the top of each route;
 * v5 uses a functional `CanActivateFn` returning a `UrlTree` — one guard factory
 * bound per route with a per-route `open` value and query mapper.
 *
 * @module workspace/workspace-redirect.guard
 */

import { inject } from '@angular/core';
import { ActivatedRouteSnapshot, type CanActivateFn, Router, type UrlTree } from '@angular/router';

import { isWorkspaceTabsEnabled } from './workspace-flag';

type ParamMapper = (route: ActivatedRouteSnapshot) => Record<string, string | null | undefined>;
type BypassPredicate = (route: ActivatedRouteSnapshot) => boolean;

/**
 * Redirect to `/workspace?open=<open>` (carrying the mapped params) when the
 * workspace is enabled; otherwise let the legacy route render.
 *
 * `bypass` is an escape hatch that lets a route render routed/standalone even
 * while the workspace is on. It exists for ONE case (p4.9j3 item 4): the
 * fresh-instance wizard handoff `/settings/wizard?mode=setup` (from the shell's
 * first-run gate). The workspace contract is FROZEN and declares no
 * `SettingsWizardTabPayload`, so `mode=setup` cannot ride a tab payload; v4 has
 * no equivalent workspace arm (its setup pages are a documented divergence,
 * `m6-screen-parity.md` §1.7), so there is no oracle to violate — the fresh
 * wizard renders like the other pre-operational gate screens. A DOCUMENTED
 * DIVERGENCE, not a contract edit.
 */
export function workspaceRedirectGuard(
  open: string,
  params?: ParamMapper,
  bypass?: BypassPredicate,
): CanActivateFn {
  return (route): boolean | UrlTree => {
    if (!isWorkspaceTabsEnabled()) return true;
    if (bypass?.(route)) return true;
    const router = inject(Router);
    const queryParams: Record<string, string> = { open };
    if (params) {
      for (const [k, v] of Object.entries(params(route))) {
        if (v) queryParams[k] = v;
      }
    }
    return router.createUrlTree(['/workspace'], { queryParams });
  };
}
