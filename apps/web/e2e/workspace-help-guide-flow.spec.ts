import { expect, request as pwRequest, test, type Page } from '@playwright/test';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: rides the SHARED global-setup server, so the filename must sort
 * AFTER foundation.spec.ts (workers: 1, alphabetical file order) — foundation
 * asserts a LOCKED-first server, and every beat here unlocks before its guard,
 * so it must never run before foundation. "workspace-help-guide-flow" ('w')
 * sorts after "foundation" ('f').
 *
 * P4.9I2B — a LIVE browser walk of the Help dialog's **Guide** tab, entirely
 * ACTIVATE-AT-UNIFY. In-lane none of it can run: the four `helpDocs*` dispatch
 * verbs live in the sibling server lane P4.9I2A, and the `<qt-help-entry />`
 * shell mount is UNIFIER-ONLY (§S.2). Each beat is therefore guarded on BOTH
 * (a) the server answering `helpDocsList` and (b) the rail entry being present,
 * and skips LOUDLY otherwise. It self-activates the moment the server verbs
 * merge and the unifier mounts the entry — no further unifier wire needed.
 *
 * Beats:
 *   1. The rail opens the dialog on the Guide tab, listing categories.
 *   2. The category matching the current page is auto-expanded.
 *   3. A prose-only search term surfaces a topic with its snippet line.
 *   4. Opening a topic renders the reader; "Open this page in Quilltap"
 *      navigates and closes the walk out of the Guide.
 */

let helpDocsReady = false;

test.beforeAll(async () => {
  // Probe: is `helpDocsList` handled? In-lane the Rust core has no such
  // variant → "unknown variant" error → not ready. A success envelope (or any
  // DOMAIN error) means the handler exists → ready.
  try {
    const ctx = await pwRequest.newContext();
    const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: { type: 'helpDocsList' } });
    const body = (await res.json().catch(() => null)) as
      | { type?: string; data?: { message?: string } }
      | null;
    await ctx.dispose();
    const isUnknownVariant =
      body?.type === 'error' && /unknown variant/i.test(String(body?.data?.message ?? ''));
    helpDocsReady = body != null && !isUnknownVariant;
  } catch {
    helpDocsReady = false;
  }
});

/** Unlock only when the passphrase screen is showing (the shared server stays unlocked). */
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

/** The footer rail entry (mounted by the unifier, §S.2). */
const railEntry = (page: Page) =>
  page.locator('aside.qt-left-sidebar button[aria-label="Help"]');

/** Skip the beat loudly unless the whole surface is present. */
async function guard(page: Page): Promise<boolean> {
  const entryPresent = (await railEntry(page).count()) > 0;
  test.skip(
    !helpDocsReady || !entryPresent,
    `Help Guide not live in-lane (helpDocsList served: ${helpDocsReady}; rail entry present: ${entryPresent}) — the server verbs (P4.9I2A) + the shell mount (unifier, §S.2) self-activate this beat at unification`,
  );
  return helpDocsReady && entryPresent;
}

/** Open Help and make sure the Guide tab is the active one. */
async function openGuide(page: Page): Promise<void> {
  await railEntry(page).click();
  await expect(page.locator('qt-help-dialog .qt-dialog')).toBeVisible();
  const guideTab = page.locator('qt-help-dialog .qt-tab', { hasText: 'Guide' });
  // The tab persists in sessionStorage, so a prior beat may have left Ask up.
  if ((await guideTab.getAttribute('class'))?.includes('qt-tab-active') !== true) {
    await guideTab.click();
  }
  await expect(page.locator('qt-help-guide-tab')).toBeVisible();
}

test.describe('P4.9I2B — the Help Guide', () => {
  test('the rail opens the Help dialog on the Guide tab, listing categories', async ({ page }) => {
    await openWorkspace(page);
    if (!(await guard(page))) return;

    await openGuide(page);
    // v4's eleven categories; at minimum the two that always carry documents.
    await expect(
      page.locator('.qt-help-guide-category-label', { hasText: 'Getting Started' }),
    ).toBeVisible();
    await expect(
      page.locator('.qt-help-guide-category-label', { hasText: 'Chats (The Salon)' }),
    ).toBeVisible();
    // Every category shows its document count.
    await expect(page.locator('.qt-help-guide-category-badge').first()).toBeVisible();
  });

  test('the category matching the current page is auto-expanded', async ({ page }) => {
    await openWorkspace(page);
    if (!(await guard(page))) return;

    // The workspace resting path is `/workspace`, which matches no
    // URL_CATEGORY_MAP row, so nothing auto-expands there. Route to /aurora
    // first — `getCategoryForUrl('/aurora')` is `characters`.
    await page.goto('/aurora');
    await expect(page.locator('aside.qt-left-sidebar')).toBeVisible({ timeout: 15_000 });
    await openGuide(page);

    const characters = page
      .locator('.qt-help-guide-category', {
        has: page.locator('.qt-help-guide-category-label', { hasText: 'Characters (Aurora)' }),
      })
      .first();
    await expect(characters.locator('.qt-help-guide-category-header')).toHaveAttribute(
      'aria-expanded',
      'true',
    );
    await expect(characters.locator('.qt-help-guide-topic').first()).toBeVisible();
  });

  test('a prose-only search surfaces a topic with its snippet line', async ({ page }) => {
    await openWorkspace(page);
    if (!(await guard(page))) return;
    await openGuide(page);

    // "wardrobe" is prose in several help documents but is not the TITLE of the
    // topics that mention it, so a hit here can only have come from the
    // server-side text search — which is the half this beat is for.
    await page.locator('.qt-help-guide-search-input').fill('wardrobe');
    // Outlast SEARCH_DEBOUNCE_MS (200) plus the round trip.
    const topics = page.locator('.qt-help-guide-topic');
    await expect(topics.first()).toBeVisible({ timeout: 10_000 });
    // A searched list force-expands every surviving category.
    await expect(
      page.locator('.qt-help-guide-category-header[aria-expanded="true"]').first(),
    ).toBeVisible();

    // Clearing the box restores the unfiltered list.
    await page.locator('.qt-help-guide-search-clear').click();
    await expect(page.locator('.qt-help-guide-search-input')).toHaveValue('');
  });

  test('opening a topic renders the reader, and its page link navigates', async ({ page }) => {
    await openWorkspace(page);
    if (!(await guard(page))) return;
    await page.goto('/aurora');
    await expect(page.locator('aside.qt-left-sidebar')).toBeVisible({ timeout: 15_000 });
    await openGuide(page);

    const topic = page.locator('.qt-help-guide-topic').first();
    await expect(topic).toBeVisible();
    await topic.click();

    // The reader replaces the list, with a Back control carrying the category.
    const reader = page.locator('.qt-help-guide-reader');
    await expect(reader).toBeVisible({ timeout: 10_000 });
    await expect(page.locator('.qt-help-guide-back')).toBeVisible();
    await expect(reader.locator('h1, h2').first()).toBeVisible();

    // Every help document opens with the "Open this page in Quilltap" callout,
    // which the pipeline renders as a page-link BUTTON inside a blockquote (the
    // measured v4 shape — v4's own blockquote-callout branch is dead code).
    const pageLink = reader.locator('.qt-help-guide-page-link').first();
    if ((await pageLink.count()) > 0) {
      const label = (await pageLink.textContent())?.trim();
      expect(label && label.length > 0).toBe(true);
      await pageLink.click();
      // Navigation happened: either the workspace opened a tab in place, or the
      // route changed. Both leave the Guide's reader behind.
      await expect(reader).toBeHidden({ timeout: 10_000 });
    }

    // Back returns to the category list.
    if (await page.locator('.qt-help-guide-back').count()) {
      await page.locator('.qt-help-guide-back').click();
      await expect(page.locator('.qt-help-guide-search-input')).toBeVisible();
    }
  });
});
