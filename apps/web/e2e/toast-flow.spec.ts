import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * P4.25 — the toast subsystem, walked live (v4 `lib/toast.tsx`).
 *
 * ORDERING: rides the shared global-setup server, so the filename must sort
 * after `foundation.spec.ts` ('t' > 'f') and before the `zz…` destructives.
 *
 * The three claims v4's module makes and this walk proves against the real
 * server, on a real dialog:
 *
 *  1. a SUCCESS raises v4's sentence inside `[role="toast-container"]` — v4's
 *     own testability handle, carried verbatim into the Angular port;
 *  2. a FAILURE raises the server's sentence there instead (driven by a
 *     route-intercept so nothing in the fixture has to be broken to see it);
 *  3. the toast EXPIRES on its own after v4's 3000 ms, with no dismiss control
 *     — v4 has none, and none was invented.
 *
 * The Profile screen is the subject because its save is a single round trip
 * with no model call: the e2e instance carries no API keys by design, and this
 * beat spends nothing. It restores the name it edits, as `profile-flow` does.
 */

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

/** The live stack — the container is v4's `role="toast-container"` div. */
function toasts(page: Page) {
  return page.locator('[role="toast-container"] > div');
}

test.describe('P4.25 — the toast subsystem', () => {
  test('a save raises v4’s sentence in the toast container, and it expires on its own', async ({
    page,
  }) => {
    await page.goto('/profile');
    await maybeUnlock(page);

    const nameInput = page.locator('#profile-name');
    await expect(nameInput).toBeVisible({ timeout: 15_000 });
    const original = await nameInput.inputValue();

    // The container exists from boot (the one mechanism divergence from v4,
    // which appends it lazily) and is EMPTY until something is raised.
    await expect(page.locator('[role="toast-container"]')).toHaveCount(1);
    await expect(toasts(page)).toHaveCount(0);

    await nameInput.fill('Bertram Wilberforce Wooster');
    const save = page.getByRole('button', { name: 'Save Changes' });
    await expect(save).toBeEnabled();
    await save.click();

    // v4 `ProfileEditSection.tsx:54`, byte-for-byte, wearing the success type.
    const toast = toasts(page).filter({ hasText: 'Profile updated successfully' });
    await expect(toast).toBeVisible({ timeout: 15_000 });
    await expect(toast).toHaveClass(/qt-toast-success/);
    // v4 offers no dismiss control and this port invented none.
    await expect(toast.getByRole('button')).toHaveCount(0);

    // …and it goes by itself. v4's default is 3000 ms; allow the round trip.
    await expect(toast).toHaveCount(0, { timeout: 10_000 });

    // Restore, so the shared fixture is left as it was found.
    await nameInput.fill(original);
    await page.getByRole('button', { name: 'Save Changes' }).click();
    await expect(toasts(page).filter({ hasText: 'Profile updated successfully' })).toBeVisible({
      timeout: 15_000,
    });
  });

  test('a refused save raises the server’s sentence as an error toast', async ({ page }) => {
    await page.goto('/profile');
    await maybeUnlock(page);

    const nameInput = page.locator('#profile-name');
    await expect(nameInput).toBeVisible({ timeout: 15_000 });
    const original = await nameInput.inputValue();

    // Refuse THIS verb only, and let everything else through: breaking the
    // fixture to see a failure would leak into every later spec.
    await page.route('**/api/dispatch', async (route) => {
      const body = route.request().postDataJSON() as { type?: string } | null;
      if (body?.type !== 'userProfileUpdate') {
        await route.fallback();
        return;
      }
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          type: 'error',
          data: { kind: 'internal', message: 'the registry is closed for the afternoon' },
        }),
      });
    });

    await nameInput.fill('Augustus Fink-Nottle');
    await page.getByRole('button', { name: 'Save Changes' }).click();

    const toast = toasts(page).filter({ hasText: 'the registry is closed for the afternoon' });
    await expect(toast).toBeVisible({ timeout: 15_000 });
    await expect(toast).toHaveClass(/qt-toast-error/);
    await expect(toast).toHaveCount(0, { timeout: 10_000 });

    await page.unroute('**/api/dispatch');

    // Nothing was written: a reload shows the name the server still holds.
    await page.reload();
    await maybeUnlock(page);
    await expect(page.locator('#profile-name')).toHaveValue(original, { timeout: 15_000 });
  });
});
