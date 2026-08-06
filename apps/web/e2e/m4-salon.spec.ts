import { expect, test } from './support/fixtures';

import { startMockLlm, MOCK_LLM_REPLY, type MockLlm } from './support/mock-llm';
import { E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';

/**
 * M4: browser walk of the first Salon vertical — unlock → list → open a chat with
 * real history → the baked messages render (staff chip among them) → send in the
 * solo chat → a streamed reply appears live → it survives a reload.
 *
 * Wired live at the P4.6 unification: the global setup copies the committed Salon
 * fixture (P4.6a — `salon-main.db`/`salon-mount.db`: "Solo Voyage" + "Group
 * Expedition") and rewrites the fixture's OPENAI_COMPATIBLE profile `baseUrl` to
 * the fixed MOCK_LLM_PORT BEFORE the server launches (the CLI write-lock refuses a
 * live holder), so this spec only has to start the mock on that port. The send
 * happens in the solo chat (one LLM participant → a single-turn reply); the group
 * chat carries the staff-announcement history the read assertions use.
 */
test.describe('M4 — Salon vertical (list → open → read → send)', () => {
  let mock: MockLlm;

  test.beforeAll(async () => {
    mock = await startMockLlm(MOCK_LLM_REPLY, MOCK_LLM_PORT);
  });

  test.afterAll(async () => {
    await mock?.close();
  });

  /**
   * Unlock only when the passphrase screen is showing — the shared server stays
   * unlocked once the foundation spec (which runs first) has walked the unlock
   * flow, so this spec must handle both states.
   */
  async function maybeUnlock(page: import('@playwright/test').Page) {
    const passphrase = page.locator('#qt-passphrase');
    const chats = page.getByRole('heading', { name: 'Chats', exact: true });
    await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
  }

  test('unlock → list → open → send → streamed reply survives reload', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);

    // The Salon list shows both fixture chats.
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
    const groupCard = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Group Expedition' });
    const soloCard = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Solo Voyage' });
    await expect(groupCard).toBeVisible();
    await expect(soloCard).toBeVisible();

    // Open the group chat: the baked history renders, staff chip included.
    await groupCard.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible();
    await expect(page.locator('.qt-chat-announcement-chip').first()).toBeVisible();
    await expect(page.getByText('I stride toward the ridge.')).toBeVisible();

    // Back to the list; open the solo chat and send — the streamed mock reply
    // appears live (single LLM participant → one turn).
    await page.goBack();
    await soloCard.click();
    await expect(page.getByText('Once, above the clouds...')).toBeVisible();
    // The composer is now the qt-rich-editor (D17 adoption, P4.6ag): drive the
    // contenteditable with real key events (fill() targets input/textarea).
    const composer = page.locator('.qt-chat-composer-input .qt-rich-editor-content');
    await composer.click();
    await page.keyboard.type('Good morning.');
    await page.keyboard.press('Enter');
    await expect(page.getByText(MOCK_LLM_REPLY).first()).toBeVisible({ timeout: 15_000 });

    // The persisted reply survives a reload (canonical refetch; the server is
    // already unlocked, so the deep link renders straight into the conversation).
    await page.reload();
    await expect(page.getByText(MOCK_LLM_REPLY).first()).toBeVisible({ timeout: 15_000 });
  });

  /**
   * P4.9H2B rider (p4.9o): the Scriptorium badge on a Salon list card queues an
   * on-demand render over the already-live `chatRenderConversation` verb. This
   * asserts the ENQUEUE (the dispatch + v4's toast), not job completion.
   */
  test('the Scriptorium badge on a list card enqueues a render', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);

    const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Group Expedition' });
    await expect(card).toBeVisible();
    const badge = card.locator('button[title^="Scriptorium:"]');
    await expect(badge).toBeVisible();

    // Clicking the badge dispatches the render (and must NOT navigate into the
    // chat — the badge swallows the link click).
    const enqueued = page.waitForResponse(
      (res) => res.url().includes('/api/dispatch') && res.status() === 200,
      { timeout: 30_000 },
    );
    await badge.click();
    await enqueued;

    // v4's success toast confirms the enqueue; we're still on the list.
    await expect(page.getByText('Conversation rendering queued')).toBeVisible({ timeout: 15_000 });
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
  });
});
