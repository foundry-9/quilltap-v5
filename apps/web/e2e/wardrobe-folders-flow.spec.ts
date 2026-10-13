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
 * P4.D285 — the wardrobe's folders in the browser (v4 `4a38222e6`'s
 * `WardrobeBrowser` + `WardrobeFolderPickerDialog`, with `97f0d8cb8`'s bug-229
 * fix): New folder, entering a folder, `+ New Item` filing into it, the row's
 * **Move to folder…** picker with **(here now)** disabled, **Select** + the
 * bulk move + the picker's inline **New folder…**, inline Rename, the
 * delete-folder confirmation ("moved up a peg"), the search's folder crumbs +
 * **Searching everywhere**, the General tier row and its **Shared** chip
 * hidden by **Show shared**, the bug-229 click / Escape inside the picker, and
 * a refused `images` folder name.
 *
 * **Gated (§S.9).** The folder verbs are the server lanes' (P4.D279's C2 +
 * P4.D281's handlers) and the row's **Move to folder…** / select checkbox /
 * crumb are P4.D286's (C3 §5); until both are on the branch a folder beat
 * fails for reasons that say nothing about this lane. The unifier flips
 * `P4D279_SERVER_LANDED` (and `P4D286_ROW_LANDED`) and runs this file alone.
 * The tier-row beat needs neither and runs LIVE.
 *
 * Runs against its OWN locked server + instance dir over the committed
 * `characters-*` fixture pair (Aria, her own `Flight Jacket` and `Goggles` —
 * `Brass Goggles` is a garment `wardrobe-flow.spec.ts` MAKES, not a fixture
 * row) on the spec-private port 4336 — the `wardrobe-flow.spec.ts`
 * recipe: copy the pair, write the locked .dbkey, materialize the tables the
 * built-in stores and the transfer enumerator read, rewrite the fixture user
 * to SINGLE_USER_ID with `quilltap db --write` BEFORE launch, then boot
 * quilltap-web. No LLM send happens here.
 */
const P4D279_SERVER_LANDED = true;
const P4D286_ROW_LANDED = true;

const FOLDERS_PORT = 4336;
const BASE_URL = `http://127.0.0.1:${FOLDERS_PORT}`;
const INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'wardrobe-folders-instance');
const DATA_DIR = resolve(INSTANCE_DIR, 'data');
const SERVER_LOG = resolve(ARTIFACTS_DIR, 'wardrobe-folders-server.log');

const USER_TABLES = [
  'characters',
  'chats',
  'tags',
  'api_keys',
  'connection_profiles',
  'image_profiles',
  'files',
];

/** The General garment the tier-row beat makes, so Aria has a borrowed wardrobe. */
const GENERAL_GARMENT = 'Opera Cloak';
/** The garment the New-folder beat creates INSIDE Formal (`+ New Item` files into the folder). */
const FOLDER_GARMENT = 'Evening Spats';

let server: ChildProcess | undefined;

test.describe.configure({ mode: 'serial' });

test.describe('P4.D285 — wardrobe folders in the browser', () => {
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
        String(FOLDERS_PORT),
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
  const folderRow = (dialog: ReturnType<Page['getByRole']>, name: string) =>
    dialog.getByTestId('wardrobe-folder-row').filter({ hasText: name });
  const itemRow = (dialog: ReturnType<Page['getByRole']>, title: string) =>
    dialog.locator('qt-wardrobe-item-row').filter({ hasText: title });
  const crumbs = (dialog: ReturnType<Page['getByRole']>) =>
    dialog.getByRole('navigation', { name: 'Wardrobe folders' });

  test('a borrowed wardrobe is ONE tier row with its chip, hidden by Show shared (LIVE)', async ({
    page,
  }) => {
    test.setTimeout(90_000);
    const dialog = await openAriasWardrobe(page);
    // Give Aria something to borrow: a garment in Quilltap General, made
    // through the General container (existing verbs only).
    await dialog.locator('#wardrobe-container-select').selectOption('general:');
    await expect(dialog.getByText('Browsing a shared wardrobe', { exact: false })).toBeVisible();
    await dialog.getByRole('button', { name: '+ New Item' }).click();
    await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeVisible();
    await page.locator('#wardrobe-title').fill(GENERAL_GARMENT);
    await page
      .locator('qt-wardrobe-item-editor label')
      .filter({ hasText: /^\s*top\s*$/i })
      .locator('input[type="checkbox"]')
      .first()
      .click();
    await page.getByRole('button', { name: 'Create', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeHidden({
      timeout: 10_000,
    });
    await expect(itemRow(dialog, GENERAL_GARMENT)).toHaveCount(1, { timeout: 10_000 });
    // Browsing General itself: no tier rows — every row is the container's own.
    await expect(dialog.getByTestId('wardrobe-tier-row')).toHaveCount(0);

    // Back on Aria: the borrowed garment sits behind ONE tier row.
    const ariaValue = await dialog
      .locator('#wardrobe-container-select option')
      .filter({ hasText: 'Aria' })
      .getAttribute('value');
    await dialog.locator('#wardrobe-container-select').selectOption(ariaValue!);
    const tier = dialog.getByTestId('wardrobe-tier-row').filter({ hasText: 'General' });
    await expect(tier).toHaveCount(1, { timeout: 10_000 });
    await expect(tier.locator('.qt-badge-wardrobe-shared')).toHaveText('Shared');
    await expect(tier).toContainText('1 item');
    await expect(itemRow(dialog, GENERAL_GARMENT)).toHaveCount(0);
    await expect(itemRow(dialog, 'Flight Jacket')).toHaveCount(1);

    await tier.click();
    await expect(itemRow(dialog, GENERAL_GARMENT)).toHaveCount(1);
    await expect(crumbs(dialog)).toContainText('General');
    // No filing controls inside a borrowed wardrobe.
    await expect(dialog.getByRole('button', { name: 'New folder', exact: true })).toHaveCount(0);
    await expect(dialog.getByRole('button', { name: 'Select', exact: true })).toHaveCount(0);

    const shared = dialog.locator('label').filter({ hasText: 'Show shared' }).locator('input');
    await shared.uncheck();
    await expect(itemRow(dialog, GENERAL_GARMENT)).toHaveCount(0);
    await expect(dialog.getByTestId('wardrobe-tier-row')).toHaveCount(0);
    await expect(itemRow(dialog, 'Flight Jacket')).toHaveCount(1);
    await shared.check();
    await expect(tier).toHaveCount(1);

    await dialog.getByRole('button', { name: 'Done' }).click();
    await expect(dialog).toBeHidden();
  });

  test('New folder → enter it → "Nothing hangs here yet." → a refused `images` name (ACTIVATE-AT-UNIFY)', async ({
    page,
  }) => {
    test.skip(!P4D279_SERVER_LANDED, 'awaits the folder verbs (P4.D279 C2 + P4.D281; §S.9)');
    test.setTimeout(90_000);
    const dialog = await openAriasWardrobe(page);

    await dialog.getByRole('button', { name: 'New folder', exact: true }).click();
    const name = dialog.getByLabel('New folder name');
    await expect(name).toHaveAttribute(
      'placeholder',
      'Folder name — Enter to make it, Esc to think better of it',
    );
    await name.fill('Formal');
    await name.press('Enter');
    await expect(
      toasts(page).filter({ hasText: 'The folder "Formal" is ready for hanging things in' }),
    ).toBeVisible({ timeout: 10_000 });
    await expect(folderRow(dialog, 'Formal')).toContainText('0 items');

    await folderRow(dialog, 'Formal').getByRole('button', { name: 'Formal', exact: true }).click();
    await expect(crumbs(dialog)).toContainText('Formal');
    await expect(dialog.getByText('Nothing hangs here yet.')).toBeVisible();

    // A refused name: the server's sentence, toasted; the input stays.
    await dialog.getByRole('button', { name: 'New folder', exact: true }).click();
    await dialog.getByLabel('New folder name').fill('images');
    await dialog.getByLabel('New folder name').press('Enter');
    await expect(
      toasts(page).filter({
        hasText: '"images" is reserved for item pictures and cannot be a wardrobe folder',
      }),
    ).toBeVisible({ timeout: 10_000 });
    // Escape closes the input only — the dialog stays.
    await dialog.getByLabel('New folder name').press('Escape');
    await expect(dialog.getByLabel('New folder name')).toHaveCount(0);
    await expect(dialog).toBeVisible();

    // `+ New Item` inside a folder files the new garment THERE (v4
    // `WardrobeBrowser.tsx:679` → the editor's `initialFolder`, `:852`): the
    // row appears in this folder's view, and only there.
    await dialog.getByRole('button', { name: '+ New Item' }).click();
    await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeVisible();
    await page.locator('#wardrobe-title').fill(FOLDER_GARMENT);
    await expect(page.locator('select#wardrobe-folder option:checked')).toHaveText('Formal', {
      timeout: 10_000,
    });
    await page
      .locator('qt-wardrobe-item-editor label')
      .filter({ hasText: /^\s*top\s*$/i })
      .locator('input[type="checkbox"]')
      .first()
      .click();
    await page.getByRole('button', { name: 'Create', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeHidden({
      timeout: 10_000,
    });
    await expect(crumbs(dialog)).toContainText('Formal');
    await expect(itemRow(dialog, FOLDER_GARMENT)).toHaveCount(1, { timeout: 10_000 });
    await expect(dialog.getByText('Nothing hangs here yet.')).toHaveCount(0);
    // At the top level it is the folder that counts it, not a loose row.
    await crumbs(dialog).getByRole('button', { name: 'Wardrobe', exact: true }).click();
    await expect(itemRow(dialog, FOLDER_GARMENT)).toHaveCount(0);
    await expect(folderRow(dialog, 'Formal')).toContainText('1 item');

    await dialog.getByRole('button', { name: 'Done' }).click();
    await expect(dialog).toBeHidden();
  });

  test('Move to folder…: the picker disables (here now), files the item, and a click / Escape inside it never closes the dialog — bug 229 (ACTIVATE-AT-UNIFY)', async ({
    page,
  }) => {
    test.skip(
      !P4D279_SERVER_LANDED || !P4D286_ROW_LANDED,
      'awaits the folder verbs (P4.D279/P4.D281) and the row’s Move to folder… (P4.D286; §S.9)',
    );
    test.setTimeout(90_000);
    const dialog = await openAriasWardrobe(page);

    const jacket = itemRow(dialog, 'Flight Jacket').first();
    await jacket.getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: 'Move to folder…', exact: true }).click();
    const picker = page.getByRole('listbox', { name: 'Folders' });
    await expect(picker).toBeVisible();
    await expect(page.getByText('Move "Flight Jacket" to a folder')).toBeVisible();
    // At the top level already: that row is (here now) and disabled.
    const top = picker.getByRole('option', { name: /top level/ });
    await expect(top).toBeDisabled();
    await expect(top).toContainText('(here now)');
    await expect(page.getByRole('button', { name: 'Move', exact: true })).toBeDisabled();

    // Bug 229: a real click inside the picker leaves the wardrobe open.
    const formal = picker.getByRole('option', { name: /Formal/ });
    await formal.click();
    await expect(dialog).toBeVisible();
    await expect(page.getByRole('button', { name: 'Move', exact: true })).toBeEnabled();
    // …and ONE Escape closes the picker only.
    await page.keyboard.press('Escape');
    await expect(picker).toBeHidden();
    await expect(dialog).toBeVisible();

    // Again, and file it this time.
    await jacket.getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: 'Move to folder…', exact: true }).click();
    await page.getByRole('listbox', { name: 'Folders' }).getByRole('option', { name: /Formal/ }).click();
    await page.getByRole('button', { name: 'Move', exact: true }).click();
    await expect(
      toasts(page).filter({ hasText: '"Flight Jacket" now hangs in "Formal"' }),
    ).toBeVisible({ timeout: 10_000 });
    await expect(itemRow(dialog, 'Flight Jacket')).toHaveCount(0);
    // The New-folder beat's garment already hangs there.
    await expect(folderRow(dialog, 'Formal')).toContainText('2 items');

    // Inside Formal, the picker marks Formal (here now).
    await folderRow(dialog, 'Formal').getByRole('button', { name: 'Formal', exact: true }).click();
    await itemRow(dialog, 'Flight Jacket').first().getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: 'Move to folder…', exact: true }).click();
    await expect(
      page.getByRole('listbox', { name: 'Folders' }).getByRole('option', { name: /Formal/ }),
    ).toBeDisabled();
    await page.getByRole('button', { name: 'Cancel', exact: true }).click();

    await dialog.getByRole('button', { name: 'Done' }).click();
    await expect(dialog).toBeHidden();
  });

  test('Select + the bulk move with the picker’s inline New folder…; Rename; delete-folder moves its items up a peg (ACTIVATE-AT-UNIFY)', async ({
    page,
  }) => {
    test.skip(
      !P4D279_SERVER_LANDED || !P4D286_ROW_LANDED,
      'awaits the folder verbs (P4.D279/P4.D281) and the row’s select checkbox (P4.D286; §S.9)',
    );
    test.setTimeout(90_000);
    const dialog = await openAriasWardrobe(page);

    // Select mode: tick the fixture's Goggles (Aria's own), the sticky bar counts it.
    await dialog.getByRole('button', { name: 'Select', exact: true }).click();
    await dialog.getByLabel('Select Goggles', { exact: true }).check();
    const bar = dialog.getByTestId('wardrobe-select-bar');
    await expect(bar).toContainText('1 selected');
    await bar.getByRole('button', { name: 'Move to folder…' }).click();

    // The picker's inline New folder… makes "Hats" and highlights it.
    await page.getByRole('button', { name: 'New folder…', exact: true }).click();
    const name = page.getByRole('listbox', { name: 'Folders' }).getByLabel('New folder name');
    await name.fill('Hats');
    await page.getByRole('button', { name: 'Create', exact: true }).click();
    await expect(
      page.getByRole('listbox', { name: 'Folders' }).getByRole('option', { name: /Hats/ }),
    ).toHaveAttribute('aria-selected', 'true', { timeout: 10_000 });
    await page.getByRole('button', { name: 'Move', exact: true }).click();
    await expect(
      toasts(page).filter({ hasText: '"Goggles" now hangs in "Hats"' }),
    ).toBeVisible({ timeout: 10_000 });
    // The move ends select mode.
    await expect(bar).toHaveCount(0);

    // Inline Rename: Hats → Chapeaux.
    const hats = folderRow(dialog, 'Hats');
    await hats.getByRole('button', { name: 'Folder actions for Hats' }).click();
    await hats.getByRole('menuitem', { name: 'Rename' }).click();
    const rename = dialog.getByLabel('Rename folder Hats');
    await rename.fill('Chapeaux');
    await rename.press('Enter');
    await expect(toasts(page).filter({ hasText: 'Renamed to "Chapeaux"' })).toBeVisible({
      timeout: 10_000,
    });

    // Delete the folder: the confirmation says where its item goes.
    let asked = '';
    page.once('dialog', (d) => {
      asked = d.message();
      void d.accept();
    });
    const chapeaux = folderRow(dialog, 'Chapeaux');
    await chapeaux.getByRole('button', { name: 'Folder actions for Chapeaux' }).click();
    await chapeaux.getByRole('menuitem', { name: 'Delete' }).click();
    await expect(
      toasts(page).filter({ hasText: 'Folder removed; 1 item moved up a peg' }),
    ).toBeVisible({ timeout: 10_000 });
    expect(asked).toBe(
      'Send its 1 item (and any archived pieces filed there) up to the top of the wardrobe? Only the folder itself goes.',
    );
    await expect(itemRow(dialog, 'Goggles')).toHaveCount(1);

    await dialog.getByRole('button', { name: 'Done' }).click();
    await expect(dialog).toBeHidden();
  });

  test('a search looks everywhere and shows each hit’s folder crumb (ACTIVATE-AT-UNIFY)', async ({
    page,
  }) => {
    test.skip(
      !P4D279_SERVER_LANDED || !P4D286_ROW_LANDED,
      'awaits the folder verbs (P4.D279/P4.D281) and the row’s folder crumb (P4.D286; §S.9)',
    );
    test.setTimeout(90_000);
    const dialog = await openAriasWardrobe(page);
    // The Move beat filed Flight Jacket into Formal.
    await dialog.getByLabel('Search wardrobe by title').fill('Jacket');
    await expect(crumbs(dialog)).toHaveText('Searching everywhere');
    await expect(dialog.getByTestId('wardrobe-folder-row')).toHaveCount(0);
    await expect(
      itemRow(dialog, 'Flight Jacket').getByTestId('wardrobe-folder-crumb'),
    ).toHaveText('Formal');
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
    `wardrobe folders server did not become ready within 30s (${lastErr}); see ${SERVER_LOG}`,
  );
}
