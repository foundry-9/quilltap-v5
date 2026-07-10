import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';

import { expect, test } from '@playwright/test';

import { startMockLlm, MOCK_LLM_REPLY, type MockLlm } from './support/mock-llm';
import { cliBinary, E2E_PASSPHRASE, INSTANCE_DIR } from './support/env';

/**
 * M4: browser walk of the first Salon vertical — unlock → list → open a chat with
 * real history → the baked messages render (staff chip, whisper, reasoning) → send
 * → a streamed reply appears live → it survives a reload.
 *
 * SKIPPED until the P4.6 unification integrates the sibling lane (P4.6a): this
 * spec needs (1) the committed Salon fixture with an OPENAI_COMPATIBLE connection
 * profile and baked salon chats, and (2) the enriched `listChats` + `chatGet` +
 * `chatSend` server dispatch variants. Until then the read/send flow is carried by
 * the component tests (`salon-list.spec.ts`, `salon-conversation.spec.ts`,
 * `streaming-message.spec.ts`) and the render-parity fixtures. When both lanes are
 * on main, remove `.skip` and point `SALON_CHAT_TITLE` at the fixture's chat.
 *
 * At unification the setup below rewrites the fixture profile's `baseUrl` to the
 * in-process mock LLM so the real binary + spine round-trip a send with no live
 * model (the Shared-contract M4 recipe).
 */
const SALON_CHAT_TITLE = /.+/; // TODO(unification): the P4.6a fixture's salon chat title.

test.describe.skip('M4 — Salon vertical (list → open → read → send)', () => {
  let mock: MockLlm;

  test.beforeAll(async () => {
    mock = await startMockLlm();
    // Point the fixture's OPENAI_COMPATIBLE profile at the mock (TODO: pin the
    // profile id once the P4.6a fixture lands).
    const cli = cliBinary();
    spawnSync(
      cli,
      [
        'db',
        '--data-dir',
        INSTANCE_DIR,
        '--write',
        `UPDATE connection_profiles SET baseUrl = '${mock.url}/v1' WHERE provider = 'OPENAI_COMPATIBLE';`,
      ],
      { env: { ...process.env, QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' }, encoding: 'utf8' },
    );
  });

  test.afterAll(async () => {
    await mock?.close();
  });

  test('unlock → list → open → send → streamed reply survives reload', async ({ page }) => {
    await page.goto('/');
    await page.locator('#qt-passphrase').fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();

    // The Salon list.
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
    const card = page.locator('.chat-card-stack a.qt-entity-card').first();
    await expect(card).toBeVisible();
    await card.click();

    // The conversation with baked history (staff chip / whisper / reasoning).
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible();
    await expect(page.locator('.qt-chat-announcement-chip').first()).toBeVisible();

    // Send a message → the streamed mock reply appears live.
    const composer = page.locator('.qt-chat-composer-input');
    await composer.fill('Good morning.');
    await composer.press('Enter');
    await expect(page.getByText(MOCK_LLM_REPLY)).toBeVisible({ timeout: 15_000 });

    // The persisted reply survives a reload (canonical refetch).
    await page.reload();
    await page.locator('#qt-passphrase').fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
    await expect(page.getByText(MOCK_LLM_REPLY)).toBeVisible({ timeout: 15_000 });
    void SALON_CHAT_TITLE;
  });
});
