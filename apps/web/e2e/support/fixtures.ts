/**
 * The shared Playwright test wrapper (P4.9J1).
 *
 * The tabbed-workspace flag defaults ON, which would redirect every legacy
 * route into `/workspace`. To keep the ENTIRE existing suite exercising v4's
 * supported ROUTE mode byte-unchanged, this wrapper injects the per-browser
 * opt-out (`quilltap.workspace.tabs = '0'`) before app boot on every context —
 * so specs need only change their import to `./support/fixtures`.
 *
 * The workspace beats (`workspace-flow.spec.ts`) deliberately import the BASE
 * `@playwright/test` instead, so the flag stays ON (default) for them.
 *
 * Re-exports everything a spec needs (`expect`, `request`, and all types) so the
 * only change per spec is the module specifier.
 *
 * @module e2e/support/fixtures
 */

import { test as base } from '@playwright/test';

export const test = base.extend({
  context: async ({ context }, use) => {
    await context.addInitScript(() => {
      try {
        window.localStorage.setItem('quilltap.workspace.tabs', '0');
      } catch {
        /* storage unavailable — nothing to opt out of */
      }
    });
    await use(context);
  },
});

export { expect, request } from '@playwright/test';
export type * from '@playwright/test';
