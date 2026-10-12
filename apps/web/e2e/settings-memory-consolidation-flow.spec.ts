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
 * P4.D276 — the memory surfaces of v4 `d58548051` ("feat(memory):
 * consolidation and hot/cold tiers", pinned at `01a83539d`): the Settings →
 * Memory tab's Consolidation + Extraction Grain cards, the housekeeping
 * dialog's archive, the list's tier filter, and the digest viewer.
 *
 * **Its own instance (R-I).** A private, locked copy of the committed memories
 * fixture (`crates/quilltap-web/tests/fixtures/memories-{main,mount}.db`) on the
 * spec-private port 4335 (§R.9), the `characters-flow.spec.ts` P4.6t recipe:
 * copy the pair, key it with the test pepper behind the e2e passphrase, rewrite
 * `userId` to the single-user id table by table with the CLI, spawn
 * `quilltap-web --spa-dir`.
 *
 * **Two describes.**
 * - LIVE — the two new cards' `?section=` anchors and labels. SPA-only: against
 *   a pre-round server the cards fall back to their defaults (and show their
 *   read-error line), and the beats read labels only.
 * - GATED behind `P4D265_SERVER_LANDED` (§S.9) — every beat that needs the
 *   round's server (P4.D265's wire, P4.D266's housekeeping, P4.D269's `tier`,
 *   P4.D271's consolidation verbs). The unifier flips it on the union.
 *
 * **Never a model call (R-I).** v4 resolves a model route ONLY when a run's
 * cluster selection is non-empty (`lib/memory/consolidation.ts:837-840`). The
 * lane MEASURED the fixture (status-log P4.D276 record): Pip holds ZERO
 * memories and is on the counts roster, so a Pip dry run selects nothing and
 * calls no model BY CONSTRUCTION — and the beat asserts it
 * (`report.stats.clustersSelected === 0`, no `skippedReason`) so a regression
 * that resolves a route early fails loudly. Mnemo's ~45 templated, TF-IDF-
 * embedded notes would cluster and reach the fixture's `Local Mock` profile
 * (`http://127.0.0.1:1/v1`): every Mnemo / Orla consolidation leg is a 💸
 * dogfood row, never a beat.
 *
 * **The digest plant (R-K).** The committed fixture predates the tier columns;
 * P4.D265's boot ensure adds them to the COPY. The plant is therefore: boot →
 * `/health` → stop (SIGTERM the group, wait until it is gone — the CLI
 * write-lock refuses while a server holds the instance) → two `quilltap db
 * --write` statements (one statement per call) → reboot. It turns one Mnemo
 * note into a one-member digest and sends its pair cold, superseded by it.
 * This recipe can only run on the union (the columns do not exist before
 * P4.D265); it is exercised when §S.9 flips the gate.
 */

const P4D265_SERVER_LANDED = true;
const GATE_REASON =
  'awaits the P4.D265 round server (P4.D265 wire + P4.D266 housekeeping + P4.D269 tier + P4.D271 consolidation verbs) — §S.9 flips it on the union';

const PORT = 4335;
const BASE_URL = `http://127.0.0.1:${PORT}`;
const INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'memory-consolidation-instance');
const DATA_DIR = resolve(INSTANCE_DIR, 'data');
const SERVER_LOG = resolve(ARTIFACTS_DIR, 'memory-consolidation-server.log');
const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'memories-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'memories-mount.db');

// The builder's pinned ids (`harness/oracle/fixtures/build-memories-web-fixture.ts`).
const MNEMO = 'b1000000-0000-4000-8000-000000000001';
const PIP = 'b1000000-0000-4000-8000-000000000003';
const MEM_REL_A = 'b2000000-0000-4000-8000-0000000000e0'; // 'Smuggler cove east of the reef.'
const MEM_REL_B = 'b2000000-0000-4000-8000-0000000000e1'; // 'The reef hides a cove reachable at low tide.'

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
      throw new Error(`memory consolidation server not ready within 30s; see ${SERVER_LOG}`);
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
  // Confirm it is gone before any CLI write (the instance lock is released on exit).
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

test.describe('P4.D276 — the memory surfaces of v4 d58548051', () => {
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
    await startServer();
  });

  test.afterAll(async () => {
    await stopServer();
    rmSync(INSTANCE_DIR, { recursive: true, force: true });
  });

  // -------------------------------------------------------------------------
  // LIVE — SPA-only: the anchors and the labels.
  // -------------------------------------------------------------------------
  test.describe('LIVE — the new cards on their anchors', () => {
    test('section=memory-consolidation force-opens the Consolidation card', async ({ page }) => {
      test.setTimeout(60_000);
      await page.goto(`${BASE_URL}/settings?tab=memory&section=memory-consolidation`);
      await unlock(page, page.getByRole('heading', { name: 'Consolidation', exact: true }));
      const card = page.locator('#memory-consolidation');
      await expect(card.getByText('Enable automatic consolidation')).toBeVisible({ timeout: 10_000 });
      await expect(card.getByText('Consolidate now', { exact: true })).toBeVisible();
      await expect(card.getByRole('button', { name: 'Dry run' })).toBeVisible();
    });

    test('section=memory-extraction-grain force-opens the Extraction Grain card', async ({ page }) => {
      test.setTimeout(60_000);
      await page.goto(`${BASE_URL}/settings?tab=memory&section=memory-extraction-grain`);
      await unlock(page, page.getByRole('heading', { name: 'Extraction Grain', exact: true }));
      const card = page.locator('#memory-extraction-grain');
      await expect(card.getByText('Observations of others', { exact: true })).toBeVisible({ timeout: 10_000 });
      const options = card.locator('#extraction-other-pass option');
      await expect(options).toHaveText(['Hybrid (recommended)', 'At the fold only', 'Every turn']);
    });
  });

  // -------------------------------------------------------------------------
  // GATED — the round's server (§S.9).
  // -------------------------------------------------------------------------
  test.describe('GATED — the round server', () => {
    test.describe.configure({ mode: 'serial' });
    test.skip(!P4D265_SERVER_LANDED, GATE_REASON);

    test('Extraction Grain round-trips a mode through the server', async ({ page }) => {
      test.setTimeout(60_000);
      await page.goto(`${BASE_URL}/settings?tab=memory&section=memory-extraction-grain`);
      await unlock(page, page.getByRole('heading', { name: 'Extraction Grain', exact: true }));
      const select = page.locator('#extraction-other-pass');
      await expect(select).toBeEnabled({ timeout: 10_000 });

      const saved = dispatchResponse(page, 'memoryExtractionModeConfigSet');
      await select.selectOption('fold');
      expect((await saved).status()).toBe(200);
      // The DOM AFTER the save settles (the P4.D252 lesson).
      await expect(toast(page, 'Extraction settings saved')).toBeVisible({ timeout: 10_000 });
      await expect(
        page.getByText('Observations of others are gathered solely when a conversation is folded into its summary. The thriftiest course.'),
      ).toBeVisible();

      await page.reload();
      await unlock(page, page.getByRole('heading', { name: 'Extraction Grain', exact: true }));
      await expect(page.locator('#extraction-other-pass')).toHaveValue('fold', { timeout: 10_000 });

      const restored = dispatchResponse(page, 'memoryExtractionModeConfigSet');
      await page.locator('#extraction-other-pass').selectOption('hybrid');
      expect((await restored).status()).toBe(200);
      await expect(page.locator('#extraction-other-pass')).toHaveValue('hybrid');
    });

    test('Consolidation saves a field, refuses a bad one locally, and toggles the switch', async ({ page }) => {
      test.setTimeout(90_000);
      await page.goto(`${BASE_URL}/settings?tab=memory&section=memory-consolidation`);
      await unlock(page, page.getByRole('heading', { name: 'Consolidation', exact: true }));
      const watermark = page.locator('#consolidation-watermark');
      await expect(watermark).toBeEnabled({ timeout: 10_000 });

      const saved = dispatchResponse(page, 'memoryConsolidationConfigSet');
      await watermark.fill('200');
      await watermark.blur();
      expect((await saved).status()).toBe(200);
      await expect(toast(page, 'Consolidation settings saved')).toBeVisible({ timeout: 10_000 });
      await page.reload();
      await unlock(page, page.getByRole('heading', { name: 'Consolidation', exact: true }));
      await expect(page.locator('#consolidation-watermark')).toHaveValue('200', { timeout: 10_000 });

      // A bad threshold is refused inline and NOTHING is posted (count, not hope —
      // the P4.D253 "passed with nothing posted" shape).
      const sets = countDispatches(page, 'memoryConsolidationConfigSet');
      const threshold = page.locator('#consolidation-clusterThreshold');
      await threshold.fill('2');
      await threshold.blur();
      await expect(
        page.locator('#memory-consolidation').getByText('Cluster threshold must be a number between 0 and 1'),
      ).toBeVisible();
      await page.waitForTimeout(500);
      expect(sets.count()).toBe(0);

      // The master switch round-trips. No field is asserted after it: v4's
      // server-side `.partial()` default-fill resets them (R-F; P4.D271 owns that).
      const box = page.locator('#memory-consolidation input[type="checkbox"]');
      const toggled = dispatchResponse(page, 'memoryConsolidationConfigSet');
      await box.click();
      expect((await toggled).status()).toBe(200);
      await expect(box).toBeChecked();
      await page.reload();
      await unlock(page, page.getByRole('heading', { name: 'Consolidation', exact: true }));
      const reloaded = page.locator('#memory-consolidation input[type="checkbox"]');
      await expect(reloaded).toBeChecked({ timeout: 10_000 });
      const untoggled = dispatchResponse(page, 'memoryConsolidationConfigSet');
      await reloaded.click();
      expect((await untoggled).status()).toBe(200);
      await expect(reloaded).not.toBeChecked();
    });

    test('a dry run on the zero-memory character calls no model and reports nothing to fold; a real run queues', async ({
      page,
    }) => {
      test.setTimeout(90_000);
      await page.goto(`${BASE_URL}/settings?tab=memory&section=memory-consolidation`);
      await unlock(page, page.getByRole('heading', { name: 'Consolidation', exact: true }));
      const roster = page.getByLabel('Character to consolidate');
      await expect(roster.locator(`option[value="${PIP}"]`)).toHaveText('Pip (0 memories)', { timeout: 10_000 });
      await roster.selectOption(PIP);

      const dry = dispatchResponse(page, 'memoryConsolidate');
      await page.getByRole('button', { name: 'Dry run' }).click();
      const dryRes = await dry;
      expect(dryRes.status()).toBe(200);
      const body = (await dryRes.json()) as {
        data?: { report?: { stats?: { clustersSelected?: number }; skippedReason?: string } };
      };
      // R-I's tripwire: nothing selected, so no route was ever resolved.
      expect(body.data?.report?.stats?.clustersSelected).toBe(0);
      expect(body.data?.report?.skippedReason).toBeUndefined();

      const dialog = page.locator('qt-memory-consolidation-report-dialog');
      await expect(dialog.getByRole('heading', { name: 'Consolidation, Rehearsed — Pip' })).toBeVisible();
      await expect(dialog.getByText('Not a single cluster worth folding. The shelves are in good order.')).toBeVisible();
      await expect(dialog.getByText(/\d+\.\ds\.$/)).toBeVisible();
      await dialog.locator('.qt-dialog-footer').getByRole('button', { name: 'Close' }).click();
      await expect(dialog).toHaveCount(0);

      const real = dispatchResponse(page, 'memoryConsolidate');
      await page.getByRole('button', { name: 'Consolidate for real' }).click();
      expect((await real).status()).toBe(200);
      await expect(
        toast(page, 'The consolidation has been sent below stairs — it will run in the background'),
      ).toBeVisible({ timeout: 10_000 });
    });

    test('Cleanup archives (never deletes) and the Archived tier lists what it moved', async ({ page }) => {
      test.setTimeout(90_000);
      await page.goto(`${BASE_URL}/characters/${MNEMO}`);
      const memoriesTab = page.locator('nav.qt-tab-group').getByRole('button', { name: 'Memories' });
      await unlock(page, memoriesTab);
      await memoriesTab.click();
      await expect(page.getByRole('heading', { name: /Memories \(\d+\)/ })).toBeVisible({ timeout: 10_000 });

      await page.getByRole('button', { name: 'Cleanup', exact: true }).click();
      const dialog = page.getByRole('dialog').filter({ hasText: 'Memory Cleanup' });
      await expect(dialog.getByText('Archive', { exact: true })).toBeVisible({ timeout: 10_000 });
      const run = dialog.getByRole('button', { name: /^Archive \d+ Memories$/ });
      await expect(run).toBeEnabled({ timeout: 10_000 });
      const label = (await run.textContent())?.trim() ?? '';
      const n = Number(/^Archive (\d+) Memories$/.exec(label)?.[1]);
      // The lane measured ≥ 1 candidate (status-log P4.D276, measurement 30): a
      // preview reading 0 is a regression, not a pass.
      expect(n).toBeGreaterThanOrEqual(1);

      const ran = dispatchResponse(page, 'memoryHousekeep');
      await run.click();
      const ranRes = await ran;
      expect(ranRes.status()).toBe(200);
      const result = ((await ranRes.json()) as { data?: { result?: { demoted?: number } } }).data?.result;
      const demoted = result?.demoted ?? 0;
      expect(demoted).toBeGreaterThanOrEqual(1);
      await expect(
        toast(
          page,
          demoted === 1
            ? 'One memory has been shown to the archive, where it will keep.'
            : `${demoted} memories have been shown to the archive, where they will keep.`,
        ),
      ).toBeVisible({ timeout: 10_000 });

      const tier = page.getByLabel('Filter by tier');
      await tier.selectOption('cold');
      const cards = page.locator('qt-memory-card');
      await expect(cards.first()).toBeVisible({ timeout: 10_000 });
      // Count AFTER the filtered list has settled — never a first-matching-poll
      // toHaveCount (e2e-tohavecount-resolves-on-first-matching-poll).
      await expect(cards.first().locator('span', { hasText: /^Archived$/ })).toBeVisible();
      await page.waitForTimeout(500);
      const archivedCount = await cards.count();
      expect(archivedCount).toBeGreaterThanOrEqual(demoted);
      for (let i = 0; i < archivedCount; i++) {
        await expect(cards.nth(i).locator('span.rounded-full', { hasText: /^Archived$/ })).toBeVisible();
      }

      await tier.selectOption('hot');
      await page.waitForTimeout(500);
      await expect(page.locator('qt-memory-card span.rounded-full', { hasText: /^Archived$/ })).toHaveCount(0);
    });

    test('a superseded card opens The Digest over the page, action-less', async ({ page }) => {
      test.setTimeout(120_000);
      // R-K: stop → plant (two statements, one per call) → reboot.
      await stopServer();
      const digest = cliWrite(
        `UPDATE memories SET source = 'CONSOLIDATED', tier = 'hot', consolidatedFrom = '["${MEM_REL_B}"]' WHERE id = '${MEM_REL_A}';`,
      );
      expect(digest.status, digest.out).toBe(0);
      const cold = cliWrite(
        `UPDATE memories SET tier = 'cold', supersededById = '${MEM_REL_A}' WHERE id = '${MEM_REL_B}';`,
      );
      expect(cold.status, cold.out).toBe(0);
      await startServer();

      await page.goto(`${BASE_URL}/characters/${MNEMO}`);
      const memoriesTab = page.locator('nav.qt-tab-group').getByRole('button', { name: 'Memories' });
      await unlock(page, memoriesTab);
      await memoriesTab.click();
      await page.getByLabel('Filter by tier').selectOption('cold');
      const shelved = page.locator('qt-memory-card', { hasText: 'The reef hides a cove reachable at low tide.' });
      await expect(shelved).toBeVisible({ timeout: 10_000 });
      await expect(shelved.locator('span.rounded-full', { hasText: /^Archived$/ })).toBeVisible();

      const fetched = dispatchResponse(page, 'memoryGet');
      await shelved.getByRole('button', { name: 'Superseded by a digest' }).click();
      expect((await fetched).status()).toBe(200);
      const viewer = page.locator('qt-memory-digest-viewer');
      await expect(viewer.getByRole('heading', { name: 'The Digest' })).toBeVisible({ timeout: 10_000 });
      const card = viewer.locator('qt-memory-card');
      await expect(card.locator('span.rounded-full', { hasText: /^Digest$/ })).toBeVisible();
      await expect(card.getByText('Digested from 1 note.')).toBeVisible();
      await expect(card.getByRole('button', { name: 'Edit', exact: true })).toHaveCount(0);
      await expect(card.getByRole('button', { name: 'Delete', exact: true })).toHaveCount(0);
      await viewer.getByRole('button', { name: 'Close', exact: true }).click();
      await expect(viewer).toHaveCount(0);
    });
  });
});
