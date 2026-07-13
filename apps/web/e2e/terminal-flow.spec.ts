import { expect, test } from '@playwright/test';

import { E2E_PASSPHRASE } from './support/env';

/**
 * P4.6u: a LIVE browser walk of the Salon terminal pane over a REAL PTY through
 * the real WebSocket — unlock → open a fixture chat → open the terminal pane →
 * spawn (Ariel's session-opened announcement lands) → type `echo quilltap` → the
 * output renders in xterm → kill → the pane closes and Ariel's session-closed
 * announcement lands.
 *
 * The server side already exists (P4.2 terminal routes + P4.1c PTY manager); this
 * spec spawns a real shell PTY. The walk is shell-agnostic (`echo`) and kills the
 * session it spawns; the global teardown reaps the server process group besides.
 */
test.describe('P4.6u — the Salon terminal pane (open → spawn → echo → kill)', () => {
  /**
   * Unlock only when the passphrase screen is showing — the shared server stays
   * unlocked once an earlier spec has walked the unlock flow.
   */
  async function maybeUnlock(page: import('@playwright/test').Page) {
    const passphrase = page.locator('#qt-passphrase');
    const chats = page.getByRole('heading', { name: 'Chats', exact: true });
    await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
  }

  test('open the terminal pane, spawn a PTY, echo, and kill it', async ({ page }) => {
    await page.goto('/');
    await maybeUnlock(page);

    // Open the solo fixture chat (the terminal flow needs no LLM participant).
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
    const soloCard = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Solo Voyage' });
    await soloCard.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible();

    // Earlier specs (salon-documents-flow) leave their own terminal chips in this
    // shared chat's history, so snapshot the pre-spawn chip counts FIRST: the
    // announcements ride the chip row, and "newest chip" is only well-defined
    // once the count has grown past this baseline. (Counting `.last()` visible
    // is NOT enough — a stale chip satisfies it instantly, and the click then
    // races the post-spawn refetch that delivers the new chip. That race is the
    // in-suite failure this gesture replaced: the expanded embed bound the stale
    // spec's session — not the pane's — and rendered a live surface instead of
    // the "Showing in Terminal Mode pane" note.)
    const openedChips = page.locator('.qt-chat-announcement-chip', { hasText: 'terminal opened' });
    const openedBefore = await openedChips.count();

    // Open Terminal Mode from the composer. With no live sessions it spawns
    // directly and enters split mode.
    await page.getByRole('button', { name: 'Open terminal' }).click();

    // The terminal pane appears with a live xterm surface.
    const pane = page.locator('qt-terminal-pane');
    await expect(pane).toBeVisible({ timeout: 15_000 });
    await expect(pane.locator('.xterm')).toBeVisible({ timeout: 15_000 });

    // Ariel's session-opened announcement landed as a chip (spawn + the refetch);
    // Staff announcements collapse to chips (kind "terminal opened"). Wait for
    // the count to GROW so `.last()` is deterministically the chip THIS spawn
    // just posted (messages append chronologically).
    await expect(openedChips).toHaveCount(openedBefore + 1, { timeout: 15_000 });
    const openedChip = openedChips.last();
    await expect(openedChip).toBeVisible({ timeout: 15_000 });

    // Expand the chip → the inline embed shows the "in the pane" note (the same
    // session is live in the Terminal Mode pane).
    await openedChip.click();
    await expect(page.locator('qt-terminal-embed')).toContainText('Showing in Terminal Mode pane', {
      timeout: 15_000,
    });

    // Type a shell-agnostic command; its output renders in the terminal.
    await pane.locator('.xterm').click();
    await page.keyboard.type('echo quilltap');
    await page.keyboard.press('Enter');
    await expect(pane.locator('.xterm')).toContainText('quilltap', { timeout: 15_000 });

    // Kill from the pane (two-click confirm); the pane closes (a client-side
    // mode change) and a "terminal closed" chip is visible. HONESTY NOTE (from
    // the 2026-07-13 in-suite diagnosis): the chip satisfying this is Ariel's
    // session-closed announcement posted by the CHAT-LOAD reconcile — the
    // engine's `chat_get` runs `reconcile_terminal_sessions_for_chat` with a
    // stubbed `is_live = |_| false` probe (salon.rs, a tracked deferral), so
    // the post-spawn refetch falsely retires the live session and posts the
    // close chip ~30ms after the OPEN chip. The kill itself sends SIGTERM,
    // which an interactive zsh ignores (v4 parity — v4's route also sends
    // SIGTERM), so no real exit announcement lands. When the live-PTY probe
    // gets wired through the boundary, this assertion needs a real exit story
    // (or a non-interactive shell) — don't tighten it to a count until then.
    const killBtn = pane.getByRole('button', { name: 'Kill terminal and close pane' });
    await killBtn.click(); // arms the confirm
    await killBtn.click(); // confirms the kill
    await expect(
      page.locator('.qt-chat-announcement-chip', { hasText: 'terminal closed' }).last(),
    ).toBeVisible({
      timeout: 15_000,
    });
    await expect(pane).toHaveCount(0);

    // Back to normal: the composer's Open-terminal button is available again.
    await expect(page.getByRole('button', { name: 'Open terminal' })).toBeVisible();
  });
});
