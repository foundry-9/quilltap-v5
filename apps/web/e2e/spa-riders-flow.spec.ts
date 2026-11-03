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
import { startMockLlm, type MockLlm } from './support/mock-llm';

/**
 * P4.D359 — the SPA riders of the `236c04e35` round (item 18).
 *
 * Its OWN instance on port 4358 (mock LLM 45356 — the round's §R.9 ports for
 * this lane), over the `characters-main.db` / `characters-mount.db` pair with
 * `instance_settings`, `projects`, `groups`, `chat_settings` and
 * `group_doc_mount_links` materialized from the committed D23 dump (the
 * `inventory-flow.spec.ts` recipe, verbatim) and the shared llm-logs fixture
 * copied in for the Inspector beat. The mock answers with a line that NAMES
 * Aria — absent from nothing here, but the chips beat (gated) seats a
 * different chat.
 *
 * **LIVE beats** need only this branch: (a) the always-on launcher and the
 * floating console over a `qt-modal`; (b) the tab and the floating console on
 * two conversations; (e) the About credits; (f) the Inspector's derived Other
 * filter and the new badges; (j) the templates card's default surviving a
 * tab round trip with no reload.
 *
 * **GATED beats** — flipped at unification (§S.9). NAMED constants, never a
 * capability probe; each gated beat calls `test.skip(…)` first.
 */
// ACTIVATE-AT-UNIFY: P4.D341's keystone column / admission / API (C8).
const P4D341_LANDED = false;
// ACTIVATE-AT-UNIFY: P4.D347's `chatAbsentMentions` arm (C5) — AND P4.D344 applying this lane's
// `handoffs/p4.d359-salon-conversation.patch` (the Salon's AbsentMentionsSlot).
const P4D347_SERVER_LANDED = false;
// ACTIVATE-AT-UNIFY: P4.D349's vault keys (C6 — `humanoid`, `pronounGuidance`).
const P4D349_SERVER_LANDED = false;
// ACTIVATE-AT-UNIFY: P4.D350's R-1 location-level `inventory` publish.
const P4D350_PUBLISH_LANDED = false;
// ACTIVATE-AT-UNIFY: P4.D358's `qt-modal` Escape (R-N) carrying this lane's floating-layer guard.
const P4D358_MODAL_ESCAPE_LANDED = false;

const PORT = 4358;
const BASE_URL = `http://127.0.0.1:${PORT}`;
const MOCK_LLM_PORT = 45356;
const INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'spa-riders-instance');
const DATA_DIR = resolve(INSTANCE_DIR, 'data');
const SERVER_LOG = resolve(ARTIFACTS_DIR, 'spa-riders-server.log');

const SOLO_VOYAGE_CHAT_ID = 'c1000000-0000-4000-8000-000000000001';

let server: ChildProcess | undefined;
let mock: MockLlm | undefined;

test.use({ baseURL: BASE_URL });

test.describe('P4.D359 — the SPA riders', () => {
  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(DATA_DIR, { recursive: true });
    mkdirSync(resolve(DATA_DIR, 'files'), { recursive: true });
    copyFileSync(resolve(FIXTURES_DIR, 'characters-main.db'), resolve(DATA_DIR, 'quilltap.db'));
    copyFileSync(
      resolve(FIXTURES_DIR, 'characters-mount.db'),
      resolve(DATA_DIR, 'quilltap-mount-index.db'),
    );
    copyFileSync(
      resolve(FIXTURES_DIR, 'salon-llm-logs.db'),
      resolve(DATA_DIR, 'quilltap-llm-logs.db'),
    );
    writeFileSync(resolve(DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));

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
      const create = freshSchema[partition].find((sql) =>
        sql.startsWith(`CREATE TABLE "${table}"`),
      );
      if (!create) throw new Error(`fresh_schema.json carries no ${table} CREATE TABLE`);
      runCliWrite(cli, create.replace('CREATE TABLE', 'CREATE TABLE IF NOT EXISTS'), {
        mountPoints: partition === 'mountIndex',
      });
    }
    for (const table of [
      'characters',
      'chats',
      'tags',
      'api_keys',
      'connection_profiles',
      'image_profiles',
      'files',
    ]) {
      runCliWrite(
        cli,
        `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`,
      );
    }
    runCliWrite(
      cli,
      `UPDATE connection_profiles SET baseUrl = 'http://127.0.0.1:${MOCK_LLM_PORT}/v1' WHERE provider = 'OPENAI_COMPATIBLE';`,
    );
    // Beat (f): two of v4 `eb2d87788`'s newly-labelled types on Solo Voyage.
    for (const [id, type] of [
      ['e3590000-0000-4000-8000-000000000001', 'ANSWER_CONFIRMATION'],
      ['e3590000-0000-4000-8000-000000000002', 'IMAGE_GENERATION'],
    ] as const) {
      runCliWrite(
        cli,
        `INSERT OR REPLACE INTO llm_logs (id, userId, type, messageId, chatId, characterId, autonomousRunId, ` +
          `provider, modelName, request, response, usage, cacheUsage, rawProviderUsage, requestHashes, durationMs, ` +
          `createdAt, updatedAt) VALUES ('${id}', '${SINGLE_USER_ID}', '${type}', NULL, '${SOLO_VOYAGE_CHAT_ID}', ` +
          `NULL, NULL, 'OPENAI_COMPATIBLE', 'mock-model', '{"messageCount":0,"messages":[],"toolCount":0}', ` +
          `'{"content":"ok","contentLength":2}', NULL, NULL, NULL, NULL, 10, '2026-02-01T00:00:0${id.slice(-1)}.000Z', ` +
          `'2026-02-01T00:00:0${id.slice(-1)}.000Z');`,
        { llmLogs: true },
      );
    }

    mock = await startMockLlm('Aria would not approve.', MOCK_LLM_PORT);
    const logFd = openSync(SERVER_LOG, 'w');
    server = spawn(
      web,
      [
        '--host',
        '127.0.0.1',
        '--port',
        String(PORT),
        '--data-dir',
        INSTANCE_DIR,
        '--spa-dir',
        spaDir(),
      ],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    server.unref();
    await waitForHealth();
    await dispatch({ type: 'unlock', passphrase: E2E_PASSPHRASE });
    // The pair carries no connection profile, so the Brahma Console would be
    // ineligible; give it one on the mock (beat (b) opens two conversations).
    const profile = (await dispatch({
      type: 'connectionProfileCreate',
      profile: {
        name: 'P4D359 Engine',
        provider: 'OPENAI_COMPATIBLE',
        baseUrl: `http://127.0.0.1:${MOCK_LLM_PORT}/v1`,
        modelName: 'mock-model',
        isDefault: true,
      },
    })) as { type?: string };
    if (profile.type === 'error')
      throw new Error(`connectionProfileCreate: ${JSON.stringify(profile)}`);
    // The Inspector reads logs only while logging is on (the global-setup recipe).
    await dispatch({
      type: 'chatSettingsUpdate',
      settings: { llmLoggingSettings: { enabled: true, verboseMode: false, retentionDays: 30 } },
    });
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
    await mock?.close();
    rmSync(INSTANCE_DIR, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  });

  // -------------------------------------------------------------------------
  // LIVE
  // -------------------------------------------------------------------------
  test('(a) the launcher toggles the floating console, which sits over a qt-modal without dismissing it', async ({
    page,
  }) => {
    await page.goto('/characters');
    await maybeUnlock(page);
    const launcher = page.locator('button.qt-brahma-launcher');
    await expect(launcher).toBeVisible({ timeout: 15_000 });
    await expect(launcher).toHaveAttribute('aria-pressed', 'false');
    await expect(launcher).toHaveAttribute('aria-label', 'Open the Brahma Console');

    // A qt-modal first: the SillyTavern import dialog.
    await page.getByRole('button', { name: 'Import from SillyTavern' }).click();
    const modal = page
      .getByRole('dialog', { name: /Import from SillyTavern/ })
      .or(page.locator('.qt-dialog', { hasText: 'Import from SillyTavern' }));
    await expect(modal.first()).toBeVisible();

    // The launcher is reachable over the modal's scrim (the floating layer's z-index).
    await launcher.click();
    await expect(launcher).toHaveAttribute('aria-pressed', 'true');
    await expect(launcher).toHaveAttribute('aria-label', 'Hide the Brahma Console');
    const consoleDialog = page.locator('.qt-floating-dialog-topmost[data-qt-floating-layer]');
    await expect(consoleDialog).toBeVisible({ timeout: 15_000 });
    await expect(consoleDialog).toHaveAttribute('aria-modal', 'false');

    // A click inside the console's composer leaves the modal open.
    const composer = consoleDialog.locator('textarea');
    await composer.click();
    await expect(modal.first()).toBeVisible();

    // Escape in the composer closes the console only.
    await composer.press('Escape');
    await expect(consoleDialog).toBeHidden();
    await expect(modal.first()).toBeVisible();
    await expect(launcher).toHaveAttribute('aria-pressed', 'false');

    // The geometry persists under its key, across a reload.
    await launcher.click();
    await expect(consoleDialog).toBeVisible();
    const header = consoleDialog.locator('.qt-floating-dialog-header');
    const box = (await header.boundingBox())!;
    await page.mouse.move(box.x + 40, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + 10, box.y + box.height / 2 + 30, { steps: 5 });
    await page.mouse.up();
    const stored = await page.evaluate(() =>
      localStorage.getItem('quilltap:brahma-console-geometry'),
    );
    expect(stored, 'the drag persisted the geometry').toBeTruthy();
    const before = JSON.parse(stored!) as { x: number; y: number };
    await page.reload();
    await maybeUnlock(page);
    await page.locator('button.qt-brahma-launcher').click();
    await expect(consoleDialog).toBeVisible({ timeout: 15_000 });
    const after = await consoleDialog.evaluate((el) => ({
      x: parseFloat((el as HTMLElement).style.left),
      y: parseFloat((el as HTMLElement).style.top),
    }));
    expect(after).toEqual({ x: before.x, y: before.y });
  });

  test('(a′) Escape in the modal closes only the modal (P4.D358’s qt-modal Escape)', async ({
    page,
  }) => {
    test.skip(
      !P4D358_MODAL_ESCAPE_LANDED,
      'awaits P4.D358’s qt-modal Escape (R-N) with the floating-layer guard',
    );
    await page.goto('/characters');
    await maybeUnlock(page);
    await page.getByRole('button', { name: 'Import from SillyTavern' }).click();
    await page.locator('button.qt-brahma-launcher').click();
    const consoleDialog = page.locator('.qt-floating-dialog-topmost[data-qt-floating-layer]');
    await expect(consoleDialog).toBeVisible();
    await page
      .locator('.qt-dialog', { hasText: 'Import from SillyTavern' })
      .locator('button')
      .first()
      .focus();
    await page.keyboard.press('Escape');
    await expect(page.locator('.qt-dialog', { hasText: 'Import from SillyTavern' })).toBeHidden();
    await expect(consoleDialog).toBeVisible();
  });

  test('(b) the workspace tab and the floating console remember two different conversations', async ({
    page,
  }) => {
    const a = (await dispatch({ type: 'brahmaConsoleCreate' })) as {
      data?: { chat?: { id?: string } };
    };
    const b = (await dispatch({ type: 'brahmaConsoleCreate' })) as {
      data?: { chat?: { id?: string } };
    };
    const chatA = a.data?.chat?.id;
    const chatB = b.data?.chat?.id;
    expect(chatA && chatB, `brahmaConsoleCreate answered ${JSON.stringify(a)}`).toBeTruthy();
    await page.goto('/characters');
    await maybeUnlock(page);
    await page.evaluate(
      ([tab, floating]) => {
        localStorage.setItem('quilltap:brahma-console-last-id', tab!);
        localStorage.setItem('quilltap:brahma-floating-last-id', floating!);
      },
      [chatA, chatB],
    );
    await page.reload();
    await maybeUnlock(page);
    await page.locator('button.qt-brahma-launcher').click();
    await expect(
      page.locator('.qt-floating-dialog-topmost [title="Past conversations"]'),
    ).toBeVisible({
      timeout: 15_000,
    });
    // Each surface's key is its own: opening the floating one left the tab's alone.
    expect(await page.evaluate(() => localStorage.getItem('quilltap:brahma-console-last-id'))).toBe(
      chatA,
    );
    expect(
      await page.evaluate(() => localStorage.getItem('quilltap:brahma-floating-last-id')),
    ).toBe(chatB);
    // Past conversations returns the floating surface to the launcher and forgets ITS id only.
    await page.locator('.qt-floating-dialog-topmost [title="Past conversations"]').click();
    await expect(
      page.locator('.qt-floating-dialog-topmost').getByText('Recent Console Conversations'),
    ).toBeVisible();
    expect(
      await page.evaluate(() => localStorage.getItem('quilltap:brahma-floating-last-id')),
    ).toBeNull();
    expect(await page.evaluate(() => localStorage.getItem('quilltap:brahma-console-last-id'))).toBe(
      chatA,
    );
  });

  test('(e) the About page shows the Diagrams and Terminals & Realtime lines', async ({ page }) => {
    await page.goto('/about');
    await maybeUnlock(page);
    await expect(
      page.getByText('Mermaid, Graphviz (@hpcc-js/wasm-graphviz), svg2roughjs, DOMPurify'),
    ).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByText('xterm.js, node-pty, ws, croner')).toBeVisible();
  });

  test('(f) the Inspector’s derived Other lists an ANSWER_CONFIRMATION row; IMAGE_GENERATION is badged Image', async ({
    page,
  }) => {
    await page.goto(`/salon/${SOLO_VOYAGE_CHAT_ID}`);
    await maybeUnlock(page);
    await page.getByRole('button', { name: 'Toggle LLM Inspector' }).click();
    const filter = page.getByLabel('Filter log entries');
    await expect(filter).toBeVisible({ timeout: 15_000 });
    await filter.selectOption('other');
    await expect(page.locator('[data-log-id="e3590000-0000-4000-8000-000000000001"]')).toBeVisible({
      timeout: 15_000,
    });
    await expect(
      page.locator('[data-log-id="e3590000-0000-4000-8000-000000000001"]'),
    ).toContainText('Confirm');
    await filter.selectOption('image');
    const image = page.locator('[data-log-id="e3590000-0000-4000-8000-000000000002"]');
    await expect(image).toBeVisible();
    await expect(image).toContainText('Image');
  });

  test('(j) a default-template change holds across a tab round trip with no reload', async ({
    page,
  }) => {
    await page.goto('/settings?tab=templates');
    await maybeUnlock(page);
    const select = page.locator('qt-roleplay-templates-card select').first();
    await expect(select).toBeVisible({ timeout: 15_000 });
    const values = await select
      .locator('option')
      .evaluateAll((os) => os.map((o) => (o as HTMLOptionElement).value));
    const target = values.find((v) => v !== '' && v !== (values[0] ?? ''));
    test.skip(!target, 'the fixture carries no template to choose');
    await select.selectOption(target!);
    await expect(select).toHaveValue(target!);
    // Another tab and back — the shared ['chatSettings'] row was seeded from the PUT.
    await page.getByRole('tab', { name: /Chat/ }).first().click();
    await page
      .getByRole('tab', { name: /Templates/ })
      .first()
      .click();
    await expect(page.locator('qt-roleplay-templates-card select').first()).toHaveValue(target!);
  });

  // -------------------------------------------------------------------------
  // GATED
  // -------------------------------------------------------------------------
  test('(c) a keystone memory: the pill, Keystones only, the sixth refused', async () => {
    test.skip(!P4D341_LANDED, 'awaits P4.D341’s keystone column, admission and API (C8)');
  });

  test('(d) a reply naming an absent character chips its first mention; the card pins and unpins', async () => {
    test.skip(
      !P4D347_SERVER_LANDED,
      'awaits P4.D347’s chatAbsentMentions arm (C5) and P4.D344 applying handoffs/p4.d359-salon-conversation.patch',
    );
  });

  test('(g) untick Humanoid → the non-humanoid hint, round-tripped', async () => {
    test.skip(!P4D349_SERVER_LANDED, 'awaits P4.D349’s physicalDescription.humanoid writer (C6)');
  });

  test('(h) a She/Her/Her garment hides from a he character’s pronoun-aware wardrobe', async () => {
    test.skip(!P4D349_SERVER_LANDED, 'awaits P4.D349’s pronounGuidance writer and reader (C6)');
  });

  test('(i) an operator Inventory create refreshes a second dialog’s archetype picker', async () => {
    test.skip(!P4D350_PUBLISH_LANDED, 'awaits P4.D350’s R-1 location-level inventory publish');
  });
});

async function dispatch(body: Record<string, unknown>): Promise<unknown> {
  const res = await fetch(`${BASE_URL}/api/dispatch`, {
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

function runCliWrite(
  cli: string,
  sql: string,
  opts: { mountPoints?: boolean; llmLogs?: boolean } = {},
): void {
  const args = ['db', '--data-dir', INSTANCE_DIR, '--write', sql];
  if (opts.mountPoints) args.splice(1, 0, '--mount-points');
  if (opts.llmLogs) args.splice(1, 0, '--llm-logs');
  const res = spawnSync(cli, args, {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  if (res.status !== 0)
    throw new Error(`CLI write failed (${sql.slice(0, 80)}):\n${res.stdout}\n${res.stderr}`);
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
      const res = await fetch(`${BASE_URL}/health`);
      // 423 = up, instance locked (the server gets no passphrase env).
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${PORT}: ${lastErr}`);
}
