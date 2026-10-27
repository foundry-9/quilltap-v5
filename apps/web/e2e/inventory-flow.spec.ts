import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, mkdirSync, openSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { expect, test, type Page } from './support/fixtures';

import { makeDbKeyFile } from './support/dbkey';
import {
  ARTIFACTS_DIR,
  cliBinary,
  E2E_PASSPHRASE,
  FIXTURE_USER,
  FIXTURES_DIR,
  REPO_ROOT,
  SINGLE_USER_ID,
  spaDir,
  TEST_PEPPER,
  webBinary,
} from './support/env';
import { MOCK_LLM_REPLY, startMockLlm, type MockLlm } from './support/mock-llm';
import { openSidebarSection } from './support/sidebar';

/**
 * P4.D330 — the Inventory SPA (v4 `4fbdad186` + `e1872c94f` + `102a23648` +
 * `e27fb6b34`, and the `/photos` album selector's client half of `680b71044`,
 * pinned at `322a5488e`).
 *
 * Its OWN instance on port 4348 (mock LLM 45346 — the round's §R.9 ports for
 * this lane), over the `characters-main.db` / `characters-mount.db` pair (the
 * `wardrobe-flow.spec.ts` instance: vaulted characters incl. Aria, the userId
 * rewrite, the mock-LLM base URL), with `instance_settings`, `projects`,
 * `groups`, `chat_settings` and `group_doc_mount_links` materialized from the
 * committed D23 dump (the `workbench-wardrobe-picker-flow.spec.ts` recipe).
 * Aria's chat `Solo Voyage` is the in-chat beat's room.
 *
 * **LIVE beats** need only `main`'s verbs: (a) the rail's Inventory button →
 * `The Inventory` tab with the four catalogue groups (`characterList`,
 * `projectList`, `groupList`); (b) the Prospero card's header `Inventory (0)`
 * on a project made here (`projectCreate`); (c) the `/photos` album select's
 * `My Photos` option and the roll-up blurb.
 *
 * **GATED beats** — flipped `true` at the `322a5488e` unification (§S.9, R-V).
 * NAMED constants, never a capability probe. Each gated beat calls
 * `test.skip(…)` first thing. Beat (g) stays skipped on its own premise: the
 * shared mock LLM answers text only, so no `inventory_pick_up` turn can be posed.
 */
// ACTIVATE-AT-UNIFY: P4.D324's `INVENTORY` — the inventory module, the columns, the pool.
const P4D324_SERVER_LANDED = true;
// ACTIVATE-AT-UNIFY: P4.D325's routes — the `inventory*` / `chatInventory*` dispatch family (C7).
const P4D325_SERVER_LANDED = true;
// ACTIVATE-AT-UNIFY: P4.D328's nine tools + the turn container (the Salon `inventory_pick_up` row).
const P4D328_SERVER_LANDED = true;
// ACTIVATE-AT-UNIFY: P4.D338's photos half — `photoGalleryAlbums` + `photoGalleryList.album` (C9).
const P4D338_SERVER_LANDED = true;
const GATE_REASON =
  'awaits the Inventory server lanes — P4.D324’s module, P4.D325’s `inventory*` / `chatInventory*` verbs, P4.D328’s tools — wired at unification';
const PHOTOS_GATE_REASON = 'awaits P4.D338’s `photoGalleryAlbums` and the list request’s `album` key — wired at unification';

const INV_PORT = 4348;
const INV_BASE_URL = `http://127.0.0.1:${INV_PORT}`;
/** This spec's own mock LLM — clear of every shared port. */
const INV_MOCK_LLM_PORT = 45346;
const INV_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'inventory-instance');
const INV_DATA_DIR = resolve(INV_INSTANCE_DIR, 'data');
const INV_SERVER_LOG = resolve(ARTIFACTS_DIR, 'inventory-server.log');

const SOLO_VOYAGE_CHAT_ID = 'c1000000-0000-4000-8000-000000000001';

let server: ChildProcess | undefined;
let mock: MockLlm | undefined;

test.use({ baseURL: INV_BASE_URL });

test.describe('P4.D330 — the Inventory SPA, the /photos album selector and the inventory icon', () => {
  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(INV_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(INV_DATA_DIR, { recursive: true });
    mkdirSync(resolve(INV_DATA_DIR, 'files'), { recursive: true });
    copyFileSync(resolve(FIXTURES_DIR, 'characters-main.db'), resolve(INV_DATA_DIR, 'quilltap.db'));
    copyFileSync(
      resolve(FIXTURES_DIR, 'characters-mount.db'),
      resolve(INV_DATA_DIR, 'quilltap-mount-index.db'),
    );
    writeFileSync(resolve(INV_DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));

    // Instance MATERIALIZATION from the committed D23 dump, never a fixture
    // regen (the workbench-wardrobe-picker recipe): the pair predates
    // `instance_settings` and the projects / groups schema.
    const freshSchema = JSON.parse(
      readFileSync(
        resolve(REPO_ROOT, 'crates/quilltap-core/src/services/provisioning/fresh_schema.json'),
        'utf8',
      ),
    ) as { main: string[]; mountIndex: string[] };
    for (const [partition, table] of [
      ['main', 'instance_settings'],
      ['main', 'projects'],
      ['main', 'groups'],
      ['main', 'chat_settings'],
      ['mountIndex', 'group_doc_mount_links'],
    ] as const) {
      const create = freshSchema[partition].find((sql) => sql.startsWith(`CREATE TABLE "${table}"`));
      if (!create) throw new Error(`fresh_schema.json carries no ${table} CREATE TABLE`);
      const args = [
        'db',
        '--data-dir',
        INV_INSTANCE_DIR,
        '--write',
        create.replace('CREATE TABLE', 'CREATE TABLE IF NOT EXISTS'),
      ];
      if (partition === 'mountIndex') args.splice(1, 0, '--mount-points');
      const res = spawnSync(cli, args, {
        env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
        encoding: 'utf8',
      });
      if (res.status !== 0) throw new Error(`${table} plant failed:\n${res.stdout}\n${res.stderr}`);
    }
    for (const table of ['characters', 'chats', 'tags', 'api_keys', 'connection_profiles', 'image_profiles', 'files']) {
      runCliWrite(cli, `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`);
    }
    runCliWrite(
      cli,
      `UPDATE connection_profiles SET baseUrl = 'http://127.0.0.1:${INV_MOCK_LLM_PORT}/v1' WHERE provider = 'OPENAI_COMPATIBLE';`,
    );
    // Beat (i)'s planted tool row — an `inventory_pick_up` result the Salon renders.
    runCliWrite(
      cli,
      `INSERT INTO chat_messages (id, chatId, type, role, content, createdAt) VALUES (` +
        `'d3300000-0000-4000-8000-000000000001', '${SOLO_VOYAGE_CHAT_ID}', 'message', 'TOOL', ` +
        `'{"toolName":"inventory_pick_up","success":true,"result":{"item":{"id":"i1","name":"unicorn horn","title":null},"archetype":{"id":"a1","name":"unicorn horn","title":null},"equipped":true}}', ` +
        `'2026-02-02T00:00:00.000Z');`,
    );
    mock = await startMockLlm(MOCK_LLM_REPLY, INV_MOCK_LLM_PORT);

    const logFd = openSync(INV_SERVER_LOG, 'w');
    server = spawn(
      web,
      ['--host', '127.0.0.1', '--port', String(INV_PORT), '--data-dir', INV_INSTANCE_DIR, '--spa-dir', spaDir()],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    server.unref();
    await waitForHealth();
    await dispatch({ type: 'unlock', passphrase: E2E_PASSPHRASE });
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
    await mock?.close();
    // The server may still be flushing its file log as it exits.
    rmSync(INV_INSTANCE_DIR, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  });

  // -------------------------------------------------------------------------
  // LIVE
  // -------------------------------------------------------------------------
  test('(a) the rail’s Inventory button opens The Inventory tab with the four catalogue groups', async ({ page }) => {
    // `support/fixtures` opts every context OUT of the tabbed workspace (route
    // mode); this beat is about the workspace tab, so its page opts back in —
    // a page init script runs after the context's.
    await page.addInitScript(() => window.localStorage.removeItem('quilltap.workspace.tabs'));
    await page.goto('/workspace');
    await maybeUnlock(page);
    // Unlocking lands on the shell's root; the rail's tab arm keys on the
    // `/workspace` path (the Wardrobe entry's rule), so go there explicitly.
    await page.goto('/workspace');
    await expect(page).toHaveURL(/\/workspace/);
    await page.getByTitle('Inventory', { exact: true }).click();
    await expect(page.getByRole('tab', { name: /The Inventory/ }).first()).toBeVisible({ timeout: 15_000 });
    const select = page.locator('#inventory-container-select');
    await expect(select).toBeVisible({ timeout: 15_000 });
    await expect(select.locator('optgroup')).toHaveCount(2, { timeout: 15_000 });
    const groups = await select.locator('optgroup').evaluateAll((els) =>
      els.map((g) => (g as HTMLOptGroupElement).label),
    );
    // The pair carries characters and no projects / groups (the tables are planted empty).
    expect(groups).toEqual(['Characters', 'General']);
    await expect(select.locator('optgroup[label="Characters"] option', { hasText: 'Aria' })).toHaveCount(1);
    await expect(select.locator('optgroup[label="General"] option')).toHaveText(['Quilltap General']);
  });

  test('(b) the Prospero card reads Inventory (0) on a project', async ({ page }) => {
    const made = (await dispatch({ type: 'projectCreate', project: { name: 'E2E Inventory Project' } })) as {
      type?: string;
      data?: { project?: { id?: string } };
    };
    const projectId = made.data?.project?.id;
    expect(projectId, `projectCreate answered ${JSON.stringify(made)}`).toBeTruthy();
    await page.goto(`/prospero/${projectId}`);
    await maybeUnlock(page);
    // The header only: the body's list reads the server's `inventoryList` (gated).
    await expect(page.getByText(/^Inventory \(\d+\)$/)).toBeVisible({ timeout: 15_000 });
    await expect(page.getByText('Objects, kits and archetypes every character in this project can carry')).toBeVisible();
  });

  test('(c) the Photos page offers My Photos in its album selector, with the roll-up blurb', async ({ page }) => {
    await page.goto('/photos');
    await maybeUnlock(page);
    const select = page.getByLabel('Choose which album to show');
    await expect(select).toBeVisible({ timeout: 15_000 });
    await expect(select.locator('option').first()).toHaveText('My Photos');
    await expect(page.locator('h1.qt-page-title')).toHaveText('My Photos');
    await expect(
      page.getByText(
        'Every image you’ve kept, gathered from each project album, Quilltap General, and your private uploads — listed once and linked to wherever it lives. A character’s own album is a separate cabinet; choose it above to look inside.',
      ),
    ).toBeVisible();
  });

  // -------------------------------------------------------------------------
  // GATED
  // -------------------------------------------------------------------------
  test('(d) an object is made, filed, moved to a folder and put away', async ({ page }) => {
    test.skip(!P4D324_SERVER_LANDED || !P4D325_SERVER_LANDED, GATE_REASON);
    await openInventoryDialog(page);
    await page.getByRole('button', { name: '+ New Item' }).click();
    await page.locator('#inventory-name').fill('Brass Compass');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.getByText('"Brass Compass" is laid on the table')).toBeVisible({ timeout: 15_000 });
    const row = page.getByTestId('inventory-item-row').filter({ hasText: 'Brass Compass' });
    await expect(row.getByTestId('inventory-glyph-default')).toBeVisible();

    await page.getByTestId('inventory-folder-bar').getByRole('button', { name: 'New folder' }).click();
    await page.getByLabel('New folder name').fill('Instruments');
    await page.getByLabel('New folder name').press('Enter');
    await expect(page.getByTestId('inventory-folder-row').filter({ hasText: 'Instruments' })).toBeVisible({ timeout: 15_000 });

    await page.getByTestId('inventory-folder-bar').getByRole('button', { name: 'Select' }).click();
    await row.getByLabel('Select Brass Compass').check();
    await page.getByTestId('inventory-select-bar').getByRole('button', { name: 'Move to folder…' }).click();
    await page.getByRole('option', { name: /Instruments/ }).click();
    await page.getByRole('button', { name: 'Move', exact: true }).click();
    await expect(page.getByText('"Brass Compass" now sits in "Instruments"')).toBeVisible({ timeout: 15_000 });
    await expect(page.getByTestId('inventory-folder-row').filter({ hasText: 'Instruments' })).toContainText('1 object');

    await page.getByTestId('inventory-folder-row').filter({ hasText: 'Instruments' }).getByRole('button', { name: 'Instruments' }).click();
    await row.getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: 'Archive' }).click();
    await expect(page.getByText('"Brass Compass" is put away')).toBeVisible({ timeout: 15_000 });
    await page.getByLabel('Show archived').check();
    await expect(row.locator('.qt-badge', { hasText: 'archived' })).toBeVisible({ timeout: 15_000 });
  });

  test('(e) an archetype is instantiated for a character', async ({ page }) => {
    test.skip(!P4D324_SERVER_LANDED || !P4D325_SERVER_LANDED, GATE_REASON);
    await openInventoryDialog(page);
    await page.getByRole('button', { name: '+ New Archetype' }).click();
    await page.locator('#inventory-name').fill('Spyglass');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.getByText('"Spyglass" is laid on the table')).toBeVisible({ timeout: 15_000 });
    await page.getByRole('tab', { name: 'Archetypes' }).click();
    const row = page.getByTestId('inventory-item-row').filter({ hasText: 'Spyglass' });
    await row.getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: 'Instantiate…' }).click();
    await page.locator('#inventory-instantiate-target').selectOption({ label: 'Aria' });
    await page.getByRole('button', { name: 'Instantiate', exact: true }).click();
    await expect(page.getByText('"Spyglass" is made from its archetype')).toBeVisible({ timeout: 15_000 });
    await page.getByRole('tab', { name: 'Items' }).click();
    await expect(page.getByTestId('inventory-item-row').filter({ hasText: 'Instance of Spyglass' })).toBeVisible({
      timeout: 15_000,
    });
  });

  test('(f) in chat: the participant card opens the inventory, Equip carries an object, Unequip and Reset put it back', async ({
    page,
  }) => {
    test.skip(!P4D324_SERVER_LANDED || !P4D325_SERVER_LANDED, GATE_REASON);
    await page.goto(`/salon/${SOLO_VOYAGE_CHAT_ID}`);
    await maybeUnlock(page);
    await openSidebarSection(page, 'Participants');
    await page.getByRole('button', { name: "Open Aria's inventory" }).click();
    const pane = page.getByTestId('inventory-carrying-now');
    await expect(pane.getByText('Carrying now · Aria in this chat')).toBeVisible({ timeout: 15_000 });
    const row = page.getByTestId('inventory-item-row').filter({ hasText: 'Brass Compass' });
    await row.getByRole('button', { name: 'Equip', exact: true }).click();
    await expect(row.locator('.qt-badge', { hasText: 'Equipped' })).toBeVisible({ timeout: 15_000 });
    await expect(pane.getByRole('list', { name: 'Carried objects' })).toContainText('Brass Compass');
    await pane.getByRole('list', { name: 'Carried objects' }).getByRole('button', { name: 'Unequip' }).click();
    await expect(pane.getByText('Empty-handed. Equip something from the list, or take it up here.')).toBeVisible({
      timeout: 15_000,
    });
    await pane.getByRole('button', { name: 'Take up…' }).click();
    await page.getByRole('listbox', { name: 'Objects within reach' }).getByRole('option').first().click();
    await expect(pane.getByRole('list', { name: 'Carried objects' })).toBeVisible({ timeout: 15_000 });
    page.once('dialog', (dialog) => void dialog.accept());
    await pane.getByRole('button', { name: 'Reset to defaults' }).click();
    await expect(page.getByText('Aria is carrying their defaults again')).toBeVisible({ timeout: 15_000 });
  });

  test('(g) a tool message shows the Pick Up notice', async ({ page }) => {
    test.skip(!P4D324_SERVER_LANDED || !P4D325_SERVER_LANDED || !P4D328_SERVER_LANDED, GATE_REASON);
    // The order's beat drives a real turn whose model answers one
    // `inventory_pick_up` call. `e2e/support/mock-llm.ts` (shared, not this
    // lane's) answers TEXT only — no tool calls — so the turn cannot be posed
    // yet: HANDOFF to the unifier (a tool-calling arm on the shared mock).
    // Beat (i) proves the row's rendering from a planted message meanwhile.
    test.skip(true, 'needs a tool-calling arm on the shared mock LLM (HANDOFF — e2e/support/mock-llm.ts)');
    await page.goto(`/salon/${SOLO_VOYAGE_CHAT_ID}`);
    await maybeUnlock(page);
    await expect(page.locator('.qt-chat-wardrobe-notice').filter({ hasText: 'Inventory' })).toContainText('Picked up ');
  });

  test('(i) a planted inventory_pick_up tool row renders the Pick Up notice (LIVE — SPA rendering only)', async ({
    page,
  }) => {
    // The row is planted in beforeAll (the CLI cannot write while the server holds the lock).
    await page.goto(`/salon/${SOLO_VOYAGE_CHAT_ID}`);
    await maybeUnlock(page);
    const notice = page.locator('.qt-chat-wardrobe-notice').filter({ hasText: 'Inventory' });
    // The client's article rule, verbatim (R-6 — the server's line says "a").
    await expect(notice).toContainText('Picked up an unicorn horn and equipped it.', { timeout: 15_000 });
  });

  test('(h) choosing Aria’s album retitles the page; My Photos brings the roll-up back', async ({ page }) => {
    test.skip(!P4D338_SERVER_LANDED, PHOTOS_GATE_REASON);
    await page.goto('/photos');
    await maybeUnlock(page);
    const select = page.getByLabel('Choose which album to show');
    await select.selectOption({ label: 'Aria' });
    await expect(page.locator('h1.qt-page-title')).toHaveText('Aria’s Photos', { timeout: 15_000 });
    await select.selectOption({ label: 'My Photos' });
    await expect(page.locator('h1.qt-page-title')).toHaveText('My Photos');
    await expect(page.getByText(/^Every image you’ve kept/)).toBeVisible();
  });
});

/** The Inventory dialog from the rail, outside the workspace (the plain dialog). */
async function openInventoryDialog(page: Page): Promise<void> {
  await page.goto('/salon');
  await maybeUnlock(page);
  await page.getByTitle('Inventory', { exact: true }).click();
  await expect(page.getByRole('dialog').locator('.qt-dialog-title').first()).toHaveText('Inventory', {
    timeout: 15_000,
  });
  await page.locator('#inventory-container-select').selectOption({ label: 'Aria' });
}

async function dispatch(body: Record<string, unknown>): Promise<unknown> {
  const res = await fetch(`${INV_BASE_URL}/api/dispatch`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  return res.json();
}

/** Unlock only when the passphrase screen is showing. */
async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  await expect(passphrase.or(page.locator('qt-shell')).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
    await expect(page.locator('qt-shell')).toBeVisible({ timeout: 15_000 });
  }
}

function runCliWrite(cli: string, sql: string): void {
  const res = spawnSync(cli, ['db', '--data-dir', INV_INSTANCE_DIR, '--write', sql], {
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
      const res = await fetch(`${INV_BASE_URL}/health`);
      // 423 = up, instance locked (the server gets no passphrase env).
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${INV_PORT}: ${lastErr}`);
}
