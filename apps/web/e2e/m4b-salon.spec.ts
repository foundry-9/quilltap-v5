import { expect, test, type Page } from '@playwright/test';

import { startMockLlm, MOCK_LLM_REPLY, type MockLlm } from './support/mock-llm';
import { E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';

/**
 * M4b: the tier-2 Salon controls (P4.6c) in a real browser against the baked
 * "Group Expedition" fixture (2 LLM + 1 user participant). The robust beat is a
 * pause round-trip (a `chatUpdate` dispatch, turn-state-independent). A guarded
 * sub-beat exercises the user-turn Skip banner → Host turn-pass chip WHEN the
 * fixture's turn state actually lands on the user participant; if it does not,
 * the beat annotates and the Skip→turn-pass behaviour stays covered by the
 * component tests (`salon-turn-controls.spec.ts`) rather than being forced.
 *
 * The mock LLM is started (like the M4 spec) so a Skip that hands the turn to an
 * LLM has a responder; the pause beat itself never touches it.
 */
test.describe('M4b — Salon turn controls (pause / Speaking-As / skip)', () => {
  let mock: MockLlm;

  test.beforeAll(async () => {
    mock = await startMockLlm(MOCK_LLM_REPLY, MOCK_LLM_PORT);
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

  test('pause round-trip in the group chat, plus a guarded user-turn skip beat', async ({
    page,
  }) => {
    await page.goto('/');
    await maybeUnlock(page);

    // Open the multi-character group chat.
    const groupCard = page.locator('.chat-card-stack a.qt-entity-card', {
      hasText: 'Group Expedition',
    });
    await expect(groupCard).toBeVisible();
    await groupCard.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible();

    // --- Pause round-trip: a new control exercised end-to-end (chatUpdate). ---
    const pauseButton = page.locator('.qt-chat-pause-button');
    const pausedBanner = page.locator('.qt-chat-paused-banner');
    await expect(pauseButton).toBeVisible();
    const startedPaused = ((await pauseButton.textContent()) ?? '').includes('Resume');
    await pauseButton.click();
    if (startedPaused) {
      await expect(pausedBanner).toHaveCount(0);
      await expect(pauseButton).toContainText('Pause');
    } else {
      await expect(pausedBanner).toContainText('Auto-responses are paused');
      await expect(pauseButton).toContainText('Resume');
    }
    // Toggle back to the original state.
    await pauseButton.click();
    await expect(pauseButton).toContainText(startedPaused ? 'Resume' : 'Pause');

    // --- Guarded: the user-turn Skip banner → Host turn-pass chip. ---
    const banner = page.locator('.qt-chat-user-turn-banner');
    const skip = banner.getByRole('button', { name: 'Skip' });
    if (await skip.count()) {
      const chipsBefore = await page.locator('.qt-chat-announcement-chip').count();
      await skip.click();
      // A successful skip posts a Host turn-pass chip ("nothing to add").
      await expect(page.locator('.qt-chat-announcement-chip')).toHaveCount(chipsBefore + 1, {
        timeout: 15_000,
      });
    } else {
      test.info().annotations.push({
        type: 'note',
        description:
          "Group Expedition's turn state did not land on the user participant; the " +
          'Skip → turn-pass beat is covered by salon-turn-controls.spec.ts.',
      });
    }
  });
});
