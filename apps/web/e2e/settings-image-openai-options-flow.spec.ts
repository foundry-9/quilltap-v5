import { type Locator } from '@playwright/test';

import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this rides the SHARED global-setup server and only unlocks it if
 * the gate is showing, so its filename must sort AFTER foundation.spec.ts
 * (which walks the locked→unlock gate first; workers: 1, alphabetical).
 * "settings-image-openai-options-flow" sorts after "aa-foundation".
 *
 * P4.D197 — the OpenAI image options schema reaching the image-profile editor
 * (v4 `d8d2890ee` / PR #62).
 *
 * `openai-image-options.spec.ts` proves the RENDERER draws v4's recorded
 * schema. This beat proves the WIRE: that a server-declared schema reaches the
 * modal's schema arm, that it is refetched when the model changes, and that the
 * values it writes survive a create + reload in the stored `parameters` bag.
 * Neither half is visible from the other — a unit spec cannot see the fetch,
 * and the fetch cannot be asserted without a schema on the far end.
 *
 * SELF-CLEANING: the beat creates one profile and deletes it again, and
 * removes the OPENAI api-key row if it was the one that seeded it, so the
 * shared server is left as it was found.
 */

/**
 * ACTIVATE-AT-UNIFY (lane P4.D197 → P4.D196 — the server half of the
 * `d8d2890ee` drift).
 *
 * The beat needs the `?action=options-schema` arm to answer a real schema for
 * `provider=OPENAI`, which is P4.D196's unit. Until it lands the action answers
 * `optionsSchema: null` for OPENAI and the modal correctly falls back to the
 * JSON textarea — so every assertion below would fail on controls that never
 * appear, for a reason that says nothing about this lane. FLIP TO `true` at
 * unification, once P4.D196's OPENAI options-schema unit is picked.
 *
 * A NAMED constant, deliberately, not a capability probe: a probe cannot tell
 * a server that has not learned to serve the OpenAI schema from one that
 * legitimately declares none — `optionsSchema: null` is exactly what a provider
 * without a schema answers, and it is the answer OPENAI gives today — so a
 * probe would silently park the beat forever instead of failing loudly when
 * the server half regresses (the standing e2e rule).
 */
const P4D196_SERVER_LANDED = true;

/** The 2.5 family: six quality tiers, the thirteen wide sizes, no style. */
const SUNBURST = 'gpt-image-2.5-sunburst';
/** The one family that carries `style` and no `GPT Image Output` group. */
const DALL_E_3 = 'dall-e-3';

const PROFILE_NAME = 'P4.D197 OpenAI options walk';

/** The five values the beat stores, one per control the schema declares. */
const SAVED = {
  quality: 'max',
  size: '2048x2048',
  background: 'transparent',
  output_format: 'webp',
  output_compression: 80,
  moderation: 'low',
} as const;

/**
 * Unlock only when the passphrase screen is showing (the shared server stays
 * unlocked). `ready` is a parameter because the call sites land on DIFFERENT
 * routes — see `settings-image-lora-flow.spec.ts`, whose first live run failed
 * waiting for the Salon's landmark on the Settings screen.
 */
async function maybeUnlock(page: Page, ready?: Locator): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const landmark = ready ?? page.getByRole('heading', { name: 'Chats', exact: true });
  await expect(passphrase.or(landmark).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
    await expect(landmark).toBeVisible({ timeout: 15_000 });
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
 * The modal's own selects, in v4's field order: Provider, API Key, Model. They
 * carry no ids, so they are taken positionally the way the sibling
 * image-profile beats do — and deliberately NOT by `page.locator('select')`
 * index once the schema panel is drawn, because the panel adds selects of its
 * own BELOW them. The panel's own controls are addressed by the id the
 * renderer assigns (`pof-<key>`), which is unambiguous.
 */
function providerSelect(page: Page) {
  return page.locator('select').nth(0);
}

function apiKeySelect(page: Page) {
  return page.locator('select').nth(1);
}

function modelSelect(page: Page) {
  return page.locator('select').nth(2);
}

function nameInput(page: Page) {
  return page.getByPlaceholder('e.g., DALL-E 3 HD');
}

/** One schema-panel control, by the id `ProviderOptionsPanel` assigns. */
function optionSelect(page: Page, key: string) {
  return page.locator(`#pof-${key}`);
}

/**
 * Make sure an active OPENAI api-key row exists, because the modal's Create is
 * gated on `apiKeyId` and the eligible list is filtered by provider.
 *
 * MEASURED, not assumed: global setup seeds SERPER and NANOGPT rows explicitly
 * and everything else inherits its provider from a connection profile, so
 * whether an OPENAI row is there depends on the committed fixture's vintage.
 * Rather than edit the shared seeder (another lane's file), the beat asks and
 * creates its own if needed — through the API, never SQL, since a store-overlay
 * write behind the server's back is invisible to it. Returns the id it created,
 * or null when it found one already there, so the cleanup removes only its own.
 */
async function ensureOpenAiKey(page: Page): Promise<string | null> {
  const listed = await page.request.post('/api/dispatch', {
    data: { type: 'apiKeyList' },
  });
  expect(listed.ok(), `apiKeyList → ${listed.status()}`).toBe(true);
  // The dispatch envelope is `{type, data}` (v5 `api::types::Response`), and
  // `apiKeyList` answers `{apiKeys, count}`.
  const body = (await listed.json()) as {
    data?: { apiKeys?: { id: string; provider: string; isActive: boolean }[] };
  };
  const rows = body.data?.apiKeys;
  expect(Array.isArray(rows), 'apiKeyList answered an apiKeys array').toBe(true);
  if ((rows ?? []).some((k) => k.provider === 'OPENAI' && k.isActive)) return null;

  const created = await page.request.post('/api/dispatch', {
    data: {
      type: 'apiKeyCreate',
      label: 'E2E OpenAI (P4.D197)',
      provider: 'OPENAI',
      // Synthetic and never sent: the beat reads declarations only.
      apiKey: 'e2e-synthetic-openai-key',
    },
  });
  expect(created.ok(), `apiKeyCreate → ${created.status()}`).toBe(true);
  const madeBody = (await created.json()) as { data?: { apiKey?: { id?: string } } };
  const id = madeBody.data?.apiKey?.id;
  expect(id, 'apiKeyCreate returned an id').toBeTruthy();
  return id as string;
}

/** Pick the OPENAI key the modal offers (after the provider is chosen). */
async function pickOpenAiKey(page: Page): Promise<void> {
  await expect(apiKeySelect(page).locator('option')).not.toHaveCount(1, { timeout: 15_000 });
  const value = await apiKeySelect(page).locator('option').nth(1).getAttribute('value');
  expect(value, 'an eligible OPENAI api-key option').toBeTruthy();
  await apiKeySelect(page).selectOption(value!);
}

/** Delete a profile by name, leaving the shared server as it was found. */
async function deleteProfile(page: Page, name: string): Promise<void> {
  const card = page.locator('div.qt-card', { hasText: name });
  if ((await card.count()) === 0) return;
  await card.getByRole('button', { name: 'Delete', exact: true }).click();
  await card.getByRole('button', { name: 'Delete this profile?' }).click();
  await expect(page.locator('div.qt-card', { hasText: name })).toHaveCount(0, { timeout: 10_000 });
}

test.describe('P4.D197 — the OpenAI image options schema in the profile editor', () => {
  test('a model change re-serves the schema, and the chosen values survive a reload', async ({
    page,
  }) => {
    test.skip(
      !P4D196_SERVER_LANDED,
      'the options-schema action answers optionsSchema: null for OPENAI until P4.D196 lands — flip P4D196_SERVER_LANDED when it does',
    );
    test.setTimeout(120_000);

    const seededKeyId = await ensureOpenAiKey(page);
    try {
      await openImageProfilesCard(page);
      await page.getByRole('button', { name: 'New Profile' }).click();
      await expect(providerSelect(page)).toBeVisible({ timeout: 15_000 });

      await nameInput(page).fill(PROFILE_NAME);
      await providerSelect(page).selectOption('OPENAI');
      // Dogfood finding #108: the Provider select's rows come from an `@for`
      // over an async list, so a bound value landed before the options existed
      // and the browser settled on row 0. Assert the VALUE after the render,
      // never the click.
      await expect(providerSelect(page)).toHaveValue('OPENAI');
      await pickOpenAiKey(page);
      await modelSelect(page).selectOption(SUNBURST);

      // --- The schema arm, for the 2.5 family -----------------------------
      //
      // Every assertion below is on a control that exists only because the
      // SERVER declared it: the browser has no OpenAI capability table.
      await expect(page.getByRole('heading', { name: 'Image Parameters' })).toBeVisible({
        timeout: 15_000,
      });
      await expect(page.getByRole('heading', { name: 'GPT Image Output' })).toBeVisible();

      // Quality: the six 2.5 tiers plus the blank. `max` is the discriminator —
      // it exists in NO other family's list, so its presence cannot be a stale
      // render of some earlier model.
      await expect(optionSelect(page, 'quality').locator('option')).toHaveCount(7);
      await expect(
        optionSelect(page, 'quality').locator('option[value="max"]'),
      ).toHaveCount(1);
      await expect(
        optionSelect(page, 'quality').locator('option[value="xhigh"]'),
      ).toHaveCount(1);

      // Default Size: the thirteen wide sizes plus the blank — and `2048x2048`
      // as the discriminator (it is in the WIDE list only), so fourteen wrong
      // options cannot pass the count.
      await expect(optionSelect(page, 'size').locator('option')).toHaveCount(14);
      await expect(
        optionSelect(page, 'size').locator('option[value="2048x2048"]'),
      ).toHaveCount(1);

      // The GPT Image Output group's four fields, by their storage keys.
      for (const key of ['background', 'output_format', 'moderation']) {
        await expect(optionSelect(page, key), `select for ${key}`).toBeVisible();
      }
      await expect(page.locator('#pof-output_compression')).toBeVisible();
      await expect(page.locator('#pof-output_compression')).toHaveAttribute('type', 'number');

      // Style belongs to DALL·E 3 alone — it must NOT be here.
      await expect(optionSelect(page, 'style')).toHaveCount(0);

      // --- The refetch: switch families -----------------------------------
      await modelSelect(page).selectOption(DALL_E_3);
      // Style appearing is the positive signal the refetch landed; waiting on
      // it first means the negatives below are read AFTER the new schema, not
      // against the old one still on screen.
      await expect(optionSelect(page, 'style')).toBeVisible({ timeout: 15_000 });
      await expect(optionSelect(page, 'style').locator('option')).toHaveCount(3);
      await expect(page.getByRole('heading', { name: 'GPT Image Output' })).toHaveCount(0);
      await expect(page.locator('#pof-output_compression')).toHaveCount(0);
      // DALL·E 3's quality list is exactly blank + standard + hd, and its size
      // list the three DALL·E 3 shapes plus the blank (no `2048x2048`).
      await expect(optionSelect(page, 'quality').locator('option')).toHaveCount(3);
      await expect(optionSelect(page, 'size').locator('option')).toHaveCount(4);
      await expect(
        optionSelect(page, 'size').locator('option[value="2048x2048"]'),
      ).toHaveCount(0);
      await expect(
        optionSelect(page, 'quality').locator('option[value="max"]'),
      ).toHaveCount(0);

      // --- Back to 2.5, set the five values, save -------------------------
      await modelSelect(page).selectOption(SUNBURST);
      await expect(page.getByRole('heading', { name: 'GPT Image Output' })).toBeVisible({
        timeout: 15_000,
      });
      await optionSelect(page, 'quality').selectOption(SAVED.quality);
      await optionSelect(page, 'size').selectOption(SAVED.size);
      await optionSelect(page, 'background').selectOption(SAVED.background);
      await optionSelect(page, 'output_format').selectOption(SAVED.output_format);
      await optionSelect(page, 'moderation').selectOption(SAVED.moderation);
      await page.locator('#pof-output_compression').fill(String(SAVED.output_compression));

      await page.getByRole('button', { name: 'Create', exact: true }).click();
      await expect(page.locator('div.qt-card', { hasText: PROFILE_NAME })).toBeVisible({
        timeout: 15_000,
      });

      // --- The round trip -------------------------------------------------
      const newProfile = page.getByRole('button', { name: 'New Profile' });
      await page.reload();
      await maybeUnlock(page, newProfile);
      await expect(newProfile).toBeVisible({ timeout: 15_000 });
      await page
        .locator('div.qt-card', { hasText: PROFILE_NAME })
        .getByRole('button', { name: 'Edit' })
        .click();
      await expect(page.getByRole('heading', { name: 'GPT Image Output' })).toBeVisible({
        timeout: 15_000,
      });
      // Finding #108 again, on the re-opened editor: OPENAI IS row 0 in the
      // provider list, so this assertion is weak on its own — the model select
      // below is the one that discriminates, and it is asserted for that reason.
      await expect(providerSelect(page)).toHaveValue('OPENAI');
      await expect(modelSelect(page)).toHaveValue(SUNBURST);
      await expect(optionSelect(page, 'quality')).toHaveValue(SAVED.quality);
      await expect(optionSelect(page, 'size')).toHaveValue(SAVED.size);
      await expect(optionSelect(page, 'background')).toHaveValue(SAVED.background);
      await expect(optionSelect(page, 'output_format')).toHaveValue(SAVED.output_format);
      await expect(optionSelect(page, 'moderation')).toHaveValue(SAVED.moderation);
      await expect(page.locator('#pof-output_compression')).toHaveValue(
        String(SAVED.output_compression),
      );

      // --- And what the SERVER actually stored ----------------------------
      //
      // The controls above could read back correctly from a bag the server
      // never saw, so the stored shape is read straight off the wire. The
      // compression value is asserted as a NUMBER: the panel's number field
      // emits `Number(raw)`, and a string `'80'` would be a silent behaviour
      // change that every visual assertion above would still pass.
      const listResp = await page.request.post('/api/dispatch', {
        data: { type: 'imageProfileList' },
      });
      expect(listResp.ok(), `imageProfileList → ${listResp.status()}`).toBe(true);
      const listBody = (await listResp.json()) as {
        data?: { profiles?: { id: string; name: string }[] };
      };
      const row = (listBody.data?.profiles ?? []).find((p) => p.name === PROFILE_NAME);
      expect(row, `the created profile is listed as ${PROFILE_NAME}`).toBeTruthy();

      // `imageProfileGet` answers the BARE enriched profile as its `data`
      // (v5 `image_profile_get` → `Response::ImageProfile(v)`), not a
      // `{profile}` wrapper — the shape `image_profiles_routes_equivalence`
      // pins against v4's own route.
      const getResp = await page.request.post('/api/dispatch', {
        data: { type: 'imageProfileGet', profileId: row!.id },
      });
      expect(getResp.ok(), `imageProfileGet → ${getResp.status()}`).toBe(true);
      const got = (await getResp.json()) as {
        data?: { modelName?: string; parameters?: Record<string, unknown> };
      };
      expect(got.data, 'imageProfileGet returned a profile').toBeTruthy();
      expect(got.data!.modelName).toBe(SUNBURST);
      const stored = got.data!.parameters ?? {};
      expect(stored['quality']).toBe(SAVED.quality);
      expect(stored['size']).toBe(SAVED.size);
      expect(stored['background']).toBe(SAVED.background);
      expect(stored['output_format']).toBe(SAVED.output_format);
      expect(stored['moderation']).toBe(SAVED.moderation);
      expect(stored['output_compression']).toBe(SAVED.output_compression);

      await page.getByRole('button', { name: 'Cancel' }).click();
      await deleteProfile(page, PROFILE_NAME);
    } finally {
      // A mid-beat failure must not leave the profile behind for the next run
      // to trip over in strict mode; `deleteProfile` is a no-op when the card
      // is absent, and a cleanup failure must never mask the beat's own error.
      await deleteProfile(page, PROFILE_NAME).catch(() => undefined);
      if (seededKeyId) {
        await page.request.post('/api/dispatch', {
          data: { type: 'apiKeyDelete', apiKeyId: seededKeyId },
        });
      }
    }
  });
});
