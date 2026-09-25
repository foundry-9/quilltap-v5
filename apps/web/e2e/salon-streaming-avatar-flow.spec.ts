import { expect, request as pwRequest, test, type Page } from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { startMockLlm, MOCK_LLM_REPLY, type MockLlm } from './support/mock-llm';

/**
 * P4.75 — the STREAMING bubble's avatar column, observed MID-TURN.
 *
 * v4 opens the live assistant row with the responding character's avatar
 * (`StreamingMessage.tsx:85-96`), under the same `shouldShowAvatars` gate the
 * settled rows use (`SalonView.tsx:1171-1174`) and carrying the same
 * dangerous-chat ring (`:85`'s ternary). v5's live bubble had never rendered
 * that column at all — the gap P4.69 recorded at the site when its ring had
 * nowhere to land.
 *
 * ## Why the assertion is not "an avatar appeared"
 *
 * The column has to name the character the SERVER chose for this turn
 * (`getRespondingCharacter` → `respondingParticipantId` → that participant's
 * character), and in a multi-character chat the server's choice is a genuine
 * runtime decision — asserting a hardcoded name would either be wrong or would
 * pin the turn-selection algorithm from the wrong end. So this beat reads the
 * server's OWN answer off the wire: it captures the app's real
 * `EventSource#onmessage` handler (the `salon-optimistic-bubble-reconcile`
 * idiom, memory note `e2e-inject-wire-bytes-via-eventsource`), records every
 * frame the server sends WITHOUT changing one of them, and takes the
 * `participantId` from the turn frame. The cast is fetched over the same
 * dispatch API the app uses, so participant → character name is the server's
 * mapping too. Whatever the server picks, the avatar must name it.
 *
 * ## Why it must be sampled while the turn runs
 *
 * The column only exists between the send and the turn's end — the settled row
 * that replaces it draws its OWN avatar, from a different code path
 * (`message-row.ts`). A post-turn assertion would therefore pass on the settled
 * row and prove nothing about the streaming one, which is exactly why this is a
 * mid-turn beat (the P4.66 gesture). The mock streams slowly so the window is
 * real rather than lucky, and the poll below FAILS LOUDLY if the bubble never
 * appears rather than falling through to a settled row.
 *
 * Sends land in "Group Expedition" (never "Solo Voyage", whose token totals the
 * `salon-token-cost-flow` beat pins to an exact baseline).
 */

/**
 * P4.D229 — the Concierge's three states (v4 `4d370a90f`). The retired
 * four-state values (`monitored` / `flagged`) are a 400 on the chain's server,
 * and the three new ones (`moderated` / `unmoderated`) are a 400 on `main`'s —
 * so every Concierge dispatch here waits on the server chain (P4.D225 →
 * P4.D228), flipped by the unifier after the pick. The no-ring beat still runs
 * on `main`: the fixture's chat is Moderated by default and `main`'s chat GET
 * carries no `conciergeState`, which the SPA reads as Moderated.
 */
const P4D228_SERVER_LANDED = false;
const GATE_REASON =
  'awaits the Concierge server chain (P4.D225→P4.D228: the three-value conciergeState); flipped at unification';

interface WireFrame {
  chatId?: string;
  participantId?: string;
  turnStart?: boolean;
  content?: string;
  done?: boolean;
}

/**
 * Record every SSE frame the server sends, without altering the stream: wrap
 * the `onmessage` setter, keep the app's handler, and push a copy of each frame
 * onto `window.__qtSeenFrames` before delegating. A browser whose EventSource
 * prototype cannot be patched must not leave the beat looking healthy, so the
 * reader below throws on a missing array rather than reading an empty one.
 */
async function installFrameRecorder(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const proto = window.EventSource?.prototype;
    const desc = proto && Object.getOwnPropertyDescriptor(proto, 'onmessage');
    if (!proto || !desc?.get || !desc?.set) return; // reader throws; see below
    const nativeGet = desc.get;
    const nativeSet = desc.set;
    (window as unknown as { __qtSeenFrames: unknown[] }).__qtSeenFrames = [];
    Object.defineProperty(proto, 'onmessage', {
      configurable: true,
      get(this: EventSource) {
        return nativeGet.call(this);
      },
      set(this: EventSource, handler: unknown) {
        if (typeof handler !== 'function') {
          nativeSet.call(this, handler);
          return;
        }
        const app = handler as (ev: MessageEvent<string>) => void;
        nativeSet.call(this, (ev: MessageEvent<string>) => {
          try {
            (window as unknown as { __qtSeenFrames: unknown[] }).__qtSeenFrames.push(
              JSON.parse(ev.data),
            );
          } catch {
            // not JSON — the app's own parser decides; we only observe
          }
          app(ev);
        });
      },
    });
  });
}

async function seenFrames(page: Page): Promise<WireFrame[]> {
  return page.evaluate(() => {
    const frames = (window as unknown as { __qtSeenFrames?: unknown[] }).__qtSeenFrames;
    if (!frames) {
      throw new Error(
        'the frame recorder never installed — EventSource.prototype.onmessage is not patchable',
      );
    }
    return frames as WireFrame[];
  });
}

async function dispatch(body: Record<string, unknown>): Promise<Record<string, unknown>> {
  const ctx = await pwRequest.newContext();
  const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: body });
  const parsed = (await res.json().catch(() => null)) as {
    type?: string;
    data?: Record<string, unknown>;
  } | null;
  await ctx.dispose();
  if (!parsed || parsed.type === 'error') {
    throw new Error(`dispatch ${String(body['type'])} failed: ${JSON.stringify(parsed)}`);
  }
  return parsed.data ?? {};
}

test.describe('P4.75 — the streaming bubble names the responding character', () => {
  let mock: MockLlm;
  let chatId: string;

  test.beforeAll(async () => {
    // 350 ms per chunk: the turn is genuinely in flight while we sample.
    mock = await startMockLlm(MOCK_LLM_REPLY, MOCK_LLM_PORT, 350);
    const ctx = await pwRequest.newContext();
    await ctx
      .post(`${BASE_URL}/api/dispatch`, { data: { type: 'unlock', passphrase: E2E_PASSPHRASE } })
      .catch(() => undefined);
    await ctx.dispose();
    const chats = (await dispatch({ type: 'listChats' })) as unknown as Array<{
      id: string;
      title?: string | null;
    }>;
    chatId = (Array.isArray(chats) ? chats : []).find((c) => c.title === 'Group Expedition')!.id;
    expect(chatId, 'the fixture must carry "Group Expedition"').toBeTruthy();
  });

  test.afterAll(async () => {
    await mock?.close();
    // Leave the shared fixture as we found it (the concierge beats' idiom).
    if (chatId && P4D228_SERVER_LANDED) {
      await dispatch({
        type: 'chatUpdate',
        chatId,
        chat: {},
        conciergeState: 'moderated',
      }).catch(() => undefined);
    }
  });

  async function openChat(page: Page): Promise<void> {
    await installFrameRecorder(page);
    await page.goto('/salon');
    const passphrase = page.locator('#qt-passphrase');
    const chats = page.getByRole('heading', { name: 'Chats', exact: true });
    await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
      await expect(chats).toBeVisible({ timeout: 15_000 });
    }
    const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Group Expedition' });
    await expect(card).toBeVisible({ timeout: 15_000 });
    await card.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
  }

  async function send(page: Page, content: string): Promise<void> {
    const composer = page.locator('.qt-chat-composer-input .qt-rich-editor-content');
    await composer.click();
    await page.keyboard.type(content);
    await page.keyboard.press('Enter');
  }

  /** The live bubble's avatar column, waited for while the turn is still running. */
  function streamingColumn(page: Page) {
    return page.locator('qt-streaming-message .qt-chat-desktop-avatar');
  }

  test('the live bubble opens with the responding character (v4 StreamingMessage:85-96)', async ({
    page,
  }) => {
    // Establish the precondition rather than assuming the fixture's shipped
    // state: this chat is SHARED, and other Concierge beats move it earlier in
    // the run. The no-ring arm below asserts what a MODERATED chat does, so say
    // so. (Caught by the full suite on this beat's first whole-suite run — it
    // passed in isolation, which is exactly the shape P4.75 had just
    // root-caused in a neighbouring spec.) Until the chain lands no beat can
    // move it, and `main`'s server would 400 the new value — so the fixture's
    // own Moderated default IS the precondition there.
    if (P4D228_SERVER_LANDED) {
      await dispatch({ type: 'chatUpdate', chatId, chat: {}, conciergeState: 'moderated' });
    }
    // The cast does not change during the turn, so read it BEFORE the live
    // window: the streamed reply is ~2.5 s long, and a network round-trip inside
    // that window was a flake-in-waiting (the §3 unification review of the
    // follow-ups round 2) — only DOM reads happen while the column is live.
    const chat = (await dispatch({ type: 'chatGet', chatId }))['chat'] as {
      participants: Array<{ id: string; character?: { name?: string } | null }>;
    };
    await openChat(page);
    await send(page, 'P4.75 streaming avatar — who is answering?');

    // The column exists only while the turn runs; if it never appears this
    // fails here rather than sliding into the settled row's avatar.
    await expect(streamingColumn(page)).toHaveCount(1, { timeout: 20_000 });

    // The server's own answer, off the wire — not a guess about turn selection.
    // The column can render before the first frame arrives (the waiting arm
    // carries it too, exactly as v4's single row does), so wait for the server
    // to NAME a responder rather than reading whatever has arrived by now.
    let participantId: string | undefined;
    for (let i = 0; i < 100 && !participantId; i += 1) {
      participantId = (await seenFrames(page))
        .filter((f) => f.chatId === chatId && typeof f.participantId === 'string')
        .map((f) => f.participantId)
        .pop();
      if (!participantId) await page.waitForTimeout(200);
    }
    expect(participantId, 'the server never named a responding participant').toBeTruthy();

    // Read the column only once the responder is known, and only while the turn
    // is still live (the settled row draws its avatar from another code path).
    await expect(streamingColumn(page)).toHaveCount(1);

    const expected = chat.participants.find((p) => p.id === participantId)?.character?.name;
    expect(expected, 'the named participant should be a character seat').toBeTruthy();

    // `qt-avatar` draws the PORTRAIT when the character has one and the name's
    // INITIAL when it does not (v4 `Avatar.tsx:125` `name.charAt(0)
    // .toUpperCase()`; the image's `alt` is the name, `:147`). Which arm this
    // chat takes depends on who the server picked AND on whether an earlier
    // spec in the run gave that character a portrait — so read whichever the
    // column actually drew rather than assuming. (Caught by the full suite on
    // this beat's first whole-suite run: in isolation the responder had no
    // portrait and the initial was there; in the suite it had one, and the
    // column's text was empty.)
    const img = streamingColumn(page).locator('img');
    if (await img.count()) {
      // The stronger arm: the portrait carries the WHOLE name as its alt text.
      expect(await img.getAttribute('alt')).toBe(expected);
    } else {
      const rendered = ((await streamingColumn(page).innerText()) ?? '').trim();
      expect(rendered).toBe(expected![0]!.toUpperCase());
    }

    // A Moderated chat wears no ring (v4 :85's ternary, false arm).
    await expect(
      page.locator('qt-streaming-message .qt-chat-avatar-dangerous'),
      'no ring on a Moderated chat',
    ).toHaveCount(0);
  });

  test('an Unmoderated chat rings the live column too (v4 StreamingMessage:85 ternary)', async ({
    page,
  }) => {
    test.skip(!P4D228_SERVER_LANDED, GATE_REASON);
    // The Salon paints from `shouldShowDangerStyling(chat)` — Unmoderated,
    // whoever set it, since v4 `4d370a90f` (the operator's own pick here) —
    // which is what the settled rows already ring (P4.69). Seeded through the
    // same chat-update the Concierge surfaces use, then restored in afterAll.
    await dispatch({ type: 'chatUpdate', chatId, chat: {}, conciergeState: 'unmoderated' });
    await openChat(page);
    await send(page, 'P4.75 streaming avatar — the ring, mid-turn.');

    await expect(streamingColumn(page)).toHaveCount(1, { timeout: 20_000 });
    await expect(
      page.locator('qt-streaming-message .qt-chat-desktop-avatar.qt-chat-avatar-dangerous'),
      'the live column should wear the ring while the turn runs',
    ).toHaveCount(1);
  });
});
