import { expect, test, type Page } from './support/fixtures';

import { startMockLlm, MOCK_LLM_REPLY, type MockLlm } from './support/mock-llm';
import { E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { openSidebarSection } from './support/sidebar';

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
    await page.goto('/salon');
    await maybeUnlock(page);

    // Open the multi-character group chat.
    const groupCard = page.locator('.chat-card-stack a.qt-entity-card', {
      hasText: 'Group Expedition',
    });
    await expect(groupCard).toBeVisible();
    await groupCard.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible();

    // --- Pause round-trip: a new control exercised end-to-end (chatUpdate). ---
    // The pause control lives in the sidebar's Participants drawer since P4.9H1
    // (v4's home for it — the turn-controls bar keeps only the paused notice).
    await openSidebarSection(page, 'Participants');
    const pauseButton = page.locator('qt-chat-sidebar .qt-chat-pause-button');
    const pausedBanner = page.locator('.qt-chat-paused-banner');
    await expect(pauseButton).toBeVisible();
    const startedPaused = ((await pauseButton.textContent()) ?? '').includes('Resume');
    await pauseButton.click();
    if (startedPaused) {
      await expect(pausedBanner).toHaveCount(0);
      await expect(pauseButton).toContainText('Pause');
    } else {
      // Pin the CLAIM, not the opening words. Dogfood #118: the notice kept the
      // pre-bug-137 promise that "whoever's turn it is will still answer a
      // message you send" long after P4.D186 made a paused room answer nobody,
      // and an 'Auto-responses are paused' assertion stayed green through it.
      await expect(pausedBanner).toContainText(
        'a message you send is recorded without an answer',
      );
      await expect(pausedBanner).not.toContainText('will still answer a message you send');
      await expect(pauseButton).toContainText('Resume');
    }
    // Toggle back to the original state.
    await pauseButton.click();
    await expect(pauseButton).toContainText(startedPaused ? 'Resume' : 'Pause');

    // --- Guarded: the user-turn Skip banner → Host turn-pass chip. ---
    //
    // P4.D187 (bug 137) added the second assertion below. A skip used to lift
    // the pause silently on its way — `triggerContinueMode` refused outright
    // while paused, so Nudge and Skip cleared the pause to work at all — and v4
    // deleted both seams. A skip from a PAUSED room must now leave the room
    // paused, which the Pause button's own label is the plainest witness to.
    const banner = page.locator('.qt-chat-user-turn-banner');
    const skip = banner.getByRole('button', { name: 'Skip' });
    if (await skip.count()) {
      const chipsBefore = await page.locator('.qt-chat-announcement-chip').count();
      // Pause first, so the skip has a pause it could wrongly lift.
      await pauseButton.click();
      await expect(pauseButton).toContainText('Resume');
      await skip.click();
      // A successful skip posts a Host turn-pass chip ("nothing to add").
      await expect(page.locator('.qt-chat-announcement-chip')).toHaveCount(chipsBefore + 1, {
        timeout: 15_000,
      });
      // THE GUARD: the pause the operator set is still standing. Before bug 137
      // this read "Pause", because the skip had quietly resumed the room.
      await expect(pauseButton).toContainText('Resume');
      // Put the room back as it was found.
      await pauseButton.click();
      await expect(pauseButton).toContainText('Pause');
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
