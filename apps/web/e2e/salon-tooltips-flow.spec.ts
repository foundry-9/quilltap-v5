import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: rides the SHARED global-setup server and unlocks it, so the filename
 * sorts AFTER foundation.spec.ts (workers: 1, alphabetical).
 *
 * P4.D132 — a LIVE walk of Quilltap's own tooltips (v4 `0bd841394`):
 *
 *  1. An action-bar button grows the `qt-tooltip` bubble after the 200 ms dwell
 *     — v4's copy, no `title` attribute anywhere in the row (the native tooltip
 *     would double up on ours) — and the bubble goes when the pointer leaves.
 *  2. The answer-confirmation badge (seeded as an AMENDED verdict onto the
 *     'Let me roll for that.' assistant row — `global-setup.ts`) opens its
 *     structured note, pins on click (`data-pinned`), and Escape dismisses it.
 */

/** Unlock only when the passphrase screen is showing (the shared server stays unlocked). */
async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const chats = page.getByRole('heading', { name: 'Chats', exact: true });
  await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
    await expect(chats).toBeVisible({ timeout: 15_000 });
  }
}

async function openSoloVoyage(page: Page): Promise<void> {
  await page.goto('/salon');
  await maybeUnlock(page);
  await page.getByRole('link', { name: 'Solo Voyage' }).first().click();
  await expect(page.getByText('Once, above the clouds...')).toBeVisible({ timeout: 15_000 });
}

test.describe('P4.D132 — Quilltap-drawn tooltips in the action bar', () => {
  test('a button grows the bubble after dwell, with v4 copy and no title attribute', async ({
    page,
  }) => {
    await openSoloVoyage(page);

    const row = page
      .locator('.qt-chat-message-row')
      .filter({ hasText: 'Let me roll for that.' })
      .first();
    await expect(row).toBeVisible({ timeout: 15_000 });

    // No native title anywhere in the icons row — the copy lives in qt-tooltip.
    await expect(row.locator('.qt-chat-message-action-bar-icons [title]')).toHaveCount(0);

    const copy = row.getByRole('button', { name: 'Copy message' });
    await copy.hover();
    // The 200 ms dwell passes under the auto-waiting expect; the bubble is
    // body-portalled, so it is looked up on the page, not in the row.
    const bubble = page.locator('.qt-tooltip');
    await expect(bubble).toBeVisible({ timeout: 5_000 });
    await expect(bubble).toHaveText('Copy message');
    await expect(bubble).toHaveAttribute('aria-hidden', 'true');

    // Leave — the 120 ms close grace runs out and the bubble goes.
    await page.mouse.move(10, 10);
    await expect(bubble).toHaveCount(0, { timeout: 5_000 });
  });

  test('the amended badge pins its structured note; Escape dismisses it', async ({ page }) => {
    await openSoloVoyage(page);

    const row = page
      .locator('.qt-chat-message-row')
      .filter({ hasText: 'Let me roll for that.' })
      .first();
    const badge = row.locator('.qt-confirmation-badge');
    await expect(badge).toBeVisible({ timeout: 15_000 });
    await expect(badge).toHaveText('✎Amended');
    await expect(badge).toHaveAttribute('data-confirmation-state', 'amended');
    await expect(badge).toHaveAttribute('data-has-detail', 'true');

    // Click pins the structured note open.
    await badge.click();
    const bubble = page.locator('.qt-tooltip');
    await expect(bubble).toBeVisible({ timeout: 5_000 });
    await expect(bubble).toHaveAttribute('data-pinned', 'true');
    await expect(bubble).toContainText('What looked off');
    await expect(bubble).toContainText('The ledger excerpt shows a metric column.');
    await expect(bubble).toContainText('Originally written');
    await expect(bubble).toContainText('Altitude is reported in metres.');
    await expect(bubble).toContainText('Click the badge to pin this note; Esc dismisses it.');

    // Pinned means it survives the pointer wandering off...
    await page.mouse.move(10, 10);
    await expect(bubble).toBeVisible();

    // ...until Escape.
    await page.keyboard.press('Escape');
    await expect(bubble).toHaveCount(0, { timeout: 5_000 });
  });
});
