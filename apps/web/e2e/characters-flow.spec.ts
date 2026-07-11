import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, mkdirSync, openSync, rmSync, writeFileSync } from 'node:fs';
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

    // Open Aria's detail view.
    await aria.getByRole('link').first().click();
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
