import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, openSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { expect, test, type Locator, type Page, type Response } from './support/fixtures';

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
 * P4.D300 — the memory SPA of v4's Phase G (`b045dd986` + `48790fb73`, pinned
 * at `83334b9a5`): ONE Maintenance card on Settings → Memory (the three
 * retired anchors opening it), the Memory Maintenance dialog from the card
 * (the picker variant) and from a character's memory list (the fixed
 * variant), the ledger, and the Rate limits section on the Extraction Grain
 * card.
 *
 * **Its own instance (P4.D276 R-I).** A private, locked copy of the committed
 * memories fixture (`crates/quilltap-web/tests/fixtures/memories-{main,mount}.db`)
 * on the spec-private port 4338 (§R.9), the
 * `settings-memory-consolidation-flow.spec.ts` recipe: copy the pair, key it
 * with the test pepper behind the e2e passphrase, rewrite `userId` to the
 * single-user id table by table with the CLI, spawn `quilltap-web --spa-dir`.
 *
 * **Two describes.**
 * - LIVE — green against `main`'s server: the anchors, the seven headings,
 *   the dialog's two variants and their disable rules (no run), and the Rate
 *   limits section (its verbs already answer from instance settings on main).
 *   Against a pre-round server the Maintenance card's two settings reads
 *   answer serde's `unknown variant` 400 and the card renders over its
 *   defaults with its read-error line — the beats read labels only.
 * - GATED behind `P4D288_SERVER_LANDED` (§S.9) — every beat that needs the
 *   round's server: P4.D288's wire (`memoryMaintain`, `memoryMaintenanceStatus`,
 *   `memoryMaintenanceConfigGet/Set`, the widened counts, `tfidfClusterThreshold`),
 *   P4.D297's maintain / status / job (its `memories` publish), P4.D298's
 *   create `action`. The unifier flips it on the union.
 *
 * **Never a model call (P4.D276 R-I).** The consolidation phase resolves a model
 * route only when its cluster selection is non-empty. Pip holds ZERO memories
 * (measured in `beforeAll` by a CLI read, asserted before the gated beats), so a
 * Pip dry run with all four phases selects nothing BY CONSTRUCTION — and the
 * beat asserts it (`consolidation.stats.clustersSelected === 0`, no
 * `skippedReason`). The Mnemo leg runs with Consolidate UNTICKED (fold / demote
 * / retire only — LLM-free). A Mnemo / Orla leg with Consolidate ticked is a 💸
 * dogfood row, never a beat.
 */

const P4D288_SERVER_LANDED = true;
const GATE_REASON =
  'awaits the 83334b9a5 round server (P4.D288 wire: memoryMaintain / memoryMaintenanceStatus / memoryMaintenanceConfigGet+Set + the widened counts + tfidfClusterThreshold; P4.D297 maintain / maintenance-status / the MEMORY_MAINTENANCE job and its memories publish; P4.D298 the create action) — §S.9 flips it on the union';

const PORT = 4338;
const BASE_URL = `http://127.0.0.1:${PORT}`;
const INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'memory-maintenance-instance');
const DATA_DIR = resolve(INSTANCE_DIR, 'data');
const SERVER_LOG = resolve(ARTIFACTS_DIR, 'memory-maintenance-server.log');
const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'memories-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'memories-mount.db');

// The builder's pinned ids (`harness/oracle/fixtures/build-memories-web-fixture.ts`).
const MNEMO = 'b1000000-0000-4000-8000-000000000001';
const PIP = 'b1000000-0000-4000-8000-000000000003';

/** `characters-flow.spec.ts:439-450` — every user-scoped table the memory vertical touches. */
const MEM_USER_TABLES = [
  'characters',
  'memories',
  'tags',
  'chats',
  'files',
  'chat_settings',
  'connection_profiles',
  'api_keys',
  'embedding_profiles',
  'tfidf_vocabularies',
];

let server: ChildProcess | undefined;
/** Pip's memory count, read by the CLI before the server boots. */
let pipMemoryCount: number | null = null;

function withoutPepper(): NodeJS.ProcessEnv {
  const env = { ...process.env };
  delete env['ENCRYPTION_MASTER_PEPPER'];
  return env;
}

function cliWrite(sql: string): { status: number | null; out: string } {
  const res = spawnSync(cliBinary(), ['db', '--data-dir', INSTANCE_DIR, '--write', sql], {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  return { status: res.status, out: `${res.stdout}${res.stderr}` };
}

/** One scalar out of the main DB (the `character-avatar-rolls-flow.spec.ts` idiom). */
function cliReadOne(sql: string): string | null {
  const res = spawnSync(cliBinary(), ['db', '--data-dir', INSTANCE_DIR, '--json', sql], {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  if (res.status !== 0) throw new Error(`CLI read failed (${sql}):\n${res.stdout}\n${res.stderr}`);
  const first = (JSON.parse(res.stdout) as Array<Record<string, unknown>>)[0];
  if (!first) return null;
  const value = Object.values(first)[0];
  return value == null ? null : String(value);
}

async function startServer(): Promise<void> {
  const logFd = openSync(SERVER_LOG, 'a');
  server = spawn(
    webBinary(),
    ['--host', '127.0.0.1', '--port', String(PORT), '--data-dir', INSTANCE_DIR, '--spa-dir', spaDir()],
    { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
  );
  server.unref();
  const deadline = Date.now() + 30_000;
  for (;;) {
    try {
      const res = await fetch(`${BASE_URL}/health`);
      if (res.status === 423 || res.status === 200) return;
    } catch {
      // not up yet
    }
    if (Date.now() > deadline) {
      throw new Error(`memory maintenance server not ready within 30s; see ${SERVER_LOG}`);
    }
    await new Promise((r) => setTimeout(r, 300));
  }
}

async function stopServer(): Promise<void> {
  const pid = server?.pid;
  server = undefined;
  if (!pid) return;
  try {
    process.kill(-pid, 'SIGTERM');
  } catch {
    try {
      process.kill(pid, 'SIGTERM');
    } catch {
      return; // already gone
    }
  }
  const deadline = Date.now() + 15_000;
  for (;;) {
    try {
      process.kill(pid, 0);
    } catch {
      return;
    }
    if (Date.now() > deadline) throw new Error(`server ${pid} did not exit within 15s`);
    await new Promise((r) => setTimeout(r, 200));
  }
}

async function unlock(page: Page, ready: Locator): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  await expect(passphrase.or(ready).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.isVisible()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
  await expect(ready).toBeVisible({ timeout: 10_000 });
}

/** The first dispatch RESPONSE for `type` — register BEFORE the action that sends it. */
function dispatchResponse(page: Page, type: string): Promise<Response> {
  return page.waitForResponse(
    (r) =>
      r.url().endsWith('/api/dispatch') &&
      r.request().method() === 'POST' &&
      (r.request().postDataJSON() as { type?: string } | null)?.type === type,
    { timeout: 20_000 },
  );
}

/** Count every dispatch of `type` the page sends from now on. */
function countDispatches(page: Page, type: string): { count: () => number } {
  let n = 0;
  page.on('request', (req) => {
    if (!req.url().endsWith('/api/dispatch') || req.method() !== 'POST') return;
    if ((req.postDataJSON() as { type?: string } | null)?.type === type) n += 1;
  });
  return { count: () => n };
}

const toast = (page: Page, text: string | RegExp) =>
  page.locator('[role="toast-container"]').getByText(text);

const maintenanceHeading = (page: Page) => page.getByRole('heading', { name: 'Maintenance', exact: true });

/** A switch on the card by its label (the `<label>` wraps the box). */
const cardSwitch = (card: Locator, label: string) =>
  card.locator('label', { hasText: label }).locator('input[type="checkbox"]');

async function openMemoriesTab(page: Page, characterId: string): Promise<void> {
  await page.goto(`${BASE_URL}/characters/${characterId}`);
  const memoriesTab = page.locator('nav.qt-tab-group').getByRole('button', { name: 'Memories' });
  await unlock(page, memoriesTab);
  await memoriesTab.click();
  await expect(page.getByRole('heading', { name: /Memories \(\d+\)/ })).toBeVisible({ timeout: 10_000 });
}

test.describe('P4.D300 — the Maintenance card, dialog and ledger of v4 Phase G', () => {
  test.skip(!existsSync(MAIN_FIXTURE), 'the committed memories fixture is absent');

  test.beforeAll(async () => {
    test.setTimeout(120_000);
    rmSync(INSTANCE_DIR, { recursive: true, force: true });
    rmSync(SERVER_LOG, { force: true });
    mkdirSync(DATA_DIR, { recursive: true });
    copyFileSync(MAIN_FIXTURE, resolve(DATA_DIR, 'quilltap.db'));
    if (existsSync(MOUNT_FIXTURE)) {
      copyFileSync(MOUNT_FIXTURE, resolve(DATA_DIR, 'quilltap-mount-index.db'));
    }
    writeFileSync(resolve(DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));
    for (const table of MEM_USER_TABLES) {
      const res = cliWrite(`UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`);
      if (res.status !== 0 && !res.out.includes('no such table') && !res.out.includes('no such column: userId')) {
        throw new Error(`memories fixture rewrite failed (${table}):\n${res.out}`);
      }
    }
    pipMemoryCount = Number(cliReadOne(`SELECT COUNT(*) AS n FROM memories WHERE characterId = '${PIP}'`));
    await startServer();
  });

  test.afterAll(async () => {
    await stopServer();
    rmSync(INSTANCE_DIR, { recursive: true, force: true });
  });

  // -------------------------------------------------------------------------
  // LIVE — green against `main`'s server.
  // -------------------------------------------------------------------------
  test.describe('LIVE — the card, its anchors, the dialog and the Rate limits', () => {

    test('section=memory-maintenance and each retired anchor force-open the Maintenance card', async ({ page }) => {
      test.setTimeout(90_000);
      for (const section of ['memory-maintenance', 'memory-housekeeping', 'memory-consolidation', 'memory-deduplication']) {
        await page.goto(`${BASE_URL}/settings?tab=memory&section=${section}`);
        await unlock(page, maintenanceHeading(page));
        const card = page.locator('#memory-maintenance');
        await expect(card.getByText('Enable automatic maintenance'), section).toBeVisible({ timeout: 10_000 });
        await expect(card.getByRole('button', { name: 'Run maintenance…' })).toBeVisible();
        // The retired anchors name no card any more.
        await expect(page.locator(`#${section}`)).toHaveCount(section === 'memory-maintenance' ? 1 : 0);
      }
    });

    test('the tab shows v4\'s seven headings', async ({ page }) => {
      test.setTimeout(60_000);
      await page.goto(`${BASE_URL}/settings?tab=memory`);
      await unlock(page, maintenanceHeading(page));
      await expect(page.locator('qt-collapsible-card h3.qt-card-title')).toHaveText([
        'Embedding Profiles',
        'Repair Missing Embeddings',
        'Maintenance',
        'Extraction Grain',
        'Recall Relevance',
        'Regenerate Memories',
        'Regenerate Conversation Summaries',
      ]);
    });

    test('Run maintenance… opens the picker; Every character with Consolidate ticked refuses a dry run; Close closes', async ({
      page,
    }) => {
      test.setTimeout(60_000);
      await page.goto(`${BASE_URL}/settings?tab=memory&section=memory-maintenance`);
      await unlock(page, maintenanceHeading(page));
      // The card already read the counts under the same key; the dialog's read
      // is TanStack's cached one (no fresh dispatch to wait on) — read the options.
      await page.locator('#memory-maintenance').getByRole('button', { name: 'Run maintenance…' }).click();

      const dialog = page.locator('qt-maintenance-dialog');
      await expect(dialog.getByRole('heading', { name: 'Memory Maintenance', exact: true })).toBeVisible();
      const picker = dialog.locator('#maintenance-target');
      await expect(picker.locator('option').nth(0)).toHaveText('Choose a character…');
      await expect(picker.locator('option').nth(1)).toHaveText('Every character');
      await expect(picker.locator(`option[value="${MNEMO}"]`)).toHaveText(/^Mnemo \([\d,]+ counted\)$/, { timeout: 10_000 });
      await expect(picker.locator(`option[value="${PIP}"]`)).toHaveText('Pip (0 counted)');
      await expect(dialog.getByRole('button', { name: 'Dry run' })).toBeDisabled();
      await expect(dialog.getByRole('button', { name: 'Run for real' })).toBeDisabled();

      await picker.selectOption('__all__');
      await expect(dialog.getByRole('button', { name: 'Dry run' })).toBeDisabled();
      await expect(dialog.getByRole('button', { name: 'Run for real' })).toBeEnabled();
      await expect(
        dialog.getByText('Consolidation calls a model, so it can be rehearsed for one character at a time.'),
      ).toBeVisible();

      await dialog.locator('.qt-dialog-footer').getByRole('button', { name: 'Close' }).click();
      await expect(dialog).toHaveCount(0);
    });

    test('the memory list\'s Maintenance opens the bare-titled fixed variant', async ({ page }) => {
      test.setTimeout(60_000);
      await openMemoriesTab(page, MNEMO);
      await expect(page.getByRole('button', { name: 'Cleanup', exact: true })).toHaveCount(0);
      const button = page.getByRole('button', { name: 'Maintenance', exact: true });
      await expect(button).toHaveAttribute('title', 'Fold repetitions, consolidate, and keep this character under the cap');
      await button.click();
      const dialog = page.locator('qt-maintenance-dialog');
      await expect(dialog.getByRole('heading', { name: 'Memory Maintenance', exact: true })).toBeVisible();
      await expect(dialog.locator('#maintenance-target')).toHaveCount(0);
      await expect(dialog.locator('fieldset input[type="checkbox"]')).toHaveCount(4);
      await expect(dialog.getByRole('button', { name: 'Dry run' })).toBeEnabled();
      await dialog.getByRole('button', { name: 'Close', exact: true }).first().click();
      await expect(dialog).toHaveCount(0);
    });

    test('the Rate limits section saves `enabled`, survives a reload, and refuses Notes per hour 0 locally', async ({
      page,
    }) => {
      test.setTimeout(90_000);
      const grain = page.getByRole('heading', { name: 'Extraction Grain', exact: true });
      await page.goto(`${BASE_URL}/settings?tab=memory&section=memory-extraction-grain`);
      await unlock(page, grain);
      const section = page.locator('qt-memory-extraction-limits-section');
      await expect(section.getByRole('heading', { name: 'Rate limits' })).toBeVisible({ timeout: 10_000 });
      const box = cardSwitch(section, 'Limit how quickly a character takes notes');
      await expect(box).not.toBeChecked();
      await expect(section.locator('#extraction-limits-maxPerHour')).toBeDisabled();

      const saved = dispatchResponse(page, 'memoryExtractionLimitsSet');
      await box.click();
      expect((await saved).status()).toBe(200);
      await expect(toast(page, 'Extraction limits saved')).toBeVisible({ timeout: 10_000 });
      await expect(box).toBeChecked();

      await page.reload();
      await unlock(page, grain);
      const reloaded = page.locator('qt-memory-extraction-limits-section');
      await expect(cardSwitch(reloaded, 'Limit how quickly a character takes notes')).toBeChecked({ timeout: 10_000 });
      const perHour = reloaded.locator('#extraction-limits-maxPerHour');
      await expect(perHour).toBeEnabled();

      // Refused inline, and NOTHING is posted (count, not hope).
      const sets = countDispatches(page, 'memoryExtractionLimitsSet');
      await perHour.fill('0');
      await perHour.blur();
      await expect(reloaded.getByText('Notes per hour must be a whole number between 1 and 10000')).toBeVisible();
      await page.waitForTimeout(500);
      expect(sets.count()).toBe(0);
      await expect(perHour).toHaveValue('20');

      // Put it back.
      const restored = dispatchResponse(page, 'memoryExtractionLimitsSet');
      await cardSwitch(reloaded, 'Limit how quickly a character takes notes').click();
      expect((await restored).status()).toBe(200);
      await expect(cardSwitch(reloaded, 'Limit how quickly a character takes notes')).not.toBeChecked();
    });
  });

  // -------------------------------------------------------------------------
  // GATED — the round's server (§S.9).
  // -------------------------------------------------------------------------
  test.describe('GATED — the round server', () => {
    test.describe.configure({ mode: 'serial' });
    test.skip(!P4D288_SERVER_LANDED, GATE_REASON);

    test.beforeAll(() => {
      // The never-a-model-call guarantee rests on this fixture fact.
      expect(pipMemoryCount).toBe(0);
    });

    test('the switches, the cap and an override save and survive a reload', async ({ page }) => {
      test.setTimeout(120_000);
      await page.goto(`${BASE_URL}/settings?tab=memory&section=memory-maintenance`);
      await unlock(page, maintenanceHeading(page));
      const card = page.locator('#memory-maintenance');
      const enable = cardSwitch(card, 'Enable automatic maintenance');
      await expect(enable).toBeEnabled({ timeout: 10_000 });
      await expect(enable).not.toBeChecked();
      await expect(cardSwitch(card, 'Demote over the cap')).toBeDisabled();
      await expect(cardSwitch(card, 'Consolidate')).toBeEnabled();

      const on = dispatchResponse(page, 'memoryMaintenanceConfigSet');
      await enable.click();
      expect((await on).status()).toBe(200);
      await expect(toast(page, 'Maintenance settings saved').first()).toBeVisible({ timeout: 10_000 });
      await expect(cardSwitch(card, 'Demote over the cap')).toBeEnabled();

      const demote = dispatchResponse(page, 'memoryMaintenanceConfigSet');
      await cardSwitch(card, 'Demote over the cap').click();
      const demoteReq = (await demote).request().postDataJSON() as { config?: unknown };
      expect(demoteReq.config).toEqual({ phases: { foldNearDuplicates: true, demote: false, retire: true } });

      const cap = dispatchResponse(page, 'memoryMaintenanceConfigSet');
      await card.locator('#perCharacterCap').fill('3000');
      await card.locator('#perCharacterCap').blur();
      expect((await cap).status()).toBe(200);

      await card.getByRole('button', { name: /Per-character overrides/ }).click();
      const override = dispatchResponse(page, 'memoryMaintenanceConfigSet');
      await card.getByLabel('Cap override for Mnemo').fill('500');
      await card.getByLabel('Cap override for Mnemo').blur();
      const overrideReq = (await override).request().postDataJSON() as { config?: unknown };
      expect(overrideReq.config).toEqual({ perCharacterCapOverrides: { [MNEMO]: 500 } });

      await page.reload();
      await unlock(page, maintenanceHeading(page));
      const again = page.locator('#memory-maintenance');
      await expect(cardSwitch(again, 'Enable automatic maintenance')).toBeChecked({ timeout: 10_000 });
      await expect(cardSwitch(again, 'Demote over the cap')).not.toBeChecked();
      await expect(cardSwitch(again, 'Fold near-duplicates')).toBeChecked();
      await expect(again.locator('#perCharacterCap')).toHaveValue('3000');
      await expect(again.getByRole('button', { name: /Per-character overrides/ })).toHaveText('▸ Per-character overrides (1 active)');
      await again.getByRole('button', { name: /Per-character overrides/ }).click();
      await expect(again.getByLabel('Cap override for Mnemo')).toHaveValue('500');
      await expect(again.getByText(/\([\d,]+ counted/).first()).toBeVisible();

      // Put the cap and the switches back so the later beats run on the defaults.
      for (const patch of [
        async () => {
          await again.getByLabel('Cap override for Mnemo').fill('');
          await again.getByLabel('Cap override for Mnemo').blur();
        },
        async () => {
          await again.locator('#perCharacterCap').fill('2000');
          await again.locator('#perCharacterCap').blur();
        },
        async () => cardSwitch(again, 'Demote over the cap').click(),
        async () => cardSwitch(again, 'Enable automatic maintenance').click(),
      ]) {
        const settled = dispatchResponse(page, 'memoryMaintenanceConfigSet');
        await patch();
        expect((await settled).status()).toBe(200);
      }
    });

    test('Cluster threshold (Built-in) saves and survives a reload', async ({ page }) => {
      test.setTimeout(90_000);
      await page.goto(`${BASE_URL}/settings?tab=memory&section=memory-maintenance`);
      await unlock(page, maintenanceHeading(page));
      const card = page.locator('#memory-maintenance');
      await card.getByRole('button', { name: /Consolidation particulars/ }).click();
      const field = card.locator('#consolidation-tfidfClusterThreshold');
      await expect(field).toHaveValue('0.3', { timeout: 10_000 });
      const saved = dispatchResponse(page, 'memoryConsolidationConfigSet');
      await field.fill('0.25');
      await field.blur();
      const req = (await saved).request().postDataJSON() as { config?: unknown };
      expect(req.config).toEqual({ tfidfClusterThreshold: 0.25 });
      await expect(toast(page, 'Consolidation settings saved').first()).toBeVisible({ timeout: 10_000 });
      await page.reload();
      await unlock(page, maintenanceHeading(page));
      await page.locator('#memory-maintenance').getByRole('button', { name: /Consolidation particulars/ }).click();
      await expect(page.locator('#consolidation-tfidfClusterThreshold')).toHaveValue('0.25', { timeout: 10_000 });
    });

    test('a Pip dry run with all four phases renders the report and calls no model', async ({ page }) => {
      test.setTimeout(90_000);
      await page.goto(`${BASE_URL}/settings?tab=memory&section=memory-maintenance`);
      await unlock(page, maintenanceHeading(page));
      await page.locator('#memory-maintenance').getByRole('button', { name: 'Run maintenance…' }).click();
      const dialog = page.locator('qt-maintenance-dialog');
      await expect(dialog.locator(`#maintenance-target option[value="${PIP}"]`)).toHaveCount(1, { timeout: 10_000 });
      await dialog.locator('#maintenance-target').selectOption(PIP);

      const dry = dispatchResponse(page, 'memoryMaintain');
      await dialog.getByRole('button', { name: 'Dry run' }).click();
      const res = await dry;
      expect(res.status()).toBe(200);
      expect(JSON.stringify((res.request().postDataJSON() as { options?: unknown }).options)).toBe(
        `{"characterId":"${PIP}","phases":["fold-near-duplicates","consolidate","demote","retire"],"dryRun":true}`,
      );
      const body = (await res.json()) as {
        data?: { report?: { consolidation?: { skippedReason?: string; stats?: { clustersSelected?: number } } } };
      };
      // R-I's tripwire: nothing selected, so no route was ever resolved.
      expect(body.data?.report?.consolidation?.skippedReason).toBeUndefined();
      expect(body.data?.report?.consolidation?.stats?.clustersSelected).toBe(0);
      const report = dialog.locator('qt-maintenance-report-view');
      await expect(report.getByText('Fold near-duplicates').first()).toBeVisible();
      await expect(report.getByText(/^rehearsed · \d+\.\ds$/).first()).toBeVisible();
      await expect(report.getByText('Clusters worth folding')).toBeVisible();
      await dialog.locator('.qt-dialog-footer').getByRole('button', { name: 'Close' }).click();
      await expect(dialog).toHaveCount(0);
    });

    test('a Mnemo fixed-variant dry run with Consolidate unticked: Within the cap, no retention window', async ({ page }) => {
      test.setTimeout(90_000);
      await openMemoriesTab(page, MNEMO);
      await page.getByRole('button', { name: 'Maintenance', exact: true }).click();
      const dialog = page.locator('qt-maintenance-dialog');
      await dialog.locator('fieldset label', { hasText: 'Consolidate' }).locator('input').uncheck();
      const dry = dispatchResponse(page, 'memoryMaintain');
      await dialog.getByRole('button', { name: 'Dry run' }).click();
      const res = await dry;
      expect(res.status()).toBe(200);
      expect(JSON.stringify((res.request().postDataJSON() as { options?: unknown }).options)).toBe(
        `{"characterId":"${MNEMO}","phases":["fold-near-duplicates","demote","retire"],"dryRun":true}`,
      );
      await expect(dialog.getByText(/^Within the cap\. Nothing need go to the archive\./)).toBeVisible({ timeout: 10_000 });
      await expect(dialog.getByText('skipped — no archive retention window is set')).toBeVisible();
      await dialog.locator('.qt-dialog-footer').getByRole('button', { name: 'Close' }).click();
    });

    test('a Pip real run (retire only) toasts, closes, and the ledger shows Pip (by hand) WITHOUT a reload', async ({
      page,
    }) => {
      test.setTimeout(90_000);
      await page.goto(`${BASE_URL}/settings?tab=memory&section=memory-maintenance`);
      await unlock(page, maintenanceHeading(page));
      const card = page.locator('#memory-maintenance');
      const ledger = card.locator('qt-memory-maintenance-ledger');
      await expect(ledger).toBeVisible({ timeout: 10_000 });

      await card.getByRole('button', { name: 'Run maintenance…' }).click();
      const dialog = page.locator('qt-maintenance-dialog');
      await expect(dialog.locator(`#maintenance-target option[value="${PIP}"]`)).toHaveCount(1, { timeout: 10_000 });
      await dialog.locator('#maintenance-target').selectOption(PIP);
      for (const label of ['Fold near-duplicates', 'Consolidate', 'Demote over the cap']) {
        await dialog.locator('fieldset label', { hasText: label }).locator('input').uncheck();
      }
      const real = dispatchResponse(page, 'memoryMaintain');
      await dialog.getByRole('button', { name: 'Run for real' }).click();
      const res = await real;
      expect(res.status()).toBe(200);
      expect(JSON.stringify((res.request().postDataJSON() as { options?: unknown }).options)).toBe(
        `{"characterId":"${PIP}","phases":["retire"],"dryRun":false}`,
      );
      await expect(toast(page, 'The staff have been sent round; the work proceeds below stairs.')).toBeVisible({
        timeout: 10_000,
      });
      await expect(dialog).toHaveCount(0);

      // R-AB's push: the job's completion publishes `memories`; the unscoped hint
      // invalidates every memories key, the ledger among them. No reload.
      const pipEntry = ledger.locator('.qt-card', { has: page.locator('p.qt-text-label', { hasText: /^Pip$/ }) });
      await expect(pipEntry).toBeVisible({ timeout: 20_000 });
      await expect(pipEntry.getByText(/^last run .+ \(by hand\)$/)).toBeVisible();
    });
    /**
     * Tier 2 item 22 — the editor's reinforced toast LIVE. Self-contained: it
     * files a fresh manual note on Mnemo, then files the SAME text again, so
     * admission (P4.D290 on the union) meets an exact repeat under the
     * fixture's default BUILTIN TF-IDF profile (identical text — cosine 1).
     * The second create's reply `action` is asserted first, so a union whose
     * admission does not absorb the repeat fails loudly with the action it
     * did take, never a silent toast mismatch. The note's words are NOT the
     * fixture's: the first live run (the `83334b9a5` unification) filed "the
     * lighthouse keeper whistles … at dusk" and admission REINFORCED Mnemo's
     * seeded "Alden the lighthouse keeper …" row instead of inserting (TF-IDF
     * cosine over the shared terms crossed the reinforce band — v4's decision
     * too), so the first create must share nothing with the seeded rows.
     */
    test('a manual note filed twice: the second create toasts "Already on file"', async ({ page }) => {
      test.setTimeout(90_000);
      await openMemoriesTab(page, MNEMO);
      const summary = 'A brass thimble in the oilskin pocket';
      const content = 'Mnemo keeps a brass thimble in the left pocket of the oilskin coat and counts it before sleeping.';

      const file = async (): Promise<string | undefined> => {
        await page.getByRole('button', { name: 'Add Memory', exact: true }).click();
        const dialog = page.getByRole('dialog');
        await expect(dialog).toBeVisible();
        await dialog.getByPlaceholder('Brief summary of this memory').fill(summary);
        await expect(dialog.getByPlaceholder('Brief summary of this memory')).toHaveValue(summary);
        await dialog.getByLabel('Memory content').fill(content);
        await expect(dialog.getByPlaceholder('Brief summary of this memory')).toHaveValue(summary);
        const created = dispatchResponse(page, 'memoryCreate');
        await dialog.getByRole('button', { name: 'Create Memory' }).click();
        const res = await created;
        expect(res.status()).toBe(200);
        // No `source` in the bag (v4 `c2795733f`).
        expect(Object.keys((res.request().postDataJSON() as { memory: object }).memory)).toEqual([
          'characterId',
          'content',
          'summary',
          'keywords',
          'importance',
        ]);
        await expect(dialog).toBeHidden({ timeout: 10_000 });
        return ((await res.json()) as { data?: { action?: string } }).data?.action;
      };

      expect(['insert', 'insert-linked']).toContain(await file());
      await expect(toast(page, 'Memory created')).toBeVisible({ timeout: 10_000 });

      const second = await file();
      expect(['reinforce', 'absorb', 'promote-and-reinforce'], `admission answered ${second}`).toContain(second);
      await expect(
        toast(page, 'Already on file — the existing memory has been reinforced instead'),
      ).toBeVisible({ timeout: 10_000 });
    });
  });
});
