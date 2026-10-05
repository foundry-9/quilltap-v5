import { expect, test, type Page } from './support/fixtures';

import { request as pwRequest } from '@playwright/test';

import { BASE_URL, E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { startMockLlm, type MockLlm } from './support/mock-llm';
import { openSidebarSection } from './support/sidebar';

/**
 * P4.D181 — In Their Own Words, end to end (v4 `686954937`), re-shaped by
 * P4.D252 for v4 `07b8f0209`'s three-way mode.
 *
 * The operator takes a character's seat with Impersonate, types a line, and it
 * does NOT post: the draft is stashed and the review dialog opens on it. Under
 * `always` the seat's own model restates it at once; under `ask` the dialog
 * waits on the draft and NO model is called until the operator presses Restate.
 * Every exit that is not a Send leaves the draft exactly where it was — that
 * invariant is the whole reason the composer stopped clearing itself on emit
 * (P4.D181 unit 3), and it is what this beat exists to prove against a real
 * browser, a real binary and a real stream.
 *
 * ORDERING: rides the shared global-setup server, so the filename must sort after
 * `aa-foundation.spec.ts` ('sa' > 'aa') and before the `zz…` destructives.
 *
 * ⚠ **ACTIVATE-AT-UNIFY behind {@link P4D251_SERVER_LANDED}.** The three radios
 * write `impersonationVoiceMode`, the `chat_settings` column the SIBLING lane
 * P4.D251 puts in place of the retired boolean `impersonationVoiceRewrite`; on
 * this lane's own branch the server still speaks the boolean, so a live run
 * would fail for the right reason. A NAMED CONSTANT, never a capability probe
 * (`round-plan-takeaways`): an unknown settings key reaches a DEFINED verb, so a
 * probe would read "ready" against a server that cannot store it. The unifier
 * flips it (§S.1); activating a re-shaped beat is its first execution under the
 * new shape, so expect gesture fixes.
 *
 * **Real-spend guard.** Nothing here can reach a live model: the e2e instance
 * carries no API keys, and the fixture's OPENAI_COMPATIBLE profile is rewritten by
 * global setup to the mock below, which is what answers the rehearsal.
 */

/** @see the header — P4.D251 replaces the boolean column with `impersonationVoiceMode`. */
const P4D251_SERVER_LANDED = true;
const PARKED = 'awaits P4.D251: the chat_settings.impersonationVoiceMode column + its route arm';

type VoiceMode = 'off' | 'ask' | 'always';
/** v4's radio labels (`ImpersonationVoiceSettings.tsx:11-31`). */
const MODE_LABEL: Record<VoiceMode, string> = {
  off: 'Never',
  ask: 'Ask each time',
  always: 'Always restate',
};

/** What the mock restates every draft as — distinct from anything a human types. */
const REHEARSED = 'Indeed, sir. The matter is entirely in hand.';

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

/** Pick the instance mode through the UI, as an operator would: the radio by its label. */
async function setVoiceMode(page: Page, mode: VoiceMode): Promise<void> {
  await page.goto('/settings?tab=chat&section=composer-spellcheck');
  const row = page
    .locator('qt-impersonation-voice-settings label.qt-settings-toggle-row')
    .filter({
      has: page.locator('.font-medium', { hasText: new RegExp(`^${MODE_LABEL[mode]}$`) }),
    });
  const radio = row.locator('input[type="radio"]');
  await expect(radio).toBeVisible({ timeout: 15_000 });
  if (await radio.isChecked()) return;
  const saved = page.waitForResponse(
    (r) =>
      r.url().includes('/api/dispatch') &&
      (r.request().postData() ?? '').includes('impersonationVoiceMode'),
  );
  await radio.check();
  await saved;
}

/** v4's portrait cue for each armed mode (`SpeakingAsAvatar.tsx:21-39`). */
function portraitCue(mode: 'ask' | 'always', name: string): string {
  return mode === 'always'
    ? `Speaking as ${name} — your draft goes to ${name} to say in their own words first`
    : `Speaking as ${name} — your draft opens for review; send it as written or have ${name} restate it`;
}

/**
 * Count the page's `chatImpersonationVoicePreview` dispatches — the ONE call a
 * rehearsal spends. The mock LLM keeps no request log, and the SPA's own
 * dispatch is the instrument closest to the decision anyway: no preview
 * dispatch, no model call.
 */
function countPreviews(page: Page): () => number {
  let n = 0;
  page.on('request', (req) => {
    if (
      req.url().includes('/api/dispatch') &&
      (req.postData() ?? '').includes('chatImpersonationVoicePreview')
    ) {
      n += 1;
    }
  });
  return () => n;
}

/** Raw dispatch against the real axum server (the `chat-delete-flow` idiom). */
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

/**
 * How many persisted USER lines carry `text` — the proof a Send actually
 * posted. The mock answers EVERY completion with the same `REHEARSED` text, so
 * the newest bubble may already read `REHEARSED` (an LLM seat's reply from an
 * earlier beat) and a bubble poll alone passes whether or not Send posted
 * anything (the §3 review of the `07b8f0209` unification).
 */
async function userLinesWith(chatId: string, text: string): Promise<number> {
  const resp = await dispatch({ type: 'chatGet', chatId });
  const chat = (resp['chat'] as Record<string, unknown> | undefined) ?? {};
  const messages = (chat['messages'] as Array<Record<string, unknown>> | undefined) ?? [];
  return messages.filter((m) => m['role'] === 'USER' && String(m['content'] ?? '').includes(text))
    .length;
}

/**
 * Open the fixture's "Group Expedition" — the multi-seat chat the other
 * send-beats (`m4-salon`, `smart-typography-flow`) already share, and the one
 * no beat asserts token totals on ("Solo Voyage" is `salon-token-cost-flow`'s,
 * and this spec's sends moved its totals in the `f4ad2c8d1` unification's
 * full-suite run). Returns the chat id.
 */
async function openGroupExpedition(page: Page): Promise<string> {
  await page.goto('/salon');
  await maybeUnlock(page);
  const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Group Expedition' });
  await expect(card).toBeVisible({ timeout: 15_000 });
  await card.click();
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
  const id = new URL(page.url()).pathname.split('/').pop()!;
  expect(id).toBeTruthy();
  return id;
}

/**
 * Pin the chat's title as MANUALLY renamed. This spec's mock answers
 * non-streaming calls (see `MockLlmOptions.nonStreaming`), so the Host's title
 * checkpoints get a real verdict and would re-title the chat from the mock's
 * words — which is how "Group Expedition" became "The kettle is on. Do come
 * in." for six later beats in the unification's full-suite run. v4 skips a
 * manually renamed chat (`title_update_job.rs:192`, v4's own rule), so the pin
 * is an operator gesture, not a test seam. Idempotent.
 */
async function pinTitle(chatId: string, title: string): Promise<void> {
  const resp = await dispatch({
    type: 'chatUpdate',
    chatId,
    chat: { title, isManuallyRenamed: true },
  });
  expect(
    resp['type'],
    `chatUpdate must not refuse the title pin: ${JSON.stringify(resp)}`,
  ).not.toBe('error');
}

/** The chain is settled when no Stop button stands in for Send. */
async function waitForFloor(page: Page): Promise<void> {
  await expect(page.locator('.qt-chat-stop-button')).toHaveCount(0, { timeout: 45_000 });
}

/**
 * Take the LLM seat, the way `salon-dialogs-flow`'s impersonation beat does.
 * Returns the character's name.
 */
async function impersonateSeat(page: Page, name: string): Promise<string> {
  await openSidebarSection(page, 'Participants');
  // A sibling beat that died mid-flow can leave a seat impersonated on the
  // shared instance; release it first so this beat starts from the same place
  // every time. And take the seat by NAME — the full suite reorders the
  // participant cards, and "the first Speak as button" drifted onto Bram.
  const leftover = page.locator('qt-participant-card button[title^="Stop speaking as "]');
  if (await leftover.count()) {
    await leftover.first().click();
    await expect(
      page.locator('qt-participant-card button[title^="Speak as "]').first(),
    ).toBeVisible({ timeout: 15_000 });
  }
  const speakAs = page.locator(`qt-participant-card button[title="Speak as ${name}"]`);
  await expect(speakAs).toBeVisible({ timeout: 10_000 });
  await speakAs.click();
  const card = page.locator('qt-participant-card').filter({ hasText: name });
  await expect(card.locator('span.qt-badge-info')).toBeVisible({ timeout: 15_000 });
  return name;
}

/** Stop impersonating, so the shared instance is left as we found it. */
async function stopImpersonating(page: Page, name: string): Promise<void> {
  const card = page.locator('qt-participant-card').filter({ hasText: name });
  const stop = card.locator(`button[title="Stop speaking as ${name}"]`);
  if (await stop.count()) {
    await stop.click();
    await expect(page.locator(`qt-participant-card button[title="Speak as ${name}"]`)).toBeVisible({
      timeout: 15_000,
    });
  }
}

const composer = (page: Page) => page.locator('.qt-chat-composer-input .qt-rich-editor-content');
// The modal's `[role=dialog]`, not the `qt-impersonation-voice-dialog` host: the
// host is a box-less inline custom element whose fixed-position child IS the
// dialog, so Playwright reports the host "hidden" while the modal is on screen
// (the activated beat's first run at the `f4ad2c8d1` unification). `toHaveCount(0)`
// still measures the close, because the `@if` unmounts the host with its child.
const dialog = (page: Page) => page.locator('qt-impersonation-voice-dialog').getByRole('dialog');

async function typeAndEnter(page: Page, text: string): Promise<void> {
  // A multi-seat chat answers a line with a streamed chain; an Enter pressed
  // while the Stop button stands in for Send is swallowed by the composer's gate.
  await waitForFloor(page);
  await composer(page).click();
  await page.keyboard.type(text);
  await page.keyboard.press('Enter');
}

/**
 * Make the composer speak as `name` with In Their Own Words ARMED again (the
 * portrait's title carries the cue) after the rotation has moved off the seat.
 */
async function retakeSeat(page: Page, name: string, mode: 'ask' | 'always'): Promise<void> {
  await waitForFloor(page);
  const portrait = page.locator('.qt-speaking-as-avatar');
  const armed = portraitCue(mode, name);
  if ((await portrait.getAttribute('title')) === armed) return;
  // The rotation has moved the composer onto the owner persona (no Skip banner
  // there — the banner is an impersonated seat's). The operator's route back is
  // the participant card: release the seat and take it again, which v4's
  // `impersonate` answers by making that seat the active typing seat (P4.D60's
  // turn override above the server's rotation).
  await openSidebarSection(page, 'Participants');
  await stopImpersonating(page, name);
  const speakAs = page.locator(`qt-participant-card button[title="Speak as ${name}"]`);
  await expect(speakAs).toBeVisible({ timeout: 15_000 });
  await speakAs.click();
  await expect(portrait).toHaveAttribute('title', armed, { timeout: 15_000 });
}

/**
 * The newest rendered bubble's text. `qt-message-row` is the component's ELEMENT
 * name, not a class — the activated beat's first runs counted `.qt-message-row`
 * and saw 0 rows before and after every send (the `f4ad2c8d1` unification).
 */
async function newestBubble(page: Page): Promise<string> {
  const rows = page.locator('.qt-chat-messages-list qt-message-row');
  const n = await rows.count();
  return ((await rows.nth(n - 1).textContent()) ?? '').trim();
}

test.describe('P4.D181 — In Their Own Words, the full round trip', () => {
  let mock: MockLlm;

  test.beforeAll(async () => {
    // A slow-ish stream so the dialog's "Rehearsing…" state is genuinely
    // observable rather than settled before the first assertion polls.
    mock = await startMockLlm(REHEARSED, MOCK_LLM_PORT, 150, { nonStreaming: true });
  });
  test.afterAll(async () => {
    await mock?.close();
  });

  test('always: a typed line is stashed, restated at once, reviewed, and posted in the character’s own words', async ({
    page,
  }) => {
    test.skip(!P4D251_SERVER_LANDED, PARKED);

    await page.goto('/salon');
    await maybeUnlock(page);
    await setVoiceMode(page, 'always');
    const chatId = await openGroupExpedition(page);
    await pinTitle(chatId, 'Group Expedition');
    const name = await impersonateSeat(page, 'Aria');
    try {
      // The CUE, before anything is typed: the portrait wears the quill badge and
      // says what a send will do. The badge's positioning is the one thing no unit
      // spec can see (jsdom runs no cascade), so assert the computed box here —
      // inside the portrait, bottom-right, which needs BOTH its own
      // `position: absolute` and the wrapper's `position: relative`.
      const portrait = page.locator('.qt-speaking-as-avatar');
      await expect(portrait).toHaveAttribute('title', portraitCue('always', name), {
        timeout: 15_000,
      });
      const badge = portrait.locator('.qt-speaking-as-avatar-voice-badge');
      await expect(badge).toBeVisible({ timeout: 15_000 });
      expect(await badge.evaluate((el) => getComputedStyle(el).position)).toBe('absolute');
      expect(await portrait.evaluate((el) => getComputedStyle(el).position)).toBe('relative');
      const outer = (await portrait.boundingBox())!;
      const inner = (await badge.boundingBox())!;
      expect(inner.x).toBeGreaterThan(outer.x);
      expect(inner.y).toBeGreaterThan(outer.y);
      expect(inner.x + inner.width).toBeLessThanOrEqual(outer.x + outer.width + 1);
      expect(inner.y + inner.height).toBeLessThanOrEqual(outer.y + outer.height + 1);

      const DRAFT = 'Tell the harbourmaster we sail at dawn.';

      await typeAndEnter(page, DRAFT);

      // NOTHING posted, and the dialog opened on the draft.
      await expect(dialog(page)).toBeVisible({ timeout: 15_000 });
      await expect(page.locator('.qt-dialog-title')).toHaveText(`In ${name}'s own words`);
      await expect(dialog(page).locator('[aria-label="Your draft"]')).toContainText(DRAFT, {
        timeout: 15_000,
      });
      // Nothing posted while the dialog is up: the newest bubble is not the draft
      // (a row COUNT is not an instrument here — the list is virtualized).
      expect(await newestBubble(page)).not.toContain(DRAFT);

      // The composer was NOT cleared — the whole point of unit 3's restructure.
      await expect(composer(page)).toContainText(DRAFT);

      // The mock's restatement arrives as the proposal.
      await expect(dialog(page).locator(`[aria-label="What ${name} will say"]`)).toContainText(
        REHEARSED,
        { timeout: 20_000 },
      );

      // ── Edit original: back to the composer, draft intact, nothing posted. ──
      await dialog(page).getByRole('button', { name: 'Edit original' }).click();
      await expect(dialog(page)).toHaveCount(0, { timeout: 15_000 });
      await expect(composer(page)).toContainText(DRAFT);
      // Nothing posted while the dialog is up: the newest bubble is not the draft
      // (a row COUNT is not an instrument here — the list is virtualized).
      expect(await newestBubble(page)).not.toContain(DRAFT);

      // ── Resend, then Send as written: the operator's OWN bytes post. ──
      await composer(page).click();
      await page.keyboard.press('Enter');
      await expect(dialog(page)).toBeVisible({ timeout: 15_000 });
      await expect(dialog(page).locator(`[aria-label="What ${name} will say"]`)).toContainText(
        REHEARSED,
        { timeout: 20_000 },
      );
      await dialog(page).getByRole('button', { name: 'Send as written' }).click();
      await expect(dialog(page)).toHaveCount(0, { timeout: 15_000 });
      await expect.poll(async () => await newestBubble(page), { timeout: 20_000 }).toContain(DRAFT);
      // …and a Send DOES clear the composer, exactly as an ordinary send does.
      await expect(composer(page)).not.toContainText(DRAFT, { timeout: 15_000 });

      // ── A second line, sent as the PROPOSAL this time. ──
      // v4's bug-49 rule: after the impersonated seat's line the composer's
      // speaking-as FOLLOWS the rotation, and in this solo chat that lands on
      // the owner persona (Cleo), whom the gate never rehearses — the activated
      // beat's first isolated run posted the second line as Cleo. The operator
      // gets Aria's seat back from the participant card (release + Speak as),
      // so the beat does the same until the portrait's title says the gate is
      // armed for Aria again.
      await retakeSeat(page, name, 'always');
      const SECOND = 'And have the charts brought up.';
      await typeAndEnter(page, SECOND);
      await expect(dialog(page)).toBeVisible({ timeout: 15_000 });
      await expect(dialog(page).locator(`[aria-label="What ${name} will say"]`)).toContainText(
        REHEARSED,
        { timeout: 20_000 },
      );
      const postedBefore = await userLinesWith(chatId, REHEARSED);
      await dialog(page).getByRole('button', { name: 'Send', exact: true }).click();
      await expect(dialog(page)).toHaveCount(0, { timeout: 15_000 });
      await expect
        .poll(async () => await newestBubble(page), { timeout: 20_000 })
        .toContain(REHEARSED);
      await expect
        .poll(async () => await userLinesWith(chatId, REHEARSED), { timeout: 20_000 })
        .toBe(postedBefore + 1);
    } finally {
      // Leave the shared instance as we found it even when an assertion above
      // fails — the sibling beats read the same seat and the same setting.
      await waitForFloor(page);
      await openSidebarSection(page, 'Participants');
      await stopImpersonating(page, name);
      await setVoiceMode(page, 'off');
    }
  });

  test('ask: the dialog opens on the draft with NO model call; Restate is the one call, and Send posts it', async ({
    page,
  }) => {
    test.skip(!P4D251_SERVER_LANDED, PARKED);

    const previews = countPreviews(page);
    await page.goto('/salon');
    await maybeUnlock(page);
    await setVoiceMode(page, 'ask');
    const chatId = await openGroupExpedition(page);
    await pinTitle(chatId, 'Group Expedition');
    const name = await impersonateSeat(page, 'Aria');
    try {
      // The portrait and the Send button say the SAME thing, through v4's one
      // `voiceRehearsalTitle`.
      await expect(page.locator('.qt-speaking-as-avatar')).toHaveAttribute(
        'title',
        portraitCue('ask', name),
        { timeout: 15_000 },
      );
      await waitForFloor(page);
      await expect(page.locator('.qt-chat-composer-send')).toHaveAttribute(
        'title',
        `Opens your draft for review — send it as written or have ${name} restate it`,
      );

      const DRAFT = 'Ask the purser for the manifest.';
      await typeAndEnter(page, DRAFT);

      // The dialog opens on the draft, in its DRAFT state: Send as written is
      // the primary door, Restate sits beside it, and there is no proposal.
      await expect(dialog(page)).toBeVisible({ timeout: 15_000 });
      await expect(dialog(page).locator('[aria-label="Your draft"]')).toContainText(DRAFT, {
        timeout: 15_000,
      });
      const asWritten = dialog(page).getByRole('button', { name: 'Send as written' });
      await expect(asWritten).toHaveClass(/qt-button-primary/);
      await expect(
        dialog(page).getByRole('button', { name: 'Restate in their voice' }),
      ).toBeVisible();
      await expect(dialog(page).getByRole('button', { name: 'Send', exact: true })).toHaveCount(0);
      await expect(dialog(page).locator(`[aria-label="What ${name} will say"]`)).toHaveCount(0);
      await expect(dialog(page)).toContainText(
        `Sending as written posts your words under ${name}'s name exactly as typed`,
      );
      // NO model was called to open it — the point of `ask`. Measured after the
      // dialog has rendered, so a call made on open would already be counted.
      expect(previews()).toBe(0);
      expect(await newestBubble(page)).not.toContain(DRAFT);

      // Restate is the ONE call, and it brings the proposal.
      await dialog(page).getByRole('button', { name: 'Restate in their voice' }).click();
      await expect(dialog(page).locator(`[aria-label="What ${name} will say"]`)).toContainText(
        REHEARSED,
        { timeout: 20_000 },
      );
      expect(previews()).toBe(1);

      // Send posts the proposal — as a NEW persisted user line, not merely a
      // bubble that happens to read `REHEARSED`.
      const postedBefore = await userLinesWith(chatId, REHEARSED);
      await dialog(page).getByRole('button', { name: 'Send', exact: true }).click();
      await expect(dialog(page)).toHaveCount(0, { timeout: 15_000 });
      await expect
        .poll(async () => await newestBubble(page), { timeout: 20_000 })
        .toContain(REHEARSED);
      await expect
        .poll(async () => await userLinesWith(chatId, REHEARSED), { timeout: 20_000 })
        .toBe(postedBefore + 1);
      expect(previews()).toBe(1);
    } finally {
      await waitForFloor(page);
      await openSidebarSection(page, 'Participants');
      await stopImpersonating(page, name);
      await setVoiceMode(page, 'off');
    }
  });

  test('an attachment-only send is never rehearsed, and the setting off lets a line straight through', async ({
    page,
  }) => {
    test.skip(!P4D251_SERVER_LANDED, PARKED);

    await page.goto('/salon');
    await maybeUnlock(page);
    await setVoiceMode(page, 'always');
    const chatId = await openGroupExpedition(page);
    await pinTitle(chatId, 'Group Expedition');
    const name = await impersonateSeat(page, 'Aria');
    try {
      // Rule 3: a send with no prose has nothing to restate. Drop a file in the
      // tray and send on it alone.
      const file = page.locator('.qt-chat-composer input[type="file"]');
      await file.setInputFiles({
        name: 'chart.png',
        mimeType: 'image/png',
        // A 1×1 PNG — the smallest thing the upload path will accept.
        buffer: Buffer.from(
          'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
          'base64',
        ),
      });
      await expect(page.locator('.qt-chat-attachment-chip')).toBeVisible({ timeout: 20_000 });

      await waitForFloor(page);
      await page.locator('.qt-chat-composer-send').click();
      // No dialog, and the message goes out as an ordinary send: the newest row
      // carries the attachment (a row COUNT is not an instrument here — the
      // list is virtualized).
      await expect(dialog(page)).toHaveCount(0);
      // The proof the send went out is the tray EMPTYING: after the composer-
      // clear restructure only a real send clears it (`clearAfterSend` from the
      // Salon's one door), and an intercepted submit leaves the chip in place.
      // (A row under the virtualized transcript is not an instrument here.)
      await expect(page.locator('.qt-chat-attachment-chip')).toHaveCount(0, { timeout: 20_000 });

      // Rule 1: turn the setting off and a plain typed line posts with no dialog.
      await setVoiceMode(page, 'off');
      await page.goto(`/salon/${chatId}`);
      await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
      const STRAIGHT = 'Straight to the room, if you please.';
      await typeAndEnter(page, STRAIGHT);
      await expect(dialog(page)).toHaveCount(0);
      await expect
        .poll(async () => await newestBubble(page), { timeout: 20_000 })
        .toContain(STRAIGHT);
    } finally {
      await waitForFloor(page);
      await openSidebarSection(page, 'Participants');
      await stopImpersonating(page, name);
      await setVoiceMode(page, 'off');
    }
  });
});
