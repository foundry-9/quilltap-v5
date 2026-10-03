import { expect, request as pwRequest, test, type Page } from '@playwright/test';

import { BASE_URL, E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { MOCK_LLM_REPLY, startMockLlm, type MockLlm } from './support/mock-llm';

/**
 * P4.145 — the Salon's attachment-only send persists v4's USER row, LIVE.
 *
 * v4's client sends `Please look at the attached file(s).` as the content of a
 * send that is nothing but attachments (`useSSEStreaming.ts:845`), and its
 * server saves the USER row — and links the files to it — only for non-empty
 * content (`orchestrator.service.ts:783`; v5's twin `orchestrator.rs:1796`).
 * Before P4.145 the v5 Salon sent `''`: no row was written, no file linked, and
 * the operator's post vanished from the transcript when the turn settled.
 *
 * The beat uploads a TEXT file through the real composer (an image would arm
 * the background describe and the Librarian's announcement — noise here),
 * presses Send with an empty editor, and ends at the DATABASE: `chatGet` must
 * hold exactly one new USER row reading the sentence with the file attached,
 * and the transcript must show it once.
 *
 * ORDERING: rides the SHARED global-setup server, so the filename sorts after
 * `aa-foundation.spec.ts` and before the `zz…` destructives.
 *
 * WHY A THROWAWAY CHAT (the `chat-delete-flow` idiom), not a fixture one: the
 * committed fixture has exactly one general (non-project) chat, "Group
 * Expedition", and extra sends there push it past a Host title checkpoint that
 * a later spec's non-streaming mock then resolves — renaming it and failing
 * every title-keyed beat downstream (the P4.D187 cascade). "Solo Voyage" is a
 * project chat whose totals beat asserts a hardcoded baseline; "Ridge Reunion"
 * is an autonomous room. So the beat creates its own general chat on the mock
 * profile and deletes it afterwards — the fixture's conversations never move.
 * The create draws a greeting turn, so the mock must be listening first.
 */

const SENTENCE = 'Please look at the attached file(s).';
const FILE_NAME = 'p4145-attach-only.txt';

test.describe('P4.145 — the attachment-only send (LIVE)', () => {
  let mock: MockLlm;

  test.beforeAll(async () => {
    // A SLOW stream (the thinking-indicator recipe): the optimistic bubble is
    // asserted while the reply is still in flight, and an instant stream can
    // settle — and retire the bubble — before the first poll.
    mock = await startMockLlm(MOCK_LLM_REPLY, MOCK_LLM_PORT, 400);
  });

  test.afterAll(async () => {
    await mock?.close();
  });

  async function maybeUnlock(page: Page): Promise<void> {
    const passphrase = page.locator('#qt-passphrase');
    const chats = page.getByRole('heading', { name: 'Chats', exact: true });
    await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
    await expect(chats).toBeVisible({ timeout: 15_000 });
  }

  /** Raw dispatch against the real axum server (the `new-chat-flow` idiom). */
  async function dispatch(req: unknown): Promise<Record<string, unknown>> {
    const ctx = await pwRequest.newContext();
    try {
      const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: req });
      const body = (await res.json().catch(() => null)) as {
        type?: string;
        data?: Record<string, unknown>;
      } | null;
      return { type: body?.type ?? '', ...(body?.data ?? {}) };
    } finally {
      await ctx.dispose();
    }
  }

  type Row = { id: string; role: string; content: string; attachments?: unknown[] };

  async function userRows(chatId: string): Promise<Row[]> {
    const resp = await dispatch({ type: 'chatGet', chatId });
    const chat = resp['chat'] as { messages?: Row[] } | undefined;
    return (chat?.messages ?? []).filter((m) => m.role === 'USER');
  }

  test('an empty editor with a text file attached persists the sentence row with the file', async ({
    page,
  }) => {
    test.setTimeout(120_000);
    // Unlock FIRST: a raw dispatch against a locked vault refuses.
    await page.goto('/salon');
    await maybeUnlock(page);

    const characters = ((await dispatch({ type: 'characterList' }))['characters'] ?? []) as Array<{
      id: string;
      controlledBy?: string;
    }>;
    const seat = characters.find((c) => c.controlledBy !== 'user');
    expect(seat, 'the fixture must seed an llm character').toBeTruthy();
    const profiles = ((await dispatch({ type: 'connectionProfileList' }))['profiles'] ?? []) as Array<{
      id: string;
      provider?: string;
    }>;
    const profileId = profiles.find((p) => p.provider === 'OPENAI_COMPATIBLE')?.id;
    expect(profileId, 'the fixture must seed the mock-backed profile').toBeTruthy();

    const created = await dispatch({
      type: 'chatCreate',
      title: `P4.145 attachment-only ${Date.now()}`,
      participants: [
        { type: 'CHARACTER', characterId: seat!.id, controlledBy: 'llm', connectionProfileId: profileId },
      ],
    });
    const chatId = (created['chat'] as { id?: string } | undefined)?.id;
    expect(chatId, `chatCreate must answer a chat id, got ${JSON.stringify(created)}`).toBeTruthy();

    try {
      await page.goto(`/salon/${chatId}`);
      await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
      // The greeting turn the create drew must settle before the send, or the
      // Salon answers the send with its "still speaking" refusal.
      await expect(page.getByText(MOCK_LLM_REPLY).first()).toBeVisible({ timeout: 30_000 });
      const sendButton = page.getByRole('button', { name: 'Send message' });
      await expect(sendButton).toBeDisabled({ timeout: 15_000 });

      const before = await userRows(chatId!);

      // v4's composer hint (`ChatComposer.tsx:523`, the P4.145 Option A
      // ruling): none on an empty composer, then "Add a message (optional)..."
      // once a file is attached — the live proof the editor repaints it.
      const hint = page.locator('.qt-chat-composer-input .qt-rich-editor-content .qt-rich-editor-placeholder');
      await expect(hint).toHaveCount(0);

      await page.getByRole('button', { name: 'Attach file', exact: true }).click();
      await page.locator('input[type=file][aria-label="Choose a file to attach"]').setInputFiles({
        name: FILE_NAME,
        mimeType: 'text/plain',
        buffer: Buffer.from('A note the operator attached without a word of their own.'),
      });
      await expect(page.locator('.qt-chat-attachment-chip')).toContainText(FILE_NAME, {
        timeout: 15_000,
      });
      await expect(hint).toHaveAttribute('data-placeholder', 'Add a message (optional)...');

      // The editor is empty: the attachment alone enables Send.
      await expect(sendButton).toBeEnabled();
      await sendButton.click();

      // v4's optimistic bubble (`useSSEStreaming.ts:799-802`) names the file —
      // never the sentence, which is the request's alone — while the reply is
      // still streaming.
      await expect(
        page.locator('qt-message-row').filter({ hasText: `[Attached: ${FILE_NAME}]` }),
      ).toHaveCount(1, { timeout: 10_000 });

      // The reply lands (the greeting's bubble plus this turn's).
      await expect(page.getByText(MOCK_LLM_REPLY)).toHaveCount(2, { timeout: 30_000 });

      // (i) The database holds exactly one NEW user row: the sentence, the file
      // attached to it. Polled — the settle refetch may trail the reply.
      await expect
        .poll(async () => (await userRows(chatId!)).length - before.length, { timeout: 15_000 })
        .toBe(1);
      const after = await userRows(chatId!);
      const fresh = after.filter((r) => !before.some((b) => b.id === r.id));
      expect(fresh).toHaveLength(1);
      expect(fresh[0].content).toBe(SENTENCE);
      expect(JSON.stringify(fresh[0].attachments ?? [])).toContain(FILE_NAME);

      // (ii) The transcript shows it exactly once after the turn settles, and
      // the bubble has handed off to it.
      await expect(page.locator('qt-message-row').filter({ hasText: SENTENCE })).toHaveCount(1, {
        timeout: 15_000,
      });
      await expect(
        page.locator('qt-message-row').filter({ hasText: `[Attached: ${FILE_NAME}]` }),
      ).toHaveCount(0);
    } finally {
      await dispatch({ type: 'chatDelete', chatId });
    }
  });
});
