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
 * P4.9K3 — Rename & Replace: preview → apply on the shared characters
 * fixture's "Dax" (leaving Aria's name intact for sibling specs that key off
 * it — the `characters-flow.spec.ts` precedent), then a reload proves the
 * new name through `characterGet` (a `dogfood-verify-through-persisted-
 * state` beat, not just the in-page toast).
 *
 * ## ACTIVATE-AT-UNIFY
 *
 * `characterRename` (§B.2) is P4.9K1's verb — guarded off until the unifier
 * flips {@link P49K1_SERVER_LANDED}. Written complete per the work order.
 */
const P49K1_SERVER_LANDED = true;

const RENAME_PORT = 4334;
const RENAME_BASE_URL = `http://127.0.0.1:${RENAME_PORT}`;
const RENAME_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'rename-instance');
const RENAME_DATA_DIR = resolve(RENAME_INSTANCE_DIR, 'data');
const RENAME_SERVER_LOG = resolve(ARTIFACTS_DIR, 'rename-server.log');

const USER_TABLES = ['characters', 'chats', 'tags', 'api_keys', 'connection_profiles', 'image_profiles', 'files'];

let server: ChildProcess | undefined;

function runCliWrite(cli: string, sql: string): void {
  const result = spawnSync(
    cli,
    ['db', '--data-dir', RENAME_INSTANCE_DIR, '--write', sql],
    { env: { ...process.env, QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE } },
  );
  if (result.status !== 0) {
    throw new Error(`quilltap db write failed: ${result.stderr?.toString() ?? result.stdout?.toString()}`);
  }
}

function withoutPepper(): NodeJS.ProcessEnv {
  const env = { ...process.env };
  delete env['QUILLTAP_PEPPER'];
  return env;
}

async function waitForHealth(): Promise<void> {
  // A locked instance answers 423 — that is "ready" for the e2e (global
  // setup's own rule); the boot's help-docs sync can take longer than the
  // 10 s the first live run allowed, so the window is 60 s.
  for (let i = 0; i < 600; i++) {
    try {
      const res = await fetch(`${RENAME_BASE_URL}/health`);
      if (res.ok || res.status === 423) return;
    } catch {
      // not up yet
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error('quilltap-web did not become healthy in time');
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

test.describe('P4.9K3 — Rename & Replace (preview → apply → persisted state)', () => {
  test.skip(
    !P49K1_SERVER_LANDED,
    "ACTIVATE-AT-UNIFY: characterRename (§B.2) is P4.9K1's verb; guarded off until the unifier flips P49K1_SERVER_LANDED.",
  );

  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(RENAME_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(RENAME_DATA_DIR, { recursive: true });
    copyFileSync(resolve(FIXTURES_DIR, 'characters-main.db'), resolve(RENAME_DATA_DIR, 'quilltap.db'));
    copyFileSync(
      resolve(FIXTURES_DIR, 'characters-mount.db'),
      resolve(RENAME_DATA_DIR, 'quilltap-mount-index.db'),
    );
    writeFileSync(resolve(RENAME_DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));
    for (const table of USER_TABLES) {
      runCliWrite(cli, `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`);
    }

    const logFd = openSync(RENAME_SERVER_LOG, 'w');
    server = spawn(
      web,
      ['--host', '127.0.0.1', '--port', String(RENAME_PORT), '--data-dir', RENAME_INSTANCE_DIR, '--spa-dir', spaDir()],
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
    rmSync(RENAME_INSTANCE_DIR, { recursive: true, force: true });
  });

  test('preview then execute renames Dax, surviving a reload (persisted state, not just the toast)', async ({
    page,
  }) => {
    test.setTimeout(60_000);
    await page.goto(`${RENAME_BASE_URL}/characters`);
    await unlockIfLocked(page);

    const dax = page.locator('.character-card-grid .character-card').filter({ hasText: 'Dax' }).first();
    await dax.locator('p.line-clamp-3').click();
    await page.getByRole('link', { name: /Edit Character/i }).click();

    await page.getByRole('button', { name: 'Rename/Replace' }).click();
    await expect(page.getByText('Rename Character')).toBeVisible();

    await page.locator('input[placeholder="Enter new name"]').fill('Zephyrine');
    await page.getByRole('button', { name: 'Preview Changes' }).click();
    await expect(page.getByText(/Replacements \(\d+\)|No occurrences found/)).toBeVisible({
      timeout: 15_000,
    });

    const executeButton = page.getByRole('button', { name: /Execute \d+ Replacements/ });
    if (await executeButton.count()) {
      page.on('dialog', (d) => d.accept());
      await executeButton.click();
      await expect(page.getByText(/Successfully updated \d+ occurrences!/)).toBeVisible({
        timeout: 15_000,
      });
    }

    // Persisted-state proof: reload the roster and read the card back
    // through `characterGet` (the fetch the SPA re-issues on load), not the
    // in-page toast or client cache.
    await page.goto(`${RENAME_BASE_URL}/characters`);
    await page.reload();
    const renamed = page.locator('.character-card-grid .character-card').filter({ hasText: 'Zephyrine' });
    await expect(renamed).toBeVisible({ timeout: 10_000 });
  });
});
