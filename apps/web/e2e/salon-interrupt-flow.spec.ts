import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, openSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer, type Server } from 'node:http';
import { resolve } from 'node:path';

import { expect, test, type Page } from './support/fixtures';

import { makeDbKeyFile } from './support/dbkey';
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
 * P4.D336 — raise a hand / stop, the SPA half (v4 `2b5e8ee24` + `b8362beb5`):
 * the composer's always-rendered trio (Send, an amber raised hand, a red stop
 * sign) stacked in `.qt-chat-composer-actions`, `stop()` as v4's server-first
 * `stopStreaming`, `toggleRaisedHand()`, and the participant card's Stop gone.
 *
 * Its own instance on port 4350 and its own mock on 45348 (the round's §R.9
 * ports), copied from the `scenario-browser-flow` recipe: the salon pair, the
 * userId rewrite, `connection_profiles.baseUrl` aimed at the mock, the
 * `instance_settings` plant.
 *
 * **The mock is SPEC-LOCAL and SEQUENCED** (R-10; `e2e/support/mock-llm.ts` is
 * not this lane's): `startMockLlm` answers ONE fixed reply per server, and the
 * raise-hand beat needs the SECOND speaker's reply to be `[YIELD]` while the
 * first streams prose slowly. `createServer` answers `GET /models` with a model
 * list and `POST /chat/completions` with SSE deltas (one per word, `plan.delayMs`
 * apart) then a final chunk with `usage` and `[DONE]`, taking its reply from
 * `plan.replies[callIndex]` (the last one repeating). A beat sets `plan` BEFORE
 * each send.
 *
 * **UNGATED beats (C)–(G)** hold on main's server: the trio at rest, enabled
 * mid-stream, the lone-character title, (F) Stop against a server that does not
 * know `chatStopTurn` (R-V's degradation, measured LIVE), and the participant
 * card showing `Speaking...` with no Stop.
 *
 * **GATED beats (A)/(B)** need P4.D335 (`chatStopTurn` / `chatRaiseHand`, the
 * `handAnswered` / `stopped` / `yielded` frames). ACTIVATE-AT-UNIFY (§S.9, the
 * `P4D279_SERVER_LANDED` precedent): flipped `true` at the `322a5488e`
 * unification. With the server landed, (F)'s premise (a refused dispatch) is
 * gone, so (F) skips itself; (A) supersedes it.
 */
const P4D335_SERVER_LANDED = true;
const GATE_REASON =
  'awaits P4.D335’s chatStopTurn / chatRaiseHand variants and the handAnswered / stopped / yielded frames (wired at unification)';

const INT_PORT = 4350;
const INT_BASE_URL = `http://127.0.0.1:${INT_PORT}`;
const INT_MOCK_PORT = 45348;
const INT_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'salon-interrupt-instance');
const INT_DATA_DIR = resolve(INT_INSTANCE_DIR, 'data');
const INT_SERVER_LOG = resolve(ARTIFACTS_DIR, 'salon-interrupt-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'salon-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'salon-mount.db');
const USER_TABLES = ['characters', 'chats', 'tags', 'groups', 'projects', 'files'];

const SLOW_REPLY = 'Splendid weather for it, though one does hope the kettle stays warm throughout';
const YIELD_REPLY = '[YIELD]';
const STOP_TOAST = 'Stopped. The floor is yours, and the room is paused.';
const HAND_TITLE_REST = 'Raise your hand (while someone is speaking)';
const STOP_TITLE_REST = 'Stop (while someone is speaking)';
const HAND_TITLE_LIVE = 'Raise your hand — the next speaker may yield the floor to you';
const HAND_TITLE_SINGLE = 'Only one character is present — the floor returns to you after this reply';
const HAND_TITLE_UP = 'Hand raised — the next speaker will be asked to yield. Click to lower it.';

/** What the spec-local mock answers; a beat sets it before each send. */
const plan: { replies: string[]; delayMs: number; calls: number } = {
  replies: [SLOW_REPLY],
  delayMs: 350,
  calls: 0,
};

let server: ChildProcess | undefined;
let mock: Server | undefined;
let groupChatId = '';
let soloChatId = '';

function startSequencedMock(port: number): Promise<Server> {
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
    req.on('data', () => {});
    req.on('end', () => {
      const reply = plan.replies[Math.min(plan.calls, plan.replies.length - 1)] ?? SLOW_REPLY;
      plan.calls += 1;
      res.writeHead(200, {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        Connection: 'keep-alive',
      });
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
  const res = await fetch(`${INT_BASE_URL}/api/dispatch`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(req),
  });
  return ((await res.json().catch(() => null)) as { type?: string; data?: Record<string, unknown> } | null) ?? {};
}

test.describe('P4.D336 — raise a hand / stop', () => {
  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(INT_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(INT_DATA_DIR, { recursive: true });
    mkdirSync(resolve(INT_DATA_DIR, 'files'), { recursive: true });
    copyFileSync(MAIN_FIXTURE, resolve(INT_DATA_DIR, 'quilltap.db'));
    if (existsSync(MOUNT_FIXTURE)) {
      copyFileSync(MOUNT_FIXTURE, resolve(INT_DATA_DIR, 'quilltap-mount-index.db'));
    }
    writeFileSync(resolve(INT_DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));
    for (const table of [
      ...USER_TABLES,
      'connection_profiles',
      'api_keys',
      'chat_settings',
      'memories',
      'image_profiles',
    ]) {
      runCliWrite(cli, `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`);
    }
    runCliWrite(cli, `UPDATE users SET id = '${SINGLE_USER_ID}' WHERE id = '${FIXTURE_USER}';`);
    runCliWrite(
      cli,
      `UPDATE connection_profiles SET baseUrl = 'http://127.0.0.1:${INT_MOCK_PORT}/v1' WHERE provider = 'OPENAI_COMPATIBLE';`,
    );
    // Solo Voyage seats Aria AND the user's own character; v4's
    // `isSingleCharacterChat` counts user-controlled seats, so deactivate the
    // user's seat to make the chat genuinely single-character (beat E).
    runCliWrite(
      cli,
      `UPDATE chats SET participants = (SELECT json_group_array(CASE WHEN json_extract(value, '$.controlledBy') = 'user' THEN json_set(value, '$.isActive', json('false')) ELSE value END) FROM json_each(chats.participants)) WHERE title = 'Solo Voyage';`,
    );
    mock = await startSequencedMock(INT_MOCK_PORT);

    // The salon pair carries no `instance_settings` table; plant generateDDL's
    // own from the committed D23 dump (the scenario-browser-flow recipe).
    const freshSchema = JSON.parse(
      readFileSync(resolve(REPO_ROOT, 'crates/quilltap-core/src/services/provisioning/fresh_schema.json'), 'utf8'),
    ) as { main: string[] };
    const settingsDdl = freshSchema.main.find((sql) => sql.startsWith('CREATE TABLE "instance_settings"'));
    if (!settingsDdl) throw new Error('fresh_schema.json carries no instance_settings CREATE TABLE');
    const ddl = spawnSync(cli, ['db', '--data-dir', INT_INSTANCE_DIR, '--write', settingsDdl], {
      env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
      encoding: 'utf8',
    });
    if (ddl.status !== 0 && !`${ddl.stdout}${ddl.stderr}`.includes('already exists')) {
      throw new Error(`instance_settings plant failed:\n${ddl.stdout}\n${ddl.stderr}`);
    }

    const logFd = openSync(INT_SERVER_LOG, 'w');
    server = spawn(
      web,
      ['--host', '127.0.0.1', '--port', String(INT_PORT), '--data-dir', INT_INSTANCE_DIR, '--spa-dir', spaDir()],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    server.unref();
    await waitForHealth();

    await dispatchRaw({ type: 'unlock', passphrase: E2E_PASSPHRASE });
    const chats = ((await dispatchRaw({ type: 'listChats' })).data as unknown as { id: string; title: string }[]) ?? [];
    groupChatId = chats.find((c) => c.title === 'Group Expedition')?.id ?? '';
    soloChatId = chats.find((c) => c.title === 'Solo Voyage')?.id ?? '';
    expect(groupChatId, 'Group Expedition on the pair').toBeTruthy();
    expect(soloChatId, 'Solo Voyage on the pair').toBeTruthy();
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
    rmSync(INT_INSTANCE_DIR, { recursive: true, force: true });
  });

  // ---- ungated ---------------------------------------------------------

  test('(C) the trio renders in the actions column; both interrupts are disabled at rest', async ({ page }) => {
    await openChat(page, groupChatId);
    const col = page.locator('.qt-chat-composer-actions');
    await expect(col).toBeVisible();
    await expect(col.getByRole('button', { name: 'Send message' })).toBeVisible();
    await expect(hand(page)).toBeDisabled();
    await expect(hand(page)).toHaveAttribute('title', HAND_TITLE_REST);
    await expect(stopButton(page)).toBeDisabled();
    await expect(stopButton(page)).toHaveAttribute('title', STOP_TITLE_REST);
  });

  test('(D) mid-stream both interrupts open and Send is greyed and titled Generating...', async ({ page }) => {
    test.setTimeout(120_000);
    plan.replies = [SLOW_REPLY];
    plan.delayMs = 350;
    await openChat(page, groupChatId);
    await send(page, 'Is anyone about?');
    await expect(stopButton(page)).toBeEnabled({ timeout: 15_000 });
    await expect(hand(page)).toBeEnabled();
    await expect(hand(page)).toHaveAttribute('title', HAND_TITLE_LIVE);
    const sendBtn = page.getByRole('button', { name: 'Send message' });
    await expect(sendBtn).toBeDisabled();
    await expect(sendBtn).toHaveAttribute('title', 'Generating...');
    await expect(stopButton(page)).toBeDisabled({ timeout: 60_000 });
  });

  test('(E) a lone character: the hand is disabled with v4’s title mid-stream', async ({ page }) => {
    test.setTimeout(120_000);
    plan.replies = [SLOW_REPLY];
    plan.delayMs = 350;
    await openChat(page, soloChatId);
    await send(page, 'Just us, then?');
    await expect(stopButton(page)).toBeEnabled({ timeout: 15_000 });
    await expect(hand(page)).toBeDisabled();
    await expect(hand(page)).toHaveAttribute('title', HAND_TITLE_SINGLE);
    await expect(stopButton(page)).toBeDisabled({ timeout: 60_000 });
  });

  test('(F) Stop against a server without chatStopTurn degrades to the local clear', async ({ page }) => {
    test.skip(P4D335_SERVER_LANDED, 'the dispatch succeeds on a landed server — (A) supersedes this');
    plan.replies = [SLOW_REPLY];
    plan.delayMs = 350;
    const logged: string[] = [];
    page.on('console', (m) => {
      if (m.text().includes('[Salon] Stop request failed; abandoning the stream locally')) logged.push(m.text());
    });
    await openChat(page, groupChatId);
    await send(page, 'Hold that thought.');
    await expect(stopButton(page)).toBeEnabled({ timeout: 15_000 });
    await stopButton(page).click();
    await expect(page.locator('[role="toast-container"] .qt-toast-info', { hasText: STOP_TOAST })).toBeVisible({
      timeout: 10_000,
    });
    await expect(stopButton(page)).toBeDisabled({ timeout: 10_000 });
    expect(logged).toHaveLength(1);
  });

  test('(G) the participant card offers no Stop while a seat generates', async ({ page }) => {
    test.setTimeout(120_000);
    plan.replies = [SLOW_REPLY];
    plan.delayMs = 350;
    await openChat(page, groupChatId);
    const expand = page.getByRole('button', { name: 'Expand chat sidebar' });
    if (await expand.count()) await expand.click();
    await send(page, 'Who has the floor?');
    await expect(stopButton(page)).toBeEnabled({ timeout: 15_000 });
    await expect(page.getByRole('button', { name: /stop generating/i })).toHaveCount(0);
    await expect(page.locator('qt-chat-sidebar').getByRole('button', { name: /^Stop/ })).toHaveCount(0);
    await expect(stopButton(page)).toBeDisabled({ timeout: 60_000 });
  });

  test('phone width: the three buttons stack right of the editor', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 800 });
    await openChat(page, groupChatId);
    const boxes = [
      await page.getByRole('button', { name: 'Send message' }).boundingBox(),
      await hand(page).boundingBox(),
      await stopButton(page).boundingBox(),
    ];
    const [a, b, c] = boxes;
    expect(a && b && c).toBeTruthy();
    expect(b!.y).toBeGreaterThan(a!.y);
    expect(c!.y).toBeGreaterThan(b!.y);
    expect(Math.abs(a!.x - b!.x)).toBeLessThan(2);
    expect(Math.abs(b!.x - c!.x)).toBeLessThan(2);
    const editor = await page.locator('.qt-chat-composer-input .qt-rich-editor-content').boundingBox();
    expect(editor).toBeTruthy();
    expect(a!.x).toBeGreaterThan(editor!.x);
  });

  // ---- gated (P4.D335) ---------------------------------------------------

  test('(A) Stop mid-stream keeps the prose, pauses the room, one toast', async ({ page }) => {
    test.skip(!P4D335_SERVER_LANDED, GATE_REASON);
    plan.replies = [SLOW_REPLY];
    plan.delayMs = 350;
    await openChat(page, groupChatId);
    await send(page, 'Pray continue.');
    await expect(stopButton(page)).toBeEnabled({ timeout: 15_000 });
    await expect(page.getByText('Splendid weather').first()).toBeVisible({ timeout: 15_000 });
    await stopButton(page).click();
    const infos = page.locator('[role="toast-container"] .qt-toast-info');
    await expect(infos.filter({ hasText: STOP_TOAST })).toHaveCount(1, { timeout: 15_000 });
    await expect(page.getByText('Auto-responses are paused')).toHaveCount(0);
    // The partial bubble persists after the refetch; the full reply does not.
    await expect(page.getByText('Splendid weather').first()).toBeVisible();
    await expect(page.getByText(SLOW_REPLY)).toHaveCount(0);
  });

  test('(B) a raised hand is answered [YIELD] by the next speaker', async ({ page }) => {
    test.skip(!P4D335_SERVER_LANDED, GATE_REASON);
    plan.replies = [SLOW_REPLY, YIELD_REPLY];
    plan.delayMs = 350;
    plan.calls = 0;
    await openChat(page, groupChatId);
    await send(page, 'Who speaks next?');
    await expect(hand(page)).toBeEnabled({ timeout: 15_000 });
    await hand(page).click();
    await expect(hand(page)).toHaveAttribute('aria-pressed', 'true');
    await expect(hand(page)).toHaveClass(/qt-chat-interrupt-button-raised/);
    await expect(hand(page)).toHaveAttribute('title', HAND_TITLE_UP);
    await expect(page.getByText('yields the floor to').first()).toBeVisible({ timeout: 60_000 });
    await expect(hand(page)).toHaveAttribute('aria-pressed', 'false');
    await expect(page.getByText('carried on regardless')).toHaveCount(0);
  });
});

function hand(page: Page) {
  return page.locator('.qt-chat-composer-actions .qt-chat-interrupt-button');
}
function stopButton(page: Page) {
  return page.locator('.qt-chat-composer-actions .qt-chat-stop-button');
}

async function openChat(page: Page, chatId: string): Promise<void> {
  await dispatchRaw({ type: 'unlock', passphrase: E2E_PASSPHRASE });
  await page.goto(`${INT_BASE_URL}/salon/${chatId}`);
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
  const res = spawnSync(cli, ['db', '--data-dir', INT_INSTANCE_DIR, '--write', sql], {
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
      const res = await fetch(`${INT_BASE_URL}/health`);
      // 423 = up, instance locked (the server gets no passphrase env).
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${INT_PORT}: ${lastErr}`);
}
