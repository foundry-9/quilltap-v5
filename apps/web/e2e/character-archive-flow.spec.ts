import { expect, test, type Page } from '@playwright/test';

import { E2E_PASSPHRASE } from './support/env';
import { openSidebarSection } from './support/sidebar';
import {
  ARCHIVED_AT,
  ARCHIVED_CHARACTER_NAME,
  ARCHIVED_CHAT_TITLE,
  ARCHIVED_GROUP_NAME,
} from './support/seed-archived-character';

/**
 * P4.D64 — the character-archive client surface (v4 `d553f72a`).
 *
 * The beats split in two, because the feature lands over two rounds:
 *
 *  - **Tombstone READS** need only an archived ROW, which `global-setup` seeds
 *    (see `support/seed-archived-character.ts`). They activate the moment
 *    P4.D63's `characters.archivedAt` column exists — the seeder probes for it
 *    and writes nothing without it, so these beats are gated on
 *    {@link ARCHIVE_TOMBSTONE_SEEDED}, which THIS ROUND'S UNIFIER flips.
 *  - **ACTIONS** (archive / rehydrate) need the archive SERVICE, which is
 *    ROUND 2. They are gated on {@link CHARACTER_ARCHIVE_SERVER_LANDED} and stay
 *    skipped through this round's unification by design.
 *
 * Both are NAMED constants rather than capability probes, deliberately: a probe
 * cannot tell a verb that is DEFINED-but-refusing from one that works, and would
 * silently activate a beat into guaranteed failure (the standing e2e rule).
 */

/**
 * ACTIVATE-AT-UNIFY behind {@link ARCHIVE_TOMBSTONE_SEEDED}. Flip to `true` at
 * this round's unification, once P4.D63's schema is on the branch — the global
 * setup logs `seeded the archived-character island` when the seeder ran, which
 * is the confirmation to look for.
 */
const ARCHIVE_TOMBSTONE_SEEDED = true;

/**
 * Flipped TRUE at the round-2 unification (P4.D65 ∥ P4.D66 ∥ P4.D67): the
 * archive service and both verbs are LIVE. Beats 3–4 read the bundle beats
 * 1–2 leave on the shelf, which is safe because the suite runs
 * `fullyParallel: false, workers: 1` — within-file order is guaranteed.
 */
const CHARACTER_ARCHIVE_SERVER_LANDED = true;

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

async function openRoster(page: Page): Promise<void> {
  await page.goto('/salon');
  await maybeUnlock(page);
  await page.goto('/characters');
  await expect(page.getByRole('heading', { name: 'Characters', exact: true })).toBeVisible({
    timeout: 15_000,
  });
}

test.describe('P4.D64 — the archive tombstone, read-only', () => {
  test('the roster hides archived characters until Show Archived is pressed', async ({ page }) => {
    test.skip(!ARCHIVE_TOMBSTONE_SEEDED, 'awaits P4.D63 schema + the unifier seeding the tombstone');
    await openRoster(page);

    const cards = page.locator('qt-character-card');
    const tomb = cards.filter({ hasText: ARCHIVED_CHARACTER_NAME });
    // Excluded by default — v4's list chokepoint, which every picker inherits.
    await expect(tomb).toHaveCount(0);

    const toggle = page.getByRole('button', { name: 'Show Archived' });
    await expect(toggle).toHaveAttribute('title', 'Show characters resting in the archive');
    await toggle.click();

    await expect(page.getByRole('button', { name: 'Hide Archived' })).toHaveAttribute(
      'title',
      'Tuck the archived characters back out of sight',
    );
    await expect(tomb).toHaveCount(1, { timeout: 15_000 });

    // The badge, dated; the tooltip is locale-formatted, so assert the prefix.
    const badge = tomb.first().locator('.qt-badge', { hasText: 'Archived' });
    await expect(badge).toBeVisible();
    await expect(badge).toHaveAttribute('title', /^Resting in the archive since /);

    // Archived last on the shelf — rule 0 outranks every other ordering key.
    // Case-insensitively: the aa-foundation walk applies a bundled theme whose
    // `text-transform: uppercase` reaches innerText (the P4.43 lesson), so a
    // full-suite run reads "MARCHPANE" where isolation reads "Marchpane".
    const names = await cards.locator('h2').allInnerTexts();
    expect(names[names.length - 1].toUpperCase()).toContain(
      ARCHIVED_CHARACTER_NAME.toUpperCase(),
    );

    // The card offers no chat and no export — one inert note instead.
    const actions = tomb.first().locator('.character-card-actions');
    await expect(actions.getByText('Resting in the archive')).toBeVisible();
    await expect(actions.locator('.character-card__action--chat')).toHaveCount(0);
    await expect(actions.locator('qt-icon[name="download"]')).toHaveCount(0);
    await expect(actions.locator('qt-icon[name="image"]')).toHaveCount(0);

    // Hiding them again puts the shelf back.
    await page.getByRole('button', { name: 'Hide Archived' }).click();
    await expect(tomb).toHaveCount(0, { timeout: 15_000 });
  });

  test("an archived character's page reads but does not write", async ({ page }) => {
    test.skip(!ARCHIVE_TOMBSTONE_SEEDED, 'awaits P4.D63 schema + the unifier seeding the tombstone');
    await openRoster(page);
    await page.getByRole('button', { name: 'Show Archived' }).click();
    const tomb = page.locator('qt-character-card').filter({ hasText: ARCHIVED_CHARACTER_NAME });
    await expect(tomb).toHaveCount(1, { timeout: 15_000 });
    await tomb.first().locator('h2').click();

    // The banner, then the read-only page.
    await expect(page.getByText(`${ARCHIVED_CHARACTER_NAME} rests in the archive.`)).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByText('packed into a sealed bundle on the shelf')).toBeVisible();

    // Rehydrate is PRESENT (this beat does not press it — that is round 2's).
    const rehydrate = page.getByRole('button', { name: 'Rehydrate' });
    await expect(rehydrate).toBeVisible();
    await expect(rehydrate).toHaveAttribute(
      'title',
      'Restore this character from their archive bundle and wake them',
    );

    // And the live cluster is gone, along with the edit door.
    await expect(page.getByRole('button', { name: 'Archive', exact: true })).toHaveCount(0);
    await expect(page.getByRole('link', { name: 'Edit Character' })).toHaveCount(0);
    await expect(page.getByText('Convert to NPC')).toHaveCount(0);

    // Every tab still renders — inside a disabled fieldset.
    const fieldset = page.locator('fieldset[disabled]');
    await expect(fieldset).toBeVisible();
    await expect(fieldset.locator('qt-character-details-tab')).toBeVisible();
  });

  test('a live character offers Archive at the end of the action column', async ({ page }) => {
    test.skip(!ARCHIVE_TOMBSTONE_SEEDED, 'awaits P4.D63 schema (the header fork needs archivedAt)');
    await openRoster(page);
    const live = page.locator('qt-character-card').filter({ hasText: 'Aria' }).first();
    await live.locator('h2').click();
    await expect(page.getByRole('heading', { name: 'Aria', level: 1 })).toBeVisible({
      timeout: 15_000,
    });
    const archive = page.getByRole('button', { name: 'Archive', exact: true });
    await expect(archive).toBeVisible();
    await expect(archive).toHaveAttribute(
      'title',
      "Pack this character's effects into a sealed bundle and set them resting in the archive",
    );
    await expect(page.getByRole('button', { name: 'Rehydrate' })).toHaveCount(0);
  });

  test('a group counts who can still speak, and badges the member who cannot', async ({ page }) => {
    test.skip(!ARCHIVE_TOMBSTONE_SEEDED, 'awaits P4.D63 schema + the unifier seeding the tombstone');
    await openRoster(page);
    const card = page.locator('qt-group-card').filter({ hasText: ARCHIVED_GROUP_NAME });
    await expect(card).toHaveCount(1, { timeout: 15_000 });
    // Open by clicking the CARD's title: the whole card routes to the editor
    // in both shells, where the Edit affordance's element kind differs by
    // hosting mode and proved run-order-flaky (gesture fixed twice at the
    // round-1 unification — the card click is the deterministic door).
    await card.first().scrollIntoViewIfNeeded();
    await card.first().getByText(ARCHIVED_GROUP_NAME, { exact: true }).click();
    await expect(page.locator('#qt-group-name')).toBeVisible({ timeout: 15_000 });

    // The subtitle names the split. Membership survives archiving.
    await expect(page.getByText(/2 members \/ 1 can speak \(1 archived\)/)).toBeVisible();

    // Expand the Members card for the per-member badge.
    await page.locator('qt-group-members-card .qt-collapsible-card-header').click();
    const row = page.locator('qt-group-members-card').getByText(ARCHIVED_CHARACTER_NAME);
    await expect(row).toBeVisible();
    await expect(
      page.locator('qt-group-members-card span', { hasText: 'Archived' }).first(),
    ).toHaveAttribute(
      'title',
      'Resting in the archive — still a member, but takes no turns until rehydrated',
    );
  });

  // ⚠ V4 BUG, REPRODUCED FAITHFULLY (found by this beat's first live run at
  // the round-1 unification, 2026-08-11): v4's `01e481f6` added `archivedAt`
  // to `chats/[id]/helpers.ts getEnrichedCharacter` and taught
  // `ParticipantCard` to badge on `participant.character?.archivedAt` — but
  // the chat GET the sidebar renders from enriches through a DIFFERENT
  // projection (`chat-enrichment.service.ts getCharacterDetail`), which v4
  // never gave the key; the helpers enrichment serves only the participants
  // `?action=` replies and the chat PUT, and v4's client refetches (the GET)
  // rather than rendering those replies. So on a FRESH LOAD the Archived seat
  // badge cannot light in v4 either. v5 mirrors both projections faithfully
  // (`chat_enrichment.rs` without the key, `chat_participants.rs` with it).
  // This beat pins the v4-faithful fresh-load state: Absent shows, Archived
  // does NOT. When v4 fixes its GET projection (a v4-first item — recorded in
  // the round record for the human to file), the drift round that absorbs it
  // flips this beat's tail to the two-badge assertion.
  test('an archived seat is badged Absent; the Archived badge awaits a v4-side GET fix', async ({ page }) => {
    test.skip(!ARCHIVE_TOMBSTONE_SEEDED, 'awaits P4.D63 schema + the unifier seeding the tombstone');
    await page.goto('/salon');
    await maybeUnlock(page);
    // Scope to the LIST'S card (the m4-salon pattern): a bare getByText can
    // resolve a hidden duplicate (keep-alive workspace panes), and the list is
    // virtualized (dogfood #3b) — the seeder floats this chat to the top by
    // recency so the card is in the render window. Gesture fixed at the
    // round-1 unification (the beat's first live runs).
    const chatCard = page
      .locator('.chat-card-stack a.qt-entity-card', { hasText: ARCHIVED_CHAT_TITLE })
      .first();
    await chatCard.scrollIntoViewIfNeeded();
    await chatCard.click();
    // The participant cards live in the sidebar's Participants section, which
    // is closed until opened (the sibling beats' shared gesture — missed on
    // this beat's first live run at the round-1 unification).
    await openSidebarSection(page, 'Participants');
    await expect(page.locator('qt-participant-card').first()).toBeVisible({ timeout: 15_000 });

    const seat = page.locator('qt-participant-card').filter({ hasText: ARCHIVED_CHARACTER_NAME });
    await expect(seat).toHaveCount(1);
    const badges = seat.first().locator('.qt-badge-absent');
    // Fresh-load = ONE badge (see the header note): the flip is server-side —
    // the GET's enrichment lacks `archivedAt` in v4 too. The card's own
    // two-badge rendering (order, title, coexistence) is pinned at unit level
    // in `participant-card` specs, where the key is supplied.
    await expect(badges).toHaveCount(1);
    await expect(badges.nth(0)).toHaveText('Absent');
    await expect(seat.first().getByText('Archived', { exact: true })).toHaveCount(0);
  });

  test('the seeded stamp is the one the badge dates from', async ({ page }) => {
    test.skip(!ARCHIVE_TOMBSTONE_SEEDED, 'awaits P4.D63 schema + the unifier seeding the tombstone');
    // A cheap guard on the seeder itself: if the stamp ever drifts, the badge
    // tooltip stops matching the fixture and this says so directly rather than
    // through a confusing date mismatch in another beat.
    await openRoster(page);
    await page.getByRole('button', { name: 'Show Archived' }).click();
    const badge = page
      .locator('qt-character-card')
      .filter({ hasText: ARCHIVED_CHARACTER_NAME })
      .locator('.qt-badge', { hasText: 'Archived' });
    await expect(badge).toBeVisible({ timeout: 15_000 });
    const title = await badge.getAttribute('title');
    expect(title).toContain(String(new Date(ARCHIVED_AT).getFullYear()));
  });
});

test.describe('P4.D64 — the archive ACTIONS (round 2)', () => {
  test('the archive dialog itemizes what goes and what stays, then archives', async ({ page }) => {
    test.skip(
      !CHARACTER_ARCHIVE_SERVER_LANDED,
      'the archive service is ROUND 2 of the character-archive catch-up',
    );
    await openRoster(page);
    const live = page.locator('qt-character-card').filter({ hasText: 'Dax' }).first();
    await live.locator('h2').click();
    await page.getByRole('button', { name: 'Archive', exact: true }).click();

    const dialog = page.locator('qt-archive-character-dialog');
    await expect(dialog.getByText('Set Dax resting in the archive?')).toBeVisible();
    await expect(dialog.getByText('Packed into the bundle and cleared away:')).toBeVisible();
    await expect(dialog.getByText('Their memories (the Commonplace Book falls silent)')).toBeVisible();
    await expect(dialog.getByText('Kept in place, exactly as it stands:')).toBeVisible();
    await expect(dialog.getByText('Their portrait, so old conversations keep their face')).toBeVisible();
    await expect(dialog.getByText('files/<id>/character-archive.qtap')).toBeVisible();

    await dialog.getByRole('button', { name: 'Archive', exact: true }).click();
    await expect(
      page.getByText('Dax rests in the archive, bundle sealed and shelved.'),
    ).toBeVisible({ timeout: 30_000 });
    // The page has become the tombstone.
    await expect(page.getByText('Dax rests in the archive.')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Rehydrate' })).toBeVisible();
  });

  test('rehydrating wakes them and offers the leftover bundle', async ({ page }) => {
    test.skip(
      !CHARACTER_ARCHIVE_SERVER_LANDED,
      'the archive service is ROUND 2 of the character-archive catch-up',
    );
    await openRoster(page);
    await page.getByRole('button', { name: 'Show Archived' }).click();
    const tomb = page.locator('qt-character-card').filter({ hasText: 'Dax' }).first();
    await tomb.locator('h2').click();
    await page.getByRole('button', { name: 'Rehydrate' }).click();

    await expect(page.getByText(/Dax is awake again/)).toBeVisible({ timeout: 30_000 });
    const dialog = page.locator('qt-rehydrate-bundle-dialog');
    await expect(dialog.getByText('The empty bundle remains on the shelf')).toBeVisible();
    // The polarity: discarding is the SECONDARY arm, keeping the primary.
    await expect(dialog.getByRole('button', { name: 'Discard the Bundle' })).toBeVisible();
    await dialog.getByRole('button', { name: 'Keep It' }).click();
    await expect(dialog).toHaveCount(0);
    // Awake: the live cluster is back.
    await expect(page.getByRole('button', { name: 'Archive', exact: true })).toBeVisible();
  });

  test('Delete All Data offers to leave the bundles on the shelf', async ({ page }) => {
    test.skip(
      !CHARACTER_ARCHIVE_SERVER_LANDED,
      'the ARCHIVE-category files rows arrive with round 2',
    );
    await page.goto('/salon');
    await maybeUnlock(page);
    // This spec runs the WORKSPACE shell (base @playwright/test, no legacy
    // opt-out), where the hosted settings page ignores `?section=` exactly as
    // v4 does — so expand the collapsible by its header, as a user would,
    // instead of relying on the deep link (the legacy-mode
    // zz-delete-all-destructive spec keeps the `section=` path covered).
    await page.goto('/settings?tab=system');
    await page
      .locator('qt-collapsible-card', { hasText: 'Permanently delete all application data' })
      .getByRole('button')
      .first()
      .click();
    await page
      .locator('qt-delete-data-card')
      .getByRole('button', { name: 'Delete All Data', exact: true })
      .click();
    await expect(page.getByText('Archived Character Bundles')).toBeVisible({ timeout: 15_000 });
    await expect(page.getByText(/Leave the archived-character bundles on the shelf \(\d+ files?\)/)).toBeVisible();
    // Read-only: this beat does not press Delete Everything. The destructive
    // walk is `zz-delete-all-destructive.spec.ts` and stays the only one.
  });

  test('changing the passphrase reports the bundles it rewrote', async ({ page }) => {
    test.skip(
      !CHARACTER_ARCHIVE_SERVER_LANDED,
      'the re-encrypt sweep needs round 2 bundles to rewrite',
    );
    await page.goto('/salon');
    await maybeUnlock(page);
    // Workspace shell: expand the card by its header (see the note in the
    // delete-all beat above).
    await page.goto('/settings?tab=system');
    await page
      .locator('qt-collapsible-card', {
        hasText: 'Change or remove the passphrase protecting your encryption key',
      })
      .getByRole('button')
      .first()
      .click();
    await expect(
      page.getByText(/archived-character bundles? (is|are) sealed under this passphrase/),
    ).toBeVisible({ timeout: 15_000 });
  });
});
