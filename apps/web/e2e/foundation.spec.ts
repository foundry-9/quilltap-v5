import { expect, test } from './support/fixtures';

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

  // Finding #15: the theme must be stamped BEFORE unlock — with no
  // .light/.dark on <html> the auth screens render light-mode text on the
  // hard-coded dark backdrop (near-invisible) and qt-card never paints.
  await expect(page.locator('html.light, html.dark')).toHaveCount(1);

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

  // P4.d17 — Madman's Box 1.1.6/1.1.7, proven through the SERVED pack rather
  // than by reading the file: the pack's stylesheet actually reaches the page.
  await page.getByRole('button', { name: /Madman's Box/ }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'madmans-box');

  // 1.1.6: headings render as small caps, not uppercase.
  const heading = page.getByRole('heading', { name: 'Settings', exact: true });
  await expect(async () => {
    const caps = await heading.evaluate((el) => getComputedStyle(el).fontVariantCaps);
    expect(caps).toBe('small-caps');
  }).toPass({ timeout: 15_000 });

  // 1.1.7: the pack's unlayered [data-icon="thinking"] override beats core's
  // @layer default, so the indicator wears the theme's own brand mark. Probed on
  // a scratch element — no thinking icon is mounted on this screen.
  const mask = await page.evaluate(() => {
    const probe = document.createElement('span');
    probe.className = 'qt-icon';
    probe.setAttribute('data-icon', 'thinking');
    document.body.appendChild(probe);
    const value = getComputedStyle(probe).maskImage || getComputedStyle(probe).webkitMaskImage;
    probe.remove();
    return value;
  });
  expect(mask).toContain('/themes/madmans-box/icons/brand.svg');

  // Restore art-deco: the preference persists server-side (chat_settings), and
  // the rest of the suite runs against the state this spec leaves behind.
  await page.getByRole('button', { name: /Art Deco/ }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'art-deco');
});
