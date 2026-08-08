import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this rides the SHARED global-setup server and only unlocks it if the
 * gate is showing, so its filename must sort AFTER foundation.spec.ts (which
 * walks the locked→unlock gate first; workers: 1, alphabetical).
 * "settings-brahma-console-flow" sorts after "foundation" ('se' > 'fo').
 *
 * P4.D59 — the Brahma Console budget card (Settings → Chat → Brahma Console),
 * walked LIVE against the real `brahmaConsoleSettings` / `brahmaConsoleSettingsUpdate`
 * verbs (P4.D57's Shared contract). Deliberately a server round-trip rather than
 * a UI-state check: the card commits on blur, and a RELOAD re-reads
 * `instance_settings['brahmaConsole']`, so the persisted value proves the whole
 * chain agrees — card → dispatch → instance_settings → back.
 *
 * DESTRUCTIVE-ish but self-cleaning: the beat restores the budget to the default
 * (50) at the end, and the setting is instance-wide, so it leaves the shared
 * server as it found it.
 *
 * ACTIVE since the P4.D57∥D58∥D59 unification: the beat was authored gated
 * behind the named constant below (P4.D59 was the SPA-only lane; the
 * `brahmaConsole*` verbs landed in the sibling lane P4.D57), and the unifier
 * flipped it once both lanes were on one branch. Gating by a NAMED constant —
 * not a capability probe — is the round rule: a probe would silently activate
 * the beat into a guaranteed failure the moment the verb is DEFINED but still
 * refusing.
 */
const BRAHMA_CONSOLE_SERVER_LANDED = true;

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

/** Open the Chat tab with the Brahma Console card force-opened by its `?section=` anchor. */
async function openBrahmaCard(page: Page) {
  await page.goto('/salon');
  await maybeUnlock(page);
  await page.goto('/settings?tab=chat&section=brahma-console');
  const card = page.locator('#brahma-console');
  await expect(card).toBeVisible({ timeout: 15_000 });
  // Force-open means the body is mounted: the number input is on screen.
  await expect(turnsInput(page)).toBeVisible({ timeout: 15_000 });
  return card;
}

/** The card's number input (unique id `#brahma-max-turns`). */
function turnsInput(page: Page) {
  return page.locator('#brahma-max-turns');
}

/** Set the budget to `value`, waiting for the commit dispatch the blur triggers. */
async function setTurns(page: Page, value: string) {
  const saved = page.waitForResponse(
    (res) => res.url().includes('/api/dispatch') && res.status() === 200,
    { timeout: 30_000 },
  );
  await turnsInput(page).fill(value);
  await turnsInput(page).blur();
  await saved;
}

const describeOrSkip = BRAHMA_CONSOLE_SERVER_LANDED ? test.describe : test.describe.skip;

describeOrSkip('P4.D59 — the Brahma Console budget card', () => {
  test('read default → edit in range → reload → persisted, through the live verbs', async ({
    page,
  }) => {
    await openBrahmaCard(page);

    // A fresh instance carries no `brahmaConsole` setting → the default budget.
    await expect(turnsInput(page)).toHaveValue('50');

    // Edit within [5, 200] and let it commit on blur.
    await setTurns(page, '75');
    await expect(turnsInput(page)).toHaveValue('75');

    // The round-trip: reload and re-read from instance_settings.
    await openBrahmaCard(page);
    await expect(turnsInput(page)).toHaveValue('75');

    // Leave the shared instance as we found it.
    await setTurns(page, '50');
    await openBrahmaCard(page);
    await expect(turnsInput(page)).toHaveValue('50');
  });
});
