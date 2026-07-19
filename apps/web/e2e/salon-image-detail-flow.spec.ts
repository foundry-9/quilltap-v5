import { expect, request as pwRequest, test, type Page } from '@playwright/test';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so
 * its filename must sort AFTER foundation.spec.ts (workers: 1, alphabetical).
 *
 * P4.9a2 — a LIVE walk of the deep image-detail modal family layered on the
 * in-chat photo gallery (the order's tier-2 beat, verbatim):
 *   boot → unlock → open a chat's photo gallery → open a thumbnail into the
 *   detail modal → arrow to the next image → Escape closes the DETAIL modal
 *   only (the gallery stays open) → a second Escape closes the gallery.
 *
 * The beat SEEDS its own certainty: it uploads two distinct tiny PNGs over
 * the live chat-files multipart leg (P4.6ah) into a general chat, so the
 * gallery is guaranteed ≥2 items and the prev/next arrows have something to
 * do. Uploaded chat files carry no message row, so the chat's rendered
 * transcript — which later content-discovering specs scan — is unchanged.
 */

/** A 1×1 transparent PNG; a distinct trailing tail keeps the sha unique. */
function tinyPng(tag: string): Buffer {
  const png = Buffer.from(
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==',
    'base64',
  );
  return Buffer.concat([png, Buffer.from(`\n${tag}\n`, 'utf-8')]);
}

async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const messages = page.locator('.qt-chat-messages-list');
  await expect(passphrase.or(messages).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

test.describe('P4.9a2 — the deep image-detail modal family', () => {
  test('gallery → detail modal → arrow → nested Escape closes one layer at a time', async ({
    page,
  }) => {
    test.setTimeout(90_000);

    // 1. A general (non-project) chat, via the API.
    const ctx = await pwRequest.newContext();
    let chatId: string | null = null;
    try {
      const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: { type: 'listChats' } });
      const body = (await res.json().catch(() => null)) as {
        data?: { id: string; project?: unknown }[];
      } | null;
      // A locked server answers an error envelope (data is an object) — guard
      // so an out-of-order run skips instead of throwing; foundation.spec.ts
      // unlocks the shared server before this file in the full suite.
      chatId = Array.isArray(body?.data)
        ? (body.data.find((c) => !c.project)?.id ?? null)
        : null;

      test.skip(!chatId, 'no general (non-project) chat in the shared fixture');

      // 2. Seed two distinct images over the live multipart chat-files leg.
      for (const tag of ['image-detail-beat-one', 'image-detail-beat-two']) {
        const upload = await ctx.post(`${BASE_URL}/api/v1/chats/${chatId}/files`, {
          multipart: {
            file: {
              name: `${tag}.png`,
              mimeType: 'image/png',
              buffer: tinyPng(tag),
            },
          },
        });
        expect(upload.ok(), `chat-file upload ${tag} → ${upload.status()}`).toBe(true);
      }
    } finally {
      await ctx.dispose();
    }

    // 3. Open the chat and the photo gallery.
    await page.goto(`/salon/${chatId}`);
    await maybeUnlock(page);
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible();
    await page.getByRole('button', { name: 'View chat photos' }).click();

    const gallery = page.locator('qt-photo-gallery-modal');
    await expect(gallery.getByRole('heading', { name: 'Chat Photos' })).toBeVisible();
    // The two seeded PNGs guarantee ≥2 LOADABLE tiles; the fixture chat may
    // carry more image files of its own (some possibly rendering as
    // missing-image placeholders), so the walk asserts NAVIGATION, not the
    // list length — the end-of-list arrow disappearance is unit-pinned in
    // photo-gallery-modal.spec.ts.
    const tiles = gallery.locator('.qt-dialog img');
    await expect(async () => {
      expect(await tiles.count()).toBeGreaterThanOrEqual(2);
    }).toPass({ timeout: 10_000 });

    // 4. A thumbnail opens the z-[60] chat detail modal over the z-50 grid.
    //    Assert on the inner overlay — the component HOST element has no
    //    layout box, so Playwright reads it as hidden (the P4.9b lesson).
    await tiles.first().click();
    const detail = page.locator('qt-chat-gallery-image-view-modal');
    await expect(detail.locator('[role="dialog"]')).toBeVisible();
    await expect(detail.locator('img')).toBeVisible();
    // First item of the list: next exists, prev does not (the
    // conditionally-undefined arrow idiom, PhotoGalleryModal.tsx:343-344).
    const nextArrow = detail.getByRole('button', { name: 'Next image (Right Arrow)' });
    await expect(nextArrow).toBeVisible();
    await expect(
      detail.getByRole('button', { name: 'Previous image (Left Arrow)' }),
    ).toHaveCount(0);

    // 5. Arrow to the next image — the prev arrow appears (the host's index
    //    arithmetic moved off the start of the list).
    await nextArrow.click();
    await expect(
      detail.getByRole('button', { name: 'Previous image (Left Arrow)' }),
    ).toBeVisible();

    // 6. The nested-Escape invariant (PhotoGalleryModal.tsx:170-174): the
    //    first press closes the DETAIL modal only — the gallery stays open.
    await page.keyboard.press('Escape');
    await expect(detail).toHaveCount(0);
    await expect(gallery.getByRole('heading', { name: 'Chat Photos' })).toBeVisible();

    // 7. The second press closes the gallery.
    await page.keyboard.press('Escape');
    await expect(gallery.getByRole('heading', { name: 'Chat Photos' })).toHaveCount(0);
  });
});
