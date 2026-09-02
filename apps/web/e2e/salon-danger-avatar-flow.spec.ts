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
 * ## Why the Uncensored leg is the whole point
 *
 * The predicate is `getConciergeState(chat) === 'flagged'` — NOT the raw
 * `chat.isDangerousChat`. The two come apart on exactly the pair v4's own
 * acceptance walk calls CT-2: flip Flagged → Uncensored and the stored label
 * SURVIVES as `1` under the operator's override. So this walk drives that
 * transition and asserts the rings vanish while the DB still says `1`. Had the
 * Salon bound the raw flag (the shape `SalonView.tsx:1876` passes to the
 * SIDEBAR, a different consumer), the rings would still be on screen and only
 * this leg would catch it.
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

let server: ChildProcess | undefined;

test.describe('P4.69 — the dangerous-chat avatar ring', () => {
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

  test('rings the assistant avatars when Flagged, and drops them for Uncensored with the label still stored', async ({
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

    // --- baseline: a Monitored chat paints nothing -------------------------
    // Four avatars are on screen (2 USER + 2 ASSISTANT rows). Pin the total so
    // a later zero-ring assertion cannot pass because the rows never rendered.
    await expect(avatars(page), 'four avatars on the settled rows').toHaveCount(4, {
      timeout: 15_000,
    });
    await expect(rings(page), 'Monitored paints no ring').toHaveCount(0);
    expect(readStoredPair(chatId)).toEqual({ override: null, dangerous: 0 });

    // --- Flagged: the two ASSISTANT avatars ring ---------------------------
    await dispatch(ctx, { type: 'chatUpdate', chatId, chat: {}, conciergeState: 'flagged' });
    await page.reload();
    await unlockIfLocked(page);
    await expect(avatars(page)).toHaveCount(4, { timeout: 15_000 });
    await expect(rings(page), 'Flagged rings both assistant avatars').toHaveCount(2, {
      timeout: 15_000,
    });
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
    expect(readStoredPair(chatId)).toEqual({ override: null, dangerous: 1 });

    // --- Uncensored: rings gone, label PRESERVED (v4's CT-2 pair) ----------
    await dispatch(ctx, { type: 'chatUpdate', chatId, chat: {}, conciergeState: 'uncensored' });
    await page.reload();
    await unlockIfLocked(page);
    await expect(avatars(page), 'the rows are still on screen').toHaveCount(4, { timeout: 15_000 });
    await expect(
      rings(page),
      'an operator-Uncensored chat is deliberately NOT painted (v4 chat-override.ts:110)',
    ).toHaveCount(0, { timeout: 15_000 });
    // The discriminator: the raw flag underneath is still 1. Binding
    // `chat.isDangerousChat` instead of `shouldShowDangerStyling(chat)` would
    // have left both rings on screen right here.
    expect(
      readStoredPair(chatId),
      'the stored label survives the override — this is what makes the leg above discriminating',
    ).toEqual({ override: 'UNCENSORED', dangerous: 1 });
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

/** The stored pair, straight out of the DB — the UI cannot show the label under an override. */
function readStoredPair(chatId: string): { override: string | null; dangerous: number } {
  const res = spawnSync(
    cliBinary(),
    [
      'db',
      '--data-dir',
      INSTANCE_DIR,
      '--json',
      `SELECT "conciergeOverride" AS o, "isDangerousChat" AS d FROM chats WHERE id = '${chatId}';`,
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
  const rows = JSON.parse(res.stdout) as { o: string | null; d: number | null }[];
  return { override: rows[0]?.o ?? null, dangerous: Number(rows[0]?.d ?? 0) };
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
