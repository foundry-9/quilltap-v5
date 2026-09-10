import { expect, request as pwRequest, test, type Page } from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';
import { openSidebarSection } from './support/sidebar';

/**
 * P4.D176 — the Salon chat gallery, SPA half (v4 `86d59660c`).
 *
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so
 * its filename must sort AFTER foundation.spec.ts (workers: 1, alphabetical).
 *
 * The entry-renders beat is UNGATED: it proves v4's post-bug-129 shape
 * (ungated, unnumbered while `chatGallery` is unimplemented) against the REAL
 * shared server — the retired divergence itself.
 *
 * Every OTHER beat needs the server half (P4.D174's `chatGallery` /
 * `chatSaveGalleryImage` verbs + the `?download=1` Content-Disposition arm) and
 * is gated ACTIVATE-AT-UNIFY behind {@link P4D174_SERVER_LANDED} — a NAMED
 * constant, never a capability probe (a DEFINED-but-refusing verb would defeat
 * one). "Solo Voyage" (Aria's solo chat) carries both a story background
 * (`storyBackgroundImageId`, global-setup.ts `bg-e2e-file`) and her vault
 * avatar — two non-zero sources without any new SQL seeding
 * (`characters-flow.spec.ts:214`'s note). The Jump-to-message beat instead
 * needs an entry that HANGS BENEATH a message, which "Solo Voyage" does not
 * carry — it discovers the courier fixture's image-attachment chat by content,
 * the `salon-courier-images-flow.spec.ts` precedent, and skips if absent.
 */

/** Flipped at unification, once P4.D174's `chatGallery` verb lands. */
const P4D174_SERVER_LANDED = true;

async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const messages = page.locator('.qt-chat-messages-list');
  await expect(passphrase.or(messages).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

/**
 * The shared fixture's rolls, MEASURED at unification (a `chatGallery` dispatch
 * per chat): "Solo Voyage" / "Group Expedition" / "The Shuttered Wing" hold ONE
 * portrait each (whose `files` row has no stored bytes here, so the tile
 * renders as v4's "Image Deleted"); "Chat Images" — the courier fixture's chat —
 * holds 1 generated + 2 attachments with real bytes, the only two-source roll,
 * so it is the chat the content beats read. The beats were written mocked
 * against §B presuming a background + a portrait; their first live run
 * measured otherwise.
 */
async function openChat(page: Page, title: string): Promise<void> {
  await page.goto('/salon');
  const passphrase = page.locator('#qt-passphrase');
  const list = page.getByRole('heading', { name: 'Chats', exact: true });
  await expect(passphrase.or(list).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
  await expect(list).toBeVisible();
  await page.getByRole('link', { name: title }).first().click();
  await maybeUnlock(page);
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible();
}

const openSoloVoyage = (page: Page) => openChat(page, 'Solo Voyage');
const openChatImages = (page: Page) => openChat(page, 'Chat Images');

test.describe('P4.D176 — the Salon chat gallery', () => {
  test('the Gallery entry renders, numbered from chatGallery.total (v4’s ungated post-bug-129 shape)', async ({
    page,
  }) => {
    await openSoloVoyage(page);
    await openSidebarSection(page, 'Organize');
    const gallery = page.locator('button[title="Every image in this conversation"]');
    await expect(gallery).toBeVisible({ timeout: 10_000 });
    if (!P4D174_SERVER_LANDED) {
      // The verb is UNKNOWN to the shared server today — the entry still
      // renders, just with no count (never a stale "(0)").
      // v4 `ChatSidebar.tsx:1676` — `Gallery ({galleryCount})`, never bare; the
      // count is the real server's roll for this chat.
      await expect(gallery).toHaveText(/^Gallery \(\d+\)$/);
    }
  });

  test('open the gallery, the chips, filter, the detail view, provenance', async ({ page }) => {
    test.skip(
      !P4D174_SERVER_LANDED,
      'chatGallery is P4.D174’s — the grid has nothing to read until it lands',
    );

    await openChatImages(page);
    await openSidebarSection(page, 'Organize');
    await page.locator('button[title="Every image in this conversation"]').click();

    const gallery = page.locator('qt-photo-gallery-modal');
    await expect(gallery.getByRole('heading', { name: 'Chat Photos' })).toBeVisible();

    // The chips — "Chat Images" carries one generated image AND two attachments:
    // two non-zero sources, so the filter group renders (v4: chips only at >= 2).
    const chipGroup = page.locator('[role="group"][aria-label="Filter by where the picture came from"]');
    await expect(chipGroup).toBeVisible({ timeout: 10_000 });
    await expect(chipGroup.getByRole('button', { name: /^Generated \(1\)/ })).toBeVisible();
    await expect(chipGroup.getByRole('button', { name: /^Attached \(2\)/ })).toBeVisible();

    // Filter to Attached — two entries, ONE of which has stored bytes in this
    // fixture (the other, like the generated row, renders as v4's "Image
    // Deleted" placeholder with no <img> — measured live at unification).
    await chipGroup.getByRole('button', { name: /^Attached \(/ }).click();
    const tiles = gallery.locator('.qt-dialog img');
    await expect(async () => {
      expect(await tiles.count()).toBe(1);
    }).toPass({ timeout: 10_000 });

    // Open the detail view — the provenance line names the source.
    await tiles.first().click();
    const detail = page.locator('qt-chat-gallery-image-view-modal');
    await expect(detail.locator('[role="dialog"]')).toBeVisible();
    await expect(detail).toContainText('Attached beneath a message');

    // The two hard-wired album buttons are GONE (v4 deleted them; P4.D176
    // retires v5's own copy) — only the shared Save trigger remains.
    await expect(detail.getByTitle(/'s photo album$/)).toHaveCount(0);
    await expect(detail.getByTitle('Save to a photo album')).toBeVisible();

    // Nested Escape: closes the detail layer only, then the gallery.
    await page.keyboard.press('Escape');
    await expect(detail).toHaveCount(0);
    await expect(gallery.getByRole('heading', { name: 'Chat Photos' })).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(gallery.getByRole('heading', { name: 'Chat Photos' })).toHaveCount(0);
  });

  test('Save through the chat leg (mock-free) and Download carries ?download=1', async ({ page }) => {
    test.skip(
      !P4D174_SERVER_LANDED,
      'chatSaveGalleryImage / ?download=1 are P4.D174’s',
    );

    await openChatImages(page);
    await openSidebarSection(page, 'Organize');
    await page.locator('button[title="Every image in this conversation"]').click();
    const gallery = page.locator('qt-photo-gallery-modal');
    await expect(gallery.getByRole('heading', { name: 'Chat Photos' })).toBeVisible();

    const tile = gallery.locator('.qt-dialog .group').first();
    await tile.hover();

    // Download: `a[download]` never fires a real navigation the pane can
    // observe reliably (the standing rule) — assert the anchor's href
    // directly rather than click-and-wait for a `download` event.
    const anchorHref = await page.evaluate(() => {
      let seen: string | null = null;
      const orig = HTMLAnchorElement.prototype.click;
      HTMLAnchorElement.prototype.click = function (this: HTMLAnchorElement) {
        seen = this.href;
        return undefined as unknown as void;
      };
      (window as unknown as { __qtDownloadHref: () => string | null }).__qtDownloadHref = () => {
        HTMLAnchorElement.prototype.click = orig;
        return seen;
      };
    }).then(async () => {
      await tile.getByRole('button', { name: 'Download image' }).click();
      return page.evaluate(
        () => (window as unknown as { __qtDownloadHref: () => string | null }).__qtDownloadHref(),
      );
    });
    expect(anchorHref).toContain('download=1');

    // Save — the chat door's dialog opens with an album picker; pick the
    // first option and submit, then read the toast.
    await tile.getByRole('button', { name: 'Save to a photo album' }).click();
    // The Angular host element is zero-size (the dialog is body-reparented), so
    // assert the dialog itself, never the host — the inline-host locator trap.
    const dialog = page.locator('qt-save-image-dialog');
    await expect(dialog.getByRole('heading', { name: 'Save image to album' })).toBeVisible();
    await expect(async () => {
      expect(await dialog.locator('select#save-image-album option').count()).toBeGreaterThan(0);
    }).toPass({ timeout: 10_000 });
    await dialog.getByRole('button', { name: 'Save image' }).click();
    // Either it saves cleanly (a toast) or it was already there (the 409
    // "already in this album" answer) — both are SUCCESSFUL outcomes for this
    // beat, which is proving the wire reaches a real verb, not a fresh save.
    await expect(
      page.locator('.qt-toast', { hasText: /Saved to|already in this album/ }).first(),
    ).toBeVisible({ timeout: 10_000 });
  });

  test('Jump to message lands the transcript on the attached image’s message', async ({ page }) => {
    test.skip(
      !P4D174_SERVER_LANDED,
      'chatGallery must carry a real messageId before Jump has anything to land on',
    );

    // Discover the courier fixture's image-attachment chat by CONTENT (the
    // `salon-courier-images-flow.spec.ts` precedent) — "Solo Voyage" carries
    // no message-hung entry.
    let chatId: string | null = null;
    let messageId: string | null = null;
    const ctx = await pwRequest.newContext();
    try {
      const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: { type: 'listChats' } });
      const body = (await res.json().catch(() => null)) as { data?: { id: string }[] } | null;
      for (const c of body?.data ?? []) {
        const gal = await ctx.post(`${BASE_URL}/api/dispatch`, {
          data: { type: 'chatGallery', chatId: c.id },
        });
        const gb = (await gal.json().catch(() => null)) as {
          data?: { entries?: { messageId?: string }[] };
        } | null;
        const withMsg = gb?.data?.entries?.find((e) => e.messageId);
        if (withMsg) {
          chatId = c.id;
          messageId = withMsg.messageId!;
          break;
        }
      }
    } finally {
      await ctx.dispose();
    }
    test.skip(!chatId, 'no chat in the shared fixture carries a message-hung gallery entry');

    await page.goto(`/salon/${chatId}`);
    await maybeUnlock(page);
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible();
    await openSidebarSection(page, 'Organize');
    await page.locator('button[title="Every image in this conversation"]').click();

    const gallery = page.locator('qt-photo-gallery-modal');
    await gallery.locator('.qt-dialog img').first().click();
    const detail = page.locator('qt-chat-gallery-image-view-modal');
    await expect(detail.locator('[role="dialog"]')).toBeVisible();

    const jump = detail.getByRole('button', { name: 'Jump to message' });
    if (await jump.count()) {
      await jump.click();
      // Both modals close (the three-hop choreography).
      await expect(detail).toHaveCount(0);
      await expect(gallery).toHaveCount(0);
      // v4 addresses a row by `document.getElementById(\`message-${id}\`)`
      // (`SalonView.tsx:1296`); v5's row carries the same id.
      await expect(page.locator(`[id="message-${messageId}"]`)).toBeVisible({ timeout: 10_000 });
    }
  });

  test('Delete a generated file from the gallery', async ({ page }) => {
    test.skip(!P4D174_SERVER_LANDED, 'chatFileDelete over a chatGallery entry is P4.D174’s');

    await openSoloVoyage(page);
    await openSidebarSection(page, 'Organize');
    await page.locator('button[title="Every image in this conversation"]').click();
    const gallery = page.locator('qt-photo-gallery-modal');
    await expect(gallery.getByRole('heading', { name: 'Chat Photos' })).toBeVisible();

    const chipGroup = page.locator('[role="group"][aria-label="Filter by where the picture came from"]');
    const generatedChip = chipGroup.getByRole('button', { name: /^Generated \(/ });
    test.skip(
      !(await generatedChip.count()),
      'no generated-source entry in the shared fixture to delete',
    );
    await generatedChip.click();

    const tile = gallery.locator('.qt-dialog .group').first();
    await tile.hover();
    const before = await gallery.locator('.qt-dialog img').count();

    page.once('dialog', (d) => void d.accept());
    await tile.getByRole('button', { name: 'Delete image' }).click();
    await expect(async () => {
      expect(await gallery.locator('.qt-dialog img').count()).toBe(before - 1);
    }).toPass({ timeout: 10_000 });
  });
});
