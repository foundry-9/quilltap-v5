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
 * P4.D321 — browser walk of the Wardrobe Pictures shelf in a character's Photo
 * Gallery tab (v4 `1cfc9b504`). LIVE in-lane: both halves are this lane's.
 * Runs its OWN locked server + instance dir over the committed `characters-*`
 * fixture pair (the `character-avatar-rolls-flow.spec.ts` recipe: copy the
 * pair, write the locked .dbkey, rewrite the fixture user id to
 * SINGLE_USER_ID with `quilltap db --write`, boot quilltap-web on a
 * spec-private port). No LLM call happens — the read is model-free — so no
 * mock LLM (its reserved port 45344 stays unused).
 *
 * ## The seed (the order's R-9 (b))
 *
 * The pair carries no generated wardrobe picture, and no UI path makes one
 * without a provider (an editor upload is `UPLOADED` — which the shelf
 * excludes). So the walk seeds through SHIPPED edges only: once unlocked it
 * creates a garment in Aria's own wardrobe and one in Quilltap General through
 * the REST edges and uploads a real picture to each through the item-images
 * edge — and checks the shelf is still ABSENT (uploads are not generated).
 * Then it stops the server, flips each picture's `files` row to `GENERATED`
 * (+ a prompt and a pinned `createdAt`, Aria's one second NEWER) with one
 * `quilltap db --write` statement per call, and boots again.
 */

const PICTURES_PORT = 4346;
const PICTURES_BASE_URL = `http://127.0.0.1:${PICTURES_PORT}`;
const PICTURES_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'wardrobe-pictures-instance');
const PICTURES_DATA_DIR = resolve(PICTURES_INSTANCE_DIR, 'data');
const PICTURES_SERVER_LOG = resolve(ARTIFACTS_DIR, 'wardrobe-pictures-server.log');

/** Every fixture table the walk reads is filtered by userId — rewrite them all. */
const USER_TABLES = ['characters', 'chats', 'tags', 'files'];

const OWN_TITLE = 'Flying Coat';
const GENERAL_TITLE = 'Brass Goggles';
const GROUP_NAME = 'The Flying Circus';
const GROUP_TITLE = 'Tin Whistle';

/** A real 2×2 lossy WebP (the avatar-rolls spec's bytes) — an `<img>` it can load. */
const PICTURE_BYTES = Buffer.from(
  'UklGRjQAAABXRUJQVlA4ICgAAACQAQCdASoCAAIAAUAmJZgCdLoAA5gA/vCbQ/4DdfFtMv0fmXJznAAA',
  'base64',
);

let server: ChildProcess | undefined;
/** Aria's id, read by NAME before boot (the builder mints ids). */
let ariaId = '';

test.describe('P4.D321 — Wardrobe Pictures in the Photo Gallery tab', () => {
  test.beforeAll(async () => {
    // Each `quilltap db --write` unwraps the .dbkey via PBKDF2 (≈5s).
    test.setTimeout(150_000);
    const cli = cliBinary();
    rmSync(PICTURES_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(PICTURES_DATA_DIR, { recursive: true });
    copyFileSync(
      resolve(FIXTURES_DIR, 'characters-main.db'),
      resolve(PICTURES_DATA_DIR, 'quilltap.db'),
    );
    copyFileSync(
      resolve(FIXTURES_DIR, 'characters-mount.db'),
      resolve(PICTURES_DATA_DIR, 'quilltap-mount-index.db'),
    );
    writeFileSync(
      resolve(PICTURES_DATA_DIR, 'quilltap.dbkey'),
      makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE),
    );
    for (const table of USER_TABLES) {
      runCliWrite(
        cli,
        `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`,
      );
    }
    // The committed pair carries no `instance_settings` table, and
    // `ensure_builtin_mounts`' first act is v4's own `shouldRun` guard on it —
    // so without the (empty) table boot never mints Quilltap General, and the
    // General plate has nowhere to live. Instance MATERIALIZATION, not a
    // fixture regen (the `wardrobe-flow.spec.ts` P4.D130 precedent).
    runCliWrite(
      cli,
      'CREATE TABLE IF NOT EXISTS instance_settings (' +
        '"key" TEXT PRIMARY KEY, "value" TEXT NOT NULL);',
    );
    // The group plate (G.2) needs a group: the pair predates the `groups` /
    // `group_doc_mount_links` tables, so materialize them from the committed D23
    // dump (the `inventory-flow.spec.ts` recipe) — instance materialization, not
    // a fixture regen.
    const freshSchema = JSON.parse(
      readFileSync(
        resolve(REPO_ROOT, 'crates/quilltap-core/src/services/provisioning/fresh_schema.json'),
        'utf8',
      ),
    ) as { main: string[]; mountIndex: string[] };
    for (const [partition, table] of [
      ['main', 'groups'],
      ['mountIndex', 'group_doc_mount_links'],
    ] as const) {
      const create = freshSchema[partition].find((sql) => sql.startsWith(`CREATE TABLE "${table}"`));
      if (!create) throw new Error(`fresh_schema.json carries no ${table} CREATE TABLE`);
      const args = [
        'db',
        '--data-dir',
        PICTURES_INSTANCE_DIR,
        '--write',
        create.replace('CREATE TABLE', 'CREATE TABLE IF NOT EXISTS'),
      ];
      if (partition === 'mountIndex') args.splice(1, 0, '--mount-points');
      const res = spawnSync(cli, args, { env: cliEnv(), encoding: 'utf8' });
      if (res.status !== 0) throw new Error(`${table} plant failed:\n${res.stdout}\n${res.stderr}`);
    }
    ariaId = readOne(cli, `SELECT id FROM characters WHERE name = 'Aria' LIMIT 1;`) ?? '';
    if (!ariaId) throw new Error('the characters fixture has no Aria');
    await startServer();
  });

  test.afterAll(async () => {
    stopServer();
    rmSync(PICTURES_INSTANCE_DIR, { recursive: true, force: true });
  });

  test('the Wardrobe Pictures shelf: hidden with none; expands to captioned plates; Download; the detail modal', async ({
    page,
  }) => {
    test.setTimeout(240_000);
    const cli = cliBinary();

    // (1) Seed through the shipped edges, then: NO shelf — the uploads are
    //     UPLOADED, which the gallery leaves out.
    await page.goto(`${PICTURES_BASE_URL}/characters`);
    await unlockIfLocked(page);
    const ownItemId = await createItem(`/api/v1/characters/${ariaId}/wardrobe`, OWN_TITLE, 'top');
    // Quilltap General is ensured by the host's boot pass AFTER the unlock —
    // wait for it rather than race it.
    await expect
      .poll(async () => (await fetch(`${PICTURES_BASE_URL}/api/v1/wardrobe`)).status, {
        timeout: 30_000,
      })
      .toBe(200);
    const generalItemId = await createItem('/api/v1/wardrobe', GENERAL_TITLE, 'accessories');
    await uploadPicture(ownItemId, `scope=character&id=${ariaId}`, 'flying-coat.webp');
    await uploadPicture(generalItemId, 'scope=general', 'brass-goggles.webp');
    // The group tier (G.2): a group with Aria as a member and one garment of its
    // own, so the third plate reads `Group · <name>`.
    const groupId = await dispatchData<{ group: { id: string } }>({
      type: 'groupCreate',
      name: GROUP_NAME,
    }).then((d) => d.group.id);
    await dispatchData({ type: 'groupMemberAdd', groupId, characterId: ariaId });
    const groupItemId = await dispatchData<{ wardrobeItem: { id: string } }>({
      type: 'groupWardrobeCreate',
      groupId,
      item: { title: GROUP_TITLE, types: ['accessories'] },
    }).then((d) => d.wardrobeItem.id);
    await uploadPicture(groupItemId, `scope=group&id=${groupId}`, 'tin-whistle.webp');

    await openAriaGallery(page);
    await expect(page.locator('qt-avatar-rolls-section')).toBeAttached();
    await expect(page.locator('qt-wardrobe-pictures-section')).toBeAttached();
    // The tab has settled (the album's count line is up) and the shelf is empty.
    await expect(page.locator('qt-character-gallery-tab p.qt-text-small').first()).toBeVisible();
    await expect(page.getByText('Wardrobe Pictures', { exact: true })).toHaveCount(0);

    // (2) Stop, flip both pictures to GENERATED (Aria's newer), reboot.
    stopServer();
    const ownFile = readOne(
      cli,
      `SELECT id FROM files WHERE category = 'IMAGE' AND linkedTo LIKE '%${ownItemId}%' LIMIT 1;`,
    );
    const generalFile = readOne(
      cli,
      `SELECT id FROM files WHERE category = 'IMAGE' AND linkedTo LIKE '%${generalItemId}%' LIMIT 1;`,
    );
    const groupFile = readOne(
      cli,
      `SELECT id FROM files WHERE category = 'IMAGE' AND linkedTo LIKE '%${groupItemId}%' LIMIT 1;`,
    );
    if (!ownFile || !generalFile || !groupFile) throw new Error('the uploads left no files rows to flip');
    const ownName = readOne(cli, `SELECT originalFilename FROM files WHERE id = '${ownFile}';`);
    const generalName = readOne(
      cli,
      `SELECT originalFilename FROM files WHERE id = '${generalFile}';`,
    );
    if (!ownName || !generalName) throw new Error('the flipped pictures carry no file names');
    runCliWrite(
      cli,
      `UPDATE files SET source = 'GENERATED', generationPrompt = 'Product photograph of a flying coat', createdAt = '2026-10-17T10:00:01.000Z' WHERE id = '${ownFile}';`,
    );
    runCliWrite(
      cli,
      `UPDATE files SET source = 'GENERATED', generationPrompt = 'Product photograph of brass goggles', createdAt = '2026-10-17T10:00:00.000Z' WHERE id = '${generalFile}';`,
    );
    runCliWrite(
      cli,
      `UPDATE files SET source = 'GENERATED', generationPrompt = 'Product photograph of a tin whistle', createdAt = '2026-10-17T09:59:59.000Z' WHERE id = '${groupFile}';`,
    );
    await startServer();

    await page.goto(`${PICTURES_BASE_URL}/characters`);
    await unlockIfLocked(page);
    await openAriaGallery(page);

    // The shelf: heading, `3 pictures`, collapsed.
    const section = page.locator('qt-wardrobe-pictures-section');
    const header = section.locator('button[aria-expanded]');
    await expect(header).toBeVisible({ timeout: 15_000 });
    await expect(header).toContainText('Wardrobe Pictures');
    await expect(header).toContainText('3 pictures');
    await expect(header).toHaveAttribute('aria-expanded', 'false');
    await expect(
      section.getByText('Every likeness the house has sketched of the garments within Aria', {
        exact: false,
      }),
    ).toBeVisible();
    await expect(section.locator('img')).toHaveCount(0);

    // Expand → three captioned plates, newest first (Aria's own, General's, the group's).
    await header.click();
    await expect(section.locator('img')).toHaveCount(3);
    const plates = section.locator('.grid > .relative.group');
    await expect(plates).toHaveCount(3);
    await expect(plates.nth(0).locator('.qt-gallery-caption-title')).toHaveText(OWN_TITLE);
    await expect(plates.nth(0).locator('.qt-gallery-caption-detail')).toHaveText('Own wardrobe');
    await expect(plates.nth(1).locator('.qt-gallery-caption-title')).toHaveText(GENERAL_TITLE);
    await expect(plates.nth(1).locator('.qt-gallery-caption-detail')).toHaveText(
      'Shared · Quilltap General',
    );
    await expect(plates.nth(2).locator('.qt-gallery-caption-title')).toHaveText(GROUP_TITLE);
    await expect(plates.nth(2).locator('.qt-gallery-caption-detail')).toHaveText(`Group · ${GROUP_NAME}`);
    await expect(plates.nth(0).locator('.qt-gallery-caption')).toHaveAttribute(
      'title',
      `${OWN_TITLE} — Own wardrobe`,
    );

    // (3) Hover → ONLY Download; it saves the file under its original name.
    await plates.nth(0).hover();
    const actions = plates.nth(0).locator(':scope > div button');
    await expect(actions).toHaveCount(1);
    await expect(actions.first()).toHaveAttribute('title', 'Download image');
    const downloadPromise = page.waitForEvent('download');
    await actions.first().click();
    const download = await downloadPromise;
    expect(download.suggestedFilename()).toBe(ownName);
    await expect(page.locator('qt-image-detail-modal')).toHaveCount(0);

    // (4) Click a plate → the SHARED detail modal; Next → the second; close.
    await plates.nth(0).locator('button.aspect-square').click();
    // The host element portals its overlay and has no box of its own; the
    // overlay is what is visible.
    const modal = page.locator('qt-image-detail-modal .fixed.inset-0').first();
    await expect(modal).toBeVisible();
    await expect(modal).toContainText(ownName);
    // No generation prompt reaches the modal (the order's R-7).
    await expect(modal).not.toContainText('Product photograph of a flying coat');
    // Next — the modal's own ArrowRight binding (v4 :72-78).
    await page.keyboard.press('ArrowRight');
    await expect(modal).toContainText(generalName, { timeout: 10_000 });
    await page.keyboard.press('Escape');
    await expect(page.locator('qt-image-detail-modal')).toHaveCount(0);
  });
});

async function startServer(): Promise<void> {
  const logFd = openSync(PICTURES_SERVER_LOG, 'a');
  server = spawn(
    webBinary(),
    [
      '--host',
      '127.0.0.1',
      '--port',
      String(PICTURES_PORT),
      '--data-dir',
      PICTURES_INSTANCE_DIR,
      '--spa-dir',
      spaDir(),
    ],
    { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
  );
  server.unref();
  await waitForHealth();
}

function stopServer(): void {
  const pid = server?.pid;
  if (!pid) return;
  try {
    process.kill(-pid, 'SIGTERM');
  } catch {
    try {
      process.kill(pid, 'SIGTERM');
    } catch {
      // already gone
    }
  }
  // Wait for the instance lock to clear (the server releases it on SIGTERM).
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    try {
      process.kill(pid, 0);
    } catch {
      break;
    }
    spawnSync('sleep', ['0.2']);
  }
  server = undefined;
}

async function dispatchData<T = unknown>(request: Record<string, unknown>): Promise<T> {
  const res = await api<{ type: string; data: T }>('/api/dispatch', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(request),
  });
  if (res.type === 'error') throw new Error(`${request['type']} → ${JSON.stringify(res.data)}`);
  return res.data;
}

async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${PICTURES_BASE_URL}${path}`, init);
  const text = await res.text();
  if (!res.ok) throw new Error(`${init?.method ?? 'GET'} ${path} → ${res.status}: ${text}`);
  return JSON.parse(text) as T;
}

async function createItem(path: string, title: string, type: string): Promise<string> {
  const body = await api<{ wardrobeItem: { id: string } }>(path, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ title, types: [type] }),
  });
  return body.wardrobeItem.id;
}

async function uploadPicture(itemId: string, query: string, filename: string): Promise<void> {
  const form = new FormData();
  form.append('file', new File([PICTURE_BYTES], filename, { type: 'image/webp' }));
  await api(`/api/v1/wardrobe/${itemId}/images?action=upload&${query}`, {
    method: 'POST',
    body: form,
  });
}

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

async function openAriaGallery(page: Page): Promise<void> {
  const aria = page
    .locator('.character-card-grid .character-card')
    .filter({ hasText: 'Aria' })
    .first();
  await aria.locator('p.line-clamp-3').click();
  await expect(page.getByRole('heading', { name: 'Aria' })).toBeVisible();
  await page.getByRole('button', { name: 'Photo Gallery' }).click();
}

function cliEnv(): NodeJS.ProcessEnv {
  return { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' };
}

function runCliWrite(cli: string, sql: string): void {
  const res = spawnSync(cli, ['db', '--data-dir', PICTURES_INSTANCE_DIR, '--write', sql], {
    env: cliEnv(),
    encoding: 'utf8',
  });
  if (res.status !== 0) {
    throw new Error(`CLI write failed (${sql}):\n${res.stdout}\n${res.stderr}`);
  }
}

/** One scalar out of the main DB. */
function readOne(cli: string, sql: string): string | null {
  const res = spawnSync(cli, ['db', '--data-dir', PICTURES_INSTANCE_DIR, '--json', sql], {
    env: cliEnv(),
    encoding: 'utf8',
  });
  if (res.status !== 0) {
    throw new Error(`CLI read failed (${sql}):\n${res.stdout}\n${res.stderr}`);
  }
  const rows = JSON.parse(res.stdout) as Array<Record<string, unknown>>;
  const first = rows[0];
  if (!first) return null;
  const value = Object.values(first)[0];
  return value == null ? null : String(value);
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
      const res = await fetch(`${PICTURES_BASE_URL}/health`);
      if (res.status === 423 || res.status === 200) return;
      lastErr = `health status ${res.status}`;
    } catch (e) {
      lastErr = e instanceof Error ? e.message : String(e);
    }
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(
    `wardrobe-pictures server did not become ready within 30s (${lastErr}); see ${PICTURES_SERVER_LOG}`,
  );
}
