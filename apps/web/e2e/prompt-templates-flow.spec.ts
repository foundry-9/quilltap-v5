import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * P4.83 — Import from Template, end to end on BOTH character hosts.
 *
 * The catalogue behind it is v4's 21 built-in "Sample Prompts", which v5 seeds
 * LAZILY: the first `GET /api/v1/prompt-templates` on an instance inserts them.
 * The e2e fixture is a copy of the committed chat-send instance, which carries
 * none — so the first open below is what fills the table, and the section
 * appearing at all is that seeding running through the real server.
 *
 * ⚠ The seeded row's name is the registry's DISPLAY name, not the filename:
 * `MODERN_GENERAL.md` is listed as **`MODERN General`**. Anything asserting
 * `MODERN_GENERAL` is asserting a string that never reaches the database.
 *
 * The two beats deliberately differ in gesture because v4's two hosts do: the
 * editor's `openImportModal` ALWAYS refetches, while the new-character host
 * opens first and fetches only while its list is empty. The unit specs pin the
 * refetch counts; these beats pin what the operator sees.
 *
 * The walk writes nothing that outlives it: beat (a) opens the create-prompt
 * modal with the imported content and CANCELS, beat (b) fills a form it never
 * submits. The seeded `prompt_templates` rows do outlive it, by design — they
 * are what v4 would have on any instance anyone has ever opened this modal on.
 */

const BUILTIN = 'MODERN General';

/** Unlock if the instance is still locked; both beats land on the roster first. */
async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const roster = page.getByRole('heading', { name: 'Characters', exact: true });
  await expect(passphrase.or(roster).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
    await expect(roster).toBeVisible({ timeout: 15_000 });
  }
}

test.describe('P4.83 — Import from Template', () => {
  test('(a) the editor host: the seeded Sample Prompts fill the create modal', async ({
    page,
  }) => {
    await page.goto('/characters');
    await maybeUnlock(page);
    await expect(page.getByRole('heading', { name: 'Characters', exact: true })).toBeVisible({
      timeout: 15_000,
    });
    await page.locator('qt-character-card').filter({ hasText: 'Bram' }).first().locator('h2').click();
    await expect(page.getByRole('heading', { name: 'Bram', level: 1 })).toBeVisible({
      timeout: 15_000,
    });
    await page.getByRole('link', { name: /Edit Character/i }).click();
    await page.getByRole('button', { name: 'System Prompts' }).click();
    await expect(page.getByRole('heading', { name: 'Subprompts', exact: true })).toBeVisible({
      timeout: 15_000,
    });

    await page.getByRole('button', { name: 'Import Template', exact: true }).click();

    // The seeding ran: v4's "Sample Prompts" section, not the empty-state copy.
    const modal = page.locator('qt-character-prompt-import-modal');
    await expect(modal.getByText('Sample Prompts', { exact: true })).toBeVisible({
      timeout: 15_000,
    });
    await expect(modal.getByText('No templates available')).toHaveCount(0);
    const row = modal.locator('button').filter({ hasText: BUILTIN }).first();
    await expect(row).toBeVisible();
    // The row carries v4's badges, built from the seeded category / modelHint.
    await expect(row).toContainText('GENERAL');
    await expect(row).toContainText('MODERN');
    await expect(row).toContainText('GENERAL prompt optimized for MODERN models');

    await row.click();

    // v4 `handleImport`: the create-prompt modal opens with the template's name
    // as the suggested name and its content in the body.
    //
    // Asserted on the modal's CONTENT, never on the `qt-prompt-modal` host: an
    // Angular custom element with no CSS rule of its own is `display: inline`,
    // and everything it renders is a FIXED overlay, so the host's own box is
    // empty and Playwright reads it as hidden (the first live run's catch — the
    // `qt-tab-view` / `qt-markdown-field` inline-host family; harmless here,
    // since the overlay is what the operator sees, but it makes the host a bad
    // locator).
    const promptModal = page.locator('qt-prompt-modal');
    await expect(page.getByRole('heading', { name: 'Create Prompt' })).toBeVisible({
      timeout: 15_000,
    });
    await expect(promptModal.locator('input[type="text"]').first()).toHaveValue(BUILTIN);
    await expect(promptModal.locator('.qt-dialog-title')).toHaveText('Create Prompt');
    await expect(promptModal).toContainText('{{char}}');

    await promptModal.getByRole('button', { name: 'Cancel', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Create Prompt' })).toHaveCount(0);
  });

  test('(b) the new-character host: importing fills the System Prompt field', async ({ page }) => {
    // Unlock on the roster — the unlock gate's heading lives there, not on the
    // New Character page (the wizard spec's first-live-run catch).
    await page.goto('/characters');
    await maybeUnlock(page);
    await page.goto('/characters/new');

    const importButton = page.getByRole('button', { name: 'Import Template', exact: true });
    await expect(importButton).toBeVisible({ timeout: 15_000 });
    await importButton.click();

    const modal = page.locator('qt-character-prompt-import-modal');
    await expect(modal.getByText('Sample Prompts', { exact: true })).toBeVisible({
      timeout: 15_000,
    });
    await modal.locator('button').filter({ hasText: BUILTIN }).first().click();

    // v4 `handleTemplateImport`: the modal closes and the content lands in the
    // System Prompt field (no suggested name on this host — v4 ignores it).
    await expect(modal).toHaveCount(0);
    await expect(page.locator('[aria-label="System Prompt"]')).toContainText('{{char}}', {
      timeout: 15_000,
    });
  });
});
