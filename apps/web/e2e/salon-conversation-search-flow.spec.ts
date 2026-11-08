import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, openSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
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
 * P4.D384 — the Salon's "This conversation" search, the `?msg=` reveal and the
 * compact header breadcrumb (v4 `4f5047241`, `3c41cf3da`), on its OWN
 * instance: port 4361, no LLM (the round's §R.9 ports; the mock port 45359 is
 * reserved for the gated beats' future streaming premise and unused here).
 *
 * The transcript is PLANTED, never streamed (memory note
 * `own-instance-e2e-fixture-choice`: the salon fixture pair + row copies):
 * "Group Expedition" (two LLM seats, a story background; moved into project
 * Skyhaven here) gains 60 rows. Every fifth carries the word `zephyr` (12 hits, oldest far
 * up the transcript), one Host announcement carries it too (a collapsed chip),
 * and one whisper to another seat carries `nightingale`.
 *
 * LIVE beats (S1–S8) need no server contract. GATED beats (G1, G2) name the
 * sibling lanes they wait on and are flipped at the union (§S.9); each fails
 * on its own premise if run early (memory note `unify-review-finding-shapes`).
 */

// === gates (flip at the union, §S.9) ===
const P4D366_FRAMES_LANDED = false;
const P4D385_CHIP_LANDED = false;
const P4D372_DEUS_LANDED = false;
// === end gates ===

const CS_PORT = 4361;
const CS_BASE_URL = `http://127.0.0.1:${CS_PORT}`;
const CS_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'salon-conversation-search-instance');
const CS_DATA_DIR = resolve(CS_INSTANCE_DIR, 'data');
const CS_SERVER_LOG = process.env['CS_SERVER_LOG'] ?? resolve(ARTIFACTS_DIR, 'salon-conversation-search-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'salon-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'salon-mount.db');
const USER_TABLES = ['characters', 'chats', 'tags', 'groups', 'projects', 'files'];

const CHAT_TITLE = 'Group Expedition';
const ROWS = 60;
/**
 * Planted ids are UUIDs: the transcript read drops a non-UUID message id
 * (memory note `memory-raw-reads-drop-non-uuid-fixtures`).
 */
const rowId = (i: number): string => `c5000000-0000-4000-8000-${String(i).padStart(12, '0')}`;
/** The row the far-up search beat reveals (the OLDEST `zephyr` hit). */
const FAR_ROW = rowId(0);
const ANNOUNCE_ID = 'c5000000-0000-4000-8000-0000000a0001';
const WHISPER_ID = 'c5000000-0000-4000-8000-0000000b0001';
/** The project the chat is moved into, so the breadcrumb has a project link. */
const PROJECT_ID = '70000002-0000-4000-8000-000000000001';
const PROJECT_NAME = 'Skyhaven';
const LONG_TITLE = 'The Group Expedition Across the Northern Ridge, Its Lanterns, Kettles and Unreliable Barometers';

let server: ChildProcess | undefined;
let chatId = '';

async function dispatchRaw(req: unknown): Promise<{ type?: string; data?: Record<string, unknown> }> {
  const res = await fetch(`${CS_BASE_URL}/api/dispatch`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(req),
  });
  return ((await res.json().catch(() => null)) as { type?: string; data?: Record<string, unknown> } | null) ?? {};
}

function sqlString(s: string): string {
  return `'${s.replace(/'/g, "''")}'`;
}

/** The participant id of a seat by its character's (original) name. */
function seatOf(name: string): string {
  return (
    `(SELECT json_extract(p.value, '$.id') FROM chats c2, json_each(c2.participants) p ` +
    `JOIN characters ch ON ch.id = json_extract(p.value, '$.characterId') ` +
    `WHERE c2.title = ${sqlString(CHAT_TITLE)} AND ch.name = ${sqlString(name)} LIMIT 1)`
  );
}

function plantRows(cli: string): void {
  const base = Date.parse('2026-10-01T09:00:00.000Z');
  const speakers = ['Aria', 'Bram'];
  // ONE statement for all sixty rows (each CLI write re-opens the encrypted
  // database, and sixty of them overran the hook's budget on a busy machine).
  const values: string[] = [];
  for (let i = 0; i < ROWS; i++) {
    const words =
      i % 5 === 0
        ? `Row ${i}: the zephyr off the ridge carried the smell of rain past the old mill.`
        : `Row ${i}: the lantern guttered while the kettle sang and the clock struck the quarter.`;
    const at = new Date(base + i * 60_000).toISOString();
    values.push(`(${sqlString(rowId(i))}, ${sqlString(words)}, ${sqlString(speakers[i % 2])}, ${sqlString(at)})`);
  }
  runCliWrite(
    cli,
    `INSERT INTO chat_messages (id, chatId, type, role, content, participantId, createdAt) ` +
      `SELECT v.column1, c.id, 'message', 'ASSISTANT', v.column2, ` +
      `(SELECT json_extract(p.value, '$.id') FROM json_each(c.participants) p ` +
      `JOIN characters ch ON ch.id = json_extract(p.value, '$.characterId') WHERE ch.name = v.column3 LIMIT 1), ` +
      `v.column4 FROM chats c, (VALUES ${values.join(', ')}) v WHERE c.title = ${sqlString(CHAT_TITLE)};`,
  );
  // A Host announcement (a collapsed chip) between rows 30 and 31.
  runCliWrite(
    cli,
    `INSERT INTO chat_messages (id, chatId, type, role, content, systemSender, createdAt) ` +
      `SELECT ${sqlString(ANNOUNCE_ID)}, c.id, 'message', 'ASSISTANT', ` +
      `${sqlString('The Host notes a zephyr rising over the ridge.')}, 'host', ` +
      `${sqlString(new Date(base + 30 * 60_000 + 30_000).toISOString())} FROM chats c WHERE c.title = ${sqlString(CHAT_TITLE)};`,
  );
  // A whisper between two LLM seats — not shown unless "Show all whispers".
  runCliWrite(
    cli,
    `INSERT INTO chat_messages (id, chatId, type, role, content, participantId, targetParticipantIds, createdAt) ` +
      `SELECT ${sqlString(WHISPER_ID)}, c.id, 'message', 'ASSISTANT', ` +
      `${sqlString('A nightingale sang only for Aria.')}, ${seatOf('Bram')}, ` +
      `json_array(${seatOf('Aria')}), ${sqlString(new Date(base + 45 * 60_000 + 30_000).toISOString())} ` +
      `FROM chats c WHERE c.title = ${sqlString(CHAT_TITLE)};`,
  );
}

test.describe('P4.D384 — "This conversation" search, the ?msg= reveal, the breadcrumb', () => {
  test.beforeAll(async () => {
    test.setTimeout(240_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(CS_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(CS_DATA_DIR, { recursive: true });
    mkdirSync(resolve(CS_DATA_DIR, 'files'), { recursive: true });
    copyFileSync(MAIN_FIXTURE, resolve(CS_DATA_DIR, 'quilltap.db'));
    if (existsSync(MOUNT_FIXTURE)) {
      copyFileSync(MOUNT_FIXTURE, resolve(CS_DATA_DIR, 'quilltap-mount-index.db'));
    }
    writeFileSync(resolve(CS_DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));
    for (const table of [...USER_TABLES, 'connection_profiles', 'api_keys', 'chat_settings', 'memories', 'image_profiles']) {
      runCliWrite(cli, `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`);
    }
    runCliWrite(cli, `UPDATE users SET id = '${SINGLE_USER_ID}' WHERE id = '${FIXTURE_USER}';`);
    plantRows(cli);
    // A long title fills its 256 px measuring cap, so the row is wide enough
    // for all three densities to be reachable above the `md` breakpoint.
    runCliWrite(
      cli,
      `UPDATE chats SET projectId = ${sqlString(PROJECT_ID)}, title = ${sqlString(LONG_TITLE)} WHERE title = ${sqlString(CHAT_TITLE)};`,
    );

    // The salon pair carries no `instance_settings` table (memory note
    // `builder-fixtures-lack-instance-settings`); plant generateDDL's own.
    const freshSchema = JSON.parse(
      readFileSync(resolve(REPO_ROOT, 'crates/quilltap-core/src/services/provisioning/fresh_schema.json'), 'utf8'),
    ) as { main: string[] };
    const settingsDdl = freshSchema.main.find((sql) => sql.startsWith('CREATE TABLE "instance_settings"'));
    if (!settingsDdl) throw new Error('fresh_schema.json carries no instance_settings CREATE TABLE');
    const ddl = spawnSync(cli, ['db', '--data-dir', CS_INSTANCE_DIR, '--write', settingsDdl], {
      env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
      encoding: 'utf8',
    });
    if (ddl.status !== 0 && !`${ddl.stdout}${ddl.stderr}`.includes('already exists')) {
      throw new Error(`instance_settings plant failed:\n${ddl.stdout}\n${ddl.stderr}`);
    }

    const logFd = openSync(CS_SERVER_LOG, 'w');
    server = spawn(
      web,
      ['--host', '127.0.0.1', '--port', String(CS_PORT), '--data-dir', CS_INSTANCE_DIR, '--spa-dir', spaDir()],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    server.unref();
    await waitForHealth();

    await dispatchRaw({ type: 'unlock', passphrase: E2E_PASSPHRASE });
    const chats = ((await dispatchRaw({ type: 'listChats' })).data as unknown as { id: string; title: string }[]) ?? [];
    chatId = chats.find((c) => c.title === LONG_TITLE)?.id ?? '';
    expect(chatId, `${CHAT_TITLE} on the pair`).toBeTruthy();
    // The plant self-checks (memory note `tier-r-vacuous-equal-rows`): every
    // row landed, so a beat can never pass on an empty transcript.
    const transcript = await dispatchRaw({ type: 'chatTranscript', chatId });
    const ids = ((transcript.data as { messages?: { id: string }[] } | undefined)?.messages ?? []).map((m) => m.id);
    expect(ids.filter((id) => Array.from({ length: ROWS }, (_, i) => rowId(i)).includes(id))).toHaveLength(ROWS);
    expect(ids).toEqual(expect.arrayContaining([ANNOUNCE_ID, WHISPER_ID]));
    // …and the chat itself still reads (a malformed plant fails here, not in a beat).
    const got = await dispatchRaw({ type: 'chatGet', chatId });
    expect(got.type, `chatGet on the planted chat: ${JSON.stringify(got).slice(0, 600)}`).not.toBe('error');
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
    rmSync(CS_INSTANCE_DIR, { recursive: true, force: true });
  });

  // ---- live -----------------------------------------------------------

  test('(S1) a header search inside the chat shows `This conversation (5/n)` above `Everywhere`', async ({ page }) => {
    await openChat(page);
    await searchFor(page, 'zephyr');
    const group = page.getByRole('group', { name: 'Matches in this conversation' });
    await expect(group).toBeVisible({ timeout: 15_000 });
    // 12 row hits + the Host's chip = 13; the bar shows five.
    await expect(group).toContainText('This conversation (5/13)');
    await expect(group.getByRole('button')).toHaveCount(5);
    const everywhere = page.getByText('Everywhere', { exact: true });
    await expect(everywhere).toBeVisible();
    const groupBox = (await group.boundingBox())!;
    const dividerBox = (await everywhere.boundingBox())!;
    expect(groupBox.y).toBeLessThan(dividerBox.y);
  });

  test('(S2) clicking a match far up lands that row at the top and flashes it; (S3) the dropdown closes and the query stays, Escape clears it', async ({ page }) => {
    await openChat(page);
    // Search for the far row's own number so it is among the five newest hits.
    await searchFor(page, 'Row 0: the zephyr');
    const group = page.getByRole('group', { name: 'Matches in this conversation' });
    await expect(group.getByRole('button')).toHaveCount(1, { timeout: 15_000 });
    await group.getByRole('button').first().click();
    const row = page.locator(`#message-${FAR_ROW}`);
    await expect(row).toHaveClass(/qt-memory-source-highlight/, { timeout: 10_000 });
    const list = page.locator('.qt-chat-messages').first();
    const rowTop = (await row.boundingBox())!.y;
    const listTop = (await list.boundingBox())!.y;
    expect(Math.abs(rowTop - listTop - 12)).toBeLessThan(8);
    // (S3) the dropdown closed; the query stayed.
    await expect(group).toHaveCount(0);
    const input = searchInput(page);
    await expect(input).toHaveValue('Row 0: the zephyr');
    // The two-stage Escape: refocusing may reopen the dropdown, and Escape
    // closes an open dropdown first; with it closed, Escape clears the query.
    await input.focus();
    if (await group.isVisible()) {
      await page.keyboard.press('Escape');
      await expect(group).toHaveCount(0);
      await expect(input).toHaveValue('Row 0: the zephyr');
    }
    await page.keyboard.press('Escape');
    await expect(input).toHaveValue('');
  });

  test('(S4) a match inside a collapsed announcement scrolls to its chip', async ({ page }) => {
    await openChat(page);
    await searchFor(page, 'zephyr rising');
    const group = page.getByRole('group', { name: 'Matches in this conversation' });
    await expect(group.getByRole('button')).toHaveCount(1, { timeout: 15_000 });
    await expect(group).toContainText('Announcement');
    await group.getByRole('button').first().click();
    await expect(page.locator(`#message-${ANNOUNCE_ID}`).first()).toBeInViewport({ timeout: 10_000 });
    // U2 / HANDOFF H3: expanding the chip first is P4.D385's announcement-group
    // input; once it lands the chip's words show here.
    if (P4D385_CHIP_LANDED) {
      await expect(page.getByText('The Host notes a zephyr rising over the ridge.')).toBeVisible();
    }
  });

  test('(S5) a `/salon/<id>?msg=<id>` link opened afresh scrolls to the row and flashes it', async ({ page }) => {
    await unlock(page);
    await page.goto(`${CS_BASE_URL}/salon/${chatId}?msg=${rowId(5)}`);
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
    await expect(page.locator(`#message-${rowId(5)}`)).toHaveClass(/qt-memory-source-highlight/, {
      timeout: 15_000,
    });
  });

  test('(S6) a `?msg=` to a whisper the view does not show toasts the whisper sentence', async ({ page }) => {
    await unlock(page);
    await page.goto(`${CS_BASE_URL}/salon/${chatId}?msg=${WHISPER_ID}`);
    await expect(
      page.getByText('That line was a whisper meant for other ears. Choose “Show all whispers” in the sidebar to see it.'),
    ).toBeVisible({ timeout: 15_000 });
  });

  test('(S7) the breadcrumb reads full → abbreviations → avatars as the header narrows, with the compact tooltips', async ({ page }) => {
    await page.setViewportSize({ width: 1800, height: 900 });
    await openChat(page);
    // The visible row only: the measuring copies are aria-hidden + inert.
    const project = page.locator('qt-conversation-breadcrumb a[href*="/prospero/"]:not([inert] *):not([aria-hidden] *)').first();
    await expect(page.getByRole('link', { name: PROJECT_NAME, exact: true })).toBeVisible({ timeout: 15_000 });
    await expect(page.locator('qt-conversation-breadcrumb').getByRole('link', { name: /(^| )Bram$/ })).toBeVisible();

    /** full = the name is the link's own words; abbrev = aria-label + `Sky`; avatar = aria-label, no words. */
    const densityNow = async (): Promise<string> => {
      const label = await project.getAttribute('aria-label');
      const text = (await project.innerText()).trim();
      if (!label) return text === PROJECT_NAME ? 'full' : `?${text}`;
      return text === '' ? 'avatar' : text === 'Sky' ? 'abbrev' : `?${text}`;
    };
    // Narrow the header itself rather than the viewport: a viewport sweep
    // crosses the participant sidebar's narrow-pane auto-collapse, which hands
    // the header its room back mid-sweep. The breadcrumb's ResizeObserver sees
    // its own container shrink either way.
    const header = page.locator('header:has(> qt-conversation-breadcrumb)').first();
    const start = Math.floor((await header.boundingBox())!.width);
    const seen: string[] = [];
    let firstAvatarWidth = 0;
    for (let width = start; width >= 160; width -= 8) {
      await header.evaluate((el, w) => {
        (el as HTMLElement).style.maxWidth = `${w}px`;
      }, width);
      await page.waitForTimeout(40);
      const d = await densityNow();
      if (seen.at(-1) !== d) seen.push(d);
      if (d === 'avatar' && !firstAvatarWidth) firstAvatarWidth = width;
    }
    // Narrowing walks the densities in order, through all three, and never back.
    expect(seen).toEqual(['full', 'abbrev', 'avatar']);

    // A compact link's tooltip: the name, then the hint — at the widest
    // avatar-only width, where the row is not yet clipped.
    await header.evaluate((el, w) => {
      (el as HTMLElement).style.maxWidth = `${w}px`;
    }, firstAvatarWidth);
    const compactProject = page.getByRole('link', { name: `Project: ${PROJECT_NAME}` });
    await compactProject.hover();
    await expect(page.getByText('Click to open project')).toBeVisible({ timeout: 5_000 });
    await page.locator('qt-conversation-breadcrumb').getByRole('link', { name: /(^| )Bram$/ }).hover();
    await expect(page.getByText('Click to open character')).toBeVisible({ timeout: 5_000 });
  });

  test('(S8) the story-background thumbnail opens the picture viewer', async ({ page }) => {
    await page.setViewportSize({ width: 1400, height: 900 });
    await openChat(page);
    const thumb = page.getByRole('button', { name: 'Display story background' });
    await expect(thumb).toBeVisible({ timeout: 15_000 });
    await thumb.click();
    await expect(page.locator('qt-image-modal [role="dialog"]')).toBeVisible({ timeout: 10_000 });
  });

  // ---- gated ----------------------------------------------------------

  test('(G1) the raised-hand chip walks raised → asked → yields / carried on → unasked (no toast)', async ({ page }) => {
    test.skip(
      !(P4D366_FRAMES_LANDED && P4D385_CHIP_LANDED),
      'awaits P4.D366 (the handAsked frame on the wire) and P4.D385 (the chip) — flipped at the union (§S.9)',
    );
    // The streamed walk itself (raised → `… is weighing your raised hand…` →
    // `… yields the floor — it is yours` / `… carried on regardless; your hand
    // is lowered`, and `The room fell quiet before anyone was asked — the floor
    // is yours` for a hand raised on the last speaker) needs a sequenced LLM
    // mock and rides `salon-interrupt-flow` (B)'s U1 assertion on its own
    // instance. Here: the chip is mounted at rest, idle and toast-free.
    await openChat(page);
    // The premise: the chip's live region exists. Run early, this fails here.
    await expect(page.locator('.qt-chat-composer [aria-live]')).toHaveCount(1);
    await expect(page.getByText('carried on regardless')).toHaveCount(0);
  });

  test('(G2) a `/deus` send dispatches `isEstablishedFact: true` and the row carries the ribbon', async ({ page }) => {
    test.skip(
      !(P4D372_DEUS_LANDED && P4D385_CHIP_LANDED),
      'awaits P4.D372 (ChatSend.is_established_fact) and P4.D385 (the composer’s fact toggle) — flipped at the union (§S.9)',
    );
    await openChat(page);
    const sent: unknown[] = [];
    page.on('request', (r) => {
      if (r.url().endsWith('/api/dispatch') && r.postData()?.includes('"chatSend"')) sent.push(JSON.parse(r.postData()!));
    });
    const composer = page.locator('.qt-chat-composer-input .qt-rich-editor-content');
    await composer.click();
    await page.keyboard.type('/deus The bridge is out.');
    await page.keyboard.press('Enter');
    await expect.poll(() => sent.length, { timeout: 10_000 }).toBeGreaterThan(0);
    expect(sent[0]).toMatchObject({ isEstablishedFact: true });
    await expect(page.locator('.qt-chat-message-established-fact').first()).toBeVisible({ timeout: 15_000 });
  });
});

function searchInput(page: Page) {
  return page.getByPlaceholder('Search this chat & all… (⌘K)');
}

async function searchFor(page: Page, query: string): Promise<void> {
  const input = searchInput(page);
  await expect(input).toBeVisible({ timeout: 15_000 });
  await input.click();
  await input.fill(query);
}

async function unlock(page: Page): Promise<void> {
  await dispatchRaw({ type: 'unlock', passphrase: E2E_PASSPHRASE });
  await page.goto(`${CS_BASE_URL}/`);
  const passphrase = page.locator('#qt-passphrase');
  if (await passphrase.isVisible({ timeout: 3_000 }).catch(() => false)) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

async function openChat(page: Page): Promise<void> {
  await dispatchRaw({ type: 'unlock', passphrase: E2E_PASSPHRASE });
  await page.goto(`${CS_BASE_URL}/salon/${chatId}`);
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
  const res = spawnSync(cli, ['db', '--data-dir', CS_INSTANCE_DIR, '--write', sql], {
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
      const res = await fetch(`${CS_BASE_URL}/health`);
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${CS_PORT}: ${lastErr}`);
}
