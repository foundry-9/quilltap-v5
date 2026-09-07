import { expect, request, test, type APIRequestContext, type Page } from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';
import { openSidebarSection } from './support/sidebar';

/**
 * P4.D165 — character subprompts end to end (v4 `2f4254b42`).
 *
 * ACTIVATE-AT-UNIFY behind {@link P4D163_SERVER_LANDED}: the five
 * `characterSubprompt*` dispatch verbs and the participant field are P4.D163 /
 * P4.D164's (sibling lanes in this round, not on this branch). A NAMED
 * constant, not a capability probe — a probe cannot tell a verb that is
 * DEFINED-but-refusing from one that genuinely answers, and would silently
 * activate these beats into a guaranteed failure (the standing e2e rule,
 * `character-archive-flow.spec.ts` precedent).
 *
 * The walk seeds nothing up front: beat (a) CREATES the vault's first subprompt
 * through the UI and deletes it again, so the Subprompts folder is back to
 * empty before beat (b) asserts `None on file`. Beat (b)'s record is torn down
 * through the API in the same beat's tail, so nothing this file writes outlives
 * it. That matters because the instance is shared with every other spec (the
 * `e2e-playwright-traps` coupling note).
 *
 * The character is **Bram**, the salon fixture's plain LLM seat: Aria carries
 * the default systemPrompt several specs key off, and Dax is what
 * `character-rename-flow` renames. Nothing asserts Bram's prompts, and Bram is
 * an LLM participant of the shared GROUP chat, which is what beats (c) and (d)
 * need.
 *
 * The order's beats (b) and (c) share ONE test on purpose: (c) unticks the
 * selection (b) made, on the chat (b) created, so splitting them would couple
 * two tests through server state — the very thing the traps note warns about.
 * (d) is its own test against the shared GROUP chat, because that is the one
 * screen carrying BOTH an LLM seat and a user-controlled one; asserting the
 * absence beside the presence is what keeps the negative from being vacuous.
 *
 * ⚠ STILL OWED before the first live run (flip {@link P4D163_SERVER_LANDED}):
 * nothing beyond the verbs. Every id the walk needs it reads back from the
 * server; the vault every character carries is minted by the fixture builder.
 */
const P4D163_SERVER_LANDED = true;

const CHARACTER = 'Bram';
const SUBPROMPT_TITLE = 'Be terse';
const SUBPROMPT_BODY = 'You keep every reply under three sentences unless asked for more.';
const RENAMED_TITLE = 'Be terser still';
const IN_CHAT_TITLE = 'No spoilers';

async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const chats = page.getByRole('heading', { name: 'Chats', exact: true });
  await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
    await expect(chats).toBeVisible({ timeout: 15_000 });
  }
}

async function dispatch(ctx: APIRequestContext, req: unknown): Promise<Record<string, unknown>> {
  const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: req });
  const body = (await res.json().catch(() => null)) as { data?: Record<string, unknown> } | null;
  return body?.data ?? {};
}

/** The fixture character's id, read back rather than transcribed. */
async function characterId(ctx: APIRequestContext, name: string): Promise<string> {
  const data = await dispatch(ctx, { type: 'characterList' });
  const rows = (data['characters'] as Array<{ id: string; name?: string }> | undefined) ?? [];
  const found = rows.find((c) => c.name === name);
  expect(found, `the fixture should carry a character named ${name}`).toBeTruthy();
  return found!.id;
}

/** The seat's persisted selection, read through the chat GET's enrichment. */
async function seatSubprompts(
  ctx: APIRequestContext,
  chatId: string,
  name: string,
): Promise<string[] | undefined> {
  const data = await dispatch(ctx, { type: 'chatGet', chatId });
  const chat = (data['chat'] as Record<string, unknown> | undefined) ?? data;
  const participants =
    (chat['participants'] as
      Array<{ character?: { name?: string }; selectedSubpromptIds?: string[] }> | undefined) ?? [];
  return participants.find((p) => p.character?.name === name)?.selectedSubpromptIds;
}

async function openCharacterEditor(page: Page, name: string): Promise<void> {
  await page.goto('/salon');
  await maybeUnlock(page);
  await page.goto('/characters');
  await expect(page.getByRole('heading', { name: 'Characters', exact: true })).toBeVisible({
    timeout: 15_000,
  });
  await page.locator('qt-character-card').filter({ hasText: name }).first().locator('h2').click();
  await expect(page.getByRole('heading', { name, level: 1 })).toBeVisible({ timeout: 15_000 });
  await page.getByRole('link', { name: /Edit Character/i }).click();
  await page.getByRole('button', { name: 'System Prompts' }).click();
  await expect(page.getByRole('heading', { name: 'Subprompts', exact: true })).toBeVisible({
    timeout: 15_000,
  });
}

test.describe('P4.D165 — character subprompts', () => {
  test('(a) Aurora: create, rename without moving the file, and delete through the inline confirm', async ({
    page,
  }) => {
    test.skip(!P4D163_SERVER_LANDED, 'awaits P4.D163: the five characterSubprompt* verbs');

    await openCharacterEditor(page, CHARACTER);
    await expect(
      page.getByText('No subprompts yet. Add one to have it on offer when a chat begins.'),
    ).toBeVisible();

    await page.getByRole('button', { name: '+ Add Subprompt' }).click();
    await expect(page.getByText(`New Subprompt for ${CHARACTER}`)).toBeVisible();
    await page.locator('#subprompt-title').fill(SUBPROMPT_TITLE);
    await page.locator('qt-rich-editor [contenteditable="true"]').first().fill(SUBPROMPT_BODY);
    await page.getByRole('button', { name: 'Create', exact: true }).click();

    // The card names the file the vault now holds — the id is the slugged title.
    await expect(page.getByText('Subprompts/be-terse.md')).toBeVisible({ timeout: 15_000 });
    await expect(page.getByText(SUBPROMPT_TITLE, { exact: true })).toBeVisible();
    await expect(page.getByText('Subprompt created')).toBeVisible();

    // Rename: the TITLE moves, the file name does NOT — which is the whole
    // reason a chat's selection survives an edit.
    await page.getByRole('button', { name: `Edit subprompt ${SUBPROMPT_TITLE}` }).click();
    await expect(page.getByText(`Edit Subprompt — ${CHARACTER}`)).toBeVisible();
    await expect(page.getByText('Kept on file as')).toBeVisible();
    await page.locator('#subprompt-title').fill(RENAMED_TITLE);
    await page.getByRole('button', { name: 'Update', exact: true }).click();

    await expect(page.getByText('Subprompt updated')).toBeVisible();
    await expect(page.getByText(RENAMED_TITLE, { exact: true })).toBeVisible({ timeout: 15_000 });
    await expect(page.getByText('Subprompts/be-terse.md')).toBeVisible();

    // Delete through the inline popover, not a modal.
    await page.getByRole('button', { name: `Delete subprompt ${RENAMED_TITLE}` }).click();
    await expect(
      page.getByText('Delete this subprompt? Any chat with it in play drops it.'),
    ).toBeVisible();
    await page.locator('qt-subprompts-section').getByRole('button', { name: 'Delete', exact: true }).click();

    await expect(page.getByText('Subprompt deleted')).toBeVisible();
    await expect(
      page.getByText('No subprompts yet. Add one to have it on offer when a chat begins.'),
    ).toBeVisible({
      timeout: 15_000,
    });
  });

  test('(b + c) New Chat: None on file → create in place → auto-ticked → persisted, then untick it in the Salon', async ({
    page,
  }) => {
    test.skip(!P4D163_SERVER_LANDED, 'awaits P4.D163: the five characterSubprompt* verbs');

    const ctx = await request.newContext();
    try {
      await page.goto('/salon');
      await maybeUnlock(page);
      await page.getByRole('link', { name: 'New Chat' }).first().click();
      await expect(page.getByRole('heading', { name: 'New Chat', exact: true })).toBeVisible();

      // Pick Bram out of the roster; its card grows the picker.
      await page
        .locator('.new-chat-character-picker button')
        .filter({ hasText: CHARACTER })
        .first()
        .click();

      const picker = page.locator('qt-subprompt-picker').first();
      await expect(picker.getByText('Subprompts · None on file')).toBeVisible({ timeout: 15_000 });

      // Create one from inside the picker — v4's whole point: no trip to Aurora.
      await picker.getByRole('button', { name: 'Subprompts · None on file' }).click();
      await picker.getByRole('button', { name: 'New subprompt…' }).click();
      await page.locator('#subprompt-title').fill(IN_CHAT_TITLE);
      await page
        .locator('qt-rich-editor [contenteditable="true"]')
        .first()
        .fill('You never reveal the ending.');
      await page.getByRole('button', { name: 'Create', exact: true }).click();

      // Auto-ticked: the summary counts it in play without a second click.
      await expect(picker.getByText('Subprompts · 1 of 1 in play')).toBeVisible({
        timeout: 15_000,
      });

      await page.getByRole('button', { name: 'Create Chat' }).click();
      await expect(page).toHaveURL(/\/salon\/[0-9a-f-]{16,}/, { timeout: 20_000 });
      const chatId = page.url().split('/').pop()!;

      // The proof is the PERSISTED seat, never the optimistic UI.
      expect(await seatSubprompts(ctx, chatId, CHARACTER)).toEqual(['no-spoilers']);

      // ---- (c) the Salon card: untick → the toast → the seat reads back empty.
      await openSidebarSection(page, 'Participants');
      const card = page.locator('qt-participant-card').filter({ hasText: CHARACTER }).first();
      const sidebarPicker = card.locator('qt-subprompt-picker');
      await sidebarPicker.getByRole('button', { name: /Subprompts · 1 of 1 in play/ }).click();
      await sidebarPicker.getByRole('checkbox', { name: `Subprompt ${IN_CHAT_TITLE}` }).click();

      await expect(page.getByText('Subprompts updated')).toBeVisible({ timeout: 15_000 });
      await expect
        .poll(async () => seatSubprompts(ctx, chatId, CHARACTER), { timeout: 15_000 })
        .toEqual([]);

      // Leave the vault as it was found.
      const bram = await characterId(ctx, CHARACTER);
      await dispatch(ctx, {
        type: 'characterSubpromptDelete',
        characterId: bram,
        subpromptId: 'no-spoilers',
      });
    } finally {
      await ctx.dispose();
    }
  });

  test('(d) a user-controlled seat is offered no picker — it has no identity stack to carry one', async ({
    page,
  }) => {
    test.skip(!P4D163_SERVER_LANDED, 'awaits P4.D163: the five characterSubprompt* verbs');

    // The shared GROUP chat, whose cast is Aria + Bram under the LLM and Cleo
    // under the operator — the one place in the fixture where BOTH arms of the
    // guard sit on the same screen, so the negative is not vacuous.
    await page.goto('/salon');
    await maybeUnlock(page);
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
    await page
      .locator('.chat-card-stack a.qt-entity-card', { hasText: 'Group Expedition' })
      .click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible();
    await openSidebarSection(page, 'Participants');

    const llmSeat = page.locator('qt-participant-card').filter({ hasText: CHARACTER }).first();
    const userSeat = page.locator('qt-participant-card').filter({ hasText: 'Cleo' }).first();
    await expect(llmSeat).toBeVisible({ timeout: 15_000 });
    await expect(userSeat).toBeVisible();

    // The positive arm is what makes the negative mean anything.
    await expect(llmSeat.locator('qt-subprompt-picker')).toHaveCount(1);
    await expect(userSeat.locator('qt-subprompt-picker')).toHaveCount(0);
  });
});
