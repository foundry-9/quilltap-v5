import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * The P4.D224 beats — the `@` character mention typeahead in the live Salon
 * composer (v4 `3376b3dfa`, `MentionTypeaheadPlugin`).
 *
 * These walk the whole feature end to end: the character list really is
 * fetched over the dispatch wire into the shared `characterKeys.list()` cache
 * entry, the cast comes from the chat GET's participants, the trigger is
 * detected against the live ProseMirror document, and what lands is plain text.
 *
 * The expectations are DERIVED from the instance rather than transcribed: the
 * spec reads Group Expedition's cast (non-removed participants with a
 * character — v4's predicate) and the roster over the same dispatch the SPA
 * uses, then picks a cast member whose name the Carina parser can address, so a
 * fixture rename moves the data, not the assertions. Every beat blanks the
 * draft afterwards and never presses Enter on an empty, settled menu (that
 * would SEND into the shared chat — the sender-contention trap).
 */

const isMac = process.platform === 'darwin';
const CHAT = 'Group Expedition';
/** v4 `lib/chat/carina-parser.ts`'s `NAME_SOURCE`, anchored. */
const INVOCABLE = /^[\w][\w ]*\w$/;

interface Roster {
  castIds: string[];
  castNames: string[];
  allNames: string[];
}

async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const chats = page.getByRole('heading', { name: 'Chats', exact: true });
  await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

async function openChat(page: Page): Promise<string> {
  await page.goto('/salon');
  await maybeUnlock(page);
  await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
  const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: CHAT });
  await expect(card).toBeVisible();
  await card.click();
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible();
  const match = /\/salon\/([^/?#]+)/.exec(page.url());
  expect(match, `the Salon URL should carry the chat id: ${page.url()}`).toBeTruthy();
  return match![1];
}

async function dispatch(page: Page, req: unknown): Promise<Record<string, unknown>> {
  const res = await page.request.post('/api/dispatch', { data: req });
  const body = (await res.json()) as { data?: Record<string, unknown> };
  return body.data ?? {};
}

async function roster(page: Page, chatId: string): Promise<Roster> {
  const chat = (await dispatch(page, { type: 'chatGet', chatId }))['chat'] as {
    participants?: Array<{ status?: string; character?: { id?: string; name?: string } | null }>;
  };
  const cast = (chat.participants ?? []).filter((p) => p.status !== 'removed' && p.character?.id);
  const characters = ((await dispatch(page, { type: 'characterList' }))['characters'] ?? []) as Array<{
    name?: string;
  }>;
  return {
    castIds: cast.map((p) => p.character!.id!),
    castNames: cast.map((p) => p.character!.name ?? ''),
    allNames: characters.map((c) => c.name ?? ''),
  };
}

/** A cast member Carina can address, and a query that is its first word. */
function pickCastMember(r: Roster): { name: string; query: string } {
  const name = r.castNames.find((n) => INVOCABLE.test(n));
  expect(name, `${CHAT} should seat a character with an ASCII word name`).toBeTruthy();
  return { name: name!, query: name!.split(' ')[0].slice(0, 12) };
}

function composerEditor(page: Page) {
  return page.locator('.qt-chat-composer-input .qt-rich-editor-content');
}

function menu(page: Page) {
  return page.locator('#qt-mention-typeahead-listbox');
}

function options(page: Page) {
  return menu(page).locator('[role="option"]');
}

/** Arrow the highlight onto `name` (the menu starts on row 0). */
async function highlight(page: Page, name: string): Promise<void> {
  const labels = await options(page).locator('.qt-typeahead-option-label').allTextContents();
  const index = labels.indexOf(name);
  expect(index, `the menu should offer ${name}: ${labels.join(', ')}`).toBeGreaterThanOrEqual(0);
  for (let i = 0; i < index; i += 1) await page.keyboard.press('ArrowDown');
}

async function blankComposer(page: Page): Promise<void> {
  await composerEditor(page).click();
  await page.keyboard.press(isMac ? 'Meta+a' : 'Control+a');
  await page.keyboard.press('Backspace');
  // The draft saves on an 800ms debounce; let the blank flush.
  await page.waitForTimeout(1200);
}

test.describe('Composer @ mention typeahead (P4.D224)', () => {
  test('holds Enter with the register still loading, then lists the cast first', async ({
    page,
  }) => {
    const chatId = await openChat(page);
    const r = await roster(page, chatId);
    const { query } = pickCastMember(r);

    // Hold the composer's characterList dispatch until the loading state has
    // been seen. A fresh page is a fresh query cache, so this IS the first fetch.
    let release: () => void = () => undefined;
    const held = new Promise<void>((resolve) => (release = resolve));
    let sawChatSend = false;
    await page.route('**/api/dispatch', async (route) => {
      const body = route.request().postDataJSON() as { type?: string } | null;
      if (body?.type === 'chatSend') sawChatSend = true;
      if (body?.type === 'characterList') await held;
      await route.continue();
    });

    const editor = composerEditor(page);
    await editor.click();
    await page.keyboard.type(`hello @${query.toLowerCase()}`);
    await expect(menu(page)).toContainText('Consulting the register', { timeout: 15_000 });
    await expect(menu(page)).not.toContainText('No such personage');

    // Held, not sent: the half-typed name stays in the composer.
    await page.keyboard.press('Enter');
    await expect(editor).toContainText(`hello @${query.toLowerCase()}`);
    expect(sawChatSend).toBe(false);

    release();
    await expect(options(page).first()).toBeVisible({ timeout: 15_000 });

    // Cast first, each cast row marked `in this chat`.
    const details = await options(page)
      .locator('.qt-typeahead-option-detail')
      .allTextContents();
    expect(details[0]).toBe('in this chat');
    const firstNonCast = details.findIndex((d) => d !== 'in this chat');
    if (firstNonCast >= 0) {
      expect(details.slice(firstNonCast)).not.toContain('in this chat');
    }
    await expect(options(page).locator('.qt-typeahead-option-glyph').first()).toHaveText('@');

    await page.unroute('**/api/dispatch');
    await page.keyboard.press('Escape');
    await blankComposer(page);
  });

  test('Enter mid-line completes the plain name, without the @', async ({ page }) => {
    const chatId = await openChat(page);
    const { name, query } = pickCastMember(await roster(page, chatId));

    const editor = composerEditor(page);
    await editor.click();
    await page.keyboard.type(`ask @${query.toLowerCase()}`);
    await expect(options(page).first()).toBeVisible({ timeout: 15_000 });
    await highlight(page, name);
    await page.keyboard.press('Enter');

    await expect(editor).toContainText(`ask ${name}`);
    await expect(editor).not.toContainText('@');
    await expect(menu(page)).toHaveCount(0);

    await blankComposer(page);
  });

  test('at line start the @ is kept for a Carina query, and dropped otherwise', async ({
    page,
  }) => {
    const chatId = await openChat(page);
    const { name, query } = pickCastMember(await roster(page, chatId));

    const editor = composerEditor(page);
    await editor.click();
    await page.keyboard.type(`@${query.toLowerCase()}`);
    await expect(options(page).first()).toBeVisible({ timeout: 15_000 });
    await highlight(page, name);
    await page.keyboard.press('Enter');
    // Undecided: the @ stands, and the menu does not reopen on the name.
    await expect(editor).toHaveText(`@${name}`);
    await expect(menu(page)).toHaveCount(0);

    await page.keyboard.type(': where are we');
    await expect(editor).toHaveText(`@${name}: where are we`);
    await blankComposer(page);

    await page.keyboard.type(`@${query.toLowerCase()}`);
    await expect(options(page).first()).toBeVisible({ timeout: 15_000 });
    await highlight(page, name);
    await page.keyboard.press('Enter');
    await expect(editor).toHaveText(`@${name}`);
    await page.keyboard.type(',');
    await expect(editor).toHaveText(`${name},`);

    // One undo takes back the comma and the dropped @ together.
    await page.keyboard.press(isMac ? 'Meta+z' : 'Control+z');
    await expect(editor).toHaveText(`@${name}`);

    await blankComposer(page);
  });

  test('offers Brahma at the start of a line only', async ({ page }) => {
    const chatId = await openChat(page);
    const r = await roster(page, chatId);
    test.skip(
      r.allNames.some((n) => n.trim().toLowerCase() === 'brahma'),
      'a character already answers to Brahma, so the console row is not added (v4 rule)',
    );

    const editor = composerEditor(page);
    await editor.click();
    await page.keyboard.type('@bra');
    const brahma = options(page).filter({ hasText: 'Brahma' });
    await expect(brahma).toHaveCount(1, { timeout: 15_000 });
    await expect(brahma.locator('.qt-typeahead-option-detail')).toHaveText('the Brahma Console');
    await page.keyboard.press('Escape');
    await blankComposer(page);

    await page.keyboard.type('ask @bra');
    // Mid-line the menu is open (a live trigger) but carries no Brahma row.
    await expect(menu(page)).toBeVisible({ timeout: 15_000 });
    await expect(options(page).filter({ hasText: 'Brahma' })).toHaveCount(0);
    await page.keyboard.press('Escape');
    await blankComposer(page);
  });
});
