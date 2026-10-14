import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, openSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { expect, test, type APIRequestContext, type Page } from './support/fixtures';

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
import { MOCK_LLM_REPLY, startMockLlm, type MockLlm } from './support/mock-llm';

/**
 * P4.D302 — scenarios from anywhere, the SPA (v4 `545e299bb`): "Browse all
 * scenarios…" on both pickers, the browser "Every scene on every shelf", a
 * store pick under "Elsewhere", and the in-chat control's recorded source
 * (provenance, revised / gone, Reload from source, the Notes box).
 *
 * Its own instance on port 4340 (the round's §R.9 port for this lane), copied
 * from the `salon-scenario-flow` recipe. Scenario FILES live in document
 * stores, so every seed goes through dispatch (`projectCreate`,
 * `projectScenarioCreate`, `scenarioCreate`, `projectScenarioUpdate`,
 * `projectScenarioDelete`) — never SQL.
 *
 * **UNGATED beats** hold on BOTH servers: the dropdown renders with "Browse
 * all scenarios…", choosing it opens the browser and leaves the select where
 * it was, Cancel closes it, and Escape inside it closes ONLY the browser — on
 * New Chat and in the Salon sidebar. (On `main`'s server the three new verbs
 * answer serde's `unknown variant`, so the browser shows "The shelves could not
 * be reached: …" — a correct v4-shaped degradation these beats do not read.)
 *
 * **GATED beats** need P4.D295's bodies (`scenarioEverywhere`,
 * `scenarioReadByRef`, `chatGetScenarioSource`, `scenarioRef` / `reload` on
 * `chatSetScenario` / `chatCreate`). ACTIVATE-AT-UNIFY (§S.9, the
 * `P4D279_SERVER_LANDED` precedent): the unifier flips the constant to `true`.
 */
const P4D288_SERVER_LANDED = false;
const GATE_REASON =
  'awaits P4.D295’s scenarioEverywhere / scenarioReadByRef / chatGetScenarioSource bodies and scenarioRef + reload (wired at unification)';

const BROWSE_PORT = 4340;
const BROWSE_BASE_URL = `http://127.0.0.1:${BROWSE_PORT}`;
/** This spec's own mock LLM (the chat-create greeting) — clear of every shared port. */
const BROWSE_MOCK_LLM_PORT = 45340;
const BROWSE_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'scenario-browser-instance');
const BROWSE_DATA_DIR = resolve(BROWSE_INSTANCE_DIR, 'data');
const BROWSE_SERVER_LOG = resolve(ARTIFACTS_DIR, 'scenario-browser-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'salon-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'salon-mount.db');
const USER_TABLES = ['characters', 'chats', 'tags', 'groups', 'projects', 'files'];

/** The seeded shelves. */
const HOME_PROJECT = 'The Scene Shop';
const FAR_PROJECT = 'The Far House';
const HOME_SCENE = { filename: 'long-gallery.md', name: 'The Long Gallery', body: 'The long gallery, hung with portraits nobody will name.' };
const FAR_SCENE = { filename: 'the-attic.md', name: 'The Attic', body: 'Dust, trunks and a single round window.' };
const FAR_ARCHIVED = { filename: 'old-wing.md', name: 'The Old Wing', body: 'Sheeted furniture and a stopped clock.' };
const GENERAL_SCENE = { filename: 'the-moor.md', name: 'The Moor', body: 'Fog on the moor at dawn.' };

let server: ChildProcess | undefined;
let mock: MockLlm | undefined;
let homeProjectId = '';
let farProjectId = '';
/** Why the General scenario could not be seeded, when it could not. */
let generalSeedRefusal: string | null = null;

/**
 * Dispatch outside a test (the seeds in `beforeAll`, where Playwright offers no
 * request fixture) — plain `fetch`, the same body.
 */
async function dispatchRaw(req: unknown): Promise<{ type?: string; data?: Record<string, unknown> }> {
  const res = await fetch(`${BROWSE_BASE_URL}/api/dispatch`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(req),
  });
  return ((await res.json().catch(() => null)) as { type?: string; data?: Record<string, unknown> } | null) ?? {};
}

async function dispatch(
  ctx: APIRequestContext,
  req: unknown,
): Promise<{ type?: string; data?: Record<string, unknown> }> {
  const res = await ctx.post(`${BROWSE_BASE_URL}/api/dispatch`, { data: req });
  return (
    ((await res.json().catch(() => null)) as {
      type?: string;
      data?: Record<string, unknown>;
    } | null) ?? {}
  );
}

test.describe('P4.D302 — the scenario browser and the recorded source', () => {
  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(BROWSE_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(BROWSE_DATA_DIR, { recursive: true });
    mkdirSync(resolve(BROWSE_DATA_DIR, 'files'), { recursive: true });
    copyFileSync(MAIN_FIXTURE, resolve(BROWSE_DATA_DIR, 'quilltap.db'));
    if (existsSync(MOUNT_FIXTURE)) {
      copyFileSync(MOUNT_FIXTURE, resolve(BROWSE_DATA_DIR, 'quilltap-mount-index.db'));
    }
    writeFileSync(
      resolve(BROWSE_DATA_DIR, 'quilltap.dbkey'),
      makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE),
    );
    for (const table of USER_TABLES) {
      runCliWrite(
        cli,
        `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`,
      );
    }
    // The gated create beat's greeting rides this spec's own mock (the
    // global-setup profile rewrite, aimed at a port nobody else uses).
    runCliWrite(
      cli,
      `UPDATE connection_profiles SET baseUrl = 'http://127.0.0.1:${BROWSE_MOCK_LLM_PORT}/v1' WHERE provider = 'OPENAI_COMPATIBLE';`,
    );
    mock = await startMockLlm(MOCK_LLM_REPLY, BROWSE_MOCK_LLM_PORT);

    const logFd = openSync(BROWSE_SERVER_LOG, 'w');
    server = spawn(
      web,
      [
        '--host',
        '127.0.0.1',
        '--port',
        String(BROWSE_PORT),
        '--data-dir',
        BROWSE_INSTANCE_DIR,
        '--spa-dir',
        spaDir(),
      ],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    server.unref();
    await waitForHealth();

    // --- The shelves: two projects with a scenario each (one archived on the
    // far shelf), and a General scenario.
    await dispatchRaw({ type: 'unlock', passphrase: E2E_PASSPHRASE });
    const home = await dispatchRaw({ type: 'projectCreate', project: { name: HOME_PROJECT } });
    homeProjectId = (home.data?.['project'] as { id: string }).id;
    const far = await dispatchRaw({ type: 'projectCreate', project: { name: FAR_PROJECT } });
    farProjectId = (far.data?.['project'] as { id: string }).id;
    for (const [projectId, scenario] of [
      [homeProjectId, HOME_SCENE],
      [farProjectId, FAR_SCENE],
      [farProjectId, FAR_ARCHIVED],
    ] as const) {
      const made = await dispatchRaw({ type: 'projectScenarioCreate', projectId, scenario });
      expect(made.type, JSON.stringify(made)).not.toBe('error');
    }
    await dispatchRaw({
      type: 'projectScenarioUpdate',
      projectId: farProjectId,
      scenarioPath: `Scenarios/${FAR_ARCHIVED.filename}`,
      scenario: { archived: true },
    });
    // The General shelf needs the "Quilltap General" mount, which a server
    // provisions at boot; recorded rather than asserted, so a fixture without
    // it skips only the General beat (the ungated beats never read it).
    const general = await dispatchRaw({ type: 'scenarioCreate', scenario: GENERAL_SCENE });
    generalSeedRefusal = general.type === 'error' ? JSON.stringify(general.data ?? general) : null;
    if (generalSeedRefusal) console.warn(`[scenario-browser-flow] General seed refused: ${generalSeedRefusal}`);
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
    rmSync(BROWSE_INSTANCE_DIR, { recursive: true, force: true });
  });

  // -------------------------------------------------------------------------
  // UNGATED — hold on both servers
  // -------------------------------------------------------------------------

  test('New Chat: Browse opens the browser, leaves the select, Cancel closes it', async ({ page }) => {
    await openNewChat(page);
    const select = page.locator('#new-chat-scenario-select');
    await expect(select).toBeVisible();
    await expect(select.locator('option', { hasText: 'Browse all scenarios…' })).toHaveCount(1);
    const before = await select.inputValue();

    await select.selectOption('__browse__');
    await expect(browser(page)).toBeVisible({ timeout: 10_000 });
    // An action, never a selection: the select snapped back.
    await expect(select).toHaveValue(before);

    await browser(page).getByRole('button', { name: 'Cancel', exact: true }).click();
    await expect(browser(page)).toHaveCount(0);
    await expect(select).toHaveValue(before);
  });

  test('New Chat: Escape inside the browser closes ONLY the browser', async ({ page }) => {
    await openNewChat(page);
    await page.locator('#new-chat-scenario-select').selectOption('__browse__');
    await expect(browser(page)).toBeVisible({ timeout: 10_000 });
    await expect(page.getByLabel('Search scenarios')).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(browser(page)).toHaveCount(0);
    await expect(page.getByRole('heading', { name: 'New Chat', exact: true })).toBeVisible();
    await expect(page.locator('#new-chat-scenario-select')).toBeVisible();
  });

  test('Salon: Browse, Cancel and Escape on the in-chat control leave the sidebar open', async ({
    page,
  }) => {
    const chatId = await soloVoyage(page.request);
    await page.goto(`${BROWSE_BASE_URL}/salon/${chatId}`);
    await unlockIfLocked(page);
    await openChatDrawer(page);
    const control = page.locator('qt-chat-scenario-control');
    const select = control.locator('#chat-scenario-select');
    await expect(select).toBeVisible({ timeout: 15_000 });
    await expect(select.locator('option', { hasText: 'Browse all scenarios…' })).toHaveCount(1);
    const before = await select.inputValue();

    await select.selectOption('__browse__');
    await expect(browser(page)).toBeVisible({ timeout: 10_000 });
    await expect(select).toHaveValue(before);
    await browser(page).getByRole('button', { name: 'Cancel', exact: true }).click();
    await expect(browser(page)).toHaveCount(0);

    await select.selectOption('__browse__');
    await expect(browser(page)).toBeVisible({ timeout: 10_000 });
    await page.keyboard.press('Escape');
    await expect(browser(page)).toHaveCount(0);
    // The sidebar (and the control inside it) is still open.
    await expect(control).toBeVisible();
    await expect(select).toHaveValue(before);
  });

  // -------------------------------------------------------------------------
  // GATED — need P4.D295's server (§S.9)
  // -------------------------------------------------------------------------

  test('New Chat: a scene from ANOTHER project, picked in the browser, names its shelf after Create', async ({
    page,
  }) => {
    test.skip(!P4D288_SERVER_LANDED, GATE_REASON);
    test.setTimeout(90_000);
    await openNewChat(page);
    await page.locator('.new-chat-character-picker button').first().click();
    await expect(page.getByText('Speaks First')).toBeVisible();

    const select = page.locator('#new-chat-scenario-select');
    await select.selectOption('__browse__');
    const dialog = browser(page);
    await expect(dialog.locator('nav[aria-label="Scenario owners"]')).toBeVisible({ timeout: 15_000 });
    await expect(dialog.getByRole('button', { name: /Projects$/ })).toBeVisible();
    await dialog.locator('nav li button', { hasText: FAR_PROJECT }).click();
    await dialog.getByRole('option', { name: new RegExp(FAR_SCENE.name) }).click();
    await expect(dialog.getByText(`${FAR_PROJECT} › ${FAR_SCENE.name}`)).toBeVisible();
    await dialog.getByRole('button', { name: 'Use this scenario', exact: true }).click();
    await expect(dialog).toHaveCount(0);

    const label = `${FAR_PROJECT} › ${FAR_SCENE.name}`;
    await expect(select.locator('optgroup[label="Elsewhere"] option')).toHaveText(label);
    await expect(select).toHaveValue(/^store:/);
    await expect(page.getByText(FAR_SCENE.body)).toBeVisible({ timeout: 10_000 });

    await page.getByRole('button', { name: 'Create Chat' }).click();
    await expect(page).toHaveURL(/\/salon\/[0-9a-f-]{16,}/, { timeout: 30_000 });
    await unlockIfLocked(page);
    await openChatDrawer(page);
    const control = page.locator('qt-chat-scenario-control');
    await expect(control.getByText(/^From:/)).toHaveText(`From: ${label}`, { timeout: 15_000 });
    await expect(control.locator('#chat-scenario-select')).toHaveValue(/^store:/);
  });

  test('the browser searches every shelf, says when nothing answers, and Show archived reveals', async ({
    page,
  }) => {
    test.skip(!P4D288_SERVER_LANDED, GATE_REASON);
    await openNewChat(page);
    await page.locator('#new-chat-scenario-select').selectOption('__browse__');
    const dialog = browser(page);
    await expect(dialog.locator('nav[aria-label="Scenario owners"]')).toBeVisible({ timeout: 15_000 });

    // Narrow to one shelf, then search: the search spans EVERY shelf.
    await dialog.locator('nav li button', { hasText: HOME_PROJECT }).click();
    await page.getByLabel('Search scenarios').fill('round window');
    await expect(dialog.getByRole('option')).toHaveCount(1);
    await expect(dialog.getByRole('option')).toContainText(FAR_SCENE.name);
    await page.getByLabel('Search scenarios').fill('conservatory');
    await expect(dialog.getByText('Nothing on that shelf answers to the name.')).toBeVisible();

    await page.getByLabel('Search scenarios').fill('');
    await expect(dialog.getByRole('option', { name: new RegExp(FAR_ARCHIVED.name) })).toHaveCount(0);
    await dialog.locator('label', { hasText: 'Show archived' }).locator('input').check();
    await dialog.locator('nav li button', { hasText: FAR_PROJECT }).click();
    const archived = dialog.getByRole('option', { name: new RegExp(FAR_ARCHIVED.name) });
    await expect(archived).toBeVisible({ timeout: 10_000 });
    await expect(archived.locator('.qt-badge-warning')).toHaveText('archived');
  });

  test('a General pick snaps to its native kind — no Elsewhere', async ({ page }) => {
    test.skip(!P4D288_SERVER_LANDED, GATE_REASON);
    expect(generalSeedRefusal, 'the General scenario seed was refused').toBeNull();
    await openNewChat(page);
    const select = page.locator('#new-chat-scenario-select');
    await select.selectOption('__browse__');
    const dialog = browser(page);
    await expect(dialog.locator('nav[aria-label="Scenario owners"]')).toBeVisible({ timeout: 15_000 });
    await page.getByLabel('Search scenarios').fill(GENERAL_SCENE.name);
    await dialog.getByRole('option', { name: new RegExp(GENERAL_SCENE.name) }).dblclick();
    await expect(dialog).toHaveCount(0);
    await expect(select).toHaveValue(/^general:/);
    await expect(select.locator('optgroup[label="Elsewhere"]')).toHaveCount(0);
  });

  test('in-chat: the Notes box, revised → keep vs Reload (the Host), then gone', async ({ page }) => {
    test.skip(!P4D288_SERVER_LANDED, GATE_REASON);
    test.setTimeout(120_000);
    const ctx = page.request;
    const chatId = await soloVoyage(ctx);
    await dispatch(ctx, { type: 'chatUpdate', chatId, chat: { projectId: homeProjectId } });
    const homePath = `Scenarios/${HOME_SCENE.filename}`;
    const set = await dispatch(ctx, { type: 'chatSetScenario', chatId, projectScenarioPath: homePath });
    expect(set.data?.['message']).toBeTruthy();

    await page.goto(`${BROWSE_BASE_URL}/salon/${chatId}`);
    await unlockIfLocked(page);
    await openChatDrawer(page);
    const control = page.locator('qt-chat-scenario-control');
    await expect(control.getByText(/^From:/)).toHaveText(`From: ${HOME_PROJECT} › ${HOME_SCENE.name}`, {
      timeout: 15_000,
    });

    // --- The Notes box: saved, and re-seeded from the source on reopen.
    await control.getByLabel('Notes beneath the scene').fill('And the lamps are low.');
    await save(page);
    await expect(page.getByText(/Scenario (updated|unchanged)/)).toBeVisible({ timeout: 10_000 });
    await page.reload();
    await unlockIfLocked(page);
    await openChatDrawer(page);
    await expect(control.getByLabel('Notes beneath the scene')).toHaveValue('And the lamps are low.', {
      timeout: 15_000,
    });

    // --- The shelf copy revised: a plain save keeps the scene, Reload re-reads it.
    await dispatch(ctx, {
      type: 'projectScenarioUpdate',
      projectId: homeProjectId,
      scenarioPath: homePath,
      scenario: { body: 'The long gallery, the portraits turned to the wall.' },
    });
    await page.reload();
    await unlockIfLocked(page);
    await openChatDrawer(page);
    await expect(control.getByText(/the shelf copy has been revised/)).toBeVisible({ timeout: 15_000 });
    const before = await scenarioChips(page).count();
    await save(page);
    await expect(page.getByText('Scenario unchanged')).toBeVisible({ timeout: 10_000 });
    await expect(scenarioChips(page)).toHaveCount(before);
    await control.getByRole('button', { name: 'Reload from source' }).click();
    await expect(page.getByText('Scenario updated')).toBeVisible({ timeout: 10_000 });
    await expect(scenarioChips(page)).toHaveCount(before + 1);

    // --- The shelf copy gone: said so, and the scene kept as Custom text.
    await dispatch(ctx, { type: 'projectScenarioDelete', projectId: homeProjectId, scenarioPath: homePath });
    await page.reload();
    await unlockIfLocked(page);
    await openChatDrawer(page);
    await expect(control.getByText(/the shelf copy is gone/)).toBeVisible({ timeout: 15_000 });
    await expect(control.locator('#chat-scenario-select')).toHaveValue('__custom__');
    await expect(control.getByLabel('Custom scenario text')).toHaveValue(/portraits turned to the wall/);
  });
});

// ---------------------------------------------------------------------------
// Helpers (the `salon-scenario-flow` recipe)
// ---------------------------------------------------------------------------

function browser(page: Page) {
  return page.locator('[role="dialog"]').filter({ hasText: 'Every scene on every shelf' });
}

async function soloVoyage(ctx: APIRequestContext): Promise<string> {
  await dispatch(ctx, { type: 'unlock', passphrase: E2E_PASSPHRASE });
  const chats = await dispatch(ctx, { type: 'listChats' });
  const chatId = ((chats.data as unknown as { id: string; title: string }[]) ?? []).find(
    (c) => c.title === 'Solo Voyage',
  )!.id;
  expect(chatId).toBeTruthy();
  return chatId;
}

async function openNewChat(page: Page): Promise<void> {
  await dispatch(page.request, { type: 'unlock', passphrase: E2E_PASSPHRASE });
  await page.goto(`${BROWSE_BASE_URL}/salon/new`);
  const passphrase = page.locator('#qt-passphrase');
  const heading = page.getByRole('heading', { name: 'New Chat', exact: true });
  await expect(passphrase.or(heading).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
  await expect(heading).toBeVisible({ timeout: 15_000 });
}

/** The collapsed Host chips a scenario change creates, by their kind label. */
function scenarioChips(page: Page) {
  return page
    .locator('.qt-chat-announcement-chip')
    .filter({ has: page.locator('.qt-chat-system-bar-kind', { hasText: 'scenario change' }) });
}

async function save(page: Page): Promise<void> {
  await page
    .locator('qt-chat-scenario-control')
    .getByRole('button', { name: 'Change scenario' })
    .click();
}

async function openChatDrawer(page: Page): Promise<void> {
  const sidebar = page.locator('qt-chat-sidebar');
  await expect(sidebar).toBeVisible({ timeout: 15_000 });
  const expand = page.getByRole('button', { name: 'Expand chat sidebar' });
  if (await expand.count()) await expand.click();
  const header = page
    .locator('qt-chat-sidebar .qt-collapsible-card-header')
    .filter({ hasText: 'Chat' })
    .first();
  await header.click();
}

async function unlockIfLocked(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const messages = page.locator('.qt-chat-messages-list');
  await expect(passphrase.or(messages).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
  await expect(messages).toBeVisible({ timeout: 15_000 });
}

function runCliWrite(cli: string, sql: string): void {
  const res = spawnSync(cli, ['db', '--data-dir', BROWSE_INSTANCE_DIR, '--write', sql], {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  if (res.status !== 0) {
    const out = `${res.stdout}${res.stderr}`;
    if (out.includes('no such table') || out.includes('no such column: userId')) {
      console.warn(`fixture rewrite skipped (not user-scoped): ${sql}`);
      return;
    }
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
      const res = await fetch(`${BROWSE_BASE_URL}/health`);
      // 423 = up, instance locked (the server gets no passphrase env).
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${BROWSE_PORT}: ${lastErr}`);
}
