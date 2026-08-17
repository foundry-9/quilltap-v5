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
const P4D83_OPTIONS_SCHEMA_LANDED = true;

/**
 * ACTIVATE-AT-UNIFY (lane P4.D85 — the server half of the `d123658d` round).
 *
 * The three tag verbs (`connectionProfileGetTags` / `AddTag` / `RemoveTag`)
 * land with that lane; until then the editor's first read is refused and the
 * beat below would fail on an empty chip row. Flip this to `true` at
 * unification.
 *
 * A NAMED constant, deliberately, not a capability probe: a probe cannot tell
 * "the verb is not implemented" from "this profile genuinely has no tags", and
 * would silently activate the beat into guaranteed failure (the standing e2e
 * rule).
 */
const P4D85_PROFILE_TAGS_LANDED = true;

const PROFILE_NAME = 'P4.D84 options walk';
const BASE_URL_PROFILE = 'P4.D86 base-url walk';
const TAG_PROFILE = 'P4.D86 tag walk';
const NUMBER_PROFILE = 'P4.D86 number walk';
const TAG_NAME = 'p4d86-fast-and-cheap';

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

/**
 * P4.D86 — v4's own verification walk for `d123658d`, beat for beat:
 * "connect succeeds on OpenAI after visiting Ollama and the saved row holds
 * NULL; a cleared timeout stays cleared, stores 5 rather than 3005, and
 * round-trips as absent; a tag added through the modal persists, survives a
 * reopen, and shows its name on the card."
 *
 * SELF-CLEANING: each beat deletes the profile it created.
 */
test.describe('P4.D86 — the profile editor’s three fixed seams', () => {
  test('a base URL picked up from Ollama does not follow the profile to a hosted provider', async ({
    page,
  }) => {
    await openProfilesCard(page);
    await page.getByRole('button', { name: '+ Add Profile' }).click();
    await expect(providerSelect(page)).toBeVisible({ timeout: 15_000 });

    // Ollama auto-fills its default; the row is saved WITH it, so the beat has
    // a genuinely poisoned row to heal rather than an empty one.
    await providerSelect(page).selectOption('OLLAMA');
    await expect(page.locator('#qt-pf-baseurl')).toHaveValue('http://localhost:11434');
    await page.locator('#qt-pf-name').fill(BASE_URL_PROFILE);
    await page.locator('#qt-pf-model').fill('qwen3:8b');
    await page.getByRole('button', { name: 'Create Profile' }).click();
    await expect(providerSelect(page)).toHaveCount(0);

    const card = page.locator('.qt-card').filter({ hasText: BASE_URL_PROFILE });
    await expect(card).toBeVisible({ timeout: 15_000 });
    await expect(card.getByText('Base URL: http://localhost:11434')).toBeVisible();

    // Move it to a hosted provider: the field goes away, and the save must send
    // an EMPTY baseUrl so the stored row is cleared rather than left broken and
    // invisible. Read back off the card, which renders the stored column.
    await card.getByRole('button', { name: 'Edit' }).click();
    await expect(providerSelect(page)).toBeVisible({ timeout: 15_000 });
    await providerSelect(page).selectOption('OPENAI');
    await expect(page.locator('#qt-pf-baseurl')).toHaveCount(0);
    await page.getByRole('button', { name: 'Update Profile' }).click();
    await expect(providerSelect(page)).toHaveCount(0);

    const healed = page.locator('.qt-card').filter({ hasText: BASE_URL_PROFILE });
    await expect(healed).toBeVisible({ timeout: 15_000 });
    await expect(healed.getByText('Base URL:')).toHaveCount(0);

    await healed.getByRole('button', { name: 'Delete' }).click();
    await healed.getByRole('button', { name: 'Confirm' }).click();
    await expect(page.locator('.qt-card').filter({ hasText: BASE_URL_PROFILE })).toHaveCount(0, {
      timeout: 15_000,
    });
  });

  test('a cleared numeric option stores what is typed next, and round-trips absent', async ({
    page,
  }) => {
    test.skip(
      !P4D83_OPTIONS_SCHEMA_LANDED,
      'the providers listing serves optionsSchema: null until P4.D83 lands',
    );

    await openProfilesCard(page);
    await page.getByRole('button', { name: '+ Add Profile' }).click();
    await expect(providerSelect(page)).toBeVisible({ timeout: 15_000 });

    await providerSelect(page).selectOption('OLLAMA');
    await page.locator('#qt-pf-name').fill(NUMBER_PROFILE);
    await page.locator('#qt-pf-baseurl').fill('http://localhost:11434');
    await page.locator('#qt-pf-model').fill('qwen3:8b');

    // Unset: blank box, default showing through as the placeholder. "Absent"
    // and "explicitly the default" must not look identical (Bug 72).
    const timeout = page.locator('#pof-request_timeout_seconds');
    await expect(timeout).toHaveValue('');
    await expect(timeout).toHaveAttribute('placeholder', '300');

    await timeout.fill('300');
    await timeout.fill('');
    await timeout.pressSequentially('5');
    // The bug's signature was 3005 — the default repainted with the caret after it.
    await expect(timeout).toHaveValue('5');

    await page.getByRole('button', { name: 'Create Profile' }).click();
    await expect(providerSelect(page)).toHaveCount(0);
    const card = page.locator('.qt-card').filter({ hasText: NUMBER_PROFILE });
    await expect(card).toBeVisible({ timeout: 15_000 });

    await card.getByRole('button', { name: 'Edit' }).click();
    await expect(page.locator('#pof-request_timeout_seconds')).toHaveValue('5');

    // Clear it and save: the key leaves the bag, so the reopen shows blank
    // rather than resurrecting the schema default as a stored-looking value.
    await page.locator('#pof-request_timeout_seconds').fill('');
    await page.getByRole('button', { name: 'Update Profile' }).click();
    await expect(providerSelect(page)).toHaveCount(0);
    await expect(card).toBeVisible({ timeout: 15_000 });
    await card.getByRole('button', { name: 'Edit' }).click();
    await expect(page.locator('#pof-request_timeout_seconds')).toHaveValue('');
    await expect(page.locator('#pof-request_timeout_seconds')).toHaveAttribute(
      'placeholder',
      '300',
    );
    await page.getByRole('button', { name: 'Cancel' }).click();
    await expect(providerSelect(page)).toHaveCount(0);

    await card.getByRole('button', { name: 'Delete' }).click();
    await card.getByRole('button', { name: 'Confirm' }).click();
    await expect(page.locator('.qt-card').filter({ hasText: NUMBER_PROFILE })).toHaveCount(0, {
      timeout: 15_000,
    });
  });

  test('a tag added in the modal persists, survives a reopen, and names itself on the card', async ({
    page,
  }) => {
    test.skip(
      !P4D85_PROFILE_TAGS_LANDED,
      'the three connection-profile tag verbs land with P4.D85 — flip P4D85_PROFILE_TAGS_LANDED when they do',
    );

    await openProfilesCard(page);
    await page.getByRole('button', { name: '+ Add Profile' }).click();
    await expect(providerSelect(page)).toBeVisible({ timeout: 15_000 });
    await providerSelect(page).selectOption('OPENAI');
    await page.locator('#qt-pf-name').fill(TAG_PROFILE);
    await page.locator('#qt-pf-model').fill('gpt-4');
    await page.getByRole('button', { name: 'Create Profile' }).click();
    await expect(providerSelect(page)).toHaveCount(0);

    const card = page.locator('.qt-card').filter({ hasText: TAG_PROFILE });
    await expect(card).toBeVisible({ timeout: 15_000 });

    // The editor only exists when EDITING — v4 renders it on `profile?.id`.
    await card.getByRole('button', { name: 'Edit' }).click();
    await expect(providerSelect(page)).toBeVisible({ timeout: 15_000 });
    await page.getByRole('button', { name: '+ Add Tag' }).click();
    await page.getByPlaceholder('Add a tag...').fill(TAG_NAME);
    await page.getByPlaceholder('Add a tag...').press('Enter');

    // Immediate persistence: the chip is there before any Save.
    const chip = page.locator('.qt-tag-badge').filter({ hasText: TAG_NAME });
    await expect(chip).toHaveCount(1, { timeout: 15_000 });
    await page.getByRole('button', { name: 'Cancel' }).click();
    await expect(providerSelect(page)).toHaveCount(0);

    // Survives a reopen (the get-tags read), and NAMES itself on the card (the
    // `{tagId, tag}` envelope read correctly — Bug 74's third layer drew every
    // pill empty).
    const tagged = page.locator('.qt-card').filter({ hasText: TAG_PROFILE });
    await expect(tagged.locator('.qt-tag-badge').filter({ hasText: TAG_NAME })).toHaveCount(1, {
      timeout: 15_000,
    });
    await tagged.getByRole('button', { name: 'Edit' }).click();
    await expect(page.locator('.qt-tag-badge').filter({ hasText: TAG_NAME })).toHaveCount(1, {
      timeout: 15_000,
    });

    // Detach again, so the shared server is left as it was found.
    await page.getByRole('button', { name: `Remove ${TAG_NAME} tag` }).click();
    await expect(page.locator('.qt-tag-badge').filter({ hasText: TAG_NAME })).toHaveCount(0, {
      timeout: 15_000,
    });
    await page.getByRole('button', { name: 'Cancel' }).click();
    await expect(providerSelect(page)).toHaveCount(0);

    await tagged.getByRole('button', { name: 'Delete' }).click();
    await tagged.getByRole('button', { name: 'Confirm' }).click();
    await expect(page.locator('.qt-card').filter({ hasText: TAG_PROFILE })).toHaveCount(0, {
      timeout: 15_000,
    });
  });
});
