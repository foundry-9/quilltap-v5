import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * The P4.D75 character-insertion beats — the `:` emoji and `\` symbol
 * typeaheads in the live Salon composer, and a toolbar picker in a live
 * markdown field.
 *
 * These walk the whole feature end to end: the dataset really is fetched over
 * HTTP from `public/`, the trigger really is detected against the live
 * ProseMirror document, and what lands is plain Unicode text — no node, no
 * image, no shortcode.
 *
 * The composer beats use **Group Expedition**, the chat no spec asserts totals
 * or counts on, and blank the draft afterwards so nothing leaks to a sibling
 * spec. The picker beat opens the Insert-announcement dialog for its markdown
 * field and CANCELS — posting an announcement into the shared instance is the
 * sender-contention trap, and this beat needs none of it.
 */

const isMac = process.platform === 'darwin';

test.describe('Composer character insertion (P4.D75)', () => {
  async function maybeUnlock(page: Page) {
    const passphrase = page.locator('#qt-passphrase');
    const chats = page.getByRole('heading', { name: 'Chats', exact: true });
    await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
  }

  async function openChat(page: Page, title: string) {
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
    const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: title });
    await expect(card).toBeVisible();
    await card.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible();
  }

  function composerEditor(page: Page) {
    return page.locator('.qt-chat-composer-input .qt-rich-editor-content');
  }

  /** Leave no draft behind — a sibling spec opening this chat would inherit it. */
  async function blankComposer(page: Page) {
    await composerEditor(page).click();
    await page.keyboard.press(isMac ? 'Meta+a' : 'Control+a');
    await page.keyboard.press('Backspace');
    // The draft saves on an 800ms debounce; let the blank flush.
    await page.waitForTimeout(1200);
  }

  test('`:smile:` lands the emoji as plain text, and a clock time opens nothing', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    const editor = composerEditor(page);
    await editor.click();

    // A clock time is the canonical never-fire case: the colon is preceded by a
    // digit, so no menu opens and nothing is rewritten.
    await page.keyboard.type('meet at 10:30');
    await expect(page.locator('.qt-typeahead-menu')).toHaveCount(0);
    await expect(editor).toContainText('meet at 10:30');

    // The menu opens on a live query (two characters in), then the closing
    // colon commits the exact shortcode without the menu ever mattering.
    await page.keyboard.type(' and :smi');
    await expect(page.locator('.qt-typeahead-menu')).toBeVisible({ timeout: 15_000 });
    await page.keyboard.type('le:');

    await expect(editor).toContainText('and 😄');
    // Plain text: no image, no node, and no trailing space after the emoji.
    await expect(editor.locator('img')).toHaveCount(0);
    await expect(editor).not.toContainText(':smile');
    await expect(page.locator('.qt-typeahead-menu')).toHaveCount(0);

    await blankComposer(page);
  });

  test('the menu commits on Enter and dismisses on Escape', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    const editor = composerEditor(page);
    await editor.click();

    await page.keyboard.type(':tada');
    const menu = page.locator('.qt-typeahead-menu');
    await expect(menu).toBeVisible({ timeout: 15_000 });

    // Escape leaves the typed text exactly as it is.
    await page.keyboard.press('Escape');
    await expect(menu).toHaveCount(0);
    await expect(editor).toContainText(':tada');

    // Retyping reopens it, and Enter commits the highlighted row — the message
    // is NOT sent, because the open menu owns Enter.
    await page.keyboard.press('Backspace');
    await page.keyboard.type('a');
    await expect(menu).toBeVisible();
    await page.keyboard.press('Enter');

    await expect(editor).toContainText('🎉');
    await expect(editor).not.toContainText(':tada');
    await expect(menu).toHaveCount(0);

    await blankComposer(page);
  });

  test('`\\to ` inserts → and KEEPS the space the writer typed', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    const editor = composerEditor(page);
    await editor.click();

    // The Unicode dataset is fetched lazily, on the first live `\` query — and
    // the space bar deliberately never fetches it (`mayTriggerLoad: false`: the
    // most-pressed key on the keyboard must not pull down 300 KB). So a writer
    // who types `\to ` faster than the fetch gets the literal text, in v4 as
    // much as here. Wait for the menu — which is proof the dataset landed —
    // before pressing the commit key, exactly as v4's own suite warms the index
    // before its commit assertions.
    await page.keyboard.type('go \\to');
    await expect(page.locator('.qt-typeahead-menu')).toBeVisible({ timeout: 15_000 });

    await page.keyboard.press('Space');
    await expect(editor).toContainText('go → ');
    await expect(editor).not.toContainText('\\to');

    // Case is significant: \Phi is Φ, not φ. (The dataset is in memory now, so
    // this one commits at typing speed.)
    await page.keyboard.type('and \\Phi ');
    await expect(editor).toContainText('Φ');
    await expect(editor).not.toContainText('φ');

    await blankComposer(page);
  });

  test('the toolbar`s emoji picker inserts into a markdown field', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    // The Insert-announcement dialog hosts a qt-markdown-field, i.e. the
    // formatting toolbar the pickers live in. Nothing is posted — the beat
    // cancels out.
    await page.getByRole('button', { name: 'Insert announcement' }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();

    await dialog.locator('.qt-markdown-field .qt-rich-editor-content').click();
    await page.keyboard.type('Notice: ');

    await dialog.getByRole('button', { name: 'Insert emoji' }).click();
    const search = dialog.getByRole('textbox', { name: 'Search emoji by name' });
    await expect(search).toBeVisible({ timeout: 15_000 });
    await search.fill('tada');

    const cell = dialog.locator('.qt-emoji-picker-cell').first();
    await expect(cell).toHaveAttribute('aria-label', 'party popper');
    await cell.click();

    await expect(dialog.locator('.qt-markdown-field .qt-rich-editor-content')).toContainText(
      'Notice: 🎉',
    );
    // Picking closes the picker.
    await expect(dialog.locator('.qt-emoji-picker')).toHaveCount(0);

    // Reopening shows the pick in a Recently used row (localStorage, under v4's
    // own key).
    await dialog.getByRole('button', { name: 'Insert emoji' }).click();
    await expect(dialog.locator('.qt-emoji-picker-group-header').first()).toHaveText(
      'Recently used',
    );
    await page.keyboard.press('Escape');

    await dialog.getByRole('button', { name: 'Cancel' }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
  });
});
