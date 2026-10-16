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
 * P4.D301 — v4 bug 241 (`93c5af6ad`), live: a send that fails before the
 * server saves it hands the operator's words back.
 *
 * Beat 1 (nothing saved): the single-character "Solo Voyage" seat answers
 * through a requires-key profile with NO key, so the orchestrator refuses
 * BEFORE any write (`No API key configured for this connection profile`). The
 * rejection carries `submission: {false, false}`; the Salon reads the chat,
 * puts the text and the attached file back into the empty composer, writes the
 * draft at once, and raises exactly ONE error toast — v4's bytes, the note
 * appended. A reload finds the draft; the CLI finds no new USER row.
 *
 * Beat 2 (saved, then failed): the seat moves to a KEYED profile at a dead
 * endpoint (`http://127.0.0.1:9/v1`, the 2026-09-06 A1 walk's idiom), so the
 * user row lands before the model call dies: ONE toast WITHOUT the note, and
 * the composer stays empty — nothing restored over a saved row.
 *
 * Own server on 4339 (§R.9) over a COPY of `salon-main.db`, the
 * `salon-danger-avatar-flow.spec.ts` recipe; the profile edits are pre-spawn
 * CLI writes on that copy. GATED behind {@link P4D288_SERVER_LANDED} (§S.9 —
 * the unifier flips it on the union, where the fixture pair is widened).
 */
const P4D288_SERVER_LANDED = true;

const PORT = 4339;
const BASE = `http://127.0.0.1:${PORT}`;
const INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'send-failure-restore-instance');
const DATA_DIR = resolve(INSTANCE_DIR, 'data');
const SERVER_LOG = resolve(ARTIFACTS_DIR, 'send-failure-restore-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'salon-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'salon-mount.db');
const USER_TABLES = ['characters', 'chats', 'tags', 'groups', 'projects', 'files'];

/** The fixture's two profiles: Solo Voyage's seat uses the first. */
const NO_KEY_PROFILE = 'c0000001-0000-4000-8000-000000000001';
const DEAD_ENDPOINT_PROFILE = 'c0000001-0000-4000-8000-000000000002';
const SOLO_VOYAGE = 'c1000000-0000-4000-8000-000000000001';

const TEXT = 'Shall we take the air, Friday? The terrace is lovely at this hour.';
const FILE_NAME = 'p4d301-itinerary.txt';
const NO_KEY_TOAST =
  'Failed to generate response: No API key configured for this connection profile. Your message is back in the composer.';
const DRAFT_KEY = `quilltap-draft-${SOLO_VOYAGE}`;

let server: ChildProcess | undefined;

test.describe('P4.D301 — a failed send hands the submission back (bug 241)', () => {
  test.beforeAll(async () => {
    test.skip(!P4D288_SERVER_LANDED, 'P4.D301 — flips LIVE at the round unification (§S.9)');
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(DATA_DIR, { recursive: true });
    mkdirSync(resolve(DATA_DIR, 'files'), { recursive: true });
    copyFileSync(MAIN_FIXTURE, resolve(DATA_DIR, 'quilltap.db'));
    if (existsSync(MOUNT_FIXTURE)) {
      copyFileSync(MOUNT_FIXTURE, resolve(DATA_DIR, 'quilltap-mount-index.db'));
    }
    writeFileSync(resolve(DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));
    for (const table of USER_TABLES) {
      runCliWrite(
        cli,
        `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`,
      );
    }
    // Beat 1's profile: a provider that REQUIRES a key, with none attached.
    runCliWrite(
      cli,
      `UPDATE connection_profiles SET provider = 'OPENAI', apiKeyId = NULL WHERE id = '${NO_KEY_PROFILE}';`,
    );
    // Beat 2's profile: keyed, at an endpoint nothing listens on.
    runCliWrite(
      cli,
      `UPDATE connection_profiles SET baseUrl = 'http://127.0.0.1:9/v1' WHERE id = '${DEAD_ENDPOINT_PROFILE}';`,
    );

    const logFd = openSync(SERVER_LOG, 'w');
    server = spawn(
      web,
      ['--host', '127.0.0.1', '--port', String(PORT), '--data-dir', INSTANCE_DIR, '--spa-dir', spaDir()],
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
    rmSync(INSTANCE_DIR, { recursive: true, force: true });
  });

  test('nothing saved: ONE toast with the note; the text, the file and the draft come back', async ({ page }) => {
    test.skip(!P4D288_SERVER_LANDED, 'P4.D301 — flips LIVE at the round unification (§S.9)');
    test.setTimeout(180_000);
    await dispatch(page.request, { type: 'unlock', passphrase: E2E_PASSPHRASE });
    const usersBefore = userRowCount(SOLO_VOYAGE);

    await page.goto(`${BASE}/salon/${SOLO_VOYAGE}`);
    await unlockIfLocked(page);
    await expect(composer(page)).toBeVisible({ timeout: 15_000 });

    await page.getByRole('button', { name: 'Attach file', exact: true }).click();
    await page.locator('input[type=file][aria-label="Choose a file to attach"]').setInputFiles({
      name: FILE_NAME,
      mimeType: 'text/plain',
      buffer: Buffer.from('Terrace, then tea.'),
    });
    await expect(attachmentChips(page)).toContainText(FILE_NAME, { timeout: 15_000 });
    await composer(page).click();
    await page.keyboard.type(TEXT);
    await page.getByRole('button', { name: 'Send message' }).click();

    // Exactly ONE error toast, v4's bytes (the measured pre-fix state raised two).
    await expect(errorToasts(page)).toHaveCount(1, { timeout: 20_000 });
    await expect(errorToasts(page).first()).toHaveText(NO_KEY_TOAST);
    // …and it stays one: no late second toast from the frame.
    await page.waitForTimeout(1_500);
    await expect(errorToasts(page)).toHaveCount(1);

    // The words, the file and the draft are back.
    await expect(composer(page)).toHaveText(TEXT);
    await expect(attachmentChips(page)).toContainText(FILE_NAME);
    expect(await page.evaluate((k) => localStorage.getItem(k), DRAFT_KEY)).toBe(TEXT);
    // Nothing reached the database, and the optimistic bubble is gone.
    expect(userRowCount(SOLO_VOYAGE)).toBe(usersBefore);
    await expect(page.locator('qt-message-row').filter({ hasText: TEXT })).toHaveCount(0);

    // A reload finds the draft (the restore wrote it at once, no 800 ms wait).
    await page.reload();
    await unlockIfLocked(page);
    await expect(composer(page)).toHaveText(TEXT, { timeout: 15_000 });
  });

  test('saved, then failed: ONE toast without the note, and the composer stays empty', async ({ page }) => {
    test.skip(!P4D288_SERVER_LANDED, 'P4.D301 — flips LIVE at the round unification (§S.9)');
    test.setTimeout(180_000);
    const ctx = page.request;
    await dispatch(ctx, { type: 'unlock', passphrase: E2E_PASSPHRASE });
    // Move the seat onto the keyed, dead-endpoint profile.
    const chat = (await dispatch(ctx, { type: 'chatGet', chatId: SOLO_VOYAGE })).data?.['chat'] as {
      participants: { id: string; controlledBy: string }[];
    };
    const seat = chat.participants.find((p) => p.controlledBy === 'llm')!;
    const moved = await dispatch(ctx, {
      type: 'chatUpdateParticipant',
      chatId: SOLO_VOYAGE,
      participantId: seat.id,
      connectionProfileId: DEAD_ENDPOINT_PROFILE,
    });
    expect(moved.type, `the seat move must not refuse: ${JSON.stringify(moved)}`).not.toBe('error');
    const usersBefore = userRowCount(SOLO_VOYAGE);

    await page.goto(`${BASE}/salon/${SOLO_VOYAGE}`);
    await unlockIfLocked(page);
    // Beat 1 left a draft behind; start from an empty composer.
    await page.evaluate((k) => localStorage.removeItem(k), DRAFT_KEY);
    await page.reload();
    await unlockIfLocked(page);
    await expect(composer(page)).toHaveText('', { timeout: 15_000 });

    const line = 'Is anyone at the terrace door?';
    await composer(page).click();
    await page.keyboard.type(line);
    await page.getByRole('button', { name: 'Send message' }).click();

    await expect(errorToasts(page)).toHaveCount(1, { timeout: 60_000 });
    await expect(errorToasts(page).first()).toContainText('Failed to generate response:');
    await expect(errorToasts(page).first()).not.toContainText('Your message is back in the composer.');
    await page.waitForTimeout(1_500);
    await expect(errorToasts(page)).toHaveCount(1);

    // The row landed, so nothing comes back.
    expect(userRowCount(SOLO_VOYAGE)).toBe(usersBefore + 1);
    await expect(composer(page)).toHaveText('');
    expect(await page.evaluate((k) => localStorage.getItem(k), DRAFT_KEY)).toBeNull();
  });
});

function composer(page: Page) {
  return page.locator('.qt-chat-composer-input .qt-rich-editor-content');
}

function attachmentChips(page: Page) {
  return page.locator('.qt-chat-attachment-chip');
}

function errorToasts(page: Page) {
  return page.locator('[role="toast-container"] > .qt-toast-error');
}

async function dispatch(
  ctx: APIRequestContext,
  req: unknown,
): Promise<{ type?: string; data?: Record<string, unknown> }> {
  const res = await ctx.post(`${BASE}/api/dispatch`, { data: req });
  return (
    ((await res.json().catch(() => null)) as {
      type?: string;
      data?: Record<string, unknown>;
    } | null) ?? {}
  );
}

/** The chat's USER rows, straight out of the DB. */
function userRowCount(chatId: string): number {
  const res = spawnSync(
    cliBinary(),
    [
      'db',
      '--data-dir',
      INSTANCE_DIR,
      '--json',
      `SELECT COUNT(*) AS n FROM chat_messages WHERE chatId = '${chatId}' AND role = 'USER';`,
    ],
    {
      env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
      encoding: 'utf8',
    },
  );
  if (res.status !== 0) throw new Error(`CLI read failed:\n${res.stdout}\n${res.stderr}`);
  return (JSON.parse(res.stdout) as { n: number }[])[0]?.n ?? 0;
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
  const res = spawnSync(cli, ['db', '--data-dir', INSTANCE_DIR, '--write', sql], {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  if (res.status !== 0) {
    const out = `${res.stdout}${res.stderr}`;
    if (out.includes('no such table') || out.includes('no such column: userId')) {
      console.warn(`fixture rewrite skipped (not user-scoped): ${sql}`);
      return;
    }
    throw new Error(`CLI write failed (${sql}):\n${res.stdout}\n${res.stderr}`);
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
      const res = await fetch(`${BASE}/health`);
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${PORT}: ${lastErr}`);
}
