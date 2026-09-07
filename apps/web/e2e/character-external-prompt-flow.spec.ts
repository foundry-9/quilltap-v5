import {
  expect,
  request as pwRequest,
  test,
  type APIRequestContext,
  type Page,
} from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * p4.9k4 — the External Prompt dialog + result dialog, and the
 * Reverse-`{{user}}` picker (v4 `ExternalPromptDialog`/
 * `ExternalPromptResultDialog`/`ReverseUserDialog`).
 *
 * The External Prompt beat is ACTIVATE-AT-UNIFY behind
 * {@link P49K1_SERVER_LANDED} — `characterGenerateExternalPrompt` is P4.9K1's
 * (a sibling lane, not yet on this branch); a NAMED constant, never a
 * capability probe (the standing e2e rule).
 *
 * The Reverse-`{{user}}` beat needs NO sibling lane — that feature already
 * rides existing verbs (`characterUpdate`/`characterPromptUpdate`) and is not
 * gated. It seeds its own throwaway second user-controlled character and a
 * literal `{{user}}` token via API dispatch (never SQL), so it does not
 * depend on the shared fixture's shape.
 */
const P49K1_SERVER_LANDED = false;

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

async function openAria(page: Page): Promise<void> {
  await page.goto('/salon');
  await maybeUnlock(page);
  await page.goto('/characters');
  await expect(page.getByRole('heading', { name: 'Characters', exact: true })).toBeVisible({
    timeout: 15_000,
  });
  const card = page.locator('qt-character-card').filter({ hasText: 'Aria' }).first();
  await card.locator('h2').click();
  await expect(page.getByRole('heading', { name: 'Aria', level: 1 })).toBeVisible({ timeout: 15_000 });
}

async function dispatch(ctx: APIRequestContext, req: unknown): Promise<Record<string, unknown>> {
  const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: req });
  const body = (await res.json().catch(() => null)) as { data?: Record<string, unknown> } | null;
  return body?.data ?? {};
}

test.describe('p4.9k4 — external prompt + reverse-{{user}}', () => {
  test('generate an external prompt, view the result, copy it', async ({ page }) => {
    test.skip(!P49K1_SERVER_LANDED, 'awaits P4.9K1: the characterGenerateExternalPrompt verb');
    await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
    await openAria(page);

    await page.getByRole('button', { name: 'Non-Quilltap Prompt' }).click();
    await expect(page.getByRole('heading', { name: /Generate External Prompt/ })).toBeVisible();
    await page.getByRole('button', { name: 'Generate Prompt' }).click();

    await expect(page.getByRole('heading', { name: /Generated Prompt/ })).toBeVisible({
      timeout: 20_000,
    });
    await page.getByRole('button', { name: 'Copy' }).click();
    await expect(page.getByRole('button', { name: 'Copied' })).toBeVisible();
  });

  test('reverse-{{user}}: choosing a name replaces every literal token, verified through characterGet', async ({
    page,
  }) => {
    const ctx = await pwRequest.newContext();
    let secondCharacterId: string | null = null;
    let ariaId: string | null = null;
    let originalIdentity: string | null = null;
    try {
      // Seed a second user-controlled character to pick from the reverse picker.
      const created = await dispatch(ctx, {
        type: 'characterCreate',
        character: { name: 'Second Persona', controlledBy: 'user' },
      });
      secondCharacterId = (created['character'] as { id?: string } | undefined)?.id ?? null;
      expect(secondCharacterId).toBeTruthy();

      // Find Aria's id + current identity so the test can restore it afterward.
      const list = await dispatch(ctx, { type: 'characterList' });
      const characters = (list['characters'] as Array<{ id: string; name: string }>) ?? [];
      ariaId = characters.find((c) => c.name === 'Aria')?.id ?? null;
      expect(ariaId).toBeTruthy();
      const ariaBefore = await dispatch(ctx, { type: 'characterGet', characterId: ariaId });
      const ariaCharacter = ariaBefore['character'] as { identity?: string | null };
      originalIdentity = ariaCharacter.identity ?? null;

      await dispatch(ctx, {
        type: 'characterUpdate',
        characterId: ariaId,
        character: { identity: 'A keeper of the archive, sworn to serve {{user}} alone.' },
      });

      await openAria(page);
      await expect(page.getByText('sworn to serve {{user}} alone.')).toBeVisible({ timeout: 15_000 });

      const reverseButton = page.getByRole('button', { name: /^\{\{user\}\}.*name…/ });
      await reverseButton.click();
      await expect(page.getByRole('heading', { name: 'Restore {{user}} to a name' })).toBeVisible();
      await page
        .locator('select')
        .filter({ has: page.locator('option', { hasText: 'Second Persona' }) })
        .selectOption({ label: /Second Persona/ });
      await page.getByRole('button', { name: 'Replace {{user}}' }).click();

      await expect(page.getByText('Restored {{user}} to Second Persona')).toBeVisible({ timeout: 15_000 });

      // Verify through the PERSISTED state, never the optimistic UI.
      const ariaAfter = await dispatch(ctx, { type: 'characterGet', characterId: ariaId });
      const afterCharacter = ariaAfter['character'] as { identity?: string | null };
      expect(afterCharacter.identity).toBe(
        'A keeper of the archive, sworn to serve Second Persona alone.',
      );
    } finally {
      if (ariaId && originalIdentity !== null) {
        await dispatch(ctx, {
          type: 'characterUpdate',
          characterId: ariaId,
          character: { identity: originalIdentity },
        });
      }
      if (secondCharacterId) {
        await dispatch(ctx, {
          type: 'characterDelete',
          characterId: secondCharacterId,
          cascadeChats: true,
          cascadeImages: true,
        });
      }
      await ctx.dispose();
    }
  });
});
