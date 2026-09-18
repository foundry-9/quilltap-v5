import { expect, request, test, type APIRequestContext, type Page } from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * P4.D202 — the System Prompts tab's star, end to end (v4 `baa85e19b`, bug 154).
 *
 * A NEW file rather than a beat inside `characters-flow.spec.ts`, deliberately:
 * that spec boots its OWN locked server on port 4322 against the committed
 * `characters-*` fixture pair, and this walk needs nothing from that fixture —
 * it mints a throwaway character of its own. Running on the shared
 * global-setup server keeps the suite one server lighter, and the file's name
 * says what it walks.
 *
 * ⚠ **ACTIVATE-AT-UNIFY behind {@link P4D201_SERVER_LANDED}.** The CLIENT half
 * of bug 154 is this lane's and is live; the lockstep the wire assertion reads
 * — the character's `defaultSystemPromptId` column moving with the prompt's
 * `isDefault` flag on every system-prompt write — is P4.D201's, a sibling lane
 * in this round and not on this branch. A NAMED constant, not a capability
 * probe: `characterPromptSetDefault` already EXISTS and already answers on
 * `main`, so a probe would see a working verb, activate the beat, and fail on
 * the column alone (the standing e2e rule — `character-archive-flow.spec.ts`
 * and `character-subprompts-flow.spec.ts`'s precedent).
 *
 * The UI half of the walk is UNGATED and runs today: the client's optimistic
 * cache write stands on its own, and the badge moving is the whole of what this
 * lane shipped. Only the `defaultSystemPromptId` assertion waits.
 *
 * ⚠ STILL OWED before the first live run (flip {@link P4D201_SERVER_LANDED} to
 * `true`): nothing but the sibling lane. The walk seeds no fixture state — it
 * creates its own character and deletes it in the same beat's tail, so nothing
 * it writes outlives it (the instance is shared with every other spec, the
 * `e2e-playwright-traps` coupling note).
 */
const P4D201_SERVER_LANDED = false;

const THROWAWAY = 'Pentimento';
const FIRST_PROMPT = 'The everyday voice';
const SECOND_PROMPT = 'The fighting has started';

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

/** Read an id back rather than transcribing one. */
async function characterId(ctx: APIRequestContext, name: string): Promise<string> {
  const data = await dispatch(ctx, { type: 'characterList' });
  const rows = (data['characters'] as Array<{ id: string; name?: string }> | undefined) ?? [];
  const found = rows.find((c) => c.name === name);
  expect(found, `the walk should have created a character named ${name}`).toBeTruthy();
  return found!.id;
}

/** Create one prompt through the tab's own modal. */
async function addPrompt(page: Page, name: string, content: string): Promise<void> {
  await page.getByRole('button', { name: '+ Add Prompt' }).click();
  await page.locator('input[placeholder="e.g., Romantic, Companion, Professional"]').fill(name);
  // The markdown field's contenteditable surface — the same handle the tab's
  // unit specs drive through `RichEditor`.
  await page.locator('qt-prompt-modal [contenteditable="true"]').first().fill(content);
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(page.locator('qt-prompt-modal')).toHaveCount(0, { timeout: 15_000 });
  await expect(
    page.getByRole('heading', { name, level: 4 }).or(page.getByText(name).first()),
  ).toBeVisible({ timeout: 15_000 });
}

/** The card for one prompt, by its name. */
function promptCard(page: Page, name: string) {
  return page.locator('.qt-card').filter({ hasText: name }).first();
}

test.describe('P4.D202 — the System Prompts tab’s star (v4 baa85e19b, bug 154)', () => {
  test('starring a prompt moves the badge, and the character’s default column moves with it', async ({
    page,
  }) => {
    test.setTimeout(90_000);
    const ctx = await request.newContext();
    let id: string | null = null;

    try {
      // A throwaway character, so the shared fixture's cast is left intact.
      await page.goto(`${BASE_URL}/characters`);
      await maybeUnlock(page);
      await page.goto(`${BASE_URL}/characters`);
      await page.getByRole('link', { name: 'Create Character' }).click();
      await page.locator('#name').fill(THROWAWAY);
      await page.getByRole('button', { name: /Create Character/i }).click();
      // The create screen lands on the character VIEW, not the editor
      // (`new-character.ts:513` — `/characters/{id}`), so the walk takes the
      // editor link the same way `characters-flow.spec.ts` does.
      await expect(page.getByRole('heading', { name: THROWAWAY, level: 1 })).toBeVisible({
        timeout: 15_000,
      });
      id = await characterId(ctx, THROWAWAY);

      await page.getByRole('link', { name: /Edit Character/i }).click();
      await page.getByRole('button', { name: 'System Prompts' }).click();
      await expect(page.getByText('No system prompts yet')).toBeVisible({ timeout: 15_000 });

      // Two prompts: the first starred through the create modal's own
      // checkbox, the second left plain, so the second is the one carrying a
      // star to click.
      await page.getByRole('button', { name: '+ Add Prompt' }).click();
      await page
        .locator('input[placeholder="e.g., Romantic, Companion, Professional"]')
        .fill(FIRST_PROMPT);
      await page.locator('qt-prompt-modal [contenteditable="true"]').first().fill('Speak plainly.');
      await page.locator('#isDefault').check();
      await page.getByRole('button', { name: 'Create', exact: true }).click();
      await expect(page.locator('qt-prompt-modal')).toHaveCount(0, { timeout: 15_000 });
      await addPrompt(page, SECOND_PROMPT, 'Speak urgently.');

      await expect(promptCard(page, FIRST_PROMPT).getByText('Default')).toBeVisible({
        timeout: 15_000,
      });

      // The star — the client half, UNGATED. This is the whole of what P4.D202
      // shipped: the badge moves on the click, before the round trip answers.
      await promptCard(page, SECOND_PROMPT).locator('[title="Set as default"]').click();
      await expect(promptCard(page, SECOND_PROMPT).getByText('Default')).toBeVisible({
        timeout: 15_000,
      });
      await expect(promptCard(page, FIRST_PROMPT).getByText('Default')).toHaveCount(0);

      // The wire — P4.D201's lockstep. The flag and the column must agree, and
      // both must name the prompt that was starred.
      const prompts = (
        (await dispatch(ctx, { type: 'characterPromptList', characterId: id }))[
          'prompts'
        ] as Array<{ id: string; name: string; isDefault: boolean }>
      ).map((p) => ({ name: p.name, id: p.id, isDefault: p.isDefault }));
      const starred = prompts.find((p) => p.name === SECOND_PROMPT)!;
      expect(prompts.filter((p) => p.isDefault).map((p) => p.name)).toEqual([SECOND_PROMPT]);

      const detail = (await dispatch(ctx, { type: 'characterGet', characterId: id }))[
        'character'
      ] as { defaultSystemPromptId?: string | null };
      test.skip(
        !P4D201_SERVER_LANDED,
        'awaits P4.D201: the systemPromptsPatch lockstep that moves defaultSystemPromptId with the flag',
      );
      expect(detail.defaultSystemPromptId).toBe(starred.id);
    } finally {
      if (id) await dispatch(ctx, { type: 'characterDelete', characterId: id });
      await ctx.dispose();
    }
  });
});
