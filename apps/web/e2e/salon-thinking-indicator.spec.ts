import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { startMockLlm, MOCK_LLM_REPLY, type MockLlm } from './support/mock-llm';

/**
 * P4.d17 — the waiting quill, live.
 *
 * The indicator only exists while a turn is in flight, so this walks a real send
 * through the real binary + spine and catches it mid-stream: the motion class is
 * mounted while the reply streams and gone once the turn settles, and the status
 * strip's `streaming` stage carries the quill rather than the pulsing dot.
 *
 * The mock streams SLOWLY here (`delayMs`) on purpose. Its default instant
 * stream can complete before the first assertion polls, which would make this
 * beat a coin flip; spacing the deltas gives the in-flight UI a window wide
 * enough to observe without any arbitrary sleep on the test side.
 *
 * Sends land in "Group Expedition", NOT "Solo Voyage": the P4.6ap chat-totals
 * beat (`salon-token-cost-flow.spec.ts`, which runs AFTER this file
 * alphabetically) asserts a hardcoded 15.4K-token baseline for Solo Voyage, and
 * every message sent there shifts it. No spec asserts token totals or message
 * counts on Group Expedition.
 */
test.describe('P4.d17 — the waiting quill (LIVE)', () => {
  let mock: MockLlm;

  test.beforeAll(async () => {
    mock = await startMockLlm(MOCK_LLM_REPLY, MOCK_LLM_PORT, 350);
  });
  test.afterAll(async () => {
    await mock?.close();
  });

  async function maybeUnlock(page: Page) {
    const passphrase = page.locator('#qt-passphrase');
    const chats = page.getByRole('heading', { name: 'Chats', exact: true });
    await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
  }

  test('the quill rocks while the reply streams and is gone once it settles', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);

    const card = page.locator('.chat-card-stack a.qt-entity-card', {
      hasText: 'Group Expedition',
    });
    await expect(card).toBeVisible();
    await card.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible();

    // Nothing in flight yet — no indicator anywhere on the settled conversation.
    const quill = page.locator('.qt-thinking-indicator');
    await expect(quill).toHaveCount(0);

    // The composer is the qt-rich-editor (P4.6ag): drive the contenteditable
    // with real key events.
    const composer = page.locator('.qt-chat-composer-input .qt-rich-editor-content');
    await composer.click();
    await page.keyboard.type('Are we there yet?');
    await page.keyboard.press('Enter');

    // Mid-flight: the quill is mounted, wearing the `thinking` glyph.
    await expect(quill.first()).toBeVisible({ timeout: 15_000 });
    await expect(quill.first()).toHaveAttribute('data-icon', 'thinking');

    // The status strip carries it while tokens actually stream, and says so.
    const strip = page.locator('.qt-chat-response-status[data-stage="streaming"]');
    await expect(strip.locator('.qt-thinking-indicator')).toBeVisible({ timeout: 15_000 });
    await expect(strip.locator('.animate-pulse')).toHaveCount(0);

    // Settled: the reply is persisted and every indicator is gone.
    await expect(page.getByText(MOCK_LLM_REPLY).first()).toBeVisible({ timeout: 30_000 });
    await expect(quill).toHaveCount(0, { timeout: 30_000 });
  });
});
