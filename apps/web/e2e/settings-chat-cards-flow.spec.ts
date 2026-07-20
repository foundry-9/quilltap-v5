import { expect, request as pwRequest, test, type Page } from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so its
 * filename must sort AFTER foundation.spec.ts (foundation walks the locked→unlock
 * gate and must reach the shared server first; workers: 1, alphabetical order).
 * "settings-chat-cards-flow" sorts after "foundation" ('se' > 'fo').
 *
 * P4.6an — a LIVE browser walk of the newly fitted-out Settings → Chat tab: the
 * full v4 card order renders; a scalar toggle (Auto-Scroll) and a nested-bag
 * select (Dangerous Content's mode) each round-trip through the real
 * `chatSettingsUpdate` dispatch and survive a reload; and the cron next-run
 * preview computes client-side as you type.
 *
 * Every write here goes through the real server against the shared instance's
 * `chat_settings` row — no route mocks. The two persistence beats deliberately
 * pick one scalar and one BAG: the bag path is the one where a partial nested
 * patch would silently drop sibling keys, and only a real round-trip proves it
 * doesn't.
 */

/**
 * ACTIVATE-AT-UNIFY: the General State card edits its instance-wide tier through
 * the §A `generalState*` verbs, which land in lane D10. This probe skips the
 * General State beat in-lane (the Rust core has no state arms — the request
 * deserialization fails with an "unknown variant" error) and runs it live once
 * lane D10's arms merge.
 */
let stateBackendReady = false;

test.beforeAll(async () => {
  try {
    const ctx = await pwRequest.newContext();
    const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: { type: 'generalStateGet' } });
    const body = (await res.json().catch(() => null)) as
      | { type?: string; data?: { message?: string } }
      | null;
    await ctx.dispose();
    const isUnknownVariant =
      body?.type === 'error' && /unknown variant/i.test(String(body?.data?.message ?? ''));
    stateBackendReady = body != null && !isUnknownVariant;
  } catch {
    stateBackendReady = false;
  }
});

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

test.describe('P4.6an — the Chat-tab settings cards', () => {
  test('the tab renders v4\'s full card order, placeholder gone', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/settings?tab=chat');

    // A card from each band of v4's order (see chat-tab.spec.ts for the full pin).
    for (const title of [
      'Composition Mode',
      'Composer',
      'Auto-Scroll',
      'Text Replacement',
      'Token Display',
      'Context Compression',
      'Memory Cascade',
      'Image Description',
      'Automation',
      'General State',
      'Agent Mode',
      'Thinking / Reasoning',
      'Answer Confirmation',
      'Dangerous Content',
      'Data Retention',
      'Autonomous Rooms',
      'Scheduled Autonomous Rooms',
    ]) {
      await expect(page.getByText(title, { exact: true }).first()).toBeVisible({ timeout: 15_000 });
    }

    // The retired loud deferral is gone for good.
    await expect(page.getByText('not yet fitted out')).toHaveCount(0);
  });

  test('Auto-Scroll: toggle → reload → persisted (a scalar round-trip)', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/settings?tab=chat&section=auto-scroll');

    const box = page.locator('qt-auto-scroll-settings input[type="checkbox"]');
    await expect(box).toBeVisible({ timeout: 15_000 });
    // v4's default when unset is false.
    await expect(box).not.toBeChecked();

    const saved = waitForSave(page, 'autoScrollOnResponseComplete');
    await box.check();
    await saved;

    await page.goto('/settings?tab=chat&section=auto-scroll');
    const after = page.locator('qt-auto-scroll-settings input[type="checkbox"]');
    await expect(after).toBeChecked({ timeout: 15_000 });

    // Put it back, so the beat is re-runnable against the shared instance.
    const restored = waitForSave(page, 'autoScrollOnResponseComplete');
    await after.uncheck();
    await restored;
  });

  test('Dangerous Content: change the mode → reload → persisted (a BAG round-trip)', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/settings?tab=chat&section=dangerous-content');

    const mode = page.locator('#danger-mode');
    await expect(mode).toBeVisible({ timeout: 15_000 });

    // Normalize to OFF first — the committed salon fixture ships
    // `dangerousContentSettings.mode = 'DETECT_ONLY'`, and a run that died
    // mid-walk could leave it elsewhere again. Asserting the instance ARRIVES
    // at OFF would just be encoding today's fixture into this beat.
    if ((await mode.inputValue()) !== 'OFF') {
      const reset = waitForSave(page, 'dangerousContentSettings');
      await mode.selectOption('OFF');
      await reset;
    }
    await expect(mode).toHaveValue('OFF');
    // While OFF, the rest of the card stays hidden (v4's gate).
    await expect(page.locator('#danger-display-mode')).toHaveCount(0);

    const saved = waitForSave(page, 'dangerousContentSettings');
    await mode.selectOption('DETECT_ONLY');
    await saved;

    // The gate opens on the live response, not an optimistic guess.
    await expect(page.locator('#danger-display-mode')).toBeVisible();

    await page.goto('/settings?tab=chat&section=dangerous-content');
    await expect(page.locator('#danger-mode')).toHaveValue('DETECT_ONLY', { timeout: 15_000 });
    // The whole bag survived the round-trip — the sibling keys are still there,
    // which is what a partial nested patch would have destroyed.
    await expect(page.locator('#danger-threshold')).toHaveValue('0.7');

    const restored = waitForSave(page, 'dangerousContentSettings');
    await page.locator('#danger-mode').selectOption('OFF');
    await restored;
  });

  test('General State: open the editor → save a key → reload → sees it (§A)', async ({ page }) => {
    test.skip(!stateBackendReady, 'awaits lane D10 state dispatch (activates at unification)');

    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/settings?tab=chat&section=general-state');

    // The card is open on its deep link; the entry button reveals the editor.
    await page.getByRole('button', { name: 'Edit General State' }).click();
    const modal = page.locator('qt-state-editor-modal');
    await expect(modal).toBeVisible({ timeout: 15_000 });
    await expect(modal).toContainText('General State');
    // The instance-wide tier never shows the chat cascade note.
    await expect(modal).not.toContainText('narrower tiers win');

    // Edit → type a JSON object with an underscore key (user-only, never
    // AI-modified) so the beat leaves no live-facing state behind → Save.
    await modal.getByRole('button', { name: 'Edit' }).click();
    const textarea = modal.locator('textarea');
    await textarea.fill('{\n  "_e2e_general": "brass"\n}');
    const saved = page.waitForResponse(
      (r) =>
        r.url().includes('/api/dispatch') &&
        r.request().method() === 'POST' &&
        (r.request().postData() ?? '').includes('generalStateSet'),
    );
    await modal.getByRole('button', { name: 'Save', exact: true }).click();
    await saved;

    // Close and reopen — the saved key survives the round-trip through the tier.
    await modal.getByRole('button', { name: 'Close' }).click();
    await page.getByRole('button', { name: 'Edit General State' }).click();
    await expect(page.locator('qt-state-editor-modal textarea')).toHaveValue(/_e2e_general/, {
      timeout: 15_000,
    });

    // Reset the tier so the shared instance is left clean (re-runnable).
    const reset = page.locator('qt-state-editor-modal');
    await reset.getByRole('button', { name: 'Reset State' }).click();
    await reset.getByRole('button', { name: 'Confirm Reset' }).click();
    await expect(reset.locator('textarea')).toHaveValue('{}', { timeout: 15_000 });
  });

  test('the cron next-run preview computes as you type', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/settings?tab=chat&section=autonomous-rooms');

    // The New-Chat form hosts the shared card with the cron field.
    await page.goto('/salon/new?autonomous=1');
    const cron = page.locator('#autonomous-cron');
    await expect(cron).toBeVisible({ timeout: 15_000 });

    // Blank previews nothing.
    await expect(page.getByText('Next run:')).toHaveCount(0);
    await expect(page.getByText('Invalid cron:')).toHaveCount(0);

    // A valid five-field cron previews the next fire time — computed in the
    // browser by croner, exactly as v4 does, with no round-trip.
    await cron.fill('0 4 * * *');
    await expect(page.getByText('Next run:')).toBeVisible({ timeout: 5_000 });

    // Garbage surfaces croner's own message.
    await cron.fill('nonsense');
    await expect(page.getByText('Invalid cron:')).toBeVisible({ timeout: 5_000 });
    await expect(page.getByText('Next run:')).toHaveCount(0);

    // Clearing it returns to no preview at all.
    await cron.fill('');
    await expect(page.getByText('Invalid cron:')).toHaveCount(0);
    await expect(page.getByText('Next run:')).toHaveCount(0);
  });
});
