import { expect, test } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * P4.9G3 — the Delete All Data walk. **DESTRUCTIVE: it wipes the shared
 * instance**, so it must be the very last spec file Playwright runs.
 *
 * ⚠ THE `zz-` FILENAME IS LOAD-BEARING. The suite runs `workers: 1`,
 * `fullyParallel: false`, and Playwright orders spec files by path. This beat
 * was written (as P4.9G2) inside `settings-data-system-flow.spec.ts` with a
 * comment claiming it "runs LAST in its OWN spec file-position" — but EIGHT
 * spec files sort after `settings-data-…` (`settings-flow`, `setup-flow`,
 * `terminal-flow`, `wardrobe-flow`, `workbench-flow`, and the three
 * `workspace-*` files), every one of which reads the seeded graph the wipe
 * destroys. P4.9G3 moved the beat here when it landed the server family; do not
 * rename this file to something that sorts earlier.
 *
 * The instance itself is a fresh copy of the committed fixture per run
 * (`global-setup.ts`), so wiping it at the end is safe.
 *
 * Server side: `systemDeleteDataPreview` / `systemDeleteData` over
 * `quilltap-core::services::delete_all`, differential-verified by
 * `system_delete_data_equivalence` (the summary body + a row-count map over
 * every table in all three partitions, diffed against v4's real delete service).
 */

/**
 * LANDED under P4.9G3 (2026-07-24). Kept as the beat's named gate so a future
 * lane that needs to park it has an obvious switch rather than a `.skip`.
 */
const DELETE_ALL_SERVER_LANDED = true;

test.describe('P4.9G3 — Delete All Data (destructive; LAST)', () => {
  test('delete-all preview → confirm → complete', async ({ page }) => {
    test.skip(!DELETE_ALL_SERVER_LANDED, 'delete-all server family not landed');
    await page.goto('/salon');
    // Unlock only if the gate is showing (the shared server usually stays open).
    const passphrase = page.locator('#qt-passphrase');
    const chats = page.getByRole('heading', { name: 'Chats', exact: true });
    await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }

    await page.goto('/settings?tab=system&section=delete-all-data');
    // Scoped + EXACT: the collapsible card header is also a button whose
    // accessible name contains "Delete All Data" ("Delete All Data
    // Permanently"), so the unscoped substring match is a strict-mode violation.
    await page
      .locator('qt-delete-data-card')
      .getByRole('button', { name: 'Delete All Data', exact: true })
      .click();
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
