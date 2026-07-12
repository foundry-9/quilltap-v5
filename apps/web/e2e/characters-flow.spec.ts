import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  openSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { resolve } from 'node:path';

import { expect, test, type Page } from '@playwright/test';

import { makeDbKeyFile } from './support/dbkey';
import {
  ARTIFACTS_DIR,
  cliBinary,
  E2E_PASSPHRASE,
  FIXTURE_USER,
  FIXTURES_DIR,
  SINGLE_USER_ID,
  spaDir,
  TEST_PEPPER,
  webBinary,
} from './support/env';

/**
 * P4.6g — browser walk of the Characters vertical, un-skipped at the P4.6f/g
 * unification: unlock → the roster renders lane A's committed `characters-*`
 * fixture cards → toggle a favorite → open Aria's detail view → remove a tag
 * on the Tags tab → the change survives a full reload.
 *
 * The mutation beats: `characterRemoveTag` (P4.6f slice 2) over the fixture's
 * baked "Adventure" tag; the add-tag beat (`tagCreate` + `characterAddTag`,
 * P4.6f slice 4d — RESTORED when slice 4 landed) minting a brand-new tag
 * through the Tags tab's Enter-to-create path; and the edit-title→Save beat
 * (`characterUpdate`, P4.6f slice 4a — RESTORED with it) proving the write
 * through the roster card's title line after a full reload.
 *
 * Runs against its OWN locked server + instance dir (the shared global-setup
 * server is pinned to the small Salon fixture) — the recipe mirrors
 * `salon-scroll.spec.ts`: copy the fixture pair, write the locked .dbkey,
 * rewrite the fixture user id to SINGLE_USER_ID via `quilltap db --write`
 * BEFORE launch (the write-lock refuses a live holder), then boot quilltap-web
 * on a spec-private port. No LLM send happens here, so no mock LLM is needed.
 */

const CHAR_PORT = 4322;
const CHAR_BASE_URL = `http://127.0.0.1:${CHAR_PORT}`;
const CHAR_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'characters-instance');
const CHAR_DATA_DIR = resolve(CHAR_INSTANCE_DIR, 'data');
const CHAR_SERVER_LOG = resolve(ARTIFACTS_DIR, 'characters-server.log');

/** Every fixture table the walk reads is filtered by userId — rewrite them all. */
const USER_TABLES = [
  'characters',
  'chats',
  'tags',
  'api_keys',
  'connection_profiles',
  'image_profiles',
  'files',
];

let server: ChildProcess | undefined;

test.describe('P4.6g — Characters vertical (list → view → toggle → mutate)', () => {
  test.beforeAll(async () => {
    // Each `quilltap db --write` unwraps the .dbkey via PBKDF2 (≈5s); the
    // rewrites plus server boot need more than the default per-hook timeout.
    test.setTimeout(120_000);

    const web = webBinary();
    const cli = cliBinary();

    rmSync(CHAR_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(CHAR_DATA_DIR, { recursive: true });
    copyFileSync(
      resolve(FIXTURES_DIR, 'characters-main.db'),
      resolve(CHAR_DATA_DIR, 'quilltap.db'),
    );
    copyFileSync(
      resolve(FIXTURES_DIR, 'characters-mount.db'),
      resolve(CHAR_DATA_DIR, 'quilltap-mount-index.db'),
    );

    writeFileSync(
      resolve(CHAR_DATA_DIR, 'quilltap.dbkey'),
      makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE),
    );
    for (const table of USER_TABLES) {
      runCliWrite(cli, `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`);
    }

    const logFd = openSync(CHAR_SERVER_LOG, 'w');
    server = spawn(
      web,
      ['--host', '127.0.0.1', '--port', String(CHAR_PORT), '--data-dir', CHAR_INSTANCE_DIR, '--spa-dir', spaDir()],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    server.unref();
    await waitForHealth();
  });

  test.afterAll(async () => {
    if (server?.pid) {
      try {
        process.kill(-server.pid, 'SIGTERM');
      } catch {
        try {
          process.kill(server.pid, 'SIGTERM');
        } catch {
          // already gone
        }
      }
    }
    rmSync(CHAR_INSTANCE_DIR, { recursive: true, force: true });
  });

  test('unlock → roster → toggle favorite → detail → remove/add tag → edit title persists', async ({
    page,
  }) => {
    // The walk crosses three full reloads plus several mutation round-trips.
    test.setTimeout(60_000);
    await page.goto(`${CHAR_BASE_URL}/characters`);

    // Unlock (this server starts locked and is only used by this spec).
    const passphrase = page.locator('#qt-passphrase');
    await expect(passphrase).toBeVisible({ timeout: 15_000 });
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();

    // The roster renders the fixture cards. Sort order: favorites first, so
    // Aria (isFavorite, Sky-Captain) leads the grid.
    await expect(page.getByRole('heading', { name: 'Characters', exact: true })).toBeVisible();
    const cards = page.locator('.character-card-grid .character-card');
    await expect(cards.first()).toBeVisible();
    const aria = cards.filter({ hasText: 'Aria' }).first();
    await expect(aria).toContainText('Sky-Captain');

    // Toggle a favorite on a NON-Aria card (optimistic star flip both ways —
    // leave Aria's favorite alone so the detail assertions below stay stable).
    const other = cards.filter({ hasText: 'Dax' }).first();
    const star = other.getByTitle('Add to favorites');
    await star.click();
    await expect(other.getByTitle('Remove from favorites')).toBeVisible();

    // Open Aria's detail view by clicking the card BODY (the description
    // preview), not the name link — the whole card navigates in v4
    // (`handleCardClick`; dogfood finding #4).
    await aria.locator('p.line-clamp-3').click();
    await expect(page.getByRole('heading', { name: 'Aria' })).toBeVisible();
    const editLink = page.getByRole('link', { name: /Edit Character/i });
    await expect(editLink).toBeVisible();

    // The Tags tab: the fixture's baked "Adventure" chip renders
    // (characterGetTags), then removing it is a real `characterRemoveTag`
    // server round-trip.
    await page.getByRole('button', { name: 'Tags' }).click();
    const removeAdventure = page.getByLabel('Remove tag Adventure');
    await expect(removeAdventure).toBeVisible();
    await removeAdventure.click();
    await expect(removeAdventure).toBeHidden();

    // The change persists across a full reload (server state, not client
    // state — the SPA re-reads everything from the dispatch API).
    await page.reload();
    await page.getByRole('button', { name: 'Tags' }).click();
    await expect(page.getByRole('button', { name: '+ Add Tag' })).toBeVisible({ timeout: 10_000 });
    await expect(page.getByLabel('Remove tag Adventure')).toBeHidden();

    // Add a BRAND-NEW tag through the Enter-to-create path (`tagCreate` then
    // `characterAddTag` — P4.6f slice 4d). "Zeppelin" substring-matches no
    // catalog tag, so Enter takes the create branch, not a suggestion.
    await page.getByRole('button', { name: '+ Add Tag' }).click();
    const tagInput = page.getByPlaceholder('Add a tag...');
    await tagInput.fill('Zeppelin');
    await tagInput.press('Enter');
    const removeZeppelin = page.getByLabel('Remove tag Zeppelin');
    await expect(removeZeppelin).toBeVisible();

    // ...and it persists server-side across a reload.
    await page.reload();
    await page.getByRole('button', { name: 'Tags' }).click();
    await expect(page.getByLabel('Remove tag Zeppelin')).toBeVisible({ timeout: 10_000 });

    // Edit-title→Save (`characterUpdate` — P4.6f slice 4a): retitle Aria on
    // the edit screen. The "Edit Character" link renders on the DETAILS tab
    // (not the header), so switch back off the Tags tab first.
    await page.getByRole('button', { name: 'Details' }).click();
    await page.getByRole('link', { name: /Edit Character/i }).click();
    const titleInput = page.locator('#title');
    await expect(titleInput).toHaveValue('Sky-Captain');
    await titleInput.fill('Fleet Admiral');
    await page.getByRole('button', { name: 'Save Character' }).click();
    await expect(page.getByRole('heading', { name: 'Aria' })).toBeVisible({ timeout: 10_000 });

    // The new title survives a full reload of the roster (server state).
    await page.goto(`${CHAR_BASE_URL}/characters`);
    await page.reload();
    const retitled = page.locator('.character-card-grid .character-card').filter({ hasText: 'Aria' }).first();
    await expect(retitled).toContainText('Fleet Admiral', { timeout: 10_000 });
  });

  // ---------------------------------------------------------------------------
  // P4.6j live beats — activated at the P4.6i/j unification over lane A's live
  // dispatch arms (the P4.6b/g precedent). The committed fixture's Aria carries
  // the "Solo Voyage" chat and her vault avatar counts as a gallery entry.
  // Unlock-state-tolerant: the shared server is unlocked by whichever beat in
  // this file runs first, so a later beat may never see the passphrase gate.
  // ---------------------------------------------------------------------------

  async function unlockIfLocked(page: Page): Promise<void> {
    const passphrase = page.locator('#qt-passphrase');
    const roster = page.getByRole('heading', { name: 'Characters', exact: true });
    await expect(passphrase.or(roster).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.isVisible()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
    await expect(roster).toBeVisible({ timeout: 10_000 });
  }

  test(
    'Conversations tab lists a per-character chat that links into the Salon',
    async ({ page }) => {
      test.setTimeout(60_000);
      await page.goto(`${CHAR_BASE_URL}/characters`);
      await unlockIfLocked(page);

      const cards = page.locator('.character-card-grid .character-card');
      const aria = cards.filter({ hasText: 'Aria' }).first();
      await aria.locator('p.line-clamp-3').click();
      await expect(page.getByRole('heading', { name: 'Aria' })).toBeVisible();

      await page.getByRole('button', { name: 'Conversations' }).click();
      const chatCard = page.locator('a.chat-card').first();
      await expect(chatCard).toBeVisible({ timeout: 10_000 });
      await chatCard.click();
      await expect(page).toHaveURL(/\/salon\/[^/]+$/);
    },
  );

  test('delete a throwaway character via the cascade-preview dialog', async ({ page }) => {
    test.setTimeout(60_000);
    await page.goto(`${CHAR_BASE_URL}/characters`);
    await unlockIfLocked(page);

    // Create a throwaway so the fixture cast is left intact.
    await page.getByRole('link', { name: 'Create Character' }).click();
    await page.locator('#name').fill('Ephemeron');
    await page.getByRole('button', { name: /Create Character/i }).click();
    await expect(page.getByRole('heading', { name: /Edit: Ephemeron|Ephemeron/ })).toBeVisible({
      timeout: 10_000,
    });

    // Delete from the edit view's danger zone → cascade dialog → confirm.
    await page.goto(`${CHAR_BASE_URL}/characters`);
    const throwaway = page
      .locator('.character-card-grid .character-card')
      .filter({ hasText: 'Ephemeron' })
      .first();
    await expect(throwaway).toBeVisible();
    // A fresh quick-create has no description, so the card body's
    // `p.line-clamp-3` is empty (zero-height, unclickable) — click the `h2`
    // title (it sits inside the card's routerLink anchor).
    await throwaway.locator('h2').first().click();
    await page.getByRole('link', { name: /Edit Character/i }).click();
    await page.getByRole('button', { name: 'Delete Character' }).click();
    // The dialog's own confirm (destructive) — preview loads first. Scope to
    // the dialog: the edit view's danger-zone button keeps the same accname
    // and sits under the overlay.
    await page
      .locator('qt-character-delete-dialog')
      .getByRole('button', { name: 'Delete Character' })
      .click();

    await expect(page).toHaveURL(/\/characters$/);
    await expect(
      page.locator('.character-card-grid .character-card').filter({ hasText: 'Ephemeron' }),
    ).toHaveCount(0, { timeout: 10_000 });
  });

  test('Photo Gallery lists photos and removes one', async ({ page }) => {
    test.setTimeout(60_000);
    await page.goto(`${CHAR_BASE_URL}/characters`);
    await unlockIfLocked(page);

    const aria = page.locator('.character-card-grid .character-card').filter({ hasText: 'Aria' }).first();
    await aria.locator('p.line-clamp-3').click();
    await expect(page.getByRole('heading', { name: 'Aria' })).toBeVisible();

    await page.getByRole('button', { name: 'Photo Gallery' }).click();
    // Count gallery tiles by their delete affordance — a bare `img` count
    // would also catch the detail header's avatar.
    const deleteButtons = page.getByTitle('Delete this photo');
    await expect(deleteButtons.first()).toBeVisible({ timeout: 10_000 });
    const before = await deleteButtons.count();
    await deleteButtons.first().click();
    await expect(deleteButtons).toHaveCount(before - 1, { timeout: 10_000 });
  });

  // The two P4.6l riders over lane C's byte-leg web routes, live at unification.

  test('Upload Photo adds a gallery tile via the multipart route', async ({ page }) => {
    test.setTimeout(60_000);
    await page.goto(`${CHAR_BASE_URL}/characters`);
    await unlockIfLocked(page);

    const aria = page.locator('.character-card-grid .character-card').filter({ hasText: 'Aria' }).first();
    await aria.locator('p.line-clamp-3').click();
    await expect(page.getByRole('heading', { name: 'Aria' })).toBeVisible();

    await page.getByRole('button', { name: 'Photo Gallery' }).click();
    const deleteButtons = page.getByTitle('Delete this photo');
    // The earlier gallery beat may have removed Aria's only photo — wait for
    // the tab to settle on EITHER a tile or the empty state, then count.
    await expect(
      deleteButtons.first().or(page.getByText('No photos yet')),
    ).toBeVisible({ timeout: 10_000 });
    const before = await deleteButtons.count();

    // A 1×1 transparent PNG; the route validates non-empty image/* bytes and
    // dedups by sha — fresh per run since beforeAll recreates the instance.
    const tinyPng = Buffer.from(
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==',
      'base64',
    );
    await page.locator('input[aria-label="Upload photo"]').setInputFiles({
      name: 'unification-beat.png',
      mimeType: 'image/png',
      buffer: tinyPng,
    });
    await expect(deleteButtons).toHaveCount(before + 1, { timeout: 15_000 });
  });

  test('Export as SillyTavern PNG downloads a PNG card', async ({ page }, testInfo) => {
    test.setTimeout(60_000);
    await page.goto(`${CHAR_BASE_URL}/characters`);
    await unlockIfLocked(page);

    const aria = page.locator('.character-card-grid .character-card').filter({ hasText: 'Aria' }).first();
    // Glyph button — locate by title (the accname gotcha).
    const downloadPromise = page.waitForEvent('download');
    await aria.getByTitle('Export as SillyTavern PNG card').click();
    const download = await downloadPromise;
    expect(download.suggestedFilename()).toBe('Aria.png');

    const saved = testInfo.outputPath('aria-card.png');
    await download.saveAs(saved);
    const bytes = readFileSync(saved);
    // Container bytes are v4-faithful, NOT necessarily a valid PNG: Aria's
    // avatar is a WebP, and v4's createSTCharacterPNG appends the tEXt card to
    // whatever container the avatar bytes are (the clamped-offset quirk — see
    // the P4.6m unit-3 status-log note). Byte-exactness is the tier-1
    // st_png_equivalence differential's job; this beat proves the SEAM: the
    // route streams the codec output carrying the embedded ST card.
    const text = bytes.toString('latin1');
    expect(text).toContain('tEXt');
    expect(text).toContain('chara');
    expect(text).toContain('"spec":"chara_card_v2"');
  });
});

function runCliWrite(cli: string, sql: string): void {
  const res = spawnSync(cli, ['db', '--data-dir', CHAR_INSTANCE_DIR, '--write', sql], {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  if (res.status !== 0) {
    throw new Error(`CLI rewrite failed (${sql}):\n${res.stdout}\n${res.stderr}`);
  }
}

function withoutPepper(): NodeJS.ProcessEnv {
  const env = { ...process.env };
  delete env['ENCRYPTION_MASTER_PEPPER'];
  return env;
}

async function waitForHealth(): Promise<void> {
  const deadline = Date.now() + 30_000;
  let lastErr = '';
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${CHAR_BASE_URL}/health`);
      if (res.status === 423 || res.status === 200) return;
      lastErr = `health status ${res.status}`;
    } catch (e) {
      lastErr = e instanceof Error ? e.message : String(e);
    }
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(`characters server did not become ready within 30s (${lastErr}); see ${CHAR_SERVER_LOG}`);
}

// ===========================================================================
// P4.6t — the character Memories (Commonplace Book) tab. Authored
// pre-unification against lane A's NEW `memories-{main,mount}.db` fixture; the
// describe SKIPS when that fixture is absent (this worktree) and auto-activates
// once lane A lands it + the `memory*` dispatch variants at unification. Runs
// its OWN locked server + instance dir on a spec-private port (the P4.6r
// recipe). The CLI rewrite is tolerant of tables the fixture may not carry.
// ===========================================================================

const MEM_PORT = 4327;
const MEM_BASE_URL = `http://127.0.0.1:${MEM_PORT}`;
const MEM_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'memories-instance');
const MEM_DATA_DIR = resolve(MEM_INSTANCE_DIR, 'data');
const MEM_SERVER_LOG = resolve(ARTIFACTS_DIR, 'memories-server.log');
const MEM_MAIN_FIXTURE = resolve(FIXTURES_DIR, 'memories-main.db');
const MEM_MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'memories-mount.db');
const MEM_FIXTURE_READY = existsSync(MEM_MAIN_FIXTURE);
// Every user-scoped table the memory vertical touches. The embedding tables
// matter: memoryCreate runs the gate, which resolves the DEFAULT embedding
// profile for the SESSION user — left un-rewritten, the fixture's BUILTIN
// profile stays on FIXTURE_USER and every create fails with the (faithful)
// "Failed to generate embedding" server error. The rewrite loop tolerates
// tables/columns a fixture doesn't carry.
const MEM_USER_TABLES = [
  'characters',
  'memories',
  'tags',
  'chats',
  'files',
  'chat_settings',
  'connection_profiles',
  'api_keys',
  'embedding_profiles',
  'tfidf_vocabularies',
];

let memServer: ChildProcess | undefined;

test.describe('P4.6t — Memories tab (Commonplace Book)', () => {
  test.skip(!MEM_FIXTURE_READY, 'awaits lane A memories fixture (wired at unification)');

  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(MEM_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(MEM_DATA_DIR, { recursive: true });
    copyFileSync(MEM_MAIN_FIXTURE, resolve(MEM_DATA_DIR, 'quilltap.db'));
    if (existsSync(MEM_MOUNT_FIXTURE)) {
      copyFileSync(MEM_MOUNT_FIXTURE, resolve(MEM_DATA_DIR, 'quilltap-mount-index.db'));
    }
    writeFileSync(resolve(MEM_DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));
    for (const table of MEM_USER_TABLES) {
      const res = spawnSync(
        cli,
        [
          'db',
          '--data-dir',
          MEM_INSTANCE_DIR,
          '--write',
          `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`,
        ],
        {
          env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
          encoding: 'utf8',
        },
      );
      if (res.status !== 0) {
        const out = `${res.stdout}${res.stderr}`;
        if (!out.includes('no such table') && !out.includes('no such column: userId')) {
          throw new Error(`memories fixture rewrite failed (${table}):\n${res.stdout}\n${res.stderr}`);
        }
      }
    }

    const logFd = openSync(MEM_SERVER_LOG, 'w');
    memServer = spawn(
      web,
      ['--host', '127.0.0.1', '--port', String(MEM_PORT), '--data-dir', MEM_INSTANCE_DIR, '--spa-dir', spaDir()],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    memServer.unref();

    const deadline = Date.now() + 30_000;
    for (;;) {
      try {
        const res = await fetch(`${MEM_BASE_URL}/health`);
        if (res.status === 423 || res.status === 200) break;
      } catch {
        // not up yet
      }
      if (Date.now() > deadline) {
        throw new Error(`memories server not ready within 30s; see ${MEM_SERVER_LOG}`);
      }
      await new Promise((r) => setTimeout(r, 300));
    }
  });

  test.afterAll(() => {
    if (memServer?.pid) {
      try {
        process.kill(-memServer.pid, 'SIGTERM');
      } catch {
        try {
          process.kill(memServer.pid, 'SIGTERM');
        } catch {
          // already gone
        }
      }
    }
    rmSync(MEM_INSTANCE_DIR, { recursive: true, force: true });
  });

  async function unlockIfLocked(page: Page): Promise<void> {
    const passphrase = page.locator('#qt-passphrase');
    const roster = page.getByRole('heading', { name: 'Characters', exact: true });
    await expect(passphrase.or(roster).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.isVisible()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
    await expect(roster).toBeVisible({ timeout: 10_000 });
  }

  test('open a character\'s Memories tab → baked memories render → create → edit → delete', async ({
    page,
  }) => {
    test.setTimeout(60_000);
    await page.goto(`${MEM_BASE_URL}/characters`);
    await unlockIfLocked(page);

    // Open the first character card's detail view (h2 title works whether or
    // not the card carries a description body).
    const firstCard = page.locator('.character-card-grid .character-card').first();
    await expect(firstCard).toBeVisible({ timeout: 10_000 });
    await firstCard.locator('h2').first().click();

    // The Memories tab renders v4's MemoryList over the fixture. The section
    // header carries the total count — its presence proves the paginated list
    // queried the fixture (the baked memories render as cards below it, or the
    // empty state if this character has none — either way the vertical is live).
    // Scope to the tab bar: a conversation card's "Memories" count glyph shares
    // the accessible name (strict-mode collision — the P4.6pqr scoping recipe).
    await page.locator('nav.qt-tab-group').getByRole('button', { name: 'Memories' }).click();
    await expect(page.getByRole('heading', { name: /Memories \(\d+\)/ })).toBeVisible({
      timeout: 10_000,
    });

    // Create a manual memory (memoryCreate). A distinctive summary avoids
    // colliding with any baked memory.
    const summary = 'E2E Commonplace walk';
    await page.getByRole('button', { name: 'Add Memory', exact: true }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();
    // Assert each fill stuck before submitting (explicit sync points — the
    // fresh dialog's first fill was observed lost to a render race without
    // them, leaving the required field empty and the submit a silent no-op).
    await dialog.getByPlaceholder('Brief summary of this memory').fill(summary);
    await expect(dialog.getByPlaceholder('Brief summary of this memory')).toHaveValue(summary);
    await dialog.getByLabel('Memory content').fill('A note filed away by the P4.6t e2e walk.');
    await expect(dialog.getByPlaceholder('Brief summary of this memory')).toHaveValue(summary);
    await dialog.getByRole('button', { name: 'Create Memory' }).click();
    await expect(dialog).toBeHidden({ timeout: 10_000 });

    const card = page.locator('qt-memory-card', { hasText: summary });
    await expect(card).toBeVisible({ timeout: 10_000 });

    // Edit it (memoryUpdate).
    const renamed = 'E2E Commonplace walk (revised)';
    await card.getByRole('button', { name: 'Edit', exact: true }).click();
    const editDialog = page.getByRole('dialog');
    await expect(editDialog).toBeVisible();
    await editDialog.getByPlaceholder('Brief summary of this memory').fill(renamed);
    await editDialog.getByRole('button', { name: 'Save Changes' }).click();
    await expect(editDialog).toBeHidden({ timeout: 10_000 });
    const renamedCard = page.locator('qt-memory-card', { hasText: renamed });
    await expect(renamedCard).toBeVisible({ timeout: 10_000 });

    // Delete it (memoryDelete) via the inline confirm dialog.
    await renamedCard.getByRole('button', { name: 'Delete', exact: true }).click();
    const confirm = page.getByRole('dialog');
    await expect(confirm).toContainText('Are you sure you want to delete this memory?');
    await confirm.getByRole('button', { name: 'Delete', exact: true }).click();
    await expect(page.locator('qt-memory-card', { hasText: renamed })).toHaveCount(0, {
      timeout: 10_000,
    });
  });
});
