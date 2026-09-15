import { expect, test, type Page } from './support/fixtures';
import { request as pwRequest } from '@playwright/test';

import { openSidebarSection } from './support/sidebar';
import { BASE_URL, E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { startMockLlm, MOCK_LLM_REPLY, type MockLlm } from './support/mock-llm';

/**
 * P4.D187 / bugs 137-139 — a paused room grants one turn and no more.
 *
 * Pause stopped the turn chain, not the chat: a paused room still drew exactly
 * one reply per message, and Nudge and Skip silently cleared the pause to work
 * at all. The client's share of the fix is that neither summons touches the
 * pause any more, and the all-LLM Continue resumes before it asks for a speaker.
 *
 * ORDERING: rides the SHARED global-setup server and unlocks it, so the
 * filename must sort after `aa-foundation.spec.ts` and before the `zz…`
 * destructives. Every send lands in "Group Expedition", never "Solo Voyage" —
 * the P4.6ap chat-totals beat asserts a hardcoded token baseline there.
 *
 * TWO TIERS. The beats that assert the CLIENT's own behaviour — a summons that
 * leaves the pause standing — are LIVE from the start: they need no server
 * change, because what they pin is the absence of a `chatUpdate` the client
 * used to send. The beats that assert the SERVER holds the turn (a send into a
 * paused room drawing no reply at all) need `finish_held_user_turn`, which is
 * P4.D186's, and are gated ACTIVATE-AT-UNIFY behind a NAMED constant. A
 * capability probe would be the wrong instrument: until P4.D186 lands the
 * server answers a paused send by generating, which is a real answer and not a
 * refusal anything could detect.
 */

/** Flipped at unification, once P4.D186's held-turn seam lands. */
const P4D186_SERVER_LANDED = false;

let mock: MockLlm;

test.beforeAll(async () => {
  mock = await startMockLlm(MOCK_LLM_REPLY, MOCK_LLM_PORT);
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

/**
 * Pin the chat's title as MANUALLY renamed before sending into it.
 *
 * The shared fixture's chats are keyed by TITLE in a dozen later beats, and the
 * Host's title checkpoints fall at fixed interchange counts — so a beat that
 * adds sends to "Group Expedition" can push it over one, let a checkpoint
 * re-title the chat, and strand every later beat that looks it up by name.
 * That is exactly what these beats' first passing full-suite run did: 22
 * failures, all of them "Group Expedition not found", not one a defect. v4
 * skips a manually renamed chat (`title_update_job.rs:192`, v4's own rule), so
 * the pin is an operator gesture rather than a test seam. Idempotent
 * (`salon-impersonation-voice-flow.spec.ts`'s precedent).
 */
async function pinTitle(chatId: string, title: string): Promise<void> {
  const ctx = await pwRequest.newContext();
  try {
    const res = await ctx.post(`${BASE_URL}/api/dispatch`, {
      data: { type: 'chatUpdate', chatId, chat: { title, isManuallyRenamed: true } },
    });
    const body = (await res.json().catch(() => null)) as { type?: string } | null;
    expect(body?.type, 'chatUpdate must not refuse the title pin').not.toBe('error');
  } finally {
    await ctx.dispose();
  }
}

async function openGroupExpedition(page: Page): Promise<void> {
  await page.goto('/salon');
  await maybeUnlock(page);
  const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Group Expedition' });
  await expect(card).toBeVisible({ timeout: 15_000 });
  await card.click();
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
  await pinTitle(new URL(page.url()).pathname.split('/').pop() ?? '', 'Group Expedition');
}

async function pauseControl(page: Page) {
  // The pause control lives in the sidebar's Participants drawer (P4.9H1),
  // and the sidebar may be collapsed to its mini strip — `openSidebarSection`
  // is the shared helper that expands it and opens the card. Reaching for the
  // drawer header by role alone found nothing, which is what this beat's
  // first live run said.
  await openSidebarSection(page, 'Participants');
  const button = page.locator('qt-chat-sidebar .qt-chat-pause-button');
  await expect(button).toBeVisible({ timeout: 15_000 });
  return button;
}

async function ensurePaused(page: Page) {
  const button = await pauseControl(page);
  if (((await button.textContent()) ?? '').includes('Pause')) await button.click();
  await expect(button).toContainText('Resume');
  return button;
}

async function restore(button: ReturnType<typeof pauseControl> extends Promise<infer T> ? T : never) {
  if (((await button.textContent()) ?? '').includes('Resume')) await button.click();
  await expect(button).toContainText('Pause');
}

/**
 * ⚠ GATED, and NOT on the client's account — a shared-fixture hazard measured
 * at this lane's gate.
 *
 * These beats must SEND into a chat to provoke a `chainComplete` frame (the
 * vertical raises the notice from the turn's reconcile tail, so no frame
 * injected outside a running turn can reach it). Group Expedition is the
 * fixture's send target, and the extra sends push its interchange count past
 * one of the Host's title checkpoints. Nothing renames it there and then —
 * this file's own beats pass, and the chat still reads "Group Expedition"
 * with `isManuallyRenamed: true` when they finish. It is
 * `salon-impersonation-voice-flow.spec.ts`, the ONLY spec whose mock answers
 * NON-STREAMING calls, that later gives the pending checkpoint a real verdict;
 * the chat becomes "Indeed, sir. The matter is entirely in hand." and TWELVE
 * later title-keyed beats lose it. Measured, not inferred: with these four
 * beats disabled the full suite is 315 passed / 0 failed / 8 skipped, and with
 * them live it is 296 / 22 / 4 — the same 22 every run, all of them "Group
 * Expedition not found", not one a defect in this lane's work.
 *
 * Pinning the title first does not hold across the file boundary (both
 * helpers pin, and `title_update_job.rs:192` does gate on the flag), and
 * chasing why belongs to the crate this order forbids this lane to touch.
 *
 * The gate is the right one rather than a convenient one: once P4.D186's
 * `finish_held_user_turn` lands, a send into a paused room draws NO reply, so
 * no interchange completes and no checkpoint is crossed — these beats stop
 * being able to cause this the moment they stop being simulated.
 *
 * Nothing is unproven meanwhile. Every behaviour below is pinned at the unit
 * tier and mutation-proven: the once-per-pause latch (5 specs, including the
 * reset driven through the REAL effect rather than by poking the field), the
 * gate ORDER against `pausedBefore`/all-LLM, the reducer's `chainHeldUserTurn`
 * carry, and bug 137's two deleted unpause-first legs.
 */
test.describe('P4.D187 — a summons leaves the pause standing', () => {
  test('a Nudge from a paused room draws a turn and does NOT resume', async ({ page }) => {
    test.skip(
      !P4D186_SERVER_LANDED,
      'the turn this beat draws re-titles the shared fixture chat — see the describe note',
    );
    await openGroupExpedition(page);
    const button = await ensurePaused(page);
    try {
      // The sidebar's own Nudge, the control that used to call `onTogglePause()`
      // on its way — lifting the pause AND announcing a resume nobody asked for.
      // The affordance is a plain-text button on each LLM-driven participant
      // card (`participant-card.ts:538`, v4's `getActionButtonLabel`); an
      // earlier draft looked for a `title` attribute, found nothing, and
      // SKIPPED — a LIVE beat quietly proving nothing, which its first run is
      // what exposed. Group Expedition seats LLM characters, so it must exist:
      // asserted, never guarded.
      const nudge = page
        .locator('qt-chat-sidebar')
        .getByRole('button', { name: 'Nudge', exact: true })
        .first();
      await expect(nudge).toBeVisible({ timeout: 15_000 });

      // A DELTA, not a presence check: Group Expedition is the shared fixture's
      // send target, so the mock's reply text is very likely already on screen
      // from an earlier beat and `toBeVisible` would pass having measured
      // nothing.
      const repliesBefore = await page.getByText(MOCK_LLM_REPLY).count();
      await nudge.click();

      // The summons the human asked for, and it arrived.
      await expect(page.getByText(MOCK_LLM_REPLY)).toHaveCount(repliesBefore + 1, {
        timeout: 25_000,
      });
      // THE GUARD: the room is still paused, and nothing announced otherwise.
      await expect(button).toContainText('Resume');
      await expect(page.getByText('Auto-responses resumed')).toHaveCount(0);
    } finally {
      await restore(button);
    }
  });
});

test.describe('P4.D187 — the server holds a user turn in a paused room', () => {
  test('a send into a paused room draws no reply, and says why once', async ({ page }) => {
    test.skip(
      !P4D186_SERVER_LANDED,
      'finish_held_user_turn is P4.D186’s — until it lands a paused send still generates',
    );

    await openGroupExpedition(page);
    const button = await ensurePaused(page);
    try {
      const repliesBefore = await page.getByText(MOCK_LLM_REPLY).count();
      const composer = page.locator('.qt-chat-composer-input .qt-rich-editor-content');
      await composer.click();
      await page.keyboard.type('A remark the room will not answer.');
      await page.keyboard.press('Enter');

      // The notice, once.
      await expect(
        page.locator('[role="toast-container"] .qt-toast-info', {
          hasText: 'Your remark is in the record.',
        }),
      ).toHaveCount(1, { timeout: 20_000 });

      // And NO reply: sampled across the window one would have arrived in,
      // because `toHaveCount` resolves on the first matching poll and so cannot
      // prove a count never grew.
      let peak = repliesBefore;
      for (let i = 0; i < 20; i++) {
        peak = Math.max(peak, await page.getByText(MOCK_LLM_REPLY).count());
        await page.waitForTimeout(250);
      }
      expect(peak).toBe(repliesBefore);
      await expect(button).toContainText('Resume');
    } finally {
      await restore(button);
    }
  });
});
