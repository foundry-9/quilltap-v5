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
 * P4.D141 — the Concierge four-state per-chat control (v4 `60e3c4a0a`),
 * modelled on v4's own acceptance script `concierge-four-state-test.sh`.
 *
 * The walk drives every transition through the SIDEBAR CONTROL (not the API),
 * and after each one asserts three things:
 *
 *   1. the stored pair `(conciergeOverride, isDangerousChat)` — read straight
 *      out of the DB through the CLI, because the pair is the whole point: both
 *      operator states PRESERVE the label underneath;
 *   2. the Concierge's announcement phrase in the transcript — the five kinds
 *      have five distinct sentences, so the phrase identifies the transition
 *      the server actually took;
 *   3. the header badge, which renders NOTHING for Monitored and one pill
 *      otherwise.
 *
 * The ten transitions cover both operator states in and out, both provenances,
 * and the two CT-2 preserve pairs (`OFF,1` and `UNCENSORED,1`) — the ones that
 * prove an operator state does not clear the classifier's verdict.
 *
 * ## ACTIVATE-AT-UNIFY
 *
 * The control derives its state from BOTH stored fields, and
 * `conciergeOverride` reaches `<qt-chat-sidebar>` from
 * `screens/salon/salon-conversation.ts` — a file **P4.66 owns**, so this lane
 * cannot add the binding. Until the unifier adds
 * `[conciergeOverride]="c.conciergeOverride ?? null"` to that element, the
 * control can only ever DISPLAY Monitored/Flagged, and every step of this walk
 * that reads back an operator state would fail for a reason that says nothing
 * about the feature. The unifier flips {@link P4D141_SALON_WIRE_LANDED} to
 * `true` with that binding.
 */
const P4D141_SALON_WIRE_LANDED = true;

const CONCIERGE_PORT = 4331;
const BASE = `http://127.0.0.1:${CONCIERGE_PORT}`;
const INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'concierge-instance');
const DATA_DIR = resolve(INSTANCE_DIR, 'data');
const SERVER_LOG = resolve(ARTIFACTS_DIR, 'concierge-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'salon-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'salon-mount.db');
const USER_TABLES = ['characters', 'chats', 'tags', 'groups', 'projects', 'files'];

/**
 * v4's five manual sentences (`concierge-notifications/writer.ts`), each cut to
 * the phrase that identifies its kind — the same phrases v4's own acceptance
 * script greps for.
 */
const PHRASE = {
  flagged: 'thrown the switch',
  safe: 'stands down for the moment',
  vouched: 'takes the afternoon off',
  resumed: 'returns to his post',
  uncensored: 'uncensored door stands open',
} as const;

/**
 * The ten transitions, in order. `stored` is the `(conciergeOverride,
 * isDangerousChat)` pair the DB must hold afterwards — `null` for a cleared
 * override, and the two CT-2 preserve pairs are the rows where the label stays
 * `1` under an operator state.
 */
const WALK: {
  pick: 'monitored' | 'flagged' | 'vouched' | 'uncensored';
  label: string;
  phrase: string;
  stored: { override: string | null; dangerous: number };
  badge: string | null;
}[] = [
  { pick: 'flagged', label: 'Flagged', phrase: PHRASE.flagged, stored: { override: null, dangerous: 1 }, badge: 'Flagged' },
  { pick: 'monitored', label: 'Monitored', phrase: PHRASE.safe, stored: { override: null, dangerous: 0 }, badge: null },
  { pick: 'vouched', label: 'Vouched Safe', phrase: PHRASE.vouched, stored: { override: 'OFF', dangerous: 0 }, badge: 'Vouched Safe' },
  { pick: 'monitored', label: 'Monitored', phrase: PHRASE.resumed, stored: { override: null, dangerous: 0 }, badge: null },
  { pick: 'uncensored', label: 'Uncensored', phrase: PHRASE.uncensored, stored: { override: 'UNCENSORED', dangerous: 0 }, badge: 'Uncensored' },
  { pick: 'monitored', label: 'Monitored', phrase: PHRASE.resumed, stored: { override: null, dangerous: 0 }, badge: null },
  // CT-2, part one: Flagged first, then Vouched — the label must SURVIVE.
  { pick: 'flagged', label: 'Flagged', phrase: PHRASE.flagged, stored: { override: null, dangerous: 1 }, badge: 'Flagged' },
  { pick: 'vouched', label: 'Vouched Safe', phrase: PHRASE.vouched, stored: { override: 'OFF', dangerous: 1 }, badge: 'Vouched Safe' },
  // CT-2, part two: straight across to Uncensored — the label still survives.
  { pick: 'uncensored', label: 'Uncensored', phrase: PHRASE.uncensored, stored: { override: 'UNCENSORED', dangerous: 1 }, badge: 'Uncensored' },
  // …and back, which CLEARS the label (the Monitored arm resets the metadata).
  { pick: 'monitored', label: 'Monitored', phrase: PHRASE.resumed, stored: { override: null, dangerous: 0 }, badge: null },
];

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

test.describe('P4.D141 — the Concierge four-state per-chat control', () => {
  test.skip(
    !P4D141_SALON_WIRE_LANDED,
    'awaits the salon-conversation [conciergeOverride] binding (P4.66’s file; wired at unification)',
  );

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

  test('walks all ten transitions, preserving the label under both operator states', async ({
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
    // A fresh fixture chat is Monitored: no override, no verdict.
    await expect(select).toHaveValue('monitored');
    await expect(conciergeBadge(page)).toHaveCount(0);

    // The provenance is structural, not just copy: two optgroups, two each.
    await expect(select.locator('optgroup')).toHaveCount(2);
    expect(await select.locator('optgroup').first().getAttribute('label')).toBe(
      'The Concierge decides',
    );
    expect(await select.locator('optgroup').last().getAttribute('label')).toBe('You decide');

    for (const [i, step] of WALK.entries()) {
      const before = await conciergeChips(page).count();
      await select.selectOption(step.pick);

      // 1. The control settles on the picked state.
      await expect(select, `step ${i + 1} (${step.pick}) — the select`).toHaveValue(step.pick, {
        timeout: 15_000,
      });

      // 2. The Concierge said the right thing. Exactly ONE new chip, and
      //    expanding it shows the phrase that identifies THIS transition —
      //    `manual-resumed` and `manual-safe` both return to Monitored and are
      //    told apart only by their sentences.
      await expect(conciergeChips(page), `step ${i + 1} — one new chip`).toHaveCount(before + 1, {
        timeout: 15_000,
      });
      await conciergeChips(page).last().click();
      await expect(
        page.locator('.qt-chat-messages-list').getByText(step.phrase),
        `step ${i + 1} — the phrase "${step.phrase}"`,
      ).toBeVisible({ timeout: 15_000 });

      // 3. The stored PAIR, read from the DB. This is where CT-2 lives: after
      //    Flagged → Vouched the label is still 1, and after Vouched →
      //    Uncensored it is still 1.
      const row = readStoredPair(chatId);
      expect(row, `step ${i + 1} (${step.pick}) — the stored pair`).toEqual(step.stored);

      // 4. The header badge: nothing at all for Monitored, one pill otherwise.
      if (step.badge === null) {
        await expect(conciergeBadge(page), `step ${i + 1} — no badge`).toHaveCount(0);
      } else {
        await expect(conciergeBadge(page), `step ${i + 1} — one badge`).toHaveCount(1);
        await expect(conciergeBadge(page)).toHaveText(step.badge);
      }
    }
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
 * The stored pair, straight out of the DB — the assertion the UI cannot make,
 * since both operator states render the same whatever the label underneath is.
 */
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
