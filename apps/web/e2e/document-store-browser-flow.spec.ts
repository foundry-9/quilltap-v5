import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  openSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
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
 * P4.D387 item 22 — the document-store browser walks (v4 `c34986421`,
 * `5efe53ea7`, `c17033f04` at the pin `624d0d25c`), on an OWN instance at
 * 4364 (§R.9; the `diagrams-flow.spec.ts` recipe: the salon pair copied,
 * re-keyed to the test pepper, the user rewritten to the single user,
 * `instance_settings` planted from `fresh_schema.json`). A database store and
 * its files are seeded through dispatch and the multipart item PUT; nothing
 * here needs an LLM, so no mock server is started (45362 stays reserved).
 *
 * UNGATED beats need no new server wire: the store page's `Files` browser,
 * New folder, Upload here, a `.md` opened read-only in the viewer, rename in
 * place, delete, pick mode in Document Mode's picker, `New document here` in a
 * folder, one Escape over the Move dialog closing only it (R11), and a PDF in
 * the native `<object>` (`NATIVE_PDF_VIEWER`).
 *
 * GATED beats wait on the server lanes; the unifier flips the two consts
 * (§S.9): P4.D381 (links / unlink, copy-folder, cross-store folder move,
 * recursive folder delete, the `.docx` 422 fallback, the realtime counts) and
 * P4.D380 (a `.png` opened in Document Mode showing the viewer pane, the
 * General library button, FilesCard Browse All on the official store).
 */

const P4D381_SERVER_LANDED = false;
const P4D380_SERVER_LANDED = false;
const GATE_381 =
  'awaits P4.D381’s C13 wire (folder family, links, render-html 422, per-store mountPoints hints) — flipped at unification';
const GATE_380 =
  'awaits P4.D380’s C14 wire (viewer on open / read, generalLibrary, officialMountPointId) — flipped at unification';

const DB_PORT = 4364;
const BASE = `http://127.0.0.1:${DB_PORT}`;
const INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'document-store-browser-instance');
const DATA_DIR = resolve(INSTANCE_DIR, 'data');
const SERVER_LOG = resolve(ARTIFACTS_DIR, 'document-store-browser-server.log');
const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'salon-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'salon-mount.db');
const USER_TABLES = ['characters', 'chats', 'tags', 'groups', 'projects', 'files'];

/** A minimal one-page PDF (enough for the platform viewer to accept it). */
const PDF_BYTES = Buffer.from(
  '%PDF-1.1\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n' +
    '3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF\n',
);
const PNG_1X1 = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
  'base64',
);

let server: ChildProcess | undefined;
let storeId = '';
let otherStoreId = '';
const STORE_NAME = 'Browser Archive';
const OTHER_STORE = 'Second Shelf';

async function dispatchRaw(
  req: unknown,
): Promise<{ type?: string; data?: Record<string, unknown> }> {
  const res = await fetch(`${BASE}/api/dispatch`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(req),
  });
  return (
    ((await res.json().catch(() => null)) as {
      type?: string;
      data?: Record<string, unknown>;
    } | null) ?? {}
  );
}

async function createStore(name: string): Promise<string> {
  const out = await dispatchRaw({
    type: 'mountPointCreate',
    mountPoint: { name, mountType: 'database' },
  });
  const id = (out.data?.['mountPoint'] as { id?: string } | undefined)?.id ?? '';
  if (!id) throw new Error(`mountPointCreate failed: ${JSON.stringify(out)}`);
  return id;
}

async function putFile(
  id: string,
  path: string,
  body: Buffer | string,
  type: string,
): Promise<void> {
  const form = new FormData();
  form.set('file', new Blob([body], { type }), path.split('/').pop());
  const url = `${BASE}/api/v1/mount-points/${encodeURIComponent(id)}/files/${path.split('/').map(encodeURIComponent).join('/')}`;
  const res = await fetch(url, { method: 'PUT', body: form });
  if (!res.ok) throw new Error(`upload ${path} failed: ${res.status} ${await res.text()}`);
}

test.describe.serial('P4.D387 — the document-store browser', () => {
  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();
    rmSync(INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(DATA_DIR, { recursive: true });
    mkdirSync(resolve(DATA_DIR, 'files'), { recursive: true });
    copyFileSync(MAIN_FIXTURE, resolve(DATA_DIR, 'quilltap.db'));
    if (existsSync(MOUNT_FIXTURE))
      copyFileSync(MOUNT_FIXTURE, resolve(DATA_DIR, 'quilltap-mount-index.db'));
    writeFileSync(resolve(DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));
    for (const table of [
      ...USER_TABLES,
      'connection_profiles',
      'api_keys',
      'chat_settings',
      'memories',
      'image_profiles',
    ]) {
      runCliWrite(
        cli,
        `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`,
      );
    }
    runCliWrite(cli, `UPDATE users SET id = '${SINGLE_USER_ID}' WHERE id = '${FIXTURE_USER}';`);
    const freshSchema = JSON.parse(
      readFileSync(
        resolve(REPO_ROOT, 'crates/quilltap-core/src/services/provisioning/fresh_schema.json'),
        'utf8',
      ),
    ) as { main: string[] };
    // The pair predates two tables a walk here needs: `instance_settings` (the
    // diagrams-flow plant) and `chat_documents` (Document Mode's open rows).
    for (const table of ['instance_settings', 'chat_documents']) {
      const ddlSql = freshSchema.main.find((sql) => sql.startsWith(`CREATE TABLE "${table}"`));
      if (!ddlSql) throw new Error(`fresh_schema.json carries no ${table} CREATE TABLE`);
      const ddl = spawnSync(cli, ['db', '--data-dir', INSTANCE_DIR, '--write', ddlSql], {
        env: {
          ...withoutPepper(),
          QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE,
          QUILLTAP_QUIET_HINTS: '1',
        },
        encoding: 'utf8',
      });
      if (ddl.status !== 0 && !`${ddl.stdout}${ddl.stderr}`.includes('already exists')) {
        throw new Error(`${table} plant failed:\n${ddl.stdout}\n${ddl.stderr}`);
      }
    }
    const logFd = openSync(SERVER_LOG, 'w');
    server = spawn(
      web,
      [
        '--host',
        '127.0.0.1',
        '--port',
        String(DB_PORT),
        '--data-dir',
        INSTANCE_DIR,
        '--spa-dir',
        spaDir(),
      ],
      {
        stdio: ['ignore', logFd, logFd],
        detached: true,
        env: withoutPepper(),
      },
    );
    server.unref();
    await waitForHealth();
    await dispatchRaw({ type: 'unlock', passphrase: E2E_PASSPHRASE });

    storeId = await createStore(STORE_NAME);
    otherStoreId = await createStore(OTHER_STORE);
    await dispatchRaw({ type: 'mountFolderCreate', mountPointId: storeId, path: 'Drafts' });
    await putFile(storeId, 'notes.md', '# Notes\n\nA line of notes.\n', 'text/markdown');
    await putFile(storeId, 'Drafts/chapter-one.md', '# Chapter One\n', 'text/markdown');
    await putFile(storeId, 'scan.pdf', PDF_BYTES, 'application/pdf');
    await putFile(storeId, 'pics/dot.png', PNG_1X1, 'image/png');
    await putFile(storeId, 'doomed.md', 'to be deleted\n', 'text/markdown');
    await putFile(storeId, 'rename-me.md', 'rename me\n', 'text/markdown');
  });

  test.afterAll(async () => {
    if (server?.pid) {
      try {
        process.kill(-server.pid, 'SIGTERM');
      } catch {
        try {
          process.kill(server.pid, 'SIGTERM');
        } catch {
          /* already gone */
        }
      }
    }
    // Let the server finish its shutdown writes before its directory goes.
    const deadline = Date.now() + 10_000;
    while (server?.pid && Date.now() < deadline) {
      try {
        process.kill(server.pid, 0);
      } catch {
        break;
      }
      await new Promise((r) => setTimeout(r, 200));
    }
    rmSync(INSTANCE_DIR, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
  });

  // --- UNGATED ------------------------------------------------------------

  test('(1) the store page heads its files "Files" and shows the browser', async ({ page }) => {
    await openStore(page);
    await expect(page.getByRole('heading', { name: 'Files', exact: true })).toBeVisible();
    await expect(page.locator('qt-document-store-browser')).toBeVisible();
    await expect(docRow(page, 'notes.md')).toBeVisible();
    await expect(
      page.locator('[data-testid="documents-folder-row"]', { hasText: 'Drafts' }),
    ).toBeVisible();
    await expect(page.getByRole('searchbox', { name: 'Search this store' })).toBeVisible();
  });

  test('(2) New folder makes a folder at the top', async ({ page }) => {
    await openStore(page);
    await page
      .locator('qt-document-store-browser')
      .getByRole('button', { name: 'New folder', exact: true })
      .first()
      .click();
    const input = page.locator('qt-new-folder-input input');
    await input.fill('Minutes');
    await input.press('Enter');
    await expect(
      page.locator('[data-testid="documents-folder-row"]', { hasText: 'Minutes' }),
    ).toBeVisible({ timeout: 15_000 });
  });

  test('(3) Upload here files into the folder being browsed', async ({ page }) => {
    await openStore(page);
    await page
      .locator('[data-testid="documents-folder-row"]', { hasText: 'Drafts' })
      .getByRole('button', { name: 'Drafts', exact: true })
      .click();
    await page.getByLabel('Upload files into this folder').setInputFiles({
      name: 'chapter-two.md',
      mimeType: 'text/markdown',
      buffer: Buffer.from('# Chapter Two\n'),
    });
    await expect(docRow(page, 'chapter-two.md')).toBeVisible({ timeout: 15_000 });
    await expect(docRow(page, 'chapter-two.md')).toHaveAttribute(
      'data-path',
      'Drafts/chapter-two.md',
    );
  });

  test('(4) a .md opens read-only in the viewer dialog', async ({ page }) => {
    await openStore(page);
    await docRow(page, 'notes.md').getByRole('button', { name: 'notes.md', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'notes.md' });
    await expect(dialog).toBeVisible();
    await expect(dialog.locator('pre code')).toContainText('A line of notes.', { timeout: 15_000 });
    await expect(dialog).toContainText('notes.md · read-only');
    await dialog.getByRole('button', { name: 'Close', exact: true }).last().click();
    await expect(dialog).toBeHidden();
  });

  test('(5) rename in place', async ({ page }) => {
    await openStore(page);
    await menuItem(page, 'rename-me.md', 'Rename').click();
    const input = page.getByRole('textbox', { name: 'Rename rename-me.md' });
    await input.fill('renamed.md');
    await input.press('Enter');
    await expect(docRow(page, 'renamed.md')).toBeVisible({ timeout: 15_000 });
    await expect(docRow(page, 'rename-me.md')).toHaveCount(0);
  });

  test('(6) delete a file after the confirmation', async ({ page }) => {
    await openStore(page);
    await menuItem(page, 'doomed.md', 'Delete').click();
    const confirm = page.getByRole('alertdialog');
    await expect(confirm).toContainText(
      'Delete "doomed.md"? It goes for good, and links to it elsewhere will find nothing.',
    );
    await confirm.getByRole('button', { name: 'Confirm', exact: true }).click();
    await expect(docRow(page, 'doomed.md')).toHaveCount(0, { timeout: 15_000 });
  });

  test('(7) one Escape over the Move dialog closes only it (R11, v4 5efe53ea7)', async ({
    page,
  }) => {
    await openStore(page);
    await menuItem(page, 'notes.md', 'Move to…').click();
    const move = page.getByRole('dialog', { name: /Move "notes\.md" to a folder/ });
    await expect(move).toBeVisible();
    await move.getByLabel('Store').focus();
    await page.keyboard.press('Escape');
    await expect(move).toBeHidden();
    // The store page (and its browser) are still there.
    await expect(page.locator('qt-document-store-browser')).toBeVisible();
    await expect(docRow(page, 'notes.md')).toBeVisible();
  });

  test('(8) a PDF opens in the native <object> (NATIVE_PDF_VIEWER)', async ({ page }) => {
    await openStore(page);
    await docRow(page, 'scan.pdf').getByRole('button', { name: 'scan.pdf', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'scan.pdf' });
    const object = dialog.locator('object[type="application/pdf"]');
    await expect(object).toHaveCount(1);
    expect(await object.getAttribute('data')).toContain('/scan.pdf?raw=1');
  });

  test('(9) Document Mode’s picker browses in pick mode — a .md opens in the editor', async ({
    page,
  }) => {
    await openSoloChat(page);
    const picker = await openPickerOnStore(page);
    await picker
      .locator('[data-testid="document-row"]', { hasText: 'notes.md' })
      .getByRole('button', { name: 'notes.md', exact: true })
      .click();
    const pane = page.locator('qt-document-pane');
    await expect(pane).toBeVisible({ timeout: 15_000 });
    await expect(pane.locator('.qt-rich-editor-content')).toContainText('A line of notes.', {
      timeout: 15_000,
    });
    await pane.getByRole('button', { name: 'Exit document mode', exact: true }).click();
    // The close persists server-side; let it settle before the next beat reloads.
    await expect(pane).toBeHidden({ timeout: 15_000 });
  });

  test('(10) New document here makes a document in the folder being browsed', async ({ page }) => {
    await openSoloChat(page);
    const picker = await openPickerOnStore(page);
    await picker
      .locator('[data-testid="documents-folder-row"]', { hasText: 'Drafts' })
      .getByRole('button', { name: 'Drafts', exact: true })
      .click();
    await picker.getByRole('button', { name: 'New document here', exact: true }).click();
    const pane = page.locator('qt-document-pane');
    await expect(pane).toBeVisible({ timeout: 15_000 });
    await expect(pane).toContainText('Drafts/', { timeout: 15_000 });
    await pane.getByRole('button', { name: 'Exit document mode', exact: true }).click();
    // The close persists server-side; let it settle before the next beat reloads.
    await expect(pane).toBeHidden({ timeout: 15_000 });
  });

  // --- GATED: P4.D381 (C13) -------------------------------------------------

  test('(11) Where else this appears… and Unlink from its twins', async ({ page }) => {
    test.skip(!P4D381_SERVER_LANDED, GATE_381);
    await dispatchRaw({
      type: 'mountFileLink',
      mountPointId: storeId,
      sourcePath: 'notes.md',
      destMountPointId: otherStoreId,
      destPath: 'notes.md',
    });
    await openStore(page);
    await expect(docRow(page, 'notes.md').getByText('Hard-linked')).toBeVisible({
      timeout: 15_000,
    });
    await menuItem(page, 'notes.md', 'Where else this appears…').click();
    const links = page.getByRole('dialog', { name: 'Where else this appears' });
    await expect(links.locator('[data-testid="document-links-list"] li')).toHaveCount(2, {
      timeout: 15_000,
    });
    await links.getByRole('button', { name: 'Close', exact: true }).last().click();
    await menuItem(page, 'notes.md', 'Unlink from its twins').click();
    await expect(docRow(page, 'notes.md').getByText('Hard-linked')).toHaveCount(0, {
      timeout: 15_000,
    });
  });

  test('(12) copy a folder, move a folder across stores, delete a folder with its files', async ({
    page,
  }) => {
    test.skip(!P4D381_SERVER_LANDED, GATE_381);
    await openStore(page);
    await folderMenuItem(page, 'Drafts', 'Copy to…').click();
    let dialog = page.getByRole('dialog', { name: /Copy folder "Drafts" to a folder/ });
    await dialog.getByLabel('Store').selectOption(otherStoreId);
    await dialog.getByRole('button', { name: 'Copy', exact: true }).click();
    await expect(dialog).toBeHidden({ timeout: 15_000 });
    await folderMenuItem(page, 'Minutes', 'Move to…').click();
    dialog = page.getByRole('dialog', { name: /Move folder "Minutes" to a folder/ });
    await dialog.getByLabel('Store').selectOption(otherStoreId);
    await dialog.getByRole('button', { name: 'Move', exact: true }).click();
    await expect(
      page.locator('[data-testid="documents-folder-row"]', { hasText: 'Minutes' }),
    ).toHaveCount(0, { timeout: 15_000 });
    await folderMenuItem(page, 'Drafts', 'Delete').click();
    const confirm = page.getByRole('alertdialog');
    await expect(confirm).toContainText('files inside it? They go for good.');
    await confirm.getByRole('button', { name: 'Confirm', exact: true }).click();
    await expect(
      page.locator('[data-testid="documents-folder-row"]', { hasText: 'Drafts' }),
    ).toHaveCount(0, { timeout: 15_000 });
  });

  test('(13) a .docx shows the download fallback on the server’s 422', async ({ page }) => {
    test.skip(!P4D381_SERVER_LANDED, GATE_381);
    await putFile(
      storeId,
      'report.docx',
      Buffer.from('PK\u0003\u0004'),
      'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    );
    await openStore(page);
    await docRow(page, 'report.docx')
      .getByRole('button', { name: 'report.docx', exact: true })
      .click();
    const dialog = page.getByRole('dialog', { name: 'report.docx' });
    await expect(dialog.locator('[data-testid="document-viewer-fallback"]')).toBeVisible({
      timeout: 15_000,
    });
    await expect(dialog.getByRole('link', { name: 'Download File' })).toBeVisible();
  });

  test('(14) the store page’s counts move on a write from elsewhere (the realtime hint)', async ({
    page,
  }) => {
    test.skip(!P4D381_SERVER_LANDED, GATE_381);
    await openStore(page);
    const before = await page.locator('.qt-heading-2').first().textContent();
    await putFile(storeId, 'arrived-elsewhere.md', 'hello\n', 'text/markdown');
    await expect(docRow(page, 'arrived-elsewhere.md')).toBeVisible({ timeout: 15_000 });
    await expect(page.locator('.qt-heading-2').first()).not.toHaveText(before ?? '', {
      timeout: 15_000,
    });
  });

  // --- GATED: P4.D380 (C14) -------------------------------------------------

  test('(15) a .png opened in Document Mode shows the read-only viewer pane', async ({ page }) => {
    test.skip(!P4D380_SERVER_LANDED, GATE_380);
    await openSoloChat(page);
    const picker = await openPickerOnStore(page);
    await picker
      .locator('[data-testid="documents-folder-row"]', { hasText: 'pics' })
      .getByRole('button', { name: 'pics', exact: true })
      .click();
    await picker.getByRole('button', { name: 'dot.png', exact: true }).click();
    const viewer = page.locator('[data-testid="document-viewer-pane"]');
    await expect(viewer).toBeVisible({ timeout: 15_000 });
    await expect(viewer).toContainText('View only — this kind of file cannot be edited here.');
    await viewer.getByRole('button', { name: 'Close document', exact: true }).click();
  });

  test('(16) the General library button browses Quilltap General', async ({ page }) => {
    test.skip(!P4D380_SERVER_LANDED, GATE_380);
    await openSoloChat(page);
    await page.getByRole('button', { name: 'Open document', exact: true }).click();
    const picker = page.getByRole('dialog', { name: 'Open Document' });
    await picker.getByRole('button', { name: /General library/ }).click();
    await expect(page.getByRole('dialog', { name: 'Select File' })).toContainText(
      'General Library',
    );
  });

  test('(17) FilesCard Browse All opens the project’s official store', async ({ page }) => {
    test.skip(!P4D380_SERVER_LANDED, GATE_380);
    const projects = (await dispatchRaw({ type: 'projectList' })).data?.['projects'] as
      Array<{ id: string }> | undefined;
    const projectId = projects?.[0]?.id ?? '';
    expect(projectId, 'a project on the pair').toBeTruthy();
    await page.goto(`${BASE}/prospero/${projectId}`);
    await maybeUnlock(page);
    await page.getByRole('button', { name: 'Browse All Files', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Project Files' });
    await expect(dialog.locator('qt-document-store-browser')).toBeVisible({ timeout: 15_000 });
  });
});

// --- helpers -------------------------------------------------------------------

function docRow(page: Page, name: string) {
  return page.locator('[data-testid="document-row"]', { hasText: name });
}

function menuItem(page: Page, fileName: string, label: string) {
  return {
    click: async () => {
      await docRow(page, fileName)
        .getByRole('button', { name: `Actions for ${fileName}` })
        .click();
      await page.getByRole('menuitem', { name: label, exact: true }).click();
    },
  };
}

function folderMenuItem(page: Page, folder: string, label: string) {
  return {
    click: async () => {
      await page
        .locator('[data-testid="documents-folder-row"]', { hasText: folder })
        .getByRole('button', { name: `Folder actions for ${folder}` })
        .click();
      await page.getByRole('menuitem', { name: label, exact: true }).click();
    },
  };
}

async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  if (await passphrase.isVisible({ timeout: 2_000 }).catch(() => false)) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock', exact: true }).click();
  }
}

async function openStore(page: Page): Promise<void> {
  await dispatchRaw({ type: 'unlock', passphrase: E2E_PASSPHRASE });
  await page.goto(`${BASE}/scriptorium/${storeId}`);
  await maybeUnlock(page);
  await expect(page.getByRole('heading', { name: STORE_NAME })).toBeVisible({ timeout: 15_000 });
}

async function openSoloChat(page: Page): Promise<void> {
  await dispatchRaw({ type: 'unlock', passphrase: E2E_PASSPHRASE });
  const chats =
    ((await dispatchRaw({ type: 'listChats' })).data as unknown as {
      id: string;
      title: string;
    }[]) ?? [];
  const solo = chats.find((c) => c.title === 'Solo Voyage')?.id ?? '';
  expect(solo, 'Solo Voyage on the pair').toBeTruthy();
  // A clean slate: a document a previous beat opened stays open server-side
  // (and the chat stays in document mode), so close every open one first.
  const open = ((await dispatchRaw({ type: 'chatOpenDocuments', chatId: solo })).data?.[
    'documents'
  ] ?? []) as Array<{ id: string }>;
  for (const doc of open) {
    await dispatchRaw({ type: 'chatDocumentClose', chatId: solo, chatDocumentId: doc.id });
  }
  await page.goto(`${BASE}/salon/${solo}`);
  await maybeUnlock(page);
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
}

/** Open Document → Look everywhere → the seeded store → the browse step. */
async function openPickerOnStore(page: Page) {
  await page.getByRole('button', { name: 'Open document', exact: true }).click();
  const source = page.getByRole('dialog', { name: 'Open Document' });
  await expect(source).toBeVisible();
  await source.getByLabel(/Look everywhere/).check();
  await source.getByRole('button', { name: STORE_NAME, exact: true }).click();
  const browse = page.getByRole('dialog', { name: 'Select File' });
  await expect(browse.locator('qt-document-store-browser')).toBeVisible({ timeout: 15_000 });
  await expect(browse.locator('[data-testid="document-row"]').first()).toBeVisible({
    timeout: 15_000,
  });
  return browse;
}

function runCliWrite(cli: string, sql: string): void {
  const res = spawnSync(cli, ['db', '--data-dir', INSTANCE_DIR, '--write', sql], {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  if (res.status !== 0) {
    const out = `${res.stdout}${res.stderr}`;
    if (out.includes('no such table') || out.includes('no such column: userId')) return;
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
      const res = await fetch(`${BASE}/health`);
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${DB_PORT}: ${lastErr}`);
}
