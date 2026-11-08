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

/**
 * P4.D386 — the shared item browser on the Wardrobe (v4 `b2b783b47` /
 * `29ba4d3f8` / `600c2906f`, pinned at `624d0d25c`).
 *
 * Its OWN instance on port 4363 (the round's §R.9 pair for this lane; the mock
 * LLM port 45361 is RESERVED and stays un-started — no beat needs a model turn,
 * L-7), over the `characters-main.db` / `characters-mount.db` pair with
 * `instance_settings`, `projects`, `groups`, `chat_settings` and
 * `group_doc_mount_links` materialized from the committed D23 dump (the
 * `inventory-flow.spec.ts` recipe). Aria's garments are made by dispatch.
 *
 * **UNGATED beats** need only `main`'s verbs (and this branch's SPA):
 *  (a) the ⋮ menu by keyboard on a wardrobe row inside the Wardrobe dialog —
 *      ArrowDown opens it focused on the first entry, End / Home, Escape closes
 *      the MENU and leaves the dialog open with focus back on the ⋮; the menu
 *      is `position: fixed` and inside the viewport;
 *  (b) the transfer dialog's container select — the catalogue's groups, the
 *      item's home absent — and NO destinations read (C12, a network listener);
 *  (c) a folder made from the shared folder bar, an item filed into it through
 *      the shared picker, the toast's crumb.
 *
 * **GATED beats** — flipped at the round's unification (§S.9):
 *  (d) Duplicate on a GROUP-store item through `?action=duplicate`;
 *  (e) two pages on one Wardrobe — a folder made in B appears in A (the
 *      `wardrobe` topic).
 */
// ACTIVATE-AT-UNIFY: P4.D377's duplicate verb + the wardrobe topic (C12)
const P4D377_SERVER_LANDED = false;
const GATE_REASON =
  'awaits P4.D377 — the `wardrobeItemDuplicate` verb and the `wardrobe` topic published from every wardrobe location write (C12), wired at unification';

const IB_PORT = 4363;
const IB_BASE_URL = `http://127.0.0.1:${IB_PORT}`;
const IB_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'item-browser-instance');
const IB_DATA_DIR = resolve(IB_INSTANCE_DIR, 'data');
const IB_SERVER_LOG = resolve(ARTIFACTS_DIR, 'item-browser-server.log');

/** Aria on the `characters-main.db` pair (the `workbench-wardrobe-picker-flow` constant). */
const ARIA_ID = 'a1000000-0000-4000-8000-000000000001';
/** Enough rows that the last one sits at the list's foot. */
const GARMENTS = ['Linen Shirt', 'Tweed Waistcoat', 'Silk Cravat', 'Wool Trousers', 'Spats', 'Bowler Hat'];

let server: ChildProcess | undefined;

test.use({ baseURL: IB_BASE_URL });

test.describe('P4.D386 — the shared item browser on the Wardrobe', () => {
  test.describe.configure({ mode: 'serial' });

  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(IB_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(IB_DATA_DIR, { recursive: true });
    mkdirSync(resolve(IB_DATA_DIR, 'files'), { recursive: true });
    copyFileSync(resolve(FIXTURES_DIR, 'characters-main.db'), resolve(IB_DATA_DIR, 'quilltap.db'));
    copyFileSync(resolve(FIXTURES_DIR, 'characters-mount.db'), resolve(IB_DATA_DIR, 'quilltap-mount-index.db'));
    writeFileSync(resolve(IB_DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));

    const freshSchema = JSON.parse(
      readFileSync(resolve(REPO_ROOT, 'crates/quilltap-core/src/services/provisioning/fresh_schema.json'), 'utf8'),
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
      const args = ['db', '--data-dir', IB_INSTANCE_DIR, '--write', create.replace('CREATE TABLE', 'CREATE TABLE IF NOT EXISTS')];
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

    const logFd = openSync(IB_SERVER_LOG, 'w');
    server = spawn(
      web,
      ['--host', '127.0.0.1', '--port', String(IB_PORT), '--data-dir', IB_INSTANCE_DIR, '--spa-dir', spaDir()],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    server.unref();
    await waitForHealth();
    await dispatch({ type: 'unlock', passphrase: E2E_PASSPHRASE });
    for (const title of GARMENTS) {
      const made = (await dispatch({
        type: 'characterWardrobeCreate',
        characterId: ARIA_ID,
        item: { title, types: ['top'], description: `${title}, for the beats` },
      })) as { type?: string };
      if (made.type === 'error') throw new Error(`characterWardrobeCreate ${title} answered ${JSON.stringify(made)}`);
    }
    const project = (await dispatch({ type: 'projectCreate', project: { name: 'Blackwood Manor' } })) as { type?: string };
    if (project.type === 'error') throw new Error(`projectCreate answered ${JSON.stringify(project)}`);
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
    rmSync(IB_INSTANCE_DIR, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  });

  // -------------------------------------------------------------------------
  // UNGATED
  // -------------------------------------------------------------------------
  test('(a) the ⋮ menu by keyboard: ArrowDown / End / Home / Escape, fixed and inside the viewport, the dialog left open', async ({
    page,
  }) => {
    const dialog = await openAriaWardrobe(page);
    const lastRow = dialog.locator('qt-wardrobe-item-row').filter({ hasText: GARMENTS.at(-1)! }).first();
    await lastRow.scrollIntoViewIfNeeded();
    const kebab = lastRow.getByRole('button', { name: 'More actions' });
    await expect(kebab).toHaveAttribute('aria-haspopup', 'menu');
    await kebab.focus();
    await page.keyboard.press('ArrowDown');
    const menu = page.getByRole('menu', { name: 'More actions' });
    await expect(menu).toBeVisible();
    await expect(kebab).toHaveAttribute('aria-expanded', 'true');
    const items = menu.getByRole('menuitem');
    await expect(items.first()).toBeFocused();
    await expect(items.first()).toHaveText('Edit');
    await page.keyboard.press('End');
    await expect(items.last()).toBeFocused();
    await expect(items.last()).toHaveText('Delete');
    await page.keyboard.press('Home');
    await expect(items.first()).toBeFocused();

    // Fixed to the viewport, and never clipped by the list's foot.
    const wrap = menu.locator('xpath=..');
    expect(await wrap.evaluate((el) => getComputedStyle(el).position)).toBe('fixed');
    const box = await wrap.boundingBox();
    const viewport = page.viewportSize()!;
    expect(box, 'the menu has a box').not.toBeNull();
    expect(box!.y).toBeGreaterThanOrEqual(0);
    expect(box!.y + box!.height).toBeLessThanOrEqual(viewport.height);

    await page.keyboard.press('Escape');
    await expect(menu).toHaveCount(0);
    await expect(dialog).toBeVisible();
    await expect(kebab).toBeFocused();
  });

  test('(b) the transfer dialog lists the container catalogue, the home left out — and reads no destinations verb (C12)', async ({
    page,
  }) => {
    const retired: string[] = [];
    page.on('request', (req) => {
      const url = req.url();
      const body = req.postData() ?? '';
      if (url.includes('/wardrobe/transfers') && req.method() === 'GET') retired.push(`GET ${url}`);
      if (body.includes('wardrobeTransferDestinations')) retired.push(body);
    });
    const dialog = await openAriaWardrobe(page);
    const row = dialog.locator('qt-wardrobe-item-row').filter({ hasText: GARMENTS[0] }).first();
    await row.getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: 'Copy to another wardrobe…', exact: true }).click();
    const copy = page.getByRole('dialog').filter({ hasText: 'Copy wardrobe item' });
    const select = copy.locator('#wardrobe-transfer-destination');
    await expect(select).toBeVisible({ timeout: 15_000 });
    const groups = await select.locator('optgroup').evaluateAll((els) => els.map((g) => (g as HTMLOptGroupElement).label));
    // The pair's characters, Quilltap General, and the project made here (no group).
    expect(groups).toEqual(['Characters', 'General', 'Projects']);
    await expect(select.locator('optgroup[label="General"] option')).toHaveText(['Quilltap General']);
    await expect(select.locator('optgroup[label="Projects"] option')).toHaveText(['Blackwood Manor']);
    // Aria's own vault is the item's home — left out.
    await expect(select.locator(`option[value="character:${ARIA_ID}"]`)).toHaveCount(0);
    // General is the natural first choice.
    await expect(select).toHaveValue('general:');
    await copy.getByRole('button', { name: 'Cancel' }).click();
    expect(retired, 'no request may read the retired transfers destinations').toEqual([]);
  });

  test('(c) a folder from the shared folder bar, an item filed into it through the shared picker, the toast’s crumb', async ({
    page,
  }) => {
    const dialog = await openAriaWardrobe(page);
    const bar = dialog.getByTestId('wardrobe-folder-bar');
    await bar.getByRole('button', { name: 'New folder' }).click();
    await dialog.getByLabel('New folder name').fill('Neckwear');
    await dialog.getByLabel('New folder name').press('Enter');
    await expect(page.getByText('The folder "Neckwear" is ready for hanging things in')).toBeVisible({ timeout: 15_000 });
    await expect(dialog.getByTestId('wardrobe-folder-row').filter({ hasText: 'Neckwear' })).toBeVisible();

    const row = dialog.locator('qt-wardrobe-item-row').filter({ hasText: 'Silk Cravat' }).first();
    await row.getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: 'Move to folder…', exact: true }).click();
    await page.getByRole('option', { name: /Neckwear/ }).click();
    await page.getByRole('button', { name: 'Move', exact: true }).click();
    await expect(page.getByText('"Silk Cravat" now hangs in "Neckwear"')).toBeVisible({ timeout: 15_000 });
    await expect(dialog.getByTestId('wardrobe-folder-row').filter({ hasText: 'Neckwear' })).toContainText('1 item');
  });

  // -------------------------------------------------------------------------
  // GATED (§S.9)
  // -------------------------------------------------------------------------
  test('(d) Duplicate on a GROUP-store item: the copy’s title is (copy)-suffixed against that tier; the body carries no title', async ({
    page,
  }) => {
    test.skip(!P4D377_SERVER_LANDED, GATE_REASON);
    const group = (await dispatch({ type: 'groupCreate', name: 'The Sisters' })) as { data?: { group?: { id?: string } } };
    const groupId = group.data?.group?.id;
    expect(groupId, `groupCreate answered ${JSON.stringify(group)}`).toBeTruthy();
    await dispatch({ type: 'groupWardrobeCreate', groupId, item: { title: 'Sash', types: ['accessories'] } });
    await dispatch({ type: 'groupWardrobeCreate', groupId, item: { title: 'Sash (copy)', types: ['accessories'] } });
    const bodies: string[] = [];
    page.on('request', (req) => {
      const body = req.postData() ?? '';
      if (body.includes('wardrobeItemDuplicate')) bodies.push(body);
    });
    await page.goto('/salon');
    await maybeUnlock(page);
    await page.getByTitle('Wardrobe', { exact: true }).click();
    const dialog = page.getByRole('dialog');
    await dialog.locator('#wardrobe-container-select').selectOption({ label: 'The Sisters' });
    const row = dialog.locator('qt-wardrobe-item-row').filter({ hasText: /^\s*Sash\s/ }).first();
    await row.getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: 'Duplicate', exact: true }).click();
    await expect(page.getByText('Duplicated "Sash"')).toBeVisible({ timeout: 15_000 });
    await expect(dialog.locator('qt-wardrobe-item-row').filter({ hasText: 'Sash (copy 2)' })).toBeVisible();
    expect(bodies).toHaveLength(1);
    const sent = JSON.parse(bodies[0]) as Record<string, unknown>;
    expect(sent).toEqual({ type: 'wardrobeItemDuplicate', scope: 'group', containerId: groupId, itemId: expect.any(String) });
  });

  test('(e) two pages on one Wardrobe: a folder made by dispatch in B appears in A with no reload (the wardrobe topic)', async ({
    page,
    browser,
  }) => {
    test.skip(!P4D377_SERVER_LANDED, GATE_REASON);
    const dialog = await openAriaWardrobe(page);
    const pageB = await browser.newPage({ baseURL: IB_BASE_URL });
    await pageB.goto('/salon');
    await maybeUnlock(pageB);
    await pageB.request.post(`${IB_BASE_URL}/api/dispatch`, {
      data: { type: 'wardrobeFolderCreate', scope: 'character', containerId: ARIA_ID, path: 'From Page B' },
    });
    await expect(dialog.getByTestId('wardrobe-folder-row').filter({ hasText: 'From Page B' })).toBeVisible({
      timeout: 15_000,
    });
    await pageB.close();
  });
});

async function openAriaWardrobe(page: Page) {
  await page.goto('/salon');
  await maybeUnlock(page);
  await page.getByTitle('Wardrobe', { exact: true }).click();
  const dialog = page.getByRole('dialog').filter({ has: page.locator('.qt-dialog-title', { hasText: 'Wardrobe' }) });
  await expect(dialog.locator('.qt-dialog-title').first()).toHaveText('Wardrobe', { timeout: 15_000 });
  const select = dialog.locator('#wardrobe-container-select');
  await expect(select.locator(`option[value="character:${ARIA_ID}"]`)).toHaveCount(1, { timeout: 15_000 });
  await select.selectOption(`character:${ARIA_ID}`);
  await expect(dialog.locator('qt-wardrobe-item-row').filter({ hasText: GARMENTS[0] }).first()).toBeVisible({
    timeout: 15_000,
  });
  return dialog;
}

async function dispatch(body: Record<string, unknown>): Promise<unknown> {
  const res = await fetch(`${IB_BASE_URL}/api/dispatch`, {
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
  const res = spawnSync(cli, ['db', '--data-dir', IB_INSTANCE_DIR, '--write', sql], {
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
      const res = await fetch(`${IB_BASE_URL}/health`);
      // 423 = up, instance locked (the server gets no passphrase env).
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${IB_PORT}: ${lastErr}`);
}
