import { expect, test, type Page } from '@playwright/test';

import { E2E_PASSPHRASE } from './support/env';

/**
 * Dogfood finding #9 — the chat story background. The Salon fetches
 * `chatGetBackground` on chat open and binds the resolved file URL as
 * `--story-background-url` on the `.qt-chat-layout` root, where the ported
 * `::before` layer (`_chat.css`, opacity 0.45, fixed/cover) draws it.
 *
 * The live `chatGetBackground` dispatch is lane A's (wired at unification), so
 * in-lane this beat ROUTE-MOCKS the resolver and the file byte route — a green
 * in-lane mock beats a never-firing guard (the P4.6aj precedent). It proves the
 * full SPA path end-to-end in a real browser: fetch → apply the var → the CSS
 * layer stops being `display:none`. At unification the mock is dropped and the
 * beat rides the live leg against a seeded background (see the status log).
 */

// A 1x1 transparent PNG — the background image bytes the ::before layer loads.
const PNG_1x1 = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
  'base64',
);
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

  test('applies --story-background-url from the resolver and draws the ::before layer', async ({
    page,
  }) => {
    // Mock the background resolver — pass every other dispatch through to the
    // real server unchanged.
    await page.route('**/api/dispatch', async (route) => {
      const post = route.request().postData() ?? '';
      if (post.includes('"chatGetBackground"')) {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            type: 'ack',
            data: {
              backgroundUrl: null,
              fileId: FILE_ID,
              filename: 'bg.webp',
              sha256: null,
              linkSummary: null,
            },
          }),
        });
        return;
      }
      await route.continue();
    });
    // Serve the background image bytes for the id-keyed byte route.
    await page.route(`**/api/v1/files/${FILE_ID}`, async (route) => {
      await route.fulfill({ status: 200, contentType: 'image/png', body: PNG_1x1 });
    });

    await page.goto('/');
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

    // With the var set, the ::before background layer is no longer hidden
    // (`:not([style*="--story-background-url"])::before { display:none }` no
    // longer matches) — the CSS actually draws the backdrop.
    const beforeDisplay = await layout.evaluate(
      (el) => getComputedStyle(el, '::before').display,
    );
    expect(beforeDisplay).not.toBe('none');
  });
});
