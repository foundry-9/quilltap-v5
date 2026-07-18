import { expect, request as pwRequest, test, type Page } from '@playwright/test';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * The §4 quick-hide localStorage keys, mirrored from
 * `src/app/quick-hide/quick-hide.storage.ts`. Restated rather than imported:
 * the e2e is compiled standalone and does not reach into the app sources.
 */
const ACTIVE_TAGS_KEY = 'quilltap.quickHide.activeTags';
const HIDE_DANGEROUS_KEY = 'quilltap.quickHide.hideDangerous';

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so
 * its filename must sort AFTER foundation.spec.ts (workers: 1, alphabetical
 * file order) — "quick-hide-flow" ('q') sorts after "foundation" ('f').
 *
 * P4.9d — a LIVE walk of the quick-hide system:
 *   1. Authoring beat: seed a tag onto a character, then drive Settings →
 *      Appearance → Tags: pick the tag, Add Style, tick "Enable quick-hide
 *      button". The flag round-trips to the server (a reload still shows it
 *      ticked).
 *   2. Hiding beat: with the tag hidden, the tagged character vanishes from
 *      the roster; clearing restores it.
 *
 * ⚠ Why step 2 pokes localStorage instead of clicking the menu section:
 * `qt-quick-hide-menu-section` ships UNMOUNTED in this lane by §2b — lane
 * P4.9c owns the `qt-user-menu` that hosts it, and the unifier does the mount.
 * Writing the three §4 keys is exactly what the section's toggles do, so this
 * proves the whole chain the section would drive (storage → service → the
 * consumer's filter) in the real app. The MENU-WALK VARIANT IS
 * ACTIVATE-AT-UNIFY: once the section is mounted, replace the storage poke in
 * `setHiddenTags` with clicks on the section's per-tag eye button, and the rest
 * of the beat stands unchanged. The section's own behaviour (copy, both icon
 * polarities, the active class) is unit-covered meanwhile.
 *
 * The walk MUTATES the shared fixture (it creates a tag and styles it), so it
 * cleans up after itself in `afterAll`.
 */

const TAG_NAME = 'E2E Quick Hide';

let tagId: string | undefined;
let taggedCharacterId: string | undefined;
let taggedCharacterName: string | undefined;

/** Drive a dispatch verb straight at the shared server. */
async function dispatch(body: Record<string, unknown>): Promise<Record<string, unknown>> {
  const ctx = await pwRequest.newContext();
  const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: body });
  const parsed = (await res.json().catch(() => null)) as {
    type?: string;
    data?: Record<string, unknown>;
  } | null;
  await ctx.dispose();
  if (!parsed || parsed.type === 'error') {
    throw new Error(`dispatch ${String(body['type'])} failed: ${JSON.stringify(parsed)}`);
  }
  return parsed.data ?? {};
}

/**
 * The locked engine refuses every dispatch, so seeding must follow an unlock
 * (the established pattern — see `scriptorium-file-manager-flow.spec.ts:25-32`).
 * When the whole suite runs, foundation.spec.ts has already unlocked the shared
 * server and this is a no-op; run alone, this is what opens it.
 */
async function ensureUnlocked(): Promise<void> {
  const ctx = await pwRequest.newContext();
  await ctx
    .post(`${BASE_URL}/api/dispatch`, {
      data: { type: 'unlock', passphrase: E2E_PASSPHRASE },
    })
    .catch(() => undefined);
  await ctx.dispose();
}

test.beforeAll(async () => {
  await ensureUnlocked();
  // Seed: a fresh tag attached to whichever character the fixture leads with.
  // Never assert an absolute roster count — sibling specs can grow the seed.
  const chars = (await dispatch({ type: 'characterList' }))['characters'] as Array<{
    id: string;
    name: string;
  }>;
  if (!chars?.length) throw new Error('fixture carries no characters to tag');
  taggedCharacterId = chars[0].id;
  taggedCharacterName = chars[0].name;

  const created = (await dispatch({ type: 'tagCreate', name: TAG_NAME }))['tag'] as {
    id: string;
  };
  tagId = created.id;
  await dispatch({ type: 'characterAddTag', characterId: taggedCharacterId, tagId });
});

test.afterAll(async () => {
  if (tagId) {
    await dispatch({ type: 'tagDelete', tagId }).catch(() => undefined);
  }
});

/** Unlock only when the passphrase screen is showing (the shared server stays unlocked). */
async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  await page.waitForLoadState('domcontentloaded');
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

/**
 * Set the quick-hide state the way `qt-quick-hide-menu-section` would, then
 * reload so the service re-reads it. ACTIVATE-AT-UNIFY: replace with clicks on
 * the mounted section.
 */
async function setHiddenTags(page: Page, ids: string[]): Promise<void> {
  await page.evaluate(
    ([key, value]) => window.localStorage.setItem(key, value),
    [ACTIVE_TAGS_KEY, JSON.stringify(ids)] as const,
  );
  await page.reload();
}

test.describe('P4.9d — quick-hide authoring and hiding', () => {
  test('Settings → Appearance → Tags flags a tag for quick-hide, and it persists', async ({
    page,
  }) => {
    await page.goto(`${BASE_URL}/settings`);
    await maybeUnlock(page);

    await page.getByRole('button', { name: 'Appearance' }).click();

    // The Tags card is the last card in the tab.
    const tagsCard = page.locator('qt-settings-tags');
    await expect(tagsCard.getByText('Tag Management')).toBeVisible({ timeout: 15_000 });

    // The seeded tag shows in Management with its usage from the character.
    await expect(tagsCard.locator('.qt-tag-badge', { hasText: TAG_NAME })).toHaveCount(1);

    // Give it a style so the authoring card (and its checkbox) renders — v4
    // puts the quick-hide checkbox inside the style card, so a tag must be
    // styled before it can be flagged.
    await tagsCard.locator('select#tag-style-picker').selectOption({ label: TAG_NAME });
    await tagsCard.getByRole('button', { name: 'Add Style' }).click();

    const quickHideBox = tagsCard
      .locator('label', { hasText: 'Enable quick-hide button' })
      .locator('input[type="checkbox"]');
    await expect(quickHideBox).toBeVisible({ timeout: 10_000 });
    await expect(quickHideBox).not.toBeChecked();

    await quickHideBox.check();
    await expect(quickHideBox).toBeChecked();

    // The subtext is v4-verbatim.
    await expect(tagsCard.getByText('Adds this tag to the navbar quick-hide controls.')).toBeVisible();

    // It round-tripped to the server, not just the signal.
    await page.reload();
    await page.getByRole('button', { name: 'Appearance' }).click();
    await expect(
      tagsCard
        .locator('label', { hasText: 'Enable quick-hide button' })
        .locator('input[type="checkbox"]'),
    ).toBeChecked({ timeout: 15_000 });
  });

  test('hiding the tag removes the tagged character from the roster, and clearing restores it', async ({
    page,
  }) => {
    // The tag must be FLAGGED quickHide for hiding to bite: the service prunes
    // hidden ids down to the flagged set on every refresh (v4
    // quick-hide-provider.tsx:71-75), so an unflagged id is dropped and hides
    // nothing. Test 1 flags it through the UI; doing it here too keeps this
    // test independent of that one's ordering.
    await dispatch({ type: 'tagUpdate', tagId, tag: { quickHide: true } });

    await page.goto(`${BASE_URL}/characters`);
    await maybeUnlock(page);

    // Scope to the roster grid — the character's name can appear elsewhere on
    // the page (shell chrome, chat lists), and a bare text match would catch it.
    const card = page.locator('.character-card-grid qt-character-card', {
      hasText: taggedCharacterName!,
    });
    await expect(card).toHaveCount(1, { timeout: 15_000 });

    // Hide it.
    await setHiddenTags(page, [tagId!]);
    await expect(card).toHaveCount(0, { timeout: 15_000 });

    // Clear All Hidden (the section's clear path: the tag list empties; the
    // dangerous filter clears with it).
    await page.evaluate((key) => window.localStorage.setItem(key, 'false'), HIDE_DANGEROUS_KEY);
    await setHiddenTags(page, []);
    await expect(card).toHaveCount(1, { timeout: 15_000 });
  });
});

