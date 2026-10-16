import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, mkdirSync, openSync, rmSync, writeFileSync } from 'node:fs';
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
 * P4.D309 — **Copy to folder…** in the browser (v4 `bbbcd2388`): the row's
 * kebab entry right after **Move to folder…**, the picker in copy mode (its
 * title and helper; the folder the garment already hangs in stays choosable,
 * with no `(here now)` — a copy may land beside its original), the copy toast,
 * the `(copy)` → `(copy 2)` titles with the original untouched, and the select
 * bar's **Copy to folder…** over two items (`Copies of 2 items …`).
 *
 * **LIVE — no gate flag.** Both halves of the feature (the `copy-items` verb
 * and the SPA) land on the same lane branch; the `P4D3xx_SERVER_LANDED` idiom
 * is for a sibling server lane's premise, and there is none here.
 *
 * Runs against its OWN locked server + instance dir over the committed
 * `characters-*` fixture pair (Aria and her own `Flight Jacket`) on the
 * lane-unique port 4345 (§R.9; the mock-LLM port 45343 is reserved for this
 * file but unused — no send happens) — the `wardrobe-folders-flow.spec.ts`
 * recipe: copy the pair, write the locked .dbkey, materialize the tables the
 * built-in stores and the transfer enumerator read, rewrite the fixture user
 * to SINGLE_USER_ID with `quilltap db --write` BEFORE launch, then boot
 * quilltap-web.
 *
 * The outfit-asks-after-its-components beat (Tier 2, R-5) is NOT here: the
 * committed pair has no outfit, and composing one through the item editor's
 * component picker would not stay under the order's ~40 lines; the radio
 * contract rides `wardrobe-folder-picker-dialog.spec.ts` and the transcribed
 * `wardrobe-browser.spec.ts` case instead (recorded in the lane record).
 */
const COPY_PORT = 4345;
const BASE_URL = `http://127.0.0.1:${COPY_PORT}`;
const INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'wardrobe-copy-to-folder-instance');
const DATA_DIR = resolve(INSTANCE_DIR, 'data');
const SERVER_LOG = resolve(ARTIFACTS_DIR, 'wardrobe-copy-to-folder-server.log');

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

test.describe.configure({ mode: 'serial' });

test.describe('P4.D309 — wardrobe copy to folder in the browser', () => {
  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(DATA_DIR, { recursive: true });
    copyFileSync(resolve(FIXTURES_DIR, 'characters-main.db'), resolve(DATA_DIR, 'quilltap.db'));
    copyFileSync(
      resolve(FIXTURES_DIR, 'characters-mount.db'),
      resolve(DATA_DIR, 'quilltap-mount-index.db'),
    );
    writeFileSync(resolve(DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));
    // The same instance MATERIALIZATION as `wardrobe-flow.spec.ts` (P4.D130 /
    // P4.69): the built-in stores' `instance_settings` guard, and the empty
    // `projects` / `groups` the enumerators read. The committed pair is untouched.
    runCliWrite(
      cli,
      'CREATE TABLE IF NOT EXISTS instance_settings (' +
        '"key" TEXT PRIMARY KEY, "value" TEXT NOT NULL);',
    );
    runCliWrite(
      cli,
      'CREATE TABLE IF NOT EXISTS projects (' +
        '"id" TEXT PRIMARY KEY NOT NULL, "name" TEXT NOT NULL, ' +
        '"officialMountPointId" TEXT, "createdAt" TEXT NOT NULL, ' +
        '"updatedAt" TEXT NOT NULL, "description" TEXT, "instructions" TEXT, ' +
        '"state" TEXT DEFAULT \'{}\', "allowAnyCharacter" INTEGER DEFAULT 0, ' +
        '"characterRoster" TEXT DEFAULT \'[]\', "color" TEXT, "icon" TEXT, ' +
        '"defaultDisabledTools" TEXT DEFAULT \'[]\', ' +
        '"defaultDisabledToolGroups" TEXT DEFAULT \'[]\', ' +
        '"defaultAgentModeEnabled" INTEGER, ' +
        '"defaultAvatarGenerationEnabled" INTEGER, ' +
        '"defaultImageProfileId" TEXT, "defaultRoleplayTemplateId" TEXT, ' +
        '"defaultAlertCharactersOfLanternImages" INTEGER, ' +
        '"answerConfirmationOverride" TEXT, "storyBackgroundsEnabled" INTEGER, ' +
        '"staticBackgroundImageId" TEXT, "storyBackgroundImageId" TEXT, ' +
        '"backgroundDisplayMode" TEXT DEFAULT \'theme\');',
    );
    runCliWrite(
      cli,
      'CREATE TABLE IF NOT EXISTS groups (' +
        '"id" TEXT PRIMARY KEY NOT NULL, "name" TEXT NOT NULL, ' +
        '"officialMountPointId" TEXT, "createdAt" TEXT NOT NULL, ' +
        '"updatedAt" TEXT NOT NULL, "description" TEXT, "instructions" TEXT, ' +
        '"state" TEXT DEFAULT \'{}\', "color" TEXT, "icon" TEXT);',
    );
    for (const table of USER_TABLES) {
      runCliWrite(
        cli,
        `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`,
      );
    }

    const logFd = openSync(SERVER_LOG, 'w');
    server = spawn(
      web,
      [
        '--host',
        '127.0.0.1',
        '--port',
        String(COPY_PORT),
        '--data-dir',
        INSTANCE_DIR,
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
    rmSync(INSTANCE_DIR, { recursive: true, force: true });
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

  /** Aria's detail → the Wardrobe tab → the dialog (the `wardrobe-flow` route). */
  async function openAriasWardrobe(page: Page): Promise<ReturnType<Page['getByRole']>> {
    await page.goto(`${BASE_URL}/characters`);
    await unlockIfLocked(page);
    const aria = page
      .locator('.character-card-grid .character-card')
      .filter({ hasText: 'Aria' })
      .first();
    await expect(aria).toBeVisible();
    await aria.locator('p.line-clamp-3').click();
    await expect(page.getByRole('heading', { name: 'Aria' })).toBeVisible();
    await page
      .locator('.character-view')
      .getByRole('button', { name: 'Wardrobe', exact: true })
      .click();
    await page.getByRole('button', { name: 'Open wardrobe for Aria' }).click();
    // The WARDROBE dialog by its title: the folder picker is a modal of its
    // own (`role="dialog"`, v4's `BaseModal` too), so a bare `getByRole('dialog')`
    // is ambiguous while it is open.
    const dialog = page
      .getByRole('dialog')
      .filter({ has: page.locator('.qt-dialog-title', { hasText: /^\s*Wardrobe\s*$/ }) });
    await expect(dialog.locator('.qt-dialog-title')).toHaveText('Wardrobe');
    return dialog;
  }

  const toasts = (page: Page) => page.locator('[role="toast-container"] > div');
  const itemRow = (dialog: ReturnType<Page['getByRole']>, title: string) =>
    dialog.locator('qt-wardrobe-item-row').filter({ hasText: title });

  test('Copy to folder…: a copy lands beside its original as (copy), then (copy 2); the select bar copies two', async ({
    page,
  }) => {
    test.setTimeout(90_000);
    const dialog = await openAriasWardrobe(page);
    const topLevelToast = (text: string) => toasts(page).filter({ hasText: text });

    const copyJacket = async (): Promise<void> => {
      const jacket = itemRow(dialog, 'Flight Jacket').filter({ hasNotText: '(copy' }).first();
      await jacket.getByRole('button', { name: 'More actions' }).click();
      // The entry sits right after Move to folder… (read once the menu is up —
      // `allTextContents` does not wait).
      await expect(page.getByRole('menuitem', { name: 'Copy to folder…', exact: true })).toBeVisible();
      const entries = page.getByRole('menuitem');
      const labels = (await entries.allTextContents()).map((t) => t.trim());
      expect(labels[labels.indexOf('Move to folder…') + 1]).toBe('Copy to folder…');
      await page.getByRole('menuitem', { name: 'Copy to folder…', exact: true }).click();
      const picker = page.getByRole('listbox', { name: 'Folders' });
      await expect(picker).toBeVisible();
      await expect(page.getByText('Copy "Flight Jacket" to a folder')).toBeVisible();
      await expect(
        page.getByText(
          'Each copy is a new item with its own ID and a "(copy)" title, filed in the folder you choose.',
        ),
      ).toBeVisible();
      // The jacket hangs at the top level — and that row stays choosable.
      const top = picker.getByRole('option', { name: /top level/ });
      await expect(top).toBeEnabled();
      await expect(top).not.toContainText('(here now)');
      const copy = page.getByRole('button', { name: 'Copy', exact: true });
      await expect(copy).toBeDisabled();
      await top.click();
      await expect(copy).toBeEnabled();
      await copy.click();
      await expect(picker).toBeHidden({ timeout: 10_000 });
    };

    await copyJacket();
    await expect(topLevelToast('A copy of "Flight Jacket" now hangs in the top of the wardrobe')).toBeVisible({ timeout: 10_000 });
    await expect(itemRow(dialog, 'Flight Jacket (copy)')).toHaveCount(1, { timeout: 10_000 });
    // The original is still there, still itself.
    await expect(
      itemRow(dialog, 'Flight Jacket').filter({ hasNotText: '(copy' }),
    ).toHaveCount(1);

    await copyJacket();
    await expect(itemRow(dialog, 'Flight Jacket (copy 2)')).toHaveCount(1, { timeout: 10_000 });
    await expect(itemRow(dialog, 'Flight Jacket')).toHaveCount(3);

    // The select bar: the two copies, copied again.
    await dialog.getByRole('button', { name: 'Select', exact: true }).click();
    const bar = dialog.getByTestId('wardrobe-select-bar');
    await expect(bar.getByRole('button', { name: 'Copy to folder…', exact: true })).toBeDisabled();
    await dialog.getByLabel('Select Flight Jacket (copy)', { exact: true }).check();
    await dialog.getByLabel('Select Flight Jacket (copy 2)', { exact: true }).check();
    await bar.getByRole('button', { name: 'Copy to folder…', exact: true }).click();
    const picker = page.getByRole('listbox', { name: 'Folders' });
    await expect(page.getByText('Copy 2 items to a folder')).toBeVisible();
    await picker.getByRole('option', { name: /top level/ }).click();
    await page.getByRole('button', { name: 'Copy', exact: true }).click();
    await expect(topLevelToast('Copies of 2 items now hang in the top of the wardrobe')).toBeVisible({ timeout: 10_000 });
    await expect(itemRow(dialog, 'Flight Jacket')).toHaveCount(5, { timeout: 10_000 });

    await dialog.getByRole('button', { name: 'Done' }).click();
    await expect(dialog).toBeHidden();
  });
});

function runCliWrite(cli: string, sql: string): void {
  const res = spawnSync(cli, ['db', '--data-dir', INSTANCE_DIR, '--write', sql], {
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
      const res = await fetch(`${BASE_URL}/health`);
      if (res.status === 423 || res.status === 200) return;
      lastErr = `health status ${res.status}`;
    } catch (e) {
      lastErr = e instanceof Error ? e.message : String(e);
    }
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(
    `wardrobe copy-to-folder server did not become ready within 30s (${lastErr}); see ${SERVER_LOG}`,
  );
}
