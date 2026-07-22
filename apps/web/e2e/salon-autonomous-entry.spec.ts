import {
  expect,
  request as pwRequest,
  test,
  type APIRequestContext,
  type Page,
} from './support/fixtures';
import { openSidebarSection } from './support/sidebar';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so its
 * filename must sort AFTER foundation.spec.ts (foundation walks the locked→unlock
 * gate and must reach the shared server first; workers: 1, alphabetical).
 * "salon-autonomous-entry" sorts after "foundation" ('sa' > 'fo'). The room is
 * seeded via the frozen-live `chatCreate` dispatch AFTER unlock (a dispatch
 * against the locked vault refuses) — the P4.6z lesson.
 *
 * P4.6af lane B — a LIVE walk of the two salon autonomous riders over the shared
 * Salon server: (1) the include-autonomous toggle (a seeded cron room is hidden
 * by default + the hidden hint shows; toggling ON reveals it), and (2) the
 * conversation-header Edit-Enclave button (renders for the autonomous room, opens
 * the frozen `qt-edit-enclave-modal`, and round-trips a title save). A CRON room
 * is used deliberately: it stays deterministically `idle` (no scheduler wake, no
 * background turn to race), so no mock LLM is needed here.
 */

let chatId = '';
const ROOM_TITLE = 'The Salon Entry Enclave';

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
      title: ROOM_TITLE,
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

test.describe('P4.6af — the salon autonomous riders', () => {
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
  });

  test('the toggle hides the seeded room by default and reveals it when ON', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await seedRoom();
    test.skip(!chatId, 'fixture had fewer than two LLM characters — nothing to seed');

    await page.goto('/salon');
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible({
      timeout: 15_000,
    });

    // Default (toggle OFF, owner_only visibility): the autonomous room is excluded,
    // and — since rooms exist — the hidden-rooms hint shows.
    await expect(page.getByText(ROOM_TITLE)).toHaveCount(0);
    await expect(
      page.getByText('You have autonomous rooms hidden by your visibility default'),
    ).toBeVisible();

    // Toggle ON → the room joins the list (the flag rides the query key → refetch).
    await page.getByRole('button', { name: 'Show Autonomous Rooms' }).click();
    await expect(page.getByText(ROOM_TITLE)).toBeVisible({ timeout: 15_000 });
    // With rooms now included, the hint is gone.
    await expect(
      page.getByText('You have autonomous rooms hidden by your visibility default'),
    ).toHaveCount(0);
  });

  test('the "New Autonomous Room" action links to /salon/new?autonomous=1', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    const link = page.getByRole('link', { name: 'New Autonomous Room' });
    await expect(link).toBeVisible();
    await link.click();
    await expect(page).toHaveURL(/\/salon\/new\?.*autonomous=1/);
  });

  test('the Edit-Enclave sidebar entry opens the modal and round-trips a title save', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await seedRoom();
    test.skip(!chatId, 'fixture had fewer than two LLM characters — nothing to drive');

    await page.goto(`/salon/${chatId}`);
    // The sidebar's Organize drawer renders the gated Edit-Enclave entry for the
    // autonomous room (P4.9H1 moved it back from the header — v4's home for it).
    await openSidebarSection(page, 'Organize');
    const editButton = page.getByRole('button', { name: 'Edit Enclave' });
    await expect(editButton).toBeVisible({ timeout: 15_000 });

    // Open the frozen enclave modal, change the title, and save.
    await editButton.click();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible({ timeout: 10_000 });
    await expect(dialog.getByText('Edit Enclave', { exact: true })).toBeVisible();
    const titleInput = dialog.locator('#edit-enclave-title');
    await expect(titleInput).toBeVisible({ timeout: 10_000 });
    await titleInput.fill('The Salon Entry Enclave (renamed)');
    await dialog.getByRole('button', { name: 'Save Changes' }).click();
    await expect(dialog).toBeHidden({ timeout: 10_000 });
  });
});
