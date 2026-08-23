import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { startMockLlm, MOCK_LLM_REPLY, type MockLlm } from './support/mock-llm';

/**
 * P4.D109 — the attachment-failure ledger's reader, live (v4 `a14a1811`, bug 94).
 *
 * ORDERING: rides the SHARED global-setup server and unlocks it, so the filename
 * must sort after `aa-foundation.spec.ts` and before the `zz…` destructives.
 * The send lands in "Group Expedition", NOT "Solo Voyage" — the P4.6ap chat-
 * totals beat asserts a hardcoded token baseline for Solo Voyage and every
 * message sent there shifts it (the `salon-thinking-indicator` note).
 *
 * ## Why the ledger is INJECTED at the wire rather than provoked
 *
 * A failed ledger is a provider-plugin report, and after this round's bug-91
 * fix the host no longer hands an image to a plugin that cannot send one: an
 * untransportable attachment routes to the describer instead. What remains is a
 * per-attachment failure INSIDE a transporting plugin (an unsupported MIME on
 * NanoGPT, say) — unreachable from the e2e's mock OpenAI-compatible endpoint,
 * which serves chat completions and knows nothing of attachments.
 *
 * So this beat fakes exactly one thing — the bytes of one `done` frame — and
 * drives everything downstream for real: the live `EventSource`, the transport's
 * parse, the pure reducer's carry, the Salon's single toast door, and the real
 * toast stack in the real DOM. The injection is a wire-level init script, not a
 * component stub; nothing in the app is mocked. (💸 The live proof on real data
 * rides the P4.D106/P4.D107 dogfood walk.)
 */

/** The two failures the injected ledger reports, first one first. */
const FIRST_ERROR = 'the mock endpoint declined the picture';
const SECOND_ERROR = 'and it declined the second one too';

/** v4's plural sentence, with the count of the failures NOT shown. */
const EXPECTED_TOAST = `2 attachments were not sent to the model (and 1 more): ${FIRST_ERROR}`;

/**
 * Rewrite the FIRST `done` frame this page receives so it carries a ledger with
 * two failures. Patches the `onmessage` accessor on `EventSource.prototype`
 * before app boot, which is where the real transport assigns its handler
 * (`core-transport.ts:151`); every other frame passes through untouched.
 */
async function injectFailedLedger(page: Page): Promise<void> {
  await page.addInitScript(
    ([firstError, secondError]) => {
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
          const wrapped = (ev: MessageEvent<string>) => {
            let data = ev.data;
            if (!injected && typeof data === 'string') {
              try {
                const frame = JSON.parse(data) as Record<string, unknown>;
                if (frame['done'] === true) {
                  frame['attachmentResults'] = {
                    sent: [],
                    failed: [
                      { id: 'e2e-attachment-1', error: firstError },
                      { id: 'e2e-attachment-2', error: secondError },
                    ],
                  };
                  data = JSON.stringify(frame);
                  injected = true;
                }
              } catch {
                /* not JSON (a keep-alive) — pass it through untouched */
              }
            }
            (handler as (e: MessageEvent<string>) => void)(
              new MessageEvent('message', { data }),
            );
          };
          nativeSet.call(this, wrapped);
        },
      });
    },
    [FIRST_ERROR, SECOND_ERROR] as const,
  );
}

test.describe('P4.D109 — the dropped-attachment warning (LIVE)', () => {
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

  test('a done frame reporting dropped attachments raises v4’s warning toast', async ({ page }) => {
    await injectFailedLedger(page);
    await page.goto('/salon');
    await maybeUnlock(page);

    const card = page.locator('.chat-card-stack a.qt-entity-card', {
      hasText: 'Group Expedition',
    });
    await expect(card).toBeVisible({ timeout: 15_000 });
    await card.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });

    // Nothing is warning before the turn.
    const warnings = page.locator('[role="toast-container"] .qt-toast-warning');
    await expect(warnings).toHaveCount(0);

    // The composer is the qt-rich-editor: drive the contenteditable with real keys.
    const composer = page.locator('.qt-chat-composer-input .qt-rich-editor-content');
    await composer.click();
    await page.keyboard.type('Did the picture arrive?');
    await page.keyboard.press('Enter');

    // The warning carries v4's sentence: the plural arm, the `(and N more)`
    // suffix BEFORE the colon, and only the FIRST plugin error. One assertion,
    // not two: it pins the text AND that exactly one warning stands, and a
    // toast expires on v4's 3000 ms timer, so a second assertion afterwards
    // would be racing that timer for no extra proof.
    await expect(warnings).toHaveText([EXPECTED_TOAST], { timeout: 20_000 });
    // The second error is counted, never shown.
    await expect(page.getByText(SECOND_ERROR)).toHaveCount(0);

    // And the turn itself completed normally alongside the warning.
    await expect(page.getByText(MOCK_LLM_REPLY).first()).toBeVisible({ timeout: 20_000 });
  });
});
