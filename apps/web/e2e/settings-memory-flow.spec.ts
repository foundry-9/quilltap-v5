import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this rides the SHARED global-setup server and only unlocks it if the
 * gate is showing, so its filename must sort AFTER foundation.spec.ts (which
 * walks the locked→unlock gate first; workers: 1, alphabetical).
 * "settings-memory-flow" sorts after "foundation" ('se' > 'fo').
 *
 * P4.9H2B — the embedding-profiles + memory-maintenance cards (Settings →
 * Memory), walked LIVE against P4.9H2A's `embeddingProfile*` /
 * `memoryDedup*` / `conversationSummariesRegenerate*` verbs.
 *
 * ACTIVATE-AT-UNIFY: these beats need P4.9H2A's server half. Until it lands on
 * main the describe SKIPS on a NAMED constant (never a capability probe). The
 * unifier flips `P49H2A_SERVER_LANDED` to `true`.
 *
 * FLIPPED at the P4.41/42/9H2 unification (2026-08-06): the profile verbs are
 * LIVE, so the CRUD beat runs. P4.9H2A landed its tier-2 units 6+7 as LOUD
 * REFUSALS (`memoryDedup*` / `conversationSummariesRegenerate*` answer the
 * named not-available arm), so the two maintenance beats stay gated on their
 * OWN named constant below until the follow-up ports the implementations.
 *
 * The shared e2e instance has NO API keys and a mock-pointed default embedding
 * profile — so these beats create/edit/delete a NON-default BUILTIN profile
 * only, never flipping the default or triggering a reindex/refit (which would
 * unset the instance default or mint embedding jobs).
 */
const P49H2A_SERVER_LANDED = true;
/**
 * The memory-dedup + conversation-summaries implementations (P4.9H2A units 6+7,
 * landed by P4.43 — `services/memory_dedup.rs` +
 * `services/conversation_summaries_regen.rs`). Both beats below are now active.
 */
const P49H2A_MAINTENANCE_LANDED = true;

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

/** Open the Memory tab with the given card force-opened by its `?section=` anchor. */
async function openMemoryCard(page: Page, section: string) {
  await page.goto('/salon');
  await maybeUnlock(page);
  await page.goto(`/settings?tab=memory&section=${section}`);
  const card = page.locator(`#${section}`);
  await expect(card).toBeVisible({ timeout: 15_000 });
  return card;
}

/** Wait for the next successful dispatch (a server round-trip). */
function nextDispatch(page: Page) {
  return page.waitForResponse(
    (res) => res.url().includes('/api/dispatch') && res.status() === 200,
    { timeout: 30_000 },
  );
}

test.describe('P4.9H2B — the Commonplace Book management cards', () => {
  test.skip(!P49H2A_SERVER_LANDED, 'awaits P4.9H2A server verbs (flip at unification)');

  test('embedding profiles: create → listed → rename → delete a NON-default BUILTIN profile', async ({
    page,
  }) => {
    const card = await openMemoryCard(page, 'embedding-profiles');
    const NAME = 'E2E Temp Profile';
    const RENAMED = 'E2E Temp Renamed';

    // Create — BUILTIN needs no key/model and we leave it non-default.
    await card.getByRole('button', { name: 'New Profile' }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();
    await dialog.locator('select').first().selectOption('BUILTIN');
    await dialog.locator('input[type="text"]').first().fill(NAME);
    let saved = nextDispatch(page);
    await dialog.getByRole('button', { name: 'Create Profile' }).click();
    await saved;
    await expect(dialog).toBeHidden({ timeout: 10_000 });

    const profileCard = page.locator('.qt-card', { hasText: NAME });
    await expect(profileCard).toBeVisible({ timeout: 10_000 });

    // Edit → rename.
    await profileCard.getByRole('button', { name: 'Edit', exact: true }).click();
    const editDialog = page.getByRole('dialog');
    await expect(editDialog).toBeVisible();
    await editDialog.locator('input[type="text"]').first().fill(RENAMED);
    saved = nextDispatch(page);
    await editDialog.getByRole('button', { name: 'Update Profile' }).click();
    await saved;
    await expect(editDialog).toBeHidden({ timeout: 10_000 });
    const renamed = page.locator('.qt-card', { hasText: RENAMED });
    await expect(renamed).toBeVisible({ timeout: 10_000 });

    // Delete (two-step inline confirm) — leaves the shared instance as found.
    await renamed.getByRole('button', { name: 'Delete', exact: true }).click();
    const removed = nextDispatch(page);
    await renamed.getByRole('button', { name: 'Delete this profile?' }).click();
    await removed;
    await expect(page.locator('.qt-card', { hasText: RENAMED })).toHaveCount(0, { timeout: 10_000 });
  });

  // P4.D95 (v4 `870a57fa`): the third Recall Relevance toggle. Walked LIVE
  // against the `memoryRecallConfigSet` merge-patch — toggled on, read back
  // across a full reload (so the value came from the instance settings row, not
  // the card's local echo), then toggled off so the shared instance is left as
  // found.
  test('recall relevance: the per-turn conversation-summary toggle round-trips', async ({
    page,
  }) => {
    const card = await openMemoryCard(page, 'memory-recall');
    const toggle = card.getByRole('checkbox').nth(1);
    await expect(card.getByText('Consult past conversations every turn')).toBeVisible();
    await expect(toggle).not.toBeChecked();

    const saved = nextDispatch(page);
    await toggle.check();
    await saved;
    await expect(page.getByText('Recall settings saved')).toBeVisible({ timeout: 10_000 });

    // Reload: the checked state must come back from the server.
    const reloaded = await openMemoryCard(page, 'memory-recall');
    const reloadedToggle = reloaded.getByRole('checkbox').nth(1);
    await expect(reloadedToggle).toBeChecked({ timeout: 15_000 });
    // The sibling toggle is untouched — the patch is per-field.
    await expect(reloaded.getByRole('checkbox').first()).not.toBeChecked();

    // Put it back.
    const cleared = nextDispatch(page);
    await reloadedToggle.uncheck();
    await cleared;
    await expect(reloadedToggle).not.toBeChecked({ timeout: 10_000 });
  });

  test('memory deduplication: preview renders and Run is disabled at zero removable', async ({
    page,
  }) => {
    test.skip(
      !P49H2A_MAINTENANCE_LANDED,
      'memoryDedupPreview refuses loudly (P4.9H2A units 6+7 deferred) — flip P49H2A_MAINTENANCE_LANDED when the dedup port lands',
    );
    const card = await openMemoryCard(page, 'memory-deduplication');
    const analyzed = nextDispatch(page);
    await card.getByRole('button', { name: 'Analyze Memories' }).click();
    await analyzed;

    // The dialog opens; the seeded memories have no near-duplicates, so Run is
    // disabled and the zero-state copy shows.
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();
    await expect(dialog.getByText('No duplicate memories found at this threshold')).toBeVisible({
      timeout: 15_000,
    });
    await expect(dialog.getByRole('button', { name: 'Run Deduplication' })).toBeDisabled();
    await dialog.getByRole('button', { name: 'Cancel' }).click();
  });

  test('conversation summaries: a single click enqueues a regeneration', async ({ page }) => {
    test.skip(
      !P49H2A_MAINTENANCE_LANDED,
      'conversationSummariesRegenerate refuses loudly (P4.9H2A units 6+7 deferred) — flip P49H2A_MAINTENANCE_LANDED when the summaries port lands',
    );
    const card = await openMemoryCard(page, 'conversation-summaries-regenerate');
    const enqueued = nextDispatch(page);
    // `exact` so the action button doesn't also match the collapsible card
    // header, whose title text is "Regenerate Conversation Summaries …".
    await card
      .getByRole('button', { name: 'Regenerate conversation summaries', exact: true })
      .click();
    await enqueued;
    // The card re-reads the in-flight status; the button returns to its resting
    // label (proof the enqueue resolved, not the job).
    await expect(
      card.getByRole('button', { name: 'Regenerate conversation summaries', exact: true }),
    ).toBeEnabled({ timeout: 15_000 });
  });
});
