import { expect, request as pwRequest, test, type Page } from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this rides the SHARED global-setup server and only unlocks it if the
 * gate is showing, so its filename must sort AFTER foundation.spec.ts (which
 * walks the locked→unlock gate first; workers: 1, alphabetical).
 * "settings-data-system-flow" sorts after "foundation" ('se' > 'fo').
 *
 * P4.9G2 — a LIVE walk of the newly fitted-out Settings → Data & System tab.
 * The runnable beats use verbs already on main (changePassphrase / lock /
 * unlockState / chatSettingsUpdate): the eight-card tab walk + a `?section=`
 * deep-link, a passphrase round-trip (changed to a temp value and BACK, so the
 * shared server's passphrase is left exactly as found), and the app-wide
 * auto-lock provider's idle warning under a Playwright fake clock. The
 * Tasks Queue / Import-Export / Delete-All beats need the sixteen §1 verbs
 * P4.9G1 delivers, so they ACTIVATE-AT-UNIFY behind a probe.
 */

const TEMP_PASSPHRASE = 'a temporary interlude';

/** ACTIVATE-AT-UNIFY: the Data & System server verbs land in sibling P4.9G1. */
let serverBackendReady = false;

test.beforeAll(async () => {
  try {
    const ctx = await pwRequest.newContext();
    const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: { type: 'systemTasksQueue' } });
    const body = (await res.json().catch(() => null)) as
      | { type?: string; data?: { message?: string } }
      | null;
    await ctx.dispose();
    const unknownVariant =
      body?.type === 'error' && /unknown variant/i.test(String(body?.data?.message ?? ''));
    serverBackendReady = body != null && !unknownVariant;
  } catch {
    serverBackendReady = false;
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

test.describe('P4.9G2 — the Data & System tab', () => {
  test('the tab renders v4\'s card order (no Plugins) and honours a ?section= deep link', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/settings?tab=system');

    for (const title of [
      'Encryption Passphrase',
      'Auto-Lock',
      'Backup & Restore',
      'Import / Export',
      'LLM Logging',
      'Tasks Queue',
      'LLM Logs',
      'Delete All Data',
    ]) {
      await expect(page.getByText(title, { exact: true }).first()).toBeVisible({ timeout: 15_000 });
    }

    // The WON'T-PORT Plugins card renders nothing.
    await expect(page.getByText('Plugins', { exact: true })).toHaveCount(0);
    // The placeholder is gone.
    await expect(page.getByText('not yet fitted out')).toHaveCount(0);

    // A ?section= deep link force-opens its card: the Auto-Lock body appears.
    await page.goto('/settings?tab=system&section=auto-lock');
    await expect(
      page.getByText('Automatically lock after idle period').or(
        page.getByText('Auto-lock requires a passphrase'),
      ),
    ).toBeVisible({ timeout: 15_000 });
  });

  test('passphrase round-trip: change to a temp value and back (state restored)', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/settings?tab=system&section=encryption-passphrase');

    const current = page.locator('#cp-current');
    const next = page.locator('#cp-new');
    const confirm = page.locator('#cp-confirm');
    await expect(current).toBeVisible({ timeout: 15_000 });

    // E2E_PASSPHRASE → TEMP_PASSPHRASE.
    await current.fill(E2E_PASSPHRASE);
    await next.fill(TEMP_PASSPHRASE);
    await confirm.fill(TEMP_PASSPHRASE);
    await page.getByRole('button', { name: 'Change Passphrase' }).click();
    await expect(page.getByText('Passphrase changed successfully.')).toBeVisible({ timeout: 15_000 });

    // TEMP_PASSPHRASE → E2E_PASSPHRASE (restore so later specs' unlock still works).
    await current.fill(TEMP_PASSPHRASE);
    await next.fill(E2E_PASSPHRASE);
    await confirm.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Change Passphrase' }).click();
    await expect(page.getByText('Passphrase changed successfully.')).toBeVisible({ timeout: 15_000 });
  });

  test('auto-lock: the provider warns after the idle threshold (fake clock)', async ({ page }) => {
    // Install a controllable clock BEFORE the app boots so the provider's
    // setInterval + Date.now ride it.
    await page.clock.install();
    await page.goto('/salon');
    await maybeUnlock(page);

    // idleMinutes 2 ⇒ warning at 1 min, lock at 2 min. Enable via the card.
    await page.goto('/settings?tab=system&section=auto-lock');
    const toggle = page.locator('qt-auto-lock-settings-card input[type="checkbox"]');
    await expect(toggle).toBeVisible({ timeout: 15_000 });
    if (!(await toggle.isChecked())) {
      await toggle.check();
    }
    const minutes = page.locator('qt-auto-lock-settings-card input[type="number"]');
    await expect(minutes).toBeVisible({ timeout: 15_000 });
    const savedTwo = page.waitForResponse(
      (r) =>
        r.url().includes('/api/dispatch') &&
        (r.request().postData() ?? '').includes('autoLockSettings') &&
        (r.request().postData() ?? '').includes('"idleMinutes":2'),
    );
    await minutes.fill('2');
    await minutes.blur();
    await savedTwo;

    // Advance past the 1-minute warning threshold (but short of the 2-minute
    // lock) with no interaction; the 60 s idle check fires the warning banner.
    await page.clock.fastForward('01:05');
    await expect(page.getByText('Auto-Lock Warning')).toBeVisible({ timeout: 15_000 });

    // Restore: disable auto-lock so no later spec inherits the idle timer.
    const savedOff = page.waitForResponse(
      (r) =>
        r.url().includes('/api/dispatch') &&
        (r.request().postData() ?? '').includes('autoLockSettings') &&
        (r.request().postData() ?? '').includes('"enabled":false'),
    );
    await page.getByRole('button', { name: 'Dismiss' }).click();
    await toggle.uncheck();
    await savedOff;
  });

  // === ACTIVATE-AT-UNIFY (needs P4.9G1's §1 verbs) ===

  test('tasks queue: view a background job', async ({ page }) => {
    test.skip(!serverBackendReady, 'Needs P4.9G1 systemTasksQueue (ACTIVATE-AT-UNIFY)');
    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/settings?tab=system&section=tasks-queue');
    await expect(page.getByText('Simultaneous Labours', { exact: false })).toBeVisible({
      timeout: 15_000,
    });
    // The stats + queue render from the live systemTasksQueue verb.
    await expect(page.getByText('Queue Items', { exact: true })).toBeVisible({ timeout: 15_000 });
  });

  test('export → import round-trip', async ({ page }) => {
    test.skip(!serverBackendReady, 'Needs P4.9G1 export/import legs (ACTIVATE-AT-UNIFY)');
    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/settings?tab=system&section=import-export');
    await page.getByRole('button', { name: 'Export Data' }).click();
    await expect(page.getByText('Select the type of data you want to export')).toBeVisible({
      timeout: 15_000,
    });
    // The full export→import round-trip is fleshed out when the web-edge legs land.
  });
});

/**
 * The delete-all beat runs LAST in its OWN spec file-position, sequenced after
 * every other beat: the fixture is copied fresh per run, so a full wipe is safe.
 * ACTIVATE-AT-UNIFY behind the same probe.
 */
test.describe('P4.9G2 — Delete All Data (destructive; last)', () => {
  test('delete-all preview → confirm → complete', async ({ page }) => {
    test.skip(!serverBackendReady, 'Needs P4.9G1 systemDeleteData (ACTIVATE-AT-UNIFY)');
    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/settings?tab=system&section=delete-all-data');
    await page.getByRole('button', { name: 'Delete All Data' }).click();
    await expect(page.getByText('The following data will be permanently deleted')).toBeVisible({
      timeout: 15_000,
    });
    await page.getByRole('button', { name: 'Continue' }).click();
    await page.locator('input[placeholder="Type DELETE to confirm"]').fill('DELETE');
    await page.getByRole('button', { name: 'Delete Everything' }).click();
    await expect(page.getByText('Successfully deleted', { exact: false })).toBeVisible({
      timeout: 30_000,
    });
  });
});
