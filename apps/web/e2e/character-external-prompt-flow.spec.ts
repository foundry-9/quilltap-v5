import {
  expect,
  request as pwRequest,
  test,
  type APIRequestContext,
  type Page,
} from './support/fixtures';

import { createServer, type Server } from 'node:http';

import { BASE_URL, E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';

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
const P49K1_SERVER_LANDED = true;

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

/**
 * A non-streaming OPENAI-compatible chat-completions mock (the wizard beat's
 * shape) — the external-prompt generator calls the NON-streaming
 * `send_message`, which the shared SSE mock cannot answer, and this beat
 * started no mock at all before the `2f4254b42` unification's first live run
 * (its request died on an empty port).
 */
async function startNonStreamingMockLlm(
  reply: string,
  port: number,
): Promise<{ url: string; close: () => Promise<void> }> {
  const httpServer: Server = createServer((req, res) => {
    if (req.method === 'GET' && req.url?.includes('/models')) {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ data: [{ id: 'mock-model' }] }));
      return;
    }
    if (req.method !== 'POST' || !req.url?.includes('/chat/completions')) {
      res.writeHead(404).end();
      return;
    }
    req.on('data', () => {});
    req.on('end', () => {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(
        JSON.stringify({
          id: 'mock-external-1',
          object: 'chat.completion',
          model: 'mock-model',
          choices: [{ index: 0, message: { role: 'assistant', content: reply }, finish_reason: 'stop' }],
          usage: { prompt_tokens: 20, completion_tokens: 12, total_tokens: 32 },
        }),
      );
    });
  });
  const boundPort: number = await new Promise((res) => {
    httpServer.listen(port, '127.0.0.1', () => {
      const addr = httpServer.address();
      res(typeof addr === 'object' && addr ? addr.port : 0);
    });
  });
  return {
    url: `http://127.0.0.1:${boundPort}`,
    close: () => new Promise<void>((res, rej) => httpServer.close((e) => (e ? rej(e) : res()))),
  };
}

test.describe('p4.9k4 — external prompt + reverse-{{user}}', () => {
  test('generate an external prompt, view the result, copy it', async ({ page }) => {
    test.skip(!P49K1_SERVER_LANDED, 'awaits P4.9K1: the characterGenerateExternalPrompt verb');
    await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
    await openAria(page);

    const mockLlm = await startNonStreamingMockLlm(
      'You are Aria, a methodical archivist. Speak plainly and never raise your voice.',
      MOCK_LLM_PORT,
    );
    try {
      await page.getByRole('button', { name: 'Non-Quilltap Prompt' }).click();
      await expect(page.getByRole('heading', { name: /Generate External Prompt/ })).toBeVisible();
      await page.getByRole('button', { name: 'Generate Prompt' }).click();

      await expect(page.getByRole('heading', { name: /Generated Prompt/ })).toBeVisible({
        timeout: 20_000,
      });
      await page.getByRole('button', { name: 'Copy' }).click();
      await expect(page.getByRole('button', { name: 'Copied' })).toBeVisible();
    } finally {
      await mockLlm.close();
    }
  });

  test('reverse-{{user}}: choosing a name replaces every literal token, verified through characterGet', async ({
    page,
  }) => {
    const ctx = await pwRequest.newContext();
    let secondCharacterId: string | null = null;
    let ariaId: string | null = null;
    let originalIdentity: string | null = null;
    try {
      // UNLOCK FIRST: a raw dispatch before the instance is unlocked answers an
      // EMPTY character list (the P4.6z lesson `chat-delete-flow.spec.ts` and
      // `salon-autonomous-entry` both record) — this beat's first isolated run
      // died at `expect(ariaId).toBeTruthy()` for exactly that reason.
      await page.goto('/salon');
      await maybeUnlock(page);

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
      // `qt-template-display` renders a `{{user}}` token as the RESOLVED user
      // character name wearing a badge whose `title` carries the literal — so
      // the literal `{{user}}` is never visible text. Assert the rendered
      // shape: the prefix text plus the badge (the beat's first live run, at
      // the p4.9k unification, died on a literal-text assertion).
      const identity = page
        .locator('qt-template-display')
        .filter({ hasText: 'A keeper of the archive, sworn to serve' })
        .first();
      await expect(identity).toBeVisible({ timeout: 15_000 });
      await expect(identity.locator('[title="User character name (from {{user}})"]')).toBeVisible();

      const reverseButton = page.getByRole('button', { name: /^\{\{user\}\}.*name…/ });
      await reverseButton.click();
      await expect(page.getByRole('heading', { name: 'Restore {{user}} to a name' })).toBeVisible();
      // The picker's option values are character ids (`details-tab.ts:183`);
      // `selectOption`'s `label` must be a string, never a regex.
      await page
        .locator('select')
        .filter({ has: page.locator('option', { hasText: 'Second Persona' }) })
        .selectOption({ value: secondCharacterId! });
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
