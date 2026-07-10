import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, openSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';

import { expect, test, type Page } from '@playwright/test';

import { ARTIFACTS_DIR, spaDir, webBinary } from './support/env';
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
