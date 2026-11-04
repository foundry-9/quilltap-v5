import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, openSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { expect, test, type Page } from './support/fixtures';

import { makeDbKeyFile } from './support/dbkey';
import {
  ARTIFACTS_DIR,
  cliBinary,
  E2E_PASSPHRASE,
  FIXTURE_USER,
  REPO_ROOT,
  FIXTURES_DIR,
  SINGLE_USER_ID,
  spaDir,
  TEST_PEPPER,
  webBinary,
} from './support/env';
import { MOCK_LLM_REPLY, startMockLlm, type MockLlm } from './support/mock-llm';

/**
 * P4.D346 — Pascal operator intervention, the SPA half (v4 `839575d89` +
 * `8ae8a127c` + `b02d902f3` + `2041bb875`, pinned at `236c04e35`): the
 * Workbench's raised hands, the library's hand mark, the Automation card's
 * "Hold intervenable rolls for me" row, and the Salon's hold dialog.
 *
 * Its OWN instance on port 4354 (mock LLM 45352 — the `236c04e35` round's §R.9
 * ports for this lane), copied from `workbench-triggers-effects-flow.spec.ts`'s
 * recipe (R-7: one new file, helpers copied, no shared-support edits): the
 * Automation beats toggle `customTools`, which the shared server's specs
 * depend on, and the Salon beats switch trigger phrases on and plant a
 * definition. Every writing beat restores what it changed.
 *
 * **LIVE beats** ride what `main` already has: (W1) the hands and the live
 * JSON (client-side), (S1) the Automation row's default and its dead state
 * (the existing `customTools` PUT arm; the gate is a client rule).
 *
 * **GATED beats** — ACTIVATE-AT-UNIFY (§S.9): the unifier flips each
 * constant. NAMED CONSTANTS, never capability probes (R-11): the settings PUT
 * accepts and DROPS an unknown key on `main`, and a definition carrying
 * `intervention` loads (the key tolerated as unknown) — a probe would lie
 * twice.
 */
const P4D345_SERVER_LANDED = true;
const P4D345_GATE_REASON =
  'awaits P4.D345’s four chatCustomToolIntervention* verbs, the hold on the run service and the library’s intervention key (wired at unification)';
const P4D345_STOP_VETO_LANDED = true;
const P4D345_STOP_VETO_GATE_REASON =
  'awaits P4.D345’s Stop veto through P4.D348’s TurnStopReach (outcome a / b of its item 9) — wired at unification';
const P4D341_P4D356_SETTINGS_LANDED = true;
const P4D341_P4D356_GATE_REASON =
  'awaits P4.D341’s hydrated customToolIntervention and P4.D356’s PUT arm (SettingsArmOwner) — wired at unification';

const IV_PORT = 4354;
const IV_BASE_URL = `http://127.0.0.1:${IV_PORT}`;
/** This spec's own mock LLM — clear of every shared port. */
const IV_MOCK_LLM_PORT = 45352;
const IV_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'salon-intervention-instance');
const IV_DATA_DIR = resolve(IV_INSTANCE_DIR, 'data');
const IV_SERVER_LOG = resolve(ARTIFACTS_DIR, 'salon-intervention-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'salon-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'salon-mount.db');

/** The intervenable definition the Salon beats deal (the order's bytes; the phrase is 12 characters, inside 3–80). */
const CROUPIER = {
  $schema: '/schemas/qtap-custom-tool.schema.json',
  name: 'croupier_e2e',
  title: 'Croupier',
  description: 'Cuts the deck for the e2e walk.',
  triggers: ['cut the deck'],
  intervention: { enabled: true },
  roll: { min: 1, max: 10, round: true },
  outcomes: [
    { when: { gte: 6 }, message: 'High card: {{value}}.', state: 'success' },
    { when: true, message: 'Low card: {{value}}.', state: 'failure' },
  ],
};
const CROUPIER_PATH = 'Tools/croupier_e2e.tool.json';
const CHAT_TITLE = 'Group Expedition';

let server: ChildProcess | undefined;
let mock: MockLlm | undefined;

test.use({ baseURL: IV_BASE_URL });

test.beforeAll(async () => {
  test.setTimeout(120_000);
  const web = webBinary();
  const cli = cliBinary();

  rmSync(IV_INSTANCE_DIR, { recursive: true, force: true });
  mkdirSync(IV_DATA_DIR, { recursive: true });
  mkdirSync(resolve(IV_DATA_DIR, 'files'), { recursive: true });
  copyFileSync(MAIN_FIXTURE, resolve(IV_DATA_DIR, 'quilltap.db'));
  if (existsSync(MOUNT_FIXTURE)) {
    copyFileSync(MOUNT_FIXTURE, resolve(IV_DATA_DIR, 'quilltap-mount-index.db'));
  }
  writeFileSync(resolve(IV_DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));
  // The fixture's user-scoped rows belong to FIXTURE_USER; the server acts as
  // SINGLE_USER_ID (the `scenario-browser-flow` rewrite, verbatim).
  for (const table of [
    'chats',
    'connection_profiles',
    'api_keys',
    'chat_settings',
    'characters',
    'tags',
    'projects',
    'memories',
    'files',
    'image_profiles',
    'groups',
  ]) {
    runCliWrite(cli, `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`);
  }
  runCliWrite(cli, `UPDATE users SET id = '${SINGLE_USER_ID}' WHERE id = '${FIXTURE_USER}';`);
  runCliWrite(
    cli,
    `UPDATE connection_profiles SET baseUrl = 'http://127.0.0.1:${IV_MOCK_LLM_PORT}/v1' WHERE provider = 'OPENAI_COMPATIBLE';`,
  );
  mock = await startMockLlm(MOCK_LLM_REPLY, IV_MOCK_LLM_PORT);

  // The salon pair carries NO `instance_settings` table; the boot's "Quilltap
  // General" provisioning records its mount there, and the Workbench's save
  // picker offers that store. Planted from the committed D23 dump, never
  // hand-written (the `scenario-browser-flow` precedent).
  const freshSchema = JSON.parse(
    readFileSync(resolve(REPO_ROOT, 'crates/quilltap-core/src/services/provisioning/fresh_schema.json'), 'utf8'),
  ) as { main: string[]; mountIndex: string[] };
  const settingsDdl = freshSchema.main.find((sql) => sql.startsWith('CREATE TABLE "instance_settings"'));
  if (!settingsDdl) throw new Error('fresh_schema.json carries no instance_settings CREATE TABLE');
  const ddl = spawnSync(cli, ['db', '--data-dir', IV_INSTANCE_DIR, '--write', settingsDdl], {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  if (ddl.status !== 0 && !`${ddl.stdout}${ddl.stderr}`.includes('already exists')) {
    throw new Error(`instance_settings plant failed:\n${ddl.stdout}\n${ddl.stderr}`);
  }
  // The pair also predates the groups schema, and the save picker's
  // destinations verb reads `groups` (main) and `group_doc_mount_links`
  // (mount index) on every request. Empty tables are the honest state; the DDL
  // is the dump's.
  for (const [partition, table] of [
    ['main', 'groups'],
    ['mountIndex', 'group_doc_mount_links'],
  ] as const) {
    const create = freshSchema[partition].find((sql) => sql.startsWith(`CREATE TABLE "${table}"`));
    if (!create) throw new Error(`fresh_schema.json carries no ${table} CREATE TABLE`);
    const args = ['db', '--data-dir', IV_INSTANCE_DIR, '--write', create.replace('CREATE TABLE', 'CREATE TABLE IF NOT EXISTS')];
    if (partition === 'mountIndex') args.splice(1, 0, '--mount-points');
    const res = spawnSync(cli, args, {
      env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
      encoding: 'utf8',
    });
    if (res.status !== 0) throw new Error(`${table} plant failed:\n${res.stdout}\n${res.stderr}`);
  }

  const logFd = openSync(IV_SERVER_LOG, 'w');
  server = spawn(
    web,
    ['--host', '127.0.0.1', '--port', String(IV_PORT), '--data-dir', IV_INSTANCE_DIR, '--spa-dir', spaDir()],
    { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
  );
  server.unref();
  await waitForHealth();
  await dispatch({ type: 'unlock', passphrase: E2E_PASSPHRASE });
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
  rmSync(IV_INSTANCE_DIR, { recursive: true, force: true });
});

// ---------------------------------------------------------------------------
// The Workbench
// ---------------------------------------------------------------------------

test.describe('P4.D346 — the Workbench takes a hand', () => {
  test('(W1) the master tick raises a hand beside every eligible field, a click strikes it through, and the live JSON carries the block', async ({
    page,
  }) => {
    await newContrivance(page);
    const master = page.getByLabel('Let me take a hand in each roll');
    await expect(master).not.toBeChecked();
    await expect(page.getByRole('button', { name: 'Show the title in the hold dialog' })).toHaveCount(0);

    await master.check();
    await expect(page.getByText(/With this on, each run a character or a trigger phrase reaches is held/)).toBeVisible();
    const titleHand = page.getByRole('button', { name: 'Show the title in the hold dialog' });
    await expect(titleHand).toHaveAttribute('aria-pressed', 'true');
    for (const label of [
      'Show the chip label in the hold dialog',
      'Show the visibility in the hold dialog',
      'Show the announcer in the hold dialog',
      'Show the roll in the hold dialog',
      'Show the final value in the hold dialog',
      'Show the winning outcome in the hold dialog',
      'Show the rendered message in the hold dialog',
      'Show the outcome state in the hold dialog',
    ]) {
      await expect(page.getByRole('button', { name: label }), label).toHaveAttribute('aria-pressed', 'true');
    }

    await titleHand.click();
    await expect(titleHand).toHaveAttribute('aria-pressed', 'false');
    await expect(titleHand).toHaveClass(/qt-intervene-toggle--off/);
    await expect(titleHand).toHaveAttribute('title', 'Show the title in the hold dialog (hidden)');

    // The exact bytes Save would write.
    const preview = page.locator('pre').filter({ hasText: '"$schema"' }).first();
    await expect(preview).toContainText('"intervention": {');
    await expect(preview).toContainText('"enabled": true');
    await expect(preview).toContainText('"title": false');

    // Untick the master: the hands go, and the author's choice stays in the file.
    await master.uncheck();
    await expect(page.getByRole('button', { name: 'Show the title in the hold dialog' })).toHaveCount(0);
    await expect(preview).toContainText('"enabled": false');
    await expect(preview).toContainText('"title": false');
  });

  test("(W2) a saved contrivance with a hand shows the library's mark and reopens with its hands", async ({ page }) => {
    test.skip(!P4D345_SERVER_LANDED, P4D345_GATE_REASON);
    await newContrivance(page);
    await page.locator('#wb-title').fill('E2E Held Contrivance');
    await page.locator('#wb-name').fill('e2e_held_contrivance');
    await page.locator('#wb-description').fill('A held contrivance for the P4.D346 walk.');
    await page.getByRole('button', { name: 'add condition' }).first().click();
    await page.getByLabel('Operand number').first().fill('0.5');
    await page.getByLabel('Outcome message').first().fill('The hand is played.');
    await page.getByLabel('Let me take a hand in each roll').check();
    await page.getByRole('button', { name: 'Show the title in the hold dialog' }).click();

    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Where shall Pascal keep this contrivance?' })).toBeVisible({
      timeout: 15_000,
    });
    await page.getByRole('dialog').locator('.qt-badge').first().click();
    await page.getByRole('button', { name: 'Keep it here' }).click();
    await expect(page.getByText('Pascal has filed the contrivance.')).toBeVisible({ timeout: 15_000 });

    await page.getByRole('button', { name: 'Library' }).click();
    await expect(page.getByRole('button', { name: 'New contrivance', exact: true })).toBeVisible({ timeout: 15_000 });
    const row = page.getByRole('button', { name: /E2E Held Contrivance/ }).first();
    await expect(row).toBeVisible({ timeout: 15_000 });
    // The gate's premise: the library entry carries `intervention: true`.
    await expect(row.getByRole('img', { name: 'Holds each roll for you' })).toBeVisible({ timeout: 15_000 });

    await row.click();
    await expect(page.getByText('The contrivance itself')).toBeVisible({ timeout: 15_000 });
    await expect(page.getByLabel('Let me take a hand in each roll')).toBeChecked();
    await expect(page.getByRole('button', { name: 'Show the title in the hold dialog' })).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });
});

// ---------------------------------------------------------------------------
// The Automation card
// ---------------------------------------------------------------------------

test.describe('P4.D346 — the Automation row', () => {
  test('(S1) the intervention row is ticked by default, live with Custom Tools on, dead with it off', async ({ page }) => {
    await openSettings(page, 'automation');
    const row = automationRow(page, 'Hold intervenable rolls for me');
    await expect(row).toBeVisible();
    await expect(row).toContainText(
      'A custom tool marked for intervention pauses for your say-so before Pascal announces it.',
    );
    const box = row.locator('input[type="checkbox"]');
    await expect(box).toBeChecked();
    await expect(box).toBeEnabled();
    await expect(box).toHaveClass('qt-checkbox mt-1');
    await expect(row).toHaveClass(/qt-hover-accent/);

    await setCustomTools(page, false);
    try {
      await openSettings(page, 'automation');
      const dead = automationRow(page, 'Hold intervenable rolls for me');
      await expect(dead).toHaveClass(/opacity-50/);
      await expect(dead).toHaveClass(/cursor-not-allowed/);
      await expect(dead.locator('input[type="checkbox"]')).toBeDisabled();
      // The RNG row is untouched by Custom Tools.
      await expect(automationRow(page, 'Auto-Detect RNG Calls').locator('input[type="checkbox"]')).toBeEnabled();
    } finally {
      await setCustomTools(page, true);
    }
    await openSettings(page, 'automation');
    const live = automationRow(page, 'Hold intervenable rolls for me');
    await expect(live.locator('input[type="checkbox"]')).toBeEnabled();
    await expect(live).not.toHaveClass(/opacity-50/);
  });

  test('(S2) unticking the intervention row persists across a reload', async ({ page }) => {
    test.skip(!P4D341_P4D356_SETTINGS_LANDED, P4D341_P4D356_GATE_REASON);
    await openSettings(page, 'automation');
    const box = () => automationRow(page, 'Hold intervenable rolls for me').locator('input[type="checkbox"]');
    await expect(box()).toBeChecked();
    const saved = waitForSave(page, 'customToolIntervention');
    await box().uncheck();
    await saved;
    try {
      await page.reload();
      await expect(box()).not.toBeChecked({ timeout: 15_000 });
    } finally {
      // Restore only what this beat changed: when the reload already reads
      // ticked (the server dropped the key), there is nothing to undo.
      if (!(await box().isChecked())) {
        const restored = waitForSave(page, 'customToolIntervention');
        await box().check();
        await restored;
      }
    }
    await page.reload();
    await expect(box()).toBeChecked({ timeout: 15_000 });
  });
});

// ---------------------------------------------------------------------------
// The Salon's hold dialog
// ---------------------------------------------------------------------------

test.describe('P4.D346 — Pascal holds a roll in the Salon', () => {
  test.describe.configure({ mode: 'serial' });

  let generalMountId = '';
  let triggersBefore: boolean | undefined;

  /** Plant the intervenable Croupier in Quilltap General and switch trigger phrases on. */
  async function plantCroupier(): Promise<void> {
    const listed = (await dispatch({ type: 'mountPointList' })) as {
      mountPoints?: Array<{ id: string; name: string }>;
    };
    const general = (listed.mountPoints ?? []).find((m) => m.name === 'Quilltap General');
    expect(general, 'the boot should have provisioned Quilltap General').toBeTruthy();
    generalMountId = general!.id;
    await dispatch({
      type: 'mountFileWrite',
      mountPointId: generalMountId,
      path: CROUPIER_PATH,
      content: `${JSON.stringify(CROUPIER, null, 2)}\n`,
    });
    const settings = (await dispatch({ type: 'chatSettings' })) as Record<string, unknown>;
    triggersBefore = settings['customToolTriggers'] as boolean | undefined;
    await dispatch({ type: 'chatSettingsUpdate', settings: { customToolTriggers: true } });
  }

  async function unplantCroupier(): Promise<void> {
    if (generalMountId) {
      await dispatch({ type: 'mountFileDelete', mountPointId: generalMountId, path: CROUPIER_PATH }).catch(() => {});
    }
    await dispatch({ type: 'chatSettingsUpdate', settings: { customToolTriggers: triggersBefore ?? false } }).catch(
      () => {},
    );
  }

  async function sayThePhrase(page: Page): Promise<void> {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, CHAT_TITLE);
    await expect(page.locator('.qt-chat-stop-button')).toBeDisabled({ timeout: 30_000 });
    const editor = page.locator('.qt-chat-composer-input .qt-rich-editor-content');
    await editor.click();
    await page.keyboard.type('Let us cut the deck.');
    await page.keyboard.press('Enter');
  }

  const holdDialog = (page: Page) => page.getByTestId('custom-tool-intervention');

  test('(A) a trigger phrase on an intervenable tool opens Pascal\'s hold, and "As rolled" posts the bubble unchanged', async ({
    page,
  }) => {
    test.skip(!P4D345_SERVER_LANDED, P4D345_GATE_REASON);
    test.setTimeout(90_000);
    await plantCroupier();
    try {
      await sayThePhrase(page);
      // The gate's premise: the run is HELD rather than posted.
      await expect(holdDialog(page)).toBeVisible({ timeout: 30_000 });
      await expect(page.getByRole('heading', { name: 'Pascal holds: Croupier' })).toBeVisible();
      await expect(holdDialog(page)).toContainText('overheard: “cut the deck”.');
      const bubblesBefore = await page.locator('.qt-chat-system-bar', { hasText: 'Croupier' }).count();
      await page.getByRole('button', { name: 'As rolled', exact: true }).click();
      await expect(holdDialog(page)).toHaveCount(0, { timeout: 15_000 });
      await expect(page.locator('.qt-chat-system-bar', { hasText: 'Croupier' })).toHaveCount(bubblesBefore + 1, {
        timeout: 15_000,
      });
      await expect(page.getByText(/card: \d+\./).last()).toBeVisible();
      await expect(page.locator('.qt-chat-system-bar', { hasText: 'Croupier' }).last().locator('.qt-intervention-badge')).toHaveCount(0);
    } finally {
      await unplantCroupier();
    }
  });

  test('(B) "Deal it" with a forced outcome row posts the adjusted badge', async ({ page }) => {
    test.skip(!P4D345_SERVER_LANDED, P4D345_GATE_REASON);
    test.setTimeout(90_000);
    await plantCroupier();
    try {
      await sayThePhrase(page);
      await expect(holdDialog(page)).toBeVisible({ timeout: 30_000 });
      const outcome = holdDialog(page).locator('section', { hasText: 'The outcome' });
      await outcome.getByRole('button', { pressed: false }).first().click();
      await expect(holdDialog(page)).toContainText('The table would have dealt row', { timeout: 15_000 });
      await expect(holdDialog(page)).toContainText('— 1 change pinned');
      await page.getByRole('button', { name: 'Deal it', exact: true }).click();
      await expect(holdDialog(page)).toHaveCount(0, { timeout: 15_000 });
      const badge = page.locator('.qt-chat-system-bar', { hasText: 'Croupier' }).last().locator('.qt-intervention-badge');
      await expect(badge).toBeVisible({ timeout: 15_000 });
      await expect(badge).toHaveAttribute('title', /^Adjusted before it was dealt: outcome row /);
    } finally {
      await unplantCroupier();
    }
  });

  test('(C) Stop while held posts nothing and closes the dialog', async ({ page }) => {
    test.skip(!P4D345_STOP_VETO_LANDED, P4D345_STOP_VETO_GATE_REASON);
    test.setTimeout(90_000);
    await plantCroupier();
    try {
      await sayThePhrase(page);
      await expect(holdDialog(page)).toBeVisible({ timeout: 30_000 });
      const bubblesBefore = await page.locator('.qt-chat-system-bar', { hasText: 'Croupier' }).count();
      // The hold dialog is a modal whose full-screen overlay covers the
      // composer (v4 too: `BaseModal`'s `.qt-dialog-overlay` is `fixed inset-0
      // z-[60]`), so the composer's Stop cannot take a pointer while a roll is
      // held. Send exactly what that button sends — `chatStopTurn` on this chat
      // — and assert what the operator sees.
      const chatId = new URL(page.url()).pathname.split('/').pop()!;
      await dispatch({ type: 'chatStopTurn', chatId }); // throws on an error answer
      await expect(holdDialog(page)).toHaveCount(0, { timeout: 15_000 });
      await expect(page.locator('.qt-chat-stop-button')).toBeDisabled({ timeout: 15_000 });
      await expect(page.locator('.qt-chat-system-bar', { hasText: 'Croupier' })).toHaveCount(bubblesBefore);
    } finally {
      await unplantCroupier();
    }
  });

  test('(D) a reload with a hold pending reopens the dialog, and "Set the table aside" sweeps it', async ({ page }) => {
    test.skip(!P4D345_SERVER_LANDED, P4D345_GATE_REASON);
    test.setTimeout(90_000);
    await plantCroupier();
    try {
      await sayThePhrase(page);
      await expect(holdDialog(page)).toBeVisible({ timeout: 30_000 });
      const bubblesBefore = await page.locator('.qt-chat-system-bar', { hasText: 'Croupier' }).count();
      await page.reload();
      await maybeUnlockAnywhere(page);
      await expect(holdDialog(page)).toBeVisible({ timeout: 30_000 });
      await expect(page.getByRole('heading', { name: 'Pascal holds: Croupier' })).toBeVisible();

      await page.getByRole('button', { name: 'Set the table aside', exact: true }).click();
      await expect(holdDialog(page).locator('..').locator('..')).toContainText(
        'Sweep the cards away? Nothing is announced and nothing changes.',
      );
      await page.getByRole('button', { name: 'Keep the table', exact: true }).click();
      await expect(page.getByRole('button', { name: 'As rolled', exact: true })).toBeVisible();
      await page.getByRole('button', { name: 'Set the table aside', exact: true }).click();
      await page.getByRole('button', { name: 'Set it aside', exact: true }).click();
      await expect(holdDialog(page)).toHaveCount(0, { timeout: 15_000 });
      await expect(page.locator('.qt-chat-system-bar', { hasText: 'Croupier' })).toHaveCount(bubblesBefore);
    } finally {
      await unplantCroupier();
    }
  });
});

// ---------------------------------------------------------------------------
// Tier 2 — the spans and the hidden consult (v4 `8ae8a127c`, `b02d902f3`)
// ---------------------------------------------------------------------------

test.describe('P4.D346 — what the dialog says about the deal', () => {
  test.describe.configure({ mode: 'serial' });

  let mountId = '';
  let triggersBefore: boolean | undefined;

  async function plant(path: string, definition: Record<string, unknown>): Promise<void> {
    const listed = (await dispatch({ type: 'mountPointList' })) as { mountPoints?: Array<{ id: string; name: string }> };
    mountId = (listed.mountPoints ?? []).find((m) => m.name === 'Quilltap General')?.id ?? '';
    expect(mountId, 'the boot should have provisioned Quilltap General').toBeTruthy();
    await dispatch({ type: 'mountFileWrite', mountPointId: mountId, path, content: `${JSON.stringify(definition, null, 2)}\n` });
    const settings = (await dispatch({ type: 'chatSettings' })) as Record<string, unknown>;
    triggersBefore = settings['customToolTriggers'] as boolean | undefined;
    await dispatch({ type: 'chatSettingsUpdate', settings: { customToolTriggers: true } });
  }

  async function unplant(path: string): Promise<void> {
    if (mountId) await dispatch({ type: 'mountFileDelete', mountPointId: mountId, path }).catch(() => {});
    await dispatch({ type: 'chatSettingsUpdate', settings: { customToolTriggers: triggersBefore ?? false } }).catch(() => {});
  }

  async function say(page: Page, words: string): Promise<void> {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, CHAT_TITLE);
    await expect(page.locator('.qt-chat-stop-button')).toBeDisabled({ timeout: 30_000 });
    await page.locator('.qt-chat-composer-input .qt-rich-editor-content').click();
    await page.keyboard.type(words);
    await page.keyboard.press('Enter');
  }

  test("(E) the roll's and the value's bounds are spelled out under their controls", async ({ page }) => {
    test.skip(!P4D345_SERVER_LANDED, P4D345_GATE_REASON);
    test.setTimeout(90_000);
    const path = 'Tools/span_reader_e2e.tool.json';
    await plant(path, {
      $schema: '/schemas/qtap-custom-tool.schema.json',
      name: 'span_reader_e2e',
      title: 'Span Reader',
      description: 'Reads its own span for the e2e walk.',
      triggers: ['read the span'],
      intervention: { enabled: true },
      roll: { min: 1, max: 10, round: true, multiplier: 2 },
      outcomes: [{ when: true, message: 'Read {{value}}.', state: 'info' }],
    });
    try {
      await say(page, 'Please read the span.');
      const dialog = page.getByTestId('custom-tool-intervention');
      await expect(dialog).toBeVisible({ timeout: 30_000 });
      await expect(page.getByTestId('intervention-roll-span')).toHaveText(/^The table draws .+\.$/);
      // A string `toHaveText` normalizes the whitespace Angular keeps around the
      // text node (an anchored regex runs over the RAW text and missed it). The
      // roll is 1–10 rounded, ×2: the value spans 2 to 20.
      await expect(page.getByTestId('intervention-value-span')).toHaveText('After the modifiers, it comes to 2 to 20.');
      await page.getByRole('button', { name: 'Set the table aside', exact: true }).click();
      await page.getByRole('button', { name: 'Set it aside', exact: true }).click();
      await expect(dialog).toHaveCount(0, { timeout: 15_000 });
    } finally {
      await unplant(path);
    }
  });

  test('(F) a hidden consult still offers Re-ask from the compact oracle section', async ({ page }) => {
    test.skip(!P4D345_SERVER_LANDED, P4D345_GATE_REASON);
    test.setTimeout(90_000);
    const path = 'Tools/hidden_oracle_e2e.tool.json';
    await plant(path, {
      $schema: '/schemas/qtap-custom-tool.schema.json',
      name: 'hidden_oracle_e2e',
      title: 'Hidden Oracle',
      description: 'Consults an oracle the operator may not read.',
      triggers: ['ask the oracle'],
      intervention: { enabled: true, fields: { llmPrompt: false, llmAnswer: false } },
      roll: { min: 1, max: 10, round: true },
      llm: { prompt: 'The roll came up {{value}}. Say YES or NO.', errorMessage: 'The wire crackles.' },
      outcomes: [{ when: true, message: '{{llm}}', state: 'info' }],
    });
    try {
      await say(page, 'Let us ask the oracle.');
      const hidden = page.getByTestId('intervention-hidden-consult');
      await expect(hidden).toBeVisible({ timeout: 30_000 });
      await expect(hidden).toContainText('The oracle’s answer still fits the question.');
      // 💸 one mock completion.
      await hidden.getByRole('button', { name: 'Re-ask' }).click();
      await expect(hidden).toContainText('Asked 2 times.', { timeout: 15_000 });
      await page.getByRole('button', { name: 'Set the table aside', exact: true }).click();
      await page.getByRole('button', { name: 'Set it aside', exact: true }).click();
      await expect(page.getByTestId('custom-tool-intervention')).toHaveCount(0, { timeout: 15_000 });
    } finally {
      await unplant(path);
    }
  });
});

// ---------------------------------------------------------------------------
// helpers (copied — R-7)
// ---------------------------------------------------------------------------

/** One dispatch against this spec's own server; the `data` body on success. */
async function dispatch(body: Record<string, unknown>): Promise<unknown> {
  const res = await fetch(`${IV_BASE_URL}/api/dispatch`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  const json = (await res.json()) as { type?: string; data?: { message?: string } & Record<string, unknown> };
  if (json.type === 'error') throw new Error(`${String(body['type'])}: ${json.data?.message ?? 'error'}`);
  return json.data ?? {};
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

/** After a reload on a chat: unlock if asked, else the chat is already back. */
async function maybeUnlockAnywhere(page: Page): Promise<void> {
  await maybeUnlock(page);
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
}

async function openChat(page: Page, title: string): Promise<void> {
  await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible({ timeout: 15_000 });
  const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: title });
  await expect(card).toBeVisible();
  await card.click();
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible();
}

/** The Workbench, a fresh contrivance's editor open in form mode. */
async function newContrivance(page: Page): Promise<void> {
  await page.goto('/custom-tools');
  await maybeUnlock(page);
  await page.getByRole('button', { name: 'New contrivance', exact: true }).click();
  await expect(page.getByText('The contrivance itself')).toBeVisible({ timeout: 15_000 });
}

/** Settings → Chat with one card deep-linked open. */
async function openSettings(page: Page, section: string): Promise<void> {
  await page.goto(`/settings?tab=chat&section=${section}`);
  await maybeUnlock(page);
  await expect(page.locator(`#${section}`)).toBeVisible({ timeout: 15_000 });
}

/** One Automation row's `<label>`, by its title text. */
function automationRow(page: Page, label: string) {
  return page.locator('#automation label', { hasText: label });
}

/** Flip the Custom Tools card's toggle through the UI and wait for the real PUT. */
async function setCustomTools(page: Page, on: boolean): Promise<void> {
  await openSettings(page, 'custom-tools');
  const box = page.locator('#custom-tools input[type="checkbox"]').first();
  if ((await box.isChecked()) === on) return;
  const saved = waitForSave(page, 'customTools');
  if (on) await box.check();
  else await box.uncheck();
  await saved;
}

/** Wait for the settings PUT carrying `key` to come back from the real server. */
function waitForSave(page: Page, key: string) {
  return page.waitForResponse(
    (r) =>
      r.url().includes('/api/dispatch') &&
      r.request().method() === 'POST' &&
      (r.request().postData() ?? '').includes('chatSettingsUpdate') &&
      (r.request().postData() ?? '').includes(key),
  );
}

function runCliWrite(cli: string, sql: string): void {
  const res = spawnSync(cli, ['db', '--data-dir', IV_INSTANCE_DIR, '--write', sql], {
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
      const res = await fetch(`${IV_BASE_URL}/health`);
      // 423 = up, instance locked (the server gets no passphrase env).
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${IV_PORT}: ${lastErr}`);
}
