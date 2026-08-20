import {
  expect,
  request as pwRequest,
  test,
  type APIRequestContext,
  type Page,
} from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so
 * its filename must sort AFTER foundation.spec.ts — foundation walks the
 * locked -> unlock gate and must reach the shared server first (workers: 1,
 * alphabetical file order). Every shared-server spec obeys this; a
 * "document-flow" name broke it at unification.
 *
 * P4.6x — a LIVE browser walk of Document Mode over the shared Salon server:
 * open a fixture chat → open the Document Picker → create a new blank document →
 * the pane renders beside the chat → edit → flush-save (status → Saved) → the
 * content survives a reload → rename (the Librarian chip lands) → close (the pane
 * collapses and the Open-document button returns). Plus a both-panes beat
 * (document + terminal stacked in the right pane's vertical split).
 *
 * FIXTURE-GUARDED. The document dispatch (`chatDocumentOpen` &c.) lands in lane B
 * (P4.6w); this SPA lane builds against the contract with mocks + unit specs. The
 * beats auto-activate at unification: a `beforeAll` capability probe skips the
 * whole describe when the shared server doesn't implement the document dispatch
 * (the in-lane signature — serde rejects the unknown request variant), and runs
 * it live once lane B's arms merge. `chat_documents` is materialized pre-launch
 * in `global-setup.ts` (the terminal_sessions precedent).
 */

let docBackendReady = false;
const isMac = process.platform === 'darwin';

test.beforeAll(async () => {
  // Probe the shared server: is the document dispatch handled? In-lane the Rust
  // core has no document arms, so the request deserialization fails with an
  // "unknown variant" error → not ready. Any other outcome (a success envelope
  // or a domain error like not-found) means the handler exists → ready.
  try {
    const ctx = await pwRequest.newContext();
    const res = await ctx.post(`${BASE_URL}/api/dispatch`, {
      data: { type: 'chatActiveDocument', chatId: '00000000-0000-0000-0000-000000000000' },
    });
    const body = (await res.json().catch(() => null)) as
      | { type?: string; data?: { message?: string } }
      | null;
    await ctx.dispose();
    const isUnknownVariant =
      body?.type === 'error' && /unknown variant/i.test(String(body?.data?.message ?? ''));
    docBackendReady = body != null && !isUnknownVariant;
  } catch {
    docBackendReady = false;
  }
});

/** Raw dispatch against the real axum server (mirrors new-chat-flow). */
async function dispatch(ctx: APIRequestContext, req: unknown): Promise<Record<string, unknown>> {
  const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: req });
  const body = (await res.json().catch(() => null)) as { data?: Record<string, unknown> } | null;
  return body?.data ?? {};
}

/** Unlock only when the passphrase screen is showing (the shared server stays unlocked). */
async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const chats = page.getByRole('heading', { name: 'Chats', exact: true });
  await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

/** Open the solo fixture chat (needs no LLM participant for the document walk). */
async function openSoloChat(page: Page): Promise<void> {
  await page.goto('/salon');
  await maybeUnlock(page);
  await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
  await page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Solo Voyage' }).click();
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible();
}

/** Create a new blank document through the picker and return the pane locator. */
async function openBlankDocument(page: Page) {
  await page.getByRole('button', { name: 'Open document' }).click();
  const picker = page.getByRole('dialog');
  await expect(picker).toBeVisible();
  await picker.getByRole('button', { name: /New blank document/ }).click();
  const pane = page.locator('qt-document-pane');
  await expect(pane).toBeVisible({ timeout: 15_000 });
  return pane;
}

test.describe('P4.6x — Document Mode (open → edit → save → rename → close)', () => {
  test.beforeEach(() => {
    test.skip(!docBackendReady, 'awaits lane B document dispatch (activates at unification)');
  });

  test('create a blank document, edit, save, rename, and close', async ({ page }) => {
    await openSoloChat(page);
    const pane = await openBlankDocument(page);

    // A fresh Untitled markdown doc edits in the rich editor (D17 GREEN,
    // P4.6ag) — a ProseMirror contenteditable, no longer a textarea.
    await expect(pane).toContainText('Untitled');
    const editor = pane.locator('.qt-rich-editor-content');
    await expect(editor).toBeVisible();

    // Edit → the status bar shows Unsaved.
    await editor.click();
    await page.keyboard.type('Hello from the Scriptorium.');
    await expect(pane).toContainText('Unsaved');

    // Blur the editor (focus the composer) → flush the save → Saved.
    await page.locator('.qt-chat-composer-input .qt-rich-editor-content').click();
    await expect(pane).toContainText('Saved', { timeout: 15_000 });

    // The edit survives a full reload (the server persisted it). The reload
    // lands back on the CHAT page (the server stays unlocked), so wait for the
    // chat body — not the shell entry maybeUnlock() expects.
    await page.reload();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
    const reopened = page.locator('qt-document-pane');
    await expect(reopened).toBeVisible({ timeout: 15_000 });
    await expect(reopened.locator('.qt-rich-editor-content')).toContainText(
      'Hello from the Scriptorium.',
    );

    // Rename via the title → the title updates and a Librarian chip lands.
    await reopened.locator('.qt-doc-title').click();
    const titleInput = reopened.locator('.qt-doc-title-input');
    await titleInput.fill('Expedition Notes');
    await titleInput.press('Enter');
    await expect(reopened).toContainText('Expedition Notes', { timeout: 15_000 });
    await expect(page.locator('.qt-chat-announcement-chip').first()).toBeVisible({ timeout: 15_000 });

    // Close → the pane collapses and the Open-document button returns.
    await reopened.getByRole('button', { name: 'Exit document mode' }).click();
    await expect(page.locator('qt-document-pane')).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Open document' })).toBeVisible();
  });

  test('markdown edits round-trip the v4 dialect through the rich editor', async ({ page }) => {
    await openSoloChat(page);
    const pane = await openBlankDocument(page);
    const editor = pane.locator('.qt-rich-editor-content');
    await editor.click();

    // A heading via the markdown input rule, then a body line whose literal
    // roleplay punctuation (single `*`, `_`) must survive unescaped.
    await page.keyboard.type('# Chapter One');
    await page.keyboard.press('Enter');
    await page.keyboard.type('She waves *without a word* and _leaves_.');

    // Flush-save (focus the composer).
    await page.locator('.qt-chat-composer-input .qt-rich-editor-content').click();
    await expect(pane).toContainText('Saved', { timeout: 15_000 });

    // Toggle to raw source: the on-disk bytes are the v4 composer dialect — the
    // heading is real markdown, the `*`/`_` are literal (block-separated).
    await pane.getByRole('button', { name: 'Show markdown source' }).click();
    await expect(pane.locator('textarea')).toHaveValue(
      '# Chapter One\n\nShe waves *without a word* and _leaves_.',
    );

    await pane.getByRole('button', { name: 'Exit document mode' }).click();
    await expect(page.locator('qt-document-pane')).toHaveCount(0);
  });

  test('the composer sends the v4 dialect bytes (literal star + underscore)', async ({ page }) => {
    await openSoloChat(page);

    // Capture the outgoing chatSend request — its `content` is exactly what the
    // editor handle serialized (send-reads-handle, v4 ComposerSyncPlugin).
    let sent: string | undefined;
    await page.route('**/api/dispatch', async (route) => {
      const body = route.request().postDataJSON() as { type?: string; content?: string };
      if (body?.type === 'chatSend' && sent === undefined) sent = body.content;
      await route.continue();
    });

    const composer = page.locator('.qt-chat-composer-input .qt-rich-editor-content');
    await composer.click();
    await page.keyboard.type('*She waves.* Then _softly_ she speaks.');
    await page.keyboard.press('Enter');

    await expect
      .poll(() => sent, { timeout: 15_000 })
      .toBe('*She waves.* Then _softly_ she speaks.');
    await page.unroute('**/api/dispatch');
  });

  test('document and terminal panes stack in the right pane', async ({ page }) => {
    // The unwind's typed `exit` can sit in the tty typeahead for >10s while
    // the shell sources its rc files — give the beat room.
    test.setTimeout(90_000);
    await openSoloChat(page);
    await openBlankDocument(page);

    // Open the terminal too → both panes show, stacked via the vertical split.
    await page.getByRole('button', { name: 'Open terminal' }).click();
    const terminalPane = page.locator('qt-terminal-pane');
    await expect(terminalPane).toBeVisible({ timeout: 15_000 });
    await expect(page.locator('qt-document-pane')).toBeVisible();
    await expect(page.locator('.qt-doc-vertical-split-layout')).toBeVisible();

    // Unwind what this beat opened — the shared chat's pane state persists
    // server-side, and terminal-flow.spec (later in the order) walks this same
    // chat expecting a bare composer. End the shell for REAL (chat GET
    // reconciles with the live PTY probe now, and the kill's SIGTERM is v4
    // parity — an interactive shell may ignore it; a session left live here
    // would greet terminal-flow's Open-terminal click with the session picker
    // instead of a direct spawn). The typed `exit` rides the tty typeahead
    // even while the shell is still sourcing its rc files (which can take
    // >10s — nvm), so allow a generous window; the exit cascade then lands in
    // ONE of two UI shapes, per how the flush-triggered rehydrate races the
    // exit stamp: the dead-session fallback closes the pane on its own, OR
    // the pane survives and xterm prints its "[session ended …]" line.
    await expect(terminalPane.locator('.xterm')).toBeVisible({ timeout: 15_000 });
    // P4.40 deflake. The xterm canvas mounts BEFORE its WebSocket opens, and
    // the transport DROPS anything sent while the socket is not OPEN —
    // silently, by design (`terminal-stream-transport.ts:84`). So typing the
    // moment `.xterm` is visible can send NOTHING, and the beat then waits out
    // its whole `toPass` window below for a shell that never heard the `exit`.
    // That is the full-suite failure: the pane is up, the assertion is right,
    // and the gesture never happened. Reproduced deterministically by holding
    // the terminal WebSocket handshake for 8 s.
    //
    // Bytes can only REACH the pane once the same socket is open, so the
    // shell's own first output is the round trip that proves the input path is
    // live — the trap-10 shape (await the page's own downstream traffic, not
    // just the affordance that triggered it). Generous window on purpose: the
    // prompt lands only after the rc files are sourced, which is the >10 s nvm
    // case the beat's timeout already accounts for.
    await expect
      .poll(async () => (await terminalPane.locator('.xterm').innerText()).trim().length, {
        timeout: 45_000,
        intervals: [250],
      })
      .toBeGreaterThan(0);
    await terminalPane.locator('.xterm').click();
    await page.keyboard.type('exit');
    await page.keyboard.press('Enter');
    await expect(async () => {
      const pane = page.locator('qt-terminal-pane');
      if ((await pane.count()) === 0) return; // the rehydrate fallback closed it
      const text = await pane.locator('.xterm').innerText();
      expect(text).toContain('session ended');
    }).toPass({ timeout: 30_000, intervals: [500] });

    // If the pane survived the exit, kill it (two-click confirm) — a
    // client-side mode change; the shell is already dead. Then close the
    // document.
    if (await page.locator('qt-terminal-pane').count()) {
      const killBtn = terminalPane.getByRole('button', { name: 'Kill terminal and close pane' });
      await killBtn.click(); // arms the confirm
      await killBtn.click(); // confirms the kill
    }
    await expect(page.locator('qt-terminal-pane')).toHaveCount(0, { timeout: 15_000 });
    await page
      .locator('qt-document-pane')
      .getByRole('button', { name: 'Exit document mode' })
      .click();
    await expect(page.locator('qt-document-pane')).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Open terminal' })).toBeVisible();
  });

  /**
   * P4.9L2 — the pane's formatting toolbar, live.
   *
   * The transforms and the button inventory are already byte-diffed against
   * v4's own code (`editor/delimiter-transforms.spec.ts`,
   * `documents/document-pane.toolbar.spec.ts`, both over the P4.9L recorded
   * corpus). What only a browser can prove is that the CHAT'S template reaches
   * THIS pane — v4 threads `chat?.roleplayTemplateId` from `SalonView` down
   * through `DocumentPaneBinding` — and that a button press lands in the
   * document's own bytes.
   *
   * The bytes are read through the toolbar's source toggle, which shows exactly
   * what the pane would save; nothing is sent, and the chat is put back on "No
   * Template" and the template deleted, so every later spec finds the instance
   * as it was.
   */
  test('the document toolbar carries the chat’s template delimiters, and a press reaches the bytes (p4.9l2)', async ({
    page,
  }) => {
    test.setTimeout(120_000);

    const ctx = await pwRequest.newContext();
    let templateId = '';
    let chatId = '';
    try {
      // A template whose ONE delimiter is the recorder's `~` wrap — the shape
      // whose exported bytes the corpus pins (`~hello~ world`, unescaped).
      const created = await dispatch(ctx, {
        type: 'roleplayTemplateCreate',
        template: {
          name: `P4.9L2 Tilde ${Date.now()}`,
          description: null,
          systemPrompt: 'Wrap private thought in tildes.',
          narrationDelimiters: '*',
          delimiters: [
            {
              kind: 'wrap',
              name: 'Thoughts',
              buttonName: 'Th',
              style: 'qt-rp-thoughts',
              delimiters: '~',
            },
          ],
        },
      });
      templateId = ((created['template'] as { id?: string } | undefined)?.id ??
        (created as { id?: string }).id ??
        '') as string;
      expect(templateId).toBeTruthy();

      await openSoloChat(page);
      chatId = (page.url().match(/\/salon\/([0-9a-f-]{16,})/) ?? [])[1] ?? '';
      expect(chatId).toBeTruthy();

      // Hang the template on the chat, then READ IT BACK: a dispatch verb
      // ignores unknown fields and answers 200 with the unchanged entity, so
      // the round trip is the only proof the write landed.
      await dispatch(ctx, { type: 'chatUpdate', chatId, chat: { roleplayTemplateId: templateId } });
      const fetched = await dispatch(ctx, { type: 'chatGet', chatId });
      expect(((fetched['chat'] ?? fetched) as { roleplayTemplateId?: string }).roleplayTemplateId).toBe(
        templateId,
      );

      // Reload so the Salon re-fetches the chat and, with it, the template.
      await page.reload();
      await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
      const pane = await openBlankDocument(page);

      // The toolbar is there, in v4's row, with the whole markdown inventory…
      const toolbar = pane.locator('.qt-doc-toolbar .qt-formatting-toolbar');
      await expect(toolbar).toBeVisible({ timeout: 15_000 });
      await expect(toolbar.locator('.qt-formatting-button-bold')).toBeVisible();

      // …and the chat's template delimiter, with v4's `getDelimiterTooltip`
      // string byte for byte. Exactly ONE button: v4's `DocToolbar` passes no
      // `narrationDelimiters`, so the synthesized "Nar" the composer shows for
      // this same template is absent here.
      const delimiters = toolbar.locator('.qt-rp-annotation-button');
      await expect(delimiters).toHaveCount(1, { timeout: 20_000 });
      await expect(delimiters.first()).toHaveAttribute('title', 'Thoughts (~...~)');
      await expect(delimiters.first()).toHaveText('Th');

      // Write a line, select it, press the delimiter.
      const editor = pane.locator('.qt-rich-editor-content');
      await editor.click();
      await page.keyboard.type('she smiled');
      await page.keyboard.press(isMac ? 'Meta+a' : 'Control+a');
      await delimiters.first().click();

      // The document's own bytes, read through the TOOLBAR's source toggle
      // (the second control on the pane's one `showSource` signal).
      await toolbar.locator('.qt-formatting-button-source').click();
      await expect(pane.locator('textarea')).toHaveValue('~she smiled~');

      // A markdown button acts on the raw textarea while source mode is on.
      await pane.locator('textarea').click();
      await page.keyboard.press(isMac ? 'Meta+a' : 'Control+a');
      await toolbar.locator('.qt-formatting-button-bold').click();
      await expect(pane.locator('textarea')).toHaveValue('**~she smiled~**');

      // Back to rich text through the same toggle, then leave.
      await toolbar.locator('.qt-formatting-button-source').click();
      await expect(pane.locator('textarea')).toHaveCount(0);
      await pane.getByRole('button', { name: 'Exit document mode' }).click();
      await expect(page.locator('qt-document-pane')).toHaveCount(0);
    } finally {
      if (chatId) {
        await dispatch(ctx, { type: 'chatUpdate', chatId, chat: { roleplayTemplateId: null } });
      }
      if (templateId) {
        await dispatch(ctx, { type: 'roleplayTemplateDelete', templateId });
      }
      await ctx.dispose();
    }
  });
});
