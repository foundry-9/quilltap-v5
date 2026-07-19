import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * Dogfood finding #9 — the chat story background. The Salon fetches
 * `chatGetBackground` on chat open and binds the resolved file URL as
 * `--story-background-url` on the `.qt-chat-layout` root, where the ported
 * `::before` layer (`_chat.css`, opacity 0.45, fixed/cover) draws it.
 *
 * LIVE since the P4.6ak∥al∥am unification: global-setup seeds a background on
 * "Solo Voyage" (a `files` row + the PNG bytes at the storage-backend path +
 * `chats.storyBackgroundImageId`), so this beat rides lane A's real
 * `chatGetBackground` dispatch AND the real `/api/v1/files/{id}` byte route —
 * no route mocks. (In-lane it ran route-mocked, the P4.6aj precedent.)
 */

// Must match the global-setup seed.
const FILE_ID = 'bg-e2e-file';

test.describe('Salon story background (dogfood #9)', () => {
  async function maybeUnlock(page: Page) {
    const passphrase = page.locator('#qt-passphrase');
    const chats = page.getByRole('heading', { name: 'Chats', exact: true });
    await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
  }

  test('applies --story-background-url from the live resolver and draws the ::before layer', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);

    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
    const soloCard = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Solo Voyage' });
    await expect(soloCard).toBeVisible();
    await soloCard.click();

    const layout = page.locator('.qt-chat-layout');
    await expect(layout).toBeVisible();

    // The background var lands on the layout root (fetched async on open) and
    // points at the id-keyed byte route (not v4's path string).
    await expect(async () => {
      const style = (await layout.getAttribute('style')) ?? '';
      expect(style).toContain('--story-background-url');
      expect(style).toContain(`/api/v1/files/${FILE_ID}`);
    }).toPass({ timeout: 5_000 });

    // The live byte route actually serves the seeded PNG.
    const bytes = await page.request.get(`/api/v1/files/${FILE_ID}`);
    expect(bytes.status()).toBe(200);
    expect(bytes.headers()['content-type']).toContain('image/png');

    // With the var set, the ::before background layer is no longer hidden
    // (`:not([style*="--story-background-url"])::before { display:none }` no
    // longer matches) — the CSS actually draws the backdrop.
    const beforeDisplay = await layout.evaluate(
      (el) => getComputedStyle(el, '::before').display,
    );
    expect(beforeDisplay).not.toBe('none');
  });
});
