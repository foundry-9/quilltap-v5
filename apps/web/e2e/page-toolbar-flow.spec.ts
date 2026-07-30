import { expect, request as pwRequest, test, type Page } from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * P4.9P — the top page-toolbar walk (dogfood #38):
 *   1. Gate beat: the toolbar is ABSENT on the unlock screen (the shell never
 *      mounts pre-operational) and PRESENT right after unlock, with v4's
 *      occupants — the search input, the five queue badges, the width toggle.
 *   2. Search beat: a real `GET /api/v1/ui/search` round-trip — type a
 *      fixture character's name into the toolbar bar, see the dropdown card,
 *      click it, land on the character screen (the `/aurora/{id}` →
 *      `/characters/{id}` mapping working end-to-end).
 *   3. Queue-badge beat: the deterministic variant (see the beat's comment —
 *      no real job can HOLD a countable state on this server, by design), a
 *      browser-side interception of the jobs endpoint driving the Sum badge
 *      through light → poll → dim-at-zero.
 *   4. Width beat: the toggle flips `data-full-width` on <html> and the
 *      preference survives a reload (v4's exact localStorage key).
 *
 * ORDERING: rides the shared global-setup server; 'p' sorts after
 * 'foundation' (workers: 1, alphabetical). Beat 1 LOCKS the shared server via
 * the `lock` verb and re-unlocks through the real UI in the same test, so
 * later files still find it unlocked.
 */

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

async function ensureUnlocked(): Promise<void> {
  const ctx = await pwRequest.newContext();
  await ctx
    .post(`${BASE_URL}/api/dispatch`, { data: { type: 'unlock', passphrase: E2E_PASSPHRASE } })
    .catch(() => undefined);
  await ctx.dispose();
}

/** Unlock through the UI only when the passphrase screen is showing. */
async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  await page.waitForLoadState('domcontentloaded');
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

let searchTargetId: string | undefined;
let searchTargetName: string | undefined;

test.beforeAll(async () => {
  await ensureUnlocked();
  const chars = (await dispatch({ type: 'characterList' }))['characters'] as Array<{
    id: string;
    name: string;
  }>;
  if (!chars?.length) throw new Error('fixture carries no characters to search for');
  searchTargetId = chars[0].id;
  searchTargetName = chars[0].name;
});

test.afterAll(async () => {
  // Belt-and-braces: never leave a locked server behind for later files.
  await ensureUnlocked();
});

test('the toolbar is absent on the unlock screen and mounts with its occupants after unlock', async ({
  page,
}) => {
  // Lock the shared server, then walk the unlock screen.
  await dispatch({ type: 'lock' });
  await page.goto('/salon');
  await expect(
    page.getByRole('heading', { name: 'Quilltap Awaits Your Credentials' }),
  ).toBeVisible();
  await expect(page.locator('qt-page-toolbar')).toHaveCount(0);

  // Unlock through the real UI — the shell (and with it the toolbar) mounts.
  await page.locator('#qt-passphrase').fill(E2E_PASSPHRASE);
  await page.getByRole('button', { name: 'Unlock' }).click();
  const toolbar = page.locator('.qt-page-toolbar');
  await expect(toolbar).toBeVisible();

  // v4's occupants: center search, the five queue badges, the width toggle.
  await expect(toolbar.getByPlaceholder('Search... (⌘K)')).toBeVisible();
  await expect(toolbar.locator('.qt-queue-badge-group > span')).toHaveCount(5);
  await expect(toolbar.locator('.qt-queue-badge-memory')).toContainText('Mem');
  await expect(toolbar.getByRole('button', { name: 'Switch to wide layout' })).toBeVisible();

  // The sidebar-footer stopgap is retired: any autonomous badges now live in
  // the toolbar, not the rail footer.
  await expect(page.locator('.qt-left-sidebar-footer qt-autonomous-room-badges')).toHaveCount(0);
  await expect(toolbar.locator('qt-autonomous-room-badges')).toHaveCount(1);
});

test('global search round-trips: query → dropdown card → navigate to the character', async ({
  page,
}) => {
  await page.goto('/salon');
  await maybeUnlock(page);
  const input = page.getByPlaceholder('Search... (⌘K)');
  await expect(input).toBeVisible();

  await input.fill(searchTargetName!);
  // The debounced fetch lands and the dropdown groups appear.
  const card = page.locator(`a[href="/characters/${searchTargetId}"]`).first();
  await expect(card).toBeVisible();
  await expect(card).toContainText(searchTargetName!);

  await card.click();
  await expect(page).toHaveURL(new RegExp(`/characters/${searchTargetId}`));
  // The click clears the bar (v4 handleResultClick).
  await expect(page.getByPlaceholder('Search... (⌘K)')).toHaveValue('');
});

test('the queue badges light while jobs are active and dim back to idle', async ({ page }) => {
  // THE DETERMINISTIC VARIANT (the order's "or"): no real job can HOLD a
  // countable state — `activeByType` counts PENDING|PROCESSING only
  // (v4-faithful), a handler-less probe is claimed and failed within
  // milliseconds, a retrying job sits in FAILED between attempts, and every
  // enqueue/resume path calls `pump.start()`, clearing any stop. So this beat
  // intercepts `/api/v1/system/jobs` IN THE BROWSER and drives the counts,
  // proving the live wiring (mount-fired check → render → 5s poll → dim at
  // zero) end to end; the SERVER side of the endpoint is pinned by the
  // P4.9G1/P4.9G3 differentials, and the first beat above already rendered
  // the badges from a real un-intercepted fetch.
  let activeByType: Record<string, number> = { SCENE_STATE_TRACKING: 2 };
  await page.route('**/api/v1/system/jobs', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ activeByType }),
    }),
  );

  await page.goto('/salon');
  await maybeUnlock(page);
  const sumBadge = page.locator('.qt-queue-badge-summary');
  await expect(sumBadge).toContainText('2');
  await expect(sumBadge).not.toHaveClass(/qt-queue-badge-idle/);
  await expect(sumBadge).toHaveAttribute(
    'title',
    'Post-turn processing queue (summaries, titles, scene state, rendering): 2 active',
  );
  // The other buckets stay idle.
  await expect(page.locator('.qt-queue-badge-memory')).toHaveClass(/qt-queue-badge-idle/);

  // The counts drain; the 5s poll notices and the badge dims to idle.
  activeByType = {};
  await expect(sumBadge).toContainText('0', { timeout: 10_000 });
  await expect(sumBadge).toHaveClass(/qt-queue-badge-idle/);

  await page.unroute('**/api/v1/system/jobs');
});

test('the width toggle flips data-full-width and persists across reload', async ({ page }) => {
  await page.goto('/salon');
  await maybeUnlock(page);
  const html = page.locator('html');
  await expect(html).not.toHaveAttribute('data-full-width', 'true');

  await page.getByRole('button', { name: 'Switch to wide layout' }).click();
  await expect(html).toHaveAttribute('data-full-width', 'true');
  await expect(html).toHaveCSS('--qt-page-max-width', '100%');

  // Persists across a reload (v4's exact localStorage key).
  await page.reload();
  await maybeUnlock(page);
  await expect(html).toHaveAttribute('data-full-width', 'true');
  await expect(page.getByRole('button', { name: 'Switch to narrow layout' })).toBeVisible();

  // Toggle back so the browser-context default is restored for the beat's end.
  await page.getByRole('button', { name: 'Switch to narrow layout' }).click();
  await expect(html).not.toHaveAttribute('data-full-width', 'true');
});
