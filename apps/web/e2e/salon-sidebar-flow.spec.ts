import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';
import { openSidebarSection } from './support/sidebar';

/**
 * P4.9H1 — the Salon chat sidebar (v4 `components/chat/ChatSidebar.tsx`), and
 * above all **the Story's Clock**, the episodic timeline-mode switch the whole
 * round exists for.
 *
 * ORDERING: rides the shared global-setup server, so the filename must sort
 * after `foundation.spec.ts` ('sa' > 'fo'), which walks the locked→unlock gate.
 *
 * Two walks:
 *
 *  1. **The sidebar itself** — the strip is what a fresh profile sees, expanding
 *     persists under v4's localStorage key, the accordion is single-open, the
 *     cast is listed with turn badges, and collapsing goes back to the strip.
 *  2. **The Story's Clock** — flip to Story time against the REAL chat update,
 *     see v4's success copy, and prove the column actually moved by asking the
 *     server. The read-back probe is a `chatUpdate` with an EMPTY bag: v4's chat
 *     GET never projects `timelineMode` (a v4 route gap this port carries
 *     faithfully — see `chat/sidebar/chat-section.ts`), but the update echoes
 *     the merged chat row, and an empty bag writes no columns. Then flip back.
 */

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

async function openSoloVoyage(page: Page): Promise<string> {
  await page.goto('/salon');
  await maybeUnlock(page);
  const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Solo Voyage' });
  await expect(card).toBeVisible({ timeout: 15_000 });
  await card.click();
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
  const id = new URL(page.url()).pathname.split('/').pop()!;
  expect(id).toBeTruthy();
  return id;
}

/** The persisted `timelineMode`, read back through the update echo (see the header). */
async function readTimelineMode(page: Page, chatId: string): Promise<string | null> {
  const resp = await page.request.post('/api/dispatch', {
    data: { type: 'chatUpdate', chatId, chat: {} },
  });
  expect(resp.ok(), `probe dispatch → ${resp.status()}`).toBe(true);
  const body = (await resp.json()) as {
    type?: string;
    data?: { chat?: { timelineMode?: string | null } };
  };
  expect(body.type).toBe('chat');
  return body.data?.chat?.timelineMode ?? null;
}

test.describe('P4.9H1 — the Salon chat sidebar', () => {
  test('opens as the mini strip, expands and persists, and runs a single-open accordion', async ({
    page,
  }) => {
    await openSoloVoyage(page);

    const sidebar = page.locator('qt-chat-sidebar');
    await expect(sidebar).toHaveClass(/qt-chat-sidebar-collapsed/);
    // The strip still shows the cast and the pause control.
    await expect(sidebar.locator('.qt-chat-sidebar-collapsed-avatar').first()).toBeVisible();
    await expect(page.getByRole('button', { name: 'Pause auto-responses' })).toBeVisible();

    await page.getByRole('button', { name: 'Expand chat sidebar' }).click();
    await expect(sidebar).toHaveClass(/qt-chat-sidebar(?!-collapsed)/);
    expect(
      await page.evaluate(() => localStorage.getItem('quilltap.chat-sidebar.collapsed')),
    ).toBe('false');

    // Participants opens by default and carries the cast.
    const participants = sidebar.locator('.qt-collapsible-card-header').filter({
      hasText: 'Participants',
    });
    await expect(participants).toHaveAttribute('aria-expanded', 'true');
    await expect(sidebar.locator('.qt-participant-card-name').first()).toBeVisible();
    await expect(sidebar.locator('[data-testid="position-badge"]').first()).toBeVisible();

    // Opening another card closes Participants (single-open accordion).
    await openSidebarSection(page, 'Organize');
    await expect(participants).toHaveAttribute('aria-expanded', 'false');
    await expect(page.getByRole('button', { name: 'Copy ID' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'State…' })).toBeVisible();

    // The preference survives a reload, and collapsing returns the strip.
    await page.reload();
    await expect(sidebar).toHaveClass(/qt-chat-sidebar(?!-collapsed)/);
    await page.getByRole('button', { name: 'Collapse chat sidebar' }).click();
    await expect(sidebar).toHaveClass(/qt-chat-sidebar-collapsed/);
    expect(
      await page.evaluate(() => localStorage.getItem('quilltap.chat-sidebar.collapsed')),
    ).toBe('true');
  });

  test('the Story’s Clock writes timelineMode through the real chat update, both ways', async ({
    page,
  }) => {
    const chatId = await openSoloVoyage(page);
    expect(await readTimelineMode(page, chatId)).toBeNull();

    await openSidebarSection(page, 'Chat');
    const clock = page
      .locator('qt-chat-section label')
      .filter({ hasText: 'The Story’s Clock' })
      .locator('select');
    await expect(clock).toBeVisible({ timeout: 10_000 });
    // A NULL column reads as Real time (v4's `=== 'narrative' ? … : 'realtime'`),
    // and the helper paragraph is the calendar-on-the-wall one.
    await expect(clock).toHaveValue('realtime');
    await expect(page.locator('qt-chat-section')).toContainText(
      'Memories are filed by the calendar on the wall',
    );

    await clock.selectOption('narrative');
    await expect(page.getByText('The story now keeps its own hours')).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.locator('qt-chat-section')).toContainText('The tale keeps its own hours');
    // The real column moved.
    expect(await readTimelineMode(page, chatId)).toBe('narrative');

    await clock.selectOption('realtime');
    await expect(page.getByText('The story is back on the clock on the wall')).toBeVisible({
      timeout: 15_000,
    });
    expect(await readTimelineMode(page, chatId)).toBe('realtime');
  });
});
