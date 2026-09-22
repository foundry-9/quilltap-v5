import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { startMockLlm, type MockLlm } from './support/mock-llm';

/**
 * P4.D206 — the re-roll that says so while it happens (v4 `f564b0de3`).
 *
 * Pressing a character line's refresh icon used to do nothing visible until the
 * new line appeared, sometimes half a minute later: no way to tell a slow model
 * from a dead one, and nothing stopping a second press — or a fresh message
 * typed into the composer — from landing on a turn already in flight. It now
 * narrates itself in the three places a first-time turn does: the line dims
 * under a `Regenerating...` plate that withdraws on the first token, the status
 * strip carries the stage, and the composer and the message's own action icons
 * are shut for the duration.
 *
 * ORDERING: rides the shared global-setup server, so the filename must sort
 * after `foundation.spec.ts` ('sa' > 'fo').
 *
 * **ACTIVATE-AT-UNIFY.** `messageSwipe { stream: true }` and the
 * `swipeProgress` events it emits are P4.D207's, which does not exist on main
 * while this lane runs — and `messageSwipe` ITSELF has been live since P4.6a,
 * so a capability probe on the verb would read true and activate these beats
 * into a false pass (they would then watch a blocking call and see no plate at
 * all). Gated by the NAMED constant instead, per the round's §R.11. **The
 * unifier flips it to `true` after P4.D207 is picked, and the beats' first live
 * run is the unified gate's own step.**
 *
 * The mock streams SLOWLY (`delayMs`), because every assertion here is about
 * the WINDOW: an instant stream settles before the first poll and the plate,
 * the lock and the inert action bar are all gone before anything can look at
 * them. Sends land in "Group Expedition", never "Solo Voyage", whose
 * hardcoded token baseline another spec asserts.
 */
const P4D207_SERVER_LANDED = false;

/** Long enough that the in-flight window is genuinely observable. */
const SLOW_STREAM_MS = 400;

test.describe('P4.D206 — a regeneration narrates itself', () => {
  let mock: MockLlm;

  test.beforeAll(async () => {
    mock = await startMockLlm(
      'A second telling, rather better than the first.',
      MOCK_LLM_PORT,
      SLOW_STREAM_MS,
    );
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

  async function openChat(page: Page, title: string) {
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
    const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: title });
    await expect(card).toBeVisible();
    await card.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible();
  }

  /**
   * Send one message and wait for the reply to settle, so there is a character
   * line to re-roll that this beat owns.
   */
  async function sendAndSettle(page: Page, text: string) {
    const composer = page.locator('.qt-chat-composer-input').first();
    await composer.click();
    await page.keyboard.type(text);
    await page.locator('button[aria-label="Send message"]').click();
    await expect(page.locator('.qt-chat-message-assistant').last()).toBeVisible({
      timeout: 30_000,
    });
    // The turn is over when the composer is open again.
    await expect(page.locator('button[aria-label="Send message"]')).toBeVisible({
      timeout: 30_000,
    });
  }

  /** The refresh icon on the newest assistant row. */
  function regenerateButton(page: Page) {
    return page
      .locator('.qt-chat-message-row-assistant')
      .last()
      .getByRole('button', { name: 'Regenerate response', exact: true });
  }

  test('the plate goes up, withdraws on the first token, and the new line lands', async ({
    page,
  }) => {
    test.skip(
      !P4D207_SERVER_LANDED,
      'awaits P4.D207: messageSwipe { stream: true } + the swipeProgress events',
    );
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');
    await sendAndSettle(page, 'Tell me about the ridge.');

    await regenerateButton(page).click();

    // 1. The plate is up, over the dimmed original, and it says what is
    //    happening to a reader who cannot see the dimming.
    const plate = page.locator('.qt-chat-regenerating-plate');
    await expect(plate).toBeVisible({ timeout: 15_000 });
    await expect(plate).toHaveAttribute('role', 'status');
    await expect(page.locator('.qt-chat-regenerating-plate-text')).toHaveText('Regenerating...');
    await expect(page.locator('.qt-chat-regenerating-original')).toBeVisible();
    await expect(page.locator('.qt-chat-message[aria-busy="true"]')).toHaveCount(1);

    // 2. The plate WITHDRAWS the moment there is prose — no blank gap. The
    //    original goes with it, because the new line is now in its place.
    await expect(plate).toHaveCount(0, { timeout: 30_000 });
    await expect(page.locator('.qt-chat-regenerating-original')).toHaveCount(0);
    await expect(page.locator('.qt-chat-regenerating')).toContainText('A second telling', {
      timeout: 30_000,
    });

    // 3. The new variant is what is left on display, and the counter moved with
    //    it: without `selectSwipeVariant` the reconcile's id-carry would put the
    //    operator back on the line they just replaced (v4 bug (b)).
    await expect(page.locator('.qt-chat-regenerating')).toHaveCount(0, { timeout: 30_000 });
    await expect(page.locator('.qt-chat-message-assistant').last()).toContainText(
      'A second telling',
      { timeout: 30_000 },
    );
  });

  test('the composer and the action bar are shut for the duration', async ({ page }) => {
    test.skip(!P4D207_SERVER_LANDED, 'awaits P4.D207: the streamed swipe');
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');
    await sendAndSettle(page, 'And the pass beyond it?');

    await regenerateButton(page).click();
    await expect(page.locator('.qt-chat-regenerating-plate')).toBeVisible({ timeout: 15_000 });

    // The row's own icons go inert: nothing on that bar is safe to press at a
    // message whose content is mid-replacement.
    await expect(page.locator('.qt-chat-message-action-bar-disabled')).toHaveCount(1);

    // …and so does the whole composer — v4's bug (c), which v5 reproduced
    // inverted (every gutter control was gated on `disabled` alone while the
    // salon only ever passed `busy`).
    await expect(page.locator('button[aria-label="Insert announcement"]')).toBeDisabled();
    await expect(page.locator('button[aria-label="Inform the cast"]')).toBeDisabled();
    await expect(page.locator('button[aria-label="Attach file"]')).toBeDisabled();

    // Both open again once the re-roll is done.
    await expect(page.locator('.qt-chat-message-action-bar-disabled')).toHaveCount(0, {
      timeout: 30_000,
    });
    await expect(page.locator('button[aria-label="Inform the cast"]')).toBeEnabled({
      timeout: 30_000,
    });
  });

  test('the status strip carries the stage while the re-roll runs', async ({ page }) => {
    test.skip(!P4D207_SERVER_LANDED, 'awaits P4.D207: the four status beats');
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');
    await sendAndSettle(page, 'Once more, with the weather.');

    await regenerateButton(page).click();

    // v5's strip normally lives inside the streaming bubble, which does not
    // mount for a re-roll; the list carries the regeneration's own. The stage
    // is what the CSS keys the streaming colour on.
    const strip = page.locator('.qt-chat-response-status');
    await expect(strip).toBeVisible({ timeout: 15_000 });
    await expect(strip).toContainText('Regenerating');

    await expect(strip).toHaveCount(0, { timeout: 30_000 });
  });
});
