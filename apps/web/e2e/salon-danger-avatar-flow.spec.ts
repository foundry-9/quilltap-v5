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
 * P4.69 — the assistant-side danger ring on message avatars.
 *
 * v4 paints it from `SalonView.tsx:1489`
 * (`isDangerousChat={shouldShowDangerStyling(chat)}`) down through
 * `VirtualizedMessageList` to `MessageDesktopAvatar.tsx:19-21`, which adds
 * `qt-chat-avatar-dangerous` beside `qt-chat-desktop-avatar` — on ASSISTANT
 * rows only (`MessageRow.tsx:232`, `:280`); the user-side avatar at `:487`
 * passes no `dangerous` at all. v5 shipped the CSS rule
 * (`_chat.css:2960`, byte-identical to v4's `:2819`) but nothing ever added
 * the class, so the rule was dead.
 *
 * ## The three states (P4.D229, v4 `4d370a90f` #75) — the leg INVERTED
 *
 * The predicate is `getConciergeState(chat) === 'unmoderated'` — Unmoderated
 * WHOEVER set it: "the provenance goes in the tooltip and helper text, never
 * in colour." Under the four-state rule an operator-set Uncensored chat was
 * deliberately left UNPAINTED and this walk asserted exactly that; v4 #75
 * inverted it. So the walk now drives the operator's own Unmoderated pick and
 * asserts the rings APPEAR, then Locked and asserts they go — reading the
 * stored triplet (`conciergeMode`, `conciergeModeSetBy`, `conciergeModeReason`)
 * through the CLI each time, which is what proves the ring followed the STATE
 * and not the classifier's `isDangerousChat` telemetry (untouched throughout).
 *
 * GATED on the server chain: `main`'s server 400s the three new values and
 * its chats carry no `conciergeMode` column (P4.D226 widens the fixture).
 *
 * 'Solo Voyage' is the fixture's 2×USER + 2×ASSISTANT chat, so the counts are
 * exact: four avatars, two of which may ring.
 */
const DANGER_PORT = 4332;
const BASE = `http://127.0.0.1:${DANGER_PORT}`;
const INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'danger-avatar-instance');
const DATA_DIR = resolve(INSTANCE_DIR, 'data');
const SERVER_LOG = resolve(ARTIFACTS_DIR, 'danger-avatar-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'salon-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'salon-mount.db');
const USER_TABLES = ['characters', 'chats', 'tags', 'groups', 'projects', 'files'];

const P4D228_SERVER_LANDED = false;
const GATE_REASON =
  'awaits the Concierge server chain (P4.D225→P4.D228: the three states + the stored conciergeMode trio); flipped at unification';

let server: ChildProcess | undefined;

test.describe('P4.69 — the dangerous-chat avatar ring', () => {
  test.skip(!P4D228_SERVER_LANDED, GATE_REASON);

  test.beforeAll(async () => {
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

    const logFd = openSync(SERVER_LOG, 'w');
    server = spawn(
      web,
      [
        '--host',
        '127.0.0.1',
        '--port',
        String(DANGER_PORT),
        '--data-dir',
        INSTANCE_DIR,
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
    rmSync(INSTANCE_DIR, { recursive: true, force: true });
  });

  test('rings the assistant avatars on an operator-set Unmoderated chat, and drops them for Locked', async ({
    page,
  }) => {
    test.setTimeout(180_000);
    const ctx = page.request;
    await dispatch(ctx, { type: 'unlock', passphrase: E2E_PASSPHRASE });

    const chats = await dispatch(ctx, { type: 'listChats' });
    const chatId = ((chats.data as unknown as { id: string; title: string }[]) ?? []).find(
      (c) => c.title === 'Solo Voyage',
    )!.id;
    expect(chatId).toBeTruthy();

    await page.goto(`${BASE}/salon/${chatId}`);
    await unlockIfLocked(page);

    // --- baseline: a Moderated chat paints nothing -------------------------
    // Four avatars are on screen (2 USER + 2 ASSISTANT rows). Pin the total so
    // a later zero-ring assertion cannot pass because the rows never rendered.
    await expect(avatars(page), 'four avatars on the settled rows').toHaveCount(4, {
      timeout: 15_000,
    });
    await expect(rings(page), 'Moderated paints no ring').toHaveCount(0);
    expect(readStoredTriplet(chatId)).toEqual({ mode: 'moderated', setBy: null, reason: null });

    // --- the OPERATOR opens the door: the two ASSISTANT avatars ring --------
    await dispatch(ctx, { type: 'chatUpdate', chatId, chat: {}, conciergeState: 'unmoderated' });
    await page.reload();
    await unlockIfLocked(page);
    await expect(avatars(page)).toHaveCount(4, { timeout: 15_000 });
    await expect(
      rings(page),
      'an operator-set Unmoderated chat IS painted — provenance is never a colour (v4 4d370a90f)',
    ).toHaveCount(2, { timeout: 15_000 });
    // ...and they are the ASSISTANT ones. v4's user-side avatar (`:487`) never
    // takes `dangerous`, so every ring must sit inside an assistant row and the
    // user rows must hold none.
    await expect(
      page.locator('.qt-chat-message-row-assistant .qt-chat-avatar-dangerous'),
      'every ring sits on an assistant row (v4 MessageRow:232/:280)',
    ).toHaveCount(2);
    await expect(
      page.locator('.qt-chat-message-row-user .qt-chat-avatar-dangerous'),
      'no ring on a user row (v4 MessageRow:487 passes no `dangerous`)',
    ).toHaveCount(0);
    // The discriminator: the operator set it, so the four-state rule would
    // have left these rows unpainted.
    expect(readStoredTriplet(chatId)).toEqual({ mode: 'unmoderated', setBy: 'operator', reason: 'manual' });

    // --- Locked: the ordinary desks only, no rings --------------------------
    await dispatch(ctx, { type: 'chatUpdate', chatId, chat: {}, conciergeState: 'locked' });
    await page.reload();
    await unlockIfLocked(page);
    await expect(avatars(page), 'the rows are still on screen').toHaveCount(4, { timeout: 15_000 });
    await expect(rings(page), 'a Locked chat is not painted').toHaveCount(0, { timeout: 15_000 });
    expect(readStoredTriplet(chatId)).toEqual({ mode: 'locked', setBy: 'operator', reason: 'manual' });
  });
});

function avatars(page: Page) {
  return page.locator('.qt-chat-messages-list .qt-chat-desktop-avatar');
}

function rings(page: Page) {
  return page.locator('.qt-chat-messages-list .qt-chat-avatar-dangerous');
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

/**
 * The stored triplet, straight out of the DB (v4's own acceptance script reads
 * the same three columns). A NULL `conciergeMode` reads as Moderated.
 */
function readStoredTriplet(chatId: string): {
  mode: string;
  setBy: string | null;
  reason: string | null;
} {
  const res = spawnSync(
    cliBinary(),
    [
      'db',
      '--data-dir',
      INSTANCE_DIR,
      '--json',
      `SELECT "conciergeMode" AS m, "conciergeModeSetBy" AS s, "conciergeModeReason" AS r FROM chats WHERE id = '${chatId}';`,
    ],
    {
      env: {
        ...withoutPepper(),
        QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE,
        QUILLTAP_QUIET_HINTS: '1',
      },
      encoding: 'utf8',
    },
  );
  if (res.status !== 0) throw new Error(`CLI read failed:\n${res.stdout}\n${res.stderr}`);
  const rows = JSON.parse(res.stdout) as { m: string | null; s: string | null; r: string | null }[];
  return { mode: rows[0]?.m ?? 'moderated', setBy: rows[0]?.s ?? null, reason: rows[0]?.r ?? null };
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
      const res = await fetch(`${BASE}/health`);
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${DANGER_PORT}: ${lastErr}`);
}
