import { expect, test } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * P4.6u: a LIVE browser walk of the Salon terminal pane over a REAL PTY through
 * the real WebSocket — unlock → open a fixture chat → open the terminal pane →
 * spawn (Ariel's session-opened announcement lands) → type `echo quilltap` → the
 * output renders in xterm → kill (the pane closes; the SIGTERM is v4 parity — an
 * interactive shell may ignore it, so the SESSION stays live) → re-open shows the
 * session picker over the surviving session → attach → type `exit` → the REAL
 * exit sequence posts Ariel's session-closed announcement and the pane cleans
 * itself up.
 *
 * The server side already exists (P4.2 terminal routes + P4.1c PTY manager); this
 * spec spawns a real shell PTY. Chat GET reconciles terminal sessions with the
 * REAL live-PTY probe (the P4.2-era stubbed `is_live = false` deferral is
 * closed), so a live session is spared — the walk guards that regression with a
 * closed-chip count baseline (the stub minted a spurious "terminal closed" chip
 * with the post-spawn refetch). The global teardown reaps the server process
 * group besides.
 */
test.describe('P4.6u — the Salon terminal pane (open → spawn → echo → kill → attach → exit)', () => {
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

  test('open the terminal pane, spawn a PTY, echo, kill, re-attach, and exit', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);

    // Open the solo fixture chat (the terminal flow needs no LLM participant).
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
    const soloCard = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Solo Voyage' });
    await soloCard.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible();

    // Earlier specs (salon-documents-flow) leave their own terminal chips in
    // this shared chat's history, so snapshot the pre-spawn chip counts FIRST:
    // the announcements ride the chip row, and "newest chip" is only
    // well-defined once the count has grown past this baseline. (Counting
    // `.last()` visible is NOT enough — a stale chip satisfies it instantly,
    // and the click then races the post-spawn refetch that delivers the new
    // chip; the expanded embed then binds the STALE spec's session and renders
    // a live surface instead of the "Showing in Terminal Mode pane" note.)
    const openedChips = page.locator('.qt-chat-announcement-chip', {
      hasText: 'terminal opened',
    });
    const closedChips = page.locator('.qt-chat-announcement-chip', {
      hasText: 'terminal closed',
    });
    // The list is VIRTUALIZED and mounts asynchronously — a too-early snapshot
    // reads 0, and the stale chips then mount TOGETHER with the post-spawn
    // refetch, overshooting `baseline + 1` (seen once the documents-flow walk
    // grew this chat's history). Settle each count until two reads a beat
    // apart agree (the 450ms drain window is the salon-scroll idiom).
    const settleCount = async (chips: typeof openedChips): Promise<number> => {
      let prev = -1;
      let cur = await chips.count();
      while (cur !== prev) {
        prev = cur;
        await page.waitForTimeout(450);
        cur = await chips.count();
      }
      return cur;
    };
    const openedBaseline = await settleCount(openedChips);
    const closedBaseline = await settleCount(closedChips);

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
    await expect(openedChips).toHaveCount(openedBaseline + 1, { timeout: 15_000 });
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

    // The reconcile regression guard: the post-spawn refetch ran chat GET's
    // terminal reconcile over the REAL live-PTY probe, which spared this live
    // session — no spurious "terminal closed" chip (the P4.2-era stubbed
    // `is_live = false` probe minted one ~30ms after every spawn).
    await expect(closedChips).toHaveCount(closedBaseline);

    // Kill from the pane (two-click confirm). The pane closes; the SIGTERM is
    // v4 parity — this interactive shell ignores it, so the session STAYS
    // live server-side and no closed chip lands.
    const killBtn = pane.getByRole('button', { name: 'Kill terminal and close pane' });
    await killBtn.click(); // arms the confirm
    await killBtn.click(); // confirms the kill
    await expect(pane).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Open terminal' })).toBeVisible();

    // Re-open: the surviving live session greets us with the session picker —
    // only possible over the real probe (the stub retired the row in the DB,
    // so the picker never saw a live session).
    await page.getByRole('button', { name: 'Open terminal' }).click();
    const picker = page.getByRole('dialog');
    await expect(picker).toBeVisible({ timeout: 15_000 });
    await picker.locator('button', { hasText: 'Started' }).first().click();
    await expect(pane).toBeVisible({ timeout: 15_000 });
    await expect(pane.locator('.xterm')).toBeVisible({ timeout: 15_000 });

    // End the shell for REAL: type `exit` → the exit sequence stamps the row
    // and posts Ariel's session-closed announcement; the expanded embed
    // dispatches `quilltap:terminal-exited` and the pane cleans itself up.
    await pane.locator('.xterm').click();
    await page.keyboard.type('exit');
    await page.keyboard.press('Enter');
    await expect(pane).toHaveCount(0, { timeout: 15_000 });

    // Reload for the closed chip (deterministic: the announcement write races
    // the exit-triggered refetch) — exactly ONE new "terminal closed" chip
    // landed, for the real exit. The reload lands back on the chat page (the
    // server stays unlocked).
    await page.reload();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
    await expect(closedChips).toHaveCount(closedBaseline + 1, { timeout: 15_000 });

    // Back to normal: the composer's Open-terminal button is available again.
    await expect(page.getByRole('button', { name: 'Open terminal' })).toBeVisible();
  });

  /**
   * P4.D34 (v4 `77ff4e2e`): an exited session stops accepting keystrokes.
   *
   * NEWLY LIVE, in both apps. v4 poked `term._input`, which is not an xterm
   * field in any version, so its guard always short-circuited and exited
   * sessions stayed typeable; v5 never had the poke at all. `textarea` is the
   * documented handle, and this is the first build that refuses the keys.
   *
   * The walk above cannot reach the assertion: the Salon pane tears the whole
   * terminal down on exit (`toHaveCount(0)`), so there is no surviving textarea
   * to inspect. The pop-out page keeps `<qt-terminal>` mounted past exit — it
   * only swaps its Kill button away — so the beat runs there, over its own PTY.
   */
  test('an exited session disables the terminal input (pop-out page)', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);

    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
    const soloCard = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Solo Voyage' });
    await expect(soloCard).toBeVisible({ timeout: 15_000 });
    const chatHref = await soloCard.getAttribute('href');
    const chatId = (chatHref ?? '').split('/').pop() as string;
    expect(chatId, 'a chat id from the salon list').toBeTruthy();

    // A real PTY over the live REST leg, so the deep link addresses a genuine
    // session (the same spawn the P4.d16 pop-out beat uses).
    const spawned = await page.request.post('/api/v1/terminals', {
      data: { chatId, label: 'Exit Disable', cols: 80, rows: 24 },
    });
    expect(spawned.ok(), 'spawn a real PTY').toBe(true);
    const session = (await spawned.json()) as { session?: { id: string }; id?: string };
    const sessionId = session.session?.id ?? session.id;
    expect(sessionId, 'the spawned session id').toBeTruthy();

    await page.goto(`/salon/${chatId}/terminal/${sessionId}`);
    const xterm = page.locator('.xterm');
    await expect(xterm).toBeVisible({ timeout: 15_000 });

    // Live: the textarea takes input, and the Kill button is offered.
    const textarea = page.locator('.xterm-helper-textarea');
    await expect(textarea).not.toBeDisabled();
    await expect(page.getByRole('button', { name: 'Kill Session' })).toBeVisible({
      timeout: 15_000,
    });

    // End the shell for real.
    await xterm.click();
    await page.keyboard.type('exit');
    await page.keyboard.press('Enter');

    // The exit lands: the Kill button goes (the popout's `state() !== 'exited'`
    // gate), the closing line is written, and input is refused.
    await expect(page.getByRole('button', { name: 'Kill Session' })).toHaveCount(0, {
      timeout: 15_000,
    });
    await expect(xterm).toContainText('session ended', { timeout: 15_000 });
    await expect(textarea).toBeDisabled({ timeout: 15_000 });
  });
});
