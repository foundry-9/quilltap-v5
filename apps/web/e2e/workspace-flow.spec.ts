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
 *      strips the URL — landing on the REAL Settings screen with the payload
 *      tab active (activated at the p4.9j unification).
 *   5. (unify) the salon funnel: the rail's Chats leaves the workspace for the
 *      standalone `/salon` list (v4-faithful); a chat click funnels back in
 *      through the redirect guard and the salon tab renders the live
 *      conversation.
 *   6. (unify) the characters in-tab drill: a roster card click drills to the
 *      detail IN PLACE (no navigation); back restores the list.
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

test('a deep-link /settings redirect carries its intent onto the real Settings screen', async ({
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
  // …and renders the REAL Settings screen (activated at unification) with the
  // payload's tab seeded active — no not-wired pane in sight.
  await expect(page.locator('[data-not-wired]')).toHaveCount(0);
  await expect(
    page.locator('.qt-tab-group .qt-tab-active', { hasText: 'Data & System' }),
  ).toBeVisible();
});

test('a chat opened from the salon list funnels into a live workspace salon tab', async ({
  page,
}) => {
  await openWorkspace(page);
  // The rail's Chats leaves the workspace for the standalone /salon list
  // (v4-faithful: the salon LIST is not a tab kind).
  await railLink(page, 'Chats').click();
  const soloCard = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Solo Voyage' });
  await expect(soloCard).toBeVisible({ timeout: 15_000 });
  // The chat click hits /salon/:id, whose redirect guard funnels straight back
  // into the workspace as a salon tab carrying the chatId.
  await soloCard.click();
  await expect(page.locator('.qt-workspace')).toBeVisible({ timeout: 15_000 });
  await expect(page).toHaveURL(/\/workspace$/);
  // The REAL conversation renders inside the tab (activated at unification).
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
  await expect(page.locator('.qt-chat-composer-input .qt-rich-editor-content')).toBeVisible();
});

test('the characters tab drills to a detail in place and back restores the roster', async ({
  page,
}) => {
  await openWorkspace(page);
  await railLink(page, 'Characters').click();
  await expect(page.getByRole('heading', { name: 'Characters', exact: true })).toBeVisible({
    timeout: 15_000,
  });
  const aria = page
    .locator('.character-card-grid .character-card')
    .filter({ hasText: 'Aria' })
    .first();
  await aria.locator('p.line-clamp-3').click();
  // The drill renders the detail IN PLACE — still /workspace, no navigation.
  await expect(page.getByRole('heading', { name: 'Aria' })).toBeVisible({ timeout: 15_000 });
  await expect(page).toHaveURL(/\/workspace$/);
  // Back restores the kept-alive roster.
  await page.getByRole('button', { name: '← Back to Characters' }).click();
  await expect(page.getByRole('heading', { name: 'Characters', exact: true })).toBeVisible();
});
