import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, mkdirSync, openSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { expect, test, request as pwRequest, type Page } from './support/fixtures';

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
 * P4.9f2 — browser walk of the wardrobe control dialog. The out-of-chat arm
 * needs only LANDED verbs (`characterWardrobe*`, `characterList`,
 * `imageProfileList`) and runs LIVE in this lane: unlock → Aria's detail →
 * the Wardrobe tab's (now enabled) button opens the dialog → create an item
 * through the editor → it appears in the character tier → mark it default →
 * compose it into the Outfit Builder (pure client state) → close → reopen and
 * the item persists.
 *
 * The in-chat staging + one-`set_all` flush arm needs lane P4.9f1's
 * `chatOutfitGet`/`chatEquip` and is ACTIVATE-AT-UNIFY: it probes the dispatch
 * surface and self-activates when the wardrobe server half lands (the
 * general-files / courier probe precedent).
 *
 * Runs against its OWN locked server + instance dir over the committed
 * `characters-*` fixture pair (Aria + her "Solo Voyage" chat) — the recipe
 * mirrors `characters-flow.spec.ts`: copy the fixture pair, write the locked
 * .dbkey, rewrite the fixture user id to SINGLE_USER_ID via
 * `quilltap db --write` BEFORE launch, then boot quilltap-web on a
 * spec-private port. No LLM send happens here, so no mock LLM is needed.
 */

const WARDROBE_PORT = 4329;
const BASE_URL = `http://127.0.0.1:${WARDROBE_PORT}`;
const INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'wardrobe-instance');
const DATA_DIR = resolve(INSTANCE_DIR, 'data');
const SERVER_LOG = resolve(ARTIFACTS_DIR, 'wardrobe-server.log');

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

test.describe('P4.9f2 — the wardrobe control dialog', () => {
  test.beforeAll(async () => {
    // Each `quilltap db --write` unwraps the .dbkey via PBKDF2 (≈5s); the
    // rewrites plus server boot need more than the default per-hook timeout.
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
        String(WARDROBE_PORT),
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

  /** Unlock only when the passphrase screen is showing. */
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

  /** Open Aria's detail view from the roster (card BODY click — finding #4). */
  async function openAriaDetail(page: Page): Promise<void> {
    const aria = page
      .locator('.character-card-grid .character-card')
      .filter({ hasText: 'Aria' })
      .first();
    await expect(aria).toBeVisible();
    await aria.locator('p.line-clamp-3').click();
    await expect(page.getByRole('heading', { name: 'Aria' })).toBeVisible();
  }

  /** Open the wardrobe dialog from the detail view's Wardrobe tab. Scoped to
   *  the detail view: the shell footer button shares the accessible name
   *  "Wardrobe" (the added-affordance locator trap). */
  async function openWardrobeDialog(page: Page): Promise<void> {
    await page
      .locator('.character-view')
      .getByRole('button', { name: 'Wardrobe', exact: true })
      .click();
    // v4's own string — the P4.9f2 stub retirement: the button is enabled.
    await page.getByRole('button', { name: 'Open wardrobe for Aria' }).click();
    await expect(page.getByRole('dialog').locator('.qt-dialog-title')).toHaveText('Wardrobe');
  }

  test('detail → Wardrobe tab → create item → mark default → compose → persists across reopen', async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await page.goto(`${BASE_URL}/characters`);
    await unlockIfLocked(page);
    await openAriaDetail(page);
    await openWardrobeDialog(page);

    // Aria is preselected in the header dropdown (the dialog opened with her
    // characterId in context).
    await expect(page.locator('#wardrobe-char-select')).toHaveValue(/./);

    // Create an item through the editor (characterWardrobeCreate — landed).
    await page.getByRole('button', { name: '+ New Item' }).click();
    await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeVisible();
    await page.locator('#wardrobe-title').fill('Brass Goggles');
    // Type(s): tick "accessories" (single-garment mode).
    // hasText is a substring match; among the four Type(s) labels only one
    // contains "accessories" (regex here would trip on un-normalized
    // whitespace — the P4.9a memory).
    await page
      .locator('label')
      .filter({ hasText: 'accessories' })
      .locator('input[type="checkbox"]')
      .click();
    await page.getByRole('button', { name: 'Create', exact: true }).click();

    // The editor closes and the item lands in the character tier list.
    await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeHidden();
    const gogglesRow = page
      .locator('.qt-card-interactive')
      .filter({ hasText: 'Brass Goggles' })
      .first();
    await expect(gogglesRow).toBeVisible();

    // Mark it default via the kebab (characterWardrobeUpdate — landed).
    await gogglesRow.getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: /Mark as default outfit item/ }).click();
    await expect(gogglesRow).toContainText('· default', { timeout: 10_000 });

    // Compose it into the Outfit Builder (pure client state — out of chat no
    // equip call ever fires; the dialog is on the Builder tab by default).
    const accessoriesRow = page.locator('.qt-card').filter({ hasText: 'Accessories' }).first();
    await accessoriesRow.getByRole('button', { name: '+', exact: true }).click();
    await accessoriesRow.getByRole('button', { name: /Brass Goggles/ }).click();
    await expect(accessoriesRow).toContainText('Brass Goggles');

    // Close, reopen: the item persisted server-side.
    await page.getByRole('button', { name: 'Done' }).click();
    await expect(page.getByRole('dialog')).toBeHidden();
    await page.getByRole('button', { name: 'Open wardrobe for Aria' }).click();
    await expect(
      page.locator('.qt-card-interactive').filter({ hasText: 'Brass Goggles' }).first(),
    ).toContainText('· default', { timeout: 10_000 });
    await page.getByRole('button', { name: 'Done' }).click();
  });

  test('out of chat: Preview reaches the no-API-key badRequest (P4.6bf; live render is a dogfood item)', async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await page.goto(`${BASE_URL}/characters`);
    await unlockIfLocked(page);
    await openAriaDetail(page);
    await openWardrobeDialog(page);

    // Out of chat the dialog opens on the Outfit Builder tab, which hosts the
    // avatar-generation pane + its Preview button (v4 :568 — rightTab defaults
    // to 'builder' out of chat). The one committed image profile has
    // apiKeyId=null, so the (enabled) Preview button reaches v4's PRE-provider
    // badRequest — this asserts the render seam is reached with ZERO
    // image-provider spend.
    //
    // The LIVE render walk (P4.6bf wired the HostAvatarPreviewRenderer, so a
    // keyed profile makes Preview cost real money) is deliberately NOT an e2e
    // beat: the shared e2e instance has no live image provider, and standing up
    // a canned localhost provider endpoint for one beat is disproportionate.
    // It is recorded as a DOGFOOD item (the P4.6bf lane record) instead.
    const dialog = page.getByRole('dialog');
    const preview = dialog.getByRole('button', { name: 'Preview', exact: true });
    await expect(preview).toBeEnabled();
    await preview.click();

    // v4 `preview-avatar/route.ts`: `!imageProfile.apiKeyId` →
    // badRequest('Selected image profile has no API key configured'). v4 reports
    // it as a toast and the dialog carries no banner (P4.25).
    await expect(
      page
        .locator('[role="toast-container"] > div')
        .filter({ hasText: 'Selected image profile has no API key configured' }),
    ).toBeVisible({ timeout: 15_000 });

    await dialog.getByRole('button', { name: 'Done' }).click();
    await expect(dialog).toBeHidden();
  });

  test('in chat: staging + the one-shot set_all flush persists the outfit (ACTIVATE-AT-UNIFY, lane P4.9f1)', async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await page.goto(`${BASE_URL}/characters`);
    await unlockIfLocked(page);

    // Probe lane P4.9f1's dispatch surface (post-unlock — a locked server
    // answers 423). "unknown variant" → the wardrobe server half is not on
    // this build; the beat self-activates at unification.
    let equipReady = false;
    const ctx = await pwRequest.newContext();
    try {
      const res = await ctx.post(`${BASE_URL}/api/dispatch`, {
        data: { type: 'chatOutfitGet', chatId: '00000000-0000-0000-0000-000000000000' },
      });
      const body = (await res.json().catch(() => null)) as {
        type?: string;
        data?: { message?: string };
      } | null;
      const isUnknownVariant =
        body?.type === 'error' && /unknown variant/i.test(String(body?.data?.message ?? ''));
      equipReady = body != null && !isUnknownVariant;
    } catch {
      equipReady = false;
    } finally {
      await ctx.dispose();
    }
    test.skip(
      !equipReady,
      "ACTIVATE-AT-UNIFY (lane P4.9f1): chatOutfitGet/chatEquip not live — the in-chat staging + Done flush walk self-activates when the wardrobe server half lands",
    );

    // Ensure the item from the live beat exists (this file runs serially; the
    // live beat created Brass Goggles). Reach Aria's chat via her
    // Conversations tab so no chat id is hard-coded.
    await openAriaDetail(page);
    await page.getByRole('button', { name: 'Conversations' }).click();
    await page.getByRole('link', { name: /Solo Voyage/ }).click();
    await expect(page).toHaveURL(/\/salon\//);

    // The shell footer Wardrobe button carries the chat scope (and resolves
    // Aria as the default character). getByTitle: the footer button is the
    // only "Wardrobe"-titled control (the detail tab has no title attr).
    await page.getByTitle('Wardrobe', { exact: true }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog.locator('.qt-dialog-title')).toHaveText('Wardrobe');
    // In chat the Live tab is the default (v4 :204) and carries the staging
    // microcopy.
    await expect(dialog.getByText('Edits stage here and apply when you click Done.')).toBeVisible();

    // Stage: Wear the goggles (a pure client-side staging gesture).
    const gogglesRow = dialog
      .locator('.qt-card-interactive')
      .filter({ hasText: 'Brass Goggles' })
      .first();
    await gogglesRow.getByRole('button', { name: 'Wear', exact: true }).click();

    // Done → ONE set_all flush per dirty character, then the dialog closes.
    await page.getByRole('button', { name: 'Done' }).click();
    await expect(dialog).toBeHidden({ timeout: 10_000 });

    // Reopen: the remounted dialog seeds the Live tab from the server's worn
    // snapshot — the flush persisted.
    await page.getByTitle('Wardrobe', { exact: true }).click();
    const accessoriesRow = page
      .getByRole('dialog')
      .locator('.qt-card')
      .filter({ hasText: 'Accessories' })
      .first();
    await expect(accessoriesRow).toContainText('Brass Goggles', { timeout: 10_000 });
    await page.getByRole('button', { name: 'Done' }).click();
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
    `wardrobe server did not become ready within 30s (${lastErr}); see ${SERVER_LOG}`,
  );
}
