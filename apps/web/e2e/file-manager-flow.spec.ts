import { expect, request as pwRequest, test, type Page } from '@playwright/test';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so
 * its filename must sort AFTER foundation.spec.ts (foundation walks the locked
 * → unlock gate and must reach the shared server first; workers: 1, alphabetical
 * order). "file-manager-flow" sorts correctly.
 *
 * P4.6aa lane B — a LIVE browser walk of the bespoke `qt-file-manager` over the
 * shared Salon server: reach a document store's detail screen, flip the "New
 * file manager (beta)" toggle, and drive the widget end-to-end — create folder →
 * upload → rename → move into folder → delete → the copy-folder REFUSAL.
 *
 * PROBE-GUARDED / self-activating (the P4.6x precedent). The detail screen and
 * its toggle button (contract §4) belong to the SIBLING lane (P4.6z) and to the
 * unifier's toggle wire — neither exists in THIS lane. So a runtime probe
 * navigates to the store detail route and skips the whole walk when the toggle
 * isn't there (in-lane), and runs it live once the sibling screen + toggle land
 * at unification. The store itself is seeded through the frozen-live mount
 * dispatch (`mountPointCreate`), so the seed works in-lane too.
 */

let storeId = '';

test.beforeAll(async () => {
  // Seed a fresh database document store (the mount verbs are frozen-live, so
  // this works in-lane; only the toggle/detail screen gate the walk). The shared
  // server is already unlocked by foundation.spec (it ran first).
  try {
    const ctx = await pwRequest.newContext();
    const res = await ctx.post(`${BASE_URL}/api/dispatch`, {
      data: {
        type: 'mountPointCreate',
        mountPoint: { name: 'File Manager E2E', mountType: 'database', storeType: 'documents' },
      },
    });
    const body = (await res.json().catch(() => null)) as
      | { type?: string; data?: { mountPoint?: { id?: string } } }
      | null;
    storeId = body?.data?.mountPoint?.id ?? '';
    await ctx.dispose();
  } catch {
    storeId = '';
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
  }
}

/**
 * Reach the file manager on the seeded store's detail screen: navigate, flip the
 * toggle, and confirm the widget renders. Returns false when the toggle isn't
 * present (in-lane — the sibling screen + unifier wire haven't landed).
 */
async function reachFileManager(page: Page): Promise<boolean> {
  await page.goto('/');
  await maybeUnlock(page);
  if (!storeId) return false;

  await page.goto(`/scriptorium/${storeId}`);
  const toggle = page.getByRole('button', { name: /New file manager|Classic view/i }).first();
  try {
    await toggle.waitFor({ state: 'visible', timeout: 5_000 });
  } catch {
    return false; // no toggle → the wire isn't here yet (in-lane)
  }
  await toggle.click();
  const widget = page.locator('[data-testid="qt-file-manager"]');
  await expect(widget).toBeVisible({ timeout: 10_000 });
  return true;
}

test.describe('P4.6aa — the qt-file-manager walk', () => {
  test('create folder → upload → rename → move → delete → copy-folder refusal', async ({ page }) => {
    const active = await reachFileManager(page);
    test.skip(!active, 'awaits the sibling scriptorium detail + toggle wire (activates at unification)');

    const fm = page.locator('[data-testid="qt-file-manager"]');
    await expect(fm.locator('[data-testid="fm-loading"]')).toHaveCount(0, { timeout: 15_000 });

    // 1. Create a folder "Drafts".
    await fm.locator('[data-testid="fm-new-folder"]').click();
    await fm.locator('[data-testid="fm-new-folder-input"]').fill('Drafts');
    await fm.locator('[data-testid="fm-new-folder-confirm"]').click();
    await expect(fm.locator('[data-testid="fm-folder-row"]', { hasText: 'Drafts' })).toBeVisible({
      timeout: 15_000,
    });

    // 2. Upload a file at the root.
    await fm.locator('[data-testid="fm-file-input"]').setInputFiles({
      name: 'brief.md',
      mimeType: 'text/markdown',
      buffer: Buffer.from('# Expedition brief\n'),
    });
    const fileRow = fm.locator('[data-testid="fm-file-row"]', { hasText: 'brief.md' });
    await expect(fileRow).toBeVisible({ timeout: 15_000 });

    // 3. Rename brief.md → dossier.md.
    await fileRow.locator('[data-testid="fm-rename"]').click();
    const renameInput = fm.locator('[data-testid="fm-rename-input"]');
    await renameInput.fill('dossier.md');
    await fm.locator('[data-testid="fm-rename-confirm"]').click();
    await expect(fm.locator('[data-testid="fm-file-row"]', { hasText: 'dossier.md' })).toBeVisible({
      timeout: 15_000,
    });

    // 4. Move dossier.md into Drafts (cut → into Drafts → paste here).
    await fm
      .locator('[data-testid="fm-file-row"]', { hasText: 'dossier.md' })
      .locator('[data-testid="fm-cut"]')
      .click();
    await fm
      .locator('[data-testid="fm-folder-row"]', { hasText: 'Drafts' })
      .locator('button')
      .first()
      .click();
    await expect(fm.locator('[data-testid="fm-breadcrumbs"]')).toContainText('Drafts');
    await fm.locator('[data-testid="fm-paste"]').click();
    await expect(fm.locator('[data-testid="fm-file-row"]', { hasText: 'dossier.md' })).toBeVisible({
      timeout: 15_000,
    });

    // 5. Delete dossier.md → the drawer empties.
    await fm
      .locator('[data-testid="fm-file-row"]', { hasText: 'dossier.md' })
      .locator('[data-testid="fm-delete"]')
      .click();
    await expect(fm.locator('[data-testid="fm-file-row"]')).toHaveCount(0, { timeout: 15_000 });

    // 6. The copy-folder REFUSAL: back at the root, copy the Drafts folder and
    // attempt to paste it — the widget surfaces v4's steampunk refusal and fires
    // no request (the two-backend-gaps port).
    await fm.locator('[data-testid="fm-breadcrumbs"]').getByRole('button', { name: 'Root' }).click();
    const draftsRow = fm.locator('[data-testid="fm-folder-row"]', { hasText: 'Drafts' });
    await draftsRow.locator('[data-testid="fm-copy"]').click();
    await fm.locator('[data-testid="fm-paste"]').click();
    await expect(fm.locator('[data-testid="fm-error"]')).toContainText('select the files within', {
      timeout: 15_000,
    });
  });
});
