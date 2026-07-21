import { expect, test, type Page } from '@playwright/test';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: rides the SHARED global-setup server (unlocked by foundation.spec).
 * "workspace-document-standalone" sorts LAST (after `workspace-flow`), so it
 * never disturbs the route-mode specs that run before it.
 *
 * P4.9J4 — the chat-less (standalone) Document Mode tab, flag ON. Like
 * `workspace-flow.spec`, this imports the BASE `@playwright/test` (NOT
 * `./support/fixtures`) so the workspace-tabs flag stays ON (the default) and
 * the real two-pane shell is exercised.
 *
 * The seven standalone document verbs already live on the shared server (P4.6w),
 * so the guard here is the SPA WIRING, not the backend: pre-unification the
 * `document-standalone` tab renders the loud not-wired pane and the rail
 * "Document Mode" entry is not mounted. These beats therefore self-activate at
 * the p4.9j unification (the registry swap + the shell mount), exactly like the
 * `home-flow` / `workbench-flow` ACTIVATE-AT-UNIFY probes:
 *
 *   1. (always) the tab kind round-trips through the layout: a
 *      `?open=document-standalone` intent mints a tab — the not-wired pane
 *      pre-unify, the live editor after. Post-P4.6bg the GENERAL scope is live
 *      (the `files_dir` engine wire + the operator fs branches), so a general
 *      open creates a blank on `<files>/_general` and round-trips
 *      create → edit → flush-on-blur autosave → reload.
 *   2. (unify) rail → picker → New blank → edit → flush-on-blur autosave →
 *      rename (title + tab label) → delete closes the tab.
 *   3. (unify) reopen-focuses-same-tab: a renamed file reopened from the picker
 *      recents focuses its existing tab (docKey identity) rather than duplicating.
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

const railDocButton = (page: Page) =>
  page.locator('aside.qt-left-sidebar button[aria-label="Document Mode"]');
const notWired = (page: Page) =>
  page.locator('[data-not-wired][data-kind="document-standalone"]');
const tabLabel = (page: Page, text: string) =>
  page.locator('.qt-tab-strip .qt-tab-label', { hasText: text });

/** True once the unifier has mounted the rail entry + swapped the registry. */
async function isWired(page: Page): Promise<boolean> {
  return (await railDocButton(page).count()) > 0;
}

/** Open the standalone picker from the rail and return the modal locator. */
async function openStandalonePicker(page: Page) {
  await railDocButton(page).click();
  const picker = page.getByRole('dialog');
  await expect(picker).toBeVisible({ timeout: 15_000 });
  return picker;
}

/**
 * Create a blank document inside a DATABASE-BACKED store ("New document here"
 * in the store's browse view). (The general-scope "New blank document" is now
 * live too — P4.6bg wired `files_dir` + ported the operator fs branches — and is
 * exercised by the first test's general round-trip; these store beats keep
 * driving the database-backed scope so they don't depend on the host disk.)
 */
async function createBlankInStore(page: Page, storeName: string) {
  const picker = await openStandalonePicker(page);
  // Exact match: a recents row's label CONTAINS the store name
  // ("Ledger.md Reopen · Project Files: Skyhaven"), so a substring match is
  // ambiguous once anything has been created.
  await picker.getByRole('button', { name: storeName, exact: true }).click();
  await picker.getByRole('button', { name: 'New document here' }).click();
}

test('the document-standalone tab kind round-trips (not-wired → live editor at unify)', async ({
  page,
}) => {
  await openWorkspace(page);
  await page.goto('/workspace?open=document-standalone&scope=general');

  const nw = notWired(page);
  const pane = page.locator('qt-document-pane');
  await expect(nw.or(pane).first()).toBeVisible({ timeout: 15_000 });

  if (await nw.count()) {
    // PRE-UNIFY: the loud not-wired pane. Prove the tab kind round-trips through
    // the layout; the live editor activates at the registry swap.
    await expect(nw).toBeVisible();
    return;
  }

  // POST-UNIFY, general scope (P4.6bg): the wire makes the general scope LIVE —
  // the standalone editor mounts and creates a blank on <files>/_general (the
  // operator open + `pick_untitled_document_path` + the empty-file write).
  await expect(pane).toBeVisible({ timeout: 15_000 });
  await expect(pane).toContainText('Untitled');

  // Edit → flush-on-blur autosave writes the general file to host disk (the
  // operator write fs branch).
  const editor = pane.locator('.qt-rich-editor-content');
  await editor.click();
  await page.keyboard.type('General desk notes.');
  await expect(pane).toContainText('Unsaved');
  await pane.locator('.qt-doc-uri-copy-button').click();
  await expect(pane).toContainText('Saved', { timeout: 15_000 });

  // Rename to a unique title (the operator rename fs branch: `fs.rename` on
  // <files>/_general) so the recents reopen below is unambiguous.
  await pane.locator('.qt-doc-title').click();
  const titleInput = pane.locator('.qt-doc-title-input');
  await titleInput.fill('General Roundtrip');
  await titleInput.press('Enter');
  await expect(tabLabel(page, 'General Roundtrip')).toBeVisible({ timeout: 15_000 });

  // Close, then reopen from the picker recents → the pane re-reads the general
  // file off disk (the operator read fs branch), proving the full
  // create → edit → autosave → read round-trip on <files>/_general.
  await pane.getByRole('button', { name: 'Exit document mode' }).click();
  await expect(page.locator('qt-document-pane')).toHaveCount(0, { timeout: 15_000 });
  const picker = await openStandalonePicker(page);
  await picker.getByRole('button', { name: /General Roundtrip/ }).click();
  await expect(page.locator('qt-document-pane')).toContainText('General desk notes.', {
    timeout: 15_000,
  });
});

test('rail → picker → open → edit → autosave → rename → delete (activates at unify)', async ({
  page,
}) => {
  await openWorkspace(page);
  test.skip(!(await isWired(page)), 'the rail Document-Mode entry mounts at the p4.9j unification');

  await createBlankInStore(page, 'Project Files: Skyhaven');

  const pane = page.locator('qt-document-pane');
  await expect(pane).toBeVisible({ timeout: 15_000 });
  await expect(pane).toContainText('Untitled');

  // Edit in the rich editor → the status bar shows Unsaved.
  const editor = pane.locator('.qt-rich-editor-content');
  await editor.click();
  await page.keyboard.type('Notes from the chat-less desk.');
  await expect(pane).toContainText('Unsaved');

  // Blur the editor (focus the pane's copy-URL button) → flush-on-blur save.
  await pane.locator('.qt-doc-uri-copy-button').click();
  await expect(pane).toContainText('Saved', { timeout: 15_000 });

  // Rename via the title → the title AND the tab label update (payload refresh).
  await pane.locator('.qt-doc-title').click();
  const titleInput = pane.locator('.qt-doc-title-input');
  await titleInput.fill('Desk Notes');
  await titleInput.press('Enter');
  await expect(pane).toContainText('Desk Notes', { timeout: 15_000 });
  await expect(tabLabel(page, 'Desk Notes')).toBeVisible({ timeout: 15_000 });

  // Delete → confirm → the tab closes (no document pane remains).
  page.once('dialog', (d) => void d.accept());
  await pane.getByRole('button', { name: 'Delete document' }).click();
  await expect(page.locator('qt-document-pane')).toHaveCount(0, { timeout: 15_000 });
});

test('reopen-focuses-same-tab: a recents reopen focuses rather than duplicates (activates at unify)', async ({
  page,
}) => {
  await openWorkspace(page);
  test.skip(!(await isWired(page)), 'the rail Document-Mode entry mounts at the p4.9j unification');

  // Create + name a file so it lands in recents.
  await createBlankInStore(page, 'Project Files: Skyhaven');
  const pane = page.locator('qt-document-pane');
  await expect(pane).toBeVisible({ timeout: 15_000 });
  await pane.locator('.qt-doc-title').click();
  await pane.locator('.qt-doc-title-input').fill('Ledger');
  await pane.locator('.qt-doc-title-input').press('Enter');
  await expect(tabLabel(page, 'Ledger')).toBeVisible({ timeout: 15_000 });
  // Close the original tab (docKey identity of a blank is a uuid, so it would
  // NOT collide with a by-path reopen — close it to isolate the recents path).
  await pane.getByRole('button', { name: 'Exit document mode' }).click();
  await expect(page.locator('qt-document-pane')).toHaveCount(0, { timeout: 15_000 });

  // First recents reopen → one Ledger tab (keyed by identity:
  // document_store:Project Files: Skyhaven:Ledger.md).
  let picker = await openStandalonePicker(page);
  await picker.getByRole('button', { name: /Ledger/ }).click();
  await expect(tabLabel(page, 'Ledger')).toHaveCount(1, { timeout: 15_000 });

  // Second recents reopen → focuses the SAME tab (same docKey), not a duplicate.
  picker = await openStandalonePicker(page);
  await picker.getByRole('button', { name: /Ledger/ }).click();
  await expect(tabLabel(page, 'Ledger')).toHaveCount(1);
});
