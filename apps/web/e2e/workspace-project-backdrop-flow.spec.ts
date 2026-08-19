import { expect, test, type Locator, type Page } from '@playwright/test';

import { E2E_PASSPHRASE } from './support/env';

/**
 * P4.D92 — v4 bug 80 (`c6ff8051`): a project's story background reaches the
 * workspace backdrop.
 *
 * ORDERING: rides the SHARED global-setup server, and sorts between
 * `workspace-flow` and `workspace-tab-refresh`. Like them it imports the BASE
 * `@playwright/test` — NOT `./support/fixtures` — so the workspace flag stays ON
 * (the default) and the arbitrated backdrop is real. Inside `.qt-workspace` the
 * per-view `.qt-page-container::before` layer is suppressed
 * (`_workspace.css:108`, v5 carrying v4's exact suppression and so its exact
 * defect site), which is why the reporter is the ONLY route to the screen.
 *
 * **The mode is set through the UI, not seeded in SQL.** `backgroundDisplayMode`
 * is a document-store OVERLAY property (`db/projects.rs` `property_keys()`): the
 * `projects` column is the slim table and is shadowed by the project's
 * `properties.json`, so a `runCliWrite` UPDATE in global-setup is invisible to
 * every reader. The beat's first live run proved that the hard way. Driving the
 * select instead exercises the real write path and leaves the shared instance in
 * its shipped state (`theme`), which is where the walk ends anyway.
 *
 * The fixture supplies the other half already: the project "Skyhaven" owns the
 * chat "Solo Voyage", and global-setup seeds that chat's `storyBackgroundImageId`
 * (`bg-e2e-file`) plus the PNG bytes — so the resolver's `latest_chat` arm has a
 * real answer to find.
 *
 * Both arms of the fix are walked:
 *  1. "Latest chat background" — the backdrop paints the chat's image, reached by
 *     a DEEP LINK straight onto the project, which is precisely the case v4's two
 *     racing reporters got wrong.
 *  2. "Theme" — v4 falls back to the Prospero subsystem image; **v5 has no
 *     subsystem-background machinery at all** (the standing deferred-loud
 *     divergence), so the honest v5 shape is NO backdrop, asserted as such.
 *
 * A mode change reaches the backdrop on the next FETCH, not instantly: v4 never
 * invalidates its project-background query key and v5 carries that (see
 * `project-detail.ts`). The beat therefore re-enters the project after each
 * write, which is also how a user sees it.
 */

const SKYHAVEN_ID = '70000002-0000-4000-8000-000000000001';
const BG_FILE_ID = 'bg-e2e-file';

/** Deep-link onto the project; the redirect guard lands us in the workspace. */
async function openProjectTab(page: Page): Promise<void> {
  await page.goto(`/prospero/${SKYHAVEN_ID}`);
  const passphrase = page.locator('#qt-passphrase');
  const workspace = page.locator('.qt-workspace');
  await expect(passphrase.or(workspace).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
  await expect(workspace).toBeVisible({ timeout: 15_000 });
  await expect(page.getByRole('heading', { name: 'Skyhaven' })).toBeVisible({ timeout: 15_000 });
}

/** The Story Backgrounds select on the (collapsible) Image Generation card. */
async function backgroundModeSelect(page: Page): Promise<Locator> {
  const card = page.locator('qt-project-image-generation-card');
  await expect(card).toBeVisible({ timeout: 10_000 });
  const select = card.getByLabel('Story Backgrounds');
  if (!(await select.isVisible().catch(() => false))) {
    await card.getByRole('button', { name: /Image Generation/ }).click();
  }
  await expect(select).toBeEnabled({ timeout: 10_000 });
  return select;
}

/** A predicate matching one dispatch verb, for `waitForRequest`. */
const dispatchOf = (verb: string) => (req: { url(): string; postData(): string | null }) =>
  req.url().endsWith('/api/dispatch') && (req.postData() ?? '').includes(`"${verb}"`);

/** Set the display mode and AWAIT the write it triggers (never a timeout). */
async function setBackgroundMode(page: Page, value: string): Promise<void> {
  const saved = page.waitForRequest(dispatchOf('projectUpdate'), { timeout: 15_000 });
  await (await backgroundModeSelect(page)).selectOption(value);
  await saved;
  await expect(await backgroundModeSelect(page)).toHaveValue(value);
}

test('a deep-linked project paints its latest-chat background on the workspace backdrop', async ({
  page,
}) => {
  test.setTimeout(120_000);
  await openProjectTab(page);

  // ARM 1 — "Latest chat background": the reporter reaches the one arbitrated
  // backdrop, resolved through the chat the project owns.
  await setBackgroundMode(page, 'latest_chat');
  await openProjectTab(page);

  const layer = page.locator('.qt-workspace-backdrop-layer');
  await expect(layer).toHaveCount(1, { timeout: 15_000 });
  await expect(async () => {
    const image = await layer.evaluate((el) => getComputedStyle(el).backgroundImage);
    expect(image).toContain(`/api/v1/files/${BG_FILE_ID}`);
  }).toPass({ timeout: 10_000 });

  // The per-view layer really is suppressed in here — arm 1 proves the REPORTER,
  // not a surviving `::before` (v4 bug 80's whole point).
  const beforeDisplay = await page
    .locator('.qt-workspace .qt-page-container')
    .first()
    .evaluate((el) => getComputedStyle(el, '::before').display);
  expect(beforeDisplay).toBe('none');

  // ARM 2 — "Theme". v5 reports nothing (no subsystem fallback exists), and
  // nothing else in this workspace reports, so the backdrop is absent entirely.
  // This also leaves the shared instance in the mode the fixture ships.
  await setBackgroundMode(page, 'theme');
  await openProjectTab(page);
  await expect(page.locator('.qt-workspace-backdrop-layer')).toHaveCount(0, { timeout: 10_000 });
});
