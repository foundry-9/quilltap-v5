import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, openSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { expect, test, type Page } from './support/fixtures';

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
 * P4.6o — browser walk of the Scenarios verticals (the scope-agnostic
 * ScenariosManager, rendered by both the project Scenarios card and the general
 * `/scenarios` page). Authored pre-unification against lane A's committed
 * groups-projects fixture (which gains general-scope scenarios this round); the
 * whole describe SKIPS when that fixture is absent (this worktree) and
 * auto-activates once lane A lands it at unification.
 *
 * Beats:
 *  - project card: create → the row shows the `.md` suffix → edit the body →
 *    set default (radio) → rename (filename prompt) → delete (confirm).
 *  - general page: create a scenario and see it listed.
 *
 * The managers use `window.confirm` (delete) and `window.prompt` (rename), so
 * each mutating beat installs a one-shot `page.on('dialog')` handler.
 */

const SCEN_PORT = 4326;
const SCEN_BASE_URL = `http://127.0.0.1:${SCEN_PORT}`;
const SCEN_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'scenarios-instance');
const SCEN_DATA_DIR = resolve(SCEN_INSTANCE_DIR, 'data');
const SCEN_SERVER_LOG = resolve(ARTIFACTS_DIR, 'scenarios-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'groups-projects-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'groups-projects-mount.db');
/** Lane A owns the fixture; skip the live walk until it is committed. */
const FIXTURE_READY = existsSync(MAIN_FIXTURE);

/** Every fixture table the scenarios walk reads is filtered by userId. */
const USER_TABLES = ['characters', 'chats', 'tags', 'groups', 'projects', 'files'];

let server: ChildProcess | undefined;

test.describe('P4.6o — Scenarios verticals (project card CRUD + the general page)', () => {
  test.skip(!FIXTURE_READY, 'awaits lane A groups-projects fixture (wired at unification)');

  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(SCEN_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(SCEN_DATA_DIR, { recursive: true });
    copyFileSync(MAIN_FIXTURE, resolve(SCEN_DATA_DIR, 'quilltap.db'));
    if (existsSync(MOUNT_FIXTURE)) {
      copyFileSync(MOUNT_FIXTURE, resolve(SCEN_DATA_DIR, 'quilltap-mount-index.db'));
    }

    writeFileSync(
      resolve(SCEN_DATA_DIR, 'quilltap.dbkey'),
      makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE),
    );
    for (const table of USER_TABLES) {
      runCliWrite(
        cli,
        `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`,
      );
    }

    const logFd = openSync(SCEN_SERVER_LOG, 'w');
    server = spawn(
      web,
      [
        '--host',
        '127.0.0.1',
        '--port',
        String(SCEN_PORT),
        '--data-dir',
        SCEN_INSTANCE_DIR,
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
    rmSync(SCEN_INSTANCE_DIR, { recursive: true, force: true });
  });

  async function unlockIfLocked(page: Page, ready: ReturnType<Page['getByRole']>): Promise<void> {
    const passphrase = page.locator('#qt-passphrase');
    await expect(passphrase.or(ready).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.isVisible()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
    await expect(ready.first()).toBeVisible({ timeout: 10_000 });
  }

  test('the project Scenarios card: create → edit → set default → rename → delete', async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await page.goto(`${SCEN_BASE_URL}/prospero`);
    await unlockIfLocked(page, page.getByRole('heading', { name: 'Projects', exact: true }));

    const projectCards = page.locator('qt-project-card');
    await expect(projectCards.first()).toBeVisible({ timeout: 10_000 });
    await projectCards.first().getByRole('link', { name: 'Open' }).click();
    await expect(page).toHaveURL(/\/prospero\/[^/]+$/);

    const card = page.locator('qt-project-scenarios-card');
    await expect(card).toBeVisible({ timeout: 10_000 });
    await expandCard(page, card);

    // --- Create ---
    await card.getByRole('button', { name: '+ New scenario' }).click();
    const filename = page.locator('#scenario-filename');
    await expect(filename).toBeVisible();
    const stamp = `walk-${Date.now()}`;
    await filename.fill(stamp);
    await page.locator('#scenario-name').fill('A Walk-created Scene');
    // The body is the qt-markdown-field now (P4.6aq) — a ProseMirror
    // contenteditable, so drive it with real key events (fill() targets
    // input/textarea).
    await typeScenarioBody(page, 'The lamplight falls on {{char}} and {{user}}.');
    await page.getByRole('button', { name: 'Create scenario' }).click();

    // The row shows the display name + the `.md` filename suffix.
    const row = card.locator('qt-scenario-row', { hasText: 'A Walk-created Scene' });
    await expect(row).toBeVisible({ timeout: 10_000 });
    await expect(row).toContainText(`${stamp}.md`);

    // --- Edit the body (the modal drops the filename field in edit mode) ---
    await rowAction(row, 'Edit');
    await expect(page.locator('#scenario-filename')).toHaveCount(0);
    await typeScenarioBody(page, 'Rewritten: {{char}} lingers at the gate.', /* replace */ true);
    await page.getByRole('button', { name: 'Save changes' }).click();
    await expect(scenarioBody(page)).toHaveCount(0);

    // --- Set default via the radio; the Default badge appears ---
    await row.locator('input[type="radio"]').check();
    await expect(row).toContainText('Default', { timeout: 10_000 });

    // --- Rename on the FILENAME (window.prompt) ---
    const renamed = `${stamp}-renamed`;
    page.once('dialog', (d) => void d.accept(renamed));
    await rowAction(row, 'Rename');
    await expect(row).toContainText(`${renamed}.md`, { timeout: 10_000 });

    // --- Delete (window.confirm) ---
    page.once('dialog', (d) => void d.accept());
    await rowAction(row, 'Delete');
    await expect(card.locator('qt-scenario-row', { hasText: 'A Walk-created Scene' })).toHaveCount(
      0,
      { timeout: 10_000 },
    );
  });

  test('the general /scenarios page: create a scenario and see it listed', async ({ page }) => {
    test.setTimeout(60_000);
    await page.goto(`${SCEN_BASE_URL}/scenarios`);
    await unlockIfLocked(page, page.getByRole('heading', { name: 'General Scenarios' }));

    await page.getByRole('button', { name: '+ New scenario' }).click();
    const stamp = `general-${Date.now()}`;
    await page.locator('#scenario-filename').fill(stamp);
    await page.locator('#scenario-name').fill('A General Scene');
    await typeScenarioBody(page, 'Everywhere, always: {{char}}.');
    await page.getByRole('button', { name: 'Create scenario' }).click();

    const row = page.locator('qt-scenario-row', { hasText: 'A General Scene' });
    await expect(row).toBeVisible({ timeout: 10_000 });
    await expect(row).toContainText(`${stamp}.md`);
  });
});

/**
 * Click a row action (Edit/Rename/Delete) whichever layout the row is in: the
 * row is container-query adaptive (v4 parity), so inside the dense project
 * card grid it renders the narrow `⋮` kebab instead of inline buttons.
 */
async function rowAction(row: ReturnType<Page['locator']>, name: string): Promise<void> {
  const inline = row.getByRole('button', { name, exact: true });
  if (await inline.isVisible().catch(() => false)) {
    await inline.click();
    return;
  }
  await row.getByRole('button', { name: /^More actions for / }).click();
  await row.getByRole('menuitem', { name, exact: true }).click();
}

/** The scenario modal's body editor — a ProseMirror contenteditable since the
 *  P4.6aq qt-markdown-field swap (it was a `#scenario-body` textarea). */
function scenarioBody(page: Page): ReturnType<Page['locator']> {
  return page.locator('qt-scenario-editor-modal .qt-rich-editor-content');
}

/**
 * Type into the scenario body. `.fill()` only targets input/textarea, so the
 * rich editor takes real key events (the P4.6ag idiom). `replace` clears the
 * seeded body first (select-all + type).
 */
async function typeScenarioBody(page: Page, text: string, replace = false): Promise<void> {
  const body = scenarioBody(page);
  await expect(body).toBeVisible();
  await body.click();
  if (replace) {
    await page.keyboard.press('ControlOrMeta+a');
  }
  await page.keyboard.type(text);
}

/** Expand a collapsible card if its body's primary action is not yet visible. */
async function expandCard(page: Page, card: ReturnType<Page['locator']>): Promise<void> {
  const newBtn = card.getByRole('button', { name: '+ New scenario' });
  if (!(await newBtn.isVisible().catch(() => false))) {
    await card.getByRole('button', { name: /^Scenarios \(/ }).click();
    await expect(newBtn).toBeVisible({ timeout: 10_000 });
  }
}

function runCliWrite(cli: string, sql: string): void {
  const res = spawnSync(cli, ['db', '--data-dir', SCEN_INSTANCE_DIR, '--write', sql], {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  if (res.status !== 0) {
    // Store-backed slim rows (groups/projects) carry no userId column, and the
    // fixture materializes only the tables its walks read — skip both shapes.
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
      const res = await fetch(`${SCEN_BASE_URL}/health`);
      if (res.status === 423 || res.status === 200) return;
      lastErr = `health status ${res.status}`;
    } catch (e) {
      lastErr = e instanceof Error ? e.message : String(e);
    }
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(
    `scenarios server did not become ready within 30s (${lastErr}); see ${SCEN_SERVER_LOG}`,
  );
}
