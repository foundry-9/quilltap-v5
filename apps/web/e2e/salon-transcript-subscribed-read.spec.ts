import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * P4.D187 — the Salon's transcript as a SUBSCRIBED read (v4 `5029075bb`).
 *
 * The incident: a reply persisted at 12:49:59.812Z and never appeared in the
 * operator's tab, because nothing existed to tell the tab to look again. The
 * stream that would have carried it was gone — the tab had been backgrounded
 * for 34 seconds of generation — and the row sat in the database waiting for a
 * reload. The write funnel now publishes `{topic:'chats', id}` on every add,
 * edit and delete, and the Salon listens.
 *
 * ORDERING: rides the SHARED global-setup server and unlocks it, so the
 * filename sorts after `aa-foundation.spec.ts` and before the `zz…`
 * destructives. Reads only — it sends nothing, so it disturbs no chat's totals.
 *
 * GATED ACTIVATE-AT-UNIFY behind a NAMED constant: the `chatTranscript` verb is
 * P4.D183's. A capability probe would be the wrong instrument here for a
 * particular reason — the client SWALLOWS a failed read by design (v4's own
 * `catch`, so a hinted re-read that fails is not a failed conversation), so an
 * unimplemented verb is indistinguishable from a quiet one at the DOM.
 */

/** Flipped at unification, once P4.D183's `chatTranscript` verb lands. */
const P4D183_SERVER_LANDED = false;

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

async function openGroupExpedition(page: Page): Promise<void> {
  await page.goto('/salon');
  await maybeUnlock(page);
  const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Group Expedition' });
  await expect(card).toBeVisible({ timeout: 15_000 });
  await card.click();
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
}

/**
 * Push a `chats` hint into the page's live event channel.
 *
 * The same wire-level idiom `salon-optimistic-bubble-reconcile.spec.ts` and
 * `salon-chain-pause-toast-flow.spec.ts` use: the app's REAL `EventSource`
 * handler is what receives it, so the hub's topic routing, the Salon's
 * subscription and the read it fires are all real. Only the arrival of the
 * hint is staged — which is exactly the thing a read-only beat cannot provoke
 * without writing to the chat it is watching.
 */
async function armHintInjection(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const proto = window.EventSource?.prototype;
    const desc = proto && Object.getOwnPropertyDescriptor(proto, 'onmessage');
    if (!proto || !desc?.get || !desc?.set) return;
    const nativeSet = desc.set;
    Object.defineProperty(proto, 'onmessage', {
      configurable: true,
      get: desc.get,
      set(this: EventSource, handler: unknown) {
        nativeSet.call(this, handler);
        if (typeof handler === 'function') {
          const call = handler as (e: MessageEvent<string>) => void;
          (window as unknown as { __qtHint?: (chatId: string) => void }).__qtHint = (
            chatId: string,
          ) => {
            call(
              new MessageEvent('message', {
                data: JSON.stringify({ v: 1, topic: 'chats', id: chatId, at: Date.now() }),
              }),
            );
          };
        }
      },
    });
  });
}

test.describe('P4.D187 — the transcript is re-read on a hint', () => {
  test('a `chats` hint mid-idle fires the cheap read, carrying the version it last saw', async ({
    page,
  }) => {
    test.skip(
      !P4D183_SERVER_LANDED,
      'chatTranscript is P4.D183’s — there is no verb to answer the read until it lands',
    );

    await armHintInjection(page);
    await openGroupExpedition(page);

    // The dispatch the hint provokes, caught at the wire. `knownVersion` is the
    // whole point: an agreeing counter costs a round trip and the word
    // "unchanged", not a re-serialized conversation — which matters because one
    // busy turn fires wardrobe, backdrop, whisper and memory hints at this same
    // topic.
    const read = page.waitForRequest(
      (req) =>
        req.method() === 'POST' &&
        req.url().includes('/api/dispatch') &&
        (req.postData() ?? '').includes('"chatTranscript"'),
      { timeout: 20_000 },
    );

    const chatId = await page.evaluate(() => location.pathname.split('/').pop() ?? '');
    await page.evaluate(
      (id) => (window as unknown as { __qtHint?: (c: string) => void }).__qtHint?.(id),
      chatId,
    );

    const body = JSON.parse((await read).postData() ?? '{}') as {
      type?: string;
      chatId?: string;
      knownVersion?: number;
    };
    expect(body.type).toBe('chatTranscript');
    expect(body.chatId).toBe(chatId);
    // Seeded from the chat GET's own `transcriptVersion`, so the very first
    // hinted read is already conditional.
    expect(typeof body.knownVersion).toBe('number');
  });

  test('a row written by someone else appears with no stream at all — the incident', async ({
    page,
    request,
  }) => {
    test.skip(
      !P4D183_SERVER_LANDED,
      'chatTranscript is P4.D183’s — the row has no path to the tab until it lands',
    );

    await armHintInjection(page);
    await openGroupExpedition(page);
    const chatId = await page.evaluate(() => location.pathname.split('/').pop() ?? '');

    // A second client writes the row — the shape the incident had: a reply
    // persisted while this tab's stream was gone. This page opens no stream of
    // its own and sends nothing.
    const line = `A line no stream carried ${Date.now()}`;
    const written = await request.post('/api/dispatch', {
      data: { type: 'chatMessageAdd', chatId, role: 'ASSISTANT', content: line },
      failOnStatusCode: false,
    });
    test.skip(
      !written.ok(),
      'no dispatch verb on this server writes a bare transcript row for another client',
    );

    await expect(page.getByText(line)).toHaveCount(0);
    await page.evaluate(
      (id) => (window as unknown as { __qtHint?: (c: string) => void }).__qtHint?.(id),
      chatId,
    );

    await expect(page.getByText(line)).toBeVisible({ timeout: 20_000 });
  });
});
