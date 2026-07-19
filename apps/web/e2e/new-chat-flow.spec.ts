import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { MOCK_LLM_REPLY, startMockLlm, type MockLlm } from './support/mock-llm';

/**
 * P4.6q: browser walk of the New-Chat vertical — unlock → the Salon list's
 * "New Chat" affordance → `/salon/new` → pick a character (its connection
 * profile auto-seeds) → Create → the Green Room narrates → the walk lands on the
 * created conversation with the greeting rendered.
 *
 * Runs against the committed Salon fixture the global setup provisions (P4.6a:
 * characters + the OPENAI_COMPATIBLE profile rewritten to the fixed
 * MOCK_LLM_PORT), so this spec only starts the mock on that port — the same
 * recipe as `m4-salon.spec.ts`. The server side of chat creation + the Green
 * Room SSE replay are already live (P4.4u2); this is the SPA leg. Activated at
 * the P4.6p/q/r unification; unlock-state-tolerant per the standing recipe.
 */
test.describe('P4.6q — New-Chat vertical (list → /salon/new → create → land)', () => {
  let mock: MockLlm;

  test.beforeAll(async () => {
    mock = await startMockLlm(MOCK_LLM_REPLY, MOCK_LLM_PORT);
  });

  test.afterAll(async () => {
    await mock?.close();
  });

  /** Unlock only when the passphrase screen is showing (the shared server may already be unlocked). */
  async function maybeUnlock(page: Page): Promise<void> {
    const passphrase = page.locator('#qt-passphrase');
    const chats = page.getByRole('heading', { name: 'Chats', exact: true });
    await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
  }

  test('New Chat → pick a character → create → land on the conversation', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);

    // The Salon list header carries the New-Chat affordance.
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
    await page.getByRole('link', { name: 'New Chat' }).first().click();

    // The New-Chat form renders.
    await expect(page.getByRole('heading', { name: 'New Chat', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Select Characters' })).toBeVisible();

    // Pick the first roster character — its connection profile auto-seeds, so the
    // "Speaks First" badge appears and the create button enables.
    await page.locator('.new-chat-character-picker button').first().click();
    await expect(page.getByText('Speaks First')).toBeVisible();

    const create = page.getByRole('button', { name: 'Create Chat' });
    await expect(create).toBeEnabled();
    await create.click();

    // The Green Room narrates while the conversation is assembled (best-effort —
    // the dispatch resolving closes it), then the walk lands on the new chat with
    // the streamed greeting rendered.
    await expect(page).toHaveURL(/\/salon\/[0-9a-f-]{16,}/, { timeout: 20_000 });
    await expect(page.getByText(MOCK_LLM_REPLY).first()).toBeVisible({ timeout: 20_000 });
  });
});
