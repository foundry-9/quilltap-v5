import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this rides the SHARED global-setup server and only unlocks it if the
 * gate is showing, so its filename must sort AFTER foundation.spec.ts (which
 * walks the locked→unlock gate first; workers: 1, alphabetical).
 * "settings-taboo-flow" sorts after "foundation" ('se' > 'fo').
 *
 * P4.D50 — the Taboo card (Settings → Chat → Taboo), walked LIVE against the
 * real `tabooSettings` / `tabooSettingsUpdate` verbs. This is deliberately a
 * server round-trip rather than a UI-state check: the PUT echoes the list the
 * server NORMALIZED, so the deduped, order-preserved result the card renders is
 * evidence the whole chain agrees — card → dispatch → instance_settings → back.
 *
 * The walk adds two phrases, the second differing from the first only by case,
 * and asserts the duplicate never reaches the network (the client guard) — then
 * adds a genuinely new one, RELOADS, and asserts the persisted list survives in
 * the order it was placed. Finally it removes one and re-reads.
 *
 * DESTRUCTIVE-ish but self-cleaning: the beat empties the list at the end, and
 * the setting is instance-wide, so it leaves the shared server as it found it.
 */

/** Unlock only when the passphrase screen is showing (the shared server stays unlocked). */
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

/** Open the Chat tab with the Taboo card force-opened by its `?section=` anchor. */
async function openTabooCard(page: Page) {
  await page.goto('/salon');
  await maybeUnlock(page);
  await page.goto('/settings?tab=chat&section=taboo');
  const card = page.locator('#taboo');
  await expect(card).toBeVisible({ timeout: 15_000 });
  // Force-open means the body is mounted: its own input is on screen. Locate it
  // by ROLE — `getByLabel` is a substring match, and the surrounding form's
  // "Add a forbidden phrase" label would make it strict-mode ambiguous.
  await expect(phraseInput(card)).toBeVisible({ timeout: 15_000 });
  return card;
}

/** The card's phrase input (by role — see the note in `openTabooCard`). */
function phraseInput(card: ReturnType<Page['locator']>) {
  return card.getByRole('textbox', { name: 'Forbidden phrase', exact: true });
}

/** The badge labels currently rendered, in DOM order. */
function badgeLabels(card: ReturnType<Page['locator']>) {
  return card.locator('.qt-tag-badge > span:first-child');
}

/** Type a phrase and press Add, waiting for the dispatch it triggers. */
async function addPhrase(page: Page, card: ReturnType<Page['locator']>, phrase: string) {
  const saved = page.waitForResponse(
    (res) => res.url().includes('/api/dispatch') && res.status() === 200,
    { timeout: 30_000 },
  );
  await phraseInput(card).fill(phrase);
  await card.getByRole('button', { name: 'Add', exact: true }).click();
  await saved;
}

test.describe('P4.D50 — the Taboo card', () => {
  test('add → dedupe guard → reload → remove, all through the live verbs', async ({ page }) => {
    const card = await openTabooCard(page);

    // A fresh instance forbids nothing.
    await expect(card.getByText('Nothing forbidden yet')).toBeVisible();

    await addPhrase(page, card, 'weight-bearing');
    await expect(badgeLabels(card)).toHaveText(['weight-bearing']);
    await expect(card.getByText('1 phrase forbidden,', { exact: false })).toBeVisible();

    // A case-insensitive duplicate is refused by the card BEFORE the network.
    await phraseInput(card).fill('WEIGHT-BEARING');
    await card.getByRole('button', { name: 'Add', exact: true }).click();
    await expect(card.getByText('That phrase is already forbidden.')).toBeVisible();
    await expect(badgeLabels(card)).toHaveText(['weight-bearing']);

    // …and a genuinely new one lands, AFTER the first (order is preserved, not
    // sorted — 'that's not nothing' would sort first).
    await addPhrase(page, card, "  that's not nothing  ");
    await expect(badgeLabels(card)).toHaveText(['weight-bearing', "that's not nothing"]);
    await expect(card.getByText('2 phrases forbidden,', { exact: false })).toBeVisible();

    // The round-trip: reload and re-read from instance_settings. The trimmed
    // second entry proves the SERVER normalized it (the card sends the trimmed
    // draft, but the stored value is what comes back here).
    const reloaded = await openTabooCard(page);
    await expect(badgeLabels(reloaded)).toHaveText(['weight-bearing', "that's not nothing"]);

    // Remove the first; the remaining list is what the server echoes back.
    const removed = page.waitForResponse(
      (res) => res.url().includes('/api/dispatch') && res.status() === 200,
      { timeout: 30_000 },
    );
    await reloaded
      .getByRole('button', { name: 'Remove "weight-bearing" from the Taboo list' })
      .click();
    await removed;
    await expect(badgeLabels(reloaded)).toHaveText(["that's not nothing"]);

    // Leave the shared instance as we found it.
    const cleared = page.waitForResponse(
      (res) => res.url().includes('/api/dispatch') && res.status() === 200,
      { timeout: 30_000 },
    );
    await reloaded
      .getByRole('button', { name: 'Remove "that\'s not nothing" from the Taboo list' })
      .click();
    await cleared;
    await expect(reloaded.getByText('Nothing forbidden yet')).toBeVisible();
  });
});
