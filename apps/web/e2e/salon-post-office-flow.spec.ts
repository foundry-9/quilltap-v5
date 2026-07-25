import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { openSidebarSection } from './support/sidebar';
import { startMockLlm, MOCK_LLM_REPLY, type MockLlm } from './support/mock-llm';

/**
 * P4.9E2B — the in-chat Post Office: the composer's megaphone and envelope, the
 * Insert Announcement and Compose Mail dialogs they open, and the Whisper dialog
 * the cast list opens.
 *
 * ORDERING: rides the shared global-setup server, so the filename must sort after
 * `foundation.spec.ts` ('sa' > 'fo'), which walks the locked→unlock gate.
 *
 * **What runs live in-lane, and what does not.** The Whisper beat is live end to
 * end today: v4's whisper is the ordinary chat send with `targetParticipantIds`,
 * which v5's spine has always carried, so it goes out through the real binary and
 * the mock LLM. The two dialogs OPEN live — their pickers ride `characterList`
 * and `connectionProfileList`, both on main — but their POSTs (`chatAnnouncement
 * Post` / `chatAnnouncementPreview` / `chatSendMail`) and Compose Mail's postbox
 * read (`chatMailboxList`) are P4.9E2A's four verbs, which do not exist on main
 * while this lane runs.
 *
 * So the two posting beats are **ACTIVATE-AT-UNIFY**: each probes the dispatch
 * surface once and annotates-and-returns when the verb is unknown, exactly as
 * `salon-custom-tools-flow.spec.ts` does. When P4.9E2A lands they self-activate
 * with no edit — the unifier only has to run them.
 *
 * The chat choice is deliberate. "Group Expedition" carries three participants
 * (Aria + Bram, LLM; Cleo, the operator's), which is what v4's whisper gate wants
 * (three or more ACTIVE) and what gives Compose Mail a sender to sign as. "Solo
 * Voyage" has two, so it is the negative case for the whisper gate.
 */

/** Is a §1 Post Office verb on the server yet? (One dispatch, no side effects.) */
async function postOfficeVerbsLive(page: Page): Promise<boolean> {
  const resp = await page.request.post('/api/dispatch', {
    // A deliberately incomplete mailbox read: a LANDED verb answers a real error
    // envelope (a missing/blank characterId), while an unlanded one fails to
    // deserialize at the union. Either way nothing is written.
    data: { type: 'chatMailboxList', chatId: 'probe', characterId: '' },
    failOnStatusCode: false,
  });
  const body = await resp.text();
  // An unknown internally-tagged variant surfaces as a serde union failure.
  return !/unknown variant|did not match any variant/i.test(body);
}

test.describe('P4.9E2B — the in-chat Post Office', () => {
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

  /** The dialogs' Cancel — the header ✕ is also named "Close", so scope to the footer. */
  function footerButton(page: Page, name: string) {
    return page.locator('[qt-modal-footer]').getByRole('button', { name, exact: true });
  }

  test('the gutter offers the megaphone and the envelope, in v4’s order', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    const actions = page.locator('.qt-chat-composer-actions');
    await expect(actions.getByRole('button', { name: 'Insert announcement' })).toBeVisible();
    await expect(actions.getByRole('button', { name: 'Post a letter' })).toBeVisible();

    // v4's grid fill order, flattened: announcement, mail, camera, paperclip.
    // (Library file is p4.9e3; RNG has no v5 server verb — both named deferrals.)
    const labels = await actions.locator('button').evaluateAll((nodes) =>
      nodes
        .map((n) => n.getAttribute('aria-label'))
        .filter(
          (l) =>
            l === 'Insert announcement' ||
            l === 'Post a letter' ||
            l === 'Generate image' ||
            l === 'Attach a file',
        ),
    );
    expect(labels).toEqual([
      'Insert announcement',
      'Post a letter',
      'Generate image',
      'Attach a file',
    ]);
  });

  test('Insert Announcement opens with the staff roster and the off-scene picker', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    await page.getByRole('button', { name: 'Insert announcement' }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog.getByText('Insert Announcement')).toBeVisible();

    // Staff is the opening arm, with v4's ten names and The Host selected.
    const staff = page.locator('#announce-staff');
    await expect(staff).toBeVisible();
    await expect(staff.locator('option')).toHaveCount(10);
    await expect(staff).toHaveValue('host');
    await expect(staff.locator('option', { hasText: 'The Commonplace Book' })).toHaveCount(1);

    // The off-scene arm loads the LIVE character list and filters the scene's own
    // cast out. Group Expedition holds Aria, Bram and Cleo, so Dax remains.
    await page.getByRole('tab', { name: 'Off-scene character' }).click();
    await expect(page.locator('#announce-character-search')).toBeVisible();
    const picker = dialog.locator('.max-h-40');
    await expect(picker.getByText('Dax', { exact: true })).toBeVisible({ timeout: 15_000 });
    await expect(picker.getByText('Aria', { exact: true })).toHaveCount(0);

    await footerButton(page, 'Cancel').click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
  });

  test('Compose Mail opens signed by the operator’s character, addressed to the roster', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    await page.getByRole('button', { name: 'Post a letter' }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog.getByText('Compose Mail')).toBeVisible();

    // Cleo is the only player-character in this scene, so the sender is locked.
    const from = page.locator('#mail-from');
    await expect(from).toBeVisible();
    await expect(from).toBeDisabled();
    await expect(from.locator('option')).toHaveText(['Cleo']);

    // The recipient list is the whole LIVE roster minus the sender — including
    // Dax, who is not in this scene at all.
    const to = page.locator('#mail-to');
    await expect(to).toBeVisible({ timeout: 15_000 });
    const recipients = await to.locator('option').allTextContents();
    expect(recipients.map((r) => r.trim())).toContain('Dax');
    expect(recipients.map((r) => r.trim())).not.toContain('Cleo');

    // The postbox read is P4.9E2A's; until it lands the dropdown holds v4's
    // default alone, which is exactly how v4's own errored query degrades.
    await expect(page.locator('#mail-reply option').first()).toHaveText('No quoted reply.');

    await footerButton(page, 'Cancel').click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
  });

  test('whisper a private line to one character, over the real send spine', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    // (`openSidebarSection` expands the strip itself — clicking Expand here too
    // races it: the helper's own `count()` probe then chases a button that has
    // already gone.)
    await openSidebarSection(page, 'Participants');

    const whisper = page.getByRole('button', { name: 'Whisper to Aria' });
    await expect(whisper).toBeVisible();
    await whisper.click();

    const dialog = page.getByRole('dialog');
    await expect(dialog.getByText('Whisper to Aria')).toBeVisible();
    const box = dialog.locator('textarea');
    const line = `Meet me on the ridge ${Date.now()}.`;
    await box.fill(line);
    await dialog.getByRole('button', { name: 'Whisper', exact: true }).click();

    // v4's sequencing: the dialog closes at once, and the turn runs on behind it.
    await expect(page.getByRole('dialog')).toHaveCount(0);

    // The whispered line lands in the transcript, labelled with its target — the
    // operator always sees their own whispers whatever the All-Whispers toggle
    // says.
    await expect(page.locator('.qt-chat-messages-list')).toContainText(line, { timeout: 30_000 });
    await expect(
      page.locator('.qt-chat-whisper-label', { hasText: 'whispered to Aria' }).first(),
    ).toBeVisible({ timeout: 30_000 });

    // Collapse again so sibling specs find the strip they expect.
    await page.getByRole('button', { name: 'Collapse chat sidebar' }).click();
  });

  test('the whisper affordance is withheld below three active participants (v4’s gate)', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Solo Voyage');

    await openSidebarSection(page, 'Participants');
    // Two in the room — everything said here is already private.
    await expect(page.locator('.qt-participant-card-name').first()).toBeVisible();
    await expect(page.getByRole('button', { name: /^Whisper to / })).toHaveCount(0);

    await page.getByRole('button', { name: 'Collapse chat sidebar' }).click();
  });

  test('post a staff announcement into the scene', async ({ page }, testInfo) => {
    // ACTIVATE-AT-UNIFY: needs P4.9E2A's `chatAnnouncementPost`.
    await page.goto('/salon');
    await maybeUnlock(page);
    if (!(await postOfficeVerbsLive(page))) {
      testInfo.annotations.push({
        type: 'activate-at-unification',
        description:
          'Posting an announcement needs P4.9E2A’s chatAnnouncementPost verb on main.',
      });
      return;
    }

    await openChat(page, 'Group Expedition');
    await page.getByRole('button', { name: 'Insert announcement' }).click();
    await expect(page.locator('#announce-staff')).toBeVisible();
    await page.locator('#announce-staff').selectOption('librarian');

    const line = `The reading room closes at dusk ${Date.now()}.`;
    await page.locator('.qt-markdown-field .qt-rich-editor-content').click();
    await page.keyboard.type(line);

    await footerButton(page, 'Post Announcement').click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await expect(page.getByText('Announcement posted')).toBeVisible({ timeout: 15_000 });
    await expect(page.locator('.qt-chat-messages-list')).toContainText(line, { timeout: 15_000 });
  });

  test('post a letter as the operator’s character', async ({ page }, testInfo) => {
    // ACTIVATE-AT-UNIFY: needs P4.9E2A's `chatSendMail` (and `chatMailboxList`
    // for the postbox dropdown, which stays at its default here).
    await page.goto('/salon');
    await maybeUnlock(page);
    if (!(await postOfficeVerbsLive(page))) {
      testInfo.annotations.push({
        type: 'activate-at-unification',
        description: 'Posting a letter needs P4.9E2A’s chatSendMail verb on main.',
      });
      return;
    }

    await openChat(page, 'Group Expedition');
    await page.getByRole('button', { name: 'Post a letter' }).click();
    await expect(page.locator('#mail-to')).toBeVisible({ timeout: 15_000 });
    await page.locator('#mail-to').selectOption({ label: 'Bram' });

    await page.locator('.qt-markdown-field .qt-rich-editor-content').click();
    await page.keyboard.type(`Dear Bram, the ridge is clear. ${Date.now()}`);

    await footerButton(page, 'Send').click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    // v4's own delivery copy, down to the diacritic.
    await expect(page.getByText('Suparṇā has the letter and is already aloft.')).toBeVisible({
      timeout: 15_000,
    });
  });
});
