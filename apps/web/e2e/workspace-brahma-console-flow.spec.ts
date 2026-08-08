import { expect, request as pwRequest, test, type Page } from '@playwright/test';

import { BASE_URL, E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { startMockLlm, MOCK_LLM_REPLY, type MockLlm } from './support/mock-llm';

/**
 * ORDERING: rides the SHARED global-setup server, so the filename must sort
 * AFTER foundation.spec.ts (workers: 1, alphabetical file order) — foundation
 * asserts a LOCKED-first server, and every beat here unlocks (`openWorkspace`)
 * before its guard, so it must never run before foundation. "workspace-brahma-
 * console-flow" ('w') sorts after "foundation" ('f'). (It also sorts before
 * lane J3's "workspace-flow" — both ride the shared server with the flag ON and
 * self-unlock idempotently.)
 *
 * P4.9I1B — a LIVE browser walk of the Brahma Console, entirely
 * ACTIVATE-AT-UNIFY. In-lane NONE of this can run: the eight `brahmaConsole*`
 * dispatch verbs live in the sibling server lane P4.9I1A, and the rail-entry
 * shell mount + the `brahma` tab-registry swap are UNIFIER-ONLY (§W.2). So each
 * beat is guarded on BOTH (a) the server answering `brahmaConsoleList` and (b)
 * the rail entry being present, and skips LOUDLY otherwise. It self-activates
 * the moment the server verbs merge and the unifier mounts the entry + swaps the
 * registry — no unifier wire needed beyond that.
 *
 * These beats import the BASE `@playwright/test` (not `./support/fixtures`), so
 * the workspace-tabs flag stays ON (v5's default post-login surface) and the
 * rail opens a `brahma` TAB rather than the floating dialog.
 *
 * Beats:
 *   1. Rail → tab: the footer's Brahma Console entry opens a `brahma` tab whose
 *      view renders the launcher (the opening composer).
 *   2. Send: posing a question opens a chat and streams an assistant reply
 *      (the e2e canned provider). Depends on a resolvable default connection
 *      profile in the fixture (see the AT-UNIFY table in the order).
 *   3. New conversation → the past-chats launcher lists the just-created chat.
 *   4. Set-model round-trip: the header picker switches the engine without error.
 *   5. Delete a past chat from the launcher.
 */

let brahmaReady = false;

// The send beats stream through the REAL orchestrator + provider transport, so
// this spec runs its own canned LLM on the fixture profile's rewritten baseUrl
// — every sending spec does (the m4-salon precedent). The gate's first live run
// caught the omission: sends only succeeded when a SIBLING spec's mock happened
// to linger, and hung (30 s provider retries) when none listened.
let mock: MockLlm;
test.beforeAll(async () => {
  mock = await startMockLlm(MOCK_LLM_REPLY, MOCK_LLM_PORT);
});
test.afterAll(async () => {
  await mock?.close();
});

test.beforeAll(async () => {
  // Probe: is `brahmaConsoleList` handled? In-lane the Rust core has no such
  // variant → "unknown variant" error → not ready. A success envelope (or any
  // domain error) means the handler exists → ready.
  try {
    const ctx = await pwRequest.newContext();
    const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: { type: 'brahmaConsoleList' } });
    const body = (await res.json().catch(() => null)) as
      | { type?: string; data?: { message?: string } }
      | null;
    await ctx.dispose();
    const isUnknownVariant =
      body?.type === 'error' && /unknown variant/i.test(String(body?.data?.message ?? ''));
    brahmaReady = body != null && !isUnknownVariant;
  } catch {
    brahmaReady = false;
  }
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

/** The footer rail entry (mounted by the unifier). */
const railEntry = (page: Page) =>
  page.locator('aside.qt-left-sidebar button[aria-label="Brahma Console"]');

/** Skip the beat loudly unless the whole surface is present. */
async function guard(page: Page): Promise<boolean> {
  const present = brahmaReady && (await railEntry(page).count()) > 0;
  test.skip(
    !present,
    'Brahma Console not live in-lane — the server verbs (P4.9I1A) + the rail/registry mounts (unifier) self-activate this beat at unification',
  );
  return present;
}

test.describe('P4.9I1B — the Brahma Console', () => {
  test('the rail opens a Brahma tab that renders the console launcher', async ({ page }) => {
    await openWorkspace(page);
    if (!(await guard(page))) return;

    await railEntry(page).click();
    // A brahma tab opens (its meta title is "Brahma Console").
    await expect(page.locator('.qt-tab-strip .qt-tab-label', { hasText: 'Brahma Console' })).toBeVisible();
    // The console view renders its launcher composer.
    await expect(page.locator('qt-brahma-console-view qt-help-composer textarea')).toBeVisible();
  });

  test('posing a question opens a chat and streams an assistant reply', async ({ page }) => {
    await openWorkspace(page);
    if (!(await guard(page))) return;

    await railEntry(page).click();
    const composer = page.locator('qt-brahma-console-view qt-help-composer textarea');
    await expect(composer).toBeVisible();
    await composer.fill('What is the answer?');
    await composer.press('Enter');

    // The operator's message shows, then an assistant bubble lands.
    await expect(page.getByText('What is the answer?', { exact: false })).toBeVisible();
    await expect(page.locator('qt-brahma-console-view .qt-help-msg-assistant').first()).toBeVisible({
      timeout: 30_000,
    });

    // Dogfood #74: with a chat open, the header (model picker + New
    // conversation) is present AND stays put — the transcript scrolls WITHIN
    // the message list, not by pushing the whole tab. The pre-fix bug was a
    // `display: block` message-list host with no bounded height, so the scroll
    // lived on the tab and the header rode off. Assert the header is visible and
    // the message-list host computes as a bounded flex child.
    const header = page.locator('qt-brahma-console-view [title="New conversation"]');
    await expect(header).toBeVisible();
    const listHost = page.locator('qt-brahma-console-message-list').first();
    const box = await listHost.evaluate((el) => {
      const s = getComputedStyle(el);
      return { display: s.display, minHeight: s.minHeight };
    });
    expect(box.display).toBe('flex');
    expect(box.minHeight).toBe('0px');
  });

  test('new conversation returns to the launcher listing the just-created chat', async ({
    page,
  }) => {
    await openWorkspace(page);
    if (!(await guard(page))) return;

    await railEntry(page).click();
    const composer = page.locator('qt-brahma-console-view qt-help-composer textarea');
    await composer.fill('A first audience.');
    await composer.press('Enter');
    await expect(page.locator('qt-brahma-console-view .qt-help-msg-assistant').first()).toBeVisible({
      timeout: 30_000,
    });

    // "New conversation" resets to the launcher.
    await page.locator('qt-brahma-console-view [title="New conversation"]').click();
    await expect(page.getByText('Recent Console Conversations')).toBeVisible();
    await expect(page.locator('.qt-help-past-chat').first()).toBeVisible();
  });

  test('the header model picker switches the engine without error', async ({ page }) => {
    await openWorkspace(page);
    if (!(await guard(page))) return;

    await railEntry(page).click();
    const composer = page.locator('qt-brahma-console-view qt-help-composer textarea');
    await composer.fill('Engine, are you there?');
    await composer.press('Enter');
    await expect(page.locator('qt-brahma-console-view .qt-help-msg-assistant').first()).toBeVisible({
      timeout: 30_000,
    });

    // Open the picker; if there is more than one profile, switching must not error.
    const picker = page.locator('qt-brahma-model-picker button').first();
    await picker.click();
    const options = page.locator('qt-brahma-model-picker [role="option"]');
    if ((await options.count()) > 1) {
      await options.nth(1).click();
      // No error banner; the conversation persists.
      await expect(page.locator('qt-brahma-console-view .qt-help-error')).toHaveCount(0);
    }
  });

  test('a past chat can be deleted from the launcher', async ({ page }) => {
    await openWorkspace(page);
    if (!(await guard(page))) return;

    await railEntry(page).click();
    const composer = page.locator('qt-brahma-console-view qt-help-composer textarea');
    await composer.fill('A chat to be dismissed.');
    await composer.press('Enter');
    await expect(page.locator('qt-brahma-console-view .qt-help-msg-assistant').first()).toBeVisible({
      timeout: 30_000,
    });

    await page.locator('qt-brahma-console-view [title="New conversation"]').click();
    await expect(page.getByText('Recent Console Conversations')).toBeVisible();

    const rows = page.locator('.qt-help-past-chat');
    const before = await rows.count();
    expect(before).toBeGreaterThan(0);
    await rows.first().hover();
    await rows.first().locator('[title="Delete"]').click();
    await expect(rows).toHaveCount(before - 1);
  });
});
