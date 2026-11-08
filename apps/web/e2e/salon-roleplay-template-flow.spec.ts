import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { openSidebarSection } from './support/sidebar';
import { startMockLlm, MOCK_LLM_REPLY, type MockLlm } from './support/mock-llm';
import { expectTooltip } from './support/tooltips';

/**
 * P4.30 — the chat's roleplay template reaching message rendering, live.
 *
 * Until this lane nothing in v5 ever read the chat's template, so every message
 * everywhere drew with the built-in defaults. This walks the whole wire against
 * the real server: create a template whose narration delimiter is `%`, hang it
 * on a chat from the sidebar, and watch a message already on screen re-dress
 * itself with NO reload — which is also the mid-session reconcile beat (v4's
 * effect re-runs when `chat.roleplayTemplateId` moves).
 *
 * The proof is the pair, not either half: with no template the `%…%` run is
 * plain text; with the template it wears `.qt-chat-narration`; remove the
 * template and it is plain again. `%` is chosen because no DEFAULT pattern
 * touches it and markdown gives it no meaning of its own — so the class can only
 * have come from the fetched template.
 *
 * The line is authored by sending it through the mock LLM the M4 beats use (the
 * operator's own bubble renders through the identical path). It deliberately
 * contains NO `*asterisks*`: the composer is a ProseMirror editor with
 * emphasis-on-type input rules (P4.6al), and this beat is about the template,
 * not about what the editor does to a star while you type. The "a custom set
 * REPLACES the defaults" arm — the other half of v4
 * `MessageContent.tsx:333-338` — is pinned by the captured v4 parity vectors
 * and by `message-list.spec.ts` instead.
 *
 * ORDERING: rides the shared global-setup server, so the filename must sort
 * after `foundation.spec.ts` ('sa' > 'fo'), which walks the locked→unlock gate.
 *
 * CHAT CHOICE: "Group Expedition" is the room that may safely GROW — "Solo
 * Voyage" carries the token-total baselines (the P4.d9 lesson).
 *
 * CLEANUP: the chat is put back on "No Template" and the template is deleted, so
 * every later spec finds the instance as it was.
 */

const TEMPLATE_NAME = 'P4.30 Percent Narration';

/**
 * ACTIVATE-AT-UNIFY (P4.D352, ruling R-5): the Novel beats need the server half
 * of v4 `778b97b2b` — the sibling lanes' work, wired at unification (C7).
 * Named constants, never capability probes: an unknown template key is not an
 * "unknown variant", it is dropped or refused (`round-plan-takeaways`).
 *
 * - `P4D351_SERVER_LANDED` — the template POST / PUT accepting
 *   `narrationDelimiters: null`, `unmarkedStyle`, `markdownAsterisks` and the
 *   two `wrap` flags, and regenerating the five-scope patterns (P4.D351, on
 *   P4.D341's columns). The composer half of the beat (prose mode, the Bold
 *   withhold) also needs P4.D344 to apply this lane's `salon-conversation.ts`
 *   HANDOFF H-2 — both are in the union.
 * - `P4D341_LANDED` — the seeded built-in Novel row (P4.D341 `KEYSTONE`).
 */
const P4D351_SERVER_LANDED = true;
const P4D351_GATE_REASON =
  'ACTIVATE-AT-UNIFY: needs P4.D351 Novel template routes + P4.D344 H-2 (wired at unification)';
const P4D341_LANDED = true;
const P4D341_GATE_REASON =
  'ACTIVATE-AT-UNIFY: needs P4.D341 seeded Novel built-in (wired at unification)';

const NOVEL_TEMPLATE_NAME = 'P4.D352 Novel-shaped';
const NOVEL_LINE = 'She set the cup down. "You came," she said. *Finally.* **Loud.** ((aside))';
const isMac = process.platform === 'darwin';
const MARKER = 'A test line with %percent narration% inside it.';

test.describe('P4.30 — roleplay-template rendering', () => {
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

  /** Create the `%` narration template from the Templates tab (shared by both beats). */
  async function createPercentTemplate(page: Page) {
    await page.goto('/settings?tab=templates');
    await expect(page.getByRole('heading', { name: 'My Templates' })).toBeVisible({
      timeout: 15_000,
    });
    await page.getByRole('button', { name: 'Create Template', exact: true }).first().click();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();
    await dialog.getByPlaceholder('My Custom RP Style').fill(TEMPLATE_NAME);
    // The LLM prompt is a qt-markdown-field (ProseMirror contenteditable), so it
    // takes real key events — `fill()` targets input/textarea.
    await dialog.locator('.qt-rich-editor-content').first().click();
    await page.keyboard.type('Mark narration with percent signs.');
    // The single-delimiter input (default `*`) — the server turns this into the
    // template's one `qt-chat-narration` wrap pattern (`generateRenderingPatterns`).
    await dialog.getByPlaceholder('*').fill('%');
    await dialog.getByRole('button', { name: 'Create Template', exact: true }).click();
    await expect(dialog).toBeHidden({ timeout: 15_000 });
    await expect(page.locator('div.qt-card', { hasText: TEMPLATE_NAME })).toBeVisible({
      timeout: 15_000,
    });
  }

  /** Put the instance back as it was. */
  async function deletePercentTemplate(page: Page) {
    await page.goto('/settings?tab=templates');
    const card = page.locator('div.qt-card', { hasText: TEMPLATE_NAME });
    await expect(card).toBeVisible({ timeout: 15_000 });
    await card.getByRole('button', { name: 'Delete', exact: true }).click();
    await card.getByRole('button', { name: 'Confirm', exact: true }).click();
    await expect(page.locator('div.qt-card', { hasText: TEMPLATE_NAME })).toHaveCount(0, {
      timeout: 15_000,
    });
  }

  /** The chat sidebar's template picker. */
  function templatePicker(page: Page) {
    return page
      .locator('qt-chat-sidebar label', { hasText: 'Roleplay Template' })
      .locator('select');
  }

  /** The bubble carrying the marker line — the one row every assertion reads. */
  function markerBody(page: Page) {
    return page.locator('.qt-chat-message-content').filter({ hasText: 'percent narration' }).last();
  }

  /** The narration spans inside that one bubble. */
  function narrationSpans(page: Page) {
    return markerBody(page).locator('span.qt-chat-narration');
  }

  test('a chat renders with its template’s patterns, and reverts when it is removed', async ({
    page,
  }) => {
    test.setTimeout(180_000);

    // --- 1. Create the template (single narration delimiter `%`) -------------
    // Unlock on /salon: the gate's landing is the chat list, so that is where
    // both the passphrase form and the unlocked state are recognisable. The
    // server holds the pepper process-wide once unlocked.
    await page.goto('/salon');
    await maybeUnlock(page);

    await createPercentTemplate(page);

    // --- 2. Author the marker line in the chat -------------------------------
    await page.goto('/salon');
    await openChat(page, 'Group Expedition');

    const composer = page.locator('.qt-chat-composer-input .qt-rich-editor-content');
    await composer.click();
    await page.keyboard.type(MARKER);
    await page.keyboard.press('Enter');
    await expect(markerBody(page)).toBeVisible({ timeout: 30_000 });
    // Let the turn settle before touching the sidebar, so the canonical refetch
    // is not racing the template write.
    await expect(page.getByText(MOCK_LLM_REPLY).first()).toBeVisible({ timeout: 60_000 });

    // --- 3. With NO template: the `%…%` run is plain text --------------------
    await expect(narrationSpans(page)).toHaveCount(0);

    // --- 4. Hang the template on the chat — no reload ------------------------
    await openSidebarSection(page, 'Chat');
    const templateSelect = templatePicker(page);
    await expect(templateSelect).toBeVisible({ timeout: 10_000 });
    await templateSelect.selectOption({ label: TEMPLATE_NAME });

    // The same bubble, re-rendered through the fetched template.
    await expect(narrationSpans(page)).toHaveCount(1, { timeout: 20_000 });
    await expect(narrationSpans(page).first()).toHaveText('%percent narration%');

    // --- 5. Remove it: the run goes back to plain text -----------------------
    await templateSelect.selectOption({ label: 'No Template' });
    await expect(narrationSpans(page)).toHaveCount(0, { timeout: 20_000 });

    // --- 6. Clean up so later specs find the instance as it was --------------
    await deletePercentTemplate(page);
  });

  /**
   * P4.D352 — a Novel-shaped template built through the editor (v4 `778b97b2b`):
   * unmarked narration set in italics, quoted dialogue upright, a single `*` as
   * concealed bold-italic emphasis, `**` dropped, `((asides))` out of character
   * — on the operator's own bubble (the identical render path a model reply
   * takes, this file's header rule). The composer previews prose and withholds
   * Bold while the template is on the chat.
   */
  test('a Novel-shaped template renders the operator’s own bubble as a novel prints it, and the toolbar withholds Bold', async ({
    page,
  }) => {
    test.skip(!P4D351_SERVER_LANDED, P4D351_GATE_REASON);
    test.setTimeout(240_000);

    await page.goto('/salon');
    await maybeUnlock(page);

    // --- 1. Build the template through the modal --------------------------
    await page.goto('/settings?tab=templates');
    await expect(page.getByRole('heading', { name: 'My Templates' })).toBeVisible({
      timeout: 15_000,
    });
    await page.getByRole('button', { name: 'Create Template', exact: true }).first().click();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();
    await dialog.getByPlaceholder('My Custom RP Style').fill(NOVEL_TEMPLATE_NAME);
    await dialog.locator('.qt-rich-editor-content').first().click();
    await page.keyboard.type('Write it as a novel prints it.');

    await dialog.getByRole('radio', { name: 'None — plain prose' }).check();
    await expect(dialog.getByLabel('Style unmarked text')).toBeChecked();
    // The unmarked flourishes: Italic. Every flourish box is matched EXACTLY:
    // a substring match also takes the `Markdown reads asterisks as bold and
    // italic…` box, which sits between the unmarked flourishes and the
    // delimiters' and shifted every `nth` below onto the wrong delimiter.
    await dialog.getByRole('checkbox', { name: 'Italic', exact: true }).first().check();
    await dialog.getByRole('checkbox', { name: /Markdown reads asterisks/ }).uncheck();

    const addDelimiter = dialog.getByRole('button', { name: '+ Add Delimiter' });
    // Dialogue: a `"` wrap, curly-tolerant and open-ended.
    await addDelimiter.click();
    await dialog.getByLabel('Name', { exact: true }).nth(0).fill('Dialogue');
    await dialog.getByLabel('Button Label', { exact: true }).nth(0).fill('Dia');
    await dialog.getByLabel('Style (CSS class)', { exact: true }).nth(1).fill('qt-chat-dialogue');
    await dialog.getByLabel('Delimiter', { exact: true }).nth(0).fill('"');
    await dialog
      .getByRole('checkbox', { name: /Straight quotes also match curly ones/ })
      .nth(0)
      .check();
    await dialog
      .getByRole('checkbox', { name: /May run unclosed/ })
      .nth(0)
      .check();
    // Emphasis: a `*` wrap, concealed, bold + italic.
    await addDelimiter.click();
    await dialog.getByLabel('Name', { exact: true }).nth(1).fill('Emphasis');
    await dialog.getByLabel('Button Label', { exact: true }).nth(1).fill('Emph');
    await dialog.getByLabel('Style (CSS class)', { exact: true }).nth(2).fill('qt-roleplay-plain');
    await dialog.getByLabel('Delimiter', { exact: true }).nth(1).fill('*');
    await dialog.getByRole('checkbox', { name: 'Conceal the marks when rendered' }).nth(1).check();
    await dialog.getByRole('checkbox', { name: 'Bold', exact: true }).nth(2).check();
    await dialog.getByRole('checkbox', { name: 'Italic', exact: true }).nth(2).check();
    // OOC: a `((` / `))` pair.
    await addDelimiter.click();
    await dialog.getByLabel('Name', { exact: true }).nth(2).fill('Out of Character');
    await dialog.getByLabel('Button Label', { exact: true }).nth(2).fill('OOC');
    await dialog.getByLabel('Style (CSS class)', { exact: true }).nth(3).fill('qt-chat-ooc');
    await dialog.getByRole('radio', { name: 'Pair' }).nth(2).check();
    await dialog.getByLabel('Opening delimiter').fill('((');
    await dialog.getByLabel('Closing delimiter').fill('))');

    await dialog.getByRole('button', { name: 'Create Template', exact: true }).click();
    await expect(dialog).toBeHidden({ timeout: 15_000 });

    try {
      // --- 2. Hang it on the chat, compose under it --------------------------
      await page.goto('/salon');
      await openChat(page, 'Group Expedition');
      await openSidebarSection(page, 'Chat');
      await templatePicker(page).selectOption({ label: NOVEL_TEMPLATE_NAME });

      const modeToggle = page.getByRole('button', { name: /^Switch to (chat|composition) mode$/ });
      await modeToggle.click();
      await expect(modeToggle).toHaveAttribute('aria-pressed', 'true');
      const toolbar = page.locator('.qt-chat-composer .qt-formatting-toolbar');
      await expect(toolbar).toBeVisible();
      // `markdownAsterisks: false` — `**` means nothing here, so no Bold.
      await expect(toolbar.locator('button.qt-formatting-button-bold')).toHaveCount(0, {
        timeout: 20_000,
      });

      // Prose mode: narration previews, and with no quote typed the draft is
      // tinted for having no dialogue at all.
      const composer = page.locator('.qt-chat-composer-input .qt-rich-editor-content');
      await composer.click();
      await page.keyboard.type('She waited by the door.');
      await expect(composer.locator('span.qt-composer-narration')).toHaveCount(1);
      await expect(composer.locator('span.qt-punctuation-no-dialogue')).toHaveCount(1);
      await page.keyboard.press(isMac ? 'Meta+a' : 'Control+a');
      await page.keyboard.press('Backspace');

      // --- 3. Send the Novel line through the source textarea ----------------
      await toolbar.locator('.qt-formatting-button-source').click();
      const source = page.locator('.qt-chat-composer .qt-source-mode-textarea');
      await expect(source).toBeVisible();
      await source.fill(NOVEL_LINE);
      await page.getByRole('button', { name: 'Send message' }).click();

      const bubble = page
        .locator('.qt-chat-message-content')
        .filter({ hasText: 'She set the cup down' })
        .last();
      await expect(bubble).toBeVisible({ timeout: 30_000 });
      await expect(page.getByText(MOCK_LLM_REPLY).first()).toBeVisible({ timeout: 60_000 });

      const narration = bubble.locator('span.qt-roleplay-plain.qt-rp-italic').first();
      await expect(narration).toContainText('She set the cup down.');
      const dialogue = bubble.locator('span.qt-chat-dialogue');
      await expect(dialogue.first()).toContainText('You came,');
      await expect(dialogue.first()).not.toHaveClass(/qt-rp-italic/);
      const emphasis = bubble.locator('span.qt-roleplay-plain.qt-rp-bold.qt-rp-italic');
      await expect(emphasis).toHaveText('Finally.');
      await expect(bubble).not.toContainText('*Finally');
      await expect(bubble).toContainText('Loud.');
      await expect(bubble).not.toContainText('**');
      await expect(bubble.locator('strong')).toHaveCount(0);
      await expect(bubble.locator('span.qt-chat-ooc')).toContainText('aside');

      // --- 4. Remove the template: Bold comes back --------------------------
      await openSidebarSection(page, 'Chat');
      await templatePicker(page).selectOption({ label: 'No Template' });
      await expect(toolbar.locator('button.qt-formatting-button-bold')).toHaveCount(1, {
        timeout: 20_000,
      });
      await modeToggle.click();
      await expect(modeToggle).toHaveAttribute('aria-pressed', 'false');
    } finally {
      // Composition mode is a per-chat SERVER flag (`documentEditingMode`): a
      // beat that dies with it on leaves Enter inserting newlines in the shared
      // Group Expedition for every later spec (the `236c04e35` full run's
      // smart-typography red). Put it back before anything else.
      const toggle = page.getByRole('button', { name: /^Switch to (chat|composition) mode$/ });
      if ((await toggle.count()) && (await toggle.getAttribute('aria-pressed')) === 'true') {
        await toggle.click();
        await expect(toggle).toHaveAttribute('aria-pressed', 'false');
      }
      await page.goto('/settings?tab=templates');
      const card = page.locator('div.qt-card', { hasText: NOVEL_TEMPLATE_NAME });
      if (await card.count()) {
        await card.getByRole('button', { name: 'Delete', exact: true }).click();
        await card.getByRole('button', { name: 'Confirm', exact: true }).click();
        await expect(card).toHaveCount(0, { timeout: 15_000 });
      }
    }
  });

  test('the built-in Novel template is listed and can be copied (P4.D352)', async ({ page }) => {
    test.skip(!P4D341_LANDED, P4D341_GATE_REASON);

    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/settings?tab=templates');
    const card = page
      .locator('div.qt-card', { hasText: 'Novel' })
      .filter({ has: page.getByRole('button', { name: 'Copy as New' }) });
    await expect(card).toBeVisible({ timeout: 15_000 });
    await card.getByRole('button', { name: 'Copy as New' }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog.getByPlaceholder('My Custom RP Style')).toHaveValue('Novel (Copy)');
    await expect(dialog.getByRole('radio', { name: 'None — plain prose' })).toBeChecked();
    await expect(dialog.getByLabel('Style unmarked text')).toBeChecked();
    await expect(
      dialog.getByRole('checkbox', { name: /Markdown reads asterisks/ }),
    ).not.toBeChecked();
    await dialog.getByRole('button', { name: 'Cancel' }).click();
  });

  /**
   * P4.9L — the composer toolbar's DELIMITER buttons, live.
   *
   * The transforms are already byte-diffed against v4's own code
   * (`editor/delimiter-transforms.spec.ts`). What only a browser can prove is
   * that the template a chat wears actually reaches the toolbar and that the
   * button's output reaches the SERIALIZED markdown — read here through the
   * source-mode toggle, which shows the very bytes a send would carry. Nothing
   * is sent: a send from this chat moves state later specs read (trap 5b), and
   * the bytes are the contract either way.
   */
  test('the composer toolbar carries the template’s narration button, and its bytes reach the markdown (p4.9l)', async ({
    page,
  }) => {
    test.setTimeout(180_000);

    await page.goto('/salon');
    await maybeUnlock(page);
    await createPercentTemplate(page);

    await page.goto('/salon');
    await openChat(page, 'Group Expedition');

    // Composition mode is where v4 shows the toolbar at all.
    const modeToggle = page.getByRole('button', { name: /^Switch to (chat|composition) mode$/ });
    await modeToggle.click();
    await expect(modeToggle).toHaveAttribute('aria-pressed', 'true');

    const toolbar = page.locator('.qt-chat-composer .qt-formatting-toolbar');
    await expect(toolbar).toBeVisible();
    // No template yet: markdown buttons, no delimiter section.
    await expect(toolbar.locator('.qt-rp-annotation-button')).toHaveCount(0);

    // Hang the template on the chat — the toolbar picks it up with no reload.
    await openSidebarSection(page, 'Chat');
    const templateSelect = templatePicker(page);
    await expect(templateSelect).toBeVisible({ timeout: 10_000 });
    await templateSelect.selectOption({ label: TEMPLATE_NAME });

    const narration = toolbar.locator('.qt-rp-annotation-button');
    await expect(narration).toHaveCount(1, { timeout: 20_000 });
    // v4's `getDelimiterTooltip` string, byte for byte.
    await expectTooltip(narration.first(), 'Narration (%...%)');
    await expect(narration.first()).toHaveText('Nar');

    // Type a line, select it, press the button.
    const composer = page.locator('.qt-chat-composer-input .qt-rich-editor-content');
    await composer.click();
    await page.keyboard.type('she smiled');
    await page.keyboard.press(isMac ? 'Meta+a' : 'Control+a');
    await narration.first().click();

    // The SERIALIZED markdown — what a send would carry — read through the
    // source toggle rather than by sending.
    await toolbar.locator('.qt-formatting-button-source').click();
    const source = page.locator('.qt-chat-composer .qt-source-mode-textarea');
    await expect(source).toBeVisible();
    await expect(source).toHaveValue('%she smiled%');

    // Back to rich text, then leave the composer and the instance as found.
    await toolbar.locator('.qt-formatting-button-source').click();
    await expect(page.locator('.qt-chat-composer .qt-source-mode-textarea')).toHaveCount(0);
    await composer.click();
    await page.keyboard.press(isMac ? 'Meta+a' : 'Control+a');
    await page.keyboard.press('Backspace');
    await page.waitForTimeout(1200); // the 800ms draft debounce

    await modeToggle.click();
    await expect(modeToggle).toHaveAttribute('aria-pressed', 'false');
    await openSidebarSection(page, 'Chat');
    await templatePicker(page).selectOption({ label: 'No Template' });
    await deletePercentTemplate(page);
  });
});
