import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, openSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { expect, test, type Locator, type Page } from './support/fixtures';

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
import { startMockLlm, type MockLlm } from './support/mock-llm';

/**
 * P4.D353 — diagrams in the Salon, the client half (v4 `6588972d5` +
 * `9faed0c7e`): a closed ```mermaid / ```dot fence in a settled Salon message
 * draws as a picture through the REAL engines in Chromium — the only proof
 * that Mermaid, the WASM Graphviz + rough.js sketch, the PNG rasterizer and
 * the clipboard work in the shipped bundle (jsdom can run none of them).
 *
 * Its own instance on 4356 and its own mock on 45354 (the round's §R.9
 * ports), from the `salon-interrupt-flow` recipe: the salon pair, the userId
 * rewrite, `connection_profiles.baseUrl` aimed at the mock, the
 * `instance_settings` plant. Four assistant rows are seeded into Solo Voyage
 * under Aria's seat: a mermaid flowchart, a dot graph, a mermaid fence that
 * does not parse, and a row whose REASONING carries a mermaid fence (the
 * thinking block is a `qt-message-content` rendered without `diagrams` — the
 * non-Salon-surface beat, reachable without leaving the chat).
 *
 * **UNGATED beats (1)–(6), (8)** need no server change: rendering a diagram
 * writes nothing. Aria carries no `diagramStyle` on main's wire, so her
 * drawings take the defensive default (`handDrawn`, v4's own default).
 *
 * **GATED beats (7)/(7b)** need P4.D349 (the `canDrawDiagrams` /
 * `diagramStyle` keys on the character PUT and the C6 wire fields).
 * ACTIVATE-AT-UNIFY (§S.9): the unifier flips `P4D349_SERVER_LANDED` to
 * `true`. Against an unported server the card's dispatch is a no-op, so the
 * toast never fires and the style stays `handDrawn`.
 */
const P4D349_SERVER_LANDED = true;
const GATE_REASON = 'awaits P4.D349’s canDrawDiagrams / diagramStyle character keys and the C6 wire (wired at unification)';

const DG_PORT = 4356;
const DG_BASE_URL = `http://127.0.0.1:${DG_PORT}`;
const DG_MOCK_PORT = 45354;
const DG_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'diagrams-instance');
const DG_DATA_DIR = resolve(DG_INSTANCE_DIR, 'data');
const DG_SERVER_LOG = resolve(ARTIFACTS_DIR, 'diagrams-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'salon-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'salon-mount.db');
const USER_TABLES = ['characters', 'chats', 'tags', 'groups', 'projects', 'files'];

const MERMAID_ROW = 'Diagram the first:\n\n```mermaid\ngraph TD; A-->B\n```';
const DOT_ROW = 'Diagram the second:\n\n```dot\ndigraph { a -> b }\n```';
const BAD_ROW = 'Diagram the third:\n\n```mermaid\ngraph TD; A--\n```';
const THINKING_ROW = 'Diagram the fourth, thought through.';
const THINKING_REASONING = '```mermaid\ngraph TD; X-->Y\n```';
/** (6): the fence closes early; the words after it keep the stream open. */
const STREAM_REPLY =
  'A sketch:\n\n```mermaid\ngraph LR; S-->T\n```\n\nand a good many words after it, so the stream stays open while the bubble is looked at closely by the beat';

let server: ChildProcess | undefined;
let mock: MockLlm | undefined;
let soloChatId = '';

async function dispatchRaw(req: unknown): Promise<{ type?: string; data?: Record<string, unknown> }> {
  const res = await fetch(`${DG_BASE_URL}/api/dispatch`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(req),
  });
  return ((await res.json().catch(() => null)) as { type?: string; data?: Record<string, unknown> } | null) ?? {};
}

function sqlString(s: string): string {
  return `'${s.replace(/'/g, "''")}'`;
}

/** One assistant row in Solo Voyage under Aria's seat (read from the chat's own participants). */
function seedAriaRow(cli: string, id: string, content: string, createdAt: string, reasoning?: string): void {
  runCliWrite(
    cli,
    `INSERT INTO chat_messages (id, chatId, type, role, content, reasoningContent, participantId, createdAt) ` +
      `SELECT ${sqlString(id)}, c.id, 'message', 'ASSISTANT', ${sqlString(content)}, ` +
      `${reasoning === undefined ? 'NULL' : sqlString(reasoning)}, ` +
      `(SELECT json_extract(p.value, '$.id') FROM json_each(c.participants) p ` +
      `JOIN characters ch ON ch.id = json_extract(p.value, '$.characterId') WHERE ch.name = 'Aria' LIMIT 1), ` +
      `${sqlString(createdAt)} FROM chats c WHERE c.title = 'Solo Voyage';`,
  );
}

test.describe('P4.D353 — diagrams in the Salon', () => {
  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(DG_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(DG_DATA_DIR, { recursive: true });
    mkdirSync(resolve(DG_DATA_DIR, 'files'), { recursive: true });
    copyFileSync(MAIN_FIXTURE, resolve(DG_DATA_DIR, 'quilltap.db'));
    if (existsSync(MOUNT_FIXTURE)) {
      copyFileSync(MOUNT_FIXTURE, resolve(DG_DATA_DIR, 'quilltap-mount-index.db'));
    }
    writeFileSync(resolve(DG_DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));
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
      `UPDATE connection_profiles SET baseUrl = 'http://127.0.0.1:${DG_MOCK_PORT}/v1' WHERE provider = 'OPENAI_COMPATIBLE';`,
    );
    // Solo Voyage seats Aria AND the user's own character; make it single-
    // character so a send gets exactly one streamed reply (beat 6).
    runCliWrite(
      cli,
      `UPDATE chats SET participants = (SELECT json_group_array(CASE WHEN json_extract(value, '$.controlledBy') = 'user' THEN json_set(value, '$.isActive', json('false')) ELSE value END) FROM json_each(chats.participants)) WHERE title = 'Solo Voyage';`,
    );
    // Fresh UUIDv4 ids (a non-UUID id is dropped by the message read).
    seedAriaRow(cli, '8f0d3a52-6b1e-4c7a-9d42-3e5b7c1a9f01', MERMAID_ROW, '2026-02-02T00:00:01.000Z');
    seedAriaRow(cli, '8f0d3a52-6b1e-4c7a-9d42-3e5b7c1a9f02', DOT_ROW, '2026-02-02T00:00:02.000Z');
    seedAriaRow(cli, '8f0d3a52-6b1e-4c7a-9d42-3e5b7c1a9f03', BAD_ROW, '2026-02-02T00:00:03.000Z');
    seedAriaRow(
      cli,
      '8f0d3a52-6b1e-4c7a-9d42-3e5b7c1a9f04',
      THINKING_ROW,
      '2026-02-02T00:00:04.000Z',
      THINKING_REASONING,
    );
    mock = await startMockLlm(STREAM_REPLY, DG_MOCK_PORT, 250);

    // The salon pair carries no `instance_settings` table; plant generateDDL's
    // own from the committed D23 dump (the scenario-browser-flow recipe).
    const freshSchema = JSON.parse(
      readFileSync(resolve(REPO_ROOT, 'crates/quilltap-core/src/services/provisioning/fresh_schema.json'), 'utf8'),
    ) as { main: string[] };
    const settingsDdl = freshSchema.main.find((sql) => sql.startsWith('CREATE TABLE "instance_settings"'));
    if (!settingsDdl) throw new Error('fresh_schema.json carries no instance_settings CREATE TABLE');
    const ddl = spawnSync(cli, ['db', '--data-dir', DG_INSTANCE_DIR, '--write', settingsDdl], {
      env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
      encoding: 'utf8',
    });
    if (ddl.status !== 0 && !`${ddl.stdout}${ddl.stderr}`.includes('already exists')) {
      throw new Error(`instance_settings plant failed:\n${ddl.stdout}\n${ddl.stderr}`);
    }

    const logFd = openSync(DG_SERVER_LOG, 'w');
    server = spawn(
      web,
      ['--host', '127.0.0.1', '--port', String(DG_PORT), '--data-dir', DG_INSTANCE_DIR, '--spa-dir', spaDir()],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    server.unref();
    await waitForHealth();

    await dispatchRaw({ type: 'unlock', passphrase: E2E_PASSPHRASE });
    const chats = ((await dispatchRaw({ type: 'listChats' })).data as unknown as { id: string; title: string }[]) ?? [];
    soloChatId = chats.find((c) => c.title === 'Solo Voyage')?.id ?? '';
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
    await mock?.close();
    rmSync(DG_INSTANCE_DIR, { recursive: true, force: true });
  });

  // ---- ungated ---------------------------------------------------------

  test('(1) a mermaid fence draws with real Mermaid, hand-drawn by default', async ({ page }) => {
    await openChat(page, soloChatId);
    const block = row(page, 'Diagram the first').locator('.qt-diagram');
    await expect(block).toHaveAttribute('data-diagram-engine', 'mermaid');
    await expect(block).toHaveAttribute('data-diagram-style', 'handDrawn');
    await expect(block.locator('.qt-diagram-canvas svg')).toBeVisible({ timeout: 30_000 });
    await expect(block.getByRole('toolbar', { name: 'Diagram actions' })).toBeVisible();
    // The fence's code block is gone from the drawn state.
    await expect(row(page, 'Diagram the first').locator('pre > code.language-mermaid')).toHaveCount(0);
  });

  test('(2) a dot fence draws with the WASM Graphviz, sketched by hand', async ({ page }) => {
    await openChat(page, soloChatId);
    const block = row(page, 'Diagram the second').locator('.qt-diagram');
    await expect(block).toHaveAttribute('data-diagram-engine', 'graphviz');
    await expect(block).toHaveAttribute('data-diagram-style', 'handDrawn');
    await expect(block.locator('.qt-diagram-canvas svg')).toBeVisible({ timeout: 30_000 });
  });

  test('(3) a fence that does not parse shows the blot note, the engine’s words and the source', async ({ page }) => {
    await openChat(page, soloChatId);
    const error = row(page, 'Diagram the third').locator('.qt-diagram-error');
    await expect(error).toBeVisible({ timeout: 30_000 });
    await expect(error.locator('.qt-diagram-error-note')).toHaveText(
      'The draughtsman’s pen has blotted — the diagram below could not be drawn.',
    );
    await expect(error.locator('details summary')).toHaveText('What the Mermaid engine said');
    await expect(error.locator('pre > code.language-mermaid')).toContainText('graph TD; A--');
  });

  test('(4) the lightbox: zoom, Fit, and Escape back to the same place with focus on the drawing', async ({ page }) => {
    await openChat(page, soloChatId);
    const canvas = row(page, 'Diagram the first').locator('.qt-diagram-canvas');
    await expect(canvas.locator('svg')).toBeVisible({ timeout: 30_000 });
    await canvas.scrollIntoViewIfNeeded();
    const before = await scrollTopOf(canvas);

    await canvas.click();
    const dialog = page.getByRole('dialog', { name: 'Diagram, enlarged' });
    await expect(dialog).toBeVisible();
    const readout = dialog.locator('.qt-diagram-lightbox-zoom');
    const fitPercent = parseInt((await readout.textContent()) ?? '0', 10);
    await expect(dialog.getByRole('button', { name: 'Fit' })).toHaveAttribute('aria-pressed', 'true');
    await page.keyboard.press('+');
    await expect.poll(async () => parseInt((await readout.textContent()) ?? '0', 10)).toBeGreaterThan(fitPercent);
    await expect(dialog.getByRole('button', { name: 'Fit' })).toHaveAttribute('aria-pressed', 'false');
    await dialog.getByRole('button', { name: 'Fit' }).click();
    await expect(readout).toHaveText(`${fitPercent}%`);

    await page.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);
    expect(Math.abs((await scrollTopOf(canvas)) - before)).toBeLessThanOrEqual(1);
    await expect(canvas).toBeFocused();
    // The Salon behind never heard the key: still on the chat, nothing left open.
    await expect(page).toHaveURL(new RegExp(`/salon/${soloChatId}`));
  });

  test('(5) Copy PNG puts an image/png on the clipboard', async ({ page }) => {
    await page.context().grantPermissions(['clipboard-read', 'clipboard-write'], { origin: DG_BASE_URL });
    await openChat(page, soloChatId);
    const canvas = row(page, 'Diagram the first').locator('.qt-diagram-canvas');
    await expect(canvas.locator('svg')).toBeVisible({ timeout: 30_000 });
    await row(page, 'Diagram the first').getByRole('button', { name: 'Enlarge' }).click();
    const dialog = page.getByRole('dialog', { name: 'Diagram, enlarged' });
    await dialog.getByRole('button', { name: /Copy PNG/ }).click();
    await expect(dialog.getByRole('button', { name: /Copied/ })).toBeVisible({ timeout: 15_000 });
    const types = await page.evaluate(async () => {
      const items = await navigator.clipboard.read();
      return items.flatMap((i) => [...i.types]);
    });
    expect(types).toContain('image/png');
    await page.keyboard.press('Escape');
  });

  test('(5b) Copy PNG without the clipboard permission raises v4’s clipboard toast', async ({ page }) => {
    const errors: string[] = [];
    page.on('console', (m) => {
      if (m.type() === 'error' && m.text().includes('Failed to copy diagram as PNG')) errors.push(m.text());
    });
    await page.context().clearPermissions();
    await openChat(page, soloChatId);
    await expect(row(page, 'Diagram the first').locator('.qt-diagram-canvas svg')).toBeVisible({ timeout: 30_000 });
    await row(page, 'Diagram the first').getByRole('button', { name: 'Enlarge' }).click();
    const dialog = page.getByRole('dialog', { name: 'Diagram, enlarged' });
    await dialog.getByRole('button', { name: /Copy PNG/ }).click();
    await expect(page.getByText('The picture could not be put on the clipboard.')).toBeVisible({ timeout: 15_000 });
    await expect(dialog.getByRole('button', { name: /Copy PNG/ })).toBeEnabled();
    expect(errors.length).toBeGreaterThan(0);
    await page.keyboard.press('Escape');
  });

  test('(6) a streaming reply keeps its fence as code; the settled row draws it', async ({ page }) => {
    test.setTimeout(120_000);
    await openChat(page, soloChatId);
    await send(page, 'Sketch me something.');
    const live = page.locator('qt-streaming-message');
    await expect(live.locator('pre > code.language-mermaid')).toContainText('graph LR; S-->T', { timeout: 30_000 });
    await expect(live.locator('qt-diagram-block')).toHaveCount(0);
    const settled = row(page, 'and a good many words after it');
    await expect(settled.locator('.qt-diagram-canvas svg')).toBeVisible({ timeout: 60_000 });
    await expect(live).toHaveCount(0);
  });

  test('(8) a surface rendered without diagrams (the thinking block) keeps the code block', async ({ page }) => {
    await openChat(page, soloChatId);
    const thinking = row(page, THINKING_ROW).locator('.qt-chat-thinking-details');
    await expect(thinking).toHaveCount(1);
    await expect(thinking.locator('pre > code.language-mermaid')).toHaveCount(1);
    await expect(thinking.locator('qt-diagram-block')).toHaveCount(0);
  });

  test('(9) a live theme switch re-draws the visible diagram in the new palette', async ({ page }) => {
    // The server's themePreference is the source of truth the shell syncs to,
    // so show the nav theme switcher there (localStorage alone is overridden).
    await dispatchRaw({ type: 'unlock', passphrase: E2E_PASSPHRASE });
    await dispatchRaw({
      type: 'chatSettingsUpdate',
      settings: { themePreference: { activeThemeId: null, colorMode: 'light', showNavThemeSelector: true } },
    });
    await openChat(page, soloChatId);
    const svg = row(page, 'Diagram the first').locator('.qt-diagram-canvas svg');
    await expect(svg).toBeVisible({ timeout: 30_000 });
    // The switcher's menu stays open across picks: open it once.
    await page.getByRole('button', { name: 'Themes' }).click();
    const pick = (mode: string) => page.getByRole('menuitemradio', { name: mode }).click();
    await pick('Light');
    await expect(svg).toBeVisible({ timeout: 30_000 });
    const light = await svg.evaluate((el) => el.outerHTML);
    await pick('Dark');
    await expect.poll(() => svg.evaluate((el) => el.outerHTML), { timeout: 30_000 }).not.toBe(light);
    // Back to Light for whatever runs next on this instance.
    await pick('Light');
    await expect.poll(() => svg.evaluate((el) => el.outerHTML), { timeout: 30_000 }).toBe(light);
  });

  // ---- gated: P4.D349 ----------------------------------------------------

  test.describe('the Aurora card (needs P4.D349)', () => {
    test.skip(!P4D349_SERVER_LANDED, GATE_REASON);

    test('(7) the Details tab card toggles, Clean saves with its toast, and persists', async ({ page }) => {
      const ariaId = await characterId('Aria');
      await page.goto(`${DG_BASE_URL}/characters/${ariaId}/edit`);
      await unlockIfAsked(page);
      const card = page.locator('[data-testid="can-draw-diagrams"]');
      await expect(card).toBeVisible({ timeout: 15_000 });
      const box = card.getByRole('checkbox', { name: /Draws diagrams/ });
      if (await box.isChecked()) {
        await box.uncheck();
        await expect(page.getByText('The pen is put away — this character will draw no more diagrams')).toBeVisible();
      }
      await expect(card.getByRole('radio', { name: /Clean/ })).toBeDisabled();
      await box.check();
      await expect(page.getByText('The pen is theirs — this character may now draw diagrams')).toBeVisible();
      await card.getByRole('radio', { name: /Clean/ }).check();
      await expect(page.getByText('Their diagrams will be drawn clean and ruled')).toBeVisible();
      await page.reload();
      await unlockIfAsked(page);
      await expect(page.locator('[data-testid="can-draw-diagrams"]').getByRole('radio', { name: /Clean/ })).toBeChecked({
        timeout: 15_000,
      });
    });

    test('(7b) after Clean, the seeded mermaid row re-renders clean (C6 on the chat wire)', async ({ page }) => {
      await openChat(page, soloChatId);
      await expect(row(page, 'Diagram the first').locator('.qt-diagram')).toHaveAttribute(
        'data-diagram-style',
        'clean',
        { timeout: 30_000 },
      );
    });
  });
});

function row(page: Page, text: string): Locator {
  return page.locator('qt-message-row').filter({ hasText: text });
}

/** The scrollTop of the drawing's nearest vertically-scrolling ancestor. */
function scrollTopOf(canvas: Locator): Promise<number> {
  return canvas.evaluate((el) => {
    for (let node = el.parentElement; node && node !== document.body; node = node.parentElement) {
      const { overflowY } = getComputedStyle(node);
      if ((overflowY === 'auto' || overflowY === 'scroll') && node.scrollHeight > node.clientHeight) {
        return node.scrollTop;
      }
    }
    return window.scrollY;
  });
}

async function characterId(name: string): Promise<string> {
  const data = (await dispatchRaw({ type: 'characterList' })).data ?? {};
  const list = (data['characters'] as { id: string; name: string }[] | undefined) ?? [];
  const id = list.find((c) => c.name === name)?.id ?? '';
  expect(id, `${name} on the pair`).toBeTruthy();
  return id;
}

async function unlockIfAsked(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  if (await passphrase.isVisible().catch(() => false)) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

async function openChat(page: Page, chatId: string): Promise<void> {
  await dispatchRaw({ type: 'unlock', passphrase: E2E_PASSPHRASE });
  await page.goto(`${DG_BASE_URL}/salon/${chatId}`);
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
  const res = spawnSync(cli, ['db', '--data-dir', DG_INSTANCE_DIR, '--write', sql], {
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
      const res = await fetch(`${DG_BASE_URL}/health`);
      // 423 = up, instance locked (the server gets no passphrase env).
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${DG_PORT}: ${lastErr}`);
}
