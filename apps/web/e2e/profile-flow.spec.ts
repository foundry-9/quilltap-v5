import { expect, test, type Page } from '@playwright/test';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so
 * its filename must sort AFTER foundation.spec.ts (workers: 1, alphabetical
 * file order) — "profile-flow" ('p') sorts after "foundation" ('f').
 *
 * P4.9c — a LIVE browser walk of the About + Profile vertical:
 *   1. The user menu in the shell footer opens and offers Profile + About.
 *   2. About renders v4's content AND the version string, which comes from the
 *      live §3 health field — proving the whole carry (health.rs → the HTTP
 *      body → interpretHealth → the screen), not a hard-coded number.
 *   3. Profile renders the three sections over the real `userProfileGet`, the
 *      data directory shows the path the server is actually serving from, a
 *      name edit round-trips through `userProfileUpdate`, and a RELOAD proves
 *      it persisted.
 *
 * The walk MUTATES the shared instance's user name, so it restores the original
 * at the end — global-setup copies the fixture fresh each run, but leaving a
 * shared fixture dirty mid-run is how later specs acquire mysterious failures.
 */

/** Unlock only when the passphrase screen is showing (the server stays unlocked). */
async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const shell = page.locator('.qt-left-sidebar');
  await expect(passphrase.or(shell).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
    await expect(shell.first()).toBeVisible({ timeout: 15_000 });
  }
}

async function openUserMenu(page: Page): Promise<void> {
  await page.getByRole('button', { name: 'User menu' }).click();
  await expect(page.locator('[role="menu"]').first()).toBeVisible();
}

test.describe('P4.9c — About + Profile', () => {
  test('the user menu opens About, which shows the live server version', async ({ page }) => {
    await page.goto('/');
    await maybeUnlock(page);

    await openUserMenu(page);
    await page.getByRole('menuitem', { name: 'About' }).click();

    await expect(page).toHaveURL(/\/about$/);
    await expect(page.getByRole('heading', { name: /About/ }).first()).toBeVisible();
    // v4 content landed.
    await expect(page.getByText('The Machinery Behind the Curtain')).toBeVisible();
    await expect(page.getByText('Charlie, Friday, and Amy')).toBeVisible();

    // The §3 carry, end to end: a real semver-shaped number, not the
    // "unknown" fallback and not a placeholder.
    const badge = page.locator('[data-testid="about-version"]');
    await expect(badge).toBeVisible();
    await expect(badge).toHaveText(/^Server version \d+\.\d+\.\d+/);

    // The ruled offline divergence: no remote badge images anywhere.
    await expect(page.locator('img[src*="shields.io"]')).toHaveCount(0);
  });

  test('the Profile screen round-trips a name edit and persists it', async ({ page }) => {
    await page.goto('/');
    await maybeUnlock(page);

    await openUserMenu(page);
    await page.getByRole('menuitem', { name: 'Profile' }).click();
    await expect(page).toHaveURL(/\/profile$/);

    // All three sections over the live verbs.
    await expect(page.getByRole('heading', { name: 'Profile Settings' })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Account Information' })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Data Directory' })).toBeVisible();

    // The data dir reports a real absolute path (the engine's actual base dir).
    const dirPath = page.locator('[data-testid="data-dir-path"]');
    await expect(dirPath).toBeVisible();
    await expect(dirPath).toHaveText(/^\//);

    const nameInput = page.locator('#profile-name');
    await expect(nameInput).toBeVisible();
    const original = await nameInput.inputValue();

    const edited = 'Reginald Jeeves';
    await nameInput.fill(edited);
    const save = page.getByRole('button', { name: 'Save Changes' });
    await expect(save).toBeEnabled();
    await save.click();
    await expect(page.locator('[data-testid="profile-status"]')).toHaveText(
      'Profile updated successfully',
    );

    // The persistence claim: a full reload re-reads from the database.
    await page.reload();
    await maybeUnlock(page);
    await expect(page.locator('#profile-name')).toHaveValue(edited, { timeout: 15_000 });

    // Restore, so the shared fixture is left as it was found.
    await page.locator('#profile-name').fill(original);
    await page.getByRole('button', { name: 'Save Changes' }).click();
    await expect(page.locator('[data-testid="profile-status"]')).toHaveText(
      'Profile updated successfully',
    );
  });
});
