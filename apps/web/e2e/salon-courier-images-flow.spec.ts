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
 *
 * The per-card check WAITS for the selector instead of sampling `isVisible()` the
 * instant the message list appears. The sample was a race — the chat's own
 * content (thumbnails, virtualized rows) lands a tick or two after the list —
 * and losing it made the caller skip a beat that should have RUN, which reads
 * exactly like "nothing to test" (P4.6as found the lightbox beat skipping for
 * this reason once a sibling query changed chat-open timing). A guard that
 * decides coverage must not be timing-dependent. The 2s ceiling is paid only on
 * chats that genuinely lack the selector.
 */
async function openChatWith(page: Page, selector: string): Promise<boolean> {
  const cards = page.locator('.chat-card-stack a.qt-entity-card');
  const count = await cards.count();
  for (let i = 0; i < count; i++) {
    await openChatList(page);
    const card = page.locator('.chat-card-stack a.qt-entity-card').nth(i);
    await card.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible();
    const appeared = await page
      .locator(selector)
      .first()
      .waitFor({ state: 'visible', timeout: 2_000 })
      .then(() => true)
      .catch(() => false);
    if (appeared) {
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

  test('composer file attach uploads over the live chat-files leg', async ({ page }) => {
    // P4.6ah ∥ P4.6aj unification wire: lane C verified the LOCKED client
    // (`chat-files.api.ts`) at the unit level and lane A landed the server
    // multipart leg (`POST /api/v1/chats/{id}/files`); neither lane could
    // prove the seam end-to-end. This beat does: pick a file → the live
    // multipart upload lands → the attachment chip renders → remove leaves
    // the composer clean (send is NOT exercised — no LLM turn).
    //
    // A GENERAL (non-project) chat, discovered via the API: the fixture's
    // project chats belong to seeded projects with no linked document store,
    // so their upload branch fails v4-faithfully — that arm is the
    // differential's job, not this walk's.
    const ctx = await pwRequest.newContext();
    let generalChatId: string | null = null;
    try {
      const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: { type: 'listChats' } });
      const body = (await res.json().catch(() => null)) as {
        data?: { id: string; project?: unknown }[];
      } | null;
      generalChatId = body?.data?.find((c) => !c.project)?.id ?? null;
    } finally {
      await ctx.dispose();
    }
    test.skip(!generalChatId, 'no general (non-project) chat in the shared fixture');

    await openChatList(page);
    await page.goto(`/salon/${generalChatId}`);
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible();

    const attachButton = page.getByRole('button', { name: 'Attach a file', exact: true });
    test.skip(!(await attachButton.isVisible().catch(() => false)), 'composer attach not available in this chat');

    await page
      .locator('input[type=file][aria-label="Attach a file"]')
      .setInputFiles({
        name: 'unify-attach-note.txt',
        mimeType: 'text/plain',
        buffer: Buffer.from('Attached over the live chat-file upload leg.'),
      });

    const chip = page.locator('.qt-chat-attachment-chip');
    await expect(chip).toContainText('unify-attach-note.txt', { timeout: 15_000 });

    // Detach; the composer returns to a clean state.
    await chip.getByRole('button', { name: 'Remove attachment' }).click();
    await expect(page.locator('.qt-chat-attachment-chip')).toHaveCount(0);
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
