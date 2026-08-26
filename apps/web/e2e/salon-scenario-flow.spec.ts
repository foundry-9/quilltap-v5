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

/**
 * P4.D116 — the in-chat scenario picker (v4 `44a8137e`).
 *
 * The walk: open a chat whose scene is already set → the picker opens on that
 * scene rather than on "Custom…" (v4's chat GET never projected `scenarioText`,
 * which is the whole reason it could only ever open on "Custom…") → pick a
 * project scenario → the Host announces the revision in v4's words → re-pick
 * the same scene → nothing new is announced → clear it → the Host draws the
 * scene aside.
 *
 * ACTIVATE-AT-UNIFY. `chatSetScenario` is the sibling server lane's (P4.D115);
 * until it is on the branch the dispatch verb's unknown-field tolerance would
 * make every save a silent 200-with-nothing-written (memory:
 * `dispatch-verb-ignores-unknown-fields`), and the reds would say nothing about
 * this lane. The unifier flips {@link P4D115_SERVER_LANDED} to `true`.
 */
const P4D115_SERVER_LANDED = true;

const SCENE_PORT = 4330;
const SCENE_BASE_URL = `http://127.0.0.1:${SCENE_PORT}`;
const SCENE_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'scenario-instance');
const SCENE_DATA_DIR = resolve(SCENE_INSTANCE_DIR, 'data');
const SCENE_SERVER_LOG = resolve(ARTIFACTS_DIR, 'scenario-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'salon-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'salon-mount.db');

/** Every fixture table this walk reads is filtered by userId. */
const USER_TABLES = ['characters', 'chats', 'tags', 'groups', 'projects', 'files'];

/**
 * The scene the chat is seeded with, and the preset it is then pointed at. The
 * seed is deliberately NOT any preset's body, so the picker must open on
 * "Custom…" with the text in the box — the other half of the projection proof.
 */
const SEEDED_SCENE = 'A cellar, lit by one lamp, and the rain going on outside.';
const PRESET_NAME = 'The Long Gallery';
const PRESET_BODY = 'The long gallery, hung with portraits nobody will name.';

/** v4 `buildScenarioRevisionContent` / `SCENARIO_CLEARED_CONTENT`, verbatim. */
const REVISION_LEAD = 'The Host revises the scene for the proceedings:';
const CLEARED_SENTENCE =
  'The Host draws the previous scene aside; the company carries on without a set scene.';

let server: ChildProcess | undefined;

async function dispatch(
  ctx: APIRequestContext,
  req: unknown,
): Promise<{ type?: string; data?: Record<string, unknown> }> {
  const res = await ctx.post(`${SCENE_BASE_URL}/api/dispatch`, { data: req });
  return (
    ((await res.json().catch(() => null)) as {
      type?: string;
      data?: Record<string, unknown>;
    } | null) ?? {}
  );
}

test.describe('P4.D116 — the in-chat scenario picker', () => {
  test.skip(!P4D115_SERVER_LANDED, 'awaits P4.D115’s chatSetScenario verb (wired at unification)');

  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(SCENE_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(SCENE_DATA_DIR, { recursive: true });
    mkdirSync(resolve(SCENE_DATA_DIR, 'files'), { recursive: true });
    copyFileSync(MAIN_FIXTURE, resolve(SCENE_DATA_DIR, 'quilltap.db'));
    if (existsSync(MOUNT_FIXTURE)) {
      copyFileSync(MOUNT_FIXTURE, resolve(SCENE_DATA_DIR, 'quilltap-mount-index.db'));
    }
    writeFileSync(
      resolve(SCENE_DATA_DIR, 'quilltap.dbkey'),
      makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE),
    );
    for (const table of USER_TABLES) {
      runCliWrite(
        cli,
        `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`,
      );
    }

    const logFd = openSync(SCENE_SERVER_LOG, 'w');
    server = spawn(
      web,
      [
        '--host',
        '127.0.0.1',
        '--port',
        String(SCENE_PORT),
        '--data-dir',
        SCENE_INSTANCE_DIR,
        '--spa-dir',
        spaDir(),
      ],
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
    rmSync(SCENE_INSTANCE_DIR, { recursive: true, force: true });
  });

  test('opens on the scene in force, changes it, no-ops, and clears it', async ({ page }) => {
    test.setTimeout(120_000);
    const ctx = page.request;

    // --- Seed through the API. A scenario FILE lives in a document store, so it
    // cannot be planted with SQL (memory:
    // `store-overlay-properties-cannot-be-sql-seeded`) — the project and its
    // Scenarios/ entry are both created the way the app creates them.
    const chats = await dispatch(ctx, { type: 'chats' });
    const chatId = ((chats.data as unknown as { id: string; title: string }[]) ?? []).find(
      (c) => c.title === 'Solo Voyage',
    )!.id;
    expect(chatId).toBeTruthy();

    const created = await dispatch(ctx, {
      type: 'projectCreate',
      project: { name: 'The Scene Shop' },
    });
    const projectId = (created.data?.['project'] as { id: string }).id;
    expect(projectId).toBeTruthy();

    const madeScenario = await dispatch(ctx, {
      type: 'projectScenarioCreate',
      projectId,
      scenario: { filename: 'long-gallery.md', name: PRESET_NAME, body: PRESET_BODY },
    });
    expect(madeScenario.type).not.toBe('error');

    await dispatch(ctx, { type: 'chatUpdate', chatId, chat: { projectId } });
    const seeded = await dispatch(ctx, {
      type: 'chatSetScenario',
      chatId,
      scenario: SEEDED_SCENE,
    });
    expect(seeded.data?.['message']).toBe('Scenario updated');

    // --- The picker opens on the scene in force (the GET projection).
    await page.goto(`${SCENE_BASE_URL}/salon/${chatId}`);
    await unlockIfLocked(page);
    await openChatDrawer(page);

    const control = page.locator('qt-chat-scenario-control');
    await expect(control).toBeVisible({ timeout: 15_000 });
    const picker = control.locator('select');
    await expect(picker).toBeVisible({ timeout: 15_000 });
    // No preset matches the seeded text, so Custom holds it — the projection is
    // demonstrably being read (before `44a8137e` the box was always EMPTY).
    await expect(picker).toHaveValue('__custom__');
    await expect(control.locator('textarea')).toHaveValue(SEEDED_SCENE);

    // --- Pick the project scenario: the Host announces the revision.
    const before = await scenarioChips(page).count();
    await picker.selectOption({ label: PRESET_NAME });
    await expect(control.locator('textarea')).toHaveCount(0);
    await save(page);
    await expect(page.getByText('Scenario updated')).toBeVisible({ timeout: 10_000 });
    await expect(scenarioChips(page)).toHaveCount(before + 1);

    const chip = scenarioChips(page).last();
    await chip.click();
    await expect(page.getByText(REVISION_LEAD)).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText(PRESET_BODY)).toBeVisible();

    // --- Re-picking the scene already in force writes nothing and says so.
    await page.reload();
    await unlockIfLocked(page);
    await openChatDrawer(page);
    // The projection now resolves to the PRESET, so the picker opens on it —
    // the exact case v4's missing projection could never show.
    await expect(control.locator('select')).toHaveValue(/^project:/);
    await save(page);
    await expect(page.getByText('Scenario unchanged')).toBeVisible({ timeout: 10_000 });
    await expect(scenarioChips(page)).toHaveCount(before + 1);

    // --- An empty Custom box clears the scene.
    await control.locator('select').selectOption('__custom__');
    await control.locator('textarea').fill('');
    await save(page);
    await expect(page.getByText('Scenario cleared')).toBeVisible({ timeout: 10_000 });
    await expect(scenarioChips(page)).toHaveCount(before + 2);
    await scenarioChips(page).last().click();
    await expect(page.getByText(CLEARED_SENTENCE)).toBeVisible({ timeout: 10_000 });
  });
});

/** The collapsed Host chips this walk creates, by their kind label. */
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
  const res = spawnSync(cli, ['db', '--data-dir', SCENE_INSTANCE_DIR, '--write', sql], {
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
      const res = await fetch(`${SCENE_BASE_URL}/health`);
      if (res.ok) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${SCENE_PORT}: ${lastErr}`);
}
