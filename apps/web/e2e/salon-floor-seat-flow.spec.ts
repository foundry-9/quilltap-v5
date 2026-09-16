import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * P4.D193 — the Skip banner speaks for the FLOOR, not the composer's seat
 * (v4 `2075242f9`, bug 146).
 *
 * With two seats the human drives, "whose turn is it" and "whose voice will the
 * composer take" are different questions. Before the fix the banner answered the
 * second while the operator read it as the first: it named the seat last written
 * as, and its Skip passed THAT seat's turn — a Host turn-pass against a seat
 * that never held the floor, with the real turn left outstanding.
 *
 * ⚠ P4.D58's note on the OLD banner beat — that no seeded chat can force a
 * weighted-random rotation onto a chosen seat — still stands, and TWO cheaper
 * levers were measured and REFUTED before this shape was settled on:
 *
 *  - The order's premise, that a post by one user seat hands the floor straight
 *    to the other, does not hold with an active LLM seat in the room. After the
 *    poster is recorded, the remaining eligible seats (the other user seat AND
 *    the LLM) both go to the weighted pick, so the floor is a coin flip.
 *  - The manual turn queue does not reach `chatTurnAction query` either.
 *    `handle_turn_action` recomputes its `TurnState` from HISTORY
 *    (`calculate_turn_state_from_history_with_cycle`), which never reads the
 *    persisted `turnQueue`; only the turn-RUN path (`resolve_responding_
 *    participant`) pops it. A queued seat is therefore invisible to `query` —
 *    measured, and the reason this beat does not use `action: 'queue'`.
 *
 * So the beat removes the randomness instead of fighting it: it **impersonates
 * the LLM seat**, which makes every active seat one the human drives (v4 bug
 * 44's overlay leaves the durable `controlledBy: 'llm'` alone). Whichever seat
 * the rotation lands on is then a floor seat the banner must speak for, so the
 * walk is TOTAL — no branch of it depends on the draw — and the overlay arm
 * gets exercised live whenever the draw picks the LLM seat.
 *
 * The composer still has to be moved deliberately: bug 49's turn-follow defaults
 * the speaking-as onto any user-driven floor seat, so on a fresh load the two
 * AGREE and the fourth sentence is unreachable. v4 names the way in
 * (`SalonView.tsx:1559-1562`): "the deliberate same-turn SpeakerSelector
 * choice", which the follow's seat-keyed latch leaves alone. With two OWNED
 * user seats there is always one to point the composer at that is not the floor.
 *
 * No committed fixture chat has two user-driven seats, so the beat builds one
 * live (the `composer-active-seat.spec.ts` recipe) — never a shared fixture
 * chat, whose sends trip the title checkpoint.
 */
test.describe.configure({ mode: 'serial' });

test.describe('P4.D193 — the Skip banner follows the floor, and so does its Skip', () => {
  /** The ids and names the whole walk is about, captured once at build time. */
  let chatId = '';
  let seatA = { id: '', name: '' };
  let seatB = { id: '', name: '' };
  let llmSeat = { id: '', name: '' };

  async function maybeUnlock(page: Page): Promise<void> {
    const passphrase = page.locator('#qt-passphrase');
    const chats = page.getByRole('heading', { name: 'Chats', exact: true });
    await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
  }

  /**
   * Land on the chat this walk built. Each test gets a fresh browser context, so
   * it must go through the LIST first: the unlock screen is the list route's,
   * and `/salon/<id>` on a locked instance renders neither the passphrase field
   * nor the Chats heading (measured — the first draft unlocked from the chat
   * route and timed out on both locators).
   */
  async function openBuiltChat(page: Page): Promise<void> {
    await page.goto('/salon');
    await maybeUnlock(page);
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible({
      timeout: 15_000,
    });
    await page.goto(`/salon/${chatId}`);
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
  }

  /** The server's own answer to "whose turn is it" — the independent source. */
  async function floorSeatId(page: Page): Promise<string | null> {
    const resp = await page.request.post('/api/dispatch', {
      data: { type: 'chatTurnAction', chatId, action: 'query' },
    });
    expect(resp.ok(), `chatTurnAction query → ${resp.status()}`).toBe(true);
    const body = (await resp.json()) as {
      data?: { turn?: { nextSpeakerId?: string | null } };
    };
    return body.data?.turn?.nextSpeakerId ?? null;
  }

  /**
   * Every Host turn-pass in the transcript, as `hostEvent.participantId` — read
   * from the SERVER, not the DOM. The Salon renders a turn-pass as a COLLAPSED
   * chip ("The Host / nothing to add"), so the sentence naming the seat is not
   * in the page at all (measured — an earlier draft asserted on it and read
   * back the chip). The id is the stronger comparand anyway: it is exactly what
   * bug 146 got wrong.
   */
  async function turnPassParticipantIds(page: Page): Promise<string[]> {
    const resp = await page.request.post('/api/dispatch', {
      data: { type: 'chatGet', chatId },
    });
    expect(resp.ok(), `chatGet → ${resp.status()}`).toBe(true);
    const body = (await resp.json()) as {
      data?: {
        chat?: {
          messages?: Array<{
            systemKind?: string | null;
            hostEvent?: { participantId?: string | null } | null;
          }>;
        };
      };
    };
    return (body.data?.chat?.messages ?? [])
      .filter((m) => m.systemKind === 'turn-pass')
      .map((m) => m.hostEvent?.participantId ?? '(none)');
  }

  /**
   * Impersonate a seat — the setup gesture that makes the LLM seat user-driven
   * so that EVERY active seat is one the banner must speak for.
   */
  async function impersonate(page: Page, participantId: string): Promise<void> {
    const resp = await page.request.post('/api/dispatch', {
      data: { type: 'chatImpersonate', chatId, participantId },
    });
    expect(resp.ok(), `chatImpersonate → ${resp.status()}`).toBe(true);
  }

  /** v4's "deliberate same-turn SpeakerSelector choice". */
  async function pickSpeaker(page: Page, name: string): Promise<void> {
    await page.locator('qt-speaker-selector button[aria-haspopup="listbox"]').click();
    await page.locator('qt-speaker-selector [role="option"]').filter({ hasText: name }).click();
    await expect(
      page.locator('qt-speaker-selector button[aria-haspopup="listbox"]'),
    ).toContainText(`Speaking as ${name}`);
  }

  const bannerText = (page: Page) => page.locator('.qt-chat-user-turn-banner span');

  test('builds a chat with one LLM seat and TWO seats the human drives', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);

    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
    await page.getByRole('link', { name: 'New Chat' }).first().click();
    await expect(page.getByRole('heading', { name: 'New Chat', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Select Characters' })).toBeVisible();

    const roster = page.locator('.new-chat-character-picker button');
    // Roster 0 stays LLM-driven: its connection profile auto-seeds, which is
    // what the create form's `llmSelected().length > 0` guard needs. Unlike
    // `composer-active-seat.spec.ts` this beat leaves it ACTIVE — the room is
    // meant to have somebody for a pass to hand the floor to.
    await roster.nth(0).click();
    await expect(page.getByText('Speaks First')).toBeVisible();

    for (const index of [1, 2]) {
      const name = (
        await roster.nth(index).locator('.qt-text-primary').first().textContent()
      )?.trim();
      expect(name, `roster row ${index} has a name`).toBeTruthy();
      await roster.nth(index).click();
      const card = page
        .locator('.new-chat-character-picker .space-y-4 > div')
        .filter({ hasText: name! });
      await card.locator('select').first().selectOption({ label: 'Play As (User)' });
      if (index === 1) seatA.name = name!;
      else seatB.name = name!;
    }
    await expect(page.getByRole('heading', { name: 'Selected Characters (3)' })).toBeVisible();

    const create = page.getByRole('button', { name: 'Create Chat' });
    await expect(create).toBeEnabled();
    await create.click();
    await expect(page).toHaveURL(/\/salon\/[0-9a-f-]{16,}/, { timeout: 20_000 });
    chatId = new URL(page.url()).pathname.split('/').pop()!;
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });

    // Resolve the two user seats' participant ids from the SERVER, so every
    // later assertion names an id the app did not hand us.
    const resp = await page.request.post('/api/dispatch', {
      data: { type: 'chatGet', chatId },
    });
    expect(resp.ok(), `chatGet → ${resp.status()}`).toBe(true);
    const body = (await resp.json()) as {
      data?: {
        chat?: {
          participants?: Array<{
            id: string;
            controlledBy?: string;
            character?: { name?: string };
          }>;
        };
      };
    };
    const parts = body.data?.chat?.participants ?? [];
    seatA.id = parts.find((p) => p.character?.name === seatA.name)?.id ?? '';
    seatB.id = parts.find((p) => p.character?.name === seatB.name)?.id ?? '';
    expect(seatA.id, `${seatA.name} has a participant id`).toBeTruthy();
    expect(seatB.id, `${seatB.name} has a participant id`).toBeTruthy();
    expect(
      parts.filter((p) => p.controlledBy === 'user').map((p) => p.id).sort(),
      'exactly two user-controlled seats',
    ).toEqual([seatA.id, seatB.id].sort());

    const llm = parts.find((p) => p.controlledBy !== 'user');
    llmSeat = { id: llm?.id ?? '', name: llm?.character?.name ?? '' };
    expect(llmSeat.id, 'the room has its LLM seat').toBeTruthy();

    // Every active seat becomes one the human drives, so whichever seat the
    // rotation lands on, the banner must speak for IT. The durable column is
    // untouched (v4 bug 44) — this is the overlay, and that is the point.
    await impersonate(page, llmSeat.id);
  });

  test('names the floor’s seat, and its Skip passes THAT turn', async ({ page }) => {
    await openBuiltChat(page);

    // Whose turn is it? Read the SERVER — the independent source, and the same
    // `chatTurnAction query` the client reads. With the LLM seat impersonated
    // every active seat is user-driven, so this is always a seat the banner
    // must speak for; only its identity is drawn, and identity is a gesture
    // here, never an assertion.
    const floorId = await floorSeatId(page);
    const seats = [seatA, seatB, llmSeat];
    const floor = seats.find((s) => s.id === floorId);
    expect(floor, `the floor names a seat of this room (got ${floorId})`).toBeTruthy();

    // Point the composer at an OWNED user seat that is NOT the floor. There are
    // two, so one of them always qualifies — and the speaker selector offers
    // only owned seats, which is why the impersonated LLM seat is never it.
    const composer = [seatA, seatB].find((s) => s.id !== floor!.id)!;

    // Bug 49's turn-follow has already moved the composer onto the floor, so the
    // two agree and the banner reads the plain turn sentence…
    await expect(bannerText(page)).toBeVisible({ timeout: 15_000 });
    await expect(bannerText(page)).toHaveText(
      `${floor!.name}'s turn — type as them, or skip to let someone else respond.`,
      { timeout: 15_000 },
    );

    // …until the human deliberately points it elsewhere. THIS is the shape bug
    // 146 is about: the floor is one seat's, the composer is another's.
    await pickSpeaker(page, composer.name);
    await expect(bannerText(page)).toHaveText(
      `${floor!.name}'s turn — switch the speaker to them to type, or skip to let someone else respond.`,
      { timeout: 15_000 },
    );

    // The Skip must pass the FLOOR's turn — the outstanding one — not the
    // composer's. The wait is ARMED BEFORE the click (an assertion after a
    // click reads the pre-click state).
    const skipPost = page.waitForRequest(
      (req) => {
        if (!req.url().includes('/api/dispatch') || req.method() !== 'POST') return false;
        try {
          const body = JSON.parse(req.postData() ?? '{}') as { action?: string };
          return body.action === 'skipUserTurn';
        } catch {
          return false;
        }
      },
      { timeout: 15_000 },
    );
    await page.locator('.qt-chat-user-turn-banner').getByRole('button', { name: 'Skip' }).click();
    const posted = JSON.parse((await skipPost).postData() ?? '{}') as {
      participantId?: string;
    };
    expect(posted.participantId, 'Skip passes the FLOOR’s seat').toBe(floor!.id);

    // …and the server RECORDED the pass against the floor's seat. This is bug
    // 146's actual harm, read back off the transcript: before the fix the Host
    // note named the composer's seat — "a Host turn-pass for a seat that never
    // held the floor" — while the real turn stayed outstanding. Three active
    // characters, so `qualifiesForTurnSkipping` holds and the note is posted.
    await expect.poll(() => turnPassParticipantIds(page), { timeout: 15_000 }).toEqual([floor!.id]);
  });
});
