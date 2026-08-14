import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { startMockLlm, MOCK_LLM_REPLY, type MockLlm } from './support/mock-llm';

/**
 * Smart typography, Layer 1.6 (P4.D74) — the live walk.
 *
 * Part B is type-time and needs no server change at all: with the settings bag
 * absent both rules default ON (v4's `?? true`), so the composer beats run LIVE
 * in-lane. Part A's display toggle is a SAVED setting, so its beat is
 * ACTIVATE-AT-UNIFY behind {@link SMART_TYPOGRAPHY_COLUMN_LANDED} — P4.D73 owns
 * the `smartTypographySettings` column, and until it lands a PUT carrying that
 * key is refused by the server rather than stored.
 *
 * The editor beats drive the `qt-rich-editor` contenteditable with real key
 * events (the P4.6ag idiom — `.fill()` targets input/textarea only), which is
 * also the only way to exercise a keydown-driven feature honestly.
 */

/** ACTIVATE-AT-UNIFY: flip to `true` once P4.D73's settings column is in. */
const SMART_TYPOGRAPHY_COLUMN_LANDED = false;

const EN_DASH = '–';
const EM_DASH = '—';
const ELLIPSIS = '…';

const isMac = process.platform === 'darwin';

test.describe('Smart typography (P4.D74)', () => {
  // The gated Part A beat posts a message, which starts a turn — so the mock
  // stands in for the model, exactly as the sibling composer spec does.
  let mock: MockLlm;

  test.beforeAll(async () => {
    mock = await startMockLlm(MOCK_LLM_REPLY, MOCK_LLM_PORT);
  });
  test.afterAll(async () => {
    await mock?.close();
  });

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

  /** Leave no draft behind — a stray one leaks into sibling specs. */
  async function clearComposer(page: Page) {
    await composerEditor(page).click();
    await page.keyboard.press(isMac ? 'Meta+a' : 'Control+a');
    await page.keyboard.press('Backspace');
    await page.waitForTimeout(1200);
  }

  test('the composer walks the dash ladder and the ellipsis as you type', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    const editor = composerEditor(page);
    await editor.click();

    // `--` → en dash, `---` → em dash: each substitution consumes the keystroke
    // and rewrites the character behind the caret.
    await page.keyboard.type('wait-');
    await expect(editor).toContainText('wait-');
    await page.keyboard.type('-');
    await expect(editor).toContainText(`wait${EN_DASH}`);
    await page.keyboard.type('-');
    await expect(editor).toContainText(`wait${EM_DASH}`);

    // A fourth hyphen is the escape hatch — the ladder ends, nothing is eaten.
    await page.keyboard.type('-');
    await expect(editor).toContainText(`wait${EM_DASH}-`);

    await clearComposer(page);

    // `...` → ellipsis, on the third dot only.
    await composerEditor(page).click();
    await page.keyboard.type('Well..');
    await expect(composerEditor(page)).toContainText('Well..');
    await page.keyboard.type('.');
    await expect(composerEditor(page)).toContainText(`Well${ELLIPSIS}`);

    await clearComposer(page);
  });

  test('one Backspace puts the literal characters back', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    const editor = composerEditor(page);
    await editor.click();
    await page.keyboard.type('1920--');
    await expect(editor).toContainText(`1920${EN_DASH}`);

    // The revert window: Backspace immediately after the substitution restores
    // what was typed, rather than deleting the dash it produced.
    await page.keyboard.press('Backspace');
    await expect(editor).toContainText('1920--');
    await expect(editor).not.toContainText(EN_DASH);

    // ...and the window is one keystroke wide. A second Backspace is ordinary
    // deletion, never a second revert.
    await page.keyboard.press('Backspace');
    await expect(editor).toContainText('1920-');
    await expect(editor).not.toContainText(EN_DASH);

    await clearComposer(page);
  });

  test('nothing is rewritten inside a fenced code block (bug 63)', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    const editor = composerEditor(page);
    await editor.click();
    // ``` at the start of a block turns it into a code fence (the dialect input
    // rule), and inside one no typing aid may fire.
    await page.keyboard.type('```');
    await expect(editor.locator('pre')).toBeVisible();
    await page.keyboard.type('npm run build --verbose...');

    await expect(editor.locator('pre')).toContainText('--verbose...');
    await expect(editor.locator('pre')).not.toContainText(EN_DASH);
    await expect(editor.locator('pre')).not.toContainText(ELLIPSIS);

    await clearComposer(page);
  });

  test('the settings card previews the substitution through the same engine', async ({ page }) => {
    await page.goto('/settings?tab=chat');

    // The workspace-hosted settings page ignores `?section=` (as v4's does), so
    // the card is opened by its own header — which is the affordance a reader
    // uses anyway. The header carries the title AND v4's description, so match
    // on the title text unanchored rather than by exact accessible name.
    //
    // Readiness is checked against THAT header rather than the salon's "Chats"
    // heading: this page never shows one, so the shared helper would wait out
    // its timeout on an instance a previous beat already unlocked.
    const header = page.getByRole('button', { name: /Smart Typography/ }).first();
    const passphrase = page.locator('#qt-passphrase');
    await expect(passphrase.or(header).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
    await expect(header).toBeVisible();
    if ((await header.getAttribute('aria-expanded')) !== 'true') await header.click();
    await expect(header).toHaveAttribute('aria-expanded', 'true');

    const tryIt = page.getByLabel('Try smart typography');
    await expect(tryIt).toBeVisible();
    // A textarea, so `fill` is the right gesture for the setup, and the
    // keystroke under test is typed.
    await tryIt.fill('wait-');
    await tryIt.press('-');
    await expect(tryIt).toHaveValue(`wait${EN_DASH}`);

    await tryIt.fill('Well..');
    await tryIt.press('.');
    await expect(tryIt).toHaveValue(`Well${ELLIPSIS}`);
  });

  test('displayQuotes curls the rendering and leaves the stored bytes alone', async ({ page }) => {
    test.skip(
      !SMART_TYPOGRAPHY_COLUMN_LANDED,
      'ACTIVATE-AT-UNIFY: needs P4.D73 smartTypographySettings column',
    );

    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');
    const chatId = new URL(page.url()).pathname.split('/').pop()!;

    // Post a straight-quoted line. The composer is the only writer of message
    // content, so this is also the proof that Part B left the quotes alone.
    const editor = composerEditor(page);
    await editor.click();
    await page.keyboard.type('"Hello there," she said warmly.');
    await page.keyboard.press('Enter');

    const bubble = page
      .locator('.qt-chat-message-content')
      .filter({ hasText: 'she said warmly' })
      .last();
    await expect(bubble).toContainText('"Hello there,"');

    // Flip the display setting, then watch the ALREADY-rendered message
    // repaint — the render memo is keyed on the typographer state, so a stale
    // memo would keep the straight quotes and this is what catches it.
    const put = await page.request.put('/api/v1/settings/chat', {
      data: { smartTypographySettings: { displayQuotes: true, dashes: true, ellipsis: true } },
    });
    expect(put.ok()).toBe(true);

    await page.reload();
    await maybeUnlock(page);
    await expect(bubble).toContainText('“Hello there,”');

    // The stored bytes never moved: what the model, the embeddings and every
    // export see is still exactly what was typed.
    const read = await page.request.get(`/api/v1/chats/${chatId}`);
    expect(read.ok()).toBe(true);
    const body = JSON.stringify(await read.json());
    expect(body).toContain('\\"Hello there,\\"');
    expect(body).not.toContain('“Hello there,”');

    // Put the instance back the way the fixture ships it.
    const reset = await page.request.put('/api/v1/settings/chat', {
      data: { smartTypographySettings: { displayQuotes: false, dashes: true, ellipsis: true } },
    });
    expect(reset.ok()).toBe(true);
  });
});
