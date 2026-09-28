import {
  expect,
  request as pwRequest,
  test,
  type APIRequestContext,
  type Page,
} from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { MOCK_LLM_REPLY, startMockLlm, type MockLlm } from './support/mock-llm';

/** Raw dispatch against the real axum server (mirrors salon-autonomous-entry). */
async function dispatch(ctx: APIRequestContext, req: unknown): Promise<Record<string, unknown>> {
  const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: req });
  const body = (await res.json().catch(() => null)) as { data?: Record<string, unknown> } | null;
  return body?.data ?? {};
}

/**
 * P4.6q: browser walk of the New-Chat vertical — unlock → the Salon list's
 * "New Chat" affordance → `/salon/new` → pick a character (its connection
 * profile auto-seeds) → Create → the Green Room narrates → the walk lands on the
 * created conversation with the greeting rendered.
 *
 * Runs against the committed Salon fixture the global setup provisions (P4.6a:
 * characters + the OPENAI_COMPATIBLE profile rewritten to the fixed
 * MOCK_LLM_PORT), so this spec only starts the mock on that port — the same
 * recipe as `m4-salon.spec.ts`. The server side of chat creation + the Green
 * Room SSE replay are already live (P4.4u2); this is the SPA leg. Activated at
 * the P4.6p/q/r unification; unlock-state-tolerant per the standing recipe.
 */
test.describe('P4.6q — New-Chat vertical (list → /salon/new → create → land)', () => {
  let mock: MockLlm;

  test.beforeAll(async () => {
    mock = await startMockLlm(MOCK_LLM_REPLY, MOCK_LLM_PORT);
  });

  test.afterAll(async () => {
    await mock?.close();
  });

  /** Unlock only when the passphrase screen is showing (the shared server may already be unlocked). */
  async function maybeUnlock(page: Page): Promise<void> {
    const passphrase = page.locator('#qt-passphrase');
    const chats = page.getByRole('heading', { name: 'Chats', exact: true });
    await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
  }

  test('New Chat → pick a character → create → land on the conversation', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);

    // The Salon list header carries the New-Chat affordance.
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
    await page.getByRole('link', { name: 'New Chat' }).first().click();

    // The New-Chat form renders.
    await expect(page.getByRole('heading', { name: 'New Chat', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Select Characters' })).toBeVisible();

    // Pick the first roster character — its connection profile auto-seeds, so the
    // "Speaks First" badge appears and the create button enables.
    await page.locator('.new-chat-character-picker button').first().click();
    await expect(page.getByText('Speaks First')).toBeVisible();

    const create = page.getByRole('button', { name: 'Create Chat' });
    await expect(create).toBeEnabled();
    await create.click();

    // The Green Room narrates while the conversation is assembled (best-effort —
    // the dispatch resolving closes it), then the walk lands on the new chat with
    // the streamed greeting rendered.
    await expect(page).toHaveURL(/\/salon\/[0-9a-f-]{16,}/, { timeout: 20_000 });
    await expect(page.getByText(MOCK_LLM_REPLY).first()).toBeVisible({ timeout: 20_000 });
  });

  /**
   * P4.6bi (v4 `e2eb3d21`): the picker re-port. A default-user persona now
   * appears in the Select Characters roster (previously filtered out), and
   * reverting Play As to "Chat as yourself" KEEPS that character in the cast
   * under LLM control (the old behavior removed it). Seeds and tears down a
   * unique user persona through the API so the shared roster is left clean.
   */
  test('full roster lists a default-user persona; reverting Play As keeps it in the cast', async ({
    page,
  }) => {
    const persona = `E2E Persona ${Date.now()}`;
    const ctx = await pwRequest.newContext();
    let personaId = '';
    try {
      // Seed a default-user persona: quick-create (defaults to llm) → toggle to user.
      const created = await dispatch(ctx, { type: 'characterQuickCreate', name: persona });
      personaId = ((created['character'] as { id?: string } | undefined)?.id ?? '') as string;
      expect(personaId).toBeTruthy();
      await dispatch(ctx, { type: 'characterToggleControlledBy', characterId: personaId });

      await page.goto('/salon');
      await maybeUnlock(page);
      await page.goto('/salon/new');
      await expect(page.getByRole('heading', { name: 'Select Characters' })).toBeVisible();

      // A favorite LLM roster character takes the speaker's chair.
      await page.locator('.new-chat-character-picker button').first().click();
      await expect(page.getByText('Speaks First')).toBeVisible();

      // Full roster: the default-user persona is now listed and can be added.
      await page.getByPlaceholder('Search characters...').fill(persona);
      const personaRow = page.locator('.new-chat-character-picker button', { hasText: persona });
      await expect(personaRow).toBeVisible();
      await personaRow.click();
      await expect(page.getByRole('heading', { name: 'Selected Characters (2)' })).toBeVisible();

      // Play As the persona, then revert to "Chat as yourself".
      const playAs = page.locator('#new-chat-partner');
      await playAs.selectOption(personaId);
      await playAs.selectOption('');

      // Keep-on-revert: the persona stays in the cast (count unchanged) — the old
      // behavior would have removed it, dropping the count to 1.
      await expect(page.getByRole('heading', { name: 'Selected Characters (2)' })).toBeVisible();
    } finally {
      if (personaId) await dispatch(ctx, { type: 'characterDelete', characterId: personaId });
      await ctx.dispose();
    }
  });

  /**
   * P4.D44 (v4 `4bbeab47`): the create-time Roleplay Template picker. Asserts
   * the pre-selection tells the truth (this instance sets no default, so
   * "No Template" wears the `(default)` label and is what is selected), then
   * picks a seeded template, creates, and reads the created chat back through
   * raw dispatch — the chat must carry the id that was on screen. Seeds and
   * deletes its own template so the shared instance is left as it was found.
   */
  test('the roleplay-template picker pre-selects the default and the pick reaches the chat', async ({
    page,
  }) => {
    const name = `E2E Template ${Date.now()}`;
    const ctx = await pwRequest.newContext();
    let templateId = '';
    try {
      const created = await dispatch(ctx, {
        type: 'roleplayTemplateCreate',
        template: {
          name,
          description: null,
          systemPrompt: 'Write plainly, and mind the lantern.',
          narrationDelimiters: '*',
        },
      });
      templateId = ((created['template'] as { id?: string } | undefined)?.id ??
        (created as { id?: string }).id ??
        '') as string;
      expect(templateId).toBeTruthy();

      await page.goto('/salon');
      await maybeUnlock(page);
      await page.goto('/salon/new');
      await expect(page.getByRole('heading', { name: 'Select Characters' })).toBeVisible();

      // The dropdown renders (templates exist) and pre-selects what the chat
      // would have gotten anyway. No default is configured here, so that is
      // "No Template", and the option says so.
      const picker = page.locator('#new-chat-roleplay-template');
      await expect(picker).toBeVisible();
      await expect(picker).toHaveValue('');
      await expect(picker.locator('option', { hasText: 'No Template (default)' })).toHaveCount(1);

      // Pick the seeded template by hand, then create.
      await picker.selectOption(templateId);
      await expect(picker).toHaveValue(templateId);

      await page.locator('.new-chat-character-picker button').first().click();
      await expect(page.getByText('Speaks First')).toBeVisible();
      const create = page.getByRole('button', { name: 'Create Chat' });
      await expect(create).toBeEnabled();
      await create.click();

      await expect(page).toHaveURL(/\/salon\/[0-9a-f-]{16,}/, { timeout: 20_000 });
      const chatId = (page.url().match(/\/salon\/([0-9a-f-]{16,})/) ?? [])[1] ?? '';
      expect(chatId).toBeTruthy();

      // The value the user saw is the value the chat was created with.
      const fetched = await dispatch(ctx, { type: 'chatGet', chatId });
      const chat = (fetched['chat'] ?? fetched) as { roleplayTemplateId?: string | null };
      expect(chat.roleplayTemplateId).toBe(templateId);
    } finally {
      if (templateId) {
        await dispatch(ctx, { type: 'roleplayTemplateDelete', templateId });
      }
      await ctx.dispose();
    }
  });

  // --- The Concierge picker at creation (v4 `303288fb4`; three states since
  //     `4d370a90f`, P4.D229) --------------------------------------------------

  /**
   * The CLIENT rule's default half, and it needs no server: intercept the
   * create dispatch and read the body off the wire. A plain create carries NO
   * `conciergeState` key at all under the (default) Moderated default, and the
   * picker is a FLAT three-option list whose first option says "(default)".
   *
   * The pick half (a three-state value on the wire) is its sibling below.
   */
  test('the create body omits conciergeState by default, under a flat three-state picker', async ({
    page,
  }) => {
    const bodies: Record<string, unknown>[] = [];
    await page.route('**/api/dispatch', async (route) => {
      const data = route.request().postDataJSON() as Record<string, unknown> | null;
      if (data && data['type'] === 'chatCreate') bodies.push(data);
      await route.fallback();
    });

    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/salon/new');
    await expect(page.getByRole('heading', { name: 'Select Characters' })).toBeVisible();

    const picker = page.locator('#new-chat-concierge');
    await expect(picker).toBeVisible();
    await expect(picker).toHaveValue('moderated');
    await expect(picker.locator('optgroup')).toHaveCount(0);
    await expect(picker.locator('option')).toHaveText(['Moderated (default)', 'Unmoderated', 'Locked']);
    // The helper sentence beneath is the shared table's `detail`, not its `hint`.
    await expect(page.getByText(MODERATED_DETAIL)).toBeVisible();
    await expect(page.getByText(CONCIERGE_HINT)).toHaveCount(0);

    await page.locator('.new-chat-character-picker button').first().click();
    await expect(page.getByText('Speaks First')).toBeVisible();
    await page.getByRole('button', { name: 'Create Chat' }).click();
    await expect(page).toHaveURL(/\/salon\/[0-9a-f-]{16,}/, { timeout: 20_000 });

    expect(bodies).toHaveLength(1);
    expect(bodies[0]).not.toHaveProperty('conciergeState');
  });

  /**
   * The whole loop: pick Unmoderated on the FORM, create — the body carries the
   * pick verbatim — and the landed chat is already Unmoderated: the sidebar
   * control reads it back, and the Concierge's `set-unmoderated` bubble is in
   * the transcript, which is the proof the flip went through
   * `applyConciergeFlip` at creation rather than being a client-side display.
   */
  test('picking Unmoderated at creation lands an Unmoderated chat with the Concierge’s bubble', async ({
    page,
  }) => {
    const bodies: Record<string, unknown>[] = [];
    await page.route('**/api/dispatch', async (route) => {
      const data = route.request().postDataJSON() as Record<string, unknown> | null;
      if (data && data['type'] === 'chatCreate') bodies.push(data);
      await route.fallback();
    });

    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/salon/new');
    await expect(page.getByRole('heading', { name: 'Select Characters' })).toBeVisible();

    const picker = page.locator('#new-chat-concierge');
    await picker.selectOption('unmoderated');
    await expect(picker).toHaveValue('unmoderated');
    // The helper sentence follows the selection (the operator's own sentence).
    await expect(page.getByText(UNMODERATED_DETAIL)).toBeVisible();

    await page.locator('.new-chat-character-picker button').first().click();
    await expect(page.getByText('Speaks First')).toBeVisible();
    await page.getByRole('button', { name: 'Create Chat' }).click();
    await expect(page).toHaveURL(/\/salon\/[0-9a-f-]{16,}/, { timeout: 20_000 });
    expect(bodies).toHaveLength(1);
    expect(bodies[0]['conciergeState']).toBe('unmoderated');

    // The chat was CREATED Unmoderated: the sidebar's control reads it back.
    await openChatDrawer(page);
    const sidebar = page
      .locator('qt-chat-sidebar label')
      .filter({ hasText: 'The Concierge' })
      .locator('select');
    await expect(sidebar).toBeVisible({ timeout: 15_000 });
    await expect(sidebar).toHaveValue('unmoderated', { timeout: 15_000 });

    // …and the Concierge said so, once, in the transcript. The announcement is
    // chipped (v5 chips Staff-signed announcements), so expand it to read the
    // sentence.
    const chips = page.locator('.qt-chat-announcement-chip').filter({ hasText: 'The Concierge' });
    await expect(chips).toHaveCount(1, { timeout: 15_000 });
    await chips.first().click();
    await expect(page.locator('.qt-chat-messages-list').getByText(UNMODERATED_PHRASE)).toHaveCount(
      1,
      { timeout: 15_000 },
    );
  });
});

/**
 * Expand the chat sidebar and open its "Chat" card, where the Concierge
 * control lives (the same three gestures `salon-concierge-four-state-flow`
 * and `salon-scenario-flow` each keep a copy of — this spec keeps its own
 * rather than reaching across into another spec's file).
 */
async function openChatDrawer(page: Page): Promise<void> {
  const sidebar = page.locator('qt-chat-sidebar');
  await expect(sidebar).toBeVisible({ timeout: 15_000 });
  const expand = page.getByRole('button', { name: 'Expand chat sidebar' });
  if (await expand.count()) await expand.click();
  const header = page
    .locator('qt-chat-sidebar .qt-collapsible-card-header')
    .filter({ hasText: 'Chat' })
    .first();
  await header.click();
}

/**
 * The helper sentences this spec reads back, quoted from the ONE shared
 * presentation table (`app/chat/concierge-state-presentation.ts`, itself pinned
 * byte-for-byte against v4's module by the `concierge-presentation` oracle).
 * Copied rather than imported because an e2e spec runs outside the Angular
 * build graph.
 */
const MODERATED_DETAIL =
  'The Concierge sends everything to the usual providers first, and to the uncensored desk only when one of them refuses. After enough refusals he moves the whole chat himself.';
const UNMODERATED_DETAIL =
  'You have opened the uncensored door yourself. Nothing here goes near a moderated provider.';
/** The table's `hint` — deliberately NOT rendered under the form's control. */
const CONCIERGE_HINT = "Change it from the Salon sidebar's Chat section.";
/** v4's `set-unmoderated` sentence, cut to the phrase that identifies the kind. */
const UNMODERATED_PHRASE = 'uncensored door stands open';
