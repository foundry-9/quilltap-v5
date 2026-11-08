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

/**
 * P4.D388 — the round's SPA smalls, end to end on their own instance.
 *
 * Its own server on 4365 (the round's §R.9 port for this lane), from the
 * `diagrams-flow` recipe: the salon pair copied, the userId rewrite, the
 * `instance_settings` plant (the Brahma Console beats read and write that
 * table). No beat streams a reply, so the lane's reserved mock port (45363)
 * is not used and no mock is started.
 *
 * **UNGATED (A), (B)** need no server change: the About page's twelve edits,
 * and the kind card's per-instance radio-group name with the view and edit
 * tabs mounted side by side.
 *
 * **GATED (C)–(H)** need a server lane's wire (§S.9 — the unifier flips each
 * constant once its lane is on the union):
 * - `P4D369_SERVER_LANDED`: (C) — `kind` on the character GET / PUT.
 * - `P4D362_SERVER_LANDED`: (D), (E) — `capabilities.samplingParameters` on the
 *   providers listing (Anthropic `false`) and `maxImagesPerRequest` on every
 *   profile DTO + the create / update arms.
 * - `P4D374_SERVER_LANDED`: (F), (G) — `brahmaConsole.inlineConnectionProfileId`
 *   and the project's `brahmaConnectionProfileId`.
 * - `P4D379_SERVER_LANDED` AND `P4D385_POPUP_H1_LANDED`: (H) — the
 *   `chatCustomToolPresets` verb and the popup's new bindings (P4.D388 H1).
 */
const P4D369_SERVER_LANDED = false;
const P4D369_REASON = 'awaits P4.D369’s character `kind` on the GET and the PUT (wired at unification)';
const P4D362_SERVER_LANDED = false;
const P4D362_REASON =
  'awaits P4.D362’s capabilities.samplingParameters on the providers listing and maxImagesPerRequest on the profile DTOs (wired at unification)';
const P4D374_SERVER_LANDED = false;
const P4D374_REASON =
  'awaits P4.D374’s brahmaConsole.inlineConnectionProfileId and the project brahmaConnectionProfileId (wired at unification)';
const P4D379_SERVER_LANDED = false;
const P4D379_REASON = 'awaits P4.D379’s chatCustomToolPresets verb (wired at unification)';
const P4D385_POPUP_H1_LANDED = false;
const P4D385_REASON = 'awaits P4.D388’s HANDOFF H1 applied to custom-tools-popup.ts by P4.D385 (wired at unification)';

const SM_PORT = 4365;
const SM_BASE_URL = `http://127.0.0.1:${SM_PORT}`;
const SM_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'spa-smalls-instance');
const SM_DATA_DIR = resolve(SM_INSTANCE_DIR, 'data');
const SM_SERVER_LOG = resolve(ARTIFACTS_DIR, 'spa-smalls-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'salon-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'salon-mount.db');
const USER_TABLES = ['characters', 'chats', 'tags', 'groups', 'projects', 'files'];

let server: ChildProcess | undefined;

async function dispatchRaw(req: unknown): Promise<{ type?: string; data?: Record<string, unknown> }> {
  const res = await fetch(`${SM_BASE_URL}/api/dispatch`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(req),
  });
  return ((await res.json().catch(() => null)) as { type?: string; data?: Record<string, unknown> } | null) ?? {};
}

test.describe('P4.D388 — the SPA smalls', () => {
  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(SM_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(SM_DATA_DIR, { recursive: true });
    mkdirSync(resolve(SM_DATA_DIR, 'files'), { recursive: true });
    copyFileSync(MAIN_FIXTURE, resolve(SM_DATA_DIR, 'quilltap.db'));
    if (existsSync(MOUNT_FIXTURE)) {
      copyFileSync(MOUNT_FIXTURE, resolve(SM_DATA_DIR, 'quilltap-mount-index.db'));
    }
    writeFileSync(resolve(SM_DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));
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

    // The salon pair carries no `instance_settings` table; plant generateDDL's
    // own from the committed D23 dump (the scenario-browser-flow recipe).
    const freshSchema = JSON.parse(
      readFileSync(resolve(REPO_ROOT, 'crates/quilltap-core/src/services/provisioning/fresh_schema.json'), 'utf8'),
    ) as { main: string[] };
    const settingsDdl = freshSchema.main.find((sql) => sql.startsWith('CREATE TABLE "instance_settings"'));
    if (!settingsDdl) throw new Error('fresh_schema.json carries no instance_settings CREATE TABLE');
    const ddl = spawnSync(cli, ['db', '--data-dir', SM_INSTANCE_DIR, '--write', settingsDdl], {
      env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
      encoding: 'utf8',
    });
    if (ddl.status !== 0 && !`${ddl.stdout}${ddl.stderr}`.includes('already exists')) {
      throw new Error(`instance_settings plant failed:\n${ddl.stdout}\n${ddl.stderr}`);
    }

    const logFd = openSync(SM_SERVER_LOG, 'w');
    server = spawn(
      web,
      ['--host', '127.0.0.1', '--port', String(SM_PORT), '--data-dir', SM_INSTANCE_DIR, '--spa-dir', spaDir()],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    server.unref();
    await waitForHealth();
    await dispatchRaw({ type: 'unlock', passphrase: E2E_PASSPHRASE });
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
    rmSync(SM_INSTANCE_DIR, { recursive: true, force: true });
  });

  // ---- ungated ---------------------------------------------------------

  test('(A) the About page carries the 624d0d25c edits (C17)', async ({ page }) => {
    await open(page, '/about');
    const main = page.locator('qt-about-page');
    await expect(main.getByRole('heading', { name: 'Key Features' })).toBeVisible({ timeout: 15_000 });
    for (const blurb of [
      'Inform notes from the wings, Deus Ex Machina established facts that every character knows, the Scenario Builder, and indexed message search that looks in the open conversation first',
      'dice rolls, coin flips, the Inventory of objects characters carry, stats, plus',
      'write back to, browsed with one folder browser shared with the Wardrobe and the Inventory and read-only viewers for pictures, PDFs and Word files',
      'ChaCha20-Poly1305-encrypted databases, the Pepper Vault for API keys',
      'with lookup shortcuts for characters, memories and conversations, reachable from any Salon as the @Brahma answerer on an engine of its own choosing',
    ]) {
      await expect(main).toContainText(blurb);
    }
    await expect(main).toContainText('TypeScript 6+');
    await expect(main).toContainText('SQLite + SQLite3 Multiple Ciphers');
    await expect(main).not.toContainText('File Manager');
    await expect(main).not.toContainText('SQLCipher');
  });

  test('(B) the kind card: Roleplay by default, and the view and edit tabs keep separate radio groups', async ({
    page,
  }) => {
    // The shared fixture pins ROUTE mode ('0'); this beat needs the tabbed
    // workspace, where the view and edit tabs can stand side by side. A later
    // init script wins, so the flag is back ON for this page only.
    await page.context().addInitScript(() => {
      window.localStorage.setItem('quilltap.workspace.tabs', '1');
    });
    await openAria(page);
    const viewCard = page.locator('[data-testid="character-kind"]').first();
    await expect(viewCard).toBeVisible({ timeout: 15_000 });
    await expect(viewCard.locator('legend')).toHaveText('Kind of character');
    await expect(viewCard).toContainText(
      'Whether this character is a player in the drama or a member of the staff. It decides how they are directed when the curtain is up.',
    );
    await expect(viewCard.getByLabel(/^Roleplay character/)).toBeChecked();
    await expect(viewCard.getByLabel(/^Assistant/)).not.toBeChecked();

    // Open the edit tab, then split it into the other pane (the workspace-flow
    // gesture): the view tab is visible again on the left, the edit tab on the
    // right, so both are mounted side by side — v4's two-tab reason.
    await page.getByRole('link', { name: /Edit Character/i }).click();
    await expect(page.locator('#title')).toBeVisible({ timeout: 15_000 });
    await page.locator('.qt-workspace').click({ position: { x: 5, y: 5 } });
    await page.keyboard.press('Control+Alt+\\');
    await expect(page.locator('.qt-workspace-divider')).toBeVisible();
    const cards = page.locator('[data-testid="character-kind"]');
    await expect(cards).toHaveCount(2, { timeout: 15_000 });

    const names = await cards.evaluateAll((els) =>
      els.map((el) => (el.querySelector('input[type="radio"]') as HTMLInputElement).name),
    );
    expect(names[0]).not.toBe('');
    expect(names[0]).not.toBe(names[1]);

    // Choose Assistant on the edit card (the one inside the form with #title);
    // the view card's Roleplay radio stays checked — one shared name would
    // have unchecked it.
    const editCard = page.locator('qt-character-edit [data-testid="character-kind"]');
    const viewCard2 = page.locator('qt-character-detail [data-testid="character-kind"]');
    // `click`, not `check`: before P4.D369 the server may refuse `{kind}`, and
    // the card's snap-back would then fail `check`'s own after-state assertion.
    // The claim here is about the OTHER card, which no save touches.
    await editCard.getByLabel(/^Assistant/).click();
    await expect(viewCard2.locator('input[value="roleplay"]')).toBeChecked();
  });

  // ---- gated -----------------------------------------------------------

  test('(C) choosing Assistant saves the kind to the vault and the edit tab follows', async ({ page }) => {
    test.skip(!P4D369_SERVER_LANDED, P4D369_REASON);
    await openAria(page);
    const card = page.locator('[data-testid="character-kind"]').first();
    await card.getByLabel(/^Assistant/).check();
    await expect(page.getByText('Very good — this character joins the staff as an assistant')).toBeVisible();

    await page.reload();
    await unlockIfAsked(page);
    await page.getByRole('link', { name: /Edit Character/i }).click();
    const editCard = page.locator('[data-testid="character-kind"]').filter({ visible: true }).first();
    await expect(editCard.getByLabel(/^Assistant/)).toBeChecked({ timeout: 15_000 });

    const aria = await character('Aria');
    const vault = aria['characterDocumentMountPointId'] as string;
    expect(vault, 'Aria’s vault mount').toBeTruthy();
    const file = await dispatchRaw({ type: 'mountFileRead', mountPointId: vault, path: 'properties.json' });
    expect(JSON.parse(String(file.data?.['content'] ?? '{}'))['kind']).toBe('assistant');

    // Leave Aria as the fixture had her.
    await dispatchRaw({ type: 'characterUpdate', characterId: aria['id'], character: { kind: 'roleplay' } });
  });

  test('(D) an Anthropic profile hides Temperature / Top P; its card shows Max Tokens alone', async ({ page }) => {
    test.skip(!P4D362_SERVER_LANDED, P4D362_REASON);
    const created = await dispatchRaw({
      type: 'connectionProfileCreate',
      profile: {
        name: 'Smalls Anthropic',
        transport: 'api',
        provider: 'ANTHROPIC',
        modelName: 'claude-sonnet-4-5',
        isDefault: false,
        parameters: { max_tokens: 1234 },
        apiKeyId: null,
        baseUrl: '',
      },
    });
    expect(created.type, JSON.stringify(created)).not.toBe('error');

    await openProfilesCard(page);
    const anthropicCard = profileCard(page, 'Smalls Anthropic');
    await expect(anthropicCard.locator('div.qt-text-xs.mt-2')).toHaveText('Max Tokens: 1234');
    const other = page.locator('div.qt-card.p-4').filter({ hasText: 'OPENAI_COMPATIBLE' }).first();
    await expect(other.locator('div.qt-text-xs.mt-2')).toContainText('Temperature:');
    await expect(other.locator('div.qt-text-xs.mt-2')).toContainText('Top P:');

    await anthropicCard.getByRole('button', { name: 'Edit' }).click();
    await expect(page.locator('#qt-pf-maxtokens')).toBeVisible({ timeout: 15_000 });
    await expect(page.locator('#qt-pf-temp')).toHaveCount(0);
    await expect(page.locator('#qt-pf-topp')).toHaveCount(0);
    await expect(
      page.getByText('This provider chooses its own temperature and Top P; only the token ceiling is yours to set.'),
    ).toBeVisible();
  });

  test('(E) "Most images per request": 4 by default, 7 saved, 99 refused back to 4', async ({ page }) => {
    test.skip(!P4D362_SERVER_LANDED, P4D362_REASON);
    const created = await dispatchRaw({
      type: 'connectionProfileCreate',
      profile: {
        name: 'Smalls Vision',
        transport: 'api',
        provider: 'OPENAI_COMPATIBLE',
        modelName: 'seer-1',
        isDefault: false,
        parameters: {},
        apiKeyId: null,
        baseUrl: 'http://127.0.0.1:9/v1',
      },
    });
    expect(created.type, JSON.stringify(created)).not.toBe('error');

    await openProfilesCard(page);
    const editAndSet = async (value: string | null): Promise<void> => {
      await profileCard(page, 'Smalls Vision').getByRole('button', { name: 'Edit' }).click();
      const vision = page
        .locator('label')
        .filter({ hasText: 'Supports image attachments (vision input)' })
        .locator('input[type=checkbox]');
      if (!(await vision.isChecked())) await vision.check();
      const cap = page.locator('#maxImagesPerRequest');
      await expect(cap).toBeVisible();
      if (value !== null) {
        await cap.fill(value);
        await page.getByRole('button', { name: 'Update Profile' }).click();
        await expect(cap).toBeHidden({ timeout: 15_000 });
      }
    };
    await editAndSet(null);
    await expect(page.locator('#maxImagesPerRequest')).toHaveValue('4');
    await page.getByRole('button', { name: 'Cancel' }).click();

    await editAndSet('7');
    await editAndSet(null);
    await expect(page.locator('#maxImagesPerRequest')).toHaveValue('7');
    await page.getByRole('button', { name: 'Cancel' }).click();

    await editAndSet('99');
    await editAndSet(null);
    await expect(page.locator('#maxImagesPerRequest')).toHaveValue('4');
  });

  test('(F) Settings → Chat → Brahma Console: an inline engine saved, then inherit', async ({ page }) => {
    test.skip(!P4D374_SERVER_LANDED, P4D374_REASON);
    await open(page, '/settings?tab=chat&section=brahma-console');
    const turns = page.locator('#brahma-max-turns');
    await expect(turns).toBeVisible({ timeout: 15_000 });
    const budget = await turns.inputValue();
    const engine = page.locator('#brahma-inline-profile');
    await expect(engine).toBeEnabled({ timeout: 15_000 });
    const firstProfile = await engine.locator('option').nth(1).getAttribute('value');
    expect(firstProfile).toBeTruthy();

    await engine.selectOption(firstProfile!);
    await expect(page.getByText('Inline Brahma engine saved')).toBeVisible();
    await page.reload();
    await unlockIfAsked(page);
    await expect(page.locator('#brahma-inline-profile')).toHaveValue(firstProfile!, { timeout: 15_000 });
    await expect(page.locator('#brahma-max-turns')).toHaveValue(budget);

    await page.locator('#brahma-inline-profile').selectOption('');
    await expect(page.getByText('Inline Brahma engine saved').first()).toBeVisible();
    await page.reload();
    await unlockIfAsked(page);
    await expect(page.locator('#brahma-inline-profile')).toHaveValue('', { timeout: 15_000 });
  });

  test('(G) a project’s Model Behavior card names its own inline engine, then inherits', async ({ page }) => {
    test.skip(!P4D374_SERVER_LANDED, P4D374_REASON);
    await open(page, '/prospero');
    const projectCards = page.locator('qt-project-card');
    await expect(projectCards.first()).toBeVisible({ timeout: 15_000 });
    await projectCards.first().getByRole('link', { name: 'Open' }).click();
    const card = page.locator('qt-project-model-behavior-card');
    await expect(card).toBeVisible({ timeout: 15_000 });
    const engine = card.getByRole('combobox', { name: 'Inline Brahma Engine' });
    if (!(await engine.isVisible().catch(() => false))) {
      await card.getByRole('button', { name: /Model Behavior/ }).click();
    }
    await expect(engine).toBeEnabled({ timeout: 15_000 });
    const firstProfile = await engine.locator('option').nth(1).getAttribute('value');

    await engine.selectOption(firstProfile!);
    await expect(page.getByText('Inline Brahma engine set for project')).toBeVisible();
    await page.reload();
    await unlockIfAsked(page);
    const reloaded = page.locator('qt-project-model-behavior-card').getByRole('combobox', { name: 'Inline Brahma Engine' });
    if (!(await reloaded.isVisible().catch(() => false))) {
      await page.locator('qt-project-model-behavior-card').getByRole('button', { name: /Model Behavior/ }).click();
    }
    await expect(reloaded).toHaveValue(firstProfile!, { timeout: 15_000 });

    await reloaded.selectOption('');
    await expect(page.getByText('Inline Brahma engine set to inherit from global')).toBeVisible();
  });

  test('(H) Run-dialog presets: filed in the vault, shadowing a farther one, and saved into a chosen store', async ({
    page,
  }) => {
    test.skip(!(P4D379_SERVER_LANDED && P4D385_POPUP_H1_LANDED), `${P4D379_REASON}; ${P4D385_REASON}`);
    const chats = ((await dispatchRaw({ type: 'listChats' })).data as unknown as { id: string; title: string }[]) ?? [];
    const chatId = chats.find((c) => c.title === 'Group Expedition')?.id ?? '';
    expect(chatId, 'Group Expedition on the pair').toBeTruthy();

    const roster = (await dispatchRaw({ type: 'chatCustomToolsList', chatId })).data as {
      tools?: { mountPointId: string; asCharacterId: string }[];
    };
    const first = roster.tools?.[0];
    expect(first, 'the seeded Tools roster').toBeTruthy();
    const definition = {
      $schema: '/schemas/qtap-custom-tool.schema.json',
      name: 'smalls_preset',
      title: 'Smalls Contrivance',
      description: 'A parameterised contrivance for the preset walk.',
      roll: { min: 1, max: 1 },
      parameters: { bonus: { type: 'number', default: 0, description: 'Skill bonus.' } },
      outcomes: [{ when: true, message: 'Settled at {{value}}.', state: 'info' }],
    };
    await dispatchRaw({
      type: 'mountFileWrite',
      mountPointId: first!.mountPointId,
      path: 'Tools/smalls_preset.tool.json',
      content: `${JSON.stringify(definition, null, 2)}\n`,
    });
    const pool = (
      await dispatchRaw({ type: 'chatCustomToolPresets', chatId, tool: 'smalls_preset', asCharacterId: first!.asCharacterId })
    ).data as { stores: { tier: string; mountPointId: string; mountName: string }[] };
    const nearest = pool.stores[0];
    const farthest = pool.stores[pool.stores.length - 1];
    expect(pool.stores.length, 'a walk of at least two stores').toBeGreaterThan(1);

    // A same-named preset in the farthest store, for the shadows title.
    await dispatchRaw({
      type: 'mountFileWrite',
      mountPointId: farthest.mountPointId,
      path: 'Tools/smalls_preset.x.settings.json',
      content: JSON.stringify({ bonus: 1 }, null, 2),
    });

    await open(page, `/salon/${chatId}`);
    await page.getByRole('button', { name: 'Custom tools' }).click();
    const dialog = page.getByRole('dialog');
    await dialog.getByRole('button').filter({ hasText: 'Smalls Contrivance' }).first().click();
    await dialog.locator('input[type="number"]').fill('3');
    await dialog.getByLabel('Name to save this preset under').fill('x');
    await dialog.getByRole('button', { name: 'Save preset' }).click();
    await expect(page.getByText(`Preset “x” filed in ${nearest.mountName}.`)).toBeVisible();

    const option = dialog.getByLabel('Load a saved preset for Smalls Contrivance').locator('option', { hasText: 'x —' });
    await expect(option).toHaveText(`x — ${nearest.mountName}`);
    await expect(option).toHaveAttribute('title', new RegExp(`^Also in .*${escapeRegExp(farthest.mountName)}.*, which this one outranks$`));

    await dialog.getByLabel('Save into').selectOption(farthest.mountPointId);
    await dialog.getByLabel('Name to save this preset under').fill('y');
    await dialog.getByRole('button', { name: 'Save preset' }).click();
    await expect(page.getByText(`Preset “y” filed in ${farthest.mountName}.`)).toBeVisible();
    const filed = await dispatchRaw({
      type: 'mountFileRead',
      mountPointId: farthest.mountPointId,
      path: 'Tools/smalls_preset.y.settings.json',
    });
    expect(JSON.parse(String(filed.data?.['content'] ?? '{}'))).toEqual({ bonus: 3 });
  });
});

// ---- helpers -----------------------------------------------------------

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

async function character(name: string): Promise<Record<string, unknown>> {
  const data = (await dispatchRaw({ type: 'characterList' })).data ?? {};
  const list = (data['characters'] as { id: string; name: string }[] | undefined) ?? [];
  const id = list.find((c) => c.name === name)?.id ?? '';
  expect(id, `${name} on the pair`).toBeTruthy();
  const detail = (await dispatchRaw({ type: 'characterGet', characterId: id })).data ?? {};
  return (detail['character'] as Record<string, unknown>) ?? { id };
}

async function unlockIfAsked(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  if (await passphrase.isVisible().catch(() => false)) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

async function open(page: Page, path: string): Promise<void> {
  await dispatchRaw({ type: 'unlock', passphrase: E2E_PASSPHRASE });
  await page.goto(`${SM_BASE_URL}${path}`);
  await page.waitForLoadState('domcontentloaded');
  await unlockIfAsked(page);
}

async function openAria(page: Page): Promise<void> {
  await open(page, '/characters');
  const aria = page.locator('.character-card-grid .character-card').filter({ hasText: 'Aria' }).first();
  await expect(aria).toBeVisible({ timeout: 15_000 });
  await aria.locator('p.line-clamp-3').click();
  await expect(page.getByRole('heading', { name: 'Aria' })).toBeVisible({ timeout: 15_000 });
}

async function openProfilesCard(page: Page): Promise<void> {
  await open(page, '/settings?tab=providers&section=connection-profiles');
  await expect(page.getByRole('button', { name: '+ Add Profile' })).toBeVisible({ timeout: 15_000 });
}

function profileCard(page: Page, name: string): Locator {
  return page.locator('div.qt-card.p-4').filter({ has: page.locator('h3', { hasText: name }) });
}

async function waitForHealth(): Promise<void> {
  const deadline = Date.now() + 30_000;
  let lastErr = '';
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${SM_BASE_URL}/health`);
      // 423 = up, instance locked (the server gets no passphrase env).
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${SM_PORT}: ${lastErr} (see ${SM_SERVER_LOG})`);
}

function runCliWrite(cli: string, sql: string): void {
  const res = spawnSync(cli, ['db', '--data-dir', SM_INSTANCE_DIR, '--write', sql], {
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
