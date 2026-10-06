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

    // The Files card names and sizes every row as `projectFileList` sends it
    // (dogfood #135: the card read `fileName` / `fileSizeBytes`, which the
    // wire never carries, so a real project's files all rendered nameless at
    // `0 B`).
    const projectId = page.url().split('/').pop() as string;
    const listed = await page.request.post(`${PROJ_BASE_URL}/api/dispatch`, {
      data: { type: 'projectFileList', projectId },
    });
    const files = ((await listed.json()) as {
      data: { files: { originalFilename: string; size: number }[] };
    }).data.files;
    expect(files.length).toBeGreaterThan(0);
    const fileRows = page.locator('qt-project-files-card .max-h-64 > button');
    await expect(fileRows).toHaveCount(Math.min(files.length, 10), { timeout: 10_000 });
    for (const [i, f] of files.slice(0, 10).entries()) {
      await expect(fileRows.nth(i).locator('p').first()).toHaveText(f.originalFilename);
      if (f.size > 0) {
        await expect(fileRows.nth(i).locator('p').nth(1)).not.toContainText(/^0 B/);
      }
    }

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

  // Dogfood #143 — a project-detail select whose save the server refuses must
  // snap back to the stored value (v4's selects are controlled by `project`,
  // which a failed save never sets). The real gesture: pick a new option.
  test('a refused Model Behavior save toasts v4 and puts the select back', async ({ page }) => {
    test.setTimeout(60_000);
    await page.goto(`${PROJ_BASE_URL}/prospero`);
    await unlockIfLocked(page);

    const projectCards = page.locator('qt-project-card');
    await expect(projectCards.first()).toBeVisible({ timeout: 10_000 });
    await projectCards.first().getByRole('link', { name: 'Open' }).click();
    await expect(page).toHaveURL(/\/prospero\/[^/]+$/);

    const card = page.locator('qt-project-model-behavior-card');
    await expect(card).toBeVisible({ timeout: 10_000 });
    const agentMode = card.getByLabel('Agent Mode');
    if (!(await agentMode.isVisible().catch(() => false))) {
      await card.getByRole('button', { name: /Model Behavior/ }).click();
    }
    await expect(agentMode).toBeEnabled({ timeout: 10_000 });
    const stored = await agentMode.inputValue();
    const other = stored === 'disabled' ? 'enabled' : 'disabled';

    // Refuse THIS verb only; every other dispatch goes through.
    await page.route('**/api/dispatch', async (route) => {
      const body = route.request().postDataJSON() as { type?: string } | null;
      if (body?.type !== 'projectUpdate') {
        await route.fallback();
        return;
      }
      await route.fulfill({
        status: 400,
        contentType: 'application/json',
        body: JSON.stringify({ type: 'error', data: { kind: 'bad-request', message: 'boom' } }),
      });
    });

    await agentMode.selectOption(other);
    const toast = page
      .locator('[role="toast-container"] > div')
      .filter({ hasText: 'Failed to update agent mode setting' });
    await expect(toast).toBeVisible({ timeout: 10_000 });
    await expect(agentMode).toHaveValue(stored);
    await page.unroute('**/api/dispatch');
  });

  // P4.9E4B — Default Tool Settings (the Model Behavior card's Configure). The
  // row was a disabled affordance until now; the write is `projectToolSettings
  // Update`, live on main since P4.9E3B's Prospero server slice, and the read-back
  // is `projectGet` after a full reload.
  test('Default Tool Settings: disable a builtin group, read it back through projectGet', async ({
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
    const configure = card.getByRole('button', { name: 'Configure', exact: true });
    if (!(await configure.isVisible().catch(() => false))) {
      await card.getByRole('button', { name: /Model Behavior/ }).click();
    }
    await expect(configure).toBeEnabled({ timeout: 10_000 });
    await expect(card).toContainText('All tools enabled');
    await configure.click();

    // A dialog's component host is zero-sized — locate by role.
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible({ timeout: 10_000 });
    await expect(dialog).toContainText('Existing chats are not affected.');
    // Save is inert until something moves (v4's set-vs-set gate).
    const save = dialog.locator('[qt-modal-footer]').getByRole('button', { name: /Save/ });
    await expect(save).toBeDisabled();

    // Turn off the first group, then save.
    await dialog.locator('[role="checkbox"]').first().click();
    await expect(save).toBeEnabled({ timeout: 10_000 });
    await save.click();
    await expect(dialog).toBeHidden({ timeout: 15_000 });

    // The summary moved at once (v4 adopts the success payload locally)...
    await expect(card).not.toContainText('All tools enabled', { timeout: 10_000 });

    // ...and it survives a full reload, so the write really reached the project.
    await page.reload();
    await unlockIfLocked(page, page.locator('qt-project-model-behavior-card'));
    const reloadedCard = page.locator('qt-project-model-behavior-card');
    if (
      !(await reloadedCard
        .getByRole('button', { name: 'Configure', exact: true })
        .isVisible()
        .catch(() => false))
    ) {
      await reloadedCard.getByRole('button', { name: /Model Behavior/ }).click();
    }
    await expect(reloadedCard).toContainText(/disabled/, { timeout: 10_000 });
  });

  // P4.D247 — the Characters card as v4 `9753d0eb2` left it, over the real
  // `projectGet` / `characterList` / `projectCharacterAdd` /
  // `projectCharacterRemove` / `projectUpdate` handlers. Before P4.D247 the
  // card read a phantom `roster` key and no v5 project showed its roster at
  // all. Earlier beats toggle and rename "the first card", so Iota's state is
  // FORCED through a `projectUpdate` first. Edda is present and ARCHIVED in
  // the fixture (read with `quilltap db` on a copy, 2026-10-03), so the picker
  // must not offer her; Diana is the other live non-roster character.
  test('the Characters card: roster, the add picker, remove, both Allow Any toasts', async ({
    page,
  }) => {
    test.setTimeout(90_000);
    const IOTA = 'a3000000-0000-4000-8000-000000000001';
    const ARIA = 'a1000000-0000-4000-8000-000000000001';
    const CLEO = 'a1000000-0000-4000-8000-000000000003';
    const toasts = page.locator('[role="toast-container"] > div');

    await page.goto(`${PROJ_BASE_URL}/prospero`);
    await unlockIfLocked(page);
    const forced = await page.request.post(`${PROJ_BASE_URL}/api/dispatch`, {
      data: {
        type: 'projectUpdate',
        projectId: IOTA,
        project: { allowAnyCharacter: false, characterRoster: [ARIA, CLEO] },
      },
    });
    expect(forced.ok()).toBe(true);

    await page.goto(`${PROJ_BASE_URL}/prospero/${IOTA}`);
    const card = page.locator('qt-project-characters-card');
    const header = card.locator('.qt-collapsible-card-header');
    await expect(header).toBeVisible({ timeout: 10_000 });
    if ((await header.getAttribute('aria-expanded')) === 'false') {
      await header.click();
    }

    // The roster renders from the GET's enriched `characterRoster`.
    await expect(card).toContainText('2 characters in roster', { timeout: 10_000 });
    await expect(card.getByRole('heading', { name: 'Aria', exact: true })).toBeVisible();
    await expect(card.getByRole('heading', { name: 'Cleo', exact: true })).toBeVisible();
    // Visible AT REST — `toBeVisible` ignores opacity, so the CSS is the proof.
    const removeAria = card.getByRole('button', { name: 'Remove Aria from roster' });
    await expect(removeAria).toBeVisible();
    // Park the pointer off every tile first: `group-hover:opacity-100` would
    // read 1 if a layout shift left a tile under it (never seen red — a
    // hardening; the button is never keyboard-focused here, so `focus:`
    // cannot fire either).
    await page.mouse.move(0, 0);
    await expect(removeAria).toHaveCSS('opacity', '0.6');

    // The picker: focused on open; live non-roster characters only.
    await card.getByRole('button', { name: 'Add character', exact: true }).click();
    const search = card.getByRole('textbox', { name: 'Search characters to add' });
    await expect(search).toBeFocused();
    const picker = card.locator('ul');
    await expect(picker.locator('li', { hasText: 'Bram' })).toBeVisible({ timeout: 10_000 });
    await expect(picker.locator('li', { hasText: 'Diana' })).toBeVisible();
    await expect(picker).not.toContainText('Aria');
    await expect(picker).not.toContainText('Cleo');
    await expect(picker).not.toContainText('Edda');

    await search.fill('zzz');
    await expect(card).toContainText('No characters match.');
    await search.fill('bra');
    await expect(picker.locator('li')).toHaveCount(1);
    await expect(picker.locator('li').first()).toContainText('Bram');

    // Add Bram: the toast, a tile, the count, and Bram gone from the list.
    await picker.locator('li', { hasText: 'Bram' }).getByRole('button').click();
    await expect(toasts.filter({ hasText: 'Character added to the roster' })).toBeVisible({
      timeout: 15_000,
    });
    await expect(card).toContainText('3 characters in roster');
    await expect(card.getByRole('heading', { name: 'Bram', exact: true })).toBeVisible();
    await expect(card.locator('ul li', { hasText: 'Bram' })).toHaveCount(0);

    // Remove Bram again.
    await card.getByRole('button', { name: 'Remove Bram from roster' }).click();
    await expect(toasts.filter({ hasText: 'Character removed from the roster' })).toBeVisible({
      timeout: 15_000,
    });
    await expect(card).toContainText('2 characters in roster');

    // Done clears the search. The REOPEN is focused too: a bare `autofocus`
    // attribute passes the FIRST open (a document honours its first autofocus
    // candidate even when inserted late — measured, P4.D247 M9) but never a
    // second, so this is the assertion that pins v4's focus-as-it-mounts.
    await card.getByRole('button', { name: 'Done', exact: true }).click();
    await card.getByRole('button', { name: 'Add character', exact: true }).click();
    await expect(search).toHaveValue('');
    await expect(search).toBeFocused();

    // Collapse and re-expand with the picker open: v4 unmounts the body on a
    // collapse and remounts it on re-expand, so `autoFocus` lands in the
    // search again. v5's input is projected content the collapse only
    // detaches — this pins the re-expand edge (the header click leaves focus
    // on the header button, so a focus here was moved by the card).
    await header.click();
    await expect(header).toHaveAttribute('aria-expanded', 'false');
    await header.click();
    await expect(header).toHaveAttribute('aria-expanded', 'true');
    await expect(search).toBeVisible();
    await expect(search).toBeFocused();

    // Allow Any ON: v4's toast, the open subtitle, the explainer, no picker.
    const toggle = card.getByRole('switch', { name: 'Allow Any Character' });
    await toggle.click();
    await expect(
      toasts.filter({ hasText: 'Every character may now use the project files and wardrobe' }),
    ).toBeVisible({ timeout: 15_000 });
    await expect(card).toContainText('Open to every character');
    await expect(card).toContainText(
      'Any character in a project chat may read and edit its files and borrow from its wardrobe. Turn this off to choose who may.',
    );
    await expect(card.getByRole('button', { name: 'Add character', exact: true })).toHaveCount(0);

    // OFF again: the other sentence.
    await toggle.click();
    await expect(
      toasts.filter({ hasText: 'Only roster characters may use the project files and wardrobe' }),
    ).toBeVisible({ timeout: 15_000 });
    await expect(card).toContainText('2 characters in roster');

    // The roster is server state.
    await page.reload();
    await unlockIfLocked(page, header);
    if ((await header.getAttribute('aria-expanded')) === 'false') {
      await header.click();
    }
    await expect(card).toContainText('2 characters in roster', { timeout: 10_000 });
    await expect(card.getByRole('heading', { name: 'Aria', exact: true })).toBeVisible();
    await expect(card.getByRole('heading', { name: 'Cleo', exact: true })).toBeVisible();
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
      // v4 `8bb1a958` renamed this checkbox's label from the bare "Default
      // item" to say WHO it dresses, now that a project-tier default reaches
      // every character in the project (P4.D39).
      .locator('label', { hasText: 'Worn by default by every character in this project' })
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
