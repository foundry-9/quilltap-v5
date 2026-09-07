import { expect, request as pwRequest, test, type Page } from '@playwright/test';

import { BASE_URL, E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { MOCK_LLM_REPLY, startMockLlm, type MockLlm } from './support/mock-llm';

/**
 * P4.80 — dogfood finding #117: a salon chat can be deleted again.
 *
 * Before this lane `DELETE /api/v1/chats/{id}` answered 405 and no `chatDelete`
 * verb existed, so the whole affordance was missing from both chat lists. These
 * beats walk what an operator actually does, and each ends at the DATABASE
 * rather than at the list: a row filtered out of a client-side array looks
 * exactly like a row that was deleted, so the proof is the server answering
 * "gone" for the id afterwards.
 *
 * Every beat creates its OWN throwaway chat through the running server, so the
 * committed fixture's cast and conversations are left intact and the suite's
 * other specs (which share this instance, `workers: 1`) see nothing move.
 *
 * The seat has to be LLM-controlled — v4's `createChatSchema` refuses a cast
 * with no llm participant ("At least one LLM-controlled CHARACTER participant
 * is required"), which is what the first live run of these beats found — so the
 * create draws a greeting turn and the mock LLM has to be listening for it
 * (the `new-chat-flow` recipe). The greeting is not waited on: the create
 * resolves with the chat id, which is all a delete needs.
 */
test.describe('P4.80 — deleting a chat', () => {
  let mock: MockLlm;

  test.beforeAll(async () => {
    mock = await startMockLlm(MOCK_LLM_REPLY, MOCK_LLM_PORT);
  });

  test.afterAll(async () => {
    await mock?.close();
  });

  /**
   * Unlock only when the passphrase screen is showing (the shared server may
   * already be unlocked by an earlier spec). `readyHeading` is what the screen
   * shows when it is NOT locked — it differs per screen, and hard-coding the
   * Salon's "Chats" is what made the Conversations beat time out here on its
   * first live run.
   */
  async function maybeUnlock(page: Page, readyHeading: string): Promise<void> {
    const passphrase = page.locator('#qt-passphrase');
    const ready = page.getByRole('heading', { name: readyHeading, exact: true });
    await expect(passphrase.or(ready).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
    await expect(ready).toBeVisible({ timeout: 15_000 });
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

  /** The first LLM-controlled roster character — the seat both beats use. */
  async function firstCharacter(): Promise<{ id: string; name: string }> {
    const list = await dispatch({ type: 'characterList' });
    const characters = (list['characters'] ?? []) as Array<{
      id: string;
      name: string;
      controlledBy?: string;
    }>;
    const llm = characters.filter((c) => c.controlledBy !== 'user');
    expect(llm.length, 'the fixture must seed at least one llm character').toBeGreaterThan(0);
    return llm[0];
  }

  /** The fixture's mock-backed profile (rewritten to MOCK_LLM_PORT in global setup). */
  async function mockProfileId(): Promise<string> {
    const list = await dispatch({ type: 'connectionProfileList' });
    const profiles = (list['profiles'] ?? []) as Array<{ id: string; provider?: string }>;
    const id = profiles.find((p) => p.provider === 'OPENAI_COMPATIBLE')?.id ?? profiles[0]?.id;
    expect(id, 'the fixture must seed a connection profile').toBeTruthy();
    return id!;
  }

  /** A throwaway one-character chat with a recognisable title. */
  async function seedChat(title: string, characterId: string): Promise<string> {
    const created = await dispatch({
      type: 'chatCreate',
      title,
      participants: [
        {
          type: 'CHARACTER',
          characterId,
          controlledBy: 'llm',
          connectionProfileId: await mockProfileId(),
        },
      ],
    });
    const chat = created['chat'] as { id?: string } | undefined;
    expect(
      chat?.id,
      `chatCreate must answer a chat id, got ${JSON.stringify(created)}`,
    ).toBeTruthy();
    return chat!.id!;
  }

  /** Does the server still know this chat? */
  async function stillExists(chatId: string): Promise<boolean> {
    const resp = await dispatch({ type: 'chatGet', chatId });
    return resp['type'] !== 'error';
  }

  test('Salon list: confirm → the chat is gone from the list AND from the server', async ({
    page,
  }) => {
    test.setTimeout(90_000);
    // Unlock FIRST: a raw dispatch against a locked vault refuses, and the
    // character list comes back empty (the P4.6z lesson — `salon-autonomous-
    // entry` seeds the same way). Running this file alone is what surfaces it;
    // in a full suite an earlier spec has already unlocked the shared server.
    await page.goto('/salon');
    await maybeUnlock(page, 'Chats');

    const title = `Delete Me ${Date.now()}`;
    const chatId = await seedChat(title, (await firstCharacter()).id);

    // CANCELLING deletes nothing — asserted FIRST, so the beat cannot pass by
    // never having been able to delete at all. Re-ROUTE rather than `reload()`:
    // a bare reload restores the workspace's own last-active tab, which is not
    // necessarily the Chats one.
    await page.goto('/salon');
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible({
      timeout: 15_000,
    });
    const card = () => page.locator('a.chat-card').filter({ hasText: title }).first();
    await expect(card()).toBeVisible({ timeout: 15_000 });

    page.once('dialog', (d) => void d.dismiss());
    await card().getByRole('button', { name: 'Delete chat' }).click();
    await expect(card()).toBeVisible();
    expect(await stillExists(chatId), 'a dismissed confirmation must delete nothing').toBe(true);

    // Then confirm, from a freshly loaded list: the greeting this chat drew is
    // still settling, and a card that moves under the pointer is not what this
    // beat is about.
    await page.goto('/salon');
    await expect(card()).toBeVisible({ timeout: 15_000 });
    let asked = '';
    page.once('dialog', (d) => {
      asked = d.message();
      void d.accept();
    });
    await card().getByRole('button', { name: 'Delete chat' }).click();
    await expect(page.locator('a.chat-card').filter({ hasText: title })).toHaveCount(0, {
      timeout: 15_000,
    });
    expect(asked).toBe('Are you sure you want to delete this chat?');
    expect(await stillExists(chatId), 'the row must be gone from the server too').toBe(false);
  });

  test('Character Conversations tab: confirm → the card leaves the local list', async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await page.goto('/characters');
    await maybeUnlock(page, 'Characters');

    const title = `Tab Delete Me ${Date.now()}`;
    const seat = await firstCharacter();
    const chatId = await seedChat(title, seat.id);
    await page.goto('/characters');
    await expect(page.getByRole('heading', { name: 'Characters', exact: true })).toBeVisible({
      timeout: 15_000,
    });

    // Open the character the chat was seeded onto — BY NAME, so a roster whose
    // order changes cannot silently open a different one, and by its NAME LINK
    // rather than the description paragraph: `p.line-clamp-3` renders even for
    // a character with no description, and an empty paragraph has no box to
    // click (the first live run of this beat timed out there).
    const seatCard = page
      .locator('.character-card-grid .character-card')
      .filter({ hasText: seat.name })
      .first();
    await expect(seatCard).toBeVisible({ timeout: 15_000 });
    await seatCard.locator('a').first().click();
    await page.getByRole('button', { name: 'Conversations' }).click();

    const card = page.locator('a.chat-card').filter({ hasText: title }).first();
    await expect(card).toBeVisible({ timeout: 15_000 });

    page.once('dialog', (d) => void d.accept());
    await card.getByRole('button', { name: 'Delete chat' }).click();
    await expect(page.locator('a.chat-card').filter({ hasText: title })).toHaveCount(0, {
      timeout: 15_000,
    });
    expect(await stillExists(chatId)).toBe(false);
  });
});
