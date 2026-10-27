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
 * P4.173 (D338-20) — a toast raised from inside an open modal is on TOP of it.
 *
 * v4 `5a8338b30` raised the toast container above every dialog overlay (z-index
 * 50 → 1000): an error or success toast from a save inside a modal used to
 * appear BEHIND it. The Inventory dialog is the room: saving a new item there
 * toasts `"<name>" is laid on the table` while the dialog stays open, with no
 * LLM call anywhere. The assertion is a hit test — `elementFromPoint` at the
 * toast's centre lands inside `.qt-toast-container`, not on the dialog's
 * overlay or panel.
 *
 * Its OWN instance on port 4352 over the `characters-*` pair (the
 * `inventory-flow.spec.ts` recipe: the userId rewrite and the D23 dump's
 * `instance_settings` / `projects` / `groups` / `chat_settings` /
 * `group_doc_mount_links` tables, materialized not regenerated).
 */
const TOAST_PORT = 4352;
const INV_BASE_URL = `http://127.0.0.1:${TOAST_PORT}`;
const INV_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'toast-stacking-instance');
const INV_DATA_DIR = resolve(INV_INSTANCE_DIR, 'data');
const INV_SERVER_LOG = resolve(ARTIFACTS_DIR, 'toast-stacking-server.log');

let server: ChildProcess | undefined;

test.use({ baseURL: INV_BASE_URL });

test.describe('P4.173 — a toast over an open modal is clickable', () => {
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

    const logFd = openSync(INV_SERVER_LOG, 'w');
    server = spawn(
      web,
      ['--host', '127.0.0.1', '--port', String(TOAST_PORT), '--data-dir', INV_INSTANCE_DIR, '--spa-dir', spaDir()],
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
    rmSync(INV_INSTANCE_DIR, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  });

  test('a save toast over the open Inventory dialog is the topmost element at its own centre', async ({
    page,
  }) => {
    await openInventoryDialog(page);
    await page.getByRole('button', { name: '+ New Item' }).click();
    await page.locator('#inventory-name').fill('Toast Test Lantern');
    await page.getByRole('button', { name: 'Save', exact: true }).click();

    const toast = page.locator('.qt-toast-container').getByText('"Toast Test Lantern" is laid on the table');
    await expect(toast).toBeVisible({ timeout: 15_000 });
    // The Inventory dialog is still open underneath.
    await expect(page.getByRole('dialog').locator('.qt-dialog-title').first()).toHaveText('Inventory');

    const hit = await toast.evaluate((el) => {
      const r = el.getBoundingClientRect();
      const top = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      return {
        inContainer: !!top?.closest('.qt-toast-container'),
        topTag: top ? `${top.tagName.toLowerCase()}.${String(top.className).slice(0, 60)}` : null,
      };
    });
    expect(hit.inContainer, `the element at the toast's centre was ${hit.topTag}`).toBe(true);
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
  throw new Error(`quilltap-web did not become healthy on ${TOAST_PORT}: ${lastErr}`);
}
