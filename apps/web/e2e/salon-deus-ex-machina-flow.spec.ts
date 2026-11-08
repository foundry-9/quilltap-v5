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
import { startMockLlm, type MockLlm } from './support/mock-llm';
import { openSidebarSection } from './support/sidebar';
import { expectTooltip } from './support/tooltips';

/**
 * P4.D385 — Deus Ex Machina in the composer and the transcript (v4
 * `db6190b44` / `df8d4d70a`), and the Salon avatar's two `27fb3ec77`
 * affordances (the picture full size; carried Inventory glyphs).
 *
 * Its own instance on 4362 and its own mock on 45360 (the round's §R.9 ports),
 * from the `diagrams-flow` recipe: the salon pair, the userId rewrite,
 * `connection_profiles.baseUrl` aimed at the mock, the `instance_settings`
 * plant. Aria gets a picture (a bundled static avatar) so the zoom beat has one.
 *
 * **UNGATED beats (a)–(h)** need no server change: arming is client state, the
 * two refusals never post, the avatar opens a viewer, and the carried glyphs
 * read the existing `chatInventoryGet`.
 *
 * **GATED beats (i)–(l)** need P4.D372's wire (`MessageDto.isEstablishedFact`,
 * `systemKind: 'established-fact'`, the announcement route's `{kind:'fact'}`
 * arm) and P4.D384's HANDOFF H1 (the composer's `establishedFact` → `chatSend`
 * `isEstablishedFact: true`, the optimistic row, the impersonation thread).
 * ACTIVATE-AT-UNIFY (§S.9): the unifier flips `DEUS_WIRE_LANDED` to `true`.
 */
const DEUS_WIRE_LANDED = false;
const GATE_REASON =
  'awaits P4.D372’s established-fact wire (isEstablishedFact, the {kind:"fact"} announcement) and P4.D384’s H1 send body (wired at unification)';

const DX_PORT = 4362;
const DX_BASE_URL = `http://127.0.0.1:${DX_PORT}`;
const DX_MOCK_PORT = 45360;
const DX_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'deus-instance');
const DX_DATA_DIR = resolve(DX_INSTANCE_DIR, 'data');
const DX_SERVER_LOG = resolve(ARTIFACTS_DIR, 'deus-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'salon-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'salon-mount.db');
const USER_TABLES = ['characters', 'chats', 'tags', 'groups', 'projects', 'files'];
const ARIA_PICTURE = '/images/avatars/lantern-avatar.webp';

const ATTACH_SENTENCE = 'A fact must be written down — attachments alone cannot be established.';
const BLANK_SENTENCE = 'A fact must be written down — there is nothing here to establish.';
const RIBBON = 'Deus Ex Machina — established as fact';

const isMac = process.platform === 'darwin';
const MOD = isMac ? 'Meta' : 'Control';

let server: ChildProcess | undefined;
let mock: MockLlm | undefined;
let soloChatId = '';
let groupChatId = '';
let ariaPictureId = '';

/** Upload a bundled webp into a chat's files (the composer's multipart leg); its file id. */
async function uploadPicture(chatId: string, name: string): Promise<string> {
  const bytes = readFileSync(resolve(spaDir(), ARIA_PICTURE.slice(1)));
  const form = new FormData();
  form.append('file', new Blob([bytes], { type: 'image/webp' }), name);
  const res = await fetch(`${DX_BASE_URL}/api/v1/chats/${chatId}/files`, { method: 'POST', body: form });
  // A duplicate (the same bytes already in the chat) answers with the
  // existing file rather than a new one — either id serves.
  const body = (await res.json().catch(() => ({}))) as {
    file?: { id?: string };
    id?: string;
    existingFile?: { id?: string };
  };
  const id = body.file?.id ?? body.id ?? body.existingFile?.id ?? '';
  expect(id, `upload ${name} (${res.status})`).toBeTruthy();
  return id;
}

async function dispatchRaw(req: unknown): Promise<{ type?: string; data?: Record<string, unknown> }> {
  const res = await fetch(`${DX_BASE_URL}/api/dispatch`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(req),
  });
  return ((await res.json().catch(() => null)) as { type?: string; data?: Record<string, unknown> } | null) ?? {};
}

test.describe('P4.D385 — Deus Ex Machina, the avatar picture, carried glyphs', () => {
  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(DX_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(DX_DATA_DIR, { recursive: true });
    mkdirSync(resolve(DX_DATA_DIR, 'files'), { recursive: true });
    copyFileSync(MAIN_FIXTURE, resolve(DX_DATA_DIR, 'quilltap.db'));
    if (existsSync(MOUNT_FIXTURE)) {
      copyFileSync(MOUNT_FIXTURE, resolve(DX_DATA_DIR, 'quilltap-mount-index.db'));
    }
    writeFileSync(resolve(DX_DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));
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
      `UPDATE connection_profiles SET baseUrl = 'http://127.0.0.1:${DX_MOCK_PORT}/v1' WHERE provider = 'OPENAI_COMPATIBLE';`,
    );
    // One assistant row from Aria in Solo Voyage, so the transcript has an
    // avatar to click and to hang glyphs under.
    runCliWrite(
      cli,
      `INSERT INTO chat_messages (id, chatId, type, role, content, participantId, createdAt) ` +
        `SELECT '5d1c2b3a-4e5f-4a6b-8c7d-9e0f1a2b3c4d', c.id, 'message', 'ASSISTANT', 'Aria looks up from the chart.', ` +
        `(SELECT json_extract(p.value, '$.id') FROM json_each(c.participants) p ` +
        `JOIN characters ch ON ch.id = json_extract(p.value, '$.characterId') WHERE ch.name = 'Aria' LIMIT 1), ` +
        `'2026-02-02T00:00:01.000Z' FROM chats c WHERE c.title = 'Solo Voyage';`,
    );
    mock = await startMockLlm('Very good.', DX_MOCK_PORT, 50);

    const freshSchema = JSON.parse(
      readFileSync(resolve(REPO_ROOT, 'crates/quilltap-core/src/services/provisioning/fresh_schema.json'), 'utf8'),
    ) as { main: string[] };
    const settingsDdl = freshSchema.main.find((sql) => sql.startsWith('CREATE TABLE "instance_settings"'));
    if (!settingsDdl) throw new Error('fresh_schema.json carries no instance_settings CREATE TABLE');
    const ddl = spawnSync(cli, ['db', '--data-dir', DX_INSTANCE_DIR, '--write', settingsDdl], {
      env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
      encoding: 'utf8',
    });
    if (ddl.status !== 0 && !`${ddl.stdout}${ddl.stderr}`.includes('already exists')) {
      throw new Error(`instance_settings plant failed:\n${ddl.stdout}\n${ddl.stderr}`);
    }

    const logFd = openSync(DX_SERVER_LOG, 'w');
    server = spawn(
      web,
      ['--host', '127.0.0.1', '--port', String(DX_PORT), '--data-dir', DX_INSTANCE_DIR, '--spa-dir', spaDir()],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    server.unref();
    await waitForHealth();

    await dispatchRaw({ type: 'unlock', passphrase: E2E_PASSPHRASE });
    const chats = ((await dispatchRaw({ type: 'listChats' })).data as unknown as { id: string; title: string }[]) ?? [];
    soloChatId = chats.find((c) => c.title === 'Solo Voyage')?.id ?? '';
    groupChatId = chats.find((c) => c.title === 'Group Expedition')?.id ?? '';
    expect(soloChatId, 'Solo Voyage on the pair').toBeTruthy();
    expect(groupChatId, 'Group Expedition on the pair').toBeTruthy();
    // Aria's picture, through the real upload leg and `characterAvatar` (a
    // character's picture is its `defaultImageId` file, not a column).
    ariaPictureId = await uploadPicture(soloChatId, 'aria-portrait.webp');
    const set = await dispatchRaw({ type: 'characterAvatar', characterId: await characterId('Aria'), imageId: ariaPictureId });
    expect(set.type, 'Aria’s avatar').not.toBe('error');
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
    rmSync(DX_INSTANCE_DIR, { recursive: true, force: true });
  });

  // ---- ungated ---------------------------------------------------------

  test('(a) /deus arms: the chip, the tinted wrapper, the Send name; the chip × disarms', async ({ page }) => {
    await openChat(page, soloChatId);
    await blankComposer(page);
    await editor(page).click();
    await page.keyboard.type('/deus');
    const option = page.locator('#qt-slash-command-listbox [role="option"]', { hasText: 'Deus Ex Machina' });
    await expect(option).toBeVisible({ timeout: 10_000 });
    await page.keyboard.press('Enter');
    await expectArmed(page, true);
    const chip = armedChip(page);
    await chip.getByRole('button', { name: 'Disarm Deus Ex Machina' }).click();
    await expectArmed(page, false);
  });

  test('(b) the portrait toggle arms and disarms with aria-pressed, keeping the caret in the editor', async ({ page }) => {
    await openChat(page, soloChatId);
    await blankComposer(page);
    await editor(page).click();
    await page.keyboard.type('The bridge');
    const toggle = page.getByRole('button', { name: 'Deus Ex Machina', exact: true });
    await expect(toggle).toHaveAttribute('aria-pressed', 'false');
    await expectTooltip(toggle, 'Deus Ex Machina — post your next line as established fact (or type /deus)');
    await toggle.click();
    await expect(toggle).toHaveAttribute('aria-pressed', 'true');
    await expectArmed(page, true);
    // The caret never left the editor: typing carries on in place.
    await page.keyboard.type(' is out.');
    await expect(editor(page)).toHaveText('The bridge is out.');
    await toggle.click();
    await expect(toggle).toHaveAttribute('aria-pressed', 'false');
    await expectArmed(page, false);
  });

  test('(c) a chat switch disarms', async ({ page }) => {
    await openChat(page, soloChatId);
    await blankComposer(page);
    await page.getByRole('button', { name: 'Deus Ex Machina', exact: true }).click();
    await expectArmed(page, true);
    await openChat(page, groupChatId);
    await expectArmed(page, false);
    await openChat(page, soloChatId);
    await expectArmed(page, false);
  });

  test('(d) Cmd/Ctrl+Shift+Enter on a blank composer refuses aloud and sends nothing', async ({ page }) => {
    await openChat(page, soloChatId);
    await blankComposer(page);
    const before = await rows(page).count();
    await editor(page).click();
    await page.keyboard.press(`${MOD}+Shift+Enter`);
    await expect(page.getByText(BLANK_SENTENCE)).toBeVisible({ timeout: 10_000 });
    await page.waitForTimeout(500);
    expect(await rows(page).count()).toBe(before);
  });

  test('(e) an attachments-only armed Send refuses aloud and stays armed', async ({ page }) => {
    await openChat(page, soloChatId);
    await blankComposer(page);
    await editor(page).click();
    await page.keyboard.type('x');
    await page.getByRole('button', { name: 'Deus Ex Machina', exact: true }).click();
    await expectArmed(page, true);
    await page
      .locator('input[type=file][aria-label="Choose a file to attach"]')
      .setInputFiles({ name: 'deus-note.txt', mimeType: 'text/plain', buffer: Buffer.from('a note') });
    await expect(page.locator('.qt-chat-attachment-chip')).toContainText('deus-note.txt', { timeout: 15_000 });
    await editor(page).click();
    await page.keyboard.press('Backspace');
    await expect(editor(page)).toHaveText('');
    const before = await rows(page).count();
    await page.getByRole('button', { name: 'Send as established fact' }).click();
    await expect(page.getByText(ATTACH_SENTENCE)).toBeVisible({ timeout: 10_000 });
    await expectArmed(page, true);
    await page.waitForTimeout(500);
    expect(await rows(page).count()).toBe(before);
    await page.getByRole('button', { name: 'Remove attachment deus-note.txt' }).click();
  });

  test('(f) the assistant avatar opens the image viewer, and not while Salon Images is hidden', async ({ page }) => {
    await openChat(page, soloChatId);
    const zoom = page.getByRole('button', { name: "View Aria's picture full size" }).first();
    await expect(zoom).toBeVisible({ timeout: 15_000 });
    await zoom.click();
    // The modal's host has no box of its own; its dialog does.
    const viewer = page.locator('qt-image-modal [role="dialog"]');
    await expect(viewer).toBeVisible({ timeout: 10_000 });
    await expect(viewer.locator('img').first()).toHaveAttribute('src', new RegExp(ariaPictureId));
    await page.keyboard.press('Escape');
    await expect(page.locator('qt-image-modal')).toHaveCount(0);

    await toggleSalonImages(page, true);
    await expect(page.getByRole('button', { name: "View Aria's picture full size" })).toHaveCount(0);
    await toggleSalonImages(page, false);
  });

  test('(g) an equipped item shows as a glyph under its character’s avatar; an unequip removes it live', async ({ page }) => {
    const ariaId = await characterId('Aria');
    const planted = await dispatchRaw({
      type: 'inventoryCreate',
      container: { scope: 'character', id: ariaId },
      body: { name: 'Pocket Watch', description: 'Ticks loudly.' },
    });
    const itemId = ((planted.data?.['item'] as { id?: string } | undefined)?.id ?? '') as string;
    expect(itemId, 'the Pocket Watch plant').toBeTruthy();
    const equip = await dispatchRaw({ type: 'chatInventoryEquip', chatId: soloChatId, characterId: ariaId, ids: [itemId] });
    expect(equip.type, 'the equip').not.toBe('error');

    await openChat(page, soloChatId);
    const carried = page.locator('[data-testid="salon-avatar-carried"]').first();
    await expect(carried.locator('li')).toHaveCount(1, { timeout: 15_000 });
    await expect(carried).toHaveAttribute('aria-label', 'Carrying');
    const glyph = carried.getByRole('img', { name: 'Pocket Watch' });
    await expect(glyph.locator('[data-testid="inventory-glyph-abbreviation"]')).toHaveText('PW');
    // The tooltip: the bold name, then the description.
    await glyph.focus();
    const bubble = page.locator('.qt-tooltip').last();
    await expect(bubble.locator('.font-semibold')).toHaveText('Pocket Watch');
    await expect(bubble).toContainText('Ticks loudly.');
    await glyph.blur();

    // An unequip publishes `chats`; the row updates with no reload.
    await dispatchRaw({ type: 'chatInventoryUnequip', chatId: soloChatId, characterId: ariaId, ids: [itemId] });
    await expect(page.locator('[data-testid="salon-avatar-carried"]')).toHaveCount(0, { timeout: 15_000 });
  });

  test('(h) the portrait keeps its 4:5 box with and without a picture; the toggle sits in its corner', async ({ page }) => {
    await openChat(page, soloChatId);
    const box = async () => {
      const portrait = page.locator('qt-speaking-as-avatar > div').first();
      await expect(portrait).toBeVisible({ timeout: 15_000 });
      const outer = (await portrait.boundingBox())!;
      const toggle = (await page.getByRole('button', { name: 'Deus Ex Machina', exact: true }).boundingBox())!;
      return { outer, toggle };
    };
    const check = ({ outer, toggle }: Awaited<ReturnType<typeof box>>, label: string) => {
      // eslint-disable-next-line no-console
      console.log(`[P4.D385 h] ${label}: portrait ${outer.width.toFixed(1)}×${outer.height.toFixed(1)}`);
      expect(outer.height, `${label}: a real box, not a sliver`).toBeGreaterThan(40);
      expect(Math.abs(outer.width - 0.8 * outer.height), `${label}: 4:5`).toBeLessThan(3);
      expect(toggle.x).toBeGreaterThanOrEqual(outer.x);
      expect(toggle.x + toggle.width).toBeLessThan(outer.x + outer.width / 2 + toggle.width);
      expect(toggle.y + toggle.height).toBeLessThanOrEqual(outer.y + outer.height + 0.5);
      expect(toggle.y).toBeGreaterThan(outer.y + outer.height / 2);
    };
    check(await box(), 'no picture');
    // Give the user's own character a picture and measure again.
    const chat = (await dispatchRaw({ type: 'chatGet', chatId: soloChatId })).data?.['chat'] as
      | { participants?: { controlledBy?: string; character?: { id?: string } | null }[] }
      | undefined;
    const userCharacterId = chat?.participants?.find((p) => p.controlledBy === 'user')?.character?.id ?? '';
    expect(userCharacterId, 'the user’s own character in Solo Voyage').toBeTruthy();
    const picture = await uploadPicture(soloChatId, 'user-portrait.webp');
    await dispatchRaw({ type: 'characterAvatar', characterId: userCharacterId, imageId: picture });
    await openChat(page, soloChatId);
    await expect(page.locator('qt-speaking-as-avatar img')).toBeVisible({ timeout: 15_000 });
    check(await box(), 'with a picture');
  });

  // ---- gated -----------------------------------------------------------

  test('(i) an armed send posts a seat line the server stores as fact, ribboned, then disarms', async ({ page }) => {
    test.skip(!DEUS_WIRE_LANDED, GATE_REASON);
    await openChat(page, soloChatId);
    await blankComposer(page);
    await editor(page).click();
    await page.keyboard.type('The bridge to Meridian is out.');
    await page.getByRole('button', { name: 'Deus Ex Machina', exact: true }).click();
    await page.getByRole('button', { name: 'Send as established fact' }).click();
    const row = page.locator('.qt-chat-message-row', { hasText: 'The bridge to Meridian is out.' });
    await expect(row.locator('.qt-chat-message-established-fact')).toHaveText(RIBBON, { timeout: 15_000 });
    await expectArmed(page, false);
    await page.reload();
    await unlockIfAsked(page);
    await expect(
      page.locator('.qt-chat-message-row', { hasText: 'The bridge to Meridian is out.' }).locator('.qt-chat-message-established-fact'),
    ).toHaveText(RIBBON, { timeout: 15_000 });
  });

  test('(j) the shortcut sends one fact without arming', async ({ page }) => {
    test.skip(!DEUS_WIRE_LANDED, GATE_REASON);
    await openChat(page, soloChatId);
    await blankComposer(page);
    await editor(page).click();
    await page.keyboard.type('The tide turns at noon.');
    await page.keyboard.press(`${MOD}+Shift+Enter`);
    const row = page.locator('.qt-chat-message-row', { hasText: 'The tide turns at noon.' });
    await expect(row.locator('.qt-chat-message-established-fact')).toHaveText(RIBBON, { timeout: 15_000 });
    await expectArmed(page, false);
  });

  test('(k) Insert Announcement → Deus Ex Machina → Establish posts a full-width Host band', async ({ page }) => {
    test.skip(!DEUS_WIRE_LANDED, GATE_REASON);
    await openChat(page, soloChatId);
    await page.getByRole('button', { name: 'Insert announcement' }).click();
    const dialog = page.getByRole('dialog');
    await dialog.getByRole('tab', { name: 'Deus Ex Machina' }).click();
    await dialog.locator('.qt-rich-editor-content').click();
    await page.keyboard.type('The lighthouse has gone dark.');
    await dialog.getByRole('button', { name: 'Establish' }).click();
    await expect(page.getByText('Established as fact')).toBeVisible({ timeout: 10_000 });
    const band = page.locator('.qt-chat-established-fact-row', { hasText: 'The lighthouse has gone dark.' });
    await expect(band).toBeVisible({ timeout: 15_000 });
    await expect(band.locator('.qt-chat-system-bar-sender')).toHaveText('The Host');
    await expect(band.locator('.qt-chat-system-bar-kind')).toHaveText('Deus Ex Machina');
    await expect(band.locator('.qt-chat-desktop-avatar')).toHaveCount(0);
    await expect(page.locator('.qt-chat-announcement-chip', { hasText: 'Deus Ex Machina' })).toHaveCount(0);
  });

  test('(l) a fact never opens the In Their Own Words dialog for an impersonated seat', async ({ page }) => {
    test.skip(!DEUS_WIRE_LANDED, GATE_REASON);
    // In Their Own Words on `ask` (the salon-impersonation-voice-flow recipe).
    await dispatchRaw({ type: 'unlock', passphrase: E2E_PASSPHRASE });
    await page.goto(`${DX_BASE_URL}/settings?tab=chat&section=composer-spellcheck`);
    await unlockIfAsked(page, false);
    const ask = page
      .locator('qt-impersonation-voice-settings label.qt-settings-toggle-row')
      .filter({ has: page.locator('.font-medium', { hasText: /^Ask each time$/ }) })
      .locator('input[type="radio"]');
    await expect(ask).toBeVisible({ timeout: 15_000 });
    if (!(await ask.isChecked())) await ask.check();
    await openChat(page, groupChatId);
    await openSidebarSection(page, 'Participants');
    await page.locator('qt-participant-card button[title="Speak as Aria"]').click();
    await blankComposer(page);
    await editor(page).click();
    await page.keyboard.type('The storm has passed.');
    await page.keyboard.press(`${MOD}+Shift+Enter`);
    await expect(page.getByRole('dialog', { name: /In Their Own Words/ })).toHaveCount(0);
    const row = page.locator('.qt-chat-message-row', { hasText: 'The storm has passed.' });
    await expect(row.locator('.qt-chat-message-established-fact')).toHaveText(RIBBON, { timeout: 15_000 });
  });
});

// ---- helpers -------------------------------------------------------------

function editor(page: Page) {
  return page.locator('.qt-chat-composer-input .qt-rich-editor-content');
}

function armedChip(page: Page) {
  return page.locator('.qt-chat-tool-result-chip', { hasText: 'Deus Ex Machina: this line will be established as fact' });
}

async function expectArmed(page: Page, armed: boolean): Promise<void> {
  const wrapper = page.locator('.qt-chat-composer-established-fact');
  if (armed) {
    await expect(armedChip(page)).toBeVisible({ timeout: 10_000 });
    await expect(wrapper).toHaveCount(1);
    await expect(page.locator('button.qt-chat-composer-send').first()).toHaveAttribute('aria-label', 'Send as established fact');
  } else {
    await expect(armedChip(page)).toHaveCount(0, { timeout: 10_000 });
    await expect(wrapper).toHaveCount(0);
    await expect(page.locator('button.qt-chat-composer-send').first()).toHaveAttribute('aria-label', 'Send message');
  }
}

async function blankComposer(page: Page): Promise<void> {
  await editor(page).click();
  await page.keyboard.press(isMac ? 'Meta+a' : 'Control+a');
  await page.keyboard.press('Backspace');
}

/** Every rendered transcript row — a refused send must not add one. */
function rows(page: Page) {
  return page.locator('.qt-chat-message-row');
}

/** Flip "Salon Images" through the user menu's quick-hide section (the quick-hide-flow recipe). */
async function toggleSalonImages(page: Page, expectPressed: boolean): Promise<void> {
  const trigger = page.getByRole('button', { name: 'User menu' });
  await trigger.click();
  const toggle = page.locator('qt-quick-hide-menu-section').getByRole('button', { name: 'Salon Images' });
  await expect(toggle).toBeVisible({ timeout: 10_000 });
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-pressed', String(expectPressed));
  await trigger.click();
}

async function characterId(name: string): Promise<string> {
  const data = (await dispatchRaw({ type: 'characterList' })).data ?? {};
  const list = (data['characters'] as { id: string; name: string }[] | undefined) ?? [];
  const id = list.find((c) => c.name === name)?.id ?? '';
  expect(id, `${name} on the pair`).toBeTruthy();
  return id;
}

async function unlockIfAsked(page: Page, inChat = true): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  if (!inChat) {
    if (await passphrase.isVisible().catch(() => false)) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
    return;
  }
  const messages = page.locator('.qt-chat-messages-list');
  await expect(passphrase.or(messages).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
  await expect(messages).toBeVisible({ timeout: 15_000 });
}

async function openChat(page: Page, chatId: string): Promise<void> {
  await dispatchRaw({ type: 'unlock', passphrase: E2E_PASSPHRASE });
  await page.goto(`${DX_BASE_URL}/salon/${chatId}`);
  await unlockIfAsked(page);
  await expect(editor(page)).toBeVisible({ timeout: 15_000 });
}

function runCliWrite(cli: string, sql: string): void {
  const res = spawnSync(cli, ['db', '--data-dir', DX_INSTANCE_DIR, '--write', sql], {
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
      const res = await fetch(`${DX_BASE_URL}/health`);
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${DX_PORT}: ${lastErr}`);
}
