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
 * P4.D307 — the Pascal SPA (v4 `362426c8f` trigger phrases + `f0db8cbb8`
 * wardrobe effects, pinned at `25b8fab6a`): the Workbench's trigger-phrase
 * field, the Side-effects card's Kind select and wardrobe fields, the Proving
 * Bench's "would attempt", the library's `✦ N phrase(s)` badge, and the
 * Automation card's trigger row.
 *
 * Its OWN instance on port 4343 (mock LLM 45341 — the round's §R.9 ports for
 * this lane), copied from the `scenario-browser-flow.spec.ts` recipe: beat (e)
 * toggles `customTools`, which `salon-custom-tools-flow.spec.ts` on the SHARED
 * server depends on, and beat (b) saves a definition into a store. Every
 * writing beat restores what it changed.
 *
 * **LIVE beats** ride verbs `main`'s server already has: (a) the trigger field
 * (client-side), (c) the Kind select + wardrobe fields (client-side), (e) the
 * Automation row dead while Custom Tools is off (the existing `customTools`
 * PUT arm; the gate is a client rule).
 *
 * **GATED beats** — ACTIVATE-AT-UNIFY (§S.9, the `P4D288_SERVER_LANDED`
 * precedent of `scenario-browser-flow.spec.ts`): the unifier flips each
 * constant to `true`. NAMED CONSTANTS, never capability probes — an unknown
 * definition or settings key reaches a DEFINED verb and is dropped or refused,
 * so a probe would lie.
 */
const P4D304_SERVER_LANDED = true;
const P4D305_SERVER_LANDED = true;
const P4D306_SERVER_LANDED = true;
const P4D304_GATE_REASON =
  'awaits P4.D304’s chatSettingsUpdate `customToolTriggers` arm (C1) — wired at unification';
const P4D305_GATE_REASON =
  'awaits P4.D305’s `triggers` on a saved definition and the library’s `triggerCount` (C2) — wired at unification';
const P4D306_GATE_REASON =
  'awaits P4.D306’s wardrobe effect on the customToolPreview definition and the bench’s third arm (C3) — wired at unification';

const WB_PORT = 4343;
const WB_BASE_URL = `http://127.0.0.1:${WB_PORT}`;
/** This spec's own mock LLM — clear of every shared port. */
const WB_MOCK_LLM_PORT = 45341;
const WB_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'workbench-triggers-instance');
const WB_DATA_DIR = resolve(WB_INSTANCE_DIR, 'data');
const WB_SERVER_LOG = resolve(ARTIFACTS_DIR, 'workbench-triggers-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'salon-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'salon-mount.db');

let server: ChildProcess | undefined;
let mock: MockLlm | undefined;

test.use({ baseURL: WB_BASE_URL });

test.describe('P4.D307 — trigger phrases and wardrobe effects in the Workbench', () => {
  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(WB_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(WB_DATA_DIR, { recursive: true });
    mkdirSync(resolve(WB_DATA_DIR, 'files'), { recursive: true });
    copyFileSync(MAIN_FIXTURE, resolve(WB_DATA_DIR, 'quilltap.db'));
    if (existsSync(MOUNT_FIXTURE)) {
      copyFileSync(MOUNT_FIXTURE, resolve(WB_DATA_DIR, 'quilltap-mount-index.db'));
    }
    writeFileSync(resolve(WB_DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));
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
      `UPDATE connection_profiles SET baseUrl = 'http://127.0.0.1:${WB_MOCK_LLM_PORT}/v1' WHERE provider = 'OPENAI_COMPATIBLE';`,
    );
    mock = await startMockLlm(MOCK_LLM_REPLY, WB_MOCK_LLM_PORT);

    // The salon pair carries NO `instance_settings` table; the boot's "Quilltap
    // General" provisioning records its mount there, and the Workbench's save
    // picker offers that store. Planted from the committed D23 dump, never
    // hand-written (the `scenario-browser-flow` precedent).
    const freshSchema = JSON.parse(
      readFileSync(
        resolve(REPO_ROOT, 'crates/quilltap-core/src/services/provisioning/fresh_schema.json'),
        'utf8',
      ),
    ) as { main: string[]; mountIndex: string[] };
    const settingsDdl = freshSchema.main.find((sql) => sql.startsWith('CREATE TABLE "instance_settings"'));
    if (!settingsDdl) throw new Error('fresh_schema.json carries no instance_settings CREATE TABLE');
    const ddl = spawnSync(cli, ['db', '--data-dir', WB_INSTANCE_DIR, '--write', settingsDdl], {
      env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
      encoding: 'utf8',
    });
    if (ddl.status !== 0 && !`${ddl.stdout}${ddl.stderr}`.includes('already exists')) {
      throw new Error(`instance_settings plant failed:\n${ddl.stdout}\n${ddl.stderr}`);
    }
    // The pair also predates the groups schema, and the save picker's
    // destinations verb reads `groups` (main) and `group_doc_mount_links`
    // (mount index) on every request — absent, the picker answers `no such
    // table: groups` and offers nothing (measured on this spec's first gated
    // run). Global setup plants both on the SHARED copy; this spec's own copy
    // needs them too. Empty tables are the honest state; the DDL is the dump's.
    for (const [partition, table] of [
      ['main', 'groups'],
      ['mountIndex', 'group_doc_mount_links'],
    ] as const) {
      const create = freshSchema[partition].find((sql) => sql.startsWith(`CREATE TABLE "${table}"`));
      if (!create) throw new Error(`fresh_schema.json carries no ${table} CREATE TABLE`);
      const args = ['db', '--data-dir', WB_INSTANCE_DIR, '--write', create.replace('CREATE TABLE', 'CREATE TABLE IF NOT EXISTS')];
      if (partition === 'mountIndex') args.splice(1, 0, '--mount-points');
      const res = spawnSync(cli, args, {
        env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
        encoding: 'utf8',
      });
      if (res.status !== 0) throw new Error(`${table} plant failed:\n${res.stdout}\n${res.stderr}`);
    }

    const logFd = openSync(WB_SERVER_LOG, 'w');
    server = spawn(
      web,
      ['--host', '127.0.0.1', '--port', String(WB_PORT), '--data-dir', WB_INSTANCE_DIR, '--spa-dir', spaDir()],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    server.unref();
    await waitForHealth();
    await fetch(`${WB_BASE_URL}/api/dispatch`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ type: 'unlock', passphrase: E2E_PASSPHRASE }),
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
    rmSync(WB_INSTANCE_DIR, { recursive: true, force: true });
  });

  // -------------------------------------------------------------------------
  // (a) LIVE — the trigger-phrase field
  // -------------------------------------------------------------------------
  test('(a) the trigger field adds a chip on Enter, refuses a short phrase, and caps at eight', async ({ page }) => {
    await newContrivance(page);
    const input = page.locator('#wb-trigger-phrase');
    const add = page.locator('qt-trigger-phrases-field').getByRole('button', { name: 'Add', exact: true });
    await expect(input).toHaveAttribute('placeholder', 'abracadabra');

    await input.fill('abracadabra');
    await input.press('Enter');
    const chips = page.getByRole('list', { name: 'Trigger phrases' }).locator('li');
    await expect(chips).toHaveCount(1);
    await expect(chips.first().locator('span')).toHaveText('abracadabra');
    await expect(page.getByRole('button', { name: 'Remove trigger phrase abracadabra' })).toBeVisible();
    await expect(input).toHaveValue('');

    await input.fill('ok');
    await expect(add).toBeDisabled();
    await input.fill('');

    for (let i = 1; i < 8; i += 1) {
      await input.fill(`phrase number ${i}`);
      await input.press('Enter');
    }
    await expect(chips).toHaveCount(8);
    await expect(input).toHaveAttribute('placeholder', 'At most 8 phrases');
    await expect(input).toBeDisabled();
    await expect(add).toBeDisabled();

    await page.getByRole('button', { name: 'Remove trigger phrase phrase number 7' }).click();
    await expect(chips).toHaveCount(7);
    await expect(input).toBeEnabled();
    await expect(input).toHaveAttribute('placeholder', 'abracadabra');
  });

  // -------------------------------------------------------------------------
  // (b) GATED P4D305 — the library badge after a save
  // -------------------------------------------------------------------------
  test('(b) a saved definition with one trigger phrase shows the ✦ 1 phrase badge', async ({ page }) => {
    test.skip(!P4D305_SERVER_LANDED, P4D305_GATE_REASON);
    await newContrivance(page);
    await fillValidIdentity(page, 'E2E Trigger Contrivance', 'e2e_trigger_contrivance');
    await page.locator('#wb-trigger-phrase').fill('abracadabra');
    await page.locator('#wb-trigger-phrase').press('Enter');

    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Where shall Pascal keep this contrivance?' })).toBeVisible({
      timeout: 15_000,
    });
    await page.getByRole('dialog').locator('.qt-badge').first().click();
    await page.getByRole('button', { name: 'Keep it here' }).click();
    await expect(page.getByText('Pascal has filed the contrivance.')).toBeVisible({ timeout: 15_000 });

    await page.getByRole('button', { name: 'Library' }).click();
    const badge = page.locator('.qt-badge', { hasText: '✦ 1 phrase' });
    await expect(badge).toBeVisible({ timeout: 15_000 });
    await expect(badge).toHaveAttribute(
      'title',
      'Listens for trigger phrases — said in a chat with the setting on, they run this tool',
    );
  });

  // -------------------------------------------------------------------------
  // (c) LIVE — the Kind select and the wardrobe fields
  // -------------------------------------------------------------------------
  test('(c) Kind "Change the outfit" swaps in the wardrobe fields and their rules', async ({ page }) => {
    await newContrivance(page);
    await page.getByRole('button', { name: /Add effect/ }).click();
    const kind = page.getByLabel('Effect kind');
    await expect(kind.locator('option')).toHaveText(['Write a value', 'Change the outfit']);
    await expect(page.getByLabel('Effect target')).toBeVisible();

    await kind.selectOption('wardrobe');
    await expect(page.getByLabel('Effect target')).toHaveCount(0);
    await expect(page.getByLabel('Wardrobe action')).toBeVisible();
    await expect(page.getByLabel('Wear mode')).toBeVisible();
    const item = page.getByLabel('Item 1');
    await expect(item).toHaveAttribute('placeholder', 'Cloak of Night');

    // A blank item list is the loader's own complaint, said beside the row.
    await item.fill('Cloak of Night');
    await expect(page.getByText('wear needs at least one item')).toHaveCount(0);
    await item.fill('');
    await expect(page.getByText('wear needs at least one item')).toBeVisible();

    await page.getByLabel('Wardrobe action').selectOption('clear_slot');
    const slot = page.getByLabel('Wardrobe slot');
    await expect(slot).toBeVisible();
    await expect(slot.locator('option').first()).toHaveText('choose a slot');
    await expect(page.getByLabel('Item 1')).toHaveCount(0);
    await expect(page.getByLabel('Wear mode')).toHaveCount(0);
    await expect(page.getByText('clear_slot needs a slot')).toBeVisible();

    // Back to a write: the write half survived the round trip.
    await kind.selectOption('write');
    await expect(page.getByLabel('Effect target')).toBeVisible();
  });

  // -------------------------------------------------------------------------
  // (d) GATED P4D306 — the bench's "would attempt"
  // -------------------------------------------------------------------------
  test('(d) the Proving Bench says a wardrobe effect "would attempt"', async ({ page }) => {
    test.skip(!P4D306_SERVER_LANDED, P4D306_GATE_REASON);
    await newContrivance(page);
    await fillValidIdentity(page, 'E2E Outfit Contrivance', 'e2e_outfit_contrivance');
    await page.getByRole('button', { name: /Add effect/ }).click();
    await page.getByLabel('Effect kind').selectOption('wardrobe');
    await page.getByLabel('Item 1').fill('Cloak of Night');

    await page.getByRole('button', { name: /Roll/ }).first().click();
    await expect(page.getByText('→ wardrobe: wear “Cloak of Night” (would attempt)')).toBeVisible({
      timeout: 15_000,
    });
  });

  // -------------------------------------------------------------------------
  // (e) LIVE — the Automation row, dead while Custom Tools is off
  // -------------------------------------------------------------------------
  test('(e) the trigger row is unchecked, live with Custom Tools on, dead with it off', async ({ page }) => {
    await openSettings(page, 'automation');
    const row = automationRow(page, 'Auto-Run Custom Tools on Trigger Phrases');
    await expect(row).toBeVisible();
    const box = row.locator('input[type="checkbox"]');
    await expect(box).not.toBeChecked();
    await expect(box).toBeEnabled();
    await expect(row).toHaveClass(/qt-hover-accent/);

    // Custom Tools OFF through the existing `customTools` PUT arm.
    await setCustomTools(page, false);
    try {
      await openSettings(page, 'automation');
      const dead = automationRow(page, 'Auto-Run Custom Tools on Trigger Phrases');
      await expect(dead).toHaveClass(/opacity-50/);
      await expect(dead).toHaveClass(/cursor-not-allowed/);
      await expect(dead.locator('input[type="checkbox"]')).toBeDisabled();
      // The RNG row is untouched by Custom Tools.
      await expect(automationRow(page, 'Auto-Detect RNG Calls').locator('input[type="checkbox"]')).toBeEnabled();
    } finally {
      await setCustomTools(page, true);
    }
    await openSettings(page, 'automation');
    const live = automationRow(page, 'Auto-Run Custom Tools on Trigger Phrases');
    await expect(live.locator('input[type="checkbox"]')).toBeEnabled();
    await expect(live).not.toHaveClass(/opacity-50/);
  });

  // -------------------------------------------------------------------------
  // (f) GATED P4D304 — the trigger row persists
  // -------------------------------------------------------------------------
  test('(f) ticking the trigger row persists across a reload', async ({ page }) => {
    test.skip(!P4D304_SERVER_LANDED, P4D304_GATE_REASON);
    await openSettings(page, 'automation');
    const box = () => automationRow(page, 'Auto-Run Custom Tools on Trigger Phrases').locator('input[type="checkbox"]');
    await expect(box()).not.toBeChecked();
    const saved = waitForSave(page, 'customToolTriggers');
    await box().check();
    await saved;
    try {
      await page.reload();
      await expect(box()).toBeChecked({ timeout: 15_000 });
    } finally {
      // Restore only what this beat changed: when the reload already reads
      // unticked (the server dropped the key), there is nothing to undo, and an
      // uncheck would wait forever on a save that never fires.
      if (await box().isChecked()) {
        const restored = waitForSave(page, 'customToolTriggers');
        await box().uncheck();
        await restored;
      }
    }
    await page.reload();
    await expect(box()).not.toBeChecked({ timeout: 15_000 });
  });
});

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

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

/** The Workbench, a fresh contrivance's editor open in form mode. */
async function newContrivance(page: Page): Promise<void> {
  await page.goto('/custom-tools');
  await maybeUnlock(page);
  await page.getByRole('button', { name: 'New contrivance', exact: true }).click();
  await expect(page.getByText('The contrivance itself')).toBeVisible({ timeout: 15_000 });
}

/** Enough identity + one tested outcome that the draft validates (the `workbench-flow` recipe). */
async function fillValidIdentity(page: Page, title: string, name: string): Promise<void> {
  await page.locator('#wb-title').fill(title);
  await page.locator('#wb-name').fill(name);
  await page.locator('#wb-description').fill('A contrivance authored by the P4.D307 walk.');
  await page.getByRole('button', { name: 'add condition' }).first().click();
  await page.getByLabel('Operand number').first().fill('0.5');
  await page.getByLabel('Outcome message').first().fill('The air shimmers.');
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
  const res = spawnSync(cli, ['db', '--data-dir', WB_INSTANCE_DIR, '--write', sql], {
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
      const res = await fetch(`${WB_BASE_URL}/health`);
      // 423 = up, instance locked (the server gets no passphrase env).
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${WB_PORT}: ${lastErr}`);
}
