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

/**
 * P4.D121's archive walk needs the sibling server lane's `archived` write
 * (`projectScenarioUpdate`) and `includeArchived` read (`projectScenarioList`)
 * — P4.D120. Until those are on the branch a dispatch verb silently IGNORES the
 * unknown field (memory: `dispatch-verb-ignores-unknown-fields`), so every
 * archive would answer 200 with nothing written and the reds would say nothing
 * about THIS lane. ACTIVATE-AT-UNIFY: the unifier flips this to `true`.
 */
const P4D120_SERVER_LANDED = true;

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

    // --- The instance boots LOCKED (the server spawn passes no passphrase),
    // and every dispatch below needs it open — unlock server-side first, the
    // same verb the UI gate sends. Idempotent when already unlocked.
    await dispatch(ctx, { type: 'unlock', passphrase: E2E_PASSPHRASE });

    // --- Seed through the API. A scenario FILE lives in a document store, so it
    // cannot be planted with SQL (memory:
    // `store-overlay-properties-cannot-be-sql-seeded`) — the project and its
    // Scenarios/ entry are both created the way the app creates them.
    // `listChats` is the REQUEST verb; `chats` is the RESPONSE tag — sending
    // the response tag answers `unknown variant` (caught on this beat's first
    // live run).
    const chats = await dispatch(ctx, { type: 'listChats' });
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
    // Scoped to the transcript: the same body is (correctly) also visible in
    // the picker's preset preview, so a bare getByText matches twice.
    await expect(page.locator('.qt-chat-messages-list').getByText(PRESET_BODY)).toBeVisible();

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

  /**
   * P4.D121 — the archive walk across the manager and the in-chat picker (v4
   * `d25dacc1`).
   *
   * Archive a project scenario in the ScenariosManager → it is gone from the
   * Salon picker (the fetch, not a filter, is what hides it) → "Show archived"
   * reveals it, suffixed "(archived)" and still selectable → restore it in the
   * manager and it comes back unsuffixed.
   */
  test('archiving a scenario hides it from the picker until Show archived reveals it (ACTIVATE-AT-UNIFY, lane P4.D120)', async ({
    page,
  }) => {
    test.skip(
      !P4D120_SERVER_LANDED,
      'awaits P4.D120’s `archived` write + `includeArchived` read (wired at unification)',
    );
    test.setTimeout(120_000);
    const ctx = page.request;
    await dispatch(ctx, { type: 'unlock', passphrase: E2E_PASSPHRASE });

    const created = await dispatch(ctx, {
      type: 'projectCreate',
      project: { name: 'The Archive Shop' },
    });
    const projectId = (created.data?.['project'] as { id: string }).id;
    const chats = await dispatch(ctx, { type: 'listChats' });
    const chatId = ((chats.data as unknown as { id: string; title: string }[]) ?? []).find(
      (c) => c.title === 'Solo Voyage',
    )!.id;
    await dispatch(ctx, { type: 'chatUpdate', chatId, chat: { projectId } });
    await dispatch(ctx, {
      type: 'projectScenarioCreate',
      projectId,
      scenario: { filename: 'attic.md', name: ARCHIVE_SCENARIO, body: 'A dusty attic.' },
    });

    // --- Archive it in the manager (the client half under test).
    await page.goto(`${SCENE_BASE_URL}/prospero/${projectId}`);
    await unlockOnProjectPage(page);
    const card = page.locator('qt-project-scenarios-card');
    await expect(card).toBeVisible({ timeout: 15_000 });
    const newButton = card.getByRole('button', { name: '+ New scenario' });
    if (!(await newButton.isVisible().catch(() => false))) {
      await card.getByRole('button', { name: /^Scenarios \(/ }).click();
      await expect(newButton).toBeVisible({ timeout: 10_000 });
    }
    const row = card.locator('qt-scenario-row', { hasText: ARCHIVE_SCENARIO });
    await expect(row).toBeVisible({ timeout: 10_000 });
    await rowAction(row, 'Archive');
    // Default hidden: the manager re-fetched WITHOUT the flag, so the row is gone.
    await expect(card.locator('qt-scenario-row', { hasText: ARCHIVE_SCENARIO })).toHaveCount(0, {
      timeout: 10_000,
    });

    // --- Gone from the in-chat picker, revealed (suffixed) by the checkbox.
    await page.goto(`${SCENE_BASE_URL}/salon/${chatId}`);
    await unlockIfLocked(page);
    await openChatDrawer(page);
    const control = page.locator('qt-chat-scenario-control');
    await expect(control).toBeVisible({ timeout: 15_000 });
    const picker = control.locator('select');
    // The archived scenario is this project's ONLY one, so with the toggle off
    // there is NOTHING to offer and v4's `hasAnyScenarioOptions` rule hides
    // the whole dropdown (the checkbox deliberately sits OUTSIDE that @if so
    // it stays reachable in exactly this state — the first live run's catch:
    // hidden-entirely is a STRONGER hiding than an option-less select).
    await expect(control.getByRole('checkbox')).toBeVisible({ timeout: 15_000 });
    await expect(picker).toHaveCount(0);

    await control.getByRole('checkbox').check();
    await expect(picker).toBeVisible({ timeout: 15_000 });
    await expect(
      picker.locator('option', { hasText: `${ARCHIVE_SCENARIO} (archived)` }),
    ).toHaveCount(1, { timeout: 10_000 });
    // Archiving hides; it does not forbid — the option is selectable.
    await picker.selectOption({ label: `${ARCHIVE_SCENARIO} (archived)` });
    await expect(picker).toHaveValue(/^project:/);

    // --- Restore it in the manager: it comes back unsuffixed.
    await page.goto(`${SCENE_BASE_URL}/prospero/${projectId}`);
    await unlockOnProjectPage(page);
    const card2 = page.locator('qt-project-scenarios-card');
    const newButton2 = card2.getByRole('button', { name: '+ New scenario' });
    if (!(await newButton2.isVisible().catch(() => false))) {
      await card2.getByRole('button', { name: /^Scenarios \(/ }).click();
      await expect(newButton2).toBeVisible({ timeout: 10_000 });
    }
    // Scoped by the label (unify §3): an empty `name` filter matches ANY
    // checkbox — the wardrobe beat's spelling, adopted here.
    await card2
      .locator('label', { hasText: 'Show archived' })
      .locator('input[type="checkbox"]')
      .check();
    const archivedRow = card2.locator('qt-scenario-row', { hasText: ARCHIVE_SCENARIO });
    await expect(archivedRow).toBeVisible({ timeout: 10_000 });
    await expect(archivedRow).toContainText('Archived');
    // The default radio is disabled while archived — it can never win.
    await expect(archivedRow.locator('input[type="radio"]')).toBeDisabled();
    await rowAction(archivedRow, 'Restore');
    await expect(
      card2.locator('qt-scenario-row', { hasText: ARCHIVE_SCENARIO }),
    ).not.toContainText('Archived', { timeout: 10_000 });
  });
});

/** The scenario this lane's archive walk creates, archives, and restores. */
const ARCHIVE_SCENARIO = 'The Attic';

/**
 * Click a row action whichever layout the row is in: the row is
 * container-query adaptive, so inside the dense project card it renders the
 * narrow `⋮` kebab instead of inline buttons.
 */
async function rowAction(row: ReturnType<Page['locator']>, name: string): Promise<void> {
  const inline = row.getByRole('button', { name, exact: true });
  if (await inline.isVisible().catch(() => false)) {
    await inline.click();
    return;
  }
  await row.getByRole('button', { name: /^More actions for / }).click();
  await row.getByRole('menuitem', { name, exact: true }).click();
}

/** The project page has no chat transcript, so it needs its own unlock gate. */
async function unlockOnProjectPage(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const card = page.locator('qt-project-scenarios-card');
  await expect(passphrase.or(card).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
  await expect(card).toBeVisible({ timeout: 15_000 });
}

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
      // 423 is "server up, instance locked" — the normal state of a fresh
      // fixture boot (the server gets no passphrase env); the walk unlocks
      // through the dispatch verb / the UI gate. The sibling own-server specs
      // (wardrobe-flow &c.) treat it the same way. Caught by this beat's FIRST
      // live run at the 8f910137-round unification: `res.ok` alone can never
      // pass on a locked fixture.
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${SCENE_PORT}: ${lastErr}`);
}
