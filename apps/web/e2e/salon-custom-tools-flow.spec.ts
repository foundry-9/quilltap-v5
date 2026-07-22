import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';
import { openSidebarSection } from './support/sidebar';

/**
 * P4.6ba — the Pascal in-chat surface (the composer custom-tools popup + run
 * flow + the All-Whispers toggle).
 *
 * The custom-tools FLOW is ACTIVATE-AT-UNIFY: the popup button appears only when
 * lane P4.6ay's `/custom-tools` server surface is on main AND the fixture carries
 * a Tools/-bearing mount tree (lane AY's route-fixture family is the natural
 * instance to reuse). Until both land, the roster resolves empty and the button
 * gates itself hidden — so that beat annotates and returns rather than forcing a
 * flow it cannot yet drive (the `m4b-salon.spec.ts` guard idiom). The Pascal
 * bubble + header chip and the run flow stay covered by the component specs
 * (`message-row.spec.ts`, `custom-tools-popup.spec.ts`) meanwhile.
 *
 * The All-Whispers toggle is CLIENT-SIDE and runs today: the header switch flips
 * its state; asserting a whispered Pascal result stays visible under toggle-off
 * (the operator-facing carve-out) needs a Pascal whisper in the fixture, so that
 * assertion rides the activated flow.
 */
test.describe('Salon custom tools + whispers (P4.6ba)', () => {
  async function maybeUnlock(page: Page) {
    const passphrase = page.locator('#qt-passphrase');
    const chats = page.getByRole('heading', { name: 'Chats', exact: true });
    await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
  }

  async function openChat(page: Page, title: string) {
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
    const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: title });
    await expect(card).toBeVisible();
    await card.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible();
  }

  test('the All Whispers toggle flips its state (client-side)', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    // The toggle lives in the sidebar's Visibility drawer since P4.9H1 (v4's
    // home for it), under v4's own "All Whispers" label.
    await openSidebarSection(page, 'Visibility');
    const toggle = page.getByRole('switch', { name: 'All Whispers' });
    await expect(toggle).toBeVisible();
    await expect(toggle).toHaveAttribute('aria-checked', 'false');
    await toggle.click();
    await expect(toggle).toHaveAttribute('aria-checked', 'true');
    // Leave it off for sibling specs.
    await toggle.click();
    await expect(toggle).toHaveAttribute('aria-checked', 'false');
  });

  test('open the popup, run a tool, and see the Pascal bubble with its header chip', async ({
    page,
  }, testInfo) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    const toolsButton = page.getByRole('button', { name: 'Custom tools' });
    if ((await toolsButton.count()) === 0) {
      testInfo.annotations.push({
        type: 'activate-at-unification',
        description:
          'The custom-tools popup activates when lane P4.6ay’s /custom-tools server surface and a Tools/-bearing fixture are on main.',
      });
      // The composer still mounted — the chat opened cleanly.
      await expect(page.locator('.qt-chat-composer')).toBeVisible();
      return;
    }

    // --- The activated flow (lane AY live + Tools fixture) ---
    await toolsButton.click();
    const menu = page.getByRole('menu');
    await expect(menu).toBeVisible();

    // The roster lists a runnable tool (title only — never its odds/outcome table).
    const firstTool = menu.getByRole('menuitem').first();
    await expect(firstTool).toBeVisible();
    const toolTitle = (await firstTool.innerText()).split('\n')[0].trim();
    await firstTool.click();

    // Run it — Pascal's outcome posts and the popup closes.
    await page.getByRole('button', { name: /^Run / }).click();
    await expect(menu).toBeHidden();

    // The Pascal roll outcome renders as its own full row with the header bar:
    // "Pascal" + the tool title. It is operator machinery, so it stays visible
    // even with All Whispers off (the operator-facing carve-out).
    const bar = page.locator('.qt-chat-system-bar', { hasText: 'Pascal' });
    await expect(bar.first()).toBeVisible({ timeout: 15_000 });
    // ignoreCase: the menu title is read via innerText (CSS-rendered, which can
    // uppercase it) while the bar carries DOM text — the assertion is "the bar
    // names the tool that ran", not its letter case.
    await expect(bar.first()).toContainText(toolTitle, { ignoreCase: true });

    await openSidebarSection(page, 'Visibility');
    const whispers = page.getByRole('switch', { name: 'All Whispers' });
    await expect(whispers).toHaveAttribute('aria-checked', 'false');
    await expect(bar.first()).toBeVisible(); // still shown with the toggle off
  });
});
