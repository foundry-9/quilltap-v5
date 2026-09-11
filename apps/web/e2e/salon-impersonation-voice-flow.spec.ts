import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { startMockLlm, type MockLlm } from './support/mock-llm';
import { openSidebarSection } from './support/sidebar';

/**
 * P4.D181 — In Their Own Words, end to end (v4 `686954937`).
 *
 * The operator takes a character's seat with Impersonate, types a line, and it
 * does NOT post: the draft is stashed, the review dialog opens on it, and the
 * seat's own model restates it. Every exit that is not a Send leaves the draft
 * exactly where it was — that invariant is the whole reason the composer stopped
 * clearing itself on emit (unit 3), and it is what this beat exists to prove
 * against a real browser, a real binary and a real stream.
 *
 * ORDERING: rides the shared global-setup server, so the filename must sort after
 * `aa-foundation.spec.ts` ('sa' > 'aa') and before the `zz…` destructives.
 *
 * ⚠ **ACTIVATE-AT-UNIFY behind {@link P4D180_SERVER_LANDED} and
 * {@link P4D179_SERVER_LANDED}.** The dialog dispatches
 * `chatImpersonationVoicePreview`, which the SIBLING lane P4.D180 defines, and
 * arming the gate needs the `impersonationVoiceRewrite` column P4.D179 adds. Both
 * are NAMED CONSTANTS, never capability probes (`round-plan-takeaways`): a
 * DEFINED verb defeats a probe, and an unknown settings key is not an "unknown
 * variant" the way a missing verb is. The unifier flips them; activating a beat is
 * its FIRST execution, so expect gesture fixes — the likely ones are noted inline.
 *
 * **Real-spend guard.** Nothing here can reach a live model: the e2e instance
 * carries no API keys, and the fixture's OPENAI_COMPATIBLE profile is rewritten by
 * global setup to the mock below, which is what answers the rehearsal.
 */

/** @see the header — P4.D180 defines `chatImpersonationVoicePreview`. */
const P4D180_SERVER_LANDED = true;
/** @see the header — P4.D179 adds `chat_settings.impersonationVoiceRewrite`. */
const P4D179_SERVER_LANDED = true;

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

/** Flip the instance setting through the UI, as an operator would. */
async function setVoiceRewrite(page: Page, on: boolean): Promise<void> {
  await page.goto('/settings?tab=chat&section=composer-spellcheck');
  const box = page.locator('qt-impersonation-voice-settings input[type="checkbox"]');
  await expect(box).toBeVisible({ timeout: 15_000 });
  if ((await box.isChecked()) === on) return;
  const saved = page.waitForResponse(
    (r) =>
      r.url().includes('/api/dispatch') &&
      (r.request().postData() ?? '').includes('impersonationVoiceRewrite'),
  );
  await box.setChecked(on);
  await saved;
}

async function openSoloVoyage(page: Page): Promise<string> {
  await page.goto('/salon');
  await maybeUnlock(page);
  const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Solo Voyage' });
  await expect(card).toBeVisible({ timeout: 15_000 });
  await card.click();
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
  const id = new URL(page.url()).pathname.split('/').pop()!;
  expect(id).toBeTruthy();
  return id;
}

/**
 * Take the LLM seat, the way `salon-dialogs-flow`'s impersonation beat does.
 * Returns the character's name.
 */
async function impersonateFirstSeat(page: Page): Promise<string> {
  await openSidebarSection(page, 'Participants');
  // A sibling beat that died mid-flow can leave the seat impersonated on the
  // shared instance; release it first so this beat starts from the same place
  // every time.
  const leftover = page.locator('qt-participant-card button[title^="Stop speaking as "]');
  if (await leftover.count()) {
    await leftover.first().click();
    await expect(
      page.locator('qt-participant-card button[title^="Speak as "]').first(),
    ).toBeVisible({ timeout: 15_000 });
  }
  const speakAs = page.locator('qt-participant-card button[title^="Speak as "]').first();
  await expect(speakAs).toBeVisible({ timeout: 10_000 });
  const name = (await speakAs.getAttribute('title'))!.replace('Speak as ', '');
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
  await composer(page).click();
  await page.keyboard.type(text);
  await page.keyboard.press('Enter');
}

/**
 * Make the composer speak as `name` with In Their Own Words ARMED again (the
 * portrait's title carries the cue) after the rotation has moved off the seat.
 */
async function retakeSeat(page: Page, name: string): Promise<void> {
  const portrait = page.locator('.qt-speaking-as-avatar');
  const armed = `Speaking as ${name} — your draft goes to ${name} to say in their own words first`;
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
    mock = await startMockLlm(REHEARSED, MOCK_LLM_PORT, 150);
  });
  test.afterAll(async () => {
    await mock?.close();
  });

  test('a typed line is stashed, reviewed, and posted in the character’s own words', async ({
    page,
  }) => {
    test.skip(
      !P4D179_SERVER_LANDED || !P4D180_SERVER_LANDED,
      'awaits P4.D179 (the impersonationVoiceRewrite column) and P4.D180 (the chatImpersonationVoicePreview verb)',
    );

    await page.goto('/salon');
    await maybeUnlock(page);
    await setVoiceRewrite(page, true);
    await openSoloVoyage(page);
    const name = await impersonateFirstSeat(page);
    try {
      // The CUE, before anything is typed: the portrait wears the quill badge and
      // says what a send will do. The badge's positioning is the one thing no unit
      // spec can see (jsdom runs no cascade), so assert the computed box here —
      // inside the portrait, bottom-right, which needs BOTH its own
      // `position: absolute` and the wrapper's `position: relative`.
      const portrait = page.locator('.qt-speaking-as-avatar');
      await expect(portrait).toHaveAttribute(
        'title',
        `Speaking as ${name} — your draft goes to ${name} to say in their own words first`,
        { timeout: 15_000 },
      );
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
      const before = await page.locator('.qt-chat-messages-list qt-message-row').count();

      await typeAndEnter(page, DRAFT);

      // NOTHING posted, and the dialog opened on the draft.
      await expect(dialog(page)).toBeVisible({ timeout: 15_000 });
      await expect(page.locator('.qt-dialog-title')).toHaveText(`In ${name}'s own words`);
      await expect(dialog(page).locator('[aria-label="Your draft"]')).toContainText(DRAFT, {
        timeout: 15_000,
      });
      expect(await page.locator('.qt-chat-messages-list qt-message-row').count()).toBe(before);

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
      expect(await page.locator('.qt-chat-messages-list qt-message-row').count()).toBe(before);

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
      await retakeSeat(page, name);
      const SECOND = 'And have the charts brought up.';
      await typeAndEnter(page, SECOND);
      await expect(dialog(page)).toBeVisible({ timeout: 15_000 });
      await expect(dialog(page).locator(`[aria-label="What ${name} will say"]`)).toContainText(
        REHEARSED,
        { timeout: 20_000 },
      );
      await dialog(page).getByRole('button', { name: 'Send', exact: true }).click();
      await expect(dialog(page)).toHaveCount(0, { timeout: 15_000 });
      await expect
        .poll(async () => await newestBubble(page), { timeout: 20_000 })
        .toContain(REHEARSED);
    } finally {
      // Leave the shared instance as we found it even when an assertion above
      // fails — the sibling beats read the same seat and the same setting.
      await openSidebarSection(page, 'Participants');
      await stopImpersonating(page, name);
      await setVoiceRewrite(page, false);
    }
  });

  test('an attachment-only send is never rehearsed, and the setting off lets a line straight through', async ({
    page,
  }) => {
    test.skip(
      !P4D179_SERVER_LANDED || !P4D180_SERVER_LANDED,
      'awaits P4.D179 (the impersonationVoiceRewrite column) and P4.D180 (the chatImpersonationVoicePreview verb)',
    );

    await page.goto('/salon');
    await maybeUnlock(page);
    await setVoiceRewrite(page, true);
    await openSoloVoyage(page);
    const name = await impersonateFirstSeat(page);

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

    const before = await page.locator('.qt-chat-messages-list qt-message-row').count();
    await page.locator('.qt-chat-composer-send').click();
    // No dialog, and the message goes out as an ordinary send.
    await expect(dialog(page)).toHaveCount(0);
    await expect
      .poll(async () => await page.locator('.qt-chat-messages-list qt-message-row').count(), {
        timeout: 20_000,
      })
      .toBeGreaterThan(before);

    // Rule 1: turn the setting off and a plain typed line posts with no dialog.
    await setVoiceRewrite(page, false);
    await openSoloVoyage(page);
    const afterAttachment = await page.locator('.qt-chat-messages-list qt-message-row').count();
    await typeAndEnter(page, 'Straight to the room, if you please.');
    await expect(dialog(page)).toHaveCount(0);
    await expect
      .poll(async () => await page.locator('.qt-chat-messages-list qt-message-row').count(), {
        timeout: 20_000,
      })
      .toBeGreaterThan(afterAttachment);

    await openSidebarSection(page, 'Participants');
    await stopImpersonating(page, name);
  });
});
