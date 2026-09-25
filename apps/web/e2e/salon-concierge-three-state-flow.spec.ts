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
 * P4.D229 — the Concierge's THREE-state per-chat control (v4 `4d370a90f` #75),
 * modelled on v4's own acceptance script `scripts/concierge-three-state-test.sh`
 * (the renamed `concierge-four-state-test.sh`). Replaces the P4.D141
 * four-state walk, whose states are retired (a 400 on the chain's server).
 *
 * The walk drives every transition through the SIDEBAR CONTROL (not the API),
 * and after each one asserts three things:
 *
 *   1. the stored TRIPLET (`conciergeMode`, `conciergeModeSetBy`,
 *      `conciergeModeReason`) — read straight out of the DB through the CLI,
 *      exactly as v4's script reads it: Moderated always clears the
 *      provenance, the other two always stamp (`operator`, `manual`);
 *   2. the Concierge's announcement phrase in the transcript — the three
 *      transitions have three distinct sentences (v4's script greps the same
 *      phrases);
 *   3. the header badge, which renders NOTHING for Moderated and one pill
 *      otherwise.
 *
 * Then the two refusals v4's script checks: a RETIRED value (`flagged`) is a
 * 400 with nothing written, and — the Locked refusal — a Locked chat refuses
 * the Concierge's line retry (§S.3's `messageRetryUncensored` answers the bare
 * token `locked`), which is why the UI offers no "Try uncensored" there.
 *
 * ## ACTIVATE-AT-UNIFY
 *
 * Everything here is the server chain's (P4.D225 → P4.D228): `main`'s server
 * takes the four retired values, and its chats carry no `conciergeMode` trio
 * (P4.D226 widens the fixture). The unifier flips
 * {@link P4D228_SERVER_LANDED} after the pick and runs the walk live.
 */
const P4D228_SERVER_LANDED = false;
const GATE_REASON =
  'awaits the Concierge server chain (P4.D225→P4.D228: the three states, the stored trio, the retry verbs); flipped at unification';

const CONCIERGE_PORT = 4331;
const BASE = `http://127.0.0.1:${CONCIERGE_PORT}`;
const INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'concierge-instance');
const DATA_DIR = resolve(INSTANCE_DIR, 'data');
const SERVER_LOG = resolve(ARTIFACTS_DIR, 'concierge-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'salon-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'salon-mount.db');
const USER_TABLES = ['characters', 'chats', 'tags', 'groups', 'projects', 'files'];

/**
 * v4's three operator sentences (`concierge-notifications/writer.ts` at
 * `4d370a90f`), each cut to the phrase v4's own acceptance script greps for.
 */
const PHRASE = {
  unmoderated: 'uncensored door stands open',
  moderated: 'Moderated once more',
  locked: 'locked the present company',
} as const;

type State = keyof typeof PHRASE;

const LABEL: Record<State, string | null> = {
  moderated: null,
  unmoderated: 'Unmoderated',
  locked: 'Locked',
};

/** v4's script's seven transitions, in its order. */
const WALK: State[] = [
  'unmoderated', // Moderated → Unmoderated
  'moderated', // Unmoderated → Moderated
  'locked', // Moderated → Locked
  'moderated', // Locked → Moderated
  'unmoderated', // Moderated → Unmoderated (again)
  'locked', // Unmoderated → Locked
  'unmoderated', // Locked → Unmoderated
];

/** Moderated always clears the provenance; the other two always stamp the operator's. */
function expectedTriplet(state: State): { mode: State; setBy: string | null; reason: string | null } {
  return state === 'moderated'
    ? { mode: 'moderated', setBy: null, reason: null }
    : { mode: state, setBy: 'operator', reason: 'manual' };
}

let server: ChildProcess | undefined;

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

test.describe('P4.D229 — the Concierge three-state per-chat control', () => {
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
        String(CONCIERGE_PORT),
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

  test('walks v4’s seven transitions, then refuses a retired value and a Locked retry', async ({
    page,
  }) => {
    test.setTimeout(180_000);
    const ctx = page.request;
    await dispatch(ctx, { type: 'unlock', passphrase: E2E_PASSPHRASE });

    // `listChats` is the REQUEST verb (`chats` is the RESPONSE tag).
    const chats = await dispatch(ctx, { type: 'listChats' });
    const chatId = ((chats.data as unknown as { id: string; title: string }[]) ?? []).find(
      (c) => c.title === 'Solo Voyage',
    )!.id;
    expect(chatId).toBeTruthy();

    await page.goto(`${BASE}/salon/${chatId}`);
    await unlockIfLocked(page);
    await openChatDrawer(page);

    const select = conciergeSelect(page);
    await expect(select).toBeVisible({ timeout: 15_000 });
    // A fresh fixture chat is Moderated: no badge, no provenance.
    await expect(select).toHaveValue('moderated');
    await expect(conciergeBadge(page)).toHaveCount(0);
    expect(readStoredTriplet(chatId)).toEqual(expectedTriplet('moderated'));

    // A FLAT list of bare labels — who set Unmoderated is a note, never an option.
    await expect(select.locator('optgroup')).toHaveCount(0);
    await expect(select.locator('option')).toHaveText(['Moderated', 'Unmoderated', 'Locked']);

    for (const [i, pick] of WALK.entries()) {
      const before = await conciergeChips(page).count();
      await select.selectOption(pick);

      // 1. The control settles on the picked state.
      await expect(select, `step ${i + 1} (${pick}) — the select`).toHaveValue(pick, {
        timeout: 15_000,
      });

      // 2. The Concierge said the right thing. Exactly ONE new chip, and
      //    expanding it shows the phrase that identifies THIS transition.
      //    Expanded chips stay expanded and phrases repeat across the walk, so
      //    the discriminating form is the COUNT of phrase bubbles so far.
      await expect(conciergeChips(page), `step ${i + 1} — one new chip`).toHaveCount(before + 1, {
        timeout: 15_000,
      });
      await conciergeChips(page).last().click();
      const phraseBubbles = WALK.slice(0, i + 1).filter((s) => s === pick).length;
      await expect(
        page.locator('.qt-chat-messages-list').getByText(PHRASE[pick]),
        `step ${i + 1} — the phrase "${PHRASE[pick]}" (${phraseBubbles} bubble(s) so far)`,
      ).toHaveCount(phraseBubbles, { timeout: 15_000 });

      // 3. The stored triplet, read from the DB.
      expect(readStoredTriplet(chatId), `step ${i + 1} (${pick}) — the stored triplet`).toEqual(
        expectedTriplet(pick),
      );

      // 4. The header badge: nothing at all for Moderated, one pill otherwise.
      const label = LABEL[pick];
      if (label === null) {
        await expect(conciergeBadge(page), `step ${i + 1} — no badge`).toHaveCount(0);
      } else {
        await expect(conciergeBadge(page), `step ${i + 1} — one badge`).toHaveCount(1);
        await expect(conciergeBadge(page)).toHaveText(label);
      }
    }

    // The walk ends Unmoderated. A RETIRED value is refused with nothing written.
    const retired = await dispatch(ctx, {
      type: 'chatUpdate',
      chatId,
      chat: {},
      conciergeState: 'flagged',
    });
    expect(retired.type, 'a retired four-state value is refused (v4 400)').toBe('error');
    expect(readStoredTriplet(chatId)).toEqual(expectedTriplet('unmoderated'));

    // The Locked refusal: lock it through the control, then ask the Concierge
    // to re-roll a line anyway — the verb refuses with the bare token, and the
    // UI offers no button to ask with.
    await select.selectOption('locked');
    await expect(conciergeBadge(page)).toHaveText('Locked', { timeout: 15_000 });
    await expect(
      page.locator('.qt-chat-message-action-bar button[aria-label="Try uncensored"]'),
    ).toHaveCount(0);
    const detail = await dispatch(ctx, { type: 'chatGet', chatId });
    const messages = ((detail.data?.['chat'] as { messages?: { id: string; role: string; systemSender?: string | null }[] })
      ?.messages ?? []);
    const line = messages.filter((m) => m.role === 'ASSISTANT' && !m.systemSender).at(-1);
    expect(line, 'the fixture chat has a character line to retry').toBeTruthy();
    const refused = await dispatch(ctx, {
      type: 'messageRetryUncensored',
      messageId: line!.id,
      stream: false,
    });
    expect(refused.type).toBe('error');
    expect((refused.data as { message?: string } | undefined)?.message).toBe('locked');
  });
});

function conciergeSelect(page: Page) {
  return page
    .locator('qt-chat-sidebar label')
    .filter({ hasText: 'The Concierge' })
    .locator('select');
}

function conciergeBadge(page: Page) {
  return page.locator('qt-conversation-header .qt-danger-badge');
}

/**
 * The Concierge's collapsed announcement chips. v5 chips Staff-signed
 * announcements, so the sentence itself is not in the DOM until the chip is
 * expanded — the same shape the scenario walk works with.
 */
function conciergeChips(page: Page) {
  return page
    .locator('.qt-chat-announcement-chip')
    .filter({ hasText: 'The Concierge' });
}

/**
 * The stored triplet, straight out of the DB — v4's acceptance script reads the
 * same three columns. A NULL `conciergeMode` reads as Moderated.
 */
function readStoredTriplet(chatId: string): { mode: string; setBy: string | null; reason: string | null } {
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
      // 423 is "server up, instance locked" — the normal state of a fresh
      // fixture boot; the walk unlocks through the dispatch verb / the UI gate.
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${CONCIERGE_PORT}: ${lastErr}`);
}
