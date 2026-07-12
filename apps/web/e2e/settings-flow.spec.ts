import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, openSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { expect, test, type Page } from '@playwright/test';

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
import { startMockLlm, type MockLlm } from './support/mock-llm';

/**
 * The LIVE Settings first-run e2e: a fresh instance walks setup → the provider
 * wizard → a validated OPENAI_COMPATIBLE key + profile against the e2e mock LLM →
 * the configured profile appears in the Providers tab.
 *
 * Live since the P4.6 unification wired the sibling lane's dispatch variants
 * (`providerList` / `apiKeyCreate` / `connectionProfileTest` / `modelFetch` /
 * `connectionProfileCreate` / `chatSettingsUpdate` — P4.6d handlers + the
 * unification's provider-actions driver). The mock serves both `GET /models`
 * (validate + models list) and streaming `POST /chat/completions`.
 */
const SETTINGS_PORT = 4321;
const SETTINGS_MOCK_PORT = 45302;
const SETTINGS_URL = `http://127.0.0.1:${SETTINGS_PORT}`;
const SETTINGS_INSTANCE = resolve(ARTIFACTS_DIR, 'settings-instance');
const SETTINGS_LOG = resolve(ARTIFACTS_DIR, 'settings-server.log');
const SETTINGS_PASSPHRASE = 'settings vertical passphrase';

let serverPid: number | undefined;
let mock: MockLlm;

test.describe('Settings vertical (fresh instance → wizard → configured profile)', () => {
  test.beforeAll(async () => {
    rmSync(SETTINGS_INSTANCE, { recursive: true, force: true });
    mkdirSync(resolve(SETTINGS_INSTANCE, 'data'), { recursive: true });
    mock = await startMockLlm(undefined, SETTINGS_MOCK_PORT);

    const web = webBinary();
    if (!existsSync(web)) {
      throw new Error(`Missing quilltap-web binary at ${web} — cargo build -p quilltap-web first`);
    }
    const env = { ...process.env };
    delete env['ENCRYPTION_MASTER_PEPPER'];
    const logFd = openSync(SETTINGS_LOG, 'w');
    const child = spawn(
      web,
      [
        '--host',
        '127.0.0.1',
        '--port',
        String(SETTINGS_PORT),
        '--data-dir',
        SETTINGS_INSTANCE,
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
        const res = await fetch(`${SETTINGS_URL}/health`);
        if (res.status === 423 || res.status === 200) break;
      } catch {
        // not up yet
      }
      if (Date.now() > deadline) {
        throw new Error(`settings server did not become ready within 30s; see ${SETTINGS_LOG}`);
      }
      await new Promise((r) => setTimeout(r, 300));
    }
  });

  test.afterAll(async () => {
    await mock?.close();
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
    rmSync(SETTINGS_INSTANCE, { recursive: true, force: true });
  });

  async function completeSetup(page: Page): Promise<void> {
    await page.goto(SETTINGS_URL);
    await expect(page.getByRole('heading', { name: 'Welcome to Quilltap' })).toBeVisible();
    await page.locator('#qt-setup-pass').fill(SETTINGS_PASSPHRASE);
    await page.locator('#qt-setup-confirm').fill(SETTINGS_PASSPHRASE);
    await page.getByRole('button', { name: 'Set Up with Passphrase' }).click();
    await expect(page.getByRole('heading', { name: 'Setup Complete' })).toBeVisible();
    await page.getByRole('button', { name: 'Continue to Quilltap' }).click();
    // The fresh instance hands off to the provider wizard.
    await expect(page.getByRole('heading', { name: 'Choose Your Providers' })).toBeVisible();
  }

  test('setup → wizard → validated key + model → save → profile in the Providers tab', async ({
    page,
  }) => {
    await completeSetup(page);

    // Step 1: pick the OpenAI-Compatible provider (requires a base URL we can
    // point at the mock).
    await page.getByRole('button', { name: /OpenAI-Compatible/i }).click();
    await page.getByRole('button', { name: 'Next' }).click();

    // Step 2: OPENAI_COMPATIBLE's key is optional (`requiresApiKey: false`) so
    // the wizard renders no key input — just the base URL, pointed at the mock,
    // then validate (a live `connectionProfileTest` → the mock's `GET /models`).
    await expect(page.getByRole('heading', { name: 'Configure API Keys' })).toBeVisible();
    await page.getByPlaceholder('http://localhost:8080/v1').fill(mock.url);
    await page.getByRole('button', { name: 'Validate' }).click();
    await expect(page.getByText('Connection validated successfully.')).toBeVisible();
    await page.getByRole('button', { name: 'Next' }).click();

    // Step 3: the mock's models are fetched; select one.
    await expect(page.getByRole('heading', { name: 'Choose Your Models' })).toBeVisible();
    await page.getByRole('combobox').first().selectOption('mock-model');
    await page.getByRole('button', { name: 'Next' }).click();

    // Skip the optional embedding + image steps.
    await page.getByRole('button', { name: 'Skip' }).click();
    await page.getByRole('button', { name: 'Skip' }).click();

    // Confirm: save & complete.
    await expect(page.getByRole('heading', { name: 'Review & Confirm' })).toBeVisible();
    await page.getByRole('button', { name: 'Save & Complete' }).click();
    await expect(page.getByText('Configuration saved successfully.')).toBeVisible();
    await page.getByRole('button', { name: 'Continue to Quilltap' }).click();

    // The Providers tab now lists the configured profile.
    await page.goto(`${SETTINGS_URL}/settings?tab=providers&section=connection-profiles`);
    await expect(
      page.getByRole('heading', { name: 'OpenAI-Compatible - mock-model' }),
    ).toBeVisible();
  });
});

// ---------------------------------------------------------------------------
// P4.6r — the Templates & Prompts + Images settings verticals. Authored
// pre-unification against lane A's extended groups-projects fixture (which gains
// user roleplay templates + image profiles); the describe SKIPS when that
// fixture is absent (this worktree) and auto-activates once lane A lands it +
// the `roleplayTemplate*` / `imageProfile*` dispatch variants at unification.
// ---------------------------------------------------------------------------

const TMPL_PORT = 4326;
const TMPL_BASE_URL = `http://127.0.0.1:${TMPL_PORT}`;
const TMPL_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'settings-templates-instance');
const TMPL_DATA_DIR = resolve(TMPL_INSTANCE_DIR, 'data');
const TMPL_SERVER_LOG = resolve(ARTIFACTS_DIR, 'settings-templates-server.log');

const GP_MAIN_FIXTURE = resolve(FIXTURES_DIR, 'groups-projects-main.db');
const GP_MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'groups-projects-mount.db');
const GP_FIXTURE_READY = existsSync(GP_MAIN_FIXTURE);
const TMPL_USER_TABLES = ['roleplayTemplates', 'imageProfiles', 'apiKeys', 'characters', 'tags'];

let tmplServer: ChildProcess | undefined;

test.describe('P4.6r — Templates & Images settings verticals', () => {
  test.skip(!GP_FIXTURE_READY, 'awaits lane A groups-projects fixture (wired at unification)');

  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(TMPL_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(TMPL_DATA_DIR, { recursive: true });
    copyFileSync(GP_MAIN_FIXTURE, resolve(TMPL_DATA_DIR, 'quilltap.db'));
    if (existsSync(GP_MOUNT_FIXTURE)) {
      copyFileSync(GP_MOUNT_FIXTURE, resolve(TMPL_DATA_DIR, 'quilltap-mount-index.db'));
    }
    writeFileSync(
      resolve(TMPL_DATA_DIR, 'quilltap.dbkey'),
      makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE),
    );
    for (const table of TMPL_USER_TABLES) {
      const res = spawnSync(
        cli,
        [
          'db',
          '--data-dir',
          TMPL_INSTANCE_DIR,
          '--write',
          `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`,
        ],
        {
          env: {
            ...withoutPepper(),
            QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE,
            QUILLTAP_QUIET_HINTS: '1',
          },
          encoding: 'utf8',
        },
      );
      if (res.status !== 0) {
        const out = `${res.stdout}${res.stderr}`;
        if (!out.includes('no such table') && !out.includes('no such column: userId')) {
          throw new Error(`fixture rewrite failed (${table}):\n${res.stdout}\n${res.stderr}`);
        }
      }
    }

    const logFd = openSync(TMPL_SERVER_LOG, 'w');
    tmplServer = spawn(
      web,
      [
        '--host',
        '127.0.0.1',
        '--port',
        String(TMPL_PORT),
        '--data-dir',
        TMPL_INSTANCE_DIR,
        '--spa-dir',
        spaDir(),
      ],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    tmplServer.unref();

    const deadline = Date.now() + 30_000;
    for (;;) {
      try {
        const res = await fetch(`${TMPL_BASE_URL}/health`);
        if (res.status === 423 || res.status === 200) break;
      } catch {
        // not up yet
      }
      if (Date.now() > deadline) {
        throw new Error(`templates settings server not ready within 30s; see ${TMPL_SERVER_LOG}`);
      }
      await new Promise((r) => setTimeout(r, 300));
    }
  });

  test.afterAll(() => {
    if (tmplServer?.pid) {
      try {
        process.kill(-tmplServer.pid, 'SIGTERM');
      } catch {
        try {
          process.kill(tmplServer.pid, 'SIGTERM');
        } catch {
          // already gone
        }
      }
    }
    rmSync(TMPL_INSTANCE_DIR, { recursive: true, force: true });
  });

  async function unlockIfLocked(page: Page, ready: ReturnType<Page['getByRole']>): Promise<void> {
    const passphrase = page.locator('#qt-passphrase');
    await expect(passphrase.or(ready).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.isVisible()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
    await expect(ready).toBeVisible({ timeout: 10_000 });
  }

  test('Templates tab: create → appears → edit → delete a roleplay template', async ({ page }) => {
    test.setTimeout(60_000);
    await page.goto(`${TMPL_BASE_URL}/settings?tab=templates`);
    await unlockIfLocked(page, page.getByRole('heading', { name: 'My Templates' }));

    // Create.
    await page.getByRole('button', { name: 'Create Template', exact: true }).first().click();
    const dialog = page.locator('qt-template-form-modal');
    await expect(dialog).toBeVisible();
    await dialog.getByPlaceholder('My Custom RP Style').fill('Walk Template');
    await dialog.locator('textarea').first().fill('Render narration between *stars*.');
    await dialog.getByRole('button', { name: 'Create Template', exact: true }).click();
    await expect(dialog).toBeHidden({ timeout: 10_000 });

    const card = page.locator('.qt-card', { hasText: 'Walk Template' });
    await expect(card).toBeVisible({ timeout: 10_000 });

    // Edit → rename.
    await card.getByRole('button', { name: 'Edit', exact: true }).click();
    const editDialog = page.locator('qt-template-form-modal');
    const nameInput = editDialog.getByPlaceholder('My Custom RP Style');
    await nameInput.fill('Walk Template Renamed');
    await editDialog.getByRole('button', { name: 'Save Changes' }).click();
    await expect(editDialog).toBeHidden({ timeout: 10_000 });
    await expect(page.locator('.qt-card', { hasText: 'Walk Template Renamed' })).toBeVisible({
      timeout: 10_000,
    });

    // Delete (inline confirm).
    const renamed = page.locator('.qt-card', { hasText: 'Walk Template Renamed' });
    await renamed.getByRole('button', { name: 'Delete', exact: true }).click();
    await renamed.getByRole('button', { name: 'Confirm', exact: true }).click();
    await expect(page.locator('.qt-card', { hasText: 'Walk Template Renamed' })).toHaveCount(0, {
      timeout: 10_000,
    });
  });

  test('Images tab: the Image Profiles card lists the fixture profiles', async ({ page }) => {
    test.setTimeout(60_000);
    await page.goto(`${TMPL_BASE_URL}/settings?tab=images`);
    await unlockIfLocked(
      page,
      page.getByRole('heading', { name: /Image Generation Profiles/ }),
    );
    // The card fetched the image-profiles listing (proves the lane-A variant is
    // live) and rendered the New Profile affordance.
    await expect(page.getByRole('button', { name: 'New Profile' })).toBeVisible({ timeout: 10_000 });
  });
});

function withoutPepper(): NodeJS.ProcessEnv {
  const env = { ...process.env };
  delete env['ENCRYPTION_MASTER_PEPPER'];
  return env;
}
