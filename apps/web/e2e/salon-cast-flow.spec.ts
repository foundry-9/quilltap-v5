import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';
import { openSidebarSection } from './support/sidebar';

/**
 * P4.9E1B — the in-chat cast: the sidebar's Add Character footer and the picker
 * it opens, the participant card's Remove, the composer gutter's RNG tool, and
 * the Chat drawer's avatar-generation switch and Tools… refusal.
 *
 * ORDERING: rides the shared global-setup server, so the filename must sort
 * after `foundation.spec.ts` ('sa' > 'fo'), which walks the locked→unlock gate.
 *
 * **What runs live in-lane, and what does not.** The Tools… refusal is live
 * today — it is a client-side refusal that names the unported inventory route
 * and needs no verb at all. Everything else rides P4.9E1A's cast verbs or
 * P4.9E3A's `chatRng`, neither of which exists on main while this lane runs, so
 * those beats are **ACTIVATE-AT-UNIFY**: each probes the dispatch surface once
 * and annotates-and-returns when the verb is unknown, exactly as
 * `salon-post-office-flow.spec.ts` does. When the server lanes land they
 * self-activate with no edit.
 *
 * **The cast walk restores what it changes, deliberately.** It adds a character
 * to "Group Expedition" and then removes it again, so the chat ends with the
 * three active participants it started with. That matters beyond tidiness:
 * `salon-post-office-flow` asserts v4's whisper gate on the SAME shared server —
 * Group Expedition qualifies at three or more active, Solo Voyage does not at
 * two — so a beat that left a fourth participant behind (or that added one to
 * Solo Voyage) would break a sibling spec through the fixture rather than
 * through the code. See the `e2e-playwright-traps` note.
 */

/** Is a §1 cast verb on the server yet? (One dispatch, nothing written.) */
async function castVerbsLive(page: Page): Promise<boolean> {
  const resp = await page.request.post('/api/dispatch', {
    // A deliberately unsatisfiable remove: a LANDED verb answers a real error
    // envelope (no such chat), an unlanded one fails to deserialize at the union.
    data: { type: 'chatRemoveParticipant', chatId: 'probe', participantId: '' },
    failOnStatusCode: false,
  });
  const body = await resp.text();
  return !/unknown variant|did not match any variant/i.test(body);
}

/** Is P4.9E3A's `chatRng` on the server yet? */
async function rngVerbLive(page: Page): Promise<boolean> {
  const resp = await page.request.post('/api/dispatch', {
    data: { type: 'chatRng', chatId: 'probe', kind: 20, rolls: 1, preview: true },
    failOnStatusCode: false,
  });
  const body = await resp.text();
  return !/unknown variant|did not match any variant/i.test(body);
}

async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const chats = page.getByRole('heading', { name: 'Chats', exact: true });
  await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

async function openChat(page: Page, title: string): Promise<void> {
  await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
  const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: title });
  await expect(card).toBeVisible();
  await card.click();
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible();
}

test.describe('P4.9E1B — the in-chat cast', () => {
  test('add a character to the cast, then remove it again', async ({ page }, testInfo) => {
    // The walk crosses two mutation round-trips and two chat refetches.
    test.setTimeout(60_000);
    await page.goto('/salon');
    await maybeUnlock(page);
    if (!(await castVerbsLive(page))) {
      testInfo.annotations.push({
        type: 'activate-at-unification',
        description:
          'Adding and removing a participant needs P4.9E1A’s chatAddParticipant / chatRemoveParticipant verbs on main.',
      });
      return;
    }

    await openChat(page, 'Group Expedition');
    await openSidebarSection(page, 'Participants');

    const castNames = page.locator('qt-chat-sidebar .qt-participant-card-name');
    const before = await castNames.count();

    await page.getByRole('button', { name: 'Add Character', exact: true }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog.getByText('Add Character to Chat')).toBeVisible();

    // The picker excludes whoever is already in the room. If the fixture roster
    // has nobody left to add, there is nothing to walk — say so rather than fail.
    // (`hasNot` matches DESCENDANTS, so it cannot exclude the two dashed
    // buttons — they carry `.border-dashed` themselves. Filter on their text.)
    const tiles = dialog
      .locator('.grid button')
      .filter({ hasNotText: 'Create New NPC' })
      .filter({ hasNotText: 'Summon from Lore' });
    const firstTile = tiles.first();
    if ((await tiles.count()) === 0) {
      testInfo.annotations.push({
        type: 'fixture',
        description: 'Every fixture character is already in Group Expedition; nothing to add.',
      });
      await page.keyboard.press('Escape');
      return;
    }

    const joinerName = ((await firstTile.locator('.font-semibold').first().textContent()) ?? '').trim();
    await firstTile.click();

    // Selecting a character reveals the Controlled-By select, seeded with a
    // profile — the footer button unlocks only once both are chosen.
    await expect(dialog.locator('#qt-add-character-profile')).toBeVisible();
    const add = dialog.getByRole('button', { name: 'Add Character', exact: true });
    await expect(add).toBeEnabled();
    await add.click();

    // The dialog closes, the Salon says who joined — through the toast v4 raises
    // from inside the dialog (P4.25) — and the cast grows by one.
    await expect(page.getByRole('dialog')).toHaveCount(0, { timeout: 15_000 });
    await expect(
      page.locator('[role="toast-container"]').getByText(`${joinerName} has joined the chat`),
    ).toBeVisible({ timeout: 15_000 });
    await expect(castNames).toHaveCount(before + 1, { timeout: 15_000 });
    await expect(castNames.filter({ hasText: joinerName })).toHaveCount(1);

    // …and back out again: Remove asks first, in v4's words.
    await page.getByRole('button', { name: `Remove ${joinerName} from chat` }).click();
    const confirm = page.getByRole('dialog');
    await expect(confirm).toContainText('will no longer participate in the conversation');
    await confirm.getByRole('button', { name: 'Remove', exact: true }).click();

    await expect(page.getByRole('dialog')).toHaveCount(0, { timeout: 15_000 });
    await expect(
      page
        .locator('[role="toast-container"]')
        .getByText(`${joinerName} has been removed from the chat`),
    ).toBeVisible({ timeout: 15_000 });

    // NOTE: there is deliberately no card-count assertion here. v4's remove is a
    // SOFT remove (`status: 'removed'`, `isActive: false`,
    // `db/chats_participants.rs:207`), and NEITHER app filters removed
    // participants out of the cast list — v4's `useParticipants.participantData`
    // maps every `chat.participants` row, as v5's `sortedParticipants` does. So
    // the card stays; asserting it vanished would be asserting behaviour neither
    // app has. What the removal restores is the ACTIVE count, which is what v4's
    // whisper gate — and the sibling spec that walks it — actually reads.

    // Collapse again so sibling specs find the strip they expect.
    await page.getByRole('button', { name: 'Collapse chat sidebar' }).click();
  });

  test('roll dice from the composer gutter — the result waits as a chip', async ({
    page,
  }, testInfo) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    if (!(await rngVerbLive(page))) {
      testInfo.annotations.push({
        type: 'activate-at-unification',
        description: 'Rolling from the gutter needs P4.9E3A’s chatRng verb on main.',
      });
      return;
    }

    await openChat(page, 'Solo Voyage');

    await page.getByRole('button', { name: 'Random number generator' }).click();
    const menu = page.getByRole('menu');
    await expect(menu).toBeVisible();
    await menu.getByRole('menuitem', { name: /^Roll 1d20/ }).click();

    // Preview mode: a chip, not a message. The transcript is untouched until a
    // send carries it.
    const chip = page.locator('.qt-chat-tool-result-chip');
    await expect(chip).toHaveCount(1, { timeout: 15_000 });
    await expect(chip).toContainText('d20');
    await expect(page.getByRole('menu')).toHaveCount(0);

    // Discarding it leaves the conversation exactly as it was — which is also
    // what keeps this beat from writing to the shared server.
    await page.getByRole('button', { name: 'Remove tool result' }).click();
    await expect(page.locator('.qt-chat-tool-result-chip')).toHaveCount(0);
  });

  test('the avatar-generation switch flips, and Tools… opens the tool tree', async ({
    page,
  }, testInfo) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Solo Voyage');
    await openSidebarSection(page, 'Chat');

    // Tools… REFUSED BY NAME until P4.9E3C brought `ChatToolSettingsModal`
    // across; the entry now opens the tool tree. The dialog's own behaviour is
    // pinned by `salon-dialogs-flow.spec.ts` (gated on the inventory verb) — all
    // this beat still owes is that the refusal is gone.
    await page.getByRole('button', { name: 'Tools…' }).click();
    await expect(page.getByRole('dialog')).toBeVisible({ timeout: 15_000 });
    await expect(page.locator('qt-chat-sidebar')).not.toContainText('/api/v1/tools');
    await page.getByRole('button', { name: 'Cancel' }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0, { timeout: 15_000 });

    if (!(await castVerbsLive(page))) {
      testInfo.annotations.push({
        type: 'activate-at-unification',
        description:
          'Flipping avatar generation needs P4.9E1A’s chatToggleAvatarGeneration verb on main.',
      });
      await page.getByRole('button', { name: 'Collapse chat sidebar' }).click();
      return;
    }

    const box = page.getByLabel('Auto-generate character avatars');
    const wasOn = await box.isChecked();
    await box.click();
    await expect(
      page
        .locator('[role="toast-container"]')
        .getByText(wasOn ? 'Avatar generation disabled' : 'Avatar generation enabled'),
    ).toBeVisible({ timeout: 15_000 });
    await expect(box).toBeChecked({ checked: !wasOn, timeout: 15_000 });

    // Put it back, so the shared server ends as it started.
    await box.click();
    await expect(box).toBeChecked({ checked: wasOn, timeout: 15_000 });

    await page.getByRole('button', { name: 'Collapse chat sidebar' }).click();
  });
});
