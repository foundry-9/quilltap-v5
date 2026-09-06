import { expect, request as pwRequest, test, type Page } from '@playwright/test';

import { BASE_URL, E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { startMockLlm, type MockLlm } from './support/mock-llm';
import { seedHelpFixture, type HelpSeedResult } from './support/seed-help-fixture';

/**
 * ORDERING: rides the SHARED global-setup server, so the filename must sort
 * AFTER foundation.spec.ts (workers: 1, alphabetical file order).
 * "workspace-help-ask-flow" ('w') sorts after "foundation" ('f') — and before
 * "workspace-help-guide-flow", which is fine: the two are independent, and each
 * makes the Guide/Ask tab active for itself rather than assuming it.
 *
 * P4.9I2B — a LIVE browser walk of the Help dialog's **Ask** tab, entirely
 * ACTIVATE-AT-UNIFY. In-lane none of it can run: the nine `helpChat*` dispatch
 * verbs live in the sibling server lane P4.9I2A, and the `<qt-help-entry />`
 * shell mount is UNIFIER-ONLY (§S.2). Each beat is guarded on THREE things —
 * the server answering `helpChatEligibility`, the seed having made a character
 * eligible, and the rail entry being present — and skips LOUDLY with whichever
 * one is missing.
 *
 * ## The Tier-3 deferral, MEASURED not assumed
 *
 * A `help_navigate` tool turn is NOT exercised here. `support/mock-llm.ts`
 * emits `content` deltas and a `finish_reason: stop` chunk and nothing else —
 * it has no `tool_calls` branch — so no browser walk can make the server emit a
 * `toolResult` frame named `help_navigate`. Rather than stub the assertion, the
 * navigation-link path is proven at unit tier (`help-stream.spec.ts` folds the
 * real frames; `help-dialog.spec.ts` drives the strips through the real
 * component). Teaching the mock to answer a tool call is the follow-up that
 * would activate a fifth beat here.
 *
 * Beats:
 *   1. The rail opens the dialog; the Ask tab shows the eligible seat as a pill.
 *   2. Posing a question creates a chat and streams an assistant reply.
 *   3. New conversation returns to the launcher, which lists the new chat.
 *   4. Deleting that chat removes it from the launcher.
 */

let helpChatsReady = false;
let seed: HelpSeedResult = { eligible: false, reason: 'seed did not run' };

// The send beat streams through the REAL orchestrator + provider transport, so
// this spec runs its own canned LLM on the fixture profile's rewritten baseUrl
// (the m4-salon precedent, and the lesson of the Brahma lane's first live run:
// without one, a send hangs on 30 s provider retries).
let mock: MockLlm;

test.beforeAll(async () => {
  mock = await startMockLlm(undefined, MOCK_LLM_PORT);

  const ctx = await pwRequest.newContext();
  try {
    // The shared server boots LOCKED; every dispatch below is readiness-gated,
    // so unlock over the API first (the `page-toolbar-flow` precedent) — in
    // the full suite an earlier spec has done it, in isolation nobody has
    // (the `p4.9i2` unification's first isolated run: probe ok, seed refused,
    // every beat skipped).
    await ctx.post(`${BASE_URL}/api/dispatch`, {
      data: { type: 'unlock', passphrase: E2E_PASSPHRASE },
    });
    // Probe: is `helpChatEligibility` handled? In-lane the Rust core has no
    // such variant → "unknown variant" error → not ready.
    const res = await ctx.post(`${BASE_URL}/api/dispatch`, {
      data: { type: 'helpChatEligibility' },
    });
    const body = (await res.json().catch(() => null)) as
      | { type?: string; data?: { message?: string } }
      | null;
    const isUnknownVariant =
      body?.type === 'error' && /unknown variant/i.test(String(body?.data?.message ?? ''));
    helpChatsReady = body != null && !isUnknownVariant;

    // The seed reports rather than throws — in-lane there is nothing to seed.
    seed = await seedHelpFixture(ctx, BASE_URL);
  } catch (error) {
    helpChatsReady = false;
    seed = { eligible: false, reason: `probe failed: ${String(error)}` };
  } finally {
    await ctx.dispose();
  }
});

test.afterAll(async () => {
  await mock?.close();
});

/** Unlock only when the passphrase screen is showing (the shared server stays unlocked). */
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

/** The footer rail entry (mounted by the unifier, §S.2). */
const railEntry = (page: Page) =>
  page.locator('aside.qt-left-sidebar button[aria-label="Help"]');

/** Skip the beat loudly, naming WHICH of the three preconditions is missing. */
async function guard(page: Page): Promise<boolean> {
  const entryPresent = (await railEntry(page).count()) > 0;
  const ok = helpChatsReady && seed.eligible && entryPresent;
  test.skip(
    !ok,
    `Help Ask not live in-lane (helpChatEligibility served: ${helpChatsReady}; eligible seat seeded: ${seed.eligible}${
      seed.reason ? ` — ${seed.reason}` : ''
    }; rail entry present: ${entryPresent}) — the server verbs (P4.9I2A) + the shell mount (unifier, §S.2) self-activate this beat at unification`,
  );
  return ok;
}

/** Open Help and make the Ask tab active (the tab persists in sessionStorage). */
async function openAsk(page: Page): Promise<void> {
  await railEntry(page).click();
  await expect(page.locator('qt-help-dialog .qt-dialog')).toBeVisible();
  const askTab = page.locator('qt-help-dialog .qt-tab', { hasText: 'Ask' });
  if ((await askTab.getAttribute('class'))?.includes('qt-tab-active') !== true) {
    await askTab.click();
  }
  await expect(page.locator('qt-help-dialog .qt-help-section-label').first()).toBeVisible();
}

const composer = (page: Page) => page.locator('qt-help-dialog .qt-help-composer-input');

test.describe('P4.9I2B — Help, the Ask tab', () => {
  test('the Ask tab offers the eligible help seat', async ({ page }) => {
    await openWorkspace(page);
    if (!(await guard(page))) return;

    await openAsk(page);
    await expect(page.locator('qt-help-dialog .qt-help-char-pill').first()).toBeVisible();
    // With a seat available the opening composer is live, not disabled.
    await expect(composer(page)).toBeEnabled();
  });

  test('posing a question opens a help chat and streams a reply', async ({ page }) => {
    await openWorkspace(page);
    if (!(await guard(page))) return;

    await openAsk(page);
    await composer(page).fill('How do I make a character?');
    await composer(page).press('Enter');

    // The optimistic user bubble shows immediately — and, because it lives in
    // the same array the reload replaces, exactly ONCE (dogfood #106's shape).
    const userBubbles = page.locator('qt-help-dialog .qt-help-msg-user');
    await expect(userBubbles.first()).toBeVisible();
    await expect(userBubbles).toHaveCount(1);

    await expect(page.locator('qt-help-dialog .qt-help-msg-assistant').first()).toBeVisible({
      timeout: 30_000,
    });
  });

  test('new conversation returns to a launcher listing the new chat', async ({ page }) => {
    await openWorkspace(page);
    if (!(await guard(page))) return;

    await openAsk(page);
    await composer(page).fill('A second question.');
    await composer(page).press('Enter');
    await expect(page.locator('qt-help-dialog .qt-help-msg-assistant').first()).toBeVisible({
      timeout: 30_000,
    });

    await page.locator('qt-help-dialog button[title="New help chat"]').click();
    // Back at the launcher: the seat pills and at least one recent chat.
    await expect(page.locator('qt-help-dialog .qt-help-char-pill').first()).toBeVisible();
    await expect(page.locator('qt-help-dialog .qt-help-past-chat').first()).toBeVisible({
      timeout: 10_000,
    });
  });

  test('deleting a past chat removes it from the launcher', async ({ page }) => {
    await openWorkspace(page);
    if (!(await guard(page))) return;

    await openAsk(page);
    const rows = page.locator('qt-help-dialog .qt-help-past-chat');
    const before = await rows.count();
    // The send beats seed one; if they regressed, this must go RED, not skip
    // (the §3 review of the `p4.9i2` unification).
    expect(before, 'the send beats seed a past chat for this beat').toBeGreaterThan(0);

    await rows.first().locator('button[title="Delete"]').click();
    await expect(rows).toHaveCount(before - 1, { timeout: 10_000 });
  });
});
