import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * P4.26 — the announcement-rendering parity audit's live proof.
 *
 * Two beats over the two worst divergences the audit found, both asserted on the
 * rendered DOM of a real posted announcement rather than on a component fixture:
 *
 *  1. **The body.** v5 dressed every expanded announcement in
 *     `.qt-chat-message-system` — `text-sm italic text-center py-2` on a muted
 *     slab. v4 defines that class and never wears it; an expanded announcement
 *     goes through v4's ordinary message block, which for a Staff row (written
 *     `role: 'ASSISTANT'`) is `qt-chat-message-assistant`.
 *  2. **The bar.** The chip's contents in v4's order — dot, sender, kind, time,
 *     chevron — with the kind span present because the row HAS a kind, and a
 *     second chip staying collapsed while the first opens (the packing rule).
 *
 * ORDERING: rides the shared global-setup server, so the filename must sort
 * after `foundation.spec.ts` ('sa' > 'fo'), which walks the locked→unlock gate.
 *
 * The chat is "Group Expedition", the same three-participant room the Post
 * Office beats use — Insert Announcement is how a Staff row gets written
 * without an LLM call (a staff announcement with the "Use as-is" profile costs
 * nothing).
 */
test.describe('P4.26 — announcement rendering', () => {
  async function maybeUnlock(page: Page) {
    const passphrase = page.locator('#qt-passphrase');
    const chats = page.getByRole('heading', { name: 'Chats', exact: true });
    await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
  }

  async function openChat(page: Page, title: string) {
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
    const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: title });
    await expect(card).toBeVisible();
    await card.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible();
  }

  function footerButton(page: Page, name: string) {
    return page.locator('[qt-modal-footer]').getByRole('button', { name, exact: true });
  }

  /** Post a Staff announcement and return the line that was written. */
  async function postStaffAnnouncement(page: Page, staff: string): Promise<string> {
    await page.getByRole('button', { name: 'Insert announcement' }).click();
    await expect(page.locator('#announce-staff')).toBeVisible({ timeout: 15_000 });
    await page.locator('#announce-staff').selectOption(staff);

    const line = `A notice from ${staff} ${Date.now()}.`;
    await page.locator('.qt-markdown-field .qt-rich-editor-content').click();
    await page.keyboard.type(line);

    await footerButton(page, 'Post Announcement').click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    return line;
  }

  test('an opened announcement reads as an ordinary message, not a centred slab', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    const line = await postStaffAnnouncement(page, 'librarian');

    const chip = page
      .locator('.qt-chat-announcement-chip')
      .filter({ hasText: 'The Librarian' })
      .last();
    await expect(chip).toBeVisible({ timeout: 15_000 });
    await chip.click();

    // The body arrives only once the chip is open (v5 keeps the chip mounted and
    // renders the body beneath it).
    const bubble = page.locator('.qt-chat-message', { hasText: line }).last();
    await expect(bubble).toBeVisible({ timeout: 15_000 });

    // THE FIX: an ordinary assistant bubble…
    await expect(bubble).toHaveClass(/qt-chat-message-assistant/);
    // …and NOT the small centred italics on a muted slab. `.qt-chat-message-system`
    // is defined in both stylesheets and worn by no v4 markup anywhere.
    await expect(bubble).not.toHaveClass(/qt-chat-message-system/);

    // The markdown pipeline ran on it, exactly as it does for a character's line.
    await expect(bubble.locator('.qt-chat-message-content')).toHaveCount(1);
  });

  test('a chip carries v4’s bar contents, and its neighbour stays shut', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    await postStaffAnnouncement(page, 'host');
    await postStaffAnnouncement(page, 'prospero');

    const hostChip = page.locator('.qt-chat-announcement-chip').filter({ hasText: 'The Host' }).last();
    const prosperoChip = page
      .locator('.qt-chat-announcement-chip')
      .filter({ hasText: 'Prospero' })
      .last();
    await expect(hostChip).toBeVisible({ timeout: 15_000 });
    await expect(prosperoChip).toBeVisible();

    // Both packed into ONE flex-wrapping group (v4 `announcement-render-items`).
    // Asserted by membership rather than by count: this file's first beat leaves
    // a Librarian chip in the same run, and the beats share one instance.
    const group = page.locator('.qt-chat-announcement-group').last();
    await expect(
      group.locator('.qt-chat-announcement-chip').filter({ hasText: 'The Host' }),
    ).toHaveCount(1);
    await expect(
      group.locator('.qt-chat-announcement-chip').filter({ hasText: 'Prospero' }),
    ).toHaveCount(1);

    // v4 `AnnouncementBarContents`, in order: dot · sender · kind · time · chevron.
    await expect(hostChip.locator('.qt-chat-announcement-dot')).toHaveCount(1);
    await expect(hostChip.locator('.qt-chat-system-bar-sender')).toHaveText('The Host');
    // The posted row carries systemKind `announcement`, which has no display
    // override, so it shows verbatim — and the span is PRESENT because there is
    // a kind to show (it is omitted entirely when there is not).
    await expect(hostChip.locator('.qt-chat-system-bar-kind')).toHaveText('announcement');
    await expect(hostChip.locator('.qt-chat-system-bar-time')).toHaveCount(1);
    await expect(hostChip.locator('[data-icon="chevron-right"]')).toHaveCount(1);
    // The dot is decorative — its colour is restated by the sender/kind text.
    await expect(hostChip.locator('.qt-chat-announcement-dot')).toHaveAttribute(
      'aria-hidden',
      'true',
    );

    // Opening one leaves the other collapsed.
    await hostChip.click();
    await expect(hostChip.locator('[data-icon="chevron-down"]')).toHaveCount(1);
    await expect(hostChip).toHaveAttribute('aria-expanded', 'true');
    await expect(prosperoChip).toHaveAttribute('aria-expanded', 'false');
    await expect(prosperoChip.locator('[data-icon="chevron-right"]')).toHaveCount(1);
  });
});
