import { expect, test, type Page } from '@playwright/test';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: rides the SHARED global-setup server (unlocked by foundation.spec).
 * "workspace-flow" ('w') sorts last, so it never disturbs the route-mode specs
 * that run before it.
 *
 * P4.9J1 — the tabbed workspace, flag ON. Unlike every other spec (which
 * imports `./support/fixtures` to inject the `quilltap.workspace.tabs = '0'`
 * opt-out and run v4's supported ROUTE mode), THIS file imports the BASE
 * `@playwright/test` so the flag stays ON (the default) — exercising the
 * workspace shell for real.
 *
 * Beats (the tier-1 floor):
 *   1. `/` redirects to `/workspace?open=home` → a Home tab, URL stripped clean.
 *   2. a rail click opens a second tab; re-clicking de-dupes (focuses, no dup).
 *   3. `Ctrl+Alt+\` splits off the active tab; a reload restores the layout.
 *   4. a deep-link `/settings?tab=…&section=…` redirect carries its intent and
 *      strips the URL — the settings tab is a loud not-yet-wired pane in-lane
 *      (its real screen lands at unification).
 */

async function openWorkspace(page: Page): Promise<void> {
  await page.goto('/');
  // Unlock only if the passphrase screen is showing (the shared server may stay
  // unlocked across contexts).
  const passphrase = page.locator('#qt-passphrase');
  const workspace = page.locator('.qt-workspace');
  await expect(passphrase.or(workspace).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
  await expect(workspace).toBeVisible({ timeout: 15_000 });
}

const railLink = (page: Page, label: string) =>
  page.locator(`aside.qt-left-sidebar a.qt-collapsed-nav-button[aria-label="${label}"]`);
const tabs = (page: Page) => page.locator('.qt-tab-strip .qt-tab');
const tabLabel = (page: Page, text: string) =>
  page.locator('.qt-tab-strip .qt-tab-label', { hasText: text });

test('flag-on: / redirects into the workspace with a Home tab', async ({ page }) => {
  await openWorkspace(page);
  // The ?open=home intent applied, then the URL was stripped clean.
  await expect(page).toHaveURL(/\/workspace$/);
  await expect(tabLabel(page, 'Home')).toBeVisible();
  await expect(tabs(page)).toHaveCount(1);
});

test('a rail click opens a second tab; re-clicking de-dupes', async ({ page }) => {
  await openWorkspace(page);
  await railLink(page, 'Characters').click();
  await expect(tabLabel(page, 'Characters')).toBeVisible();
  await expect(tabs(page)).toHaveCount(2);
  // De-dupe: opening the same surface again just focuses it.
  await railLink(page, 'Characters').click();
  await expect(tabs(page)).toHaveCount(2);
});

test('Ctrl+Alt+\\ splits the workspace; a reload restores the layout', async ({ page }) => {
  await openWorkspace(page);
  await railLink(page, 'Characters').click();
  await expect(tabs(page)).toHaveCount(2);

  // Move focus onto the pane (off the rail anchor), then split off the active tab.
  await page.locator('.qt-workspace').click({ position: { x: 5, y: 5 } });
  await page.keyboard.press('Control+Alt+\\');
  await expect(page.locator('.qt-workspace-divider')).toBeVisible();

  // Let the debounced persist land, then reload and confirm the split layout
  // (two tabs across two panes) is restored from localStorage.
  await page.waitForTimeout(400);
  await page.reload();
  await expect(page.locator('.qt-workspace')).toBeVisible({ timeout: 15_000 });
  await expect(page.locator('.qt-workspace-divider')).toBeVisible();
  await expect(tabs(page)).toHaveCount(2);
});

test('a deep-link /settings redirect carries its intent and lands on a not-wired pane', async ({
  page,
}) => {
  await page.goto('/settings?tab=system&section=memory');
  const passphrase = page.locator('#qt-passphrase');
  const workspace = page.locator('.qt-workspace');
  await expect(passphrase.or(workspace).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
  await expect(workspace).toBeVisible({ timeout: 15_000 });
  // Redirected into the workspace and the intent params stripped.
  await expect(page).toHaveURL(/\/workspace$/);
  // The settings tab opened (v4 default title "The Foundry")…
  await expect(tabLabel(page, 'The Foundry')).toBeVisible();
  // …and renders the loud not-yet-wired pane in-lane (real screen at unify).
  await expect(page.locator('[data-not-wired][data-kind="settings"]')).toBeVisible();
});
