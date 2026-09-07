import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, openSync, rmSync, writeFileSync } from 'node:fs';
import { createServer, type Server } from 'node:http';
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
 * P4.9K3 — the AI Wizard modal, browser-driven end to end on both hosts.
 *
 * ## ACTIVATE-AT-UNIFY
 *
 * `characterWizardStream` (§B.3) is P4.9K2's verb — it does not exist on the
 * server this lane's tree builds, so every beat here is guarded off (a
 * dispatch to an unknown request type answers a `badRequest` CoreError, not
 * the wizard flow). The unifier flips {@link P49K2_SERVER_LANDED} to `true`
 * once K2 lands; that flip is this beat's first real execution. Written
 * complete per the work order's "write them completely" instruction — do
 * not thin it out because it is dark today.
 *
 * The wizard's generation calls are NON-streaming completions on the mock
 * LLM's `/v1/chat/completions` endpoint (`stream: false` — the work order's
 * explicit note that the shared `support/mock-llm.ts`, which only speaks
 * SSE, must not be edited for this). `startNonStreamingMockLlm` below is
 * this spec's own support copy, not a shared-file change.
 */
const P49K2_SERVER_LANDED = false;

const WIZARD_PORT = 4333;
const WIZARD_BASE_URL = `http://127.0.0.1:${WIZARD_PORT}`;
const WIZARD_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'wizard-instance');
const WIZARD_DATA_DIR = resolve(WIZARD_INSTANCE_DIR, 'data');
const WIZARD_SERVER_LOG = resolve(ARTIFACTS_DIR, 'wizard-server.log');
const MOCK_LLM_PORT = 45304;

const GENERATED_TEXT = 'The mock model has spoken: a blacksmith who studies forbidden magic.';

/** Every fixture table the walk reads is filtered by userId — rewrite them all. */
const USER_TABLES = ['characters', 'chats', 'tags', 'api_keys', 'connection_profiles', 'image_profiles', 'files'];

let server: ChildProcess | undefined;

/**
 * A non-streaming OPENAI-compatible chat-completions mock — the wizard's own
 * flavor of the M4 `mock-llm.ts` precedent, kept local to this spec because
 * the shared file only speaks SSE (`stream: true`). Answers every
 * `POST /v1/chat/completions` with a fixed non-streaming JSON completion,
 * whatever the request's own `stream` value — the wizard's generator calls
 * ask for `stream: false`.
 */
async function startNonStreamingMockLlm(port: number): Promise<{ url: string; close: () => Promise<void> }> {
  const httpServer: Server = createServer((req, res) => {
    if (req.method === 'GET' && req.url?.includes('/models')) {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ data: [{ id: 'mock-model' }] }));
      return;
    }
    if (req.method !== 'POST' || !req.url?.includes('/chat/completions')) {
      res.writeHead(404).end();
      return;
    }
    req.on('data', () => {});
    req.on('end', () => {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(
        JSON.stringify({
          id: 'mock-wizard-1',
          object: 'chat.completion',
          model: 'mock-model',
          choices: [
            { index: 0, message: { role: 'assistant', content: GENERATED_TEXT }, finish_reason: 'stop' },
          ],
          usage: { prompt_tokens: 20, completion_tokens: 12, total_tokens: 32 },
        }),
      );
    });
  });
  const boundPort: number = await new Promise((res) => {
    httpServer.listen(port, '127.0.0.1', () => {
      const addr = httpServer.address();
      res(typeof addr === 'object' && addr ? addr.port : 0);
    });
  });
  return {
    url: `http://127.0.0.1:${boundPort}`,
    close: () => new Promise<void>((res, rej) => httpServer.close((e) => (e ? rej(e) : res()))),
  };
}

function runCliWrite(cli: string, sql: string): void {
  const result = spawnSync(
    cli,
    ['db', 'characters', 'sql', '--write', '--data-dir', WIZARD_INSTANCE_DIR, '--', sql],
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
  for (let i = 0; i < 100; i++) {
    try {
      const res = await fetch(`${WIZARD_BASE_URL}/health`);
      if (res.ok) return;
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

test.describe('P4.9K3 — the AI Wizard modal (New Character + Edit)', () => {
  test.skip(
    !P49K2_SERVER_LANDED,
    'ACTIVATE-AT-UNIFY: characterWizardStream (§B.3) is P4.9K2\'s verb; guarded off until the unifier flips P49K2_SERVER_LANDED.',
  );

  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(WIZARD_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(WIZARD_DATA_DIR, { recursive: true });
    copyFileSync(resolve(FIXTURES_DIR, 'characters-main.db'), resolve(WIZARD_DATA_DIR, 'quilltap.db'));
    copyFileSync(
      resolve(FIXTURES_DIR, 'characters-mount.db'),
      resolve(WIZARD_DATA_DIR, 'quilltap-mount-index.db'),
    );
    writeFileSync(resolve(WIZARD_DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));
    for (const table of USER_TABLES) {
      runCliWrite(cli, `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`);
    }
    // Point the fixture's OPENAI_COMPATIBLE connection profile at the mock,
    // same recipe as the M4 global setup — BEFORE the server launches (the
    // write-lock refuses a live holder).
    runCliWrite(
      cli,
      `UPDATE connection_profiles SET baseUrl = 'http://127.0.0.1:${MOCK_LLM_PORT}' WHERE provider = 'OPENAI_COMPATIBLE';`,
    );

    const logFd = openSync(WIZARD_SERVER_LOG, 'w');
    server = spawn(
      web,
      ['--host', '127.0.0.1', '--port', String(WIZARD_PORT), '--data-dir', WIZARD_INSTANCE_DIR, '--spa-dir', spaDir()],
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
    rmSync(WIZARD_INSTANCE_DIR, { recursive: true, force: true });
  });

  test('the wizard on New Character: every selected field lands the mock reply', async ({ page }) => {
    test.setTimeout(60_000);
    const mock = await startNonStreamingMockLlm(MOCK_LLM_PORT);
    try {
      await page.goto(`${WIZARD_BASE_URL}/characters/new`);
      await unlockIfLocked(page);

      await page.getByRole('button', { name: 'AI Wizard' }).click();
      await expect(page.getByText('Select AI Model')).toBeVisible();
      await page.getByRole('button', { name: 'Next' }).click();

      await expect(page.getByText('Physical Description Source')).toBeVisible();
      // 'existing' is the default source.
      await page.getByRole('button', { name: 'Next' }).click();

      await expect(page.getByText('Select Fields')).toBeVisible();
      const identityRow = page.locator('label', { hasText: 'Identity' }).first();
      await identityRow.locator('input[type=checkbox]').check();
      await page.getByRole('button', { name: 'Review & Generate' }).click();

      await expect(page.getByText('Ready to Generate')).toBeVisible();
      await page.getByRole('button', { name: 'Generate Character Content' }).click();

      await expect(page.getByText('Generation Complete')).toBeVisible({ timeout: 20_000 });
      await expect(page.getByText(GENERATED_TEXT)).toBeVisible();

      await page.getByRole('button', { name: 'Apply to Character' }).click();
      await expect(page.locator('[aria-label="Identity"]')).toContainText(GENERATED_TEXT);
    } finally {
      await mock.close();
    }
  });

  test('the wizard from Edit: existingData seeds availableFields (Identity already filled ⇒ unavailable)', async ({
    page,
  }) => {
    test.setTimeout(60_000);
    const mock = await startNonStreamingMockLlm(MOCK_LLM_PORT);
    try {
      await page.goto(`${WIZARD_BASE_URL}/characters`);
      await unlockIfLocked(page);

      const aria = page.locator('.character-card-grid .character-card').filter({ hasText: 'Aria' }).first();
      await aria.locator('p.line-clamp-3').click();
      await page.getByRole('link', { name: /Edit Character/i }).click();

      await page.getByRole('button', { name: 'AI Wizard' }).click();
      await page.getByRole('button', { name: 'Next' }).click();
      await page.getByRole('button', { name: 'Next' }).click();

      // Aria's fixture identity is non-empty, so the Identity checkbox is
      // disabled (existingData ⇒ not in availableFields).
      const identityRow = page.locator('label', { hasText: 'Identity' }).first();
      await expect(identityRow.locator('input[type=checkbox]')).toBeDisabled();
    } finally {
      await mock.close();
    }
  });
});
