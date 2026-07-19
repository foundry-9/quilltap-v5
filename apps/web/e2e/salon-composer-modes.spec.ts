import { expect, test, type Page } from './support/fixtures';

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
    await page.goto('/salon');
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
    await page.goto('/salon');
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
      await page.goto('/salon');
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

  // P4.d9 (KaTeX/markdown drift): a USER message renders through the same client
  // pipeline as any message, so this exercises the LIVE remark-math + rehype-katex
  // render with no LLM leg. We send a `$$` DISPLAY block rather than `\(E=mc^2\)`
  // on purpose: the qt-rich-editor's markdown serializer backslash-escapes typed
  // `\(`/`\)` (prosemirror-markdown `esc()`; v5's stripMarkdownEscapes preserves
  // backslash), so `\(…\)` typed into the composer serializes to `\\(…\\)` — the
  // composer's own backslash handling, out of this lane's scope. The `\(…\)` → `$$`
  // normalization is proven byte-for-byte by the captured-v4 fixtures instead
  // (`math-inline-paren` & friends). A `$$` block serializes cleanly and renders
  // `.katex-display` wrapping a `.katex`, proving the pipeline is wired live.
  // Sends land in "Group Expedition", NOT "Solo Voyage": the P4.6ap chat-totals
  // beat (`salon-token-cost-flow.spec.ts`) asserts a hardcoded 15.4K-token
  // baseline for Solo Voyage, and every message this beat sends shifts it. No
  // spec asserts token totals or message counts on Group Expedition.
  test('a $$ math message renders KaTeX live; $50/$20 stays plain text', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    // Deterministic ground: PAUSE auto-responses before sending. The group's
    // inherited turn state varies across a full-suite run, and an unpaused
    // send fires a turn chain whose terminal state is not deterministic — it
    // can park on an AI turn (Nudge affordance, composer DISABLED), swallowing
    // this beat's second send. The beat is about RENDERING, not turns; with
    // the chat paused, both sends post as plain user messages and no chain
    // ever runs. (The m4b pause round-trip proves the toggle end-to-end.)
    const pauseButton = page.locator('.qt-chat-pause-button');
    await expect(pauseButton).toBeVisible();
    if (!(((await pauseButton.textContent()) ?? '').includes('Resume'))) {
      await pauseButton.click();
      await expect(pauseButton).toContainText('Resume');
    }

    // A display block: `$$` / `E = mc^2` / `$$` on their own lines (Shift+Enter
    // inserts a soft break in chat mode, Enter sends). remark-math renders it to
    // a `.katex-display` subtree (which itself contains a `.katex`).
    const editor = composerEditor(page);
    await editor.click();
    await page.keyboard.type('$$');
    await page.keyboard.press('Shift+Enter');
    await page.keyboard.type('E = mc^2');
    await page.keyboard.press('Shift+Enter');
    await page.keyboard.type('$$');
    await page.keyboard.press('Enter');

    await expect(page.locator('.qt-chat-message-row-user .katex-display').first()).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.locator('.qt-chat-message-row-user .katex').first()).toBeVisible();

    // The negative: paired dollar amounts are NOT math (singleDollarTextMath off),
    // so the message renders as plain prose with no KaTeX subtree.
    await composerEditor(page).click();
    await page.keyboard.type('He slid $50 across the table, then another $20.');
    await page.keyboard.press('Enter');

    const currencyMsg = page.locator('.qt-chat-message-row-user .qt-chat-message-content', {
      hasText: '$50',
    });
    await expect(currencyMsg.first()).toBeVisible({ timeout: 15_000 });
    await expect(currencyMsg.first().locator('.katex')).toHaveCount(0);

    // Restore the running state for sibling specs, then let the resumed chain
    // (if the turn pointer sits on an AI participant) drain: poll until the
    // canned-reply count stops growing.
    await pauseButton.click();
    await expect(pauseButton).toContainText('Pause');
    await expect
      .poll(
        async () => {
          const before = await page.getByText(MOCK_LLM_REPLY).count();
          await page.waitForTimeout(1000);
          return (await page.getByText(MOCK_LLM_REPLY).count()) - before;
        },
        { timeout: 20_000 },
      )
      .toBe(0);
  });
});
