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
 * consolidation and hot/cold tiers", pinned at `01a83539d`) that SURVIVE v4's
 * Phase G (`b045dd986`, P4.D300 L-10): the Extraction Grain card on its anchor
 * and its round trip, the list's tier filter, and the digest viewer. The
 * Consolidation card, its report dialog and the housekeeping dialog were
 * deleted with Phase G: the `section=memory-consolidation` anchor beat MOVED to
 * `memory-maintenance-flow.spec.ts` (re-pointed at the Maintenance card), and
 * the Consolidation save / dry-run / `Consolidate for real` and Cleanup-archives
 * beats were deleted with the UI. The Cleanup beat has no like-for-like
 * successor: the cap floor is 100 and the fixture's characters hold fewer
 * notes (Mnemo ~45), so no UI run can demote on the fixture — RECORDED; the
 * Archived tier stays covered by the digest-viewer beat's planted cold row.
 *
 * **Its own instance (R-I).** A private, locked copy of the committed memories
 * fixture (`crates/quilltap-web/tests/fixtures/memories-{main,mount}.db`) on the
 * spec-private port 4335 (§R.9), the `characters-flow.spec.ts` P4.6t recipe:
 * copy the pair, key it with the test pepper behind the e2e passphrase, rewrite
 * `userId` to the single-user id table by table with the CLI, spawn
 * `quilltap-web --spa-dir`.
 *
 * **Two describes.**
 * - LIVE — the Extraction Grain card's `?section=` anchor and labels.
 * - GATED behind `P4D265_SERVER_LANDED` (§S.9; flipped on the `01a83539d`
 *   union) — the grain round trip and the digest viewer.
 *
 * **The digest plant (R-K).** The committed fixture predates the tier columns;
 * P4.D265's boot ensure adds them to the COPY. The plant is therefore: boot →
 * `/health` → stop (SIGTERM the group, wait until it is gone — the CLI
 * write-lock refuses while a server holds the instance) → two `quilltap db
 * --write` statements (one statement per call) → reboot. It turns one Mnemo
 * note into a one-member digest and sends its pair cold, superseded by it.
 */

const P4D265_SERVER_LANDED = true;
const GATE_REASON =
  'awaits the P4.D265 round server (P4.D265 wire + P4.D269 tier + the consolidation verbs — since the 83334b9a5 round ONE maintenance job, P4.D297) — §S.9 flipped it on the union';

const PORT = 4335;
const BASE_URL = `http://127.0.0.1:${PORT}`;
const INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'memory-consolidation-instance');
const DATA_DIR = resolve(INSTANCE_DIR, 'data');
const SERVER_LOG = resolve(ARTIFACTS_DIR, 'memory-consolidation-server.log');
const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'memories-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'memories-mount.db');

// The builder's pinned ids (`harness/oracle/fixtures/build-memories-web-fixture.ts`).
const MNEMO = 'b1000000-0000-4000-8000-000000000001';
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
  test.describe('LIVE — the Extraction Grain card on its anchor', () => {
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
