import { resolve } from 'node:path';

import { expect, test } from './support/fixtures';

import { E2E_PASSPHRASE, FIXTURES_DIR } from './support/env';

/**
 * P4.9G5 — the owed Backup & Restore walk: upload → preview → restore.
 *
 * Three consecutive lanes deferred this beat, because it needs a full Playwright
 * run and none of them touched `apps/web`. P4.9E2B does, so it carries it.
 *
 * ⚠ **DESTRUCTIVE, AND IT MUST RUN LAST OF ALL.** A restore rewrites the shared
 * instance. `zz-delete-all-destructive.spec.ts` already wipes it, and Playwright
 * orders spec files by path with `workers: 1` and `fullyParallel: false` — so
 * `zzz-` is load-bearing exactly as `zz-` is over there ('zz-' sorts before
 * 'zzz-'). Do not rename this file to anything that sorts earlier, and do not
 * add a spec file that sorts after it expecting the seeded graph: by the time
 * this finishes, the instance holds a restored backup and nothing else.
 *
 * Restoring onto the just-wiped instance is the point rather than an accident:
 * an empty library is the honest starting state for a restore, and it is the
 * case P4.9G5's third ruled divergence is about (v4 restores files BEFORE the
 * places files live, so on a fresh or wiped target every file fails; v5 runs the
 * files phase after the doc-store family).
 *
 * The archive is one of the committed `restore-archives/` fixtures — built by
 * v4's REAL `createBackup` and read byte-for-byte by both sides in
 * `system_restore_state`, so this beat never depends on v5's own zip writer.
 * Read-only; the file belongs to the Rust harness.
 *
 * Mode: **Import as New Data** (the wire's `new-account`), the dialog's default.
 * That is the mode that runs P4.9G6's UUID remap, i.e. the one where the most
 * server work has to be right.
 */

const ARCHIVE = resolve(FIXTURES_DIR, 'restore-archives/restore-archive.zip');

test.describe('P4.9G5 — Restore from Backup (destructive; runs after the wipe)', () => {
  test('upload → preview → restore', async ({ page }) => {
    // A whole restore is well past the 30s default.
    test.setTimeout(120_000);

    await page.goto('/salon');
    const passphrase = page.locator('#qt-passphrase');
    const chats = page.getByRole('heading', { name: 'Chats', exact: true });
    await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }

    await page.goto('/settings?tab=system&section=backup-restore');
    await page
      .locator('qt-backup-restore-card')
      .getByRole('button', { name: 'Restore from Backup' })
      .click();

    const dialog = page.getByRole('dialog');
    await expect(dialog.getByText('Restore Backup')).toBeVisible({ timeout: 15_000 });
    await expect(dialog.getByText('Step 1 of 4')).toBeVisible();

    // Step 1 only STAGES the file — the upload does not start until Next.
    await dialog.locator('input[type="file"]').setInputFiles(ARCHIVE);
    await expect(dialog.getByText('Selected: restore-archive.zip')).toBeVisible();

    // Step 2: the octet-stream upload leg, then the preview verb. The preview
    // reads the archive and writes NOTHING (proven in Rust by
    // `restore_preview_writes_nothing`); here it just has to answer.
    await dialog.getByRole('button', { name: 'Next' }).click();
    await expect(dialog.getByText('Step 2 of 4')).toBeVisible({ timeout: 60_000 });
    const cards = dialog.locator('.qt-heading-2');
    await expect(cards.first()).toBeVisible({ timeout: 30_000 });
    // v4's five preview tiles.
    await expect(dialog.getByText('Characters', { exact: true })).toBeVisible();
    await expect(dialog.getByText('Messages', { exact: true })).toBeVisible();

    // Step 3: the mode. "Import as New Data" is the default, and is the wire's
    // `new-account` — the mode that rewrites every id as it restores.
    await dialog.getByRole('button', { name: 'Next' }).click();
    await expect(dialog.getByText('Step 3 of 4')).toBeVisible();
    await expect(dialog.getByText('Import as New Data')).toBeVisible();

    // Step 4: run it.
    await dialog.getByRole('button', { name: 'Start Restore' }).click();
    await expect(dialog.getByText('Backup restored successfully!')).toBeVisible({
      timeout: 90_000,
    });
    // The summary tiles the server actually filled in.
    await expect(dialog.getByText('API Keys', { exact: true })).toBeVisible();

    // Closing reloads the app (v4 `:66` — a restore replaced everything), which
    // is also the proof the restored instance still serves. Scope to the footer:
    // the modal's header ✕ carries `aria-label="Close"` too, so an unscoped
    // by-role lookup is a strict-mode violation (the banked P4.9G4 gesture).
    await dialog.locator('[qt-modal-footer]').getByRole('button', { name: 'Close' }).click();
    await expect(page.locator('qt-app, router-outlet, body').first()).toBeVisible({
      timeout: 30_000,
    });
  });
});
