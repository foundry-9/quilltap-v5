import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, openSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer, type Server } from 'node:http';
import { resolve } from 'node:path';

import { expect, test, type Page } from './support/fixtures';

import { makeDbKeyFile } from './support/dbkey';
import { openSidebarSection } from './support/sidebar';
import {
  ARTIFACTS_DIR,
  cliBinary,
  E2E_PASSPHRASE,
  FIXTURE_USER,
  FIXTURES_DIR,
  REPO_ROOT,
  SINGLE_USER_ID,
  spaDir,
  TEST_PEPPER,
  webBinary,
} from './support/env';

/**
 * P4.D344 — Storyteller mode, the SPA half (v4 `b15fe9404` + `946a50a04` at the
 * pin `236c04e35`): the three-way Turn mode radiogroup in the Participants
 * drawer (it REPLACES the expanded Pause button), the `Invite by talkativeness`
 * checkbox, the board above the composer filling with the company's bids, the
 * operator's run / let-lie, and the Inspector's `Storyteller Rounds` filter.
 *
 * Its own instance on port 4353 and its own mock on 45351 (the round's §R.9
 * ports), copied from the `salon-interrupt-flow` recipe: the salon pair →
 * `e2e/.artifacts/salon-storyteller-instance/`, the userId rewrite,
 * `connection_profiles.baseUrl` aimed at the mock, the `instance_settings`
 * plant.
 *
 * **The mock is SPEC-LOCAL and BID-AWARE** (R-2; `e2e/support/mock-llm.ts` is
 * untouched). The recipe:
 *  - `GET /models` answers a one-model list;
 *  - `POST /chat/completions` reads the body: when ANY message carries BOTH
 *    `[Turn note from the Salon — not spoken by any character]` AND `answer
 *    only with your bid` (v4 `lib/storyteller/prompts.ts:24, :70`), it streams
 *    the four bid lines `BID: speak` / `TIER: simple` / `TOOLS: none` /
 *    `PLAN: I shall put the kettle on.`;
 *  - when a message carries `The storyteller chose you to go ahead` (the plan
 *    section's first line, `:115`), it streams `plan.turnReply`;
 *  - anything else streams `plan.reply` (prose); every reply word-by-word,
 *    `plan.delayMs` apart, then a `usage` chunk and `[DONE]`; `plan.calls`
 *    counts the calls. Keying on the prompt bytes proves the SERVER sent the
 *    right prompt to the right call — a mis-routed call is a red beat.
 *
 * **GATED beats (A)–(F2), (K)** need P4.D343 (the four `chatStoryteller*`
 * variants, the `turnMode` sibling, the board frames). ACTIVATE-AT-UNIFY
 * (§S.9): the unifier flips `P4D343_SERVER_LANDED` to `true`; (I)'s premise (a
 * server that drops `turnMode`) is then gone, so (I) skips itself and (A)
 * supersedes it.
 *
 * **UNGATED beats (G)–(J)** hold on main's server today.
 */
const P4D343_SERVER_LANDED = true;
const GATE_REASON =
  'awaits P4.D343’s chatStorytellerRound / -Run / -Ask / -Lie variants, the turnMode sibling and the storytellerRound / storytellerBid / roundOpen frames (wired at unification)';

const ST_PORT = 4353;
const ST_BASE_URL = `http://127.0.0.1:${ST_PORT}`;
const ST_MOCK_PORT = 45351;
const ST_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'salon-storyteller-instance');
const ST_DATA_DIR = resolve(ST_INSTANCE_DIR, 'data');
const ST_SERVER_LOG = resolve(ARTIFACTS_DIR, 'salon-storyteller-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'salon-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'salon-mount.db');
const USER_TABLES = ['characters', 'chats', 'tags', 'groups', 'projects', 'files'];

const NOTE_HEADER = '[Turn note from the Salon — not spoken by any character]';
const BID_ASK = 'answer only with your bid';
const PLAN_LINE = 'The storyteller chose you to go ahead';
const BID_PLAN = 'I shall put the kettle on.';
const BID_REPLY = `BID: speak\nTIER: simple\nTOOLS: none\nPLAN: ${BID_PLAN}`;
const TURN_REPLY = 'The kettle is on, as promised.';
const PROSE_REPLY = 'Indeed, the weather holds.';

const STORYTELLER_TOAST = 'Storyteller mode: each of your posts asks the company who wishes to answer';
const BOARD = 'section[aria-label="The storyteller\'s board"]';

/** What the spec-local mock answers; a beat sets it before each send. */
const plan: { reply: string; turnReply: string; delayMs: number; calls: number; bidCalls: number } = {
  reply: PROSE_REPLY,
  turnReply: TURN_REPLY,
  delayMs: 0,
  calls: 0,
  bidCalls: 0,
};

let server: ChildProcess | undefined;
let mock: Server | undefined;
let groupChatId = '';

function messageTexts(body: string): string[] {
  try {
    const parsed = JSON.parse(body) as { messages?: { content?: unknown }[] };
    return (parsed.messages ?? []).map((m) =>
      typeof m.content === 'string' ? m.content : JSON.stringify(m.content ?? ''),
    );
  } catch {
    return [];
  }
}

function startBidAwareMock(port: number): Promise<Server> {
  const srv = createServer((req, res) => {
    if (req.method === 'GET' && req.url?.includes('/models') && !req.url.includes('/chat/completions')) {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ data: [{ id: 'mock-model' }] }));
      return;
    }
    if (req.method !== 'POST' || !req.url?.includes('/chat/completions')) {
      res.writeHead(404).end();
      return;
    }
    let body = '';
    req.on('data', (chunk) => {
      body += chunk;
    });
    req.on('end', () => {
      plan.calls += 1;
      const texts = messageTexts(body);
      let reply = plan.reply;
      if (texts.some((t) => t.includes(NOTE_HEADER) && t.includes(BID_ASK))) {
        reply = BID_REPLY;
        plan.bidCalls += 1;
      } else if (texts.some((t) => t.includes(PLAN_LINE))) {
        reply = plan.turnReply;
      }
      let streamed = false;
      try {
        streamed = (JSON.parse(body) as { stream?: boolean }).stream === true;
      } catch {
        // not JSON
      }
      if (!streamed) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(
          JSON.stringify({
            id: 'mock-1',
            object: 'chat.completion',
            model: 'mock-model',
            choices: [{ index: 0, message: { role: 'assistant', content: reply }, finish_reason: 'stop' }],
            usage: { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15 },
          }),
        );
        return;
      }
      res.writeHead(200, { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache', Connection: 'keep-alive' });
      // Split on spaces only — the bid's newlines ride inside the words.
      const words = reply.split(' ');
      const write = (i: number) => {
        if (res.destroyed) return;
        if (i >= words.length) {
          res.write(
            `data: ${JSON.stringify({
              id: 'mock-1',
              object: 'chat.completion.chunk',
              model: 'mock-model',
              choices: [{ index: 0, delta: {}, finish_reason: 'stop' }],
              usage: { prompt_tokens: 10, completion_tokens: words.length, total_tokens: 10 + words.length },
            })}\n\n`,
          );
          res.write('data: [DONE]\n\n');
          res.end();
          return;
        }
        res.write(
          `data: ${JSON.stringify({
            id: 'mock-1',
            object: 'chat.completion.chunk',
            model: 'mock-model',
            choices: [{ index: 0, delta: { content: (i === 0 ? '' : ' ') + words[i] }, finish_reason: null }],
          })}\n\n`,
        );
        setTimeout(() => write(i + 1), plan.delayMs);
      };
      write(0);
    });
  });
  return new Promise((ok) => srv.listen(port, '127.0.0.1', () => ok(srv)));
}

async function dispatchRaw(req: unknown): Promise<{ type?: string; data?: Record<string, unknown> }> {
  const res = await fetch(`${ST_BASE_URL}/api/dispatch`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(req),
  });
  return ((await res.json().catch(() => null)) as { type?: string; data?: Record<string, unknown> } | null) ?? {};
}

test.describe('P4.D344 — Storyteller mode (the SPA half)', () => {
  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(ST_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(ST_DATA_DIR, { recursive: true });
    mkdirSync(resolve(ST_DATA_DIR, 'files'), { recursive: true });
    copyFileSync(MAIN_FIXTURE, resolve(ST_DATA_DIR, 'quilltap.db'));
    if (existsSync(MOUNT_FIXTURE)) {
      copyFileSync(MOUNT_FIXTURE, resolve(ST_DATA_DIR, 'quilltap-mount-index.db'));
    }
    // Beat (F2) reads the bids' LLM logs: the engine writes them only when the
    // llm-logs partition exists (the global-setup recipe — copy the committed
    // empty one) and logging is on (switched on after unlock, below).
    copyFileSync(resolve(FIXTURES_DIR, 'salon-llm-logs.db'), resolve(ST_DATA_DIR, 'quilltap-llm-logs.db'));
    writeFileSync(resolve(ST_DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));
    for (const table of [...USER_TABLES, 'connection_profiles', 'api_keys', 'chat_settings', 'memories', 'image_profiles']) {
      runCliWrite(cli, `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`);
    }
    runCliWrite(cli, `UPDATE users SET id = '${SINGLE_USER_ID}' WHERE id = '${FIXTURE_USER}';`);
    runCliWrite(
      cli,
      `UPDATE connection_profiles SET baseUrl = 'http://127.0.0.1:${ST_MOCK_PORT}/v1' WHERE provider = 'OPENAI_COMPATIBLE';`,
    );
    mock = await startBidAwareMock(ST_MOCK_PORT);

    // The salon pair carries no `instance_settings` table; plant generateDDL's
    // own from the committed D23 dump (the scenario-browser-flow recipe).
    const freshSchema = JSON.parse(
      readFileSync(resolve(REPO_ROOT, 'crates/quilltap-core/src/services/provisioning/fresh_schema.json'), 'utf8'),
    ) as { main: string[] };
    const settingsDdl = freshSchema.main.find((sql) => sql.startsWith('CREATE TABLE "instance_settings"'));
    if (!settingsDdl) throw new Error('fresh_schema.json carries no instance_settings CREATE TABLE');
    const ddl = spawnSync(cli, ['db', '--data-dir', ST_INSTANCE_DIR, '--write', settingsDdl], {
      env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
      encoding: 'utf8',
    });
    if (ddl.status !== 0 && !`${ddl.stdout}${ddl.stderr}`.includes('already exists')) {
      throw new Error(`instance_settings plant failed:\n${ddl.stdout}\n${ddl.stderr}`);
    }

    const logFd = openSync(ST_SERVER_LOG, 'w');
    server = spawn(
      web,
      ['--host', '127.0.0.1', '--port', String(ST_PORT), '--data-dir', ST_INSTANCE_DIR, '--spa-dir', spaDir()],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    server.unref();
    await waitForHealth();

    await dispatchRaw({ type: 'unlock', passphrase: E2E_PASSPHRASE });
    const logging = await dispatchRaw({
      type: 'chatSettingsUpdate',
      settings: { llmLoggingSettings: { enabled: true, verboseMode: false, retentionDays: 30 } },
    });
    expect(logging.type, `llmLoggingSettings: ${JSON.stringify(logging)}`).not.toBe('error');
    const chats = ((await dispatchRaw({ type: 'listChats' })).data as unknown as { id: string; title: string }[]) ?? [];
    groupChatId = chats.find((c) => c.title === 'Group Expedition')?.id ?? '';
    expect(groupChatId, 'Group Expedition on the pair').toBeTruthy();
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
    await new Promise<void>((r) => (mock ? mock.close(() => r()) : r()));
    // The server may still be flushing its log / journal as SIGTERM lands.
    rmSync(ST_INSTANCE_DIR, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  });

  // ---- ungated ---------------------------------------------------------

  test('(G) the radiogroup replaces the expanded Pause button; the collapsed strip keeps its own', async ({ page }) => {
    await openChat(page, groupChatId);
    await openSidebarSection(page, 'Participants');
    const group = turnMode(page);
    await expect(group).toBeVisible();
    await expect(radio(page, 'Normal')).toHaveAttribute('aria-checked', 'true');
    await expect(radio(page, 'Paused')).toHaveAttribute('aria-checked', 'false');
    await expect(radio(page, 'Storyteller')).toHaveAttribute('aria-checked', 'false');
    await expect(radio(page, 'Normal')).toHaveAttribute('title', 'The company takes turns on its own');
    await expect(radio(page, 'Paused')).toHaveAttribute('title', 'Nobody answers until you nudge them');
    await expect(radio(page, 'Storyteller')).toHaveAttribute(
      'title',
      'Each of your posts asks the company who wishes to answer; you choose',
    );
    await expect(page.locator('qt-chat-sidebar .qt-chat-pause-button')).toHaveCount(0);
    await expect(page.getByText('Invite by talkativeness')).toHaveCount(0);
    await page.getByRole('button', { name: 'Collapse chat sidebar' }).click();
    await expect(page.getByRole('button', { name: 'Pause auto-responses' })).toBeVisible();
    await page.getByRole('button', { name: 'Expand chat sidebar' }).click();
  });

  test('(H) Paused pauses with its toast and the banner; Normal resumes', async ({ page }) => {
    await openChat(page, groupChatId);
    await openSidebarSection(page, 'Participants');
    await radio(page, 'Paused').click();
    await expect(radio(page, 'Paused')).toHaveAttribute('aria-checked', 'true');
    await expect(page.getByText('Auto-responses paused', { exact: true })).toBeVisible();
    await expect(page.locator('.qt-chat-paused-banner')).toBeVisible();
    await radio(page, 'Normal').click();
    await expect(page.getByText('Auto-responses resumed', { exact: true })).toBeVisible();
    await expect(radio(page, 'Normal')).toHaveAttribute('aria-checked', 'true');
    await expect(page.locator('.qt-chat-paused-banner')).toHaveCount(0);
  });

  test('(I) Storyteller on a server that drops turnMode: the toast, then the radio SNAPS BACK and no board mounts', async ({ page }) => {
    test.skip(P4D343_SERVER_LANDED, 'the turnMode sibling persists on a landed server — (A) supersedes this');
    await openChat(page, groupChatId);
    await openSidebarSection(page, 'Participants');
    await radio(page, 'Storyteller').click();
    await expect(page.getByText(STORYTELLER_TOAST)).toBeVisible();
    // The refetch carries no `turnMode` key → `turnModeOf` reads rotation.
    await expect(radio(page, 'Normal')).toHaveAttribute('aria-checked', 'true');
    await expect(radio(page, 'Storyteller')).toHaveAttribute('aria-checked', 'false');
    await expect(page.locator(BOARD)).toHaveCount(0);
    await expect(page.locator('qt-storyteller-board')).toHaveCount(0);
  });

  test('(J) the Inspector offers Storyteller Rounds third', async ({ page }) => {
    await openChat(page, groupChatId);
    await page.getByRole('button', { name: 'Toggle LLM Inspector' }).click();
    const panel = page.locator('.qt-slide-over-panel');
    await expect(panel).toHaveAttribute('data-open', 'true');
    const options = panel.getByLabel('Filter log entries').locator('option');
    await expect(options.nth(2)).toHaveText('Storyteller Rounds');
    await expect(options.nth(2)).toHaveAttribute('value', 'storyteller');
  });

  // ---- gated -------------------------------------------------------------

  test('(A) Storyteller mode on; a post fills the board with bids', async ({ page }) => {
    test.skip(!P4D343_SERVER_LANDED, GATE_REASON);
    test.setTimeout(120_000);
    plan.delayMs = 150;
    await openChat(page, groupChatId);
    await openSidebarSection(page, 'Participants');
    await radio(page, 'Storyteller').click();
    await expect(radio(page, 'Storyteller')).toHaveAttribute('aria-checked', 'true');
    await expect(page.getByText(STORYTELLER_TOAST)).toBeVisible();
    await expect(page.getByText('Invite by talkativeness')).toBeVisible();
    await send(page, 'Who would like some tea?');
    const board = page.locator(BOARD);
    await expect(board).toBeVisible({ timeout: 30_000 });
    await expect(board).toContainText('The company bids');
    await expect(board.getByText('Considering…').first()).toBeVisible({ timeout: 15_000 });
    await expect(board.locator('.qt-badge', { hasText: 'Simple' }).first()).toBeVisible({ timeout: 30_000 });
    await expect(board.getByText(BID_PLAN).first()).toBeVisible();
    await expect(board.getByText('Considering…')).toHaveCount(0, { timeout: 30_000 });
    expect(plan.bidCalls).toBeGreaterThan(0);
  });

  test('(B) tick two, Run in order — two replies, then the summary in run order', async ({ page }) => {
    test.skip(!P4D343_SERVER_LANDED, GATE_REASON);
    test.setTimeout(120_000);
    plan.delayMs = 0;
    await openChat(page, groupChatId);
    const board = page.locator(BOARD);
    await expect(board).toBeVisible({ timeout: 30_000 });
    const boxes = board.locator('input[type="checkbox"]');
    await expect(boxes.nth(1)).toBeVisible();
    const names = [
      ((await boxes.nth(0).getAttribute('aria-label')) ?? '').replace(/^Let | go ahead$/g, ''),
      ((await boxes.nth(1).getAttribute('aria-label')) ?? '').replace(/^Let | go ahead$/g, ''),
    ];
    await page.getByLabel(`Let ${names[0]} go ahead`).check();
    await page.getByLabel(`Let ${names[1]} go ahead`).check();
    await board.getByRole('button', { name: 'Run in order' }).click();
    const replies = page.locator('.qt-chat-message', { hasText: TURN_REPLY });
    await expect(replies).toHaveCount(2, { timeout: 60_000 });
    // Untouched (not dragged), the cards run in the ROOM's rotation — drawn by
    // the server (`keepOrder: false`, storyteller_rounds.rs `rotation_order`),
    // not the board's order — so "run order" is the order the two replies
    // were posted in, and the summary must name exactly that.
    // (The author sits in the row's header, a sibling of the bubble.)
    const ran = await page
      .locator('.qt-chat-message-row', { has: page.locator('.qt-chat-message', { hasText: TURN_REPLY }) })
      .locator('.qt-chat-message-author')
      .allTextContents();
    expect([...ran].map((n) => n.trim()).sort()).toEqual([...names].sort());
    await expect(page.locator('.qt-storyteller-summary')).toHaveText(
      `Last round: ${ran.map((n) => n.trim()).join(', ')}`,
      { timeout: 30_000 },
    );
  });

  test('(C) a fresh post, then Let it lie', async ({ page }) => {
    test.skip(!P4D343_SERVER_LANDED, GATE_REASON);
    test.setTimeout(120_000);
    await openChat(page, groupChatId);
    await send(page, 'Or shall we leave it?');
    const board = page.locator(BOARD);
    await expect(board.getByText('Considering…')).toHaveCount(0, { timeout: 30_000 });
    await board.getByRole('button', { name: 'Let it lie' }).click();
    await expect(page.locator('.qt-storyteller-summary')).toHaveText('The last round was let lie.', {
      timeout: 15_000,
    });
  });

  test('(D) Normal with a round open folds the board away', async ({ page }) => {
    test.skip(!P4D343_SERVER_LANDED, GATE_REASON);
    test.setTimeout(120_000);
    await openChat(page, groupChatId);
    await send(page, 'One more question for the company.');
    await expect(page.locator(BOARD)).toBeVisible({ timeout: 30_000 });
    await openSidebarSection(page, 'Participants');
    await radio(page, 'Normal').click();
    await expect(page.locator('qt-storyteller-board')).toHaveCount(0, { timeout: 15_000 });
    await radio(page, 'Storyteller').click();
    await expect(radio(page, 'Storyteller')).toHaveAttribute('aria-checked', 'true');
  });

  test('(E) the invite checkbox round-trips through a reload', async ({ page }) => {
    test.skip(!P4D343_SERVER_LANDED, GATE_REASON);
    await openChat(page, groupChatId);
    await openSidebarSection(page, 'Participants');
    const box = page.getByLabel('Invite by talkativeness');
    const before = await box.isChecked();
    await box.click();
    await expect(box).toBeChecked({ checked: !before });
    await page.reload();
    await openSidebarSection(page, 'Participants');
    await expect(page.getByLabel('Invite by talkativeness')).toBeChecked({ checked: !before });
    await page.getByLabel('Invite by talkativeness').click();
  });

  test('(F2) the Inspector lists the bids under Storyteller Rounds, each with a round chip', async ({ page }) => {
    test.skip(!P4D343_SERVER_LANDED, GATE_REASON);
    await openChat(page, groupChatId);
    await page.getByRole('button', { name: 'Toggle LLM Inspector' }).click();
    const panel = page.locator('.qt-slide-over-panel');
    await panel.getByLabel('Filter log entries').selectOption('storyteller');
    const bids = panel.locator('.qt-inspector-entry', { hasText: 'Bid' });
    // The panel slides in and reads the logs asynchronously: wait for the
    // second bid row before counting (an immediate count read the empty panel).
    await expect(bids.nth(1)).toBeVisible({ timeout: 15_000 });
    expect(await bids.count()).toBeGreaterThanOrEqual(2);
    await expect(bids.first().getByText(/^round [0-9a-f-]{6}$/)).toBeVisible();
  });

  test('(K) All at once — two bubbles, then the summary with (all at once)', async ({ page }) => {
    test.skip(!P4D343_SERVER_LANDED, GATE_REASON);
    // Skipped BY NAME, not by gate: P4.D343 item 41 is DEFERRED (`order:
    // simultaneous` is Tier 2) — the server refuses it before any frame
    // (`storyteller_spine.rs`, 422 `not_available: storyteller simultaneous`;
    // measured at the 236c04e35 unification: the SPA toasts that raw string).
    test.skip(true, 'P4.D343 item 41 DEFERRED — the server refuses order: simultaneous');
    test.setTimeout(120_000);
    await openChat(page, groupChatId);
    await send(page, 'Everyone at once, then.');
    const board = page.locator(BOARD);
    await expect(board.getByText('Considering…')).toHaveCount(0, { timeout: 30_000 });
    const boxes = board.locator('input[type="checkbox"]');
    const names = [
      ((await boxes.nth(0).getAttribute('aria-label')) ?? '').replace(/^Let | go ahead$/g, ''),
      ((await boxes.nth(1).getAttribute('aria-label')) ?? '').replace(/^Let | go ahead$/g, ''),
    ];
    await boxes.nth(0).check();
    await boxes.nth(1).check();
    await board.getByRole('button', { name: 'All at once' }).click();
    await expect(page.locator('.qt-storyteller-summary')).toContainText(' (all at once)', { timeout: 60_000 });
    await expect(page.locator('.qt-storyteller-summary')).toContainText(names[0]);
  });
});

function turnMode(page: Page) {
  return page.getByRole('radiogroup', { name: 'Turn mode' });
}
function radio(page: Page, name: 'Normal' | 'Paused' | 'Storyteller') {
  return turnMode(page).getByRole('radio', { name });
}

async function openChat(page: Page, chatId: string): Promise<void> {
  await dispatchRaw({ type: 'unlock', passphrase: E2E_PASSPHRASE });
  await page.goto(`${ST_BASE_URL}/salon/${chatId}`);
  const passphrase = page.locator('#qt-passphrase');
  const messages = page.locator('.qt-chat-messages-list');
  await expect(passphrase.or(messages).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
  await expect(messages).toBeVisible({ timeout: 15_000 });
}

async function send(page: Page, text: string): Promise<void> {
  const composer = page.locator('.qt-chat-composer-input .qt-rich-editor-content');
  await composer.click();
  await page.keyboard.type(text);
  await page.keyboard.press('Enter');
}

function runCliWrite(cli: string, sql: string): void {
  const res = spawnSync(cli, ['db', '--data-dir', ST_INSTANCE_DIR, '--write', sql], {
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
      const res = await fetch(`${ST_BASE_URL}/health`);
      // 423 = up, instance locked (the server gets no passphrase env).
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${ST_PORT}: ${lastErr}`);
}
