import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';
import { openSidebarSection } from './support/sidebar';

/**
 * ACTIVATE-AT-UNIFY (P4.D54 bug 22 / P4.D53 §1): the chat GET now projects the
 * four controlled sidebar selects (timelineMode, alertCharactersOfLanternImages,
 * showThinking, answerConfirmationOverride), so a saved value survives a reload
 * instead of snapping back. Flipped `true` at the f4955e0e-round unification:
 * P4.D53's chat-GET projection is on the branch, so the round-trip runs live.
 */
const PROJECTION_ROUNDTRIP_SERVER_LANDED = true;

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

/** The persisted + resolved agent mode, straight off the chat read's projection. */
async function readAgentMode(
  page: Page,
  chatId: string,
): Promise<{ stored: unknown; resolved: unknown }> {
  const resp = await page.request.post('/api/dispatch', {
    data: { type: 'chatGet', chatId },
  });
  expect(resp.ok(), `chatGet → ${resp.status()}`).toBe(true);
  const body = (await resp.json()) as {
    data?: { chat?: { agentModeEnabled?: unknown; resolvedAgentModeEnabled?: unknown } };
  };
  return {
    stored: body.data?.chat?.agentModeEnabled,
    resolved: body.data?.chat?.resolvedAgentModeEnabled,
  };
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

  /**
   * P4.D54 bug 22 — the Story's Clock now survives a reload, because the chat GET
   * projects `timelineMode` (P4.D53). Distinct from the write-echo test above: it
   * flips the clock, reloads the page (a fresh GET), reopens the Chat section, and
   * asserts the select still shows the saved value. Gated until P4.D53 lands.
   */
  test('the Story’s Clock survives a reload once the column is projected (bug 22)', async ({
    page,
  }) => {
    test.skip(
      !PROJECTION_ROUNDTRIP_SERVER_LANDED,
      'awaits P4.D53 chat-GET projection of timelineMode (flip PROJECTION_ROUNDTRIP_SERVER_LANDED at unification)',
    );
    await openSoloVoyage(page);
    await openSidebarSection(page, 'Chat');

    const clock = () =>
      page
        .locator('qt-chat-section label')
        .filter({ hasText: 'The Story’s Clock' })
        .locator('select');
    await expect(clock()).toBeVisible({ timeout: 10_000 });

    await clock().selectOption('narrative');
    await expect(page.getByText('The story now keeps its own hours')).toBeVisible({
      timeout: 15_000,
    });

    // A fresh GET on reload delivers the saved column; the select seeds from it.
    await page.reload();
    await openSidebarSection(page, 'Chat');
    await expect(clock()).toHaveValue('narrative');

    // Leave Solo Voyage on the clock the sibling tests expect.
    await clock().selectOption('realtime');
    await expect(page.getByText('The story is back on the clock on the wall')).toBeVisible({
      timeout: 15_000,
    });
  });

  /**
   * P4.9E3C — the agent-mode badge (v4 `ChatSidebar.tsx:1116-1127`), the hole
   * dogfood finding #34 found: a live verb with nothing on screen to press.
   * Unlike the Story's Clock, the chat GET DOES project this column, so the
   * read-back is the plain chat read.
   */
  test('the agent-mode badge writes the column and survives a reload', async ({ page }) => {
    const chatId = await openSoloVoyage(page);
    await openSidebarSection(page, 'Chat');

    const badge = page.locator('qt-chat-section button.qt-tool-palette-badge');
    await expect(badge).toBeVisible({ timeout: 10_000 });
    await expect(badge).toHaveText('Agent Off');
    await expect(badge).toHaveClass(/qt-tool-palette-badge-off/);

    await badge.click();
    await expect(page.getByText('Agent mode enabled')).toBeVisible({ timeout: 15_000 });
    await expect(badge).toHaveText('Agent On');
    await expect(badge).toHaveClass(/qt-tool-palette-badge-on/);
    expect(await readAgentMode(page, chatId)).toEqual({ stored: true, resolved: true });

    // The badge reads the RESOLVED value off a fresh chat read, so a reload is
    // what proves the write rather than the optimistic adoption.
    await page.reload();
    await openSidebarSection(page, 'Chat');
    await expect(page.locator('qt-chat-section button.qt-tool-palette-badge')).toHaveText(
      'Agent On',
    );

    await page.locator('qt-chat-section button.qt-tool-palette-badge').click();
    await expect(page.getByText('Agent mode disabled')).toBeVisible({ timeout: 15_000 });
    expect(await readAgentMode(page, chatId)).toEqual({ stored: false, resolved: false });
  });
});
