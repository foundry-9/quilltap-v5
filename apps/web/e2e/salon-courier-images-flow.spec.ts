import { expect, request as pwRequest, test, type Page } from '@playwright/test';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so
 * its filename must sort AFTER foundation.spec.ts — foundation walks the
 * locked -> unlock gate and must reach the shared server first (workers: 1,
 * alphabetical file order). The "salon-courier-images" name sorts after it.
 *
 * P4.6ac — a LIVE browser walk of the courier bubble and the in-chat image
 * lightbox over the shared Salon server:
 *   1. Courier beat: open the chat with a PENDING external turn → the Courier
 *      bubble renders (copy button + paste textarea + "awaiting carrier") →
 *      Cancel turn settles it and the bubble disappears.
 *   2. Image beat: open a chat carrying an image attachment → click the
 *      thumbnail → the lightbox opens over the image → Escape closes it.
 *
 * FIXTURE- + PROBE-GUARDED. The courier/image mutation dispatch
 * (`messageResolveExternalTurn` &c.) lands in lane A (P4.6ab), and the
 * pending-courier / image-attachment chats live in lane A's
 * `courier-images-*.db` fixture. In-lane the shared server neither implements
 * the dispatch nor carries the fixture data, so both beats skip; they
 * auto-activate at unification once lane A's arms + fixture merge:
 *   - a `beforeAll` probe skips when the dispatch is an unknown variant;
 *   - each beat discovers its fixture chat by CONTENT (scanning the chat list),
 *     so it needs no hardcoded fixture title, and skips when absent.
 * The walk only READS + cancels (no LLM turn), leaving the shared fixture
 * pristine — the cancelled courier row is fixture-owned, not seeded here.
 */

let courierBackendReady = false;

test.beforeAll(async () => {
  // Probe: is the courier dispatch handled? In-lane the Rust core has no courier
  // arms, so the request fails with "unknown variant" → not ready. A success
  // envelope or a domain error (not-found) means the handler exists → ready.
  try {
    const ctx = await pwRequest.newContext();
    const res = await ctx.post(`${BASE_URL}/api/dispatch`, {
      data: {
        type: 'messageCancelExternalTurn',
        chatId: '00000000-0000-0000-0000-000000000000',
        messageId: '00000000-0000-0000-0000-000000000000',
      },
    });
    const body = (await res.json().catch(() => null)) as
      | { type?: string; data?: { message?: string } }
      | null;
    await ctx.dispose();
    const isUnknownVariant =
      body?.type === 'error' && /unknown variant/i.test(String(body?.data?.message ?? ''));
    courierBackendReady = body != null && !isUnknownVariant;
  } catch {
    courierBackendReady = false;
  }
});

/** Unlock only when the passphrase screen is showing (the shared server stays unlocked). */
async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const chats = page.getByRole('heading', { name: 'Chats', exact: true });
  await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

async function openChatList(page: Page): Promise<void> {
  await page.goto('/');
  await maybeUnlock(page);
  await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
}

/**
 * Open chats in turn until one renders `selector`, then return true. Discovery
 * by content keeps the walk independent of lane A's fixture chat titles. Bounded
 * to the visible cards; returns false when none match (the beat then skips).
 */
async function openChatWith(page: Page, selector: string): Promise<boolean> {
  const cards = page.locator('.chat-card-stack a.qt-entity-card');
  const count = await cards.count();
  for (let i = 0; i < count; i++) {
    await openChatList(page);
    const card = page.locator('.chat-card-stack a.qt-entity-card').nth(i);
    await card.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible();
    if (await page.locator(selector).first().isVisible().catch(() => false)) {
      return true;
    }
  }
  return false;
}

test.describe('P4.6ac — courier + in-chat images', () => {
  test('the Courier bubble renders and Cancel settles the pending turn', async ({ page }) => {
    test.skip(!courierBackendReady, 'courier dispatch not implemented in-lane (activates at unification)');
    await openChatList(page);
    const found = await openChatWith(page, '.qt-courier-bubble');
    test.skip(!found, 'no pending-courier chat in the shared fixture (lane A delivers it)');

    const bubble = page.locator('.qt-courier-bubble');
    await expect(bubble).toContainText('awaiting carrier');
    await expect(bubble.getByRole('button', { name: /Copy prompt/ })).toBeVisible();
    await expect(bubble.locator('textarea')).toBeVisible();

    await bubble.getByRole('button', { name: 'Cancel turn' }).click();
    await expect(page.locator('.qt-courier-bubble')).toHaveCount(0);
  });

  test('an image thumbnail opens the lightbox', async ({ page }) => {
    test.skip(!courierBackendReady, 'courier dispatch not implemented in-lane (activates at unification)');
    await openChatList(page);
    const found = await openChatWith(page, '.qt-chat-attachment-image');
    test.skip(!found, 'no image-attachment chat in the shared fixture (lane A delivers it)');

    await page.locator('.qt-chat-attachment-button').first().click();
    const lightbox = page.locator('[role="dialog"][aria-modal="true"]');
    await expect(lightbox).toBeVisible();
    await expect(lightbox.locator('img')).toBeVisible();

    await page.keyboard.press('Escape');
    await expect(lightbox).toHaveCount(0);
  });
});
