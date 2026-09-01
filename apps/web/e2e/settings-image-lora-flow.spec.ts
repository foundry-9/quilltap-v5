import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this rides the SHARED global-setup server and only unlocks it if
 * the gate is showing, so its filename must sort AFTER foundation.spec.ts
 * (which walks the locked→unlock gate first; workers: 1, alphabetical).
 * "settings-image-lora-flow" sorts after "aa-foundation".
 *
 * P4.D139 — the LoRA train's client half in the image-profile editor.
 *
 * Two beats, both gated: the round trip (a LoRA-capable model shows the
 * editor, an adapter saves and survives a reload) and the over-cap flag
 * (narrowing the model keeps the row and flags it, rather than deleting it).
 *
 * SELF-CLEANING: the beats create one profile each and delete it again, so the
 * shared server is left as it was found.
 */

/**
 * ACTIVATE-AT-UNIFY (lane P4.D139 → P4.D138 — the server half of the LoRA
 * train).
 *
 * Both beats need three things the sibling lane brings: the `list-models`
 * answer to carry `loraSupport`, the new `?action=options-schema` arm to
 * answer at all, and the create/update body to accept a `parameters.loras`
 * list rather than refusing it. Until then the editor correctly draws no LoRA
 * section (a model that resolves no support is ABSENT from the map, which IS
 * the "offer no rows" signal) and the beats would fail on a section that never
 * appears. Flip this to `true` at unification.
 *
 * A NAMED constant, deliberately, not a capability probe: a probe cannot tell
 * a model that legitimately declares no LoRA support — which is most of them,
 * and the whole point of the absent-not-empty rule — from a server that has
 * not learned to serve support at all, and would silently activate the beats
 * into guaranteed failure (the standing e2e rule). It also cannot tell a
 * dispatch verb that does not exist from one answering an empty bag: an
 * unknown field on a dispatch verb is silently IGNORED (memory note
 * `dispatch-verb-ignores-unknown-fields`), so a `loras` list would round-trip
 * as nothing at all and the reload assertion would fail for a reason that says
 * nothing about this lane.
 */
const P4D138_LORA_SERVER_LANDED = false;

/**
 * A NanoGPT model that declares LoRA support. NanoGPT is the feature's first
 * consumer (v4 `84f33ce94`), and this is the model whose family the static
 * dialect table covers. If P4.D138's manifest names a different one, change it
 * here — the beat asserts the SECTION, not this string.
 */
const LORA_MODEL = 'flux-2-dev';

const PROFILE_NAME = 'P4.D139 LoRA walk';
const CAP_PROFILE_NAME = 'P4.D139 LoRA over-cap walk';
const ADAPTER_SOURCE = 'XLabs-AI/flux-RealismLora';

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

/** Open Settings → Images with the Image Profiles card on screen. */
async function openImageProfilesCard(page: Page): Promise<void> {
  await page.goto('/salon');
  await maybeUnlock(page);
  await page.goto('/settings?tab=images&section=image-profiles');
  await expect(page.getByRole('button', { name: 'New Profile' })).toBeVisible({
    timeout: 15_000,
  });
}

/**
 * The modal's selects, in v4's field order: Provider, API Key, Model. None
 * carries an id, so they are taken positionally the way the sibling
 * image-profile beats do.
 */
function providerSelect(page: Page) {
  return page.locator('select').nth(0);
}

function modelSelect(page: Page) {
  return page.locator('select').nth(2);
}

function nameInput(page: Page) {
  return page.getByPlaceholder('e.g., DALL-E 3 HD');
}

/** Delete a profile by name, leaving the shared server as it was found. */
async function deleteProfile(page: Page, name: string): Promise<void> {
  const card = page.locator('div.qt-card', { hasText: name });
  if ((await card.count()) === 0) return;
  await card.getByRole('button', { name: 'Delete', exact: true }).click();
  await card.getByRole('button', { name: 'Delete this profile?' }).click();
  await expect(page.locator('div.qt-card', { hasText: name })).toHaveCount(0, { timeout: 10_000 });
}

test.describe('P4.D139 — LoRA adapters on an image profile', () => {
  test('a LoRA-capable model offers the editor, and an adapter survives a reload', async ({
    page,
  }) => {
    test.skip(
      !P4D138_LORA_SERVER_LANDED,
      'the list-models answer carries no loraSupport and the write refuses a loras list until P4.D138 lands — flip P4D138_LORA_SERVER_LANDED when it does',
    );
    test.setTimeout(90_000);

    await openImageProfilesCard(page);
    await page.getByRole('button', { name: 'New Profile' }).click();
    await expect(providerSelect(page)).toBeVisible({ timeout: 15_000 });

    await nameInput(page).fill(PROFILE_NAME);
    await providerSelect(page).selectOption('NANOGPT');
    await modelSelect(page).selectOption(LORA_MODEL);

    // The section appears only because the server resolved support for THIS
    // model — the browser never re-implements that lookup.
    await expect(page.getByRole('heading', { name: 'LoRA Adapters (Optional)' })).toBeVisible({
      timeout: 15_000,
    });
    // The capacity sentence is composed from the declared cap and source kinds.
    await expect(page.getByText(/This model accepts (a single adapter|up to \d+ adapters)\./)).toBeVisible();
    await expect(
      page.getByText('No adapters attached — the model generates in its own native manner.'),
    ).toBeVisible();

    await page.getByRole('button', { name: 'Add LoRA' }).click();
    await expect(page.locator('#lora-source-0')).toBeVisible();
    await page.locator('#lora-source-0').fill(ADAPTER_SOURCE);
    await page.locator('#lora-trigger-0').fill('in the style of ohwx');

    // The Query button lights up only once a repository can be read out of the
    // source — the client-side repo-id twin deciding, with no wire involved.
    await expect(page.getByRole('button', { name: 'Query' })).toBeEnabled();

    await page.getByRole('button', { name: 'Create', exact: true }).click();
    await expect(page.locator('div.qt-card', { hasText: PROFILE_NAME })).toBeVisible({
      timeout: 15_000,
    });

    // The round trip that matters: a full reload, then re-open the editor.
    await page.reload();
    await maybeUnlock(page);
    await expect(page.getByRole('button', { name: 'New Profile' })).toBeVisible({
      timeout: 15_000,
    });
    await page
      .locator('div.qt-card', { hasText: PROFILE_NAME })
      .getByRole('button', { name: 'Edit' })
      .click();
    await expect(page.getByRole('heading', { name: 'LoRA Adapters (Optional)' })).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.locator('#lora-source-0')).toHaveValue(ADAPTER_SOURCE);
    await expect(page.locator('#lora-trigger-0')).toHaveValue('in the style of ohwx');
    await expect(page.getByText('1 of', { exact: false })).toBeVisible();

    await page.getByRole('button', { name: 'Cancel' }).click();
    await deleteProfile(page, PROFILE_NAME);
  });

  test('narrowing the model FLAGS an over-cap adapter rather than deleting it', async ({
    page,
  }) => {
    test.skip(
      !P4D138_LORA_SERVER_LANDED,
      'needs P4.D138 to serve per-model loraSupport with differing caps — flip P4D138_LORA_SERVER_LANDED when it does',
    );
    test.setTimeout(90_000);

    await openImageProfilesCard(page);
    await page.getByRole('button', { name: 'New Profile' }).click();
    await expect(providerSelect(page)).toBeVisible({ timeout: 15_000 });

    await nameInput(page).fill(CAP_PROFILE_NAME);
    await providerSelect(page).selectOption('NANOGPT');
    await modelSelect(page).selectOption(LORA_MODEL);
    await expect(page.getByRole('heading', { name: 'LoRA Adapters (Optional)' })).toBeVisible({
      timeout: 15_000,
    });

    // Fill to the model's declared cap, then one past it if the cap allows.
    await page.getByRole('button', { name: 'Add LoRA' }).click();
    await page.locator('#lora-source-0').fill(ADAPTER_SOURCE);

    const addButton = page.getByRole('button', { name: 'Add LoRA' });
    while (await addButton.isEnabled()) {
      const before = await page.locator('input[type=range]').count();
      await addButton.click();
      await expect(page.locator('input[type=range]')).toHaveCount(before + 1);
    }
    const rowCount = await page.locator('input[type=range]').count();

    // Now narrow to a model whose cap is smaller. Every row survives: the list
    // is FLAGGED, never trimmed, so widening again loses nothing.
    const options = await modelSelect(page).locator('option').allTextContents();
    const narrower = options.find((o) => o !== LORA_MODEL && o.trim().length > 0);
    expect(narrower, 'the fixture needs a second NanoGPT model to narrow to').toBeTruthy();
    await modelSelect(page).selectOption(narrower!);

    // Whatever the second model's cap turns out to be, the rows are all still
    // there — that is the invariant, not any particular warning count.
    await expect(page.locator('input[type=range]')).toHaveCount(rowCount);

    const warning = page.getByText(/^Beyond this model's limit of \d+ —/);
    if (await warning.count()) {
      await expect(warning.first()).toContainText(
        'kept on the profile, but left behind on every request until you remove an earlier adapter or return to a model that takes more.',
      );
    }

    await page.getByRole('button', { name: 'Cancel' }).click();
    await deleteProfile(page, CAP_PROFILE_NAME);
  });
});
