import { expect, request as pwRequest, test, type Page, type Route } from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';
import { startMockLlm, MOCK_LLM_REPLY, type MockLlm } from './support/mock-llm';

/**
 * ORDERING: rides the SHARED global-setup server and unlocks it, so its
 * filename must sort AFTER aa-foundation.spec.ts ('s' does).
 *
 * P4.D229 — the Concierge's "Try uncensored" (v4 `ce2f1dabf`, #77), live. Three
 * buttons, hidden ONLY on a Locked chat (not gated on duty, nor on a desk being
 * configured — the server's `no-understudy` refusal is how a missing desk
 * surfaces):
 *
 *   (a) the action bar, on a character line, after Regenerate;
 *   (b) the `generate_image` TOOL row, beside its "Tried:" call sheet;
 *   (c) the Lantern's `background-refused` bubble (an announcement chip in v5 —
 *       the button rides the expanded body).
 *
 * ## Two halves
 *
 * The PRESENCE beats are the client's alone: they
 * plant the rows / the state by rewriting the `chatGet` dispatch's RESPONSE in
 * the browser (`page.route` → `route.fetch()` → edit → `fulfill`) — the real
 * server answers, the real SPA renders, only the planted rows are synthetic.
 * That is the honest way to reach a Locked chat, a refused picture's TOOL row
 * and a Lantern refusal on a fixture that holds none of them.
 *
 * The ROUND-TRIP beats — a 409 `no-understudy` worded as v4 words it, and a
 * live re-roll on the uncensored desk against the mock LLM — need the two §S.3
 * verbs (`messageRetryUncensored`, `chatRetryImageUncensored`), which are
 * P4.D228's. They went live at the chain's unification.
 */
const CHAT_TITLE = 'Solo Voyage';

type Json = Record<string, unknown>;

async function dispatch(body: Json): Promise<Json> {
  const ctx = await pwRequest.newContext();
  const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: body });
  const parsed = (await res.json().catch(() => null)) as { type?: string; data?: Json } | null;
  await ctx.dispose();
  if (!parsed || parsed.type === 'error') {
    throw new Error(`dispatch ${String(body['type'])} failed: ${JSON.stringify(parsed)}`);
  }
  return parsed.data ?? {};
}

async function ensureUnlocked(): Promise<void> {
  const ctx = await pwRequest.newContext();
  await ctx
    .post(`${BASE_URL}/api/dispatch`, { data: { type: 'unlock', passphrase: E2E_PASSPHRASE } })
    .catch(() => undefined);
  await ctx.dispose();
}

async function chatIdOf(title: string): Promise<string> {
  const chats = (await dispatch({ type: 'listChats' })) as unknown as Array<{ id: string; title: string }>;
  const hit = (Array.isArray(chats) ? chats : []).find((c) => c.title === title);
  if (!hit) throw new Error(`the fixture must carry "${title}"`);
  return hit.id;
}

async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  await page.waitForLoadState('domcontentloaded');
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

/**
 * Rewrite every `chatGet` answer for this page. The real server answers; the
 * edit runs over its JSON.
 */
async function rewriteChatGet(page: Page, edit: (chat: Json) => void): Promise<void> {
  await page.route('**/api/dispatch', async (route: Route) => {
    const req = route.request().postDataJSON() as Json | null;
    if (!req || req['type'] !== 'chatGet') {
      await route.fallback();
      return;
    }
    const response = await route.fetch();
    const body = (await response.json()) as { type?: string; data?: { chat?: Json } };
    if (body.type === 'chat' && body.data?.chat) edit(body.data.chat);
    await route.fulfill({ response, json: body });
  });
}

/** A planted message: a clone of the chat's first ASSISTANT line, overridden. */
function plant(chat: Json, over: Json): void {
  const messages = chat['messages'] as Json[];
  const template = messages.find((m) => m['role'] === 'ASSISTANT') ?? messages[messages.length - 1];
  const last = messages[messages.length - 1];
  const at = new Date(Date.parse(String(last['createdAt'])) + 60_000).toISOString();
  messages.push({
    ...template,
    swipeGroupId: null,
    swipeIndex: null,
    routeTrail: null,
    attachments: [],
    createdAt: at,
    ...over,
  });
}

async function openChat(page: Page, chatId: string): Promise<void> {
  await page.goto(`${BASE_URL}/salon/${chatId}`);
  await maybeUnlock(page);
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
}

/** The action bar's "Try uncensored" icons (aria-label; the TOOL/Lantern buttons are text). */
function lineButtons(page: Page) {
  return page.locator('.qt-chat-message-action-bar button[aria-label="Try uncensored"]');
}

test.beforeAll(async () => {
  await ensureUnlocked();
});

test.describe('P4.D229 — "Try uncensored": where the buttons are', () => {
  test('(a) sits on every character line, immediately after Regenerate', async ({ page }) => {
    const chatId = await chatIdOf(CHAT_TITLE);
    await openChat(page, chatId);

    const regenerate = page.locator(
      '.qt-chat-message-action-bar button[aria-label="Regenerate response"]',
    );
    await expect(regenerate.first()).toBeAttached({ timeout: 15_000 });
    // One per character line — never on the operator's own lines.
    await expect(lineButtons(page)).toHaveCount(await regenerate.count());
    await expect(
      page.locator('.qt-chat-message-row-user button[aria-label="Try uncensored"]'),
    ).toHaveCount(0);

    // v4's order: Regenerate, then Try uncensored.
    const firstBar = page
      .locator('.qt-chat-message-action-bar')
      .filter({ has: page.locator('button[aria-label="Regenerate response"]') })
      .first();
    const labels = await firstBar.locator('button').evaluateAll((els) =>
      els.map((b) => b.getAttribute('aria-label')),
    );
    expect(labels[labels.indexOf('Regenerate response') + 1]).toBe('Try uncensored');
  });

  test('(a) is ABSENT on a Locked chat — the pill says Locked and no line offers a retry', async ({
    page,
  }) => {
    const chatId = await chatIdOf(CHAT_TITLE);
    await rewriteChatGet(page, (chat) => {
      chat['conciergeState'] = 'locked';
      chat['conciergeSetBy'] = 'operator';
      chat['conciergeReason'] = 'manual';
    });
    await openChat(page, chatId);

    await expect(page.locator('qt-conversation-header .qt-danger-badge')).toHaveText('Locked', {
      timeout: 15_000,
    });
    await expect(
      page.locator('.qt-chat-message-action-bar button[aria-label="Regenerate response"]').first(),
    ).toBeAttached();
    await expect(lineButtons(page)).toHaveCount(0);
  });

  test('(b) a refused picture’s TOOL row shows "Tried:" and offers the redraw', async ({ page }) => {
    const chatId = await chatIdOf(CHAT_TITLE);
    await rewriteChatGet(page, (chat) =>
      plant(chat, {
        id: 'p4d229-planted-tool',
        role: 'TOOL',
        participantId: null,
        content: JSON.stringify({
          toolName: 'generate_image',
          success: false,
          prompt: 'a lighthouse at dusk',
          result: 'The provider declined the picture.',
        }),
        routeTrail: [
          {
            profileId: 'img-house',
            profileName: 'House Painter',
            provider: 'OPENAI',
            modelName: 'gpt-image-1',
            via: 'primary',
            outcome: 'refused',
            trigger: 'moderation-refusal',
            evidence: 'typed-error',
            profileKind: 'image',
          },
        ],
      }),
    );
    await openChat(page, chatId);

    const sheet = page.locator('[aria-label="Image profiles tried"]');
    await expect(sheet).toHaveCount(1, { timeout: 15_000 });
    await expect(sheet).toContainText('Tried:');
    // An image profile is known by its NAME on the badge (v4 #73's `label`).
    await expect(sheet).toContainText('House Painter');
    await expect(sheet.locator('[role="img"]', { hasText: '🚫' })).toHaveCount(1);

    const redraw = page.locator('qt-tool-message button', { hasText: 'Try uncensored' });
    await expect(redraw).toHaveCount(1);
    await expect(redraw).toHaveClass(/qt-button-secondary/);
  });

  test('(c) the Lantern’s refused backdrop offers ONE button in its expanded bubble', async ({
    page,
  }) => {
    const chatId = await chatIdOf(CHAT_TITLE);
    await rewriteChatGet(page, (chat) =>
      plant(chat, {
        id: 'p4d229-planted-lantern',
        role: 'ASSISTANT',
        participantId: null,
        systemSender: 'lantern',
        systemKind: 'background-refused',
        content:
          "The Lantern's usual painter (OPENAI gpt-image-1) would not take the scene — called it improper and downed brushes. The backdrop stays as it was.",
      }),
    );
    await openChat(page, chatId);

    const chip = page.locator('.qt-chat-announcement-chip').filter({ hasText: 'backdrop refused' });
    await expect(chip).toHaveCount(1, { timeout: 15_000 });
    await chip.click();
    const buttons = page
      .locator('qt-announcement-group button')
      .filter({ hasText: 'Try uncensored' });
    await expect(buttons).toHaveCount(1);
  });
});

test.describe('P4.D229 — "Try uncensored": the round trip', () => {
  test('a line retry with no uncensored desk is refused in the Concierge’s words', async ({
    page,
  }) => {
    // The precondition, stated rather than assumed: nothing on file may be
    // uncensored-compatible, or auto-detect would find a desk.
    const profiles = ((await dispatch({ type: 'connectionProfileList' }))['profiles'] ?? []) as Json[];
    expect(
      profiles.filter((p) => p['isDangerousCompatible'] === true),
      'the fixture must carry no uncensored-compatible connection profile',
    ).toHaveLength(0);

    const chatId = await chatIdOf(CHAT_TITLE);
    await openChat(page, chatId);
    await lineButtons(page).last().click({ force: true });
    await expect(
      page.getByText(
        'There is no uncensored desk to send this to — appoint one under Settings → The Concierge.',
      ),
    ).toBeVisible({ timeout: 15_000 });
  });

  test('a line retry on a configured desk re-rolls the line on the uncensored desk', async ({
    page,
  }) => {
    test.setTimeout(90_000);
    let mock: MockLlm | undefined;
    let profileId: string | undefined;
    const before = (await dispatch({ type: 'chatSettings' })) as Json;
    try {
      mock = await startMockLlm(MOCK_LLM_REPLY);
      const created = await dispatch({
        type: 'connectionProfileCreate',
        profile: {
          name: 'P4D229 Uncensored Desk',
          provider: 'OPENAI_COMPATIBLE',
          baseUrl: mock.url,
          // A model of its own: the retry excludes every profile on the model
          // that answered the original (v4's same-provider+model rule).
          modelName: 'mock-uncensored-model',
          // An understudy needs a usable API key (v4 `decryptKey`: no
          // `apiKeyId` → null → nobody takes the retry, a 409 `no-understudy`).
          // The fixture's own key row; the mock never reads it. Found by the
          // beat's first live run at the round's unification.
          apiKeyId: 'a0000001-0000-4000-8000-000000000001',
          isDangerousCompatible: true,
        },
      });
      profileId = String((created['profile'] as Json | undefined)?.['id'] ?? created['id']);
      expect(profileId).toBeTruthy();
      await dispatch({
        type: 'chatSettingsUpdate',
        settings: {
          conciergeSettings: {
            ...((before['conciergeSettings'] as Json | undefined) ?? {}),
            enabled: true,
            uncensoredTextProfileId: profileId,
          },
        },
      });

      const chatId = await chatIdOf(CHAT_TITLE);
      await openChat(page, chatId);
      await lineButtons(page).last().click({ force: true });

      // The same narration as a regenerate, then the new swipe on screen.
      await expect(page.getByText(MOCK_LLM_REPLY).first()).toBeVisible({ timeout: 30_000 });
      // The new swipe's trail ends on the Concierge's desk (§S.3).
      const trail = page.locator('[aria-label="Models tried for this reply"]').last();
      await expect(trail.locator('[title*="sent by the Concierge"]')).toHaveCount(1, {
        timeout: 15_000,
      });
    } finally {
      await dispatch({
        type: 'chatSettingsUpdate',
        settings: { conciergeSettings: before['conciergeSettings'] ?? {} },
      }).catch(() => undefined);
      if (profileId) {
        await dispatch({ type: 'connectionProfileDelete', profileId }).catch(() => undefined);
      }
      await mock?.close();
    }
  });
});
