import { expect, request, test, type APIRequestContext, type Page } from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * P4.D170 — character progressions end to end (v4 `25f534c0b`).
 *
 * The editor beats are LIVE from day one. The card saves through the ORDINARY
 * character PUT — the whole `metadata` object, replaced — and `characterUpdate`
 * carrying `metadata` has been on main since the store-backed-entity slice
 * (`db/vault_character_write.rs`), so beats (a) and (b) wait on nothing.
 *
 * Beat (c) is ACTIVATE-AT-UNIFY behind {@link P4D168_SERVER_LANDED}: the
 * prompt path — the section a character actually READS at the top of a turn —
 * is a sibling lane's, and until it lands a greeting carries no such block. A
 * NAMED constant, never a capability probe: a probe cannot tell a prompt
 * builder that is present-but-silent from one that genuinely emits, and would
 * silently activate the beat into a guaranteed failure (the standing e2e rule).
 *
 * The vault is the truth. Every assertion about what was saved reads
 * `metadata.progressions` back through `characterGet` rather than trusting the
 * screen — a card that rendered the right row while writing the wrong bytes is
 * exactly the failure mode the save-payload specs exist for, and only the
 * server can settle it.
 *
 * The character is **Bram**, the salon fixture's plain LLM seat, for the reason
 * `character-subprompts-flow` picks him: Aria carries the default systemPrompt
 * several specs key off and Dax is what `character-rename-flow` renames, while
 * nothing asserts Bram's fact sheet.
 *
 * **Everything this file writes, it removes.** The instance is shared with
 * every other spec (the `e2e-playwright-traps` coupling note), and `metadata`
 * is REPLACED whole by every write — so the tail of the last beat restores
 * Bram's metadata to whatever it was before the walk, read once at the top.
 */

/** Flipped at unification, once P4.D168's `build_progressions_section` lands. */
const P4D168_SERVER_LANDED = true;

const CHARACTER = 'Bram';
const PROGRESSION_NAME = 'Cannon recharge';
const PROGRESSION_ID = 'cannon-recharge';
const RENAMED = 'Main gun recharge';

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

async function dispatch(ctx: APIRequestContext, req: unknown): Promise<Record<string, unknown>> {
  const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: req });
  const body = (await res.json().catch(() => null)) as { data?: Record<string, unknown> } | null;
  return body?.data ?? {};
}

/** The fixture character's id, read back rather than transcribed. */
async function characterId(ctx: APIRequestContext, name: string): Promise<string> {
  const data = await dispatch(ctx, { type: 'characterList' });
  const rows = (data['characters'] as Array<{ id: string; name?: string }> | undefined) ?? [];
  const found = rows.find((c) => c.name === name);
  expect(found, `the fixture should carry a character named ${name}`).toBeTruthy();
  return found!.id;
}

/** The whole fact sheet, as the vault holds it. */
async function metadata(ctx: APIRequestContext, id: string): Promise<Record<string, unknown>> {
  const data = await dispatch(ctx, { type: 'characterGet', characterId: id });
  const character = (data['character'] as Record<string, unknown> | undefined) ?? {};
  const raw = character['metadata'];
  return typeof raw === 'object' && raw !== null && !Array.isArray(raw)
    ? (raw as Record<string, unknown>)
    : {};
}

/** The reserved key, or `undefined` when the vault does not carry it. */
async function progressions(
  ctx: APIRequestContext,
  id: string,
): Promise<Record<string, Record<string, unknown>> | undefined> {
  const bag = await metadata(ctx, id);
  return bag['progressions'] as Record<string, Record<string, unknown>> | undefined;
}

/**
 * Unlock through the BROWSER before any `/api/dispatch` read.
 *
 * The instance is locked until something unlocks it, and a dispatch against a
 * locked server answers no `data` at all — which surfaces as "the fixture
 * should carry a character named Bram", pointing at the fixture rather than at
 * the lock. In a full-suite run `aa-foundation.spec.ts` has already unlocked;
 * running this file ALONE is where it bites, and a beat that only passes inside
 * the suite is a beat nobody can debug.
 */
async function unlockedPage(page: Page): Promise<void> {
  await page.goto('/salon');
  await maybeUnlock(page);
}

async function openProgressionsCard(page: Page, name: string): Promise<void> {
  await unlockedPage(page);
  await page.goto('/characters');
  await expect(page.getByRole('heading', { name: 'Characters', exact: true })).toBeVisible({
    timeout: 15_000,
  });
  await page.locator('qt-character-card').filter({ hasText: name }).first().locator('h2').click();
  await expect(page.getByRole('heading', { name, level: 1 })).toBeVisible({ timeout: 15_000 });
  await page.getByRole('link', { name: /Edit Character/i }).click();
  await page.getByRole('button', { name: 'System Prompts' }).click();
  await expect(page.getByRole('heading', { name: 'Progressions', exact: true })).toBeVisible({
    timeout: 15_000,
  });
}

const card = (page: Page) => page.locator('qt-progressions-section');
const modal = (page: Page) => page.locator('qt-progression-editor-modal');

test.describe('P4.D170 — character progressions', () => {
  test('(a) Aurora: add, edit and delete a progression, with the vault read back each time', async ({
    page,
  }) => {
    await unlockedPage(page);
    const ctx = await request.newContext();
    const id = await characterId(ctx, CHARACTER);
    const before = await metadata(ctx, id);

    try {
      await openProgressionsCard(page, CHARACTER);

      // The card sits BELOW the subprompts one, which is v4's own slot.
      await expect(card(page)).toBeVisible();
      await expect(
        card(page).getByText(`Spans of time ${CHARACTER} is carrying`, { exact: false }),
      ).toBeVisible();

      // ---- add ----------------------------------------------------------
      await card(page).getByRole('button', { name: '+ Add Progression' }).click();
      await expect(modal(page).getByRole('heading', { name: 'New progression' })).toBeVisible();

      await modal(page).getByPlaceholder('Cannon recharge').fill(PROGRESSION_NAME);
      // The id follows the name until the author touches it.
      await expect(modal(page).getByPlaceholder('cannon', { exact: true })).toHaveValue(
        PROGRESSION_ID,
      );

      // A span that STRADDLES now, so the row reads `in progress` on any clock:
      // begins an hour ago, ends an hour out via the +1 hour shortcut.
      const begins = modal(page).locator('input[type="datetime-local"]').first();
      const startMs = Date.now() - 3_600_000;
      await begins.fill(
        new Date(startMs - new Date(startMs).getTimezoneOffset() * 60_000)
          .toISOString()
          .slice(0, 16),
      );
      await modal(page).getByRole('button', { name: '+1 day' }).click();

      // The live preview is the engine's own sentence, before anything is saved.
      await expect(
        modal(page).getByText(`As ${PROGRESSION_NAME} would read it now:`),
      ).toBeVisible();
      await expect(
        modal(page).getByText(new RegExp(`${PROGRESSION_NAME}: .* elapsed`)),
      ).toBeVisible();

      await modal(page).getByRole('button', { name: 'Add progression' }).click();
      await expect(page.getByText('Progression added')).toBeVisible({ timeout: 15_000 });

      // The row: name, id, the state pill, and the live line.
      const row = card(page).locator('.qt-card').filter({ hasText: PROGRESSION_NAME }).first();
      await expect(row.getByText(PROGRESSION_ID, { exact: true })).toBeVisible();
      await expect(row.getByText('in progress', { exact: true })).toBeVisible();
      await expect(row.getByText(new RegExp(`${PROGRESSION_NAME}: .* remaining`))).toBeVisible();

      // The vault is the truth.
      const saved = await progressions(ctx, id);
      expect(Object.keys(saved ?? {})).toEqual([PROGRESSION_ID]);
      expect(saved![PROGRESSION_ID]['name']).toBe(PROGRESSION_NAME);
      // Stamped, so the character's next turn reports it whatever the cadence.
      expect(typeof saved![PROGRESSION_ID]['updatedAt']).toBe('string');
      // Every OTHER metadata key the character had is still there.
      for (const [key, value] of Object.entries(before)) {
        if (key === 'progressions') continue;
        expect(await metadata(ctx, id).then((m) => m[key])).toEqual(value);
      }

      // ---- edit: the id is FIXED, because a tool file addresses it -------
      await card(page)
        .getByRole('button', { name: `Edit progression ${PROGRESSION_NAME}` })
        .click();
      await expect(
        modal(page).getByRole('heading', { name: `Edit “${PROGRESSION_NAME}”` }),
      ).toBeVisible();
      await expect(modal(page).getByPlaceholder('cannon', { exact: true })).toBeDisabled();
      await expect(modal(page).getByPlaceholder('cannon', { exact: true })).toHaveValue(
        PROGRESSION_ID,
      );

      await modal(page).getByPlaceholder('Cannon recharge').fill(RENAMED);
      await modal(page).getByRole('button', { name: 'Save changes' }).click();
      await expect(page.getByText('Progression updated')).toBeVisible({ timeout: 15_000 });

      const edited = await progressions(ctx, id);
      expect(Object.keys(edited ?? {})).toEqual([PROGRESSION_ID]);
      expect(edited![PROGRESSION_ID]['name']).toBe(RENAMED);

      // ---- delete through the inline popover ----------------------------
      await card(page)
        .getByRole('button', { name: `Delete progression ${RENAMED}` })
        .click();
      await expect(
        card(page).getByText(`Remove this progression? Any tool addressing`, { exact: false }),
      ).toBeVisible();
      await card(page).getByRole('button', { name: 'Delete', exact: true }).click();
      await expect(page.getByText('Progression removed')).toBeVisible({ timeout: 15_000 });

      await expect(card(page).getByText('Nothing in progress.', { exact: false })).toBeVisible({
        timeout: 15_000,
      });

      // The reserved key is GONE, not left empty — and the rest of the sheet
      // survived every one of the three writes.
      const after = await metadata(ctx, id);
      expect('progressions' in after).toBe(false);
      for (const [key, value] of Object.entries(before)) {
        if (key === 'progressions') continue;
        expect(after[key]).toEqual(value);
      }
    } finally {
      // `metadata` is replaced WHOLE by every write, so put back exactly what
      // was there — the instance is shared with every other spec.
      await dispatch(ctx, {
        type: 'characterUpdate',
        characterId: id,
        character: { metadata: before },
      });
      await ctx.dispose();
    }
  });

  test('(b) the editor refuses a backwards span, and writes nothing', async ({ page }) => {
    await unlockedPage(page);
    const ctx = await request.newContext();
    const id = await characterId(ctx, CHARACTER);
    const before = await metadata(ctx, id);

    try {
      await openProgressionsCard(page, CHARACTER);
      await card(page).getByRole('button', { name: '+ Add Progression' }).click();

      await modal(page).getByPlaceholder('Cannon recharge').fill('Fuse');
      const times = modal(page).locator('input[type="datetime-local"]');
      await times.nth(0).fill('2026-09-08T14:00');
      await times.nth(1).fill('2026-09-08T13:00');
      await modal(page).getByRole('button', { name: 'Add progression' }).click();

      // The schema's own sentence, rendered by the browser's copy of it.
      await expect(modal(page).getByText(/strictly after startTime/)).toBeVisible();

      // Nothing reached the vault — the refusal happens before the PUT.
      expect(await metadata(ctx, id)).toEqual(before);
    } finally {
      await ctx.dispose();
    }
  });

  test('(c) a greeting names the progression the character is carrying', async ({ page }) => {
    test.skip(
      !P4D168_SERVER_LANDED,
      'the prompt path is P4.D168’s — a greeting carries no progressions block yet',
    );

    await unlockedPage(page);
    const ctx = await request.newContext();
    const id = await characterId(ctx, CHARACTER);
    const before = await metadata(ctx, id);

    try {
      // Seed through the verb the card uses, never SQL: the vault's
      // `metadata.json` is a store-overlay file, and a SQL plant would leave
      // every hash beside it stale (the standing dogfood note).
      const startMs = Date.now() - 3_600_000;
      await dispatch(ctx, {
        type: 'characterUpdate',
        characterId: id,
        character: {
          metadata: {
            ...before,
            progressions: {
              cannon: {
                name: 'Cannon recharge',
                startTime: new Date(startMs).toISOString(),
                endTime: new Date(startMs + 7_200_000).toISOString(),
                timeIncrement: 'minute',
              },
            },
          },
        },
      });

      // The verb's shape is v4's `createChatSchema` (P4.78): a `participants`
      // roster, never a bare `characterIds` list — the first live run of this
      // beat sent the latter and the create answered no chat at all.
      // …and every LLM seat needs a connection profile, or the create refuses
      // (the `chat-delete-flow` idiom: the fixture's OPENAI_COMPATIBLE profile).
      const list = await dispatch(ctx, { type: 'connectionProfileList' });
      const profiles = (list['profiles'] ?? []) as Array<{ id: string; provider?: string }>;
      const profileId =
        profiles.find((p) => p.provider === 'OPENAI_COMPATIBLE')?.id ?? profiles[0]?.id;
      expect(profileId, 'the fixture must seed a connection profile').toBeTruthy();
      const created = await dispatch(ctx, {
        type: 'chatCreate',
        title: 'P4.D170 progressions greeting',
        participants: [
          { type: 'CHARACTER', characterId: id, controlledBy: 'llm', connectionProfileId: profileId },
        ],
      });
      const chatId = ((created['chat'] as { id?: string } | undefined) ?? {}).id!;
      expect(chatId, `the chat was created: ${JSON.stringify(created).slice(0, 300)}`).toBeTruthy();

      try {
        // The greeting FORCES the section, cadence bypassed, so the persisted
        // system-prompt head carries it on the very first turn.
        const events = await dispatch(ctx, { type: 'chatGet', chatId });
        const blob = JSON.stringify(events);
        expect(blob).toContain('Time-bound conditions you are carrying');
        expect(blob).toContain('Cannon recharge');
      } finally {
        await dispatch(ctx, { type: 'chatDelete', chatId });
      }
    } finally {
      await dispatch(ctx, {
        type: 'characterUpdate',
        characterId: id,
        character: { metadata: before },
      });
      await ctx.dispose();
    }
  });
});
