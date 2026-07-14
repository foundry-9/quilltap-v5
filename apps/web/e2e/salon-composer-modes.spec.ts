import { expect, test, type Page } from '@playwright/test';

import { E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { startMockLlm, MOCK_LLM_REPLY, type MockLlm } from './support/mock-llm';

/**
 * The P4.6ak∥al∥am unification beats — the composer features whose salon wiring
 * no single lane could walk (§3 of the round's Shared contract):
 *
 * 1. Composition mode (dogfood #8): the toolbar toggle flips Enter to
 *    newline-insert and Cmd/Ctrl+Enter to send, and the flag persists on the
 *    chat (`chats.documentEditingMode` via `chatUpdate`) across a reload.
 * 2. Draft persistence: an unsent composer draft (localStorage, 800ms debounce)
 *    survives leaving and reopening the chat.
 * 3. Text replacement: a rule created over lane A's LIVE
 *    `/api/v1/settings/text-replacements` REST leg fires in the composer on the
 *    trigger char (lane B's plugin over the salon's live rule fetch).
 *
 * All three drive the qt-rich-editor contenteditable with real key events
 * (the P4.6ag idiom — `.fill()` targets input/textarea).
 */

const isMac = process.platform === 'darwin';
const MOD_ENTER = isMac ? 'Meta+Enter' : 'Control+Enter';

test.describe('Salon composer modes (P4.6ak∥al∥am unification)', () => {
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

  test('composition mode: Enter inserts a newline, Mod+Enter sends, the flag persists (dogfood #8)', async ({
    page,
  }) => {
    await page.goto('/');
    await maybeUnlock(page);
    await openChat(page, 'Solo Voyage');

    const toggle = page.getByRole('button', { name: 'Toggle composition mode' });
    await expect(toggle).toHaveAttribute('aria-pressed', 'false');
    await toggle.click();
    await expect(toggle).toHaveAttribute('aria-pressed', 'true');

    // In composition mode Enter splits the block instead of sending: the text
    // stays in the editor as two paragraphs and no message is dispatched.
    const editor = composerEditor(page);
    await editor.click();
    await page.keyboard.type('First line.');
    await page.keyboard.press('Enter');
    await page.keyboard.type('Second line.');
    await expect(editor.locator('p')).toHaveCount(2);
    await expect(editor).toContainText('First line.');
    await expect(editor).toContainText('Second line.');

    // The flag persisted on the chat (chatUpdate {documentEditingMode}) — a
    // reload lands back in composition mode with the draft restored. Let the
    // 800ms draft debounce (and the toggle's async persist) flush first.
    await page.waitForTimeout(1200);
    await page.reload();
    await expect(composerEditor(page)).toContainText('First line.', { timeout: 15_000 });
    await expect(
      page.getByRole('button', { name: 'Toggle composition mode' }),
    ).toHaveAttribute('aria-pressed', 'true');

    // Mod+Enter sends: the two-line message lands in the flow and the mock
    // reply streams in.
    await composerEditor(page).click();
    await page.keyboard.press(MOD_ENTER);
    await expect(
      page.locator('.qt-chat-messages-list, .qt-chat-messages').getByText('Second line.').first(),
    ).toBeVisible({ timeout: 15_000 });
    await expect(page.getByText(MOCK_LLM_REPLY).first()).toBeVisible({ timeout: 15_000 });

    // Leave the chat in chat mode for the sibling specs.
    const toggleAfter = page.getByRole('button', { name: 'Toggle composition mode' });
    await toggleAfter.click();
    await expect(toggleAfter).toHaveAttribute('aria-pressed', 'false');
  });

  test('an unsent draft survives leaving and reopening the chat', async ({ page }) => {
    await page.goto('/');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    const editor = composerEditor(page);
    await editor.click();
    await page.keyboard.type('An unsent thought.');
    // The draft saves on an 800ms debounce — let it flush before leaving.
    await page.waitForTimeout(1200);

    await page.goBack();
    await openChat(page, 'Group Expedition');
    await expect(composerEditor(page)).toContainText('An unsent thought.');

    // Blanking the editor removes the draft (v4 persistDraft) so reruns and
    // sibling specs start clean.
    await composerEditor(page).click();
    await page.keyboard.press(isMac ? 'Meta+a' : 'Control+a');
    await page.keyboard.press('Backspace');
    await page.waitForTimeout(1200);
  });

  test('a live text-replacement rule fires in the composer on the trigger char', async ({
    page,
  }) => {
    // Create the rule over lane A's LIVE REST leg (201-created body).
    const created = await page.request.post('/api/v1/settings/text-replacements', {
      data: { fromText: 'qteh', toText: 'qthe' },
    });
    expect(created.status()).toBe(201);
    const rule = (await created.json()) as { rule?: { id: string }; id?: string };
    const ruleId = rule.rule?.id ?? rule.id;
    expect(ruleId).toBeTruthy();

    try {
      await page.goto('/');
      await maybeUnlock(page);
      await openChat(page, 'Group Expedition');

      // Typing the word + a trigger char (space) rewrites it in place — lane
      // B's plugin over the salon's live rule fetch.
      const editor = composerEditor(page);
      await editor.click();
      await page.keyboard.type('qteh ');
      await expect(editor).toContainText('qthe');
      await expect(editor).not.toContainText('qteh');

      // Clean the composer so no draft leaks to sibling specs.
      await page.keyboard.press(isMac ? 'Meta+a' : 'Control+a');
      await page.keyboard.press('Backspace');
      await page.waitForTimeout(1200);
    } finally {
      const deleted = await page.request.delete(
        `/api/v1/settings/text-replacements/${ruleId}`,
      );
      expect(deleted.status()).toBe(204);
    }
  });
});
