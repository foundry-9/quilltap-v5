import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { startMockLlm, MOCK_LLM_REPLY, type MockLlm } from './support/mock-llm';

/**
 * P4.D161 — the chain-pause announcement, live (v4 `fef7ce4f7`, bug 123).
 *
 * ORDERING: rides the SHARED global-setup server and unlocks it, so the
 * filename must sort after `aa-foundation.spec.ts` and before the `zz…`
 * destructives. The send lands in "Group Expedition", NOT "Solo Voyage" — the
 * P4.6ap chat-totals beat asserts a hardcoded token baseline for Solo Voyage
 * and every message sent there shifts it.
 *
 * ## Why the paused frame is INJECTED at the wire
 *
 * A chain stops paused when the SERVER decides it has — the paused
 * early-return, a mid-chain decision, or the chain-error safety stop. Two of
 * those need a provider failure the e2e's mock endpoint cannot produce, and
 * the `paused` key itself is P4.D160's half of this round: this lane may land
 * and gate BEFORE it, because the client reads an OPTIONAL key. Provoking the
 * frame would therefore couple this beat to a sibling lane's landing order.
 *
 * So the beat fakes exactly one thing — the bytes of one extra frame — and
 * drives everything downstream for real: the live `EventSource`, the
 * transport's parse, the pure reducer's `chainPaused` carry, the vertical's
 * pre-reconcile snapshot and four gates, and the real toast stack in the real
 * DOM. The injection is a wire-level init script, not a component stub;
 * nothing in the app is mocked. (💸 The live proof on real data — a genuine
 * server-side pause — rides the round's dogfood walk.)
 */

/** v4's two sentences, byte-for-byte (`useSSEStreaming.ts:386-391`). */
const WARNING =
  "A character's turn failed, so auto-responses are paused. Press Resume in the sidebar to carry on.";
const INFO = 'Auto-responses are paused. Press Resume in the sidebar to let the others answer.';

/**
 * Rewrite the page's `chainComplete` frame to carry `{ reason, paused: true }`
 * — precisely the key P4.D160 adds server-side, on precisely the frame that
 * will carry it (§G). Everything else about the turn is real.
 *
 * Measured on this fixture (2026-09-06) before the beat was written: a send to
 * Group Expedition streams TWO chained turns, so the wire carries two `done`
 * frames and then ONE `chainComplete` — today with keys
 * `chatId,chainComplete,reason,nextSpeakerId,chainDepth` and no `paused`,
 * exactly as §G says a pre-D160 server emits. An earlier draft appended a
 * synthetic frame after the first `done` instead; the real `chainComplete`
 * then arrived later and reset `chainPaused` to false, because the reducer's
 * arm reads `frame.paused === true` on every chainComplete and the last one
 * wins. That is correct reducer behaviour and the wrong injection point, and
 * the beat's first run is what said so.
 *
 * Patches the `onmessage` accessor on `EventSource.prototype` before app boot,
 * where the real transport assigns its handler (`core-transport.ts:151`);
 * every other frame passes through untouched.
 */
async function injectPausedChainComplete(page: Page, reason: string): Promise<void> {
  await page.addInitScript((chainReason) => {
    const proto = window.EventSource?.prototype;
    const desc = proto && Object.getOwnPropertyDescriptor(proto, 'onmessage');
    if (!proto || !desc?.get || !desc?.set) return;
    const nativeGet = desc.get;
    const nativeSet = desc.set;
    let injected = false;
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
        const call = handler as (e: MessageEvent<string>) => void;
        const wrapped = (ev: MessageEvent<string>) => {
          let data = ev.data;
          if (!injected && typeof data === 'string') {
            try {
              const frame = JSON.parse(data) as Record<string, unknown>;
              if (frame['chainComplete'] === true) {
                frame['reason'] = chainReason;
                frame['paused'] = true;
                data = JSON.stringify(frame);
                injected = true;
              }
            } catch {
              /* not JSON (a keep-alive) — pass it through untouched */
            }
          }
          call(new MessageEvent('message', { data }));
        };
        nativeSet.call(this, wrapped);
      },
    });
  }, reason);
}

test.describe('P4.D161 — a pause the user did not cause is announced (LIVE)', () => {
  let mock: MockLlm;

  test.beforeAll(async () => {
    mock = await startMockLlm(MOCK_LLM_REPLY, MOCK_LLM_PORT);
  });
  test.afterAll(async () => {
    await mock?.close();
  });

  async function maybeUnlock(page: Page) {
    const passphrase = page.locator('#qt-passphrase');
    const chats = page.getByRole('heading', { name: 'Chats', exact: true });
    await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
      await expect(chats).toBeVisible({ timeout: 15_000 });
    }
  }

  async function openGroupExpedition(page: Page) {
    await page.goto('/salon');
    await maybeUnlock(page);
    const card = page.locator('.chat-card-stack a.qt-entity-card', {
      hasText: 'Group Expedition',
    });
    await expect(card).toBeVisible({ timeout: 15_000 });
    await card.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
  }

  async function sendAMessage(page: Page, text: string) {
    const composer = page.locator('.qt-chat-composer-input .qt-rich-editor-content');
    await composer.click();
    await page.keyboard.type(text);
    await page.keyboard.press('Enter');
  }

  test('a chain that stops paused because a turn FAILED raises v4’s warning', async ({ page }) => {
    await injectPausedChainComplete(page, 'error');
    await openGroupExpedition(page);

    const warnings = page.locator('[role="toast-container"] .qt-toast-warning');
    await expect(warnings).toHaveCount(0);

    await sendAMessage(page, 'Is anyone still there?');

    // One assertion, not two: it pins the sentence AND that exactly one warning
    // stands. A toast expires on v4's timer, so a second assertion afterwards
    // would be racing it for no extra proof.
    await expect(warnings).toHaveText([WARNING], { timeout: 20_000 });
    // The turn itself completed normally alongside the announcement.
    await expect(page.getByText(MOCK_LLM_REPLY).first()).toBeVisible({ timeout: 20_000 });
  });

  test('a chain that stops paused for any other reason informs instead', async ({ page }) => {
    await injectPausedChainComplete(page, 'paused');
    await openGroupExpedition(page);

    await sendAMessage(page, 'And now?');

    const infos = page.locator('[role="toast-container"] .qt-toast-info');
    await expect(infos).toHaveText([INFO], { timeout: 20_000 });
    // The reason picks the register: the failure sentence is NOT the one shown.
    await expect(page.getByText(WARNING)).toHaveCount(0);
  });
});
