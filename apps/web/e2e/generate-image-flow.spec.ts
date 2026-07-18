import { expect, test, type Page } from '@playwright/test';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so
 * its filename must sort AFTER foundation.spec.ts (workers: 1, alphabetical file
 * order) — "generate-image-flow" ('g') sorts after "foundation" ('f').
 *
 * P4.9b — a LIVE browser walk of the standalone image-generation surface:
 *
 *   1. The screen beat: Home shows the restored "Generate Image" quick action →
 *      clicking it lands on `/generate-image` → the picker, prompt, insert
 *      widget and 1-4 count all render → Generate stays DISABLED through
 *      prompt-only and profile-only, and enables only with both. This is the
 *      no-auto-default-profile rule (v4 GenerateImageView :37,:282) walked in a
 *      real browser.
 *   2. The insert beat: the {{me}} and {{char}} quick buttons splice into the
 *      prompt, and consecutive inserts chain rather than nest (v4 :87's caret
 *      placement, end to end through the real textarea).
 *   3. The dialog beat: the composer's camera gutter button opens the STANDALONE
 *      dialog (v4's only opener chain, ComposerGutterTools :52), and the
 *      chat-profile dialog — which v4 mounts but never opens — stays absent.
 *
 * NO REAL PROVIDER SPEND. The e2e instance has no live image provider, so the
 * walk never presses Generate against a real profile: beat 1 stops at the
 * enablement gate, and the dialog beat only opens and closes. Nothing here can
 * bill an image API (the P4.6ac beat pattern).
 *
 * The walk is READ-ONLY over the shared fixture: it types into a prompt that is
 * never submitted and opens a dialog it cancels, so the instance is left
 * pristine for later specs.
 */

async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const anyScreen = page
    .locator('.qt-homepage-container')
    .or(page.getByRole('heading', { name: 'Chats', exact: true }))
    .or(page.getByRole('heading', { name: 'Generate Image', exact: true }));
  await expect(passphrase.or(anyScreen).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

function promptBox(page: Page) {
  return page.locator('#generate-image-prompt');
}

function generateButton(page: Page) {
  return page.getByRole('button', { name: 'Generate Image', exact: true });
}

test.describe('P4.9b — the standalone Generate Image screen', () => {
  test('Home links to it, and Generate stays disabled until both a profile and a prompt are set', async ({
    page,
  }) => {
    await page.goto('/');
    await maybeUnlock(page);

    // The quick action restored in unit 3 (v4 QuickActionsRow :90-98).
    const quickAction = page.getByRole('link', { name: 'Generate Image' });
    await expect(quickAction).toBeVisible();
    await quickAction.click();

    await expect(page).toHaveURL(/\/generate-image$/);
    await expect(page.getByRole('heading', { name: 'Generate Image', exact: true })).toBeVisible();

    // The form: the picker's select (leading "No image generation"), the prompt,
    // the insert widget, and the 1-4 count.
    const profileSelect = page.locator('qt-image-profile-picker select');
    await expect(profileSelect).toBeVisible();
    await expect(profileSelect.locator('option').first()).toHaveText('No image generation');
    await expect(promptBox(page)).toBeVisible();
    await expect(page.getByPlaceholder('Search characters to insert...')).toBeVisible();
    await expect(page.locator('#generate-image-count option')).toHaveCount(4);

    // The empty state, before anything is generated (v4 :358-364).
    await expect(page.getByText('No images generated yet')).toBeVisible();

    // No profile is auto-selected, so Generate starts disabled (v4 :37,:282).
    await expect(profileSelect).toHaveValue('');
    await expect(generateButton(page)).toBeDisabled();

    // A prompt alone is not enough.
    await promptBox(page).fill('a cat in a bowler hat');
    await expect(generateButton(page)).toBeDisabled();

    // Pick a real profile from the seeded instance — the enabling half.
    const profileOptions = profileSelect.locator('option');
    const optionCount = await profileOptions.count();
    if (optionCount > 1) {
      await profileSelect.selectOption({ index: 1 });
      // Both halves present → enabled. We stop HERE: pressing Generate would
      // reach a real provider.
      await expect(generateButton(page)).toBeEnabled();

      // The picker's detail card appears for the selection (v4 :110-129).
      await expect(page.locator('qt-image-profile-picker .qt-border-info')).toBeVisible();

      // And clearing the prompt disables it again, proving both halves are live.
      await promptBox(page).fill('');
      await expect(generateButton(page)).toBeDisabled();
    } else {
      // No profiles seeded: the picker says so, and Generate can never enable.
      await expect(page.getByText('No image profiles available')).toBeVisible();
      await expect(generateButton(page)).toBeDisabled();
    }
  });

  test('the {{me}} and {{char}} quick inserts splice into the prompt and chain', async ({
    page,
  }) => {
    await page.goto('/generate-image');
    await maybeUnlock(page);
    await expect(page.getByRole('heading', { name: 'Generate Image', exact: true })).toBeVisible();

    await promptBox(page).fill('');
    await page.getByRole('button', { name: 'me', exact: true }).click();
    await expect(promptBox(page)).toHaveValue('{{me}}');

    // The caret lands past the closing braces, so the next insert appends
    // rather than nesting inside the first (v4 :87).
    await page.getByRole('button', { name: 'char', exact: true }).click();
    await expect(promptBox(page)).toHaveValue('{{me}}{{char}}');

    // Leave the field clean for anything downstream.
    await promptBox(page).fill('');
  });
});

test.describe('P4.9b — the standalone dialog from the composer gutter', () => {
  test('the camera button opens the standalone dialog, not the chat-profile one', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();

    const card = page.locator('.chat-card-stack a.qt-entity-card').first();
    await expect(card).toBeVisible();
    await card.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible();

    // v4's single opener chain: ComposerGutterTools :52 → the standalone dialog.
    await page.locator('button[aria-label="Generate image"]').click();

    // The modal's own element carries role="dialog" (qt-modal); the Angular
    // host wrapper has no layout box of its own, so locate the role.
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();

    // It is the STANDALONE dialog: only that one carries a profile picker, the
    // participant quick-inserts, and a 1-4 count.
    await expect(dialog.locator('#standalone-image-prompt')).toBeVisible();
    await expect(dialog.locator('qt-image-profile-picker select')).toBeVisible();
    await expect(dialog.locator('#standalone-image-count option')).toHaveCount(4);
    await expect(dialog.getByText('Quick Insert')).toBeVisible();

    // The chat-profile dialog is mounted-but-openerless in v4; v5 keeps that,
    // so its component must not be instantiated at all.
    await expect(page.locator('qt-generate-image-dialog')).toHaveCount(0);

    // Its Generate is disabled with no profile picked — again, no provider spend.
    await expect(dialog.getByRole('button', { name: 'Generate Image' })).toBeDisabled();

    // Cancel, leaving the chat untouched.
    await dialog.getByRole('button', { name: 'Cancel' }).click();
    await expect(dialog).toHaveCount(0);
  });
});
