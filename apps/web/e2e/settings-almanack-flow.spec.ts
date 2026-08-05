import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this rides the SHARED global-setup server and only unlocks it if the
 * gate is showing, so its filename must sort AFTER foundation.spec.ts (which
 * walks the locked→unlock gate first; workers: 1, alphabetical).
 * "settings-almanack-flow" sorts after "foundation" ('se' > 'fo').
 *
 * P4.38 — the Almanack card on the AI Providers tab. Two ungated beats walk the
 * card's chrome and its `?section=` deep link, which need no server family at
 * all: the card renders, the collapsible opens, and the deep link force-opens
 * it. The generate → view → download → delete walk needs P4.37's four verbs and
 * is gated by name below.
 */

/**
 * ACTIVATE-AT-UNIFY (lane P4.37 — the server half of the `f7f1a956` round).
 *
 * The gated beat compiles a real report, watches the phase bar, opens the
 * viewer, downloads the edition through the web edge and deletes it again. All
 * of it rides `systemAlmanack{Generate,List,Get,Delete}` plus the
 * `capabilities-report-get&download=true` REST leg, none of which exist on this
 * branch.
 *
 * A NAMED constant, deliberately, not a capability probe: a probe cannot tell a
 * verb that is DEFINED-but-refusing from one that works, and would silently
 * activate the beat into guaranteed failure (the standing e2e rule).
 *
 * ⚠ NOT flipped at the `f7f1a956` unification (2026-08-05): P4.37's verified
 * pure half (renderer + phase manifest + the progress `phase` frame) landed,
 * but the collectors / verbs / host wire were HELD BACK pending their tier-2
 * differential (P4.37 unit 12 — the lane's own record forbade unifying them
 * unverified; branch `claude/almanack-server-porting-693d77`). Flip to `true`
 * when the RESUMED P4.37 lands the four verbs + the `almanack_host` wire.
 */
const P437_SERVER_LANDED = false;

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

test.describe('P4.38 — the Almanack card', () => {
  test('the Providers tab carries the card, collapsed, and it opens', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/settings?tab=providers');

    // Locate the CARD by its sectionId, not by its title: the collapsible's
    // header and the card body inside it both say "The Almanack (System
    // Report)", so a text locator is strict-mode ambiguous once it is open.
    const card = page.locator('#capabilities-report');
    await expect(card).toBeVisible({ timeout: 15_000 });
    await expect(
      card.getByText(
        'A compendium of the whole establishment — configuration, contents and condition',
      ),
    ).toBeVisible();

    // Collapsed: the body's own button is not rendered yet.
    await expect(card.getByRole('button', { name: 'Compile the Almanack' })).toHaveCount(0);

    await card.locator('.qt-collapsible-card-header').click();
    await expect(card.getByRole('button', { name: 'Compile the Almanack' })).toBeVisible({
      timeout: 15_000,
    });
    await expect(card.getByText('Previous Editions', { exact: true })).toBeVisible();
  });

  test('?section=capabilities-report force-opens the card', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    // The anchor keeps v4's OLD wording on purpose — it is what help docs and
    // old bookmarks point at, and v4 kept it when the feature was renamed.
    await page.goto('/settings?tab=providers&section=capabilities-report');

    const card = page.locator('#capabilities-report');
    await expect(card.getByRole('button', { name: 'Compile the Almanack' })).toBeVisible({
      timeout: 15_000,
    });
  });

  test('compile → phase bar → viewer → download → delete', async ({ page }) => {
    test.skip(
      !P437_SERVER_LANDED,
      'ACTIVATE-AT-UNIFY (lane P4.37): the four systemAlmanack verbs and the ' +
        'capabilities-report-get download leg are not live yet — this walk ' +
        'self-activates when the server half lands',
    );

    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/settings?tab=providers&section=capabilities-report');

    const card = page.locator('#capabilities-report');
    const compile = card.getByRole('button', { name: 'Compile the Almanack' });
    await expect(compile).toBeVisible({ timeout: 15_000 });

    // A fresh instance starts with no editions.
    await expect(card.getByText('No editions yet. Compile one to get started.')).toBeVisible();

    // Register the waiter BEFORE the click: the generate is a single blocking
    // dispatch, and the beat's next gesture must not race its response.
    const generated = page.waitForResponse(
      (res) => res.url().includes('/api/dispatch') && res.status() === 200,
      { timeout: 120_000 },
    );
    await compile.click();

    // While it runs the button relabels and the segmented bar is on screen.
    await expect(card.getByRole('button', { name: 'Compiling the Almanack…' })).toBeVisible({
      timeout: 15_000,
    });
    await expect(card.locator('.qt-progress').first()).toBeVisible();

    await generated;

    // The dispatch's own resolution opens the viewer on the fresh report.
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible({ timeout: 30_000 });
    await expect(dialog.getByText('The Almanack (System Report)')).toBeVisible();
    // The report is markdown with GFM tables — the renderer emitted at least one.
    await expect(dialog.locator('.qt-almanack-report table').first()).toBeVisible();

    // The download leg answers with the edition's bytes.
    const download = page.waitForEvent('download', { timeout: 30_000 });
    await dialog.getByRole('link', { name: 'Download' }).click();
    const file = await download;
    expect(file.suggestedFilename()).toMatch(/\.md$/);

    // Two controls answer to "Close" — the header ✕ (aria-label) and the footer
    // button. Scope to the footer so the locator is unambiguous.
    await dialog.locator('.qt-dialog-footer').getByRole('button', { name: 'Close' }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);

    // The list gained the edition.
    const rows = card.locator('a[download]');
    await expect(rows).toHaveCount(1, { timeout: 15_000 });

    // Delete asks first; accept and the list empties again.
    page.once('dialog', (d) => {
      expect(d.message()).toContain('Are you sure you want to delete');
      void d.accept();
    });
    await card.getByRole('button', { name: 'Delete Report' }).click();
    // The toast lives in the app-root container, not inside the card.
    await expect(page.getByText('Report deleted')).toBeVisible({ timeout: 15_000 });
    await expect(card.getByText('No editions yet. Compile one to get started.')).toBeVisible({
      timeout: 15_000,
    });
  });
});
