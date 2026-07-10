import { expect, test } from '@playwright/test';

import { E2E_PASSPHRASE } from './support/env';

/**
 * P4.6g — browser walk of the Characters vertical: unlock → the roster renders
 * lane A's committed `characters-*` fixture cards → open a detail view → toggle
 * a favorite → edit a field and save → a re-read shows the change.
 *
 * SKIPPED until the P4.6f/g unification. Like the Salon e2e (`m4-salon.spec.ts`),
 * it needs the live axum server booted against a committed fixture — but the
 * shared `global-setup.ts` copies the SALON fixture, so this walk only lights up
 * once the unification wires the characters fixture into the server launch (lane
 * A owns `crates/quilltap-web/tests/fixtures/characters-*.db` +
 * `build-characters-fixture.ts`). Un-skip and pin the exact fixture card names
 * then. The steps below are the intended shape, kept close to the salon precedent.
 */
test.describe.skip('P4.6g — Characters vertical (list → view → toggle → edit → save)', () => {
  /** Unlock only when the passphrase screen is showing (the shared server may
   *  already be unlocked by an earlier spec). Mirrors `m4-salon.spec.ts`. */
  async function maybeUnlock(page: import('@playwright/test').Page) {
    const passphrase = page.locator('#qt-passphrase');
    const characters = page.getByRole('heading', { name: 'Characters', exact: true });
    await expect(passphrase.or(characters).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
  }

  test('unlock → roster → open detail → toggle favorite → edit a field → save persists', async ({
    page,
  }) => {
    await page.goto('/characters');
    await maybeUnlock(page);

    // The roster renders the fixture cards.
    await expect(page.getByRole('heading', { name: 'Characters', exact: true })).toBeVisible();
    const firstCard = page.locator('.character-card-grid .character-card').first();
    await expect(firstCard).toBeVisible();

    // Toggle a favorite on the card (optimistic star flip).
    const star = firstCard.getByRole('button', { name: /favorites/i });
    await star.click();

    // Open the detail view.
    await firstCard.getByRole('link').first().click();
    await expect(page.getByRole('link', { name: /Edit Character/i })).toBeVisible();

    // Edit a field and save, then confirm it survives a re-read.
    // (Exact locators pinned against the fixture at un-skip time.)
    // await page.getByRole('link', { name: /Edit Character/i }).click();
    // await page.getByLabel('Title').fill('The Indispensable Valet');
    // await page.getByRole('button', { name: 'Save Character' }).click();
    // await expect(page.getByText('The Indispensable Valet')).toBeVisible();
  });
});
