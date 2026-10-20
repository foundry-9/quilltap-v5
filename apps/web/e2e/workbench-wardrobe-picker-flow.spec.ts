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
  REPO_ROOT,
  SINGLE_USER_ID,
  spaDir,
  TEST_PEPPER,
  webBinary,
} from './support/env';
import { MOCK_LLM_REPLY, startMockLlm, type MockLlm } from './support/mock-llm';

/**
 * P4.D320 — the Pascal SPA pair (v4 `5cb0e0190` the Workbench wardrobe picker +
 * `a05c6f755` `{{character}}` / `announceAsCharacter`, pinned at `e8e17b2d9`):
 * the whose-wardrobe lens on the Side-effects card, reference chips and the
 * unresolved advisory, the reference picker, the uncovered-slot flags, the
 * **The character announces it** toggle, the **Character name** insert, and —
 * once the server lands — the bench's would apply / would skip, a
 * `{{character}}` roll and a character-announced bubble.
 *
 * Its OWN instance on port 4347 (mock LLM 45345 — the round's §R.9 ports for
 * this lane), over the `characters-main.db` / `characters-mount.db` pair (the
 * `wardrobe-flow.spec.ts` instance: vaulted characters, with the userId
 * rewrite). MEASURED at lane time (P4.D320 R-7): with `instance_settings` +
 * the `projects` / `groups` / `group_doc_mount_links` tables planted from the
 * committed D23 dump, `customToolsDestinations` lists Quilltap General and five
 * character vaults (Aria, Bram, Cleo, Dax, Fenn; no projects, no groups), and
 * Aria's vault carries two garments — `Flight Jacket` (top, her default) and
 * `Goggles` (accessories); the other vaults and General are empty. No garment
 * is seeded: Aria's two are what the walk browses. Her chat `Solo Voyage`
 * (`c1000000-…-0001`) is the gated Salon beat's room.
 *
 * **LIVE beats** need only `main`'s server: (a) the lens + chips + advisory,
 * (b) the picker, (c) the slot flags, (d) the toggle's round trip through a
 * save (`main`'s server keeps `announceAsCharacter` as an unknown key — T19),
 * (e) the Insert value entry.
 *
 * **GATED beats** — ACTIVATE-AT-UNIFY (§S.9, R-O; the `P4D288_SERVER_LANDED`
 * precedent of `scenario-browser-flow.spec.ts`): the unifier flips the
 * constant once P4.D319's C3 is on the union. A NAMED constant, never a
 * capability probe.
 */
const P4D319_SERVER_LANDED = true;
const GATE_REASON =
  'awaits P4.D319’s C3 — §1 the preview’s `wardrobeResolution`, §3 `{{character}}` rendering, §4 the character-announced post — wired at unification';

const WP_PORT = 4347;
const WP_BASE_URL = `http://127.0.0.1:${WP_PORT}`;
/** This spec's own mock LLM — clear of every shared port. */
const WP_MOCK_LLM_PORT = 45345;
const WP_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'workbench-wardrobe-picker-instance');
const WP_DATA_DIR = resolve(WP_INSTANCE_DIR, 'data');
const WP_SERVER_LOG = resolve(ARTIFACTS_DIR, 'workbench-wardrobe-picker-server.log');

const ARIA_ID = 'a1000000-0000-4000-8000-000000000001';
const SOLO_VOYAGE_CHAT_ID = 'c1000000-0000-4000-8000-000000000001';

/** The `{{…}}` literals, kept out of template strings Playwright never sees anyway. */
const CHARACTER_PLACEHOLDER = '{{character}}';

let server: ChildProcess | undefined;
let mock: MockLlm | undefined;

test.use({ baseURL: WP_BASE_URL });

test.describe('P4.D320 — the Workbench wardrobe picker and the character announcer', () => {
  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(WP_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(WP_DATA_DIR, { recursive: true });
    mkdirSync(resolve(WP_DATA_DIR, 'files'), { recursive: true });
    copyFileSync(resolve(FIXTURES_DIR, 'characters-main.db'), resolve(WP_DATA_DIR, 'quilltap.db'));
    copyFileSync(
      resolve(FIXTURES_DIR, 'characters-mount.db'),
      resolve(WP_DATA_DIR, 'quilltap-mount-index.db'),
    );
    writeFileSync(resolve(WP_DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));

    // The pair predates `instance_settings` (so boot would provision no
    // Quilltap General) and the projects / groups schema the destinations verb
    // reads on every request (absent, it answers `no such table: projects` —
    // measured). Instance MATERIALIZATION from the committed D23 dump, never a
    // fixture regen: the committed pair is untouched. `chat_settings` joined the
    // list at the `e8e17b2d9` unification: beat (h) is this file's one Salon
    // page, and the chat GET reads the settings row (absent, `[Chats v1] Error
    // fetching chat … no such table: chat_settings` — measured on the union).
    const freshSchema = JSON.parse(
      readFileSync(
        resolve(REPO_ROOT, 'crates/quilltap-core/src/services/provisioning/fresh_schema.json'),
        'utf8',
      ),
    ) as { main: string[]; mountIndex: string[] };
    for (const [partition, table] of [
      ['main', 'instance_settings'],
      ['main', 'projects'],
      ['main', 'groups'],
      ['main', 'chat_settings'],
      ['mountIndex', 'group_doc_mount_links'],
    ] as const) {
      const create = freshSchema[partition].find((sql) => sql.startsWith(`CREATE TABLE "${table}"`));
      if (!create) throw new Error(`fresh_schema.json carries no ${table} CREATE TABLE`);
      const args = [
        'db',
        '--data-dir',
        WP_INSTANCE_DIR,
        '--write',
        create.replace('CREATE TABLE', 'CREATE TABLE IF NOT EXISTS'),
      ];
      if (partition === 'mountIndex') args.splice(1, 0, '--mount-points');
      const res = spawnSync(cli, args, {
        env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
        encoding: 'utf8',
      });
      if (res.status !== 0) throw new Error(`${table} plant failed:\n${res.stdout}\n${res.stderr}`);
    }
    for (const table of [
      'characters',
      'chats',
      'tags',
      'api_keys',
      'connection_profiles',
      'image_profiles',
      'files',
    ]) {
      runCliWrite(cli, `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`);
    }
    runCliWrite(
      cli,
      `UPDATE connection_profiles SET baseUrl = 'http://127.0.0.1:${WP_MOCK_LLM_PORT}/v1' WHERE provider = 'OPENAI_COMPATIBLE';`,
    );
    mock = await startMockLlm(MOCK_LLM_REPLY, WP_MOCK_LLM_PORT);

    const logFd = openSync(WP_SERVER_LOG, 'w');
    server = spawn(
      web,
      ['--host', '127.0.0.1', '--port', String(WP_PORT), '--data-dir', WP_INSTANCE_DIR, '--spa-dir', spaDir()],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    server.unref();
    await waitForHealth();
    await dispatch({ type: 'unlock', passphrase: E2E_PASSPHRASE });
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
    rmSync(WP_INSTANCE_DIR, { recursive: true, force: true });
  });

  // -------------------------------------------------------------------------
  // (a) LIVE — the lens on a tool in Aria's vault
  // -------------------------------------------------------------------------
  test('(a) a tool saved into Aria’s vault browses her wardrobe: a chip, an advisory, never an error', async ({
    page,
  }) => {
    await newContrivance(page);
    await fillValidIdentity(page, 'E2E Lens Contrivance', 'e2e_lens_contrivance');
    await addWardrobeEffect(page);
    await page.getByLabel('Item 1').fill('flight jacket');
    await saveInto(page, 'Aria');

    // Opened (and followed) on the store the file lives in: Aria's vault.
    await expect(page.getByText('Browsing Aria’s wardrobe')).toBeVisible({ timeout: 15_000 });
    const chip = page.getByTestId('wardrobe-reference-chip');
    await expect(chip).toBeVisible({ timeout: 15_000 });
    await expect(chip).toContainText('Flight Jacket');
    await expect(chip).toContainText('Top');
    await expect(chip).toContainText('Aria’s own');
    await expect(chip).toContainText('Aria only');
    await expect(chip).toHaveAttribute('aria-label', 'Item 1: Flight Jacket — choose another');

    await page.getByRole('button', { name: 'Add item' }).click();
    await page.getByLabel('Item 2').fill('Nonsense Cape');
    await expect(page.getByTestId('wardrobe-reference-unresolved')).toHaveText('not in Aria’s reach');
    // An advisory, never a draft error: Save stays live.
    await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  });

  // -------------------------------------------------------------------------
  // (b) LIVE — the picker
  // -------------------------------------------------------------------------
  test('(b) Choose… searches Aria’s wardrobe and writes an id when asked; the chip flips back to the title', async ({
    page,
  }) => {
    await newContrivance(page);
    await addWardrobeEffect(page);
    await chooseVantage(page, 'Aria');

    await page.getByRole('button', { name: 'Choose…' }).click();
    const picker = page.getByRole('dialog').filter({ hasText: 'Choose a garment or outfit' });
    await expect(picker).toBeVisible();
    await expect(picker.getByText('Browsing Aria’s wardrobe')).toBeVisible();
    await picker.getByLabel('Search the wardrobe').fill('gog');
    await expect(picker.getByTestId('wardrobe-reference-option')).toHaveCount(1);
    await picker.getByRole('radio', { name: 'id' }).check();
    await expect(
      picker.getByText(
        'An id names this exact garment and no other; a title names whichever garment of that name is nearest when the tool runs.',
      ),
    ).toBeVisible();
    await picker.getByRole('button', { name: 'Pick Goggles' }).click();
    await expect(picker).toBeHidden();

    const chip = page.getByTestId('wardrobe-reference-chip');
    await expect(chip).toContainText('Goggles');
    const toggle = page.getByRole('button', { name: 'Reference by title instead' });
    await expect(toggle).toHaveText('by id');
    await toggle.click();
    await expect(page.getByRole('button', { name: 'Reference by id instead' })).toHaveText('by title');
  });

  // -------------------------------------------------------------------------
  // (c) LIVE — the uncovered-slot flags
  // -------------------------------------------------------------------------
  test('(c) layering into a slot the garment does not cover flags the slot and says why', async ({ page }) => {
    await newContrivance(page);
    await addWardrobeEffect(page);
    await chooseVantage(page, 'Aria');
    await page.getByLabel('Item 1').fill('Flight Jacket');
    await page.getByLabel('Wear mode').selectOption('add_to_slot');
    await page.getByLabel('Wardrobe slot').selectOption('footwear');

    await expect(page.getByLabel('Wardrobe slot').locator('option', { hasText: 'Footwear (not covered)' })).toHaveCount(1);
    await expect(page.getByLabel('Wardrobe slot').locator('option', { hasText: /^Top$/ })).toHaveCount(1);
    await expect(page.getByTestId('wardrobe-slot-advisory')).toHaveText('⚠ “Flight Jacket” covers Top — not Footwear');
  });

  // -------------------------------------------------------------------------
  // (d) LIVE — The character announces it, through a save
  // -------------------------------------------------------------------------
  test('(d) the character-announces toggle saves after defaultVisibility and reopens checked', async ({ page }) => {
    await newContrivance(page);
    await fillValidIdentity(page, 'E2E Announced Contrivance', 'e2e_announced_contrivance');
    await page.getByLabel('Default visibility').selectOption('whisper');
    const toggle = page.locator('label', { hasText: 'The character announces it' }).locator('input');
    await expect(toggle).not.toBeChecked();
    await toggle.check();

    // The live preview is the bytes Save writes.
    await expect(page.locator('pre').filter({ hasText: 'announceAsCharacter' })).toContainText(
      /"defaultVisibility": "whisper",\s*"announceAsCharacter": true/,
    );
    await saveInto(page, 'Aria');

    await page.getByRole('button', { name: 'Library' }).click();
    await expect(page.getByRole('button', { name: 'New contrivance', exact: true })).toBeVisible({
      timeout: 15_000,
    });
    await page.getByRole('button', { name: /E2E Announced Contrivance/ }).first().click();
    await expect(page.getByText('The contrivance itself')).toBeVisible({ timeout: 15_000 });
    await expect(
      page.locator('label', { hasText: 'The character announces it' }).locator('input'),
    ).toBeChecked();
    // Known now, so never carried as an unknown key (T19).
    await expect(page.getByText('Carries keys this build doesn’t know')).toHaveCount(0);
  });

  // -------------------------------------------------------------------------
  // (e) LIVE — the Character name insert
  // -------------------------------------------------------------------------
  test('(e) Insert value → Character name puts {{character}} in the outcome message', async ({ page }) => {
    await newContrivance(page);
    const message = page.getByLabel('Outcome message').first();
    await message.fill('Over to ');
    await page.getByRole('button', { name: /Insert value/ }).first().click();
    const items = page.getByRole('menuitem');
    const labels = await items.allTextContents();
    expect(labels.map((l) => l.trim()).indexOf('Metadata key…')).toBe(
      labels.map((l) => l.trim()).indexOf('Character name') + 1,
    );
    await page.getByRole('menuitem', { name: 'Character name' }).click();
    await expect(message).toHaveValue(new RegExp(CHARACTER_PLACEHOLDER.replace(/[{}]/g, '\\$&')));
  });

  // -------------------------------------------------------------------------
  // (f) GATED P4D319 — the bench's would apply / would skip
  // -------------------------------------------------------------------------
  test('(f) with Aria in the chair the bench says would apply and would skip', async ({ page }) => {
    test.skip(!P4D319_SERVER_LANDED, GATE_REASON);
    await newContrivance(page);
    await fillValidIdentity(page, 'E2E Bench Lens', 'e2e_bench_lens');
    await addWardrobeEffect(page);
    await page.getByLabel('Item 1').fill('Flight Jacket');
    await page.getByRole('button', { name: 'Add item' }).click();
    await page.getByLabel('Item 2').fill('Nonsense Cape');
    await chooseVantage(page, 'Aria');

    await page.getByRole('button', { name: 'Roll', exact: true }).click();
    await expect(page.getByText(/→ wardrobe: wear\s*Flight Jacket \(Top\)\s*\(would apply\)/)).toBeVisible({
      timeout: 15_000,
    });
    await expect(
      page.getByText('→ wardrobe: wear “Nonsense Cape” — not in Aria’s reach (would skip)'),
    ).toBeVisible();
  });

  // -------------------------------------------------------------------------
  // (g) GATED P4D319 — a {{character}} roll names the chair
  // -------------------------------------------------------------------------
  test('(g) a {{character}} message rolls as the seated character', async ({ page }) => {
    test.skip(!P4D319_SERVER_LANDED, GATE_REASON);
    await newContrivance(page);
    await fillValidIdentity(page, 'E2E Character Roll', 'e2e_character_roll');
    // The first outcome row's default condition is `value ≥ 0.5` — a coin flip
    // the catch-all row wins half the time (measured at the `e8e17b2d9`
    // unification: raw 0.1604 → "The wheel gives 0.1604."). `≥ 0` always holds.
    await page.getByLabel('Operand number').first().fill('0');
    await page.getByLabel('Outcome message').first().fill(`${CHARACTER_PLACEHOLDER} steps up.`);
    await page.getByRole('radio', { name: 'Pick a character' }).click();
    await page.getByLabel('Character whose fact sheet to use').selectOption(ARIA_ID);
    await page.getByRole('button', { name: 'Roll', exact: true }).click();
    await expect(page.getByText('Aria steps up.').first()).toBeVisible({ timeout: 15_000 });
  });

  // -------------------------------------------------------------------------
  // (h) GATED P4D319 — a character-announced bubble names Aria on the Staff bar
  // -------------------------------------------------------------------------
  test('(h) a character-announced run in Solo Voyage names Aria on the Staff bar', async ({ page }) => {
    test.skip(!P4D319_SERVER_LANDED, GATE_REASON);
    // Written straight into Aria's vault (the `salon-custom-tools-flow` recipe),
    // run through the operator verb as Aria, then read where the Salon shows it.
    // Custom Tools must be on for the chat; the run's 2xx is asserted so a
    // refusal names itself instead of timing out below.
    const destinations = (await dispatch({ type: 'customToolsDestinations' })) as {
      data?: { characters?: Array<{ characterId: string; mountPointId: string }> };
    };
    const vault = destinations.data?.characters?.find((c) => c.characterId === ARIA_ID)?.mountPointId;
    expect(vault, 'Aria’s vault should be a destination').toBeTruthy();
    const definition = {
      $schema: '/schemas/qtap-custom-tool.schema.json',
      name: 'e2e_announced_roll',
      title: 'Announced Roll',
      description: 'A roll Aria announces herself.',
      announceAsCharacter: true,
      outcomes: [{ when: true, message: `${CHARACTER_PLACEHOLDER} rolls the bones.`, state: 'info' }],
    };
    await dispatch({
      type: 'mountFileWrite',
      mountPointId: vault,
      path: 'Tools/e2e_announced_roll.tool.json',
      content: `${JSON.stringify(definition, null, 2)}\n`,
    });
    const run = (await dispatch({
      type: 'chatCustomToolRun',
      chatId: SOLO_VOYAGE_CHAT_ID,
      tool: 'e2e_announced_roll',
      asCharacterId: ARIA_ID,
    })) as { type?: string; data?: unknown };
    expect(run.type, JSON.stringify(run)).not.toBe('error');

    await page.goto(`/salon/${SOLO_VOYAGE_CHAT_ID}`);
    await maybeUnlock(page);
    const bar = page.locator('.qt-chat-system-bar', { hasText: 'Announced Roll' }).last();
    await expect(bar).toBeVisible({ timeout: 15_000 });
    await expect(bar.locator('.qt-chat-system-bar-sender')).toHaveText('Aria');
    await expect(page.getByText('Aria rolls the bones.').last()).toBeVisible();
  });
});

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

async function dispatch(body: Record<string, unknown>): Promise<unknown> {
  const res = await fetch(`${WP_BASE_URL}/api/dispatch`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  return res.json();
}

/** Unlock only when the passphrase screen is showing. */
async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  await expect(passphrase.or(page.locator('qt-shell')).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
    await expect(page.locator('qt-shell')).toBeVisible({ timeout: 15_000 });
  }
}

/** The Workbench, a fresh contrivance's editor open in form mode. */
async function newContrivance(page: Page): Promise<void> {
  await page.goto('/custom-tools');
  await maybeUnlock(page);
  await page.getByRole('button', { name: 'New contrivance', exact: true }).click();
  await expect(page.getByText('The contrivance itself')).toBeVisible({ timeout: 15_000 });
}

/** Enough identity + one tested outcome that the draft validates (the `workbench-flow` recipe). */
async function fillValidIdentity(page: Page, title: string, name: string): Promise<void> {
  await page.locator('#wb-title').fill(title);
  await page.locator('#wb-name').fill(name);
  await page.locator('#wb-description').fill('A contrivance authored by the P4.D320 walk.');
  await page.getByRole('button', { name: 'add condition' }).first().click();
  await page.getByLabel('Operand number').first().fill('0.5');
  await page.getByLabel('Outcome message').first().fill('The air shimmers.');
}

/** One wardrobe effect row (wear, one blank item). */
async function addWardrobeEffect(page: Page): Promise<void> {
  await page.getByRole('button', { name: /Add effect/ }).click();
  await page.getByLabel('Effect kind').selectOption('wardrobe');
  await expect(page.getByLabel('Wardrobe action')).toBeVisible();
}

/** Pick a vantage by hand through the Side-effects card's select. */
async function chooseVantage(page: Page, characterName: string): Promise<void> {
  const select = page.locator('qt-side-effects-section').getByLabel('Whose wardrobe to browse').first();
  await select.selectOption({ label: characterName });
  await expect(page.locator('qt-side-effects-section').getByText(`Browsing ${characterName}’s wardrobe`)).toBeVisible();
}

/** Save into a character's vault through the destination picker. */
async function saveInto(page: Page, characterName: string): Promise<void> {
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Where shall Pascal keep this contrivance?' })).toBeVisible({
    timeout: 15_000,
  });
  await page.getByRole('dialog').locator('.qt-badge', { hasText: new RegExp(`^${characterName}$`) }).click();
  await page.getByRole('button', { name: 'Keep it here' }).click();
  await expect(page.getByText('Pascal has filed the contrivance.')).toBeVisible({ timeout: 15_000 });
}

function runCliWrite(cli: string, sql: string): void {
  const res = spawnSync(cli, ['db', '--data-dir', WP_INSTANCE_DIR, '--write', sql], {
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
      const res = await fetch(`${WP_BASE_URL}/health`);
      // 423 = up, instance locked (the server gets no passphrase env).
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${WP_PORT}: ${lastErr}`);
}
