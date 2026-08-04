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
 * P4.29 (Tier 2) — three representative beats over the census's remaining
 * OPEN rows, walked live: a character save, a project-detail action, and a
 * files action. Every screen here answered these outcomes with either
 * nothing at all or a v5-invented inline banner before this lane; all three
 * now raise v4's own sentence in `[role="toast-container"]` (`toast-flow.spec.ts`'s
 * mechanism, the toast subsystem's own e2e anchor).
 *
 * Own dedicated server + a private copy of the committed groups-projects
 * fixture (it carries characters, projects, and files together — the one
 * fixture family covering all three beats). No LLM send happens on any of
 * these paths, so no mock LLM is needed; the walk spends nothing.
 */

const PORT = 4323;
const BASE_URL = `http://127.0.0.1:${PORT}`;
const INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'toast-open-rows-instance');
const DATA_DIR = resolve(INSTANCE_DIR, 'data');
const SERVER_LOG = resolve(ARTIFACTS_DIR, 'toast-open-rows-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'groups-projects-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'groups-projects-mount.db');
const FIXTURE_READY = existsSync(MAIN_FIXTURE);

// `groups` and `projects` carry no `userId` column in this fixture family
// (neither is user-scoped) — dropped here (the rewrite is a no-op need, not
// a skipped requirement: unscoped rows are visible regardless of the
// rewritten SINGLE_USER_ID).
const USER_TABLES = ['characters', 'chats', 'tags', 'files'];

let server: ChildProcess | undefined;

function runCliWrite(cli: string, sql: string): void {
  const result = spawnSync(cli, ['db', '--data-dir', INSTANCE_DIR, '--write', sql], {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  if (result.status !== 0) {
    throw new Error(
      `quilltap db --write failed (${result.status}): ${result.stderr?.toString() ?? ''}`,
    );
  }
}

function withoutPepper(): NodeJS.ProcessEnv {
  const env = { ...process.env };
  delete env['QUILLTAP_PEPPER'];
  return env;
}

async function waitForHealth(): Promise<void> {
  const deadline = Date.now() + 30_000;
  let lastErr = '';
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${BASE_URL}/health`);
      // 423 = locked-but-up (the server answers before the passphrase unlock).
      if (res.status === 423 || res.status === 200) return;
      lastErr = `health status ${res.status}`;
    } catch (e) {
      lastErr = e instanceof Error ? e.message : String(e);
    }
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(`server did not become ready within 30s (${lastErr}); see ${SERVER_LOG}`);
}

async function unlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  if (await passphrase.isVisible().catch(() => false)) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

/** The live toast stack — v4's own testability handle, carried verbatim. */
function toasts(page: Page) {
  return page.locator('[role="toast-container"] > div');
}

test.describe('P4.29 — the census OPEN rows, three representative beats', () => {
  test.skip(!FIXTURE_READY, 'awaits the committed groups-projects fixture');

  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(DATA_DIR, { recursive: true });
    copyFileSync(MAIN_FIXTURE, resolve(DATA_DIR, 'quilltap.db'));
    if (existsSync(MOUNT_FIXTURE)) {
      copyFileSync(MOUNT_FIXTURE, resolve(DATA_DIR, 'quilltap-mount-index.db'));
    }

    writeFileSync(resolve(DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));
    for (const table of USER_TABLES) {
      runCliWrite(
        cli,
        `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`,
      );
    }
    // This fixture family predates the general-files `folders` table (the same
    // gap `global-setup.ts` patches for the shared server) — schema
    // materialization, not a fixture regen; IF NOT EXISTS keeps it a no-op once
    // the fixture itself carries the table.
    runCliWrite(
      cli,
      'CREATE TABLE IF NOT EXISTS folders (' +
        'id TEXT PRIMARY KEY, userId TEXT, path TEXT, name TEXT, ' +
        'parentFolderId TEXT, projectId TEXT, createdAt TEXT, updatedAt TEXT);',
    );

    const logFd = openSync(SERVER_LOG, 'w');
    server = spawn(
      web,
      ['--host', '127.0.0.1', '--port', String(PORT), '--data-dir', INSTANCE_DIR, '--spa-dir', spaDir()],
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
    rmSync(INSTANCE_DIR, { recursive: true, force: true });
  });

  test('a character save raises "Character saved successfully!" (v4 useCharacterEdit.ts:250)', async ({
    page,
  }) => {
    await page.goto(`${BASE_URL}/characters`);
    await unlock(page);
    await expect(page.getByRole('heading', { name: 'Characters', exact: true })).toBeVisible({
      timeout: 15_000,
    });

    const firstCard = page.locator('.character-card-grid .character-card').first();
    await expect(firstCard).toBeVisible({ timeout: 10_000 });
    await firstCard.locator('p.line-clamp-3').click();

    const editLink = page.getByRole('link', { name: /Edit Character/i });
    await expect(editLink).toBeVisible({ timeout: 10_000 });
    await editLink.click();

    // No field is touched — the Save button is not gated on dirty state (v4
    // parity: the PUT always sends the whole form bag), so this beat writes
    // back exactly what was already stored.
    const save = page.getByRole('button', { name: 'Save Character' });
    await expect(save).toBeVisible({ timeout: 10_000 });
    await save.click();

    const toast = toasts(page).filter({ hasText: 'Character saved successfully!' });
    await expect(toast).toBeVisible({ timeout: 15_000 });
    await expect(toast).toHaveClass(/qt-toast-success/);
  });

  test('toggling Allow Any Character raises v4\'s sentence for both directions', async ({
    page,
  }) => {
    await page.goto(`${BASE_URL}/prospero`);
    await unlock(page);
    await expect(page.getByRole('heading', { name: 'Projects', exact: true })).toBeVisible({
      timeout: 15_000,
    });

    const projectCards = page.locator('qt-project-card');
    await expect(projectCards.first()).toBeVisible({ timeout: 10_000 });
    await projectCards.first().getByRole('link', { name: 'Open' }).click();
    await expect(page.getByRole('heading', { name: 'Characters', exact: true })).toBeVisible({
      timeout: 10_000,
    });

    const toggle = page.getByRole('switch', { name: 'Allow Any Character' });
    const before = await toggle.getAttribute('aria-checked');
    await toggle.click();
    await expect(toggle).toHaveAttribute('aria-checked', before === 'true' ? 'false' : 'true');

    const firstMessage =
      before === 'true'
        ? 'Only roster characters can participate'
        : 'Any character can now participate';
    await expect(toasts(page).filter({ hasText: firstMessage })).toBeVisible({ timeout: 15_000 });

    // Toggle back — leaves the fixture exactly as this beat found it.
    await toggle.click();
    await expect(toggle).toHaveAttribute('aria-checked', before ?? 'false');
    const secondMessage =
      before === 'true'
        ? 'Any character can now participate'
        : 'Only roster characters can participate';
    await expect(toasts(page).filter({ hasText: secondMessage })).toBeVisible({ timeout: 15_000 });
  });

  test('creating a folder raises "Folder created" (v4 CreateFolderModal.tsx)', async ({
    page,
  }) => {
    await page.goto(`${BASE_URL}/files`);
    await unlock(page);
    await expect(page.getByRole('heading', { name: 'General Files' })).toBeVisible({
      timeout: 15_000,
    });

    await page.getByTitle('New Folder').click();
    const nameInput = page.locator('#folder-name');
    await expect(nameInput).toBeVisible({ timeout: 10_000 });
    await nameInput.fill(`e2e-toast-folder-${Date.now()}`);
    await page.getByRole('button', { name: 'Create Folder' }).click();

    const toast = toasts(page).filter({ hasText: 'Folder created' });
    await expect(toast).toBeVisible({ timeout: 15_000 });
    await expect(toast).toHaveClass(/qt-toast-success/);
  });
});
