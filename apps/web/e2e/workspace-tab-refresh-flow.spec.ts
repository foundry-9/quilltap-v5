import { expect, test, type Page } from '@playwright/test';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: rides the SHARED global-setup server, and sorts after
 * `workspace-flow` ('workspace-t' > 'workspace-f'), so it never disturbs the
 * route-mode specs. Like `workspace-flow.spec.ts` it imports the BASE
 * `@playwright/test` — NOT `./support/fixtures` — so the workspace flag stays
 * ON (the default) and tabs are real.
 *
 * P4.D90 (v4 `979652a9`) — a kept-alive tab refreshes when you navigate back to
 * it. Two paths, one beat each:
 *
 *  1. **The map path** — the Chats tab's TanStack reads are invalidated on
 *     re-activation (`workspace/core/tab-refetch.ts`), so returning re-issues
 *     `listChats`. The FIRST activation must NOT refetch (v4 mounts its
 *     invalidator only after the tab has been activated once), which the beat
 *     pins by counting.
 *  2. **The hand-rolled path** — the Scriptorium holds its stores in signals,
 *     so the list re-runs its own load with `{ silent: true }`: the current
 *     list stays on screen while the fresh one loads, no spinner swap.
 *
 * Both beats AWAIT the dispatch they trigger (`e2e-playwright-traps`
 * §the-unawaited-refetch — this feature IS a page-initiated refetch, the class
 * the 2026-08-01 deflake round hardened): the assertion is a `waitForRequest`
 * on the specific verb, never a timeout.
 */

async function openWorkspace(page: Page): Promise<void> {
  await page.goto('/');
  const passphrase = page.locator('#qt-passphrase');
  const workspace = page.locator('.qt-workspace');
  await expect(passphrase.or(workspace).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
  await expect(workspace).toBeVisible({ timeout: 15_000 });
}

const railLink = (page: Page, label: string) =>
  page.locator(`aside.qt-left-sidebar a.qt-collapsed-nav-button[aria-label="${label}"]`);
const tabLabel = (page: Page, text: string) =>
  page.locator('.qt-tab-strip .qt-tab-label', { hasText: text });

/** Every `POST /api/dispatch` verb the page has sent, in order. */
function dispatchLog(page: Page): string[] {
  const verbs: string[] = [];
  page.on('request', (req) => {
    if (req.method() !== 'POST' || !req.url().endsWith('/api/dispatch')) return;
    const body = req.postData();
    if (!body) return;
    try {
      verbs.push((JSON.parse(body) as { type?: string }).type ?? '');
    } catch {
      /* not our envelope */
    }
  });
  return verbs;
}

/** A predicate matching one dispatch verb, for `waitForRequest`. */
const dispatchOf = (verb: string) => (req: { url(): string; postData(): string | null }) =>
  req.url().endsWith('/api/dispatch') && (req.postData() ?? '').includes(`"${verb}"`);

test('returning to the Chats tab re-issues its list read', async ({ page }) => {
  const verbs = dispatchLog(page);
  await openWorkspace(page);

  await railLink(page, 'Chats').click();
  await expect(tabLabel(page, 'Chats')).toBeVisible();
  // Wait for the list itself, so the first read has certainly resolved.
  await expect(page.locator('.qt-tab-pane:not(.hidden) h1')).toBeVisible({ timeout: 15_000 });
  const listChats = () => verbs.filter((v) => v === 'listChats').length;
  const baseline = listChats();
  expect(baseline).toBeGreaterThan(0);

  // Park on a THIRD tab, not Home: `home`'s entry sweeps the chats prefix too
  // (v4's "sibling tabs sharing a prefix come back fresh"), so Home would be a
  // second refresher and the beat could not tell them apart. The Scriptorium's
  // entry is empty and its own hook touches only mount points — and this is its
  // FIRST activation, which never refreshes anything.
  await railLink(page, 'The Scriptorium').click();
  await expect(tabLabel(page, 'The Scriptorium')).toBeVisible();
  expect(listChats()).toBe(baseline);

  // …and back: the invalidation refetches. Await the request itself.
  const refetched = page.waitForRequest(dispatchOf('listChats'), { timeout: 15_000 });
  await tabLabel(page, 'Chats').click();
  await refetched;
  expect(listChats()).toBeGreaterThan(baseline);
});

test('returning to the Scriptorium re-lists its stores without swapping the page for a spinner', async ({
  page,
}) => {
  await openWorkspace(page);

  await railLink(page, 'The Scriptorium').click();
  await expect(tabLabel(page, 'The Scriptorium')).toBeVisible();
  const pane = page.locator('.qt-tab-pane:not(.hidden)');
  await expect(pane.getByRole('heading', { name: 'The Scriptorium' })).toBeVisible({
    timeout: 15_000,
  });

  await tabLabel(page, 'Home').click();
  await expect(tabLabel(page, 'Home')).toBeVisible();

  const relisted = page.waitForRequest(dispatchOf('mountPointList'), { timeout: 15_000 });
  await tabLabel(page, 'The Scriptorium').click();
  // The silent reload keeps the page up: its heading is present the whole time,
  // and the loading copy never appears.
  await expect(pane.getByRole('heading', { name: 'The Scriptorium' })).toBeVisible();
  await expect(page.getByText('Loading document stores...')).toHaveCount(0);
  await relisted;
});
