import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, openSync, rmSync, writeFileSync } from 'node:fs';
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
 * P4.6l — browser walk of the Groups vertical (the Characters-page section +
 * the routed group editor). Authored pre-unification against lane A's committed
 * groups-projects fixture; the whole describe SKIPS when that fixture is absent
 * (this worktree) and auto-activates once lane A lands it at unification. The
 * unifier reconciles the exact fixture file names if lane A differs from the
 * `groups-projects-{main,mount}.db` guess below.
 *
 * Beats: unlock → the Characters page renders the Groups section with lane A's
 * fixture group cards → open a group's editor → rename + Save (`groupUpdate`) →
 * the Members card lists its members → the rename survives a full reload
 * (server state). Plus a create-group beat through the toolbar dialog.
 *
 * Recipe mirrors `characters-flow.spec.ts`: copy the fixture pair, write the
 * locked .dbkey, rewrite the fixture user id to SINGLE_USER_ID via
 * `quilltap db --write` BEFORE launch, then boot quilltap-web on a private port.
 */

const GROUPS_PORT = 4324;
const GROUPS_BASE_URL = `http://127.0.0.1:${GROUPS_PORT}`;
const GROUPS_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'groups-instance');
const GROUPS_DATA_DIR = resolve(GROUPS_INSTANCE_DIR, 'data');
const GROUPS_SERVER_LOG = resolve(ARTIFACTS_DIR, 'groups-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'groups-projects-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'groups-projects-mount.db');
/** Lane A owns the fixture; skip the live walk until it is committed. */
const FIXTURE_READY = existsSync(MAIN_FIXTURE);

/**
 * ACTIVATE-AT-UNIFY (P4.D104 Shared contract §5). The Group Instructions
 * round-trip needs P4.D103's server half: `groupUpdate` must ACCEPT the
 * `instructions` key and `groupGet` must project it back. Until then a save
 * would be silently dropped by the dispatch verb's unknown-field tolerance
 * ([[dispatch-verb-ignores-unknown-fields]]) and the reload would read
 * nothing — a red that says nothing about this lane. The unifier flips this
 * to `true`.
 */
const P4D103_SERVER_LANDED = false;

/** Every fixture table the groups walk reads is filtered by userId. */
const USER_TABLES = ['characters', 'chats', 'tags', 'groups', 'projects'];

let server: ChildProcess | undefined;

test.describe('P4.6l — Groups vertical (section → editor → rename → persist)', () => {
  test.skip(!FIXTURE_READY, 'awaits lane A groups-projects fixture (wired at unification)');

  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(GROUPS_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(GROUPS_DATA_DIR, { recursive: true });
    copyFileSync(MAIN_FIXTURE, resolve(GROUPS_DATA_DIR, 'quilltap.db'));
    if (existsSync(MOUNT_FIXTURE)) {
      copyFileSync(MOUNT_FIXTURE, resolve(GROUPS_DATA_DIR, 'quilltap-mount-index.db'));
    }

    writeFileSync(
      resolve(GROUPS_DATA_DIR, 'quilltap.dbkey'),
      makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE),
    );
    for (const table of USER_TABLES) {
      runCliWrite(
        cli,
        `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`,
      );
    }

    const logFd = openSync(GROUPS_SERVER_LOG, 'w');
    server = spawn(
      web,
      [
        '--host',
        '127.0.0.1',
        '--port',
        String(GROUPS_PORT),
        '--data-dir',
        GROUPS_INSTANCE_DIR,
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
    rmSync(GROUPS_INSTANCE_DIR, { recursive: true, force: true });
  });

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

  test('the Groups section renders, opens the editor, renames, and persists', async ({ page }) => {
    test.setTimeout(60_000);
    await page.goto(`${GROUPS_BASE_URL}/characters`);
    await unlockIfLocked(page);

    // The Groups section sits above the character grid.
    await expect(page.getByRole('heading', { name: 'Groups', exact: true })).toBeVisible();
    const groupCards = page.locator('qt-group-card');
    await expect(groupCards.first()).toBeVisible({ timeout: 10_000 });

    // Open the first group's editor via its Edit link.
    await groupCards.first().getByRole('link', { name: 'Edit' }).click();
    await expect(page).toHaveURL(/\/characters\/groups\/[^/]+$/);
    const nameInput = page.locator('#qt-group-name');
    await expect(nameInput).toBeVisible({ timeout: 10_000 });

    // Rename + Save (`groupUpdate`).
    await nameInput.fill('Renamed by the walk');
    await page.getByRole('button', { name: 'Save Changes' }).click();

    // The Members card is present (collapsed by default).
    await expect(page.getByRole('heading', { name: 'Members' })).toBeVisible();

    // The rename survives a reload back on the Characters page (server state).
    await page.goto(`${GROUPS_BASE_URL}/characters`);
    await page.reload();
    await expect(
      page.locator('qt-group-card').filter({ hasText: 'Renamed by the walk' }),
    ).toHaveCount(1, { timeout: 10_000 });
  });

  test('Group State: the button opens the editor and round-trips (§A)', async ({ page }) => {
    test.setTimeout(60_000);
    await page.goto(`${GROUPS_BASE_URL}/characters`);
    await unlockIfLocked(page);

    // ACTIVATE-AT-UNIFY: the §A `groupState*` verbs land in lane D10. Probe the
    // now-unlocked private server — in-lane the request deserialization fails
    // with an "unknown variant" error (no state arms); at unification it runs.
    const probe = await page.request.post(`${GROUPS_BASE_URL}/api/dispatch`, {
      data: { type: 'groupStateGet', groupId: '00000000-0000-0000-0000-000000000000' },
    });
    const pbody = (await probe.json().catch(() => null)) as
      | { type?: string; data?: { message?: string } }
      | null;
    const unknownVariant =
      pbody?.type === 'error' && /unknown variant/i.test(String(pbody?.data?.message ?? ''));
    test.skip(pbody == null || unknownVariant, 'awaits lane D10 state dispatch (activates at unification)');

    // Open the first group's editor.
    await expect(page.locator('qt-group-card').first()).toBeVisible({ timeout: 10_000 });
    await page.locator('qt-group-card').first().getByRole('link', { name: 'Edit' }).click();
    await expect(page).toHaveURL(/\/characters\/groups\/[^/]+$/);
    await expect(page.locator('#qt-group-name')).toBeVisible({ timeout: 10_000 });

    // The Group State button sits in the action row, right after Save Changes.
    await page.getByRole('button', { name: 'Group State' }).click();
    // The host element has no layout box (the dialog child is fixed-position);
    // assert the inner [role=dialog], per the established idiom.
    const modal = page.locator('qt-state-editor-modal').getByRole('dialog');
    await expect(modal).toBeVisible({ timeout: 15_000 });
    await expect(modal).toContainText('Group State');
    // The group tier edits its OWN state — never the chat cascade note.
    await expect(modal).not.toContainText('narrower tiers win');

    // Edit → save a key → survives a close/reopen round-trip.
    await modal.getByRole('button', { name: 'Edit' }).click();
    await modal.locator('textarea').fill('{\n  "_e2e_group": 42\n}');
    const saved = page.waitForResponse(
      (r) =>
        r.url().includes('/api/dispatch') &&
        r.request().method() === 'POST' &&
        (r.request().postData() ?? '').includes('groupStateSet'),
    );
    await modal.getByRole('button', { name: 'Save', exact: true }).click();
    await saved;
    // Two Close buttons share the accessible name (the chrome X is
    // aria-labelled); the footer button is the one with visible text.
    await modal.getByRole('button', { name: 'Close' }).filter({ hasText: 'Close' }).click();

    await page.getByRole('button', { name: 'Group State' }).click();
    await expect(page.locator('qt-state-editor-modal textarea')).toHaveValue(/_e2e_group/, {
      timeout: 15_000,
    });

    // Reset the tier so the fixture-backed instance is left clean.
    const reopened = page.locator('qt-state-editor-modal').getByRole('dialog');
    await reopened.getByRole('button', { name: 'Reset State' }).click();
    await reopened.getByRole('button', { name: 'Confirm Reset' }).click();
    await expect(reopened.locator('textarea')).toHaveValue('{}', { timeout: 15_000 });
  });

  /**
   * P4.D104 — Group Instructions round-trip (v4 `8f868109` + `a6870c5a`).
   *
   * Two halves in one beat, both against the real server:
   *  (a) type instructions → Save → reload → the editor holds the value, which
   *      only passes if `groupUpdate` PERSISTED it and `groupGet` PROJECTS it;
   *  (b) clear the editor → Save → the outgoing dispatch body carries
   *      `instructions: null`, not `""`. The server's update path is a
   *      validated passthrough, so the CLIENT is what normalizes (v4
   *      `GroupDetailView.tsx:93`) — asserted on the wire, then confirmed by
   *      a second reload showing an empty editor.
   *
   * Instructions are written through the UI, never seeded by SQL: groups are
   * store-overlay entities and a SQL UPDATE on that column is invisible
   * ([[store-overlay-properties-cannot-be-sql-seeded]]).
   */
  test('Group Instructions: type → save → reload → clear → save sends null', async ({ page }) => {
    test.skip(
      !P4D103_SERVER_LANDED,
      'the instructions wire (groupUpdate accept + groupGet projection) lands with P4.D103',
    );
    test.setTimeout(90_000);

    await page.goto(`${GROUPS_BASE_URL}/characters`);
    await unlockIfLocked(page);
    await expect(page.locator('qt-group-card').first()).toBeVisible({ timeout: 10_000 });
    await page.locator('qt-group-card').first().getByRole('link', { name: 'Edit' }).click();
    await expect(page).toHaveURL(/\/characters\/groups\/[^/]+$/);
    const editorUrl = page.url();
    await expect(page.locator('#qt-group-name')).toBeVisible({ timeout: 10_000 });

    // The header is the shared prompt-field label, drawn from the hints table.
    const header = page.locator('qt-prompt-field-label');
    await expect(header.locator('label')).toHaveText('Group Instructions (Optional)');
    await expect(header).toContainText('Standing instructions folded into the prompt of every');

    // --- (a) type → save → reload ---
    const body = page.locator('qt-markdown-field .qt-rich-editor-content');
    await expect(body).toBeVisible();
    await body.click();
    await page.keyboard.press('ControlOrMeta+a');
    const written = 'The regulars do not explain themselves to each other.';
    await page.keyboard.type(written);

    await Promise.all([
      page.waitForResponse(
        (r) =>
          r.url().includes('/api/dispatch') &&
          r.request().method() === 'POST' &&
          (r.request().postData() ?? '').includes('groupUpdate'),
      ),
      page.getByRole('button', { name: 'Save Changes' }).click(),
    ]);

    await page.goto(editorUrl);
    await page.reload();
    await expect(page.locator('qt-markdown-field .qt-rich-editor-content')).toContainText(written, {
      timeout: 15_000,
    });

    // --- (b) clear → save → the wire carries null ---
    const cleared = page.locator('qt-markdown-field .qt-rich-editor-content');
    await cleared.click();
    await page.keyboard.press('ControlOrMeta+a');
    await page.keyboard.press('Backspace');

    // waitForRESPONSE, not waitForRequest: the goto below aborts in-flight
    // fetches, so waiting only for the request races the server commit — the
    // twice-deflaked "triggered but never awaited" class (§3 unification
    // review). The request body is still readable off the response.
    const [response] = await Promise.all([
      page.waitForResponse(
        (r) =>
          r.url().includes('/api/dispatch') &&
          r.request().method() === 'POST' &&
          (r.request().postData() ?? '').includes('groupUpdate'),
      ),
      page.getByRole('button', { name: 'Save Changes' }).click(),
    ]);
    const sent = JSON.parse(response.request().postData() ?? '{}') as {
      group?: { instructions?: unknown };
    };
    expect(sent.group).toHaveProperty('instructions');
    expect(sent.group?.instructions).toBeNull();

    await page.goto(editorUrl);
    await page.reload();
    await expect(page.locator('qt-markdown-field .qt-rich-editor-content')).toHaveText('', {
      timeout: 15_000,
    });
  });

  test('create a throwaway group through the toolbar dialog', async ({ page }) => {
    test.setTimeout(60_000);
    await page.goto(`${GROUPS_BASE_URL}/characters`);
    await unlockIfLocked(page);

    await page.getByRole('button', { name: 'Create Group' }).click();
    const dialogName = page.locator('#qt-group-name');
    await expect(dialogName).toBeVisible();
    await dialogName.fill('Walk-created Guild');
    await page.getByRole('button', { name: 'Create', exact: true }).click();

    // On success the SPA navigates into the new group's editor.
    await expect(page).toHaveURL(/\/characters\/groups\/[^/]+$/, { timeout: 10_000 });
    await expect(page.locator('#qt-group-name')).toHaveValue('Walk-created Guild', {
      timeout: 10_000,
    });
  });
});

function runCliWrite(cli: string, sql: string): void {
  const res = spawnSync(cli, ['db', '--data-dir', GROUPS_INSTANCE_DIR, '--write', sql], {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  if (res.status !== 0) {
    // The fixture materializes only the tables its walks read; v4/v5 repos
    // auto-ensure collections on first access, so a table can be legitimately
    // absent (e.g. `tags`). Skip those instead of failing the setup.
    // …and the store-backed slim rows (groups/projects) carry no userId
    // column at all — they are not user-scoped. Skip both shapes.
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
      const res = await fetch(`${GROUPS_BASE_URL}/health`);
      if (res.status === 423 || res.status === 200) return;
      lastErr = `health status ${res.status}`;
    } catch (e) {
      lastErr = e instanceof Error ? e.message : String(e);
    }
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(
    `groups server did not become ready within 30s (${lastErr}); see ${GROUPS_SERVER_LOG}`,
  );
}
