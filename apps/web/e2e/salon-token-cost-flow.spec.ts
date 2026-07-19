import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so its
 * filename must sort AFTER foundation.spec.ts (foundation walks the locked→unlock
 * gate and must reach the shared server first; workers: 1, alphabetical order).
 * "salon-token-cost-flow" sorts after "foundation" ('sa' > 'fo').
 *
 * P4.6ap — LIVE browser walks of the token/cost display and the story-background
 * generation client surface:
 *
 *  1. **The per-message token badge** (LIVE): toggle `showPerMessageTokens`
 *     through the real `chatSettingsUpdate` dispatch, then watch the badge
 *     appear on the Solo Voyage message the fixture already carries tokens for
 *     (`promptTokens=8`, `completionTokens=4` — no seeding needed).
 *  2. **The Story Backgrounds card** (LIVE): a real round-trip that survives a
 *     reload.
 *  3. **The chat-totals summary** (LIVE since unification): reads lane A's real
 *     `chatGetCost` verb over the chat-row aggregates seeded in
 *     `global-setup.ts`.
 *  4. **The regenerate entry** (LIVE since unification): drives lane A's real
 *     un-refused `chatRegenerateBackground`. Scoped to the QUEUED response +
 *     flash — the e2e host has no image-provider key, so the job's OUTCOME is
 *     deliberately out of scope. Assert the enqueue, not the image.
 */

/** Unlock only when the passphrase screen is showing (the shared server stays unlocked). */
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

/** Wait for the settings PUT carrying `key` to come back from the real server. */
function waitForSave(page: Page, key: string) {
  return page.waitForResponse(
    (r) =>
      r.url().includes('/api/dispatch') &&
      r.request().method() === 'POST' &&
      (r.request().postData() ?? '').includes('chatSettingsUpdate') &&
      (r.request().postData() ?? '').includes(key),
  );
}

/**
 * Open the Token Display card's checkbox for `label` on the Chat settings tab.
 *
 * The `?section=` deep link is REQUIRED, not decoration: the Chat tab's sixteen
 * collapsible cards all render CLOSED, and a closed card renders no content at
 * all (`@if (isOpen())` in `collapsible-card.ts`) — only its heading. Without
 * `forceOpen`, `qt-token-display-settings` is simply not in the DOM. This is the
 * settings-autonomous-flow idiom.
 */
async function tokenDisplayToggle(page: Page, label: string) {
  await page.goto('/settings?tab=chat&section=token-display');
  const card = page.locator('qt-token-display-settings');
  await expect(card).toBeVisible({ timeout: 15_000 });
  return card.locator('label', { hasText: label }).locator('input[type="checkbox"]');
}

/** Navigate to the Solo Voyage conversation (the fixture chat with token data). */
async function openSoloVoyage(page: Page): Promise<void> {
  await page.goto('/salon');
  await page.getByRole('link', { name: 'Solo Voyage' }).first().click();
  await expect(page.getByText('Well met, traveller!')).toBeVisible({ timeout: 15_000 });
}

test.describe('P4.6ap — the per-message token badge (LIVE)', () => {
  test('the badge follows showPerMessageTokens through the real dispatch', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);

    // Normalize the starting state rather than assuming it: the flag defaults
    // false, but a sibling beat may have left it on.
    const toggle = await tokenDisplayToggle(page, 'Show Token Count on Messages');
    if (await toggle.isChecked()) {
      const saved = waitForSave(page, 'tokenDisplaySettings');
      await toggle.uncheck();
      await saved;
    }

    // OFF: the message with token data shows no badge.
    await openSoloVoyage(page);
    await expect(page.locator('qt-token-badge')).toHaveCount(0);

    // Turn it ON through the real settings dispatch.
    const toggleOn = await tokenDisplayToggle(page, 'Show Token Count on Messages');
    const saved = waitForSave(page, 'tokenDisplaySettings');
    await toggleOn.check();
    await saved;

    // ON: the badge renders the fixture's stored ACTUALS (8 prompt / 4 completion)
    // on the one message that HAS counts.
    //
    // Scoped to that ROW, deliberately. An absolute badge count across the chat
    // would depend on suite history: `m4-salon.spec.ts` sends a live turn into
    // this same chat through the mock LLM, and that reply arrives with REAL token
    // counts — so it grows a badge of its own and the count is 2 in-suite, 1 in
    // isolation. (A pleasant confirmation that the finalizer stores actuals and
    // the badge picks them up on a live turn — but not something to assert by
    // counting.)
    await openSoloVoyage(page);
    const withTokens = page
      .locator('qt-message-row')
      .filter({ hasText: 'Well met, traveller!' });
    const badge = withTokens.locator('qt-token-badge');
    await expect(badge).toBeVisible({ timeout: 15_000 });
    await expect(badge.locator('[title="Prompt tokens"]')).toHaveText('8');
    await expect(badge.locator('[title="Completion tokens"]')).toHaveText('4');

    // And the gate's other arm, history-independent: the fixture's user message
    // has null counts, so v4's `promptTokens || completionTokens` keeps it bare
    // even with the flag ON.
    await expect(
      page
        .locator('qt-message-row')
        .filter({ hasText: 'Hello there, captain.' })
        .locator('qt-token-badge'),
    ).toHaveCount(0);

    // Leave the instance as we found it (the shared server is reused).
    const toggleOff = await tokenDisplayToggle(page, 'Show Token Count on Messages');
    const savedOff = waitForSave(page, 'tokenDisplaySettings');
    await toggleOff.uncheck();
    await savedOff;
  });
});

test.describe('P4.6ap — the Story Backgrounds card (LIVE)', () => {
  test('the enable toggle round-trips through the real dispatch and survives a reload', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/settings?tab=images&section=story-backgrounds');

    const card = page.locator('qt-story-backgrounds-card');
    await expect(card).toBeVisible({ timeout: 15_000 });
    const toggle = card.locator('input[type="checkbox"]').first();
    const select = card.locator('select');

    // Normalize: start from OFF whatever a sibling beat left behind.
    if (await toggle.isChecked()) {
      const saved = waitForSave(page, 'storyBackgroundsSettings');
      await toggle.uncheck();
      await saved;
    }
    // The profile select is disabled while story backgrounds are off (v4).
    await expect(select).toBeDisabled();

    const saved = waitForSave(page, 'storyBackgroundsSettings');
    await toggle.check();
    await saved;
    await expect(select).toBeEnabled();

    // The real proof: a reload reads the value back from the server.
    await page.reload();
    await expect(card.locator('input[type="checkbox"]').first()).toBeChecked({ timeout: 15_000 });

    // Restore.
    const savedOff = waitForSave(page, 'storyBackgroundsSettings');
    await card.locator('input[type="checkbox"]').first().uncheck();
    await savedOff;
  });
});

test.describe('P4.6ap — the chat-totals summary (LIVE at unification)', () => {
  /**
   * ACTIVATED at unification: the beat originally route-mocked `chatGetCost`
   * (lane A's verb) with the Shared-contract §1 body; the P4.6ao/ap/aq unifier
   * dropped the mock, so the summary now reads the REAL verb over the
   * aggregates `global-setup.ts` seeds on Solo Voyage
   * (12000/3400/0.0234/'openrouter' — v4's non-detailed breakdown reads the
   * STORED chat-row aggregates, not a message sum).
   */
  test('the header shows "<total> tokens • <cost>" when showChatTotals is on', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);

    const toggle = await tokenDisplayToggle(page, 'Show Chat Token Totals');
    if (!(await toggle.isChecked())) {
      const saved = waitForSave(page, 'tokenDisplaySettings');
      await toggle.check();
      await saved;
    }

    await openSoloVoyage(page);
    const summary = page.locator('qt-chat-cost-summary');
    await expect(summary).toBeVisible({ timeout: 15_000 });
    await expect(summary).toContainText('15.4K tokens');
    // $0.023 — 0.0234 is above the $0.01 band edge, so 3 digits, not 4.
    await expect(summary).toContainText('$0.023');

    // Restore.
    const toggleOff = await tokenDisplayToggle(page, 'Show Chat Token Totals');
    const savedOff = waitForSave(page, 'tokenDisplaySettings');
    await toggleOff.uncheck();
    await savedOff;
  });
});

test.describe('P4.6ap — the Regenerate Background entry (LIVE at unification)', () => {
  /**
   * ACTIVATED at unification: the beat originally route-mocked the §2 success
   * body while `chatRegenerateBackground` was refusal-armed; the P4.6ao/ap/aq
   * unifier dropped the mock, so the click now drives the REAL un-refused edge
   * and the flashed message is the server's own "…queued" arm (a real
   * `background_jobs` row is enqueued). Still deliberately scoped to the
   * ENQUEUE: the e2e host has no image-provider key, so the job's outcome (an
   * actual backdrop) is out of scope.
   */
  test('the entry appears with story backgrounds on, and flashes the queued message', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);

    // The entry is gated on the settings flag — turn it on through the REAL dispatch.
    await page.goto('/settings?tab=images&section=story-backgrounds');
    const card = page.locator('qt-story-backgrounds-card');
    await expect(card).toBeVisible({ timeout: 15_000 });
    const enable = card.locator('input[type="checkbox"]').first();
    if (!(await enable.isChecked())) {
      const saved = waitForSave(page, 'storyBackgroundsSettings');
      await enable.check();
      await saved;
    }

    await openSoloVoyage(page);

    const entry = page.getByRole('button', { name: 'Regenerate Background' });
    await expect(entry).toBeVisible({ timeout: 15_000 });
    await expect(entry).toHaveAttribute('title', 'Regenerate story background image');

    await entry.click();
    await expect(page.getByText('Story background regeneration queued')).toBeVisible({
      timeout: 15_000,
    });

    // Restore: the entry disappears again once the flag is off.
    await page.goto('/settings?tab=images&section=story-backgrounds');
    const savedOff = waitForSave(page, 'storyBackgroundsSettings');
    await page.locator('qt-story-backgrounds-card input[type="checkbox"]').first().uncheck();
    await savedOff;
  });
});
