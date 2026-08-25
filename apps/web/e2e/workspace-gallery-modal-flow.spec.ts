import { expect, test, type Page } from '@playwright/test';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: rides the SHARED global-setup server (unlocked by foundation.spec).
 * "workspace-gallery-modal-flow" sorts after "workspace-flow", so it runs last
 * among the workspace beats and never disturbs the route-mode specs.
 *
 * P4.D117 — v4 bug 99 (`8018c487`), the half no non-browser layer can see.
 *
 * Like `workspace-flow.spec.ts` and UNLIKE every route-mode spec, this file
 * imports the BASE `@playwright/test` so the tabbed-workspace flag stays ON.
 * That is not a preference — it is the bug's precondition. `.qt-workspace`
 * declares `isolation: isolate` (`_workspace.css:34`), which makes it a
 * stacking context; every pane's content renders inside it, so the detail
 * modal's `z-[60]` stopped being comparable with the sticky `.qt-page-toolbar`
 * (`z-30`, `_layout.css:707`) painted by an ancestor context. Nothing is
 * clipped, mispositioned or hidden — the controls lay out exactly where they
 * belong and are simply painted over, and unclickable with it. A DOM assertion
 * passes, a Playwright `toBeVisible()` passes, and jsdom runs no compositing at
 * all: only a real hit test can tell.
 *
 * So the load-bearing assertion here is `document.elementFromPoint()` at the
 * centre of the Download control, which must return that control's own subtree
 * and not the toolbar. Before the portal fix it returned
 * `SPAN › .qt-queue-badge-summary › .qt-page-toolbar` — measured, not assumed.
 *
 * Beats:
 *   1. the tile's hover Download button downloads the picture without opening
 *      the detail view (the download half of bug 99);
 *   2. the detail modal's top-right cluster is HIT-TESTABLE above the toolbar,
 *      and its Close actually closes (the stacking half).
 */

async function openWorkspace(page: Page): Promise<void> {
  await page.goto('/');
  const passphrase = page.locator('#qt-passphrase');
  const workspace = page.locator('.qt-workspace');
  await expect(passphrase.or(workspace).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
  await expect(workspace).toBeVisible({ timeout: 15_000 });
}

/** A 1×1 transparent PNG — the walk seeds its OWN tile rather than trusting a
 *  shared fixture row that sibling specs both add to and delete from. */
const TINY_PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==',
  'base64',
);

/** Aria's id over the live dispatch route (the roster drills through buttons). */
async function ariaId(page: Page): Promise<string> {
  const res = await page.request.post('/api/dispatch', { data: { type: 'characterList' } });
  expect(res.ok(), 'characterList').toBe(true);
  const body = (await res.json()) as {
    data?: { characters?: { id: string; name: string }[] };
    characters?: { id: string; name: string }[];
  };
  const aria = (body.data?.characters ?? body.characters ?? []).find((c) => c.name === 'Aria');
  expect(aria, 'the Aria fixture character').toBeTruthy();
  return aria!.id;
}

/**
 * Open Aria's Photo Gallery tab in the workspace with at least one tile,
 * uploading one through the real multipart route when the album is empty
 * (sibling specs delete tiles, so the walk must not depend on finding one).
 */
async function openGalleryWithATile(page: Page): Promise<void> {
  await openWorkspace(page);
  await page.goto(`/characters/${await ariaId(page)}`);
  await expect(page.getByRole('heading', { name: 'Aria' })).toBeVisible({ timeout: 15_000 });
  await page.getByRole('button', { name: 'Photo Gallery' }).click();

  const tiles = page.locator('button.aspect-square');
  await expect(
    tiles.first().or(page.getByText(/No photos in .*album yet/)),
  ).toBeVisible({ timeout: 15_000 });
  if ((await tiles.count()) === 0) {
    await page
      .locator('input[type="file"][aria-label="Upload photo"]')
      .setInputFiles({ name: 'd117-probe.png', mimeType: 'image/png', buffer: TINY_PNG });
    await expect(tiles.first()).toBeVisible({ timeout: 20_000 });
  }
}

test('a gallery tile downloads its picture without opening the detail view', async ({ page }) => {
  test.setTimeout(90_000);
  await openGalleryWithATile(page);

  const tile = page.locator('button.aspect-square').first();
  await tile.hover();
  const downloadButton = page.getByRole('button', { name: 'Download image' }).first();
  await expect(downloadButton).toBeVisible({ timeout: 10_000 });

  // A REAL browser download, named after the entry's stored file — and the
  // detail view stays shut (v4 EmbeddedPhotoGallery.tsx:85-88).
  const downloadPromise = page.waitForEvent('download', { timeout: 20_000 });
  await downloadButton.click();
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toMatch(/\.(webp|png|jpe?g)$/i);
  await expect(page.locator('qt-image-detail-modal')).toHaveCount(0);
});

test('the detail modal controls are clickable above the page toolbar (bug 99)', async ({
  page,
}) => {
  test.setTimeout(90_000);
  await openGalleryWithATile(page);

  // The toolbar must actually be on screen, or the beat proves nothing.
  const toolbar = page.locator('.qt-page-toolbar').first();
  await expect(toolbar).toBeVisible({ timeout: 10_000 });

  await page.locator('button.aspect-square').first().click();
  const modal = page.locator('qt-image-detail-modal');
  await expect(modal).toHaveCount(1, { timeout: 15_000 });

  const download = modal.getByTitle('Download', { exact: true });
  const close = modal.getByTitle('Close (Escape)');
  await expect(download).toBeVisible();
  await expect(close).toBeVisible();

  // THE assertion. `toBeVisible()` above is satisfied by the buggy state too;
  // only a hit test at the control's own centre separates "laid out where it
  // belongs" from "reachable". Before the portal fix this resolved into
  // `.qt-page-toolbar`.
  for (const [name, control] of [
    ['Download', download],
    ['Close', close],
  ] as const) {
    const box = await control.boundingBox();
    expect(box, `${name} has a box`).toBeTruthy();
    const hit = await page.evaluate(
      ({ x, y }) => {
        const el = document.elementFromPoint(x, y);
        return {
          insideControl: !!el?.closest('qt-image-actions button'),
          chain: (() => {
            const parts: string[] = [];
            for (let n = el; n && parts.length < 4; n = n.parentElement) {
              parts.push(n.tagName.toLowerCase() + (n.className ? `.${String(n.className).split(/\s+/)[0]}` : ''));
            }
            return parts.join(' › ');
          })(),
        };
      },
      { x: box!.x + box!.width / 2, y: box!.y + box!.height / 2 },
    );
    expect(
      hit.insideControl,
      `${name} at (${Math.round(box!.x + box!.width / 2)}, ${Math.round(
        box!.y + box!.height / 2,
      )}) is hit-testable; elementFromPoint saw ${hit.chain}`,
    ).toBe(true);
  }

  // And it is not merely reachable in theory: the real click closes the modal.
  await close.click();
  await expect(modal).toHaveCount(0, { timeout: 10_000 });
});
