import { expect, test, type Locator, type Page, type Request } from './support/fixtures';

import { E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { startMockLlm, MOCK_LLM_REPLY, type MockLlm } from './support/mock-llm';

/**
 * P4.66 — the duplicate optimistic user bubble, live (dogfood finding #106).
 *
 * v4 holds ONE array a refetch replaces wholesale (`useChatData.ts:14,83`), so
 * a mid-turn refetch can never show the human's own message twice. v5's
 * optimistic bubble lives in a separate signal appended at render
 * (`salon-conversation.ts` `optimisticUser` / `displayMessages`) — latent
 * until the P4.D123–D125 realtime work started refetching the chat mid-turn
 * (`realtime/job_topics.rs:81-88`: TITLE_UPDATE / CONTEXT_SUMMARY /
 * CHAT_DANGER_CLASSIFICATION / SCENE_STATE_TRACKING /
 * WARDROBE_OUTFIT_ANNOUNCEMENT each resolve to a `{v:1, topic:'chats', id}`
 * hint over the SSE `/api/events` channel → `chatKeys.detail(id)` →
 * `RealtimeService` invalidates the chat query while the reply is still
 * streaming). Before the fix, the just-persisted user row and the
 * still-uncleared optimistic bubble both rendered — but only for the WINDOW
 * between the mid-turn refetch landing and the turn's OWN end-of-turn
 * reconcile clearing the bubble, so a plain `toHaveCount(1)` right after
 * firing the hint proves nothing: it resolves the instant any poll matches
 * "1", including the poll BEFORE the refetch's network round trip has even
 * landed. This beat instead waits for the triggered `chatGet` response
 * itself, then samples the rendered count on a fixed cadence across the
 * following window and asserts it never once reads 2 — the only way to
 * catch a duplicate that heals itself before the next assertion would poll.
 *
 * ## Why the hint is INJECTED at the wire
 *
 * All five job kinds above resolve to the byte-identical client wire shape —
 * `job_topics.rs`'s own `TopicHint::scoped(RealtimeTopic::Chats, chat_id)` for
 * every one of them — so there is no client-observable difference between
 * "TITLE_UPDATE fired" and "CONTEXT_SUMMARY fired". Provoking a genuine
 * server-side job deterministically mid-stream would mean racing job-queue
 * scheduling against the mock's reply timing for no additional proof (v4's
 * title checkpoint alone never fires before conversational interchange 2 —
 * `context_summary.rs`'s `should_check_title_at_interchange`); the 2026-08-01
 * deflake precedent's rule applies instead: reproduce deterministically, keep
 * every assertion faithful to the real mechanism. So this beat installs a
 * `page.addInitScript` (the `salon-attachment-ledger-flow.spec.ts` idiom) that
 * captures the app's real `EventSource.prototype.onmessage` handler
 * (`core-transport.ts:150`, the SAME handler a real server frame would call)
 * and exposes `window.__qtInjectRealtimeHint(topic, id)` to feed it a
 * synthetic frame shaped exactly like the server's real hint
 * (`realtime.types.ts` `RealtimeHint`). Everything downstream is real: the
 * live EventSource, `parseEventData`, `RealtimeService.acceptFrame`,
 * `queryKeysForTopic`, the TanStack invalidation, the REAL `chatGet` refetch
 * against the real binary — only the trigger's origin (server job vs. test)
 * is faked, exactly the shape the standing e2e note recommends.
 *
 * Sends land in "Group Expedition", NOT "Solo Voyage": the P4.6ap chat-totals
 * beat (`salon-token-cost-flow.spec.ts`) asserts a hardcoded token baseline for
 * Solo Voyage and every message sent there shifts it (the
 * `salon-thinking-indicator` note); this beat sends distinctively-worded
 * messages and asserts only on their own rendered count, never on a chat-wide
 * total, so it does not perturb any other spec reading Group Expedition.
 *
 * The mock streams SLOWLY (`delayMs`) so the turn is still genuinely
 * in-flight — `busy()` true, the optimistic bubble not yet cleared — when the
 * mid-turn hint lands, matching the real defect's window.
 */

/**
 * Install the realtime-hint injection hook before app boot. Captures the
 * live `EventSource#onmessage` handler on `window.__qtEsHandler`, and defines
 * `window.__qtInjectRealtimeHint(topic, id)` to hand it a synthetic
 * `MessageEvent` shaped exactly like `RealtimeHint` (`realtime.types.ts`).
 * Every other frame the handler ever receives is untouched — this only ADDS
 * one extra call when the test explicitly asks for it.
 */
async function installRealtimeHintHook(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const proto = window.EventSource?.prototype;
    const desc = proto && Object.getOwnPropertyDescriptor(proto, 'onmessage');
    if (!proto || !desc?.get || !desc?.set) {
      // P4.69: a browser whose EventSource prototype we cannot patch must not
      // leave the beat looking healthy. Plant a poisoned injector so the first
      // injection fails LOUDLY instead of silently doing nothing.
      (
        window as unknown as { __qtInjectRealtimeHint: (topic: string, id: string) => void }
      ).__qtInjectRealtimeHint = () => {
        throw new Error(
          'realtime-hint hook not installed: EventSource.prototype.onmessage is not patchable',
        );
      };
      return;
    }
    const nativeGet = desc.get;
    const nativeSet = desc.set;
    Object.defineProperty(proto, 'onmessage', {
      configurable: true,
      get(this: EventSource) {
        return nativeGet.call(this);
      },
      set(this: EventSource, handler: unknown) {
        if (typeof handler === 'function') {
          (window as unknown as { __qtEsHandler: unknown }).__qtEsHandler = handler;
        }
        nativeSet.call(this, handler);
      },
    });
    (
      window as unknown as { __qtInjectRealtimeHint: (topic: string, id: string) => void }
    ).__qtInjectRealtimeHint = (topic: string, id: string) => {
      const handler = (
        window as unknown as { __qtEsHandler?: (ev: MessageEvent<string>) => void }
      ).__qtEsHandler;
      // P4.69: this used to `return` on a missing handler. That made the whole
      // injection a no-op the beat could not feel — the wait below would then
      // be satisfied by some unrelated refetch and the test would PASS having
      // exercised nothing (memory note `e2e-inject-wire-bytes-via-eventsource`).
      // The app's own `EventSource` handler being absent is a hard failure.
      if (!handler) {
        throw new Error(
          'realtime-hint injection had no EventSource handler to feed — the app never opened /api/events',
        );
      }
      handler(
        new MessageEvent('message', {
          data: JSON.stringify({ v: 1, topic, id, at: Date.now() }),
        }),
      );
    };
  });
}

/**
 * A running tally of the `chatGet` refetches this chat has issued.
 *
 * Matching on the request BODY (not just the URL — every dispatch verb shares
 * `POST /api/dispatch`) keeps unrelated verbs out. But the body match ALONE is
 * still not scoped enough for this beat: the Salon issues `chatGet` for its own
 * reasons (the send's own reconcile, a turn transition, the realtime hub's
 * catch-up sweep), so "a chatGet for this chat arrived" can be satisfied by a
 * request that was already in flight when the hint was injected — the beat
 * would then measure the app's ordinary refetch and never the injected one.
 *
 * P4.69 scopes it by SEQUENCE instead: snapshot the count at injection time and
 * wait for a request issued strictly after that. Requests are counted from
 * `page.on('request')`, which fires at issue time, so the ordering is the
 * client's own.
 */
class ChatRefetchTally {
  private count = 0;
  private readonly handler: (req: Request) => void;

  constructor(
    private readonly page: Page,
    chatId: string,
  ) {
    this.handler = (req: Request) => {
      if (req.method() !== 'POST' || !req.url().endsWith('/api/dispatch')) return;
      try {
        const body = JSON.parse(req.postData() ?? '{}') as { type?: string; chatId?: string };
        if (body.type === 'chatGet' && body.chatId === chatId) this.count += 1;
      } catch {
        // not JSON — not a dispatch body we care about
      }
    };
    page.on('request', this.handler);
  }

  /** The number of matching refetches seen so far. */
  seen(): number {
    return this.count;
  }

  /** Wait until a refetch issued AFTER `mark` has been seen. */
  async waitPast(mark: number, timeoutMs = 10_000): Promise<void> {
    const deadline = Date.now() + timeoutMs;
    while (this.count <= mark) {
      if (Date.now() > deadline) {
        throw new Error(
          `no chatGet refetch was issued after the injected hint (still ${this.count}, wanted > ${mark})`,
        );
      }
      await this.page.waitForTimeout(50);
    }
  }

  dispose(): void {
    this.page.off('request', this.handler);
  }
}

/**
 * Sample a locator's count repeatedly on a fixed cadence. `toHaveCount` alone
 * cannot prove "never became 2" — it resolves the instant any poll matches,
 * so a duplicate that heals before the next automatic poll would slip through
 * silently. This drives the polling itself and returns every value observed.
 */
async function sampleCounts(locator: Locator, samples: number, intervalMs: number): Promise<number[]> {
  const counts: number[] = [];
  for (let i = 0; i < samples; i++) {
    counts.push(await locator.count());
    if (i < samples - 1) {
      await new Promise((resolve) => setTimeout(resolve, intervalMs));
    }
  }
  return counts;
}

test.describe('P4.66 — the optimistic bubble survives a mid-turn refetch (LIVE, dogfood #106)', () => {
  let mock: MockLlm;

  test.beforeAll(async () => {
    mock = await startMockLlm(MOCK_LLM_REPLY, MOCK_LLM_PORT, 350);
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
      await expect(chats).toBeVisible({ timeout: 15_000 });
    }
  }

  async function openGroupExpedition(page: Page): Promise<string> {
    await page.goto('/salon');
    await maybeUnlock(page);
    const card = page.locator('.chat-card-stack a.qt-entity-card', {
      hasText: 'Group Expedition',
    });
    await expect(card).toBeVisible({ timeout: 15_000 });
    await card.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
    const id = new URL(page.url()).pathname.split('/').pop()!;
    expect(id).toBeTruthy();
    return id;
  }

  async function send(page: Page, content: string): Promise<void> {
    const composer = page.locator('.qt-chat-composer-input .qt-rich-editor-content');
    await composer.click();
    await page.keyboard.type(content);
    await page.keyboard.press('Enter');
  }

  /** Fire the injected hint and prove the bubble count never spikes to 2. */
  async function assertNoMidTurnDuplicate(
    page: Page,
    chatId: string,
    topic: string,
    bubbles: Locator,
  ): Promise<void> {
    const tally = new ChatRefetchTally(page, chatId);
    try {
      const mark = tally.seen();
      // Throws inside the page if the hook never captured a handler — a silent
      // no-op here used to be indistinguishable from a healthy run.
      await page.evaluate(
        ([t, id]) =>
          (
            window as unknown as { __qtInjectRealtimeHint: (topic: string, id: string) => void }
          ).__qtInjectRealtimeHint(t, id),
        [topic, chatId] as const,
      );
      // A refetch the INJECTED hint caused — issued strictly after the mark, so
      // an already-in-flight `chatGet` cannot stand in for it. This is the exact
      // window the bug lived in (the persisted row now in `msgs`, the optimistic
      // bubble not yet cleared because the turn is still streaming).
      await tally.waitPast(mark);
    } finally {
      tally.dispose();
    }

    const counts = await sampleCounts(bubbles, 12, 150);
    expect(Math.max(...counts), `bubble counts sampled across the refetch window: ${counts.join(', ')}`).toBe(
      1,
    );
  }

  test('a mid-turn chats-topic hint never doubles the just-sent user bubble', async ({ page }) => {
    await installRealtimeHintHook(page);
    const chatId = await openGroupExpedition(page);

    const content = 'Bertie asks: are we there yet, P4.66?';
    const bubbles = page.locator('qt-message-row').filter({ hasText: content });

    await send(page, content);

    // Mid-flight: the quill is up, so the turn is genuinely still streaming —
    // the exact window a real server hint would land in.
    const quill = page.locator('.qt-thinking-indicator');
    await expect(quill.first()).toBeVisible({ timeout: 15_000 });

    // The user's own row is already persisted by now (the send created it
    // before the reply began streaming) — only the optimistic bubble shows
    // until something refetches.
    await expect(bubbles).toHaveCount(1);

    // The mid-turn refetch: a chats-topic hint scoped to THIS chat, exactly
    // the shape TITLE_UPDATE / CONTEXT_SUMMARY / CHAT_DANGER_CLASSIFICATION /
    // SCENE_STATE_TRACKING / WARDROBE_OUTFIT_ANNOUNCEMENT all produce
    // (`job_topics.rs:81-88`). THE GUARD lives here.
    await assertNoMidTurnDuplicate(page, chatId, 'chats', bubbles);

    // Settled: the reply lands, and the post-turn transcript is intact —
    // still exactly one bubble for the sent content.
    await expect(page.getByText(MOCK_LLM_REPLY).first()).toBeVisible({ timeout: 30_000 });
    await expect(quill).toHaveCount(0, { timeout: 30_000 });
    await expect(bubbles).toHaveCount(1);
  });

  test('a second mid-turn hint (standing in for CONTEXT_SUMMARY) leaves a later send with its own single bubble', async ({
    page,
  }) => {
    // Tier 2: a distinct turn, a distinct hint firing, proving the reconcile
    // isn't a one-shot fluke wired to the first test's exact sequencing.
    await installRealtimeHintHook(page);
    const chatId = await openGroupExpedition(page);

    const content = 'Second exchange, still asking: are we there yet?';
    const bubbles = page.locator('qt-message-row').filter({ hasText: content });

    await send(page, content);
    await expect(page.locator('.qt-thinking-indicator').first()).toBeVisible({ timeout: 15_000 });
    await expect(bubbles).toHaveCount(1);

    await assertNoMidTurnDuplicate(page, chatId, 'chats', bubbles);

    await expect(page.getByText(MOCK_LLM_REPLY).first()).toBeVisible({ timeout: 30_000 });
    await expect(bubbles).toHaveCount(1);
  });
});
