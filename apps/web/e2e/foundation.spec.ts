import { expect, test } from '@playwright/test';

import { E2E_PASSPHRASE, E2E_WRONG_PASSPHRASE } from './support/env';

/**
 * The foundation walk against the real server (D13–D16): locked → unlock screen
 * → wrong passphrase error → correct passphrase → the shell with the fixture's
 * chats, plus the Appearance tab applying a bundled theme.
 *
 * P4.6e note: the nav quick-theme switcher is now opt-in (v4's default
 * `showNavThemeSelector: false`), so this walks Settings → Appearance to apply a
 * pack — the always-present theme UI.
 */
test('walks locked → unlock → shell, then applies a bundled theme', async ({ page }) => {
  await page.goto('/salon');

  // The locked vault routes to the unlock screen (v4-voiced).
  await expect(
    page.getByRole('heading', { name: 'Quilltap Awaits Your Credentials' }),
  ).toBeVisible();

  // A wrong passphrase surfaces the server's error copy without unlocking.
  await page.locator('#qt-passphrase').fill(E2E_WRONG_PASSPHRASE);
  await page.getByRole('button', { name: 'Unlock' }).click();
  await expect(page.getByText('Invalid passphrase')).toBeVisible();

  // The correct passphrase unlocks and routes to the shell.
  await page.locator('#qt-passphrase').fill(E2E_PASSPHRASE);
  await page.getByRole('button', { name: 'Unlock' }).click();

  // The shell shows the chats list (the first real server data).
  await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
  await expect(page.getByText('Loading chats...')).toBeHidden();
  // Either the fixture's chats render as cards, or the empty state — both prove
  // the `listChats` round trip completed.
  const rows = page.locator('.chat-card-stack a.qt-entity-card');
  const empty = page.getByText('No chats yet');
  await expect(async () => {
    expect((await rows.count()) > 0 || (await empty.count()) > 0).toBe(true);
  }).toPass();

  // The Appearance tab applies a bundled pack (data-theme flips on <html>).
  await page.getByRole('link', { name: 'Settings' }).click();
  await expect(page.getByRole('heading', { name: 'Settings', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Appearance' }).click();
  await page.getByRole('button', { name: /Art Deco/ }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'art-deco');
});
