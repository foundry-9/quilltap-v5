import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { expect, test, type Page } from './support/fixtures';

import { ARTIFACTS_DIR, E2E_PASSPHRASE } from './support/env';

/** A fresh directory under the e2e artifacts area (readable by the server). */
function makeFsStoreDir(tag: string): string {
  const dir = join(ARTIFACTS_DIR, `fs-store-${tag}-${Date.now()}`);
  mkdirSync(dir, { recursive: true });
  return dir;
}

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so
 * its filename must sort AFTER foundation.spec.ts (workers: 1, alphabetical
 * file order). `scriptorium-flow` does.
 *
 * P4.6z — a LIVE browser walk of the Scriptorium over the shared Salon server,
 * over the frozen P4.6v/P4.6y mount-file dispatch surface (fully live — no
 * capability probe). Three beats:
 *  1. a DATABASE store end-to-end: create → open detail → upload a text file
 *     (multipart) → the row appears → expand → edit the description → delete the
 *     file → back → delete the store.
 *  2. a FILESYSTEM store: point it at a temp dir holding a markdown file, scan,
 *     and see the file indexed.
 *  3. the convert refusal: converting the fs store fires the live-guarded verb;
 *     the server answers the typed refusal, surfaced loudly in the flash banner.
 *
 * Each beat deletes the store it creates so the shared mount-index is left clean.
 */

/**
 * Navigate to the Scriptorium list, unlocking the shared server if it's still
 * locked. The shared server stays unlocked across specs, so on a warm server a
 * direct `/scriptorium` visit lands straight on the page (no passphrase, no
 * Chats home) — key readiness on the operational nav landmark, not "Chats".
 */
async function gotoScriptorium(page: Page): Promise<void> {
  await page.goto('/scriptorium');
  const passphrase = page.locator('#qt-passphrase');
  const nav = page.getByRole('navigation', { name: 'Quick navigation' });
  await expect(passphrase.or(nav).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
    await expect(nav).toBeVisible({ timeout: 15_000 });
    await page.goto('/scriptorium');
  }
  await expect(page.getByRole('heading', { name: 'The Scriptorium' })).toBeVisible({
    timeout: 15_000,
  });
}

/** Delete a store from the list by name (accepts nothing — the confirm is a dialog). */
async function deleteStoreFromList(page: Page, name: string): Promise<void> {
  const card = page.locator('qt-store-card', { hasText: name });
  await card.getByRole('button', { name: 'Delete document store' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: 'Delete', exact: true }).click();
  await expect(page.locator('qt-store-card', { hasText: name })).toHaveCount(0, {
    timeout: 15_000,
  });
}

test.describe('P4.6z — the Scriptorium (stores + FileTable)', () => {
  test('database store: create → upload → expand → describe → delete file → delete store', async ({
    page,
  }) => {
    // window.confirm on the blob delete → auto-accept.
    page.on('dialog', (d) => void d.accept());

    const storeName = `E2E DB Store ${Date.now()}`;
    await gotoScriptorium(page);

    // Create a database-backed store (no filesystem path needed).
    await page.getByRole('button', { name: /Add.*Document Store/ }).first().click();
    const createDialog = page.getByRole('dialog');
    await expect(createDialog).toBeVisible();
    await createDialog.getByLabel('Name').fill(storeName);
    await createDialog.getByText('Database-backed — everything', { exact: false }).click();
    await createDialog.getByRole('button', { name: 'Add Store' }).click();
    await expect(createDialog).toBeHidden({ timeout: 15_000 });

    // The card lands in the grid; open its detail.
    const card = page.locator('qt-store-card', { hasText: storeName });
    await expect(card).toBeVisible({ timeout: 15_000 });
    await card.getByRole('heading', { name: storeName }).click();
    await expect(page.getByRole('heading', { name: storeName })).toBeVisible({ timeout: 15_000 });
    await expect(page.getByRole('heading', { name: 'Indexed Files' })).toBeVisible();

    // Upload a small BINARY file (a generic blob — so the row is blob-backed and
    // exposes the description editor + delete) via the multipart PUT leg.
    const fileInput = page.locator('input[type="file"]');
    await expect(fileInput).toHaveCount(1);
    await fileInput.setInputFiles({
      name: 'artifact.bin',
      mimeType: 'application/octet-stream',
      buffer: Buffer.from([0x51, 0x54, 0x00, 0x01, 0x02, 0x03, 0xff, 0xfe]),
    });

    // The row appears in the table.
    const row = page.locator('table tr', { hasText: 'artifact.bin' });
    await expect(row.first()).toBeVisible({ timeout: 15_000 });

    // Expand the row → the lazy blob detail loads → edit the description → Save.
    await row.first().click();
    const desc = page.locator('qt-file-detail-row textarea');
    await expect(desc).toBeVisible({ timeout: 15_000 });
    await desc.fill('A curious brass artifact.');
    await page.locator('qt-file-detail-row').getByRole('button', { name: 'Save' }).click();

    // Delete the file (database mount → the per-row Delete button; confirm auto-accepts).
    await page.locator('qt-file-detail-row').getByRole('button', { name: 'Delete' }).click();
    await expect(page.locator('table tr', { hasText: 'artifact.bin' })).toHaveCount(0, {
      timeout: 15_000,
    });

    // Back to the list, then delete the store.
    await page.getByRole('button', { name: 'Back to The Scriptorium' }).click();
    await expect(page.getByRole('heading', { name: 'The Scriptorium' })).toBeVisible();
    await deleteStoreFromList(page, storeName);
  });

  test('database store: an image blob is transcoded to WebP on upload (ACTIVATE-AT-UNIFY, P4.6bf S1)', async ({
    page,
  }) => {
    // window.confirm on the blob delete → auto-accept.
    page.on('dialog', (d) => void d.accept());

    const storeName = `E2E WebP Store ${Date.now()}`;
    await gotoScriptorium(page);

    await page.getByRole('button', { name: /Add.*Document Store/ }).first().click();
    const createDialog = page.getByRole('dialog');
    await expect(createDialog).toBeVisible();
    await createDialog.getByLabel('Name').fill(storeName);
    await createDialog.getByText('Database-backed — everything', { exact: false }).click();
    await createDialog.getByRole('button', { name: 'Add Store' }).click();
    await expect(createDialog).toBeHidden({ timeout: 15_000 });

    const card = page.locator('qt-store-card', { hasText: storeName });
    await expect(card).toBeVisible({ timeout: 15_000 });
    await card.getByRole('heading', { name: storeName }).click();
    await expect(page.getByRole('heading', { name: 'Indexed Files' })).toBeVisible();

    // Upload a real (1×1) PNG image. v4's blob-upload pipeline transcodes bitmap
    // images to WebP (`transcodeToWebP`) and rewrites the stored relative path's
    // extension to `.webp`. The DISPATCH layer only does this once BG's
    // `store_mount_file` gains the `webp` param AND the unifier wires
    // `EngineAssembly.blob_webp`; until then the inline `RefusingWebpTranscoder`
    // takes v4's store-original fallback arm (the `.png` stays `.png`).
    const PNG_1X1 =
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';
    const fileInput = page.locator('input[type="file"]');
    await expect(fileInput).toHaveCount(1);
    await fileInput.setInputFiles({
      name: 'portrait.png',
      mimeType: 'image/png',
      buffer: Buffer.from(PNG_1X1, 'base64'),
    });

    // The row lands under EITHER name depending on whether the wire is live.
    const webpRow = page.locator('table tr', { hasText: 'portrait.webp' });
    const pngRow = page.locator('table tr', { hasText: 'portrait.png' });
    await expect(webpRow.or(pngRow).first()).toBeVisible({ timeout: 15_000 });
    const webpCount = await webpRow.count();
    const pngCount = await pngRow.count();
    const transcoded = webpCount > 0;

    // Always clean up (a skipped beat must leave the shared mount-index clean).
    const rowText = transcoded ? 'portrait.webp' : 'portrait.png';
    await page.locator('table tr', { hasText: rowText }).first().click();
    await page.locator('qt-file-detail-row').getByRole('button', { name: 'Delete' }).click();
    await expect(page.locator('table tr', { hasText: rowText })).toHaveCount(0, {
      timeout: 15_000,
    });
    await page.getByRole('button', { name: 'Back to The Scriptorium' }).click();
    await expect(page.getByRole('heading', { name: 'The Scriptorium' })).toBeVisible();
    await deleteStoreFromList(page, storeName);

    test.skip(
      !transcoded,
      "ACTIVATE-AT-UNIFY (P4.6bf S1): the dispatch-layer blob-WebP wire needs BG's store_mount_file webp param + the engine.rs blob_webp wire; until then image blobs store original bytes (portrait.png). Self-activates at unification.",
    );

    // Wired: the image blob was transcoded and its stored path rewritten to
    // `.webp` (v4 `normaliseBlobRelativePath`).
    expect(webpCount).toBe(1);
    expect(pngCount).toBe(0);
  });

  test('filesystem store: create on a temp dir, scan, and see the file indexed', async ({
    page,
  }) => {
    // A temp directory with one markdown file the default include patterns match.
    const dir = makeFsStoreDir('fs');
    writeFileSync(join(dir, 'chronicle.md'), '# The Chronicle\n\nAn account of events.\n');

    const storeName = `E2E FS Store ${Date.now()}`;
    await gotoScriptorium(page);

    await page.getByRole('button', { name: /Add.*Document Store/ }).first().click();
    const createDialog = page.getByRole('dialog');
    await expect(createDialog).toBeVisible();
    await createDialog.getByLabel('Name').fill(storeName);
    // Filesystem is the default mount type — type the path into the picker input.
    await createDialog.locator('qt-directory-picker input').fill(dir);
    await createDialog.getByRole('button', { name: 'Add Store' }).click();
    await expect(createDialog).toBeHidden({ timeout: 15_000 });

    // Open the store, run a scan, and see the markdown file indexed.
    const card = page.locator('qt-store-card', { hasText: storeName });
    await expect(card).toBeVisible({ timeout: 15_000 });
    await card.getByRole('heading', { name: storeName }).click();
    await expect(page.getByRole('heading', { name: storeName })).toBeVisible({ timeout: 15_000 });

    await page.getByRole('button', { name: 'Scan Now' }).click();
    const row = page.locator('table tr', { hasText: 'chronicle.md' });
    await expect(row.first()).toBeVisible({ timeout: 20_000 });

    await page.getByRole('button', { name: 'Back to The Scriptorium' }).click();
    await expect(page.getByRole('heading', { name: 'The Scriptorium' })).toBeVisible();
    await deleteStoreFromList(page, storeName);
  });

  test('convert fires the live-guarded verb and surfaces the typed refusal loudly', async ({
    page,
  }) => {
    const dir = makeFsStoreDir('conv');
    writeFileSync(join(dir, 'note.md'), '# Note\n');
    const storeName = `E2E Convert Store ${Date.now()}`;
    await gotoScriptorium(page);

    await page.getByRole('button', { name: /Add.*Document Store/ }).first().click();
    const createDialog = page.getByRole('dialog');
    await expect(createDialog).toBeVisible();
    await createDialog.getByLabel('Name').fill(storeName);
    await createDialog.locator('qt-directory-picker input').fill(dir);
    await createDialog.getByRole('button', { name: 'Add Store' }).click();
    await expect(createDialog).toBeHidden({ timeout: 15_000 });

    // Convert the filesystem store → confirm → the server refuses (P4.6y), and
    // the error surfaces as the toast v4 raises (P4.25 retired the flash banner).
    const card = page.locator('qt-store-card', { hasText: storeName });
    await expect(card).toBeVisible({ timeout: 15_000 });
    await card.getByRole('button', { name: 'Convert', exact: false }).click();
    const convertDialog = page.getByRole('dialog');
    await expect(convertDialog).toBeVisible();
    await convertDialog.getByRole('button', { name: 'Convert', exact: true }).click();

    // The typed refusal lands in the error toast — a loud, visible notice.
    await expect(page.locator('[role="toast-container"] > .qt-toast-error')).toBeVisible({
      timeout: 15_000,
    });

    await deleteStoreFromList(page, storeName);
  });
});
