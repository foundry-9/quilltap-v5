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
 * P4.6h — the virtualized Salon scroll walk (dogfood finding #3b). Against a
 * SEPARATE ~300-message fixture (`salon-long-*.db`, built by
 * `build-long-chat-fixture.ts`), on its OWN locked server (the shared global-setup
 * server is pinned to the small Salon fixture): open the long chat → it becomes
 * interactive in bounded time → lands at the bottom → only a WINDOW of rows is in
 * the DOM → scroll up → the jump-to-bottom button appears → click it → back at the
 * bottom → the composer is present.
 *
 * The server-launch recipe mirrors global-setup (locked .dbkey + user-id rewrite,
 * `quilltap db --write` BEFORE launch since the write-lock refuses a live holder),
 * scoped to this spec's own instance dir + port. No LLM send happens here, so no
 * mock LLM is needed.
 */

const LONG_PORT = 4321;
const LONG_BASE_URL = `http://127.0.0.1:${LONG_PORT}`;
const LONG_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'long-instance');
const LONG_DATA_DIR = resolve(LONG_INSTANCE_DIR, 'data');
const LONG_SERVER_LOG = resolve(ARTIFACTS_DIR, 'long-server.log');

let server: ChildProcess | undefined;

test.describe('P4.6h — virtualized Salon scroll (long chat)', () => {
  test.beforeAll(async () => {
    // Each `quilltap db --write` unwraps the .dbkey via PBKDF2 (≈5s), so the few
    // rewrites plus server boot need more than the default per-hook timeout.
    test.setTimeout(120_000);

    const web = webBinary();
    const cli = cliBinary();

    // Fresh instance from the committed long fixture (copy, never mutate original).
    rmSync(LONG_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(LONG_DATA_DIR, { recursive: true });
    copyFileSync(resolve(FIXTURES_DIR, 'salon-long-main.db'), resolve(LONG_DATA_DIR, 'quilltap.db'));
    copyFileSync(
      resolve(FIXTURES_DIR, 'salon-long-mount.db'),
      resolve(LONG_DATA_DIR, 'quilltap-mount-index.db'),
    );

    // Lock it behind the test passphrase, then rewrite the user id to the engine's
    // SINGLE_USER_ID (so listChats / chatGet / chatSettings — all filtered by that
    // id — see this instance's rows). The fixture already carries the
    // `turnSkippingEnabled` column (baked by v4's current schema), so no ALTER is
    // needed. Only the tables this read-only walk touches are rewritten.
    writeFileSync(
      resolve(LONG_DATA_DIR, 'quilltap.dbkey'),
      makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE),
    );
    for (const table of ['chats', 'characters', 'chat_settings']) {
      runCliWrite(cli, `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`, false);
    }

    // Launch this spec's own locked server serving the built SPA.
    const logFd = openSync(LONG_SERVER_LOG, 'w');
    server = spawn(
      web,
      ['--host', '127.0.0.1', '--port', String(LONG_PORT), '--data-dir', LONG_INSTANCE_DIR, '--spa-dir', spaDir()],
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
    rmSync(LONG_INSTANCE_DIR, { recursive: true, force: true });
  });

  test('opens fast, lands at bottom, windows the DOM, and the jump button round-trips', async ({ page }) => {
    await page.goto(`${LONG_BASE_URL}/salon`);

    // Unlock (this server starts locked and is only used by this spec).
    const passphrase = page.locator('#qt-passphrase');
    await expect(passphrase).toBeVisible({ timeout: 15_000 });
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();

    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
    const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'The Long Expedition' });
    await expect(card).toBeVisible();

    // Open the long chat and time to interactive: the list is visible AND landed
    // at the bottom. Bounded well under the old ~10s synchronous render.
    const started = Date.now();
    await card.click();
    const list = page.locator('.qt-chat-messages');
    await expect(list).toBeVisible({ timeout: 10_000 });

    // Wait until settled at the bottom (the initial multi-strategy scroll runs
    // ~300ms after the 400ms settle; poll the real scroll position).
    await expect(async () => {
      expect(await distanceFromBottom(page)).toBeLessThanOrEqual(100);
    }).toPass({ timeout: 5_000 });
    const interactiveMs = Date.now() - started;
    expect(interactiveMs).toBeLessThan(3_000);

    // Only a WINDOW of rows is in the DOM (virtualized): far fewer than 300.
    const windowed = await page.locator('.qt-chat-messages-list [data-index]').count();
    expect(windowed).toBeGreaterThan(0);
    expect(windowed).toBeLessThan(60);

    // At the bottom, the jump-to-bottom button is hidden.
    const jump = page.locator('.qt-chat-scroll-to-bottom');
    await expect(jump).toBeHidden();

    // Let the multi-strategy initial scroll DRAIN before scrolling away: its
    // last correction fires at +300ms and would yank a too-early scroll-up
    // straight back to the bottom (v4 has the same 300ms window).
    await page.waitForTimeout(450);

    // Scroll up with REAL wheel input (fires the scroll events the stick
    // tracker listens for in every environment, unlike a bare scrollTop
    // assignment in a frame-throttled renderer) → the jump button appears.
    await list.hover();
    await page.mouse.wheel(0, -80_000);
    await expect(jump).toBeVisible({ timeout: 5_000 });

    // Click it → back at the bottom, button hidden again.
    await jump.click();
    await expect(async () => {
      expect(await distanceFromBottom(page)).toBeLessThanOrEqual(100);
    }).toPass({ timeout: 5_000 });
    await expect(jump).toBeHidden();

    // The composer is present (send path is available, no disabled-only state).
    await expect(page.locator('.qt-chat-composer-input')).toBeVisible();
  });
});

/** Distance in px from the bottom of the scroll container (mirrors the controller). */
async function distanceFromBottom(page: Page): Promise<number> {
  return page.locator('.qt-chat-messages').evaluate((el) => {
    return el.scrollHeight - el.scrollTop - el.clientHeight;
  });
}

function runCliWrite(cli: string, sql: string, allowFail = false): void {
  const res = spawnSync(cli, ['db', '--data-dir', LONG_INSTANCE_DIR, '--write', sql], {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  if (res.status !== 0 && !allowFail) {
    throw new Error(`CLI migration failed (${sql}):\n${res.stdout}\n${res.stderr}`);
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
      const res = await fetch(`${LONG_BASE_URL}/health`);
      if (res.status === 423 || res.status === 200) return;
      lastErr = `health status ${res.status}`;
    } catch (e) {
      lastErr = e instanceof Error ? e.message : String(e);
    }
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(`long-chat server did not become ready within 30s (${lastErr}); see ${LONG_SERVER_LOG}`);
}
