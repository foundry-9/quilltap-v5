import { expect, request as pwRequest, test, type Page } from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';
import { openSidebarSection } from './support/sidebar';

/**
 * P4.6ba — the Pascal in-chat surface (the composer custom-tools dialog + run
 * flow + the All-Whispers toggle). **P4.d21** turned the popup into v4's
 * two-phase modal and gave the roll announcement the outcome's own state, so the
 * gestures below drive a `role="dialog"` and the run assertion now checks the
 * accent and its visually-hidden state name.
 *
 * The custom-tools FLOW is ACTIVATE-AT-UNIFY: the dialog's button appears only when
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
/**
 * ACTIVATE-AT-UNIFY (P4.D36 unit 8): does the server render a definition's
 * `chipLabel` into `pascalMeta`? A NAMED CONSTANT, per the round's shared
 * contract. **Unifier: flip to `true` once P4.D35's server half is merged.**
 */
const CHIP_LABEL_SERVER_LANDED = true;

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

  test('open the dialog, go back, run a tool, and see the roll wear its own outcome', async ({
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
          'The custom-tools dialog activates when lane P4.6ay’s /custom-tools server surface and a Tools/-bearing fixture are on main.',
      });
      // The composer still mounted — the chat opened cleanly.
      await expect(page.locator('.qt-chat-composer')).toBeVisible();
      return;
    }

    // --- The activated flow (lane AY live + Tools fixture) ---
    // P4.d21: the wand opens a two-phase MODAL, not a 288px popover.
    await toolsButton.click();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();
    await expect(dialog.getByText("Pascal's Table")).toBeVisible();

    // Phase one lists the roster — title, description, and the store/path line;
    // never its odds or its outcome table. (The search box's six-tool threshold
    // is pinned by the component spec, not here: the roster's size depends on
    // what every tier contributes to this instance.)
    const coin = dialog.getByRole('button').filter({ hasText: 'Flip A Coin' }).first();
    await expect(coin).toBeVisible();
    await expect(coin).toContainText('Flip a coin.');

    // Phase two, then back again: the picker returns and the title with it.
    await coin.click();
    await expect(dialog.getByRole('button', { name: 'Run Flip A Coin' })).toBeVisible();
    await dialog.getByRole('button', { name: 'Choose another tool' }).click();
    await expect(dialog.getByText("Pascal's Table")).toBeVisible();

    // Choose it again and run — Pascal's outcome posts and the dialog closes.
    await dialog.getByRole('button').filter({ hasText: 'Flip A Coin' }).first().click();
    await dialog.getByRole('button', { name: 'Run Flip A Coin' }).click();
    await expect(dialog).toBeHidden();

    // The roll renders as its own full row with the header bar: "Pascal" + the
    // tool title. It is operator machinery, so it stays visible even with All
    // Whispers off (the operator-facing carve-out).
    const bar = page.locator('.qt-chat-system-bar', { hasText: 'Flip A Coin' }).last();
    await expect(bar).toBeVisible({ timeout: 15_000 });
    await expect(bar).toContainText('Pascal');

    // P4.d21 / v4 231be14c: the bar wears the OUTCOME's state, not the generic
    // importance dot. `coin`'s only outcome is a success (its roll is min===max,
    // so this is deterministic), and the state is named for screen readers too.
    // Two separate matches: Angular merges the static and bound class lists, so
    // the two accent classes are not guaranteed to end up adjacent.
    await expect(bar).toHaveClass(/qt-pascal-result\b/);
    await expect(bar).toHaveClass(/qt-pascal-result--success/);
    await expect(bar.locator('.qt-chat-announcement-dot')).toHaveClass(
      /qt-chat-announcement-dot-outcome-success/,
    );
    await expect(bar.locator('.sr-only')).toHaveText('success');

    await openSidebarSection(page, 'Visibility');
    const whispers = page.getByRole('switch', { name: 'All Whispers' });
    await expect(whispers).toHaveAttribute('aria-checked', 'false');
    await expect(bar).toBeVisible(); // still shown with the toggle off
  });

  /**
   * P4.D36 unit 8 — a labelled run's chip carries the definition's RENDERED
   * chipLabel, not the tool's title.
   *
   * ACTIVATE-AT-UNIFY behind {@link CHIP_LABEL_SERVER_LANDED}: rendering the
   * label into `pascalMeta.chipLabel` is P4.D35's server half. A NAMED CONSTANT
   * rather than a probe, per the round's shared contract — and not probe-able
   * anyway, since a server without the feature TOLERATES the unknown
   * `chipLabel` key by design and runs the tool happily, chip unlabelled.
   * **Unifier: flip this to `true` once P4.D35 is merged.**
   *
   * The beat authors its own labelled definition through the mount-file verb
   * rather than leaning on whatever P4.D35's rebuilt fixture happens to carry:
   * the claim under test is the label's journey from the file to the chip, and
   * a beat that depended on a sibling's fixture CONTENTS would be asserting
   * something it does not control.
   */
  test("a labelled run's chip shows the rendered label, not the title", async ({ page }) => {
    test.skip(
      !CHIP_LABEL_SERVER_LANDED,
      'ACTIVATE-AT-UNIFY (P4.D36 unit 8): the server does not render pascalMeta.chipLabel yet — lane P4.D35 owns it',
    );

    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    const chatId = new URL(page.url()).pathname.split('/').pop()!;
    const ctx = await pwRequest.newContext();

    // Where the seeded roster lives — read off an existing entry rather than
    // transcribed, exactly as the seed reads the fixture's sidecar.
    const rosterRes = await ctx.post(`${BASE_URL}/api/dispatch`, {
      data: { type: 'chatCustomToolsList', chatId },
    });
    const roster = (await rosterRes.json()) as {
      data?: { tools?: Array<{ mountPointId: string }> };
    };
    const mountPointId = roster.data?.tools?.[0]?.mountPointId;
    expect(mountPointId, 'the seeded Tools roster should resolve').toBeTruthy();

    // A fixed roll (min === max), so the rendered label is deterministic.
    const definition = {
      $schema: '/schemas/qtap-custom-tool.schema.json',
      name: 'e2e_labelled',
      title: 'Labelled Contrivance',
      chipLabel: 'Labelled — {{value}}',
      description: 'A labelled contrivance for the e2e walk.',
      roll: { min: 7, max: 7 },
      outcomes: [{ when: true, message: 'The label reads {{value}}.', state: 'info' }],
    };
    const write = await ctx.post(`${BASE_URL}/api/dispatch`, {
      data: {
        type: 'mountFileWrite',
        mountPointId,
        path: 'Tools/e2e_labelled.tool.json',
        content: `${JSON.stringify(definition, null, 2)}\n`,
      },
    });
    expect(write.ok()).toBe(true);

    const run = await ctx.post(`${BASE_URL}/api/dispatch`, {
      data: { type: 'chatCustomToolRun', chatId, tool: 'e2e_labelled' },
    });
    expect(run.ok()).toBe(true);
    await ctx.dispose();

    // The chip names the RUN, not the tool: "Labelled — 7", never "Labelled
    // Contrivance".
    //
    // No `maybeUnlock` here: this reload lands back on the CHAT, where neither
    // the passphrase field nor the salon list's "Chats" heading exists, so the
    // helper's `expect(passphrase.or(chats))` can only time out. The session
    // was already unlocked on the way in, and the chip assertion below carries
    // its own wait for the reloaded transcript.
    await page.reload();
    const bar = page.locator('.qt-chat-system-bar', { hasText: 'Labelled — 7' }).last();
    await expect(bar).toBeVisible({ timeout: 15_000 });
    await expect(bar).toContainText('Pascal');
    await expect(bar).not.toContainText('Labelled Contrivance');
  });
});
