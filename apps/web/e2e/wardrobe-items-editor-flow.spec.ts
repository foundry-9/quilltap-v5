import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, mkdirSync, openSync, rmSync, writeFileSync } from 'node:fs';
import { crc32, deflateSync } from 'node:zlib';
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
 * P4.D286 — the wardrobe item editor, row and picture cleanup at the
 * `4a38222e6` pin, walked in a real browser against its OWN server + instance
 * (port 4337, the round's §R.9 allocation) over the committed `characters-*`
 * pair (Aria). The instance recipe is `wardrobe-flow.spec.ts`'s verbatim: copy
 * the pair, write the locked `.dbkey`, materialize `instance_settings` (so the
 * built-in stores — Quilltap General — provision at boot) and the empty
 * `projects` / `groups` tables, rewrite the fixture user to SINGLE_USER_ID,
 * boot `quilltap-web`. No LLM call happens here.
 *
 * Four beats:
 *  - the Folder field's **New folder…** files a new garment under
 *    `Formal/Evening`, and the editor reopens on `Formal › Evening`;
 *  - **Move to folder…** is on a manageable row's kebab and absent from a
 *    borrowed one;
 *  - bug 190: an outfit whose part was archived lists the part
 *    ` · archived` and keeps its slot checked and locked;
 *  - bug 194: a picture whose bytes will not load is removed through the
 *    wardrobe images route from the viewer's placeholder.
 */

/**
 * ACTIVATE-AT-UNIFY (P4.D286 → P4.D279 / P4.D281 + P4.D285). The two folder
 * beats need the round's server half — the `wardrobeFoldersList` verb and
 * `folder` honoured on create (P4.D279's location + P4.D281's route arms) —
 * and, for Move to folder…, P4.D285's browser binding the row's
 * `canMoveToFolder` (the wardrobe dialog on this branch binds none). Against a
 * pre-round server the field lists no folders and the create files at the
 * root, so both beats would fail for a reason that says nothing about this
 * lane. Flip to `true` at unification (the order's §S.9). The bug-190 and
 * bug-194 beats need only verbs already on `main` and run LIVE.
 */
const P4D279_SERVER_LANDED = true;

const PORT = 4337;
const BASE_URL = `http://127.0.0.1:${PORT}`;
const INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'wardrobe-items-editor-instance');
const DATA_DIR = resolve(INSTANCE_DIR, 'data');
const SERVER_LOG = resolve(ARTIFACTS_DIR, 'wardrobe-items-editor-server.log');

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

/**
 * A fresh 1×1 RGB PNG of the given colour (`wardrobe-flow.spec.ts`'s helper):
 * the images route dedups by sha, so each beat hangs a picture it has never
 * seen.
 */
function onePixelPng(r: number, g: number, b: number): Buffer {
  const chunk = (type: string, data: Buffer): Buffer => {
    const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(body) >>> 0);
    return Buffer.concat([len, body, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(1, 0);
  ihdr.writeUInt32BE(1, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(Buffer.from([0, r, g, b]))),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

let server: ChildProcess | undefined;

test.describe('P4.D286 — the wardrobe item editor, row and picture cleanup', () => {
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
    // Instance MATERIALIZATION, not a fixture regen — `wardrobe-flow.spec.ts`'s
    // three tables, verbatim (see its beforeAll for the measurements): the
    // committed pair is untouched.
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
        String(PORT),
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

  async function openAriaWardrobe(page: Page): Promise<void> {
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
    await expect(page.getByRole('dialog').locator('.qt-dialog-title')).toHaveText('Wardrobe');
  }

  async function ariaId(page: Page): Promise<string> {
    const chars = ((await dispatch(page, { type: 'characterList' }))['characters'] ?? []) as Array<{
      id: string;
      name: string;
    }>;
    const aria = chars.find((c) => c.name === 'Aria');
    expect(aria?.id, 'Aria’s id').toBeTruthy();
    return aria!.id;
  }

  async function createGarment(
    page: Page,
    characterId: string,
    title: string,
    types: string[],
    componentItemIds: string[] = [],
  ): Promise<string> {
    const created = (
      await dispatch(page, {
        type: 'characterWardrobeCreate',
        characterId,
        item: {
          title,
          description: null,
          imagePrompt: null,
          types,
          appropriateness: null,
          isDefault: false,
          componentItemIds,
          replace: false,
        },
      })
    )['wardrobeItem'] as { id?: string } | undefined;
    expect(created?.id, `the id of ${title}`).toBeTruthy();
    return created!.id!;
  }

  function rowOf(page: Page, title: string) {
    return page.getByRole('dialog').locator('qt-wardrobe-item-row').filter({ hasText: title }).first();
  }

  /** The button that enters a folder row in the browser (`wardrobe-folders-flow.spec.ts`'s idiom). */
  function folderButton(page: Page, name: string) {
    return page
      .getByRole('dialog')
      .getByTestId('wardrobe-folder-row')
      .filter({ hasText: name })
      .getByRole('button', { name, exact: true });
  }

  test('New folder… files a new garment under Formal/Evening, and Edit reopens on it (ACTIVATE-AT-UNIFY, P4.D279)', async ({
    page,
  }) => {
    test.skip(
      !P4D279_SERVER_LANDED,
      'awaits the round’s `wardrobeFoldersList` verb + `folder` on create (P4.D279/P4.D281) — flipped at unification',
    );
    test.setTimeout(90_000);
    const title = `Opera Gloves ${Date.now()}`;
    await page.goto(`${BASE_URL}/characters`);
    await unlockIfLocked(page);
    await openAriaWardrobe(page);

    await page.getByRole('button', { name: '+ New Item' }).click();
    await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeVisible();
    await page.locator('#wardrobe-title').fill(title);
    const folder = page.locator('select#wardrobe-folder');
    await expect(folder.locator('option').first()).toHaveText('Wardrobe (top level)');
    await folder.selectOption({ label: 'New folder…' });
    const path = page.locator('input#wardrobe-folder');
    await expect(path).toHaveAttribute('placeholder', 'e.g., Formal/Evening');
    await path.fill('Formal/Evening');
    await page
      .locator('label')
      .filter({ hasText: 'Accessories' })
      .locator('input[type="checkbox"]')
      .click();
    await page.getByRole('button', { name: 'Create', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeHidden();

    // Under the folder browser a row lives only in its own folder's view (v4
    // `WardrobeBrowser.tsx:854-857`): walk into Formal › Evening to reach it.
    await folderButton(page, 'Formal').click();
    await folderButton(page, 'Evening').click();
    await expect(
      page.getByRole('dialog').getByRole('navigation', { name: 'Wardrobe folders' }),
    ).toContainText('Evening');
    await rowOf(page, title).getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: 'Edit' }).click();
    await expect(page.getByRole('heading', { name: 'Edit Wardrobe Item' })).toBeVisible();
    await expect(folder.locator('option:checked')).toHaveText('Formal › Evening', {
      timeout: 10_000,
    });
    await page.getByRole('button', { name: 'Cancel', exact: true }).click();
    await page.getByRole('button', { name: 'Done' }).click();
  });

  test('Move to folder… is on a manageable row and absent from a borrowed one (ACTIVATE-AT-UNIFY, P4.D279 + P4.D285)', async ({
    page,
  }) => {
    test.skip(
      !P4D279_SERVER_LANDED,
      'awaits the folder verbs and P4.D285’s browser binding the row’s Move to folder… — flipped at unification',
    );
    test.setTimeout(90_000);
    const own = `Spats ${Date.now()}`;
    const borrowed = `General Boater ${Date.now()}`;
    await page.goto(`${BASE_URL}/characters`);
    await unlockIfLocked(page);
    await createGarment(page, await ariaId(page), own, ['footwear']);
    const general = (
      await dispatch(page, {
        type: 'wardrobeCreate',
        item: {
          title: borrowed,
          description: null,
          imagePrompt: null,
          types: ['accessories'],
          appropriateness: null,
          isDefault: false,
          componentItemIds: [],
          replace: false,
        },
      })
    )['wardrobeItem'] as { id?: string } | undefined;
    expect(general?.id, 'the General garment').toBeTruthy();
    await openAriaWardrobe(page);

    await rowOf(page, own).getByRole('button', { name: 'More actions' }).click();
    await expect(page.getByRole('menuitem', { name: 'Move to folder…' })).toBeVisible();
    await expect(page.getByRole('menuitem', { name: 'Move to another wardrobe…' })).toBeVisible();
    await page.keyboard.press('Escape');

    // The borrowed garment sits behind the General tier row (v4 `4a38222e6`
    // `WardrobeBrowser.tsx:854-857`) — enter the tier first.
    await page.getByRole('dialog').getByTestId('wardrobe-tier-row').filter({ hasText: 'General' }).click();
    await rowOf(page, borrowed).getByRole('button', { name: 'More actions' }).click();
    await expect(page.getByRole('menuitem', { name: 'Copy to another wardrobe…' })).toBeVisible();
    await expect(page.getByRole('menuitem', { name: 'Move to folder…' })).toHaveCount(0);
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: 'Done' }).click();
  });

  test('bug 190: an outfit whose part was archived lists the part “· archived” and keeps its slot', async ({
    page,
  }) => {
    test.setTimeout(90_000);
    const stamp = Date.now();
    const shirt = `Dress Shirt ${stamp}`;
    const hat = `Old Topper ${stamp}`;
    const kit = `Promenade Kit ${stamp}`;
    await page.goto(`${BASE_URL}/characters`);
    await unlockIfLocked(page);
    const characterId = await ariaId(page);
    const shirtId = await createGarment(page, characterId, shirt, ['top']);
    const hatId = await createGarment(page, characterId, hat, ['accessories']);
    await createGarment(page, characterId, kit, ['top', 'accessories'], [shirtId, hatId]);
    const archived = await dispatch(page, {
      type: 'characterWardrobeUpdate',
      characterId,
      itemId: hatId,
      item: { archived: true },
    });
    expect((archived['wardrobeItem'] as { archivedAt?: string | null })?.archivedAt).toBeTruthy();

    await openAriaWardrobe(page);
    // A composite lists under the browser's Outfits tab, not Items.
    await page.getByRole('dialog').getByText('Outfits', { exact: true }).click();
    await rowOf(page, kit).getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: 'Edit' }).click();
    await expect(page.getByRole('heading', { name: 'Edit Wardrobe Item' })).toBeVisible();

    // The archived part is still listed — flagged — so it can be taken out.
    const hatCandidate = page.locator('ul li label').filter({ hasText: hat });
    await expect(hatCandidate).toContainText('Accessories · archived', { timeout: 10_000 });
    await expect(hatCandidate.locator('input[type="checkbox"]')).toBeChecked();
    // Its slot stays covered: checked and locked by the component.
    const accessories = page
      .locator('qt-slot-checkbox-group label')
      .filter({ hasText: 'Accessories' });
    await expect(accessories).toHaveAttribute('title', 'Covered by a component — always included');
    await expect(accessories.locator('input')).toBeChecked();
    await expect(accessories.locator('input')).toBeDisabled();
    await expect(page.getByText('Slots this outfit covers (its components’ slots are always included):')).toBeVisible();
    await page.getByRole('button', { name: 'Cancel', exact: true }).click();
    await page.getByRole('button', { name: 'Done' }).click();
  });

  test('bug 194: a picture whose bytes will not load is removed through the wardrobe images route', async ({
    page,
  }) => {
    test.setTimeout(120_000);
    const title = `Moth-eaten Stole ${Date.now()}`;
    await page.goto(`${BASE_URL}/characters`);
    await unlockIfLocked(page);
    const characterId = await ariaId(page);
    const itemId = await createGarment(page, characterId, title, ['accessories']);
    await openAriaWardrobe(page);

    // Hang a picture through the editor's Picture section.
    await rowOf(page, title).getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: 'Edit' }).click();
    await expect(page.getByTestId('wardrobe-item-image-section')).toContainText('No picture yet', {
      timeout: 10_000,
    });
    await page.getByLabel('Upload a picture').setInputFiles({
      name: 'stole.png',
      mimeType: 'image/png',
      buffer: onePixelPng(Date.now() % 256, 91, 47),
    });
    await expect(page.getByTestId('wardrobe-item-image-history-entry')).toHaveCount(1, {
      timeout: 15_000,
    });
    await page.getByRole('button', { name: 'Cancel', exact: true }).click();

    const listed = await dispatch(page, {
      type: 'wardrobeItemImagesList',
      scope: 'character',
      containerId: characterId,
      itemId,
    });
    const fileId = listed['current'] as string;
    expect(fileId, 'the hung picture').toBeTruthy();

    // The full-size bytes are gone (the thumbnail still loads): the viewer
    // swaps in the placeholder.
    await page.route(
      (url) => url.pathname === `/api/v1/files/${fileId}` && url.search === '',
      (route) => route.fulfill({ status: 404, body: 'gone' }),
    );
    const refusals: string[] = [];
    page.on('response', (res) => {
      if (res.url().includes('/api/v1/images/')) refusals.push(res.url());
    });
    await rowOf(page, title).getByRole('button', { name: `View the picture of ${title}` }).click();
    const viewer = page.getByRole('dialog', { name: `Picture of ${title}` });
    await expect(viewer.getByText('Image Deleted')).toBeVisible({ timeout: 10_000 });
    page.once('dialog', (d) => void d.accept());
    await viewer.getByRole('button', { name: 'Remove' }).click();
    await expect(viewer).toHaveCount(0, { timeout: 10_000 });

    // Removed through the wardrobe route: the item has no picture now, and
    // the generic image DELETE (a standing v5 refusal) was never asked.
    const after = await dispatch(page, {
      type: 'wardrobeItemImagesList',
      scope: 'character',
      containerId: characterId,
      itemId,
    });
    expect(after['current'] ?? null).toBeNull();
    expect((after['images'] as unknown[] | undefined) ?? []).toHaveLength(0);
    expect(refusals).toEqual([]);
    await expect(
      page.locator('[role="toast-container"] > div').filter({ hasText: /not yet available/i }),
    ).toHaveCount(0);
    await page.getByRole('button', { name: 'Done' }).click();
  });
});

async function dispatch(page: Page, req: unknown): Promise<Record<string, unknown>> {
  const res = await page.request.post(`${BASE_URL}/api/dispatch`, { data: req });
  const body = (await res.json().catch(() => null)) as { data?: Record<string, unknown> } | null;
  return body?.data ?? {};
}

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
    `wardrobe items-editor server did not become ready within 30s (${lastErr}); see ${SERVER_LOG}`,
  );
}
