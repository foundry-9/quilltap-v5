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

    // Open Terminal Mode from the composer. With no live sessions it spawns
    // directly and enters split mode.
    await page.getByRole('button', { name: 'Open terminal' }).click();

    // The terminal pane appears with a live xterm surface.
    const pane = page.locator('qt-terminal-pane');
    await expect(pane).toBeVisible({ timeout: 15_000 });
    await expect(pane.locator('.xterm')).toBeVisible({ timeout: 15_000 });

    // Ariel's session-opened announcement landed as a chip (spawn + the refetch);
    // Staff announcements collapse to chips (kind "terminal opened").
    const openedChip = page.locator('.qt-chat-announcement-chip', { hasText: 'terminal opened' });
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

    // Kill from the pane (two-click confirm). The pane closes and Ariel's
    // session-closed announcement lands as a "terminal closed" chip.
    const killBtn = pane.getByRole('button', { name: 'Kill terminal and close pane' });
    await killBtn.click(); // arms the confirm
    await killBtn.click(); // confirms the kill
    await expect(
      page.locator('.qt-chat-announcement-chip', { hasText: 'terminal closed' }),
    ).toBeVisible({
      timeout: 15_000,
    });
    await expect(pane).toHaveCount(0);

    // Back to normal: the composer's Open-terminal button is available again.
    await expect(page.getByRole('button', { name: 'Open terminal' })).toBeVisible();
  });
});
