import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this rides the SHARED global-setup server and only unlocks it if the
 * gate is showing, so its filename must sort AFTER foundation.spec.ts (which
 * walks the locked→unlock gate first; workers: 1, alphabetical).
 * "settings-provider-options-flow" sorts after "foundation" ('se' > 'fo').
 *
 * P4.D84 — the connection-profile editor's provider-driven surfaces.
 *
 * Two beats run now, because both ride data the server already serves: the
 * bug-71 tool-use seed hint (driven by `capabilities.toolUse`, already false on
 * the OPENAI_COMPATIBLE manifest) and the `supportsImageUpload` re-seed (driven
 * by the client-side attachment table, no wire at all).
 *
 * The third — the schema-driven options panel round trip — needs the providers
 * listing to actually carry `optionsSchema`, which is P4.D83's half of this
 * round. It is gated by name below.
 *
 * SELF-CLEANING: the gated beat creates one profile and deletes it again, so
 * the shared server is left as it was found.
 */

/**
 * ACTIVATE-AT-UNIFY (lane P4.D83 — the provider-params/wire half of the
 * `93ed8abf` round).
 *
 * Until that lane lands, every provider manifest serves `optionsSchema: null`
 * and the panel correctly draws nothing, so the round-trip beat below would
 * fail on an empty modal. Flip this to `true` at unification, once the listing
 * carries the eight bundled schemas.
 *
 * A NAMED constant, deliberately, not a capability probe: a probe cannot tell a
 * listing that legitimately serves null (google, which declares no schema at
 * all) from one that has not learned to serve schemas yet, and would silently
 * activate the beat into guaranteed failure (the standing e2e rule).
 */
const P4D83_OPTIONS_SCHEMA_LANDED = false;

const PROFILE_NAME = 'P4.D84 options walk';

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

/** Open Settings → AI Providers with the Connection Profiles card on screen. */
async function openProfilesCard(page: Page): Promise<void> {
  await page.goto('/salon');
  await maybeUnlock(page);
  await page.goto('/settings?tab=providers&section=connection-profiles');
  await expect(page.getByRole('button', { name: '+ Add Profile' })).toBeVisible({
    timeout: 15_000,
  });
}

/** The modal's provider select, present only once the modal is open. */
function providerSelect(page: Page) {
  return page.locator('#qt-pf-provider');
}

/** The vision checkbox, located through its label as the specs do. */
function visionBox(page: Page) {
  return page
    .locator('label')
    .filter({ hasText: 'Supports image attachments (vision input)' })
    .locator('input[type=checkbox]');
}

function toolUseBox(page: Page) {
  return page
    .locator('label')
    .filter({ hasText: 'Allow tool use' })
    .locator('input[type=checkbox]');
}

test.describe('P4.D84 — the profile editor’s provider-driven surfaces', () => {
  test('the tool-use hint appears for a provider that advertises no tool support', async ({
    page,
  }) => {
    await openProfilesCard(page);
    await page.getByRole('button', { name: '+ Add Profile' }).click();
    await expect(providerSelect(page)).toBeVisible({ timeout: 15_000 });

    // OPENAI_COMPATIBLE declares `toolUse: false` — the conservative case for an
    // arbitrary endpoint. The box is SEEDED off and the hint explains why.
    await providerSelect(page).selectOption('OPENAI_COMPATIBLE');
    await expect(toolUseBox(page)).not.toBeChecked();
    await expect(
      page.getByText('This provider does not advertise tool support, so new profiles start with'),
    ).toBeVisible();
    await expect(page.getByText('you may turn it on regardless')).toBeVisible();

    // Seed, never clamp: the box is live and the hint does not chase it.
    await expect(toolUseBox(page)).toBeEnabled();
    await toolUseBox(page).check();
    await expect(toolUseBox(page)).toBeChecked();
    await expect(page.getByText('you may turn it on regardless')).toBeVisible();

    // A provider that DOES advertise tool support seeds it on and drops the hint.
    await providerSelect(page).selectOption('OLLAMA');
    await expect(toolUseBox(page)).toBeChecked();
    await expect(page.getByText('This provider does not advertise tool support')).toHaveCount(0);

    await page.getByRole('button', { name: 'Cancel' }).click();
    await expect(providerSelect(page)).toHaveCount(0);
  });

  test('switching provider re-seeds the vision checkbox on a new profile', async ({ page }) => {
    await openProfilesCard(page);
    await page.getByRole('button', { name: '+ Add Profile' }).click();
    await expect(providerSelect(page)).toBeVisible({ timeout: 15_000 });

    // Ollama takes no attachments; OpenAI takes images.
    await providerSelect(page).selectOption('OLLAMA');
    await expect(visionBox(page)).not.toBeChecked();
    await providerSelect(page).selectOption('OPENAI');
    await expect(visionBox(page)).toBeChecked();

    await page.getByRole('button', { name: 'Cancel' }).click();
    await expect(providerSelect(page)).toHaveCount(0);
  });

  test('the schema-driven options panel round-trips its values through a save', async ({
    page,
  }) => {
    test.skip(
      !P4D83_OPTIONS_SCHEMA_LANDED,
      'the providers listing serves optionsSchema: null until P4.D83 lands — flip P4D83_OPTIONS_SCHEMA_LANDED when it does',
    );

    await openProfilesCard(page);
    await page.getByRole('button', { name: '+ Add Profile' }).click();
    await expect(providerSelect(page)).toBeVisible({ timeout: 15_000 });

    await providerSelect(page).selectOption('OLLAMA');
    await page.locator('#qt-pf-name').fill(PROFILE_NAME);
    await page.locator('#qt-pf-baseurl').fill('http://localhost:11434');
    await page.locator('#qt-pf-model').fill('qwen3:8b');

    // Ollama's schema draws two groups; the rows below are its own, not v5's.
    await expect(page.getByText('Ollama Options')).toBeVisible();
    await expect(page.getByText('Sampling', { exact: true })).toBeVisible();

    // Thinking Effort is guarded by `showIf: enable_thinking === true`.
    await expect(page.locator('#pof-thinking_effort')).toHaveCount(0);
    await page.locator('#pof-enable_thinking').check();
    await expect(page.locator('#pof-thinking_effort')).toBeVisible();

    await page.locator('#pof-thinking_effort').selectOption('high');
    await page.locator('#pof-keep_alive').selectOption('30m');
    await page.locator('#pof-request_timeout_seconds').fill('900');

    await page.getByRole('button', { name: 'Create Profile' }).click();
    await expect(providerSelect(page)).toHaveCount(0);

    const card = page.locator('.qt-card').filter({ hasText: PROFILE_NAME });
    await expect(card).toBeVisible({ timeout: 15_000 });

    // Reopen: the values came back off the stored `parameters` bag, which is the
    // whole point — the panel reads and writes the same flat keys the provider
    // reads at call time.
    await card.getByRole('button', { name: 'Edit' }).click();
    await expect(providerSelect(page)).toBeVisible({ timeout: 15_000 });
    await expect(page.locator('#pof-enable_thinking')).toBeChecked();
    await expect(page.locator('#pof-thinking_effort')).toHaveValue('high');
    await expect(page.locator('#pof-keep_alive')).toHaveValue('30m');
    await expect(page.locator('#pof-request_timeout_seconds')).toHaveValue('900');
    await page.getByRole('button', { name: 'Cancel' }).click();
    await expect(providerSelect(page)).toHaveCount(0);

    // Self-cleaning: leave the shared server as it was found.
    await card.getByRole('button', { name: 'Delete' }).click();
    await card.getByRole('button', { name: 'Confirm' }).click();
    await expect(page.locator('.qt-card').filter({ hasText: PROFILE_NAME })).toHaveCount(0, {
      timeout: 15_000,
    });
  });
});
