import { expect, request as pwRequest, test, type Page } from '@playwright/test';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

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
 */
test.describe('P4.80 — deleting a chat', () => {
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

  /** The first roster character — the seat both beats hang a throwaway chat on. */
  async function firstCharacter(): Promise<{ id: string; name: string }> {
    const list = await dispatch({ type: 'characterList' });
    const characters = (list['characters'] ?? []) as Array<{ id: string; name: string }>;
    expect(characters.length, 'the fixture must seed at least one character').toBeGreaterThan(0);
    return characters[0];
  }

  /**
   * A throwaway one-character chat with a recognisable title. The seat is
   * USER-controlled on purpose: an llm seat would draw a greeting turn, and
   * this spec starts no mock LLM — the delete is what is under test, not the
   * Green Room.
   */
  async function seedChat(title: string, characterId: string): Promise<string> {
    const created = await dispatch({
      type: 'chatCreate',
      title,
      participants: [{ type: 'CHARACTER', characterId, controlledBy: 'user' }],
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
    test.setTimeout(60_000);
    const title = `Delete Me ${Date.now()}`;
    const chatId = await seedChat(title, (await firstCharacter()).id);

    await page.goto('/salon');
    await maybeUnlock(page);
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();

    const card = page.locator('a.chat-card').filter({ hasText: title }).first();
    await expect(card).toBeVisible({ timeout: 15_000 });

    // CANCELLING deletes nothing — asserted FIRST, on the same card, so the
    // beat cannot pass by never having been able to delete at all.
    page.once('dialog', (d) => void d.dismiss());
    await card.getByRole('button', { name: 'Delete chat' }).click();
    await expect(card).toBeVisible();
    expect(await stillExists(chatId), 'a dismissed confirmation must delete nothing').toBe(true);

    // Then confirm, and v4's question is what the operator is asked.
    let asked = '';
    page.once('dialog', (d) => {
      asked = d.message();
      void d.accept();
    });
    await card.getByRole('button', { name: 'Delete chat' }).click();
    await expect(card).toHaveCount(0, { timeout: 15_000 });
    expect(asked).toBe('Are you sure you want to delete this chat?');
    expect(await stillExists(chatId), 'the row must be gone from the server too').toBe(false);
  });

  test('Character Conversations tab: confirm → the card leaves the local list', async ({
    page,
  }) => {
    test.setTimeout(60_000);
    const title = `Tab Delete Me ${Date.now()}`;
    const seat = await firstCharacter();
    const chatId = await seedChat(title, seat.id);

    await page.goto('/characters');
    await maybeUnlock(page);
    await expect(page.getByRole('heading', { name: 'Characters', exact: true })).toBeVisible({
      timeout: 15_000,
    });

    // Open the character the chat was seeded onto — BY NAME, so a roster whose
    // order changes cannot silently open a different one and leave the beat
    // looking for a card that was never going to be there.
    const seatCard = page
      .locator('.character-card-grid .character-card')
      .filter({ hasText: seat.name })
      .first();
    await expect(seatCard).toBeVisible({ timeout: 15_000 });
    await seatCard.locator('p.line-clamp-3').click();
    await page.getByRole('button', { name: 'Conversations' }).click();

    const card = page.locator('a.chat-card').filter({ hasText: title }).first();
    await expect(card).toBeVisible({ timeout: 15_000 });

    page.once('dialog', (d) => void d.accept());
    await card.getByRole('button', { name: 'Delete chat' }).click();
    await expect(card).toHaveCount(0, { timeout: 15_000 });
    expect(await stillExists(chatId)).toBe(false);
  });
});
