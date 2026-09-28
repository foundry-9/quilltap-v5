import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this rides the SHARED global-setup server and only unlocks it if the
 * gate is showing, so its filename must sort AFTER foundation.spec.ts (which
 * walks the locked→unlock gate first; workers: 1, alphabetical).
 * "settings-concierge-flow" sorts after "foundation" ('se' > 'fo').
 *
 * P4.D230 — the Concierge's own Settings tab (v4 #76, `3b463d6b1`), walked LIVE:
 * the five cards under their stable ids with the pre-screen collapsed; the
 * `?section=uncensored-desk` deep link force-opening the desk in routed mode;
 * a desk pick round-tripping through the real `chatSettingsUpdate` as the WHOLE
 * `conciergeSettings` object and surviving a reload; and the off-duty banner
 * following the On Duty switch across a reload.
 *
 * The two render beats read nothing stored — the tab draws the defaults over an
 * absent `conciergeSettings`. The two WRITE beats ride P4.D227's column, its
 * whole-object replace and its retired-key 400; they went live at the
 * chain's unification.
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

async function openConcierge(page: Page, query = ''): Promise<void> {
  await page.goto('/salon');
  await maybeUnlock(page);
  await page.goto(`/settings?tab=concierge${query}`);
  await expect(page.locator('#on-duty')).toBeVisible({ timeout: 15_000 });
}

/** Wait for the settings PUT carrying `conciergeSettings`; resolve to its parsed body. */
async function waitForConciergeSave(page: Page): Promise<Record<string, unknown>> {
  const resp = await page.waitForResponse(
    (r) =>
      r.url().includes('/api/dispatch') &&
      r.request().method() === 'POST' &&
      (r.request().postData() ?? '').includes('chatSettingsUpdate') &&
      (r.request().postData() ?? '').includes('conciergeSettings'),
  );
  expect(resp.ok()).toBe(true);
  const sent = JSON.parse(resp.request().postData() ?? '{}') as {
    settings?: { conciergeSettings?: Record<string, unknown> };
  };
  return sent.settings?.conciergeSettings ?? {};
}

function header(page: Page, sectionId: string) {
  return page.locator(`#${sectionId} > button.qt-collapsible-card-header`);
}

test.describe('P4.D230 — the Concierge Settings tab', () => {
  test('the five cards render under their stable ids, the pre-screen collapsed', async ({
    page,
  }) => {
    await openConcierge(page);

    // The tab sits third, and is the active one.
    await expect(page.locator('.qt-tab-active')).toContainText('The Concierge');
    await expect(
      page.getByText('Who gets asked when the usual providers refuse, and how flagged content is shown'),
    ).toBeVisible();

    for (const [id, title] of [
      ['on-duty', 'On Duty'],
      ['uncensored-desk', 'The Uncensored Desk'],
      ['refusals', 'When a Provider Refuses'],
      ['display', 'Display'],
      ['pre-screening', 'Pre-Screening (Advanced)'],
    ]) {
      await expect(page.locator(`#${id} .qt-card-title`)).toHaveText(title);
    }
    for (const id of ['on-duty', 'uncensored-desk', 'refusals', 'display']) {
      await expect(header(page, id)).toHaveAttribute('aria-expanded', 'true');
    }
    await expect(header(page, 'pre-screening')).toHaveAttribute('aria-expanded', 'false');
    await expect(page.locator('#concierge-threshold')).toHaveCount(0);

    // The Chat tab no longer carries the old card.
    await page.goto('/settings?tab=chat');
    await expect(page.getByText('Composition Mode', { exact: true }).first()).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.locator('#dangerous-content')).toHaveCount(0);
  });

  test('?section= force-opens and scrolls to its card (routed mode)', async ({ page }) => {
    // The Image Description card's link and the Salon's off-duty hint land here.
    await openConcierge(page, '&section=uncensored-desk');
    await expect(header(page, 'uncensored-desk')).toHaveAttribute('aria-expanded', 'true');
    await expect(page.locator('#concierge-uncensored-text-profile')).toBeVisible();
    await expect(page.locator('#uncensored-desk')).toBeInViewport();

    // The one card that starts collapsed opens for its own deep link.
    await openConcierge(page, '&section=pre-screening');
    await expect(header(page, 'pre-screening')).toHaveAttribute('aria-expanded', 'true');
    await expect(page.locator('#concierge-threshold')).toBeVisible();
    await expect(page.locator('#pre-screening')).toBeInViewport();
  });

  test('a desk pick round-trips as the whole conciergeSettings object → reload → persisted', async ({
    page,
  }) => {
    await openConcierge(page, '&section=uncensored-desk');

    // The crafter lists every connection profile (no compatibility filter), so
    // the seed always offers one to pick.
    const crafter = page.locator('#concierge-image-prompt-profile');
    await expect(crafter).toBeEnabled({ timeout: 15_000 });
    const pick = await crafter.locator('option').nth(1).getAttribute('value');
    expect(pick).toBeTruthy();

    // Normalize to the cheap LLM first — a run that died mid-walk may have left a pick.
    if ((await crafter.inputValue()) !== '') {
      const reset = waitForConciergeSave(page);
      await crafter.selectOption('');
      await reset;
    }

    const saved = waitForConciergeSave(page);
    await crafter.selectOption(pick!);
    const body = await saved;
    // The WHOLE object went out — the server replaces the column.
    expect(body['imagePromptProfileId']).toBe(pick);
    expect(Object.keys(body).sort()).toEqual(
      [
        'autoSwitchAfterRefusals',
        'display',
        'enabled',
        'imagePromptProfileId',
        'newChatsStartAs',
        'preScreen',
        'uncensoredImageProfileId',
        'uncensoredTextProfileId',
        'uncensoredVisionProfileId',
      ].sort(),
    );

    await openConcierge(page, '&section=uncensored-desk');
    await expect(page.locator('#concierge-image-prompt-profile')).toHaveValue(pick!, {
      timeout: 15_000,
    });

    const restored = waitForConciergeSave(page);
    await page.locator('#concierge-image-prompt-profile').selectOption('');
    expect((await restored)['imagePromptProfileId']).toBeNull();
  });

  test('the off-duty banner follows the On Duty switch across a reload', async ({ page }) => {
    await openConcierge(page);
    const onDuty = page.locator('#on-duty input[type="checkbox"]');
    const banner = page.getByText(
      'The Concierge is off duty. Nothing is rerouted, announced, switched or screened until he is back at his post.',
    );

    // Normalize: on duty (the default).
    if (!(await onDuty.isChecked())) {
      const reset = waitForConciergeSave(page);
      await onDuty.check();
      await reset;
    }
    await expect(banner).toHaveCount(0);

    const off = waitForConciergeSave(page);
    await onDuty.uncheck();
    expect((await off)['enabled']).toBe(false);
    await expect(banner).toBeVisible();

    await openConcierge(page);
    await expect(page.locator('#on-duty input[type="checkbox"]')).not.toBeChecked({
      timeout: 15_000,
    });
    await expect(banner).toBeVisible();

    const on = waitForConciergeSave(page);
    await page.locator('#on-duty input[type="checkbox"]').check();
    expect((await on)['enabled']).toBe(true);
    await expect(banner).toHaveCount(0);
  });
});
