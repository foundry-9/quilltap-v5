import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, mkdirSync, openSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
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
 * P4.D308 — the composer's `/` slash commands in the live Salon (v4
 * `4877fc2da`, `SlashCommandPlugin`).
 *
 * The walk: a bare `/` lists the composer's actions, Prospero's tools and
 * Pascal's roster; `/d20` stages the dice chip through the real `chatRng`
 * preview; `/coin` lists the Composer action and Pascal's `coin` as an exact
 * tie (action first — `KIND_ORDER`), and the Pascal row runs at once; a
 * built-in with parameters opens Run Tool ON its form; prose and Escape leave
 * the text alone; one undo restores a committed `/word`.
 *
 * Its OWN instance on {@link SLASH_PORT} (the round's port table: P4.D307 4343,
 * this lane 4344, P4.D309 4345), because these beats stage chips, run tools and
 * toggle a setting — state the shared server's other specs must not inherit.
 * No beat consults a model (dice, a `min === max` Pascal roll, dialogs), so the
 * reserved mock port {@link SLASH_MOCK_LLM_PORT} is never listened on.
 *
 * The instance (R-L8, measured): `seed-pascal-tools-fixture.ts` writes the
 * SHARED instance only — it takes no instance directory — and the pair it reads
 * from (`pascal-run-custom-*`) is a slim harness fixture the Salon cannot boot
 * on (no `memories`, `background_jobs`, `embedding_profiles`: `listChats`
 * answers 500, measured). So this spec boots the salon pair the way
 * `salon-scenario-flow.spec.ts` does, runs global-setup's user-scoped rewrite
 * (a spec-owned server that skips it sees no chats), and copies the seeder's
 * rows itself — vault A's `Tools` folder, its `Tools/*.tool.json` links and
 * their files/documents, read from the pascal pair, remapped onto Aria's vault.
 * "Group Expedition" then deals Pascal's `coin` (no parameters, `min === max`)
 * and seats active characters; both premises are checked over the dispatch
 * wire before any beat. The seeder itself is untouched.
 *
 * The setting-toggle beat needs P4.D304's `composerSlashCommands` PUT arm: until
 * the keystone is on the branch, the dispatch verb DROPS an unknown key, so a
 * save would answer 200 with nothing written and a capability probe would lie.
 * ACTIVATE-AT-UNIFY: the unifier flips {@link P4D304_SERVER_LANDED}.
 */
const P4D304_SERVER_LANDED = false;

const SLASH_PORT = 4344;
/** Reserved for this spec by the round's port table; no beat needs a model. */
const SLASH_MOCK_LLM_PORT = 45342;
const SLASH_BASE_URL = `http://127.0.0.1:${SLASH_PORT}`;
const SLASH_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'slash-instance');
const SLASH_DATA_DIR = resolve(SLASH_INSTANCE_DIR, 'data');
const SLASH_SERVER_LOG = resolve(ARTIFACTS_DIR, 'slash-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'salon-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'salon-mount.db');
const PASCAL_MAIN = resolve(FIXTURES_DIR, 'pascal-run-custom-main.db');
const PASCAL_MOUNT = resolve(FIXTURES_DIR, 'pascal-run-custom-mount.db');
const PASCAL_SRC_DIR = resolve(ARTIFACTS_DIR, 'slash-pascal-src');
/** Vault A's minted id, read from the sidecar (it re-mints per fixture build). */
const PASCAL_VAULT_A = (
  JSON.parse(readFileSync(resolve(FIXTURES_DIR, 'pascal-run-custom-main.db.meta.json'), 'utf8')) as {
    vaultA: string;
  }
).vaultA;
const CHAT = 'Group Expedition';

/** Every fixture table the walk reads is filtered by userId (the scenario spec's list). */
const USER_TABLES = ['characters', 'chats', 'tags', 'groups', 'projects', 'files'];

const isMac = process.platform === 'darwin';
let server: ChildProcess | undefined;
let chatId = '';

async function dispatch(page: Page, req: unknown): Promise<Record<string, unknown>> {
  const res = await page.request.post(`${SLASH_BASE_URL}/api/dispatch`, { data: req });
  const body = (await res.json().catch(() => null)) as { data?: Record<string, unknown> } | null;
  return body?.data ?? {};
}

function composerEditor(page: Page) {
  return page.locator('.qt-chat-composer-input .qt-rich-editor-content');
}

function menu(page: Page) {
  return page.locator('#qt-slash-command-listbox');
}

function labels(page: Page) {
  return menu(page).locator('[role="option"] .qt-typeahead-option-label');
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

async function openChat(page: Page): Promise<void> {
  await page.goto(`${SLASH_BASE_URL}/salon/${chatId}`);
  await unlockIfLocked(page);
  await expect(composerEditor(page)).toBeVisible({ timeout: 15_000 });
}

async function typeInComposer(page: Page, text: string): Promise<void> {
  await composerEditor(page).click();
  await page.keyboard.type(text);
}

async function blankComposer(page: Page): Promise<void> {
  await composerEditor(page).click();
  await page.keyboard.press(isMac ? 'Meta+a' : 'Control+a');
  await page.keyboard.press('Backspace');
  // The draft saves on an 800ms debounce; let the blank flush.
  await page.waitForTimeout(1200);
}

test.describe('Composer / slash commands (P4.D308, v4 4877fc2da)', () => {
  test.describe.configure({ mode: 'serial' });

  test.beforeAll(async ({ browser }) => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(SLASH_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(resolve(SLASH_DATA_DIR, 'files'), { recursive: true });
    copyFileSync(MAIN_FIXTURE, resolve(SLASH_DATA_DIR, 'quilltap.db'));
    copyFileSync(MOUNT_FIXTURE, resolve(SLASH_DATA_DIR, 'quilltap-mount-index.db'));
    writeFileSync(
      resolve(SLASH_DATA_DIR, 'quilltap.dbkey'),
      makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE),
    );
    // Global-setup's user-scoped rewrite: the engine reads as SINGLE_USER_ID.
    for (const table of USER_TABLES) {
      runCliWrite(cli, `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`);
    }
    copyPascalTools(cli);

    const logFd = openSync(SLASH_SERVER_LOG, 'w');
    server = spawn(
      web,
      [
        '--host',
        '127.0.0.1',
        '--port',
        String(SLASH_PORT),
        '--data-dir',
        SLASH_INSTANCE_DIR,
        '--spa-dir',
        spaDir(),
      ],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    server.unref();
    await waitForHealth();

    // The premises (R-L8), over the same dispatch wire the SPA uses.
    const page = await browser.newPage();
    await dispatch(page, { type: 'unlock', passphrase: E2E_PASSPHRASE });
    const chats = (await dispatch(page, { type: 'listChats' })) as unknown as Array<{
      id: string;
      title: string;
    }>;
    chatId = (Array.isArray(chats) ? chats : []).find((c) => c.title === CHAT)?.id ?? '';
    expect(chatId, `the instance should carry "${CHAT}"`).toBeTruthy();
    const roster = (await dispatch(page, { type: 'chatCustomToolsList', chatId }))['tools'] as
      | Array<{ name: string }>
      | undefined;
    expect((roster ?? []).some((t) => t.name === 'coin'), 'Pascal deals coin here').toBe(true);
    const chat = (await dispatch(page, { type: 'chatGet', chatId }))['chat'] as {
      participants?: Array<{ type?: string; isActive?: boolean; removedAt?: string | null }>;
    };
    expect(
      (chat.participants ?? []).some((p) => p.type === 'CHARACTER' && p.isActive && !p.removedAt),
      'the chat seats an active character',
    ).toBe(true);
    await page.close();
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
    rmSync(SLASH_INSTANCE_DIR, { recursive: true, force: true });
  });

  test('a bare / lists the composer, Prospero and Pascal; prose and Escape leave the text', async ({
    page,
  }) => {
    await openChat(page);

    // (a) The bare-slash browse: every command, actions first.
    await typeInComposer(page, '/');
    await expect(menu(page)).toBeVisible({ timeout: 15_000 });
    const all = await labels(page).allTextContents();
    for (const expected of [
      '/attach — Attach file',
      '/tool — Run Tool',
      '/roll — Roll dice',
      "/pascal — Pascal's Table",
    ]) {
      expect(all, `the bare menu lists ${expected}`).toContain(expected);
    }
    const attachRow = menu(page).locator('[role="option"]', { hasText: '/attach —' });
    await expect(attachRow.locator('.qt-typeahead-option-detail')).toHaveText('Composer');
    await page.keyboard.press('Escape');
    await blankComposer(page);

    // (e) Prose: a slash inside a word never opens the menu.
    await typeInComposer(page, 'and/or');
    await page.waitForTimeout(400);
    await expect(menu(page)).toHaveCount(0);
    await expect(composerEditor(page)).toHaveText('and/or');
    await blankComposer(page);

    // (g) Escape closes the menu and leaves the text.
    await typeInComposer(page, '/atta');
    await expect(menu(page)).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(menu(page)).toHaveCount(0);
    await expect(composerEditor(page)).toHaveText('/atta');
    await blankComposer(page);

    // (g) One undo brings a committed word back. `/atta` + Enter clicks the
    // hidden file input — answer the chooser so nothing is left waiting.
    await typeInComposer(page, '/atta');
    await expect(labels(page).first()).toHaveText('/attach — Attach file');
    const chooser = page.waitForEvent('filechooser');
    await page.keyboard.press('Enter');
    await chooser;
    await expect(composerEditor(page)).toHaveText('');
    await composerEditor(page).click();
    await page.keyboard.press(isMac ? 'Meta+z' : 'Control+z');
    await expect(composerEditor(page)).toHaveText('/atta');
    await blankComposer(page);
  });

  test('/d20 stages the dice chip from a real preview roll', async ({ page }) => {
    await openChat(page);
    const chips = page.locator('.qt-chat-tool-result-chip');
    const before = await chips.count();

    await typeInComposer(page, '/d20');
    await expect(labels(page).first()).toHaveText('/d20 — Roll a d20');
    await page.keyboard.press('Enter');

    await expect(chips).toHaveCount(before + 1, { timeout: 15_000 });
    await expect(chips.last()).toContainText('🎲');
    await expect(composerEditor(page)).toHaveText('');
    // Clear the staged chip so nothing rides a later send.
    await chips.last().getByRole('button', { name: 'Remove tool result' }).click();
    await expect(chips).toHaveCount(before);
  });

  test('/coin ties the Composer action with Pascal’s coin — action first — and the Pascal row runs at once', async ({
    page,
  }) => {
    await openChat(page);
    const roster = (await dispatch(page, { type: 'chatCustomToolsList', chatId }))['tools'] as Array<{
      name: string;
      title: string;
      asCharacterId: string;
      parameters: Record<string, unknown>;
    }>;
    const coin = roster.filter((t) => t.name === 'coin');
    expect(coin.length, 'one coin per seat that resolves it').toBeGreaterThan(0);
    expect(Object.keys(coin[0].parameters ?? {}), 'coin takes no parameters').toHaveLength(0);

    await typeInComposer(page, '/coin');
    await expect(labels(page).first()).toHaveText('/coin — Flip a coin');
    await expect(labels(page).nth(1)).toHaveText(`/coin — ${coin[0].title}`);
    const bars = page.locator('.qt-chat-system-bar', { hasText: coin[0].title });
    const before = await bars.count();

    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');

    await expect(page.getByText('Pascal has settled coin.')).toBeVisible({ timeout: 15_000 });
    await expect(composerEditor(page)).toHaveText('');
    await expect(bars).toHaveCount(before + 1, { timeout: 15_000 });
    await expect(bars.last()).toContainText('Pascal');
  });

  test('a built-in with parameters opens Run Tool ON its form', async ({ page }) => {
    await openChat(page);
    const tools = ((await dispatch(page, { type: 'toolsList', chatId, includeSchemas: true }))['tools'] ??
      []) as Array<{
      id: string;
      name: string;
      available?: boolean;
      userInvocable?: boolean;
      parameters?: { properties?: Record<string, unknown> };
    }>;
    const wear = tools.find((t) => t.id === 'wardrobe_wear');
    test.skip(
      !wear || wear.available === false || wear.userInvocable === false,
      'wardrobe_wear is not invocable in this chat on this instance',
    );

    await typeInComposer(page, '/wardrobe_w');
    await expect(labels(page).first()).toHaveText(`/wardrobe_wear — ${wear!.name}`);
    await page.keyboard.press('Enter');

    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible({ timeout: 15_000 });
    await expect(dialog.locator('.qt-dialog-title')).toHaveText(`Run Tool: ${wear!.name}`);
    await expect(composerEditor(page)).toHaveText('');
    await dialog.getByRole('button', { name: 'Cancel' }).click();
    await expect(dialog).toBeHidden();
  });

  test('a built-in with NO parameters runs at once', async ({ page }) => {
    await openChat(page);
    const tools = ((await dispatch(page, { type: 'toolsList', chatId, includeSchemas: true }))['tools'] ??
      []) as Array<{
      id: string;
      available?: boolean;
      userInvocable?: boolean;
      parameters?: { properties?: Record<string, unknown> };
    }>;
    const bare = tools.find(
      (t) =>
        t.id === 'self_inventory' &&
        t.available !== false &&
        t.userInvocable !== false &&
        Object.keys(t.parameters?.properties ?? {}).length === 0,
    );
    test.skip(!bare, 'no zero-parameter self_inventory is invocable here');

    await typeInComposer(page, `/${bare!.id}`);
    await expect(labels(page).first()).toContainText(`/${bare!.id} —`);
    await page.keyboard.press('Enter');
    await expect(page.getByText(`Prospero has run ${bare!.id}.`)).toBeVisible({ timeout: 15_000 });
    await expect(page.getByRole('dialog')).toHaveCount(0);
  });

  test('the Slash Commands card turns the menu off, and only the menu', async ({ page }) => {
    test.skip(
      !P4D304_SERVER_LANDED,
      'awaits P4.D304’s composerSlashCommands PUT arm (ACTIVATE-AT-UNIFY)',
    );
    await page.goto(`${SLASH_BASE_URL}/settings?tab=chat&section=composer-slash-commands`);
    await unlockSettings(page);
    const toggle = page.getByRole('checkbox', { name: /Slash commands/ });
    await expect(toggle).toBeVisible({ timeout: 15_000 });
    await expect(toggle).toBeChecked();
    await toggle.uncheck();
    await expect
      .poll(async () => (await dispatch(page, { type: 'chatSettings' }))['composerSlashCommands'])
      .toBe(false);

    try {
      await openChat(page);
      await typeInComposer(page, '/att');
      await page.waitForTimeout(600);
      await expect(menu(page)).toHaveCount(0);
      // The gutter is untouched by the setting.
      await expect(page.getByRole('button', { name: 'Attach file', exact: true })).toBeVisible();
      await blankComposer(page);
    } finally {
      // Restore the instance's state for any later beat.
      await page.goto(`${SLASH_BASE_URL}/settings?tab=chat&section=composer-slash-commands`);
      await unlockSettings(page);
      const again = page.getByRole('checkbox', { name: /Slash commands/ });
      await again.check();
      await expect
        .poll(async () => (await dispatch(page, { type: 'chatSettings' }))['composerSlashCommands'])
        .toBe(true);
    }
  });
});

async function unlockSettings(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  if (await passphrase.isVisible().catch(() => false)) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

/**
 * The seeder's copy (`seed-pascal-tools-fixture.ts`), into THIS instance: vault
 * A's `Tools` folder row, its `Tools/*.tool.json` links, and their
 * files/documents rows, read from a throwaway instance over the committed
 * pascal pair and re-inserted remapped onto Aria's vault. Every copied id is
 * v4-minted, so nothing collides; `doc_mount_files` / `doc_mount_documents`
 * carry no mount id and copy verbatim.
 */
function copyPascalTools(cli: string): void {
  const aria = cliRows(cli, SLASH_INSTANCE_DIR, false, "SELECT characterDocumentMountPointId AS v FROM characters WHERE name = 'Aria'")[0]?.['v'];
  if (typeof aria !== 'string' || !aria) throw new Error('slash instance: Aria has no vault');

  rmSync(PASCAL_SRC_DIR, { recursive: true, force: true });
  mkdirSync(resolve(PASCAL_SRC_DIR, 'data'), { recursive: true });
  copyFileSync(PASCAL_MAIN, resolve(PASCAL_SRC_DIR, 'data', 'quilltap.db'));
  copyFileSync(PASCAL_MOUNT, resolve(PASCAL_SRC_DIR, 'data', 'quilltap-mount-index.db'));
  writeFileSync(
    resolve(PASCAL_SRC_DIR, 'data', 'quilltap.dbkey'),
    makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE),
  );
  const read = (sql: string) => cliRows(cli, PASCAL_SRC_DIR, true, sql);
  const folders = read(`SELECT * FROM doc_mount_folders WHERE mountPointId = '${PASCAL_VAULT_A}' AND name = 'Tools'`);
  const links = read(`SELECT * FROM doc_mount_file_links WHERE mountPointId = '${PASCAL_VAULT_A}' AND relativePath LIKE 'Tools/%'`);
  const ids = links.map((l) => `'${String(l['fileId'])}'`).join(',');
  if (folders.length !== 1 || !ids) throw new Error('slash instance: vault A has no Tools roster');
  const files = read(`SELECT * FROM doc_mount_files WHERE id IN (${ids})`);
  const documents = read(`SELECT * FROM doc_mount_documents WHERE fileId IN (${ids})`);
  rmSync(PASCAL_SRC_DIR, { recursive: true, force: true });

  const remap = (v: unknown) => (typeof v === 'string' ? v.split(PASCAL_VAULT_A).join(aria) : v);
  const lit = (v: unknown) =>
    v === null || v === undefined
      ? 'NULL'
      : typeof v === 'number'
        ? String(v)
        : `'${String(v).replace(/'/g, "''")}'`;
  for (const [table, rows] of [
    ['doc_mount_folders', folders],
    ['doc_mount_files', files],
    ['doc_mount_documents', documents],
    ['doc_mount_file_links', links],
  ] as Array<[string, Array<Record<string, unknown>>]>) {
    if (rows.length === 0) continue;
    const cols = Object.keys(rows[0]);
    const values = rows.map((row) => `(${cols.map((c) => lit(remap(row[c]))).join(', ')})`).join(',\n');
    runCliWrite(
      cli,
      `INSERT OR REPLACE INTO ${table} (${cols.map((c) => `"${c}"`).join(', ')}) VALUES ${values};`,
      true,
    );
  }
}

function cliRows(cli: string, dir: string, mount: boolean, sql: string): Array<Record<string, unknown>> {
  const args = ['db', '--data-dir', dir, ...(mount ? ['--mount-points'] : []), '--json', sql];
  const res = spawnSync(cli, args, {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  });
  if (res.status !== 0) throw new Error(`CLI read failed (${sql}):\n${res.stdout}\n${res.stderr}`);
  return JSON.parse(res.stdout) as Array<Record<string, unknown>>;
}

function runCliWrite(cli: string, sql: string, mount = false): void {
  const args = ['db', '--data-dir', SLASH_INSTANCE_DIR, ...(mount ? ['--mount-points'] : []), '--write', sql];
  const res = spawnSync(cli, args, {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
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
      const res = await fetch(`${SLASH_BASE_URL}/health`);
      // 423 is "server up, instance locked" — a fresh fixture boot.
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${SLASH_PORT}: ${lastErr}`);
}
