import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, openSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { expect, test, type Locator, type Page } from './support/fixtures';

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
 * P4.6l — browser walk of the Projects (Prospero) vertical. Authored
 * pre-unification against lane A's committed groups-projects fixture; the whole
 * describe SKIPS when that fixture is absent (this worktree) and auto-activates
 * once lane A lands it at unification. The unifier reconciles the exact fixture
 * file names if lane A differs from the `groups-projects-{main,mount}.db` guess.
 *
 * Beats: enable the Projects nav → `/prospero` renders lane A's fixture project
 * cards → open a project's detail card grid → toggle Allow Any Character (an
 * immediate `projectUpdate`) → edit the title + Save → the rename survives a
 * reload. Plus a create-project beat through the dialog.
 */

const PROJ_PORT = 4325;
const PROJ_BASE_URL = `http://127.0.0.1:${PROJ_PORT}`;
const PROJ_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'projects-instance');
const PROJ_DATA_DIR = resolve(PROJ_INSTANCE_DIR, 'data');
const PROJ_SERVER_LOG = resolve(ARTIFACTS_DIR, 'projects-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'groups-projects-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'groups-projects-mount.db');
/** Lane A owns the fixture; skip the live walk until it is committed. */
const FIXTURE_READY = existsSync(MAIN_FIXTURE);

/** Every fixture table the projects walk reads is filtered by userId. */
const USER_TABLES = ['characters', 'chats', 'tags', 'groups', 'projects', 'files'];

let server: ChildProcess | undefined;

test.describe('P4.6l — Projects vertical (list → detail → toggle → rename → persist)', () => {
  test.skip(!FIXTURE_READY, 'awaits lane A groups-projects fixture (wired at unification)');

  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(PROJ_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(PROJ_DATA_DIR, { recursive: true });
    copyFileSync(MAIN_FIXTURE, resolve(PROJ_DATA_DIR, 'quilltap.db'));
    if (existsSync(MOUNT_FIXTURE)) {
      copyFileSync(MOUNT_FIXTURE, resolve(PROJ_DATA_DIR, 'quilltap-mount-index.db'));
    }

    writeFileSync(
      resolve(PROJ_DATA_DIR, 'quilltap.dbkey'),
      makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE),
    );
    for (const table of USER_TABLES) {
      runCliWrite(
        cli,
        `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`,
      );
    }

    const logFd = openSync(PROJ_SERVER_LOG, 'w');
    server = spawn(
      web,
      [
        '--host',
        '127.0.0.1',
        '--port',
        String(PROJ_PORT),
        '--data-dir',
        PROJ_INSTANCE_DIR,
        '--spa-dir',
        spaDir(),
      ],
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
    rmSync(PROJ_INSTANCE_DIR, { recursive: true, force: true });
  });

  async function unlockIfLocked(page: Page, ready?: Locator): Promise<void> {
    const passphrase = page.locator('#qt-passphrase');
    // The default ready signal is the Projects LIST heading; a beat that
    // reloads on a DETAIL page passes its own (the settings-flow idiom).
    const readySignal = ready ?? page.getByRole('heading', { name: 'Projects', exact: true });
    await expect(passphrase.or(readySignal).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.isVisible()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
    await expect(readySignal).toBeVisible({ timeout: 10_000 });
  }

  test('the Projects list opens a detail, toggles Allow Any Character, renames, persists', async ({
    page,
  }) => {
    test.setTimeout(60_000);
    await page.goto(`${PROJ_BASE_URL}/prospero`);
    await unlockIfLocked(page);

    const projectCards = page.locator('qt-project-card');
    await expect(projectCards.first()).toBeVisible({ timeout: 10_000 });

    // Open the first project's detail via its Open link.
    await projectCards.first().getByRole('link', { name: 'Open' }).click();
    await expect(page).toHaveURL(/\/prospero\/[^/]+$/);
    // `exact` — the Image Generation card's "Announce Lantern Images to
    // Characters" heading also matches a loose name filter.
    await expect(page.getByRole('heading', { name: 'Characters', exact: true })).toBeVisible({
      timeout: 10_000,
    });

    // Toggle Allow Any Character (an immediate projectUpdate).
    const toggle = page.getByRole('switch', { name: 'Allow Any Character' });
    const before = await toggle.getAttribute('aria-checked');
    await toggle.click();
    await expect(toggle).toHaveAttribute('aria-checked', before === 'true' ? 'false' : 'true');

    // Edit the title on the header + Save. Scope to the header — the P4.6o
    // wardrobe rows carry their own "Edit" buttons (strict-mode collision).
    await page
      .locator('qt-project-header')
      .getByRole('button', { name: 'Edit', exact: true })
      .click();
    const nameInput = page.getByRole('textbox', { name: 'Project name' });
    await nameInput.fill('Renamed by the walk');
    // Scope to the header — the Settings card and the two aesthetic fields
    // (commit-4 cards) carry their own Save buttons.
    await page
      .locator('qt-project-header')
      .getByRole('button', { name: 'Save', exact: true })
      .click();

    // The rename survives a full reload (server state).
    await page.reload();
    await expect(page.getByRole('heading', { name: 'Renamed by the walk' })).toBeVisible({
      timeout: 10_000,
    });
  });

  test('create a throwaway project through the dialog', async ({ page }) => {
    test.setTimeout(60_000);
    await page.goto(`${PROJ_BASE_URL}/prospero`);
    await unlockIfLocked(page);

    await page.getByRole('button', { name: 'Create Project' }).click();
    const dialogName = page.locator('#qt-project-name');
    await expect(dialogName).toBeVisible();
    await dialogName.fill('Walk-created Project');
    await page.getByRole('button', { name: 'Create', exact: true }).click();

    // On success the SPA navigates into the new project's detail.
    await expect(page).toHaveURL(/\/prospero\/[^/]+$/, { timeout: 10_000 });
    await expect(page.getByRole('heading', { name: 'Walk-created Project' })).toBeVisible({
      timeout: 10_000,
    });
  });

  // P4.6r — the Default Roleplay Template picker (Model Behavior card). Fetches
  // the roleplay-templates listing (lane A) and persists the project's
  // `defaultRoleplayTemplateId` across a reload. The extended groups-projects
  // fixture carries at least one user roleplay template for this beat.
  test('the Model Behavior roleplay-template picker seeds options and persists a selection', async ({
    page,
  }) => {
    test.setTimeout(60_000);
    await page.goto(`${PROJ_BASE_URL}/prospero`);
    await unlockIfLocked(page);

    const projectCards = page.locator('qt-project-card');
    await expect(projectCards.first()).toBeVisible({ timeout: 10_000 });
    await projectCards.first().getByRole('link', { name: 'Open' }).click();
    await expect(page).toHaveURL(/\/prospero\/[^/]+$/);

    const card = page.locator('qt-project-model-behavior-card');
    await expect(card).toBeVisible({ timeout: 10_000 });
    // Open the collapsible if it starts closed.
    const picker = card.getByLabel('Default Roleplay Template');
    if (!(await picker.isVisible().catch(() => false))) {
      await card.getByRole('button', { name: /Model Behavior/ }).click();
    }
    await expect(picker).toBeEnabled({ timeout: 10_000 });

    // Select the first real template (index 1 — index 0 is "Inherit…").
    const optionCount = await picker.locator('option').count();
    expect(optionCount).toBeGreaterThan(1);
    const value = await picker.locator('option').nth(1).getAttribute('value');
    await picker.selectOption(value!);
    await expect(page.locator('qt-project-model-behavior-card')).not.toContainText('Saving…', {
      timeout: 10_000,
    });

    // The selection survives a full reload (server state).
    await page.reload();
    await unlockIfLocked(page, page.locator('qt-project-model-behavior-card'));
    await expect(page).toHaveURL(/\/prospero\/[^/]+$/);
    const reloaded = page.locator('qt-project-model-behavior-card').getByLabel(
      'Default Roleplay Template',
    );
    if (!(await reloaded.isVisible().catch(() => false))) {
      await page
        .locator('qt-project-model-behavior-card')
        .getByRole('button', { name: /Model Behavior/ })
        .click();
    }
    await expect(reloaded).toHaveValue(value!, { timeout: 10_000 });
  });

  // P4.6o — the project Wardrobe card (inline draft form + rows).
  test('the Wardrobe card: create a default item, see its badges, delete it', async ({ page }) => {
    test.setTimeout(60_000);
    await page.goto(`${PROJ_BASE_URL}/prospero`);
    await unlockIfLocked(page);

    const projectCards = page.locator('qt-project-card');
    await expect(projectCards.first()).toBeVisible({ timeout: 10_000 });
    await projectCards.first().getByRole('link', { name: 'Open' }).click();
    await expect(page).toHaveURL(/\/prospero\/[^/]+$/);

    const card = page.locator('qt-project-wardrobe-card');
    await expect(card).toBeVisible({ timeout: 10_000 });
    const newBtn = card.getByRole('button', { name: '+ New wardrobe item' });
    if (!(await newBtn.isVisible().catch(() => false))) {
      await card.getByRole('button', { name: /^Wardrobe \(/ }).click();
      await expect(newBtn).toBeVisible({ timeout: 10_000 });
    }

    // --- Create a default (top) garment ---
    await newBtn.click();
    const title = card.getByPlaceholder('e.g. House livery jacket');
    await expect(title).toBeVisible();
    await title.fill('Walk livery cloak');
    await card
      .locator('label', { hasText: 'Default item' })
      .locator('input[type="checkbox"]')
      .check();
    await card.getByRole('button', { name: 'Create item' }).click();

    const row = card.locator('li', { hasText: 'Walk livery cloak' });
    await expect(row).toBeVisible({ timeout: 10_000 });
    await expect(row).toContainText('top');
    await expect(row).toContainText('Default');

    // --- Delete (window.confirm) ---
    page.once('dialog', (d) => void d.accept());
    await row.getByRole('button', { name: 'Delete' }).click();
    await expect(card.locator('li', { hasText: 'Walk livery cloak' })).toHaveCount(0, {
      timeout: 10_000,
    });
  });
});

function runCliWrite(cli: string, sql: string): void {
  const res = spawnSync(cli, ['db', '--data-dir', PROJ_INSTANCE_DIR, '--write', sql], {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  if (res.status !== 0) {
    // The fixture materializes only the tables its walks read; v4/v5 repos
    // auto-ensure collections on first access, so a table can be legitimately
    // absent (e.g. `tags`). Skip those instead of failing the setup.
    // …and the store-backed slim rows (groups/projects) carry no userId
    // column at all — they are not user-scoped. Skip both shapes.
    const out = `${res.stdout}${res.stderr}`;
    if (out.includes('no such table') || out.includes('no such column: userId')) {
      console.warn(`fixture rewrite skipped (not user-scoped): ${sql}`);
      return;
    }
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
      const res = await fetch(`${PROJ_BASE_URL}/health`);
      if (res.status === 423 || res.status === 200) return;
      lastErr = `health status ${res.status}`;
    } catch (e) {
      lastErr = e instanceof Error ? e.message : String(e);
    }
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(
    `projects server did not become ready within 30s (${lastErr}); see ${PROJ_SERVER_LOG}`,
  );
}
