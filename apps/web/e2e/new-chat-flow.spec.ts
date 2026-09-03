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

  // --- P4.D149: the Concierge picker at creation (v4 `303288fb4`) -------------

  /**
   * The CLIENT rule alone, and it needs no server: intercept the create
   * dispatch and read the body off the wire. A plain create carries NO
   * `conciergeState` key at all (so a create stays byte-identical to what it
   * has always been), and a picked state rides verbatim.
   *
   * The request is allowed to succeed — today's server ignores the unknown
   * field (memory note `dispatch-verb-ignores-unknown-fields`), which is
   * exactly why this beat can be UNGATED while its sibling below cannot.
   */
  test('the create body omits conciergeState by default and carries the pick verbatim', async ({
    page,
  }) => {
    const bodies: Record<string, unknown>[] = [];
    await page.route('**/api/dispatch', async (route) => {
      const data = route.request().postDataJSON() as Record<string, unknown> | null;
      if (data && data['type'] === 'chatCreate') bodies.push(data);
      await route.fallback();
    });

    // 1. The default. The picker starts on Monitored and says so.
    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/salon/new');
    await expect(page.getByRole('heading', { name: 'Select Characters' })).toBeVisible();

    const picker = page.locator('#new-chat-concierge');
    await expect(picker).toBeVisible();
    await expect(picker).toHaveValue('monitored');
    await expect(picker.locator('option', { hasText: 'Monitored (default)' })).toHaveCount(1);
    // The helper sentence beneath is the shared table's `detail`, not its `hint`.
    await expect(page.getByText(MONITORED_DETAIL)).toBeVisible();
    await expect(page.getByText(CONCIERGE_HINT)).toHaveCount(0);

    await page.locator('.new-chat-character-picker button').first().click();
    await expect(page.getByText('Speaks First')).toBeVisible();
    await page.getByRole('button', { name: 'Create Chat' }).click();
    await expect(page).toHaveURL(/\/salon\/[0-9a-f-]{16,}/, { timeout: 20_000 });

    expect(bodies).toHaveLength(1);
    expect(bodies[0]).not.toHaveProperty('conciergeState');

    // 2. The pick. Flagged rides verbatim.
    await page.goto('/salon/new');
    await expect(page.getByRole('heading', { name: 'Select Characters' })).toBeVisible();
    const second = page.locator('#new-chat-concierge');
    await second.selectOption('flagged');
    await expect(second).toHaveValue('flagged');
    // The helper sentence follows the selection.
    await expect(page.getByText(FLAGGED_DETAIL)).toBeVisible();

    await page.locator('.new-chat-character-picker button').first().click();
    await expect(page.getByText('Speaks First')).toBeVisible();
    await page.getByRole('button', { name: 'Create Chat' }).click();
    await expect(page).toHaveURL(/\/salon\/[0-9a-f-]{16,}/, { timeout: 20_000 });

    expect(bodies).toHaveLength(2);
    expect(bodies[1]['conciergeState']).toBe('flagged');
  });

  /**
   * The whole loop, once P4.D148 lands: pick Uncensored on the FORM, create,
   * and the landed chat is already Uncensored — the sidebar control reads it
   * back, and the Concierge's manual-uncensored bubble is in the transcript
   * (the sentence `salon-concierge-four-state-flow.spec.ts` asserts for that
   * kind), which is the proof the flip went through `applyConciergeFlip` at
   * creation rather than being a client-side display.
   */
  test('picking Uncensored at creation lands an Uncensored chat with the Concierge’s bubble', async ({
    page,
  }) => {
    test.skip(
      !P4D148_SERVER_LANDED,
      'awaits P4.D148’s `conciergeState` key on the chatCreate verb (flipped at unification)',
    );

    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/salon/new');
    await expect(page.getByRole('heading', { name: 'Select Characters' })).toBeVisible();

    const picker = page.locator('#new-chat-concierge');
    await picker.selectOption('uncensored');
    await expect(picker).toHaveValue('uncensored');

    await page.locator('.new-chat-character-picker button').first().click();
    await expect(page.getByText('Speaks First')).toBeVisible();
    await page.getByRole('button', { name: 'Create Chat' }).click();
    await expect(page).toHaveURL(/\/salon\/[0-9a-f-]{16,}/, { timeout: 20_000 });

    // The chat was CREATED Uncensored: the sidebar's control reads it back.
    await openChatDrawer(page);
    const sidebar = page
      .locator('qt-chat-sidebar label')
      .filter({ hasText: 'The Concierge' })
      .locator('select');
    await expect(sidebar).toBeVisible({ timeout: 15_000 });
    await expect(sidebar).toHaveValue('uncensored', { timeout: 15_000 });

    // …and the Concierge said so, once, in the transcript. The announcement is
    // chipped (v5 chips Staff-signed announcements), so expand it to read the
    // sentence — the same locator shape the four-state walk uses.
    const chips = page.locator('.qt-chat-announcement-chip').filter({ hasText: 'The Concierge' });
    await expect(chips).toHaveCount(1, { timeout: 15_000 });
    await chips.first().click();
    await expect(page.locator('.qt-chat-messages-list').getByText(UNCENSORED_PHRASE)).toHaveCount(
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
 * ACTIVATE-AT-UNIFY. The server half — the `conciergeState` key on the
 * `chatCreate` verb (shared contract §A) — is **P4.D148's**, and does not exist
 * on `main` until this round unifies. Until then the create-time beat above
 * would fail for a reason that says nothing about the client: the dispatch verb
 * ignores unknown fields, so the chat would land Monitored and no Concierge
 * bubble would ever be written. The unifier flips this to `true` once P4.D148
 * is in, and runs it live.
 */
const P4D148_SERVER_LANDED = true;

/**
 * The helper sentences this spec reads back, quoted from the ONE shared
 * presentation table (`app/chat/concierge-state-presentation.ts`, itself pinned
 * byte-for-byte against v4's module by the harness's `concierge-presentation`
 * oracle). Copied rather than imported because an e2e spec runs outside the
 * Angular build graph.
 */
const MONITORED_DETAIL =
  'The Concierge keeps watch, and will flip the switch himself if the conversation calls for it.';
const FLAGGED_DETAIL =
  'The Concierge has this chat down as dangerous, and routes it through the uncensored providers.';
/** The table's `hint` — deliberately NOT rendered under the form's control. */
const CONCIERGE_HINT = "Change it from the Salon sidebar's Chat section.";
/** v4's `manual-uncensored` sentence, cut to the phrase that identifies the kind. */
const UNCENSORED_PHRASE = 'uncensored door stands open';
