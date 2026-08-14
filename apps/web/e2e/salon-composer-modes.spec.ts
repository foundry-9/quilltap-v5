import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { openSidebarSection } from './support/sidebar';
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

  test('Enter on a blank trailing line leaves a fenced code block (dogfood #82)', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Solo Voyage');

    // Composition mode, because that is where the trap was found: with Enter
    // sending, a writer never accumulates lines inside the fence to be stuck
    // in. The escape itself is bound in both composer modes (v4 checks it
    // ahead of its chat/composition branch).
    const toggle = page.getByRole('button', { name: 'Toggle composition mode' });
    await toggle.click();
    await expect(toggle).toHaveAttribute('aria-pressed', 'true');

    const editor = composerEditor(page);
    await editor.click();
    await page.keyboard.type('Testing. This is a paragraph.');
    await page.keyboard.press('Enter');
    // ``` opens a fence — the dialect input rule. Nothing closes one.
    await page.keyboard.type('```');
    await expect(editor.locator('pre')).toBeVisible();
    await page.keyboard.type('{ "everything": "works" }');

    // The gesture that was broken: Enter for a blank line, Enter again to
    // leave. Before the fix every Enter only ever lengthened the fence, so a
    // fenced snippet mid-message was a one-way door.
    await page.keyboard.press('Enter');
    await page.keyboard.press('Enter');
    await page.keyboard.type('Prose after the fence.');

    // The prose landed OUTSIDE the fence, and the blank line was trimmed away
    // rather than left dangling inside it.
    await expect(editor.locator('pre')).toContainText('"everything": "works"');
    await expect(editor.locator('pre')).not.toContainText('Prose after the fence.');
    await expect(editor.locator('p').last()).toContainText('Prose after the fence.');

    // Leave the chat as the sibling specs expect it — the draft CLEARED, never
    // sent. Solo Voyage's stored token aggregates are asserted verbatim by
    // `salon-token-cost-flow`, so a message sent from here moves numbers two
    // specs away (measured: it did).
    await page.keyboard.press(process.platform === 'darwin' ? 'Meta+a' : 'Control+a');
    await page.keyboard.press('Backspace');
    await expect(editor.locator('pre')).toHaveCount(0);
    await page.waitForTimeout(1200); // the 800ms draft debounce
    const toggleAfter = page.getByRole('button', { name: 'Toggle composition mode' });
    await toggleAfter.click();
    await expect(toggleAfter).toHaveAttribute('aria-pressed', 'false');
  });

  test('the composer lays out v4’s way: a 2-column gutter left of a dominant editor (dogfood #75)', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Solo Voyage');

    const input = page.locator('.qt-chat-composer-input').first();
    const gutter = page.locator('.qt-composer-gutter-tools').first();
    const toggles = page.locator('.qt-chat-toolbar').first();
    await expect(input).toBeVisible();
    await expect(gutter).toBeVisible();
    await expect(toggles).toBeVisible();

    const inputBox = (await input.boundingBox())!;
    const gutterBox = (await gutter.boundingBox())!;
    const togglesBox = (await toggles.boundingBox())!;
    expect(inputBox).not.toBeNull();
    expect(gutterBox).not.toBeNull();

    // v4's geometry (ChatComposer :368-441): the tools sit to the LEFT of the
    // editor on the SAME row — not wrapped below it, which is what v5's interim
    // #75 band-aid did — with the composer-level toggles between them and the
    // box.
    expect(gutterBox.x + gutterBox.width).toBeLessThanOrEqual(inputBox.x + 1);
    expect(togglesBox.x).toBeGreaterThanOrEqual(gutterBox.x + gutterBox.width - 1);
    expect(togglesBox.x + togglesBox.width).toBeLessThanOrEqual(inputBox.x + 1);

    // The gutter really is TWO columns: six-plus tools in a grid no wider than
    // three buttons. Re-flattening it into one row is what reddens this.
    const gutterCols = await gutter.evaluate(
      (el) => getComputedStyle(el).gridTemplateColumns.split(' ').length,
    );
    expect(gutterCols).toBe(2);

    // And the editor is the dominant share of the row — the user-visible
    // symptom in #75 was the "Type a message…" placeholder clipping to
    // "Type a" once the box fell to its 12rem floor.
    expect(inputBox.width).toBeGreaterThan(300);
    expect(inputBox.width).toBeGreaterThan(gutterBox.width + togglesBox.width);
    const placeholder = page.locator('.qt-rich-editor-placeholder').first();
    if (await placeholder.count()) {
      const clipped = await placeholder.evaluate((el) => el.scrollWidth > el.clientWidth + 1);
      expect(clipped).toBe(false);
    }
  });

  test('the formatting toolbar rides above the composer in composition mode (p4.9l)', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Solo Voyage');

    // Chat mode: no toolbar (v4 gates it on documentEditingMode).
    await expect(page.locator('.qt-chat-composer .qt-formatting-toolbar')).toHaveCount(0);

    const toggle = page.getByRole('button', { name: 'Toggle composition mode' });
    await toggle.click();
    await expect(toggle).toHaveAttribute('aria-pressed', 'true');

    const toolbar = page.locator('.qt-chat-composer .qt-formatting-toolbar');
    await expect(toolbar).toBeVisible();

    // It is its OWN row, above the form — v4's placement, and the half of the
    // #75 fix that is not about the gutter.
    const toolbarBox = (await toolbar.boundingBox())!;
    const inputBox = (await page.locator('.qt-chat-composer-input').first().boundingBox())!;
    expect(toolbarBox.y + toolbarBox.height).toBeLessThanOrEqual(inputBox.y + 1);

    // A representative button from each of its three groups.
    await expect(toolbar.locator('.qt-formatting-button-bold')).toBeVisible();
    await expect(toolbar.getByRole('button', { name: 'Insert emoji' })).toBeVisible();
    await expect(toolbar.locator('.qt-formatting-button-source')).toBeVisible();

    await toggle.click();
    await expect(toggle).toHaveAttribute('aria-pressed', 'false');
    await expect(page.locator('.qt-chat-composer .qt-formatting-toolbar')).toHaveCount(0);
  });

  test('a format button and the code-block toggle reach the serialized markdown (p4.9l)', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Solo Voyage');

    const toggle = page.getByRole('button', { name: 'Toggle composition mode' });
    await toggle.click();
    await expect(toggle).toHaveAttribute('aria-pressed', 'true');
    const toolbar = page.locator('.qt-chat-composer .qt-formatting-toolbar');
    await expect(toolbar).toBeVisible();

    const editor = composerEditor(page);
    await editor.click();
    await page.keyboard.type('shout');
    await page.keyboard.press(isMac ? 'Meta+a' : 'Control+a');
    await toolbar.locator('.qt-formatting-button-bold').click();

    // The wire bytes, read through the source toggle rather than by sending —
    // a send from Solo Voyage moves the token totals `salon-token-cost-flow`
    // asserts verbatim (trap 5b).
    const source = page.locator('.qt-chat-composer .qt-source-mode-textarea');
    await toolbar.locator('.qt-formatting-button-source').click();
    await expect(source).toHaveValue('**shout**');
    await toolbar.locator('.qt-formatting-button-source').click();

    // The code-block button is a TOGGLE, and its label + title flip with the
    // caret's block (v4 `inCodeBlock`).
    const codeButton = toolbar.locator('.qt-formatting-button-code-block');
    await expect(codeButton).toHaveAttribute('title', 'Insert code block');
    await editor.click();
    await page.keyboard.press(isMac ? 'Meta+a' : 'Control+a');
    await page.keyboard.press('Backspace');
    await page.keyboard.type('x = 1');
    await codeButton.click();
    await expect(editor.locator('pre')).toBeVisible();
    await expect(codeButton).toHaveAttribute('title', 'End code block');
    await expect(codeButton).toHaveText('/CODE');

    // Enter still escapes a fence from inside it — the dogfood #82 fix, which
    // the toolbar must not have displaced.
    await editor.click();
    await page.keyboard.press('Enter');
    await page.keyboard.press('Enter');
    await page.keyboard.type('After the fence.');
    await expect(editor.locator('pre')).not.toContainText('After the fence.');
    await expect(editor.locator('p').last()).toContainText('After the fence.');

    // And the button toggles back out of a code block.
    await editor.locator('pre').click();
    await expect(codeButton).toHaveAttribute('title', 'End code block');
    await codeButton.click();
    await expect(editor.locator('pre')).toHaveCount(0);

    // Leave the chat as the sibling specs expect it: draft cleared, chat mode.
    await editor.click();
    await page.keyboard.press(isMac ? 'Meta+a' : 'Control+a');
    await page.keyboard.press('Backspace');
    await page.waitForTimeout(1200);
    await toggle.click();
    await expect(toggle).toHaveAttribute('aria-pressed', 'false');
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
  test('a $$ math message renders KaTeX live; single-$ math promotes; $50/$20 stays plain text', async ({ page }) => {
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
    // The pause control lives in the sidebar's Participants drawer since P4.9H1
    // (v4's home for it — the turn-controls bar keeps only the paused notice).
    await openSidebarSection(page, 'Participants');
    const pauseButton = page.locator('qt-chat-sidebar .qt-chat-pause-button');
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

    // The P4.d11 (5915b04e) extension rides the SAME send — no new sends, the
    // chat-totals coupling note above stands: one message carrying paired
    // currency amounts AND a clearly-LaTeX single-`$` span. The promotion pass
    // (`promoteSingleDollarMath`) must lift `$x^2$` to KaTeX while both dollar
    // amounts stay prose (`$50…$20` pairs carry no marker and are released).
    // `$x^2$` rather than `$\pi r^2$` on purpose: the composer serializer
    // backslash-escapes typed `\` (the same seam the `$$`-block comment above
    // documents), while `^` serializes cleanly — and `^` is a LATEX_MARKER.
    // Wait out the FIRST send's turn before typing the second. Pausing stops the
    // CHAIN, not the responder already chosen, so a reply still streams — and
    // while it does, the composer's `canSend` is false and its Send button is
    // swapped for Stop, so an Enter here is silently swallowed and the second
    // message never posts. (Deterministic enough to fail in isolation on main
    // too, which is how P4.9E2B's gate found it; the beat is about RENDERING,
    // so waiting costs it nothing.)
    await expect(page.locator('.qt-chat-stop-button')).toHaveCount(0, { timeout: 30_000 });

    await composerEditor(page).click();
    await page.keyboard.type('He slid $50 across the table, then another $20, as $x^2$ glowed on the board.');
    await page.keyboard.press('Enter');

    const currencyMsg = page.locator('.qt-chat-message-row-user .qt-chat-message-content', {
      hasText: '$50',
    });
    await expect(currencyMsg.first()).toBeVisible({ timeout: 15_000 });
    // Exactly ONE KaTeX subtree: the promoted formula — not the currency.
    await expect(currencyMsg.first().locator('.katex')).toHaveCount(1);
    // The currency prose survives verbatim around it, dollar signs intact.
    await expect(currencyMsg.first()).toContainText('He slid $50 across the table, then another $20,');

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
