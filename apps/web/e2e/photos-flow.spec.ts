import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';
import { PHOTOS_E2E_CAPTION_A, PHOTOS_E2E_CAPTION_B } from './support/seed-photos-fixture';

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so
 * its filename must sort AFTER foundation.spec.ts (workers: 1, alphabetical
 * file order) — "photos-flow" ('p') sorts after "foundation" ('f').
 *
 * P4.9a — a LIVE browser walk of the My Photos vertical:
 *   1. `/photos` renders the gallery over the real `photoGalleryList` verb,
 *      showing the two entries global-setup seeded for this walk.
 *   2. Clicking a card opens the detail modal with its linker list, prompt,
 *      and identity block; Escape closes it.
 *   3. The modal's Download button (v4 `af1bc479`) produces a REAL browser
 *      download named after the stored file, and a photo with no bytes behind
 *      it takes v4's failure arm instead.
 *   4. A link-only delete round-trips through `photoGalleryEntryRemove` and a
 *      RELOAD proves it persisted server-side (not just optimistic UI).
 *
 * ⚠ NAVIGATION: the walk reaches `/photos` by URL, because the shell's photos
 * nav item stays `route: null` until the §2a unifier flip. At unification this
 * beat gains a nav-CLICK step — see the marked place below.
 *
 * The walk asserts only on its OWN seeded rows (searched by their distinctive
 * captions) and never on absolute counts: the instance is shared, and sibling
 * specs both add and delete photos-folder links.
 *
 * NOT walked here, and deliberately: the SEARCH box's semantic query. v4's
 * service calls `generateEmbeddingForUser` on the query branch, and the e2e
 * instance has no default embedding profile — so a search is the seam's loud
 * refusal on this server, not a narrowed list. The refusal itself is pinned by
 * `photos_web_routes`; making the walk depend on a live embedding provider
 * would be testing the provider, not this screen.
 */

/** Unlock only when the passphrase screen is showing (the server stays unlocked). */
async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const shell = page.locator('.qt-left-sidebar');
  await expect(passphrase.or(shell).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
    await expect(shell.first()).toBeVisible({ timeout: 15_000 });
  }
}

async function gotoPhotos(page: Page): Promise<void> {
  // The deep-link entrance — beat 1 additionally walks the §2a nav click.
  await page.goto('/photos');
  await maybeUnlock(page);
  await expect(page.getByRole('heading', { name: 'My Photos' })).toBeVisible({
    timeout: 15_000,
  });
}

/** §2a (ACTIVATED at the round's unification): the shell nav entry is live. */
async function gotoPhotosViaNav(page: Page): Promise<void> {
  await page.goto('/');
  await maybeUnlock(page);
  await page.getByRole('link', { name: 'My Photos' }).click();
  await expect(page.getByRole('heading', { name: 'My Photos' })).toBeVisible({
    timeout: 15_000,
  });
}

test.describe('P4.9a — My Photos', () => {
  test('the gallery lists the seeded entries over the live list verb', async ({ page }) => {
    await gotoPhotosViaNav(page);

    // The two rows global-setup seeded for this walk are both on the wall.
    await expect(page.getByText(PHOTOS_E2E_CAPTION_A)).toBeVisible({ timeout: 15_000 });
    await expect(page.getByText(PHOTOS_E2E_CAPTION_B)).toBeVisible();

    // The counter reflects a real total (never a fixed number — the instance is
    // shared), and the empty state is NOT showing.
    await expect(page.getByText(/\d+ photos?$/)).toBeVisible();
    await expect(page.getByText('Nothing pinned to the wall yet')).toHaveCount(0);
  });

  test('a card opens the detail modal, which Escape closes', async ({ page }) => {
    await gotoPhotos(page);

    await page.getByRole('button', { name: PHOTOS_E2E_CAPTION_A }).click();

    const modal = page.getByRole('dialog');
    await expect(modal).toBeVisible();
    await expect(modal.getByRole('heading', { name: PHOTOS_E2E_CAPTION_A })).toBeVisible();
    // The seeded markdown's `## Original prompt` section reached the excerpt.
    await expect(modal.getByText(/zeppelin drifting above the ironworks/i)).toBeVisible();
    // The linker list rendered with its store badge, and the identity block.
    await expect(modal.getByText(/Linked in \d+ place/)).toBeVisible();
    // No `^` anchor: Playwright normalizes whitespace for STRING matches but
    // not for regex ones, and the identity line sits inside an indented <p>.
    await expect(modal.getByText(/sha256: 9a11/)).toBeVisible();
    // The read-only tags from the frontmatter.
    await expect(modal.getByText('zeppelin', { exact: true })).toBeVisible();

    await page.keyboard.press('Escape');
    await expect(modal).toHaveCount(0);
  });

  test('the detail modal downloads the picture on display (v4 af1bc479)', async ({ page }) => {
    await gotoPhotos(page);

    // Entry A is the one global-setup gave real bytes on the blobs route.
    await page.getByRole('button', { name: PHOTOS_E2E_CAPTION_A }).click();
    const modal = page.getByRole('dialog');
    await expect(modal).toBeVisible();

    // v4's footer row, in order. Scoped to the FOOTER, because the modal
    // header's `×` carries aria-label="Close" too and an unscoped Close
    // locator is a strict-mode violation.
    const footer = modal.locator('footer');
    // Regexes, not strings: the buttons carry template whitespace around their
    // labels. The exact bytes are pinned at the unit tier (photos.spec.ts);
    // what this beat owns is that all four are present, in v4's order.
    await expect(footer.getByRole('button')).toHaveText([
      /Remove from this album/,
      /Download/,
      /Copy/,
      /Close/,
    ]);

    // The click produces a REAL browser download, named after the stored file.
    const downloadPromise = page.waitForEvent('download', { timeout: 15_000 });
    await footer.getByRole('button', { name: 'Download' }).click();
    const download = await downloadPromise;
    expect(download.suggestedFilename()).toBe('e2e-zeppelin.webp');

    await page.keyboard.press('Escape');
    await expect(modal).toHaveCount(0);
  });

  test('a photo with no bytes behind it toasts v4’s failure sentence', async ({ page }) => {
    await gotoPhotos(page);

    // Entry B deliberately has NO blob row, so the blobs endpoint 404s and the
    // page takes v4's catch arm rather than downloading a 404 body as a file.
    await page.getByRole('button', { name: PHOTOS_E2E_CAPTION_B }).click();
    const modal = page.getByRole('dialog');
    await expect(modal).toBeVisible();
    await modal.locator('footer').getByRole('button', { name: 'Download' }).click();

    await expect(page.getByText('Failed to download photo')).toBeVisible({ timeout: 15_000 });
    // The busy latch released — the label is back to "Download", not "Downloading…".
    await expect(
      modal.locator('footer').getByRole('button', { name: 'Download', exact: true }),
    ).toBeVisible();

    await page.keyboard.press('Escape');
    await expect(modal).toHaveCount(0);
  });

  test('a link-only delete round-trips and survives a reload', async ({ page }) => {
    await gotoPhotos(page);

    // v4 gates the removal behind a confirm; accept it.
    page.on('dialog', (dialog) => void dialog.accept());

    await page.getByRole('button', { name: PHOTOS_E2E_CAPTION_B }).click();
    const modal = page.getByRole('dialog');
    await expect(modal).toBeVisible();
    await modal.getByRole('button', { name: 'Remove from this album' }).click();

    // Optimistic removal closes the modal and drops the card.
    await expect(modal).toHaveCount(0);
    await expect(page.getByText(PHOTOS_E2E_CAPTION_B)).toHaveCount(0);

    // The RELOAD is the point: it proves the server actually removed the link,
    // rather than the UI having merely forgotten it.
    await page.reload();
    await maybeUnlock(page);
    await expect(page.getByRole('heading', { name: 'My Photos' })).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByText(PHOTOS_E2E_CAPTION_A)).toBeVisible();
    await expect(page.getByText(PHOTOS_E2E_CAPTION_B)).toHaveCount(0);
  });
});
