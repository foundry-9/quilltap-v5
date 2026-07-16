import { expect, request as pwRequest, test, type Page } from '@playwright/test';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so
 * its filename must sort AFTER foundation.spec.ts (workers: 1, alphabetical
 * file order) — "home-flow" ('h') sorts after "foundation" ('f').
 *
 * P4.6av — a LIVE browser walk of the Home dashboard at `/`:
 *   1. Home beat: `/` renders the welcome greeting + at least one populated
 *      section over the seeded instance (Lorian + Riya + the salon fixture
 *      chats) → the "Start a Chat" quick action navigates to `/salon/new` →
 *      a character card body-click lands on `/characters/:id`.
 *   2. Nav beat: the shell's brand button (title "Home") navigates to `/`.
 *
 * PROBE-GUARDED, ACTIVATE-AT-UNIFY (record-and-fallback — the P4.6ar round
 * memory). The `systemHome` dispatch lands in lane A (P4.6au); in-lane the
 * shared server answers it with an unknown-variant error, so the payload
 * beats fall back to asserting the v4-faithful fetch-error state
 * ("The home parlour could not be readied just now.") and skip. They
 * auto-activate once lane A's verb merges at unification — no unifier wire
 * needed beyond the merge itself.
 *
 * The walk only READS + navigates (the New Project dialog stays closed) —
 * the shared fixture is left pristine.
 */

let homeBackendReady = false;

test.beforeAll(async () => {
  // Probe: is the systemHome dispatch handled? In-lane the Rust core has no
  // such variant, so the request fails with "unknown variant" → not ready. A
  // success envelope (or any domain error) means the handler exists → ready.
  try {
    const ctx = await pwRequest.newContext();
    const res = await ctx.post(`${BASE_URL}/api/dispatch`, {
      data: { type: 'systemHome' },
    });
    const body = (await res.json().catch(() => null)) as
      | { type?: string; data?: { message?: string } }
      | null;
    await ctx.dispose();
    const isUnknownVariant =
      body?.type === 'error' && /unknown variant/i.test(String(body?.data?.message ?? ''));
    homeBackendReady = body != null && !isUnknownVariant;
  } catch {
    homeBackendReady = false;
  }
});

/**
 * Unlock only when the passphrase screen is showing (the shared server stays
 * unlocked). Home-aware: the already-unlocked marker is the home screen
 * itself — the dashboard container when the backend is live, the v4-faithful
 * error copy when it is not.
 */
async function maybeUnlockAtHome(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const home = page
    .locator('.qt-homepage-container')
    .or(page.getByText('The home parlour could not be readied just now.'));
  await expect(passphrase.or(home).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

test.describe('P4.6av — the Home dashboard at /', () => {
  test('renders the dashboard, quick-actions navigate, a card body-click opens the character', async ({
    page,
  }) => {
    await page.goto('/');
    await maybeUnlockAtHome(page);

    if (!homeBackendReady) {
      // Record-and-fallback (ACTIVATE-AT-UNIFY): until lane A's systemHome
      // verb merges, the screen surfaces the v4-faithful fetch-error state —
      // prove that arm, then bow out.
      await expect(
        page.getByText('The home parlour could not be readied just now.'),
      ).toBeVisible();
      test.skip(true, 'systemHome dispatch not on this server yet — activates at unification');
      return;
    }

    // The welcome greeting (v4 WelcomeSection) over the live payload.
    await expect(page.getByText('Welcome back,')).toBeVisible();
    await expect(page.getByText('What would you like to do today?')).toBeVisible();

    // All three section headers render.
    await expect(page.getByRole('heading', { name: 'Recent Chats' })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Active Projects' })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Characters', exact: true })).toBeVisible();

    // At least one populated section over the seeded instance (the salon
    // fixture carries chats; the sample seed carries Lorian + Riya). Never an
    // absolute count — sibling specs may grow the data (P4.6ap memory).
    const chatRows = page.locator('qt-recent-chat-item');
    const characterCards = page.locator('qt-home-character-card');
    await expect(async () => {
      expect((await chatRows.count()) > 0 || (await characterCards.count()) > 0).toBe(true);
    }).toPass();

    // Quick action: Start a Chat → the New-Chat form.
    await page.getByRole('link', { name: 'Start a Chat' }).click();
    await expect(page.getByRole('heading', { name: 'New Chat', exact: true })).toBeVisible();
    expect(page.url()).toContain('/salon/new');

    // Back home for the card beat.
    await page.goto('/');
    await expect(page.locator('.qt-homepage-container')).toBeVisible();

    if (await characterCards.count()) {
      // Body-click (the top-left padding — off the inner link and the Chat
      // button, so the finding-#4 guard lets the card navigate).
      await characterCards.first().locator('.qt-character-card').click({ position: { x: 4, y: 4 } });
      await expect(page).toHaveURL(/\/characters\/[0-9a-f-]+$/);
    } else {
      // The seeded instance should carry characters; if a sibling spec ever
      // empties them, the empty arm still proves the section rendered.
      await expect(page.getByText('No favorite characters')).toBeVisible();
    }
  });

  test('the shell brand button navigates Home from the Salon', async ({ page }) => {
    await page.goto('/salon');
    // The shared server is unlocked by the beat above (or foundation).
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();

    await page.locator('a[title="Home"]').click();
    await expect(page).toHaveURL(`${BASE_URL}/`);
    if (homeBackendReady) {
      await expect(page.locator('.qt-homepage-container')).toBeVisible();
    } else {
      await expect(
        page.getByText('The home parlour could not be readied just now.'),
      ).toBeVisible();
    }
  });
});
