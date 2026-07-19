import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, openSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';

import { expect, test } from './support/fixtures';

import { ARTIFACTS_DIR, spaDir, webBinary } from './support/env';

/**
 * The LIVE first-run setup e2e (the P4.5 deferral closed at the P4.4/P4.5
 * unification): a second server against an EMPTY data dir walks needs-setup →
 * the setup wizard → the one-time pepper reveal → the shell on the freshly
 * provisioned instance. Self-contained — spawns/kills its own server on a
 * separate port so the globalSetup fixture server is untouched.
 *
 * P4.6e note: a fresh instance now hands off to the provider setup wizard after
 * "Continue to Quilltap" (v4 `navigateAfterSetup`: no connection profiles → the
 * wizard). The wizard is reachable from the nav's Settings, so navigating there
 * still reaches the (empty) Salon.
 */
const SETUP_PORT = 4320;
const SETUP_URL = `http://127.0.0.1:${SETUP_PORT}`;
const SETUP_INSTANCE = resolve(ARTIFACTS_DIR, 'setup-instance');
const SETUP_LOG = resolve(ARTIFACTS_DIR, 'setup-server.log');
const SETUP_PASSPHRASE = 'a fresh instance passphrase';

let serverPid: number | undefined;

test.beforeAll(async () => {
  rmSync(SETUP_INSTANCE, { recursive: true, force: true });
  mkdirSync(resolve(SETUP_INSTANCE, 'data'), { recursive: true });

  const web = webBinary();
  if (!existsSync(web)) {
    throw new Error(`Missing quilltap-web binary at ${web} — cargo build -p quilltap-web first`);
  }
  const env = { ...process.env };
  delete env['ENCRYPTION_MASTER_PEPPER']; // no env pepper → the empty dir boots needs-setup
  const logFd = openSync(SETUP_LOG, 'w');
  const child = spawn(
    web,
    [
      '--host',
      '127.0.0.1',
      '--port',
      String(SETUP_PORT),
      '--data-dir',
      SETUP_INSTANCE,
      '--spa-dir',
      spaDir(),
    ],
    { stdio: ['ignore', logFd, logFd], detached: true, env },
  );
  child.unref();
  serverPid = child.pid;

  const deadline = Date.now() + 30_000;
  for (;;) {
    try {
      const res = await fetch(`${SETUP_URL}/health`);
      if (res.status === 423 || res.status === 200) {
        break;
      }
    } catch {
      // not up yet
    }
    if (Date.now() > deadline) {
      throw new Error(`setup-flow server did not become ready within 30s; see ${SETUP_LOG}`);
    }
    await new Promise((r) => setTimeout(r, 300));
  }
});

test.afterAll(() => {
  if (serverPid !== undefined) {
    try {
      process.kill(-serverPid, 'SIGTERM');
    } catch {
      try {
        process.kill(serverPid, 'SIGTERM');
      } catch {
        // already gone
      }
    }
  }
  rmSync(SETUP_INSTANCE, { recursive: true, force: true });
});

test('walks needs-setup → wizard → pepper reveal → shell on the provisioned instance', async ({
  page,
}) => {
  await page.goto(SETUP_URL);

  // The empty instance routes to the first-run wizard (v4-voiced).
  await expect(page.getByRole('heading', { name: 'Welcome to Quilltap' })).toBeVisible();

  // A mismatched confirm surfaces the client-side validation copy.
  await page.locator('#qt-setup-pass').fill(SETUP_PASSPHRASE);
  await page.locator('#qt-setup-confirm').fill('something else entirely');
  await page.getByRole('button', { name: 'Set Up with Passphrase' }).click();
  await expect(page.getByText('Passphrases do not match')).toBeVisible();

  // The real setup dispatch provisions the encrypted instance and reveals the
  // pepper ONCE.
  await page.locator('#qt-setup-confirm').fill(SETUP_PASSPHRASE);
  await page.getByRole('button', { name: 'Set Up with Passphrase' }).click();
  await expect(page.getByRole('heading', { name: 'Setup Complete' })).toBeVisible();
  await expect(
    page.getByText('Save this encryption pepper now. It will not be shown again.'),
  ).toBeVisible();
  const pepper = (await page.locator('pre.qt-code-block').textContent())?.trim() ?? '';
  expect(pepper.length).toBeGreaterThan(0);

  // Continue hands a profile-less fresh instance off to the provider wizard
  // (v4 `navigateAfterSetup`).
  await page.getByRole('button', { name: 'Continue to Quilltap' }).click();
  await expect(page.getByRole('heading', { name: 'Choose Your Providers' })).toBeVisible();

  // The nav still reaches the (empty) Salon (the Chats rail item — the brand
  // button navigates to the Home dashboard since P4.6av).
  await page.getByRole('link', { name: 'Chats' }).click();
  await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
  await expect(page.getByText('No chats yet')).toBeVisible();

  // The provisioned instance is on disk, encrypted, alongside its .dbkey.
  expect(existsSync(resolve(SETUP_INSTANCE, 'data', 'quilltap.db'))).toBe(true);
  expect(existsSync(resolve(SETUP_INSTANCE, 'data', 'quilltap.dbkey'))).toBe(true);
});
