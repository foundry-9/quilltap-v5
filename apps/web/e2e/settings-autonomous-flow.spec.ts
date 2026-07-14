import {
  expect,
  request as pwRequest,
  test,
  type APIRequestContext,
  type Page,
} from '@playwright/test';

import { BASE_URL, E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { MOCK_LLM_REPLY, startMockLlm, type MockLlm } from './support/mock-llm';

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so its
 * filename must sort AFTER foundation.spec.ts (foundation walks the locked→unlock
 * gate and must reach the shared server first; workers: 1, alphabetical order).
 * "settings-autonomous-flow" sorts after "foundation" ('se' > 'fo'). The room is
 * seeded through the frozen-live `chatCreate` dispatch AFTER the unlock (a dispatch
 * against the locked vault refuses, so a beforeAll seed would mis-skip in
 * isolation) — the P4.6z lesson.
 *
 * P4.6ad lane C — a LIVE browser walk of the autonomous-rooms vertical over the
 * shared Salon server: the Settings → Chat tab's two autonomous cards; a cron room
 * seeded via dispatch appears in the management list; Start → Pause → Resume, the
 * Edit-Enclave modal round-trip (clear a budget cap — the nullish arm), then Stop.
 * Plus the New-Chat autonomous toggle swapping in the shared editor card.
 *
 * A CRON room (not ad-hoc) is used deliberately: cron rooms wait for the scheduler
 * and do NOT auto-start, so the seeded room is deterministically `idle` — every
 * run-state transition in the walk is then a manual verb the SPA drives, not a
 * background turn we'd have to race. The mock LLM is started only so the turn a
 * manual Start enqueues has somewhere to land.
 */

let chatId = '';
let mock: MockLlm;

/** Dispatch one CoreRequest against the shared server; returns the `data` body. */
async function dispatch(ctx: APIRequestContext, req: unknown): Promise<Record<string, unknown>> {
  const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: req });
  const body = (await res.json().catch(() => null)) as {
    type?: string;
    data?: Record<string, unknown>;
  } | null;
  return body?.data ?? {};
}

/** Seed a cron autonomous room (idle) with two LLM characters + a budget cap. */
async function seedRoom(): Promise<void> {
  if (chatId) return;
  const ctx = await pwRequest.newContext();
  try {
    const list = ((await dispatch(ctx, { type: 'characterList' }))['characters'] ?? []) as Array<{
      id: string;
      controlledBy?: string;
    }>;
    const llm = list.filter((c) => c.controlledBy !== 'user').slice(0, 2);
    if (llm.length < 2) return; // fixture too thin — the walk skips
    const profiles = ((await dispatch(ctx, { type: 'connectionProfileList' }))['profiles'] ??
      []) as Array<{ id: string; provider?: string }>;
    const profile =
      profiles.find((p) => p.provider === 'OPENAI_COMPATIBLE')?.id ?? profiles[0]?.id ?? '';
    const created = await dispatch(ctx, {
      type: 'chatCreate',
      title: 'The E2E Enclave',
      chatType: 'autonomous',
      scheduleCron: '0 9 * * *',
      budgetMaxTurns: 5,
      participants: llm.map((c) => ({
        type: 'CHARACTER',
        characterId: c.id,
        controlledBy: 'llm',
        connectionProfileId: profile,
      })),
    });
    chatId = ((created['chat'] as { id?: string } | undefined)?.id ?? '') as string;
  } finally {
    await ctx.dispose();
  }
}

test.beforeAll(async () => {
  mock = await startMockLlm(MOCK_LLM_REPLY, MOCK_LLM_PORT);
});

test.afterAll(async () => {
  // Best-effort: stop the room so it doesn't sit "running" in the shared fixture.
  if (chatId) {
    try {
      const ctx = await pwRequest.newContext();
      await dispatch(ctx, { type: 'chatAutonomousRoomStop', chatId });
      await ctx.dispose();
    } catch {
      /* best-effort */
    }
  }
  await mock?.close();
});

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

test.describe('P4.6ad — autonomous-rooms vertical (settings + new-chat)', () => {
  test('the Settings Chat tab renders both autonomous cards', async ({ page }) => {
    await page.goto('/');
    await maybeUnlock(page);

    await page.goto('/settings?tab=chat&section=autonomous-rooms');
    // The Chat tab is LIVE (not the placeholder): both v4 autonomous cards render.
    await expect(page.getByText('Autonomous Rooms', { exact: true })).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByText('Scheduled Autonomous Rooms', { exact: true })).toBeVisible();
  });

  test('the Data Retention card loads, saves, and persists the stale-chat window', async ({
    page,
  }) => {
    await page.goto('/');
    await maybeUnlock(page);

    // The card is deep-linked open; it loads the current window (default 30).
    await page.goto('/settings?tab=chat&section=data-retention');
    const input = page.locator('#stale-chat-days');
    await expect(input).toHaveValue('30', { timeout: 15_000 });

    // Change it and commit on blur (live PUT through the dispatch surface). Wait
    // for the update dispatch to resolve before reloading (the draft reflects the
    // typed value immediately, so the response is the real sync point).
    await input.fill('45');
    const saved = page.waitForResponse(
      (r) =>
        r.url().includes('/api/dispatch') &&
        r.request().method() === 'POST' &&
        (r.request().postData() ?? '').includes('dataRetentionSettingsUpdate'),
    );
    await input.blur();
    await saved;

    // Reload — the persisted value (live GET) comes back.
    await page.goto('/settings?tab=chat&section=data-retention');
    await expect(page.locator('#stale-chat-days')).toHaveValue('45', { timeout: 15_000 });
  });

  test('the New-Chat autonomous toggle swaps in the enclave editor card', async ({ page }) => {
    await page.goto('/');
    await maybeUnlock(page);

    await page.goto('/salon/new?autonomous=1');
    // With ?autonomous=1 the form opens in autonomous mode (the heading reflects it).
    await expect(
      page.getByRole('heading', { name: 'New Autonomous Room', exact: true }),
    ).toBeVisible({ timeout: 15_000 });
    // The editor card (not Reality Injection) carries v4's verbatim steampunk string.
    await expect(page.getByText('Count only the dear tokens')).toBeVisible();
  });

  test('a seeded cron room: list → start → pause → resume → edit(clear cap) → stop', async ({
    page,
  }) => {
    await page.goto('/');
    await maybeUnlock(page);
    await seedRoom();
    test.skip(!chatId, 'fixture had fewer than two LLM characters — nothing to drive');

    // The management list lives under the "Scheduled Autonomous Rooms" card.
    await page.goto('/settings?tab=chat&section=autonomous-room-schedules');
    const row = page.locator('.border', { hasText: 'The E2E Enclave' }).first();
    await expect(row).toBeVisible({ timeout: 15_000 });
    await expect(row.getByText('idle', { exact: true })).toBeVisible();

    // Start → running (the manual verb flips the row synchronously).
    await row.getByRole('button', { name: 'Start' }).click();
    await expect(row.getByText('running', { exact: true })).toBeVisible({ timeout: 10_000 });

    // Pause → paused.
    await row.getByRole('button', { name: 'Pause' }).click();
    await expect(row.getByText('paused', { exact: true })).toBeVisible({ timeout: 10_000 });

    // Resume → running.
    await row.getByRole('button', { name: 'Resume' }).click();
    await expect(row.getByText('running', { exact: true })).toBeVisible({ timeout: 10_000 });

    // Edit → the modal loads the current settings; clear the turn cap (the nullish
    // arm) and save.
    await row.getByRole('button', { name: 'Edit' }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible({ timeout: 10_000 });
    const turnsInput = dialog.getByLabel(/turns/i).first();
    await expect(turnsInput).toBeVisible();
    await turnsInput.fill('');
    await dialog.getByRole('button', { name: /save/i }).click();
    await expect(dialog).toBeHidden({ timeout: 10_000 });

    // Stop → stopped (bumps currentRunId; any queued turn exits via the stale guard).
    await row.getByRole('button', { name: 'Stop' }).click();
    await expect(row.getByText('stopped', { exact: true })).toBeVisible({ timeout: 10_000 });
  });
});
