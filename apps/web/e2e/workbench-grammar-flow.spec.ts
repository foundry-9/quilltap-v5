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
  REPO_ROOT,
  FIXTURES_DIR,
  SINGLE_USER_ID,
  spaDir,
  TEST_PEPPER,
  webBinary,
} from './support/env';
import { MOCK_LLM_REPLY, startMockLlm, type MockLlm } from './support/mock-llm';

/**
 * P4.D334 — the Workbench's grammar (v4 `a53024d3c` any-of lines + the
 * aggregate key, `322a5488e` the outcome brief, `e1872c94f` the Inventory,
 * `e4a60ad9e` the Log), driven in a real browser against a real server.
 *
 * Its OWN instance on port 4349 (mock LLM 45347 — the round's §R.9 ports for
 * this lane), booted on the `salon-main.db` / `salon-mount.db` pair exactly as
 * `workbench-triggers-effects-flow.spec.ts` does (the slim pascal pair cannot
 * boot the Salon).
 *
 * **LIVE beats** (a)–(g) read the **What Save would write** preview — the
 * bench's "exact bytes" card, `definitionFromDraft` rendered client-side with no
 * parse and no server — so the draft model is proven end to end in the browser
 * without any sibling lane.
 *
 * **GATED beats** (h)–(m) need a sibling's code: the schema parse (Save and the
 * library round trip, the bench's gate evaluation), the Inventory server wires
 * (the mock bag, the dry `→ inventory` / `→ log` lines) or the Inventory
 * reference picker. NAMED CONSTANTS, `false` until the unifier flips them at
 * §S.9 (R-V), redeclared here because the Playwright config does not compile
 * `src/` — the same names as `src/testing/p4d334-gates.ts`.
 */
const P4D333_SPA_LANDED = false;
const P4D329_SERVER_LANDED = false;
const P4D331_SERVER_LANDED = false;
const P4D330_SPA_LANDED = false;
const P4D333_REASON = 'awaits P4.D333’s schema twin (the parse behind Save, the library and the bench’s gate) — wired at unification';
const P4D329_REASON = 'awaits P4.D329’s `inventory` body on customToolPreview and the dry `inventory` effect — wired at unification';
const P4D331_REASON = 'awaits P4.D331’s dry `{ index, log }` effect — wired at unification';
const P4D330_REASON = 'awaits P4.D330’s Inventory reference picker, loader and the Inventory server lanes — wired at unification';

const GP_PORT = 4349;
const GP_BASE_URL = `http://127.0.0.1:${GP_PORT}`;
/** This spec's own mock LLM — clear of every shared port. */
const GP_MOCK_LLM_PORT = 45347;
const GP_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'workbench-grammar-instance');
const GP_DATA_DIR = resolve(GP_INSTANCE_DIR, 'data');
const GP_SERVER_LOG = resolve(ARTIFACTS_DIR, 'workbench-grammar-server.log');

const MAIN_FIXTURE = resolve(FIXTURES_DIR, 'salon-main.db');
const MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'salon-mount.db');

let server: ChildProcess | undefined;
let mock: MockLlm | undefined;

test.use({ baseURL: GP_BASE_URL });

test.describe('P4.D334 — any-of lines, briefs, the Inventory and the Log in the Workbench', () => {
  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(GP_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(GP_DATA_DIR, { recursive: true });
    mkdirSync(resolve(GP_DATA_DIR, 'files'), { recursive: true });
    copyFileSync(MAIN_FIXTURE, resolve(GP_DATA_DIR, 'quilltap.db'));
    if (existsSync(MOUNT_FIXTURE)) {
      copyFileSync(MOUNT_FIXTURE, resolve(GP_DATA_DIR, 'quilltap-mount-index.db'));
    }
    writeFileSync(resolve(GP_DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));
    // The fixture's user-scoped rows belong to FIXTURE_USER; the server acts as
    // SINGLE_USER_ID (the `scenario-browser-flow` rewrite, verbatim).
    for (const table of [
      'chats',
      'connection_profiles',
      'api_keys',
      'chat_settings',
      'characters',
      'tags',
      'projects',
      'memories',
      'files',
      'image_profiles',
      'groups',
    ]) {
      runCliWrite(cli, `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`);
    }
    runCliWrite(cli, `UPDATE users SET id = '${SINGLE_USER_ID}' WHERE id = '${FIXTURE_USER}';`);
    runCliWrite(
      cli,
      `UPDATE connection_profiles SET baseUrl = 'http://127.0.0.1:${GP_MOCK_LLM_PORT}/v1' WHERE provider = 'OPENAI_COMPATIBLE';`,
    );
    mock = await startMockLlm(MOCK_LLM_REPLY, GP_MOCK_LLM_PORT);

    // The salon pair carries NO `instance_settings` table; the boot's "Quilltap
    // General" provisioning records its mount there, and the Workbench's save
    // picker offers that store. Planted from the committed D23 dump, never
    // hand-written (the `scenario-browser-flow` precedent).
    const freshSchema = JSON.parse(
      readFileSync(
        resolve(REPO_ROOT, 'crates/quilltap-core/src/services/provisioning/fresh_schema.json'),
        'utf8',
      ),
    ) as { main: string[]; mountIndex: string[] };
    const settingsDdl = freshSchema.main.find((sql) => sql.startsWith('CREATE TABLE "instance_settings"'));
    if (!settingsDdl) throw new Error('fresh_schema.json carries no instance_settings CREATE TABLE');
    const ddl = spawnSync(cli, ['db', '--data-dir', GP_INSTANCE_DIR, '--write', settingsDdl], {
      env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
      encoding: 'utf8',
    });
    if (ddl.status !== 0 && !`${ddl.stdout}${ddl.stderr}`.includes('already exists')) {
      throw new Error(`instance_settings plant failed:\n${ddl.stdout}\n${ddl.stderr}`);
    }
    // The pair also predates the groups schema, and the save picker's
    // destinations verb reads `groups` (main) and `group_doc_mount_links`
    // (mount index) on every request — absent, the picker answers `no such
    // table: groups` and offers nothing (measured on this spec's first gated
    // run). Global setup plants both on the SHARED copy; this spec's own copy
    // needs them too. Empty tables are the honest state; the DDL is the dump's.
    for (const [partition, table] of [
      ['main', 'groups'],
      ['mountIndex', 'group_doc_mount_links'],
    ] as const) {
      const create = freshSchema[partition].find((sql) => sql.startsWith(`CREATE TABLE "${table}"`));
      if (!create) throw new Error(`fresh_schema.json carries no ${table} CREATE TABLE`);
      const args = ['db', '--data-dir', GP_INSTANCE_DIR, '--write', create.replace('CREATE TABLE', 'CREATE TABLE IF NOT EXISTS')];
      if (partition === 'mountIndex') args.splice(1, 0, '--mount-points');
      const res = spawnSync(cli, args, {
        env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
        encoding: 'utf8',
      });
      if (res.status !== 0) throw new Error(`${table} plant failed:\n${res.stdout}\n${res.stderr}`);
    }

    const logFd = openSync(GP_SERVER_LOG, 'w');
    server = spawn(
      web,
      ['--host', '127.0.0.1', '--port', String(GP_PORT), '--data-dir', GP_INSTANCE_DIR, '--spa-dir', spaDir()],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    server.unref();
    await waitForHealth();
    await fetch(`${GP_BASE_URL}/api/dispatch`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ type: 'unlock', passphrase: E2E_PASSPHRASE }),
    });
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
    rmSync(GP_INSTANCE_DIR, { recursive: true, force: true });
  });

  /** What Save would write — the bench's "exact bytes" card, whitespace-collapsed. */
  const preview = (page: Page) => page.locator('qt-proving-bench pre');
  const previewText = async (page: Page) => ((await preview(page).textContent()) ?? '').replace(/\s+/g, ' ');

  // -------------------------------------------------------------------------
  // (a) LIVE — two gate chips joined by OR
  // -------------------------------------------------------------------------
  test('(a) a gate toggled to OR writes an any-of array, and back to one object', async ({ page }) => {
    await newContrivance(page);
    const gate = page.locator('qt-gate-section');
    await gate.getByRole('radio', { name: 'Only show if…' }).click();
    await gate.getByRole('button', { name: 'add condition' }).click();
    await gate.getByLabel('Metadata key').nth(0).fill('a');
    await gate.getByRole('button', { name: 'add condition' }).click();
    await gate.getByLabel('Metadata key').nth(1).fill('b');
    await expect(preview(page)).toContainText('"availableWhen": {');

    const toggle = gate.getByRole('button', { name: 'Joined by AND — switch to OR' });
    await expect(toggle).toHaveText('AND');
    await toggle.click();
    await expect(gate.getByRole('button', { name: 'Joined by OR — switch to AND' })).toHaveText('OR');
    await expect(preview(page)).toContainText('"availableWhen": [');
    // The hairline between the two lines.
    await expect(gate.locator('qt-condition-groups > div.border-t')).toHaveCount(1);

    await gate.getByRole('button', { name: 'Joined by OR — switch to AND' }).click();
    await expect(preview(page)).toContainText('"availableWhen": {');
    await expect(gate.locator('qt-condition-groups > div.border-t')).toHaveCount(0);
  });

  // -------------------------------------------------------------------------
  // (b) LIVE — an outcome row with two lines
  // -------------------------------------------------------------------------
  test('(b) an outcome row with two lines writes an array; a repeat is fine across lines, not within one', async ({ page }) => {
    await newContrivance(page);
    const row = page.locator('qt-outcome-row').first();
    await row.getByRole('button', { name: 'add condition' }).click();
    await row.getByRole('button', { name: 'add condition' }).click();
    await row.getByLabel('Operand number').nth(0).fill('0.2');
    await row.getByLabel('Operand number').nth(1).fill('0.4');
    await expect(preview(page)).toContainText('"when": {');

    await row.getByRole('button', { name: 'Joined by AND — switch to OR' }).click();
    await expect(preview(page)).toContainText('"when": [');
    // Line 2 takes line 1's slot (Value, ≥): accepted — it is another line.
    await row.getByLabel('Comparator').nth(1).selectOption('gte');
    await expect(row.getByText('A line can test')).toHaveCount(0);

    // Fold it back into one line: the same pair is now a repeat on one line.
    await row.getByRole('button', { name: 'Joined by OR — switch to AND' }).click();
    await expect(page.getByText('a line can test this subject with this comparator only once')).toBeVisible();
  });

  // -------------------------------------------------------------------------
  // (c) LIVE — the brief
  // -------------------------------------------------------------------------
  test('(c) with the consult on each row offers a brief, written last and only when non-blank', async ({ page }) => {
    await newContrivance(page);
    const brief = page.getByLabel('Outcome brief for the LLM consult');
    await expect(brief).toHaveCount(0);
    await page.getByLabel('Consult an LLM').check();
    await expect(brief).toHaveCount(2);

    await brief.first().fill('Confirmed at depth.');
    await expect.poll(async () => previewText(page)).toMatch(/"brief": "Confirmed at depth\."\s*\}/);
    await brief.first().fill('');
    await expect.poll(async () => previewText(page)).not.toContain('"brief"');

    // A prompt that asks the model to do the arithmetic earns the advisory.
    await page.locator('#wb-llm-prompt').fill('If {{roll}} >= {{params.layers}}, report success.');
    await expect(page.getByText(/this prompt asks the model to compare numbers/)).toBeVisible();
  });

  // -------------------------------------------------------------------------
  // (d) LIVE — a log line
  // -------------------------------------------------------------------------
  test('(d) Enter a log line swaps in the textarea and writes { log }', async ({ page }) => {
    await newContrivance(page);
    await page.getByRole('button', { name: /Add effect/ }).click();
    await page.getByLabel('Effect kind').selectOption('log');
    const line = page.getByLabel('Log line');
    await expect(line).toBeVisible();
    await expect(page.getByLabel('Effect target')).toHaveCount(0);
    await expect(page.getByText(/The line is entered in the ledger/)).toBeVisible();
    await expect(page.getByText('a log line needs some prose to write down')).toBeVisible();

    await line.fill('The lock gave.');
    await expect.poll(async () => previewText(page)).toContain('{ "log": "The lock gave." }'.replace('{ ', '{ '));
    await line.fill('   ');
    await expect(page.getByText('a log line needs some prose to write down')).toBeVisible();
  });

  // -------------------------------------------------------------------------
  // (e) LIVE — an inventory effect
  // -------------------------------------------------------------------------
  test('(e) Change the inventory: pick up without equipping writes equip false, and the vantage reads inventory', async ({ page }) => {
    await newContrivance(page);
    await page.getByRole('button', { name: /Add effect/ }).click();
    await page.getByLabel('Effect kind').selectOption('inventory');
    await expect(page.getByLabel('Inventory action')).toBeVisible();
    await expect(page.getByLabel('Whose inventory to browse')).toBeVisible();
    await expect(page.getByLabel('Whose wardrobe to browse')).toHaveCount(0);

    await page.getByLabel('Inventory action').selectOption('pick_up');
    await page.getByLabel('Object 1').fill('magic bow');
    const equip = page.getByLabel('equip it at once');
    await expect(equip).toBeChecked();
    await equip.uncheck();
    await expect
      .poll(async () => previewText(page))
      .toMatch(/"inventory": \{ "action": "pick_up", "items": \[ "magic bow" \], "equip": false \}/);
  });

  // -------------------------------------------------------------------------
  // (f) LIVE — the object roll
  // -------------------------------------------------------------------------
  test('(f) From an object writes a $inventory roll', async ({ page }) => {
    await newContrivance(page);
    await page.getByRole('radio', { name: 'From an object' }).click();
    await page.getByLabel('Object and metadata key').fill('sword.metadata.damage');
    await page.getByLabel('Fallback dice').fill('1d4');
    await expect
      .poll(async () => previewText(page))
      .toContain('"roll": { "$inventory": "sword.metadata.damage", "fallback": "1d4" }');
  });

  // -------------------------------------------------------------------------
  // (g) LIVE — the Inventory subjects on a gate chip and an outcome chip
  // -------------------------------------------------------------------------
  test('(g) an inventory state gate chip offers four states and = / ≠; an aggregate chip fills its key', async ({ page }) => {
    await newContrivance(page);
    const gate = page.locator('qt-gate-section');
    await gate.getByRole('radio', { name: 'Only show if…' }).click();
    await gate.getByRole('button', { name: 'add condition' }).click();
    await gate.getByLabel('Gate subject').selectOption('inventory');
    await expect(gate.getByLabel('Inventory state').locator('option')).toHaveText(['equipped', 'carried', 'reachable', 'absent']);
    await expect(gate.getByLabel('Comparator').locator('option')).toHaveText(['=', '≠']);
    await gate.getByLabel('Inventory object reference').fill('Excalibur');
    await expect.poll(async () => previewText(page)).toContain('"inventory": { "Excalibur": { "eq": "equipped" } }');

    const row = page.locator('qt-outcome-row').first();
    await row.getByRole('button', { name: 'add condition' }).click();
    await row.getByLabel('Condition subject').selectOption('inventoryAggregate');
    await row.getByLabel('Inventory aggregate key').fill('owned.metadata.toolVersion');
    await row.getByLabel('Insert an aggregate key').selectOption('owned.metadata.<key>.max');
    await expect(row.getByLabel('Inventory aggregate key')).toHaveValue('owned.metadata.toolVersion.max');
  });

  // -------------------------------------------------------------------------
  // (h) GATED P4D333 — Save, reopen, every line / brief / kind intact
  // -------------------------------------------------------------------------
  test('(h) a saved any-of + brief + log + inventory tool reopens with everything intact', async ({ page }) => {
    test.skip(!P4D333_SPA_LANDED, P4D333_REASON);
    await newContrivance(page);
    await fillValidIdentity(page, 'E2E Grammar Contrivance', 'e2e_grammar_contrivance');
    const gate = page.locator('qt-gate-section');
    await gate.getByRole('radio', { name: 'Only show if…' }).click();
    for (const key of ['a', 'b']) {
      await gate.getByRole('button', { name: 'add condition' }).click();
      await gate.getByLabel('Metadata key').last().fill(key);
    }
    await gate.getByRole('button', { name: 'Joined by AND — switch to OR' }).click();
    await page.getByLabel('Consult an LLM').check();
    await page.locator('#wb-llm-prompt').fill('Say {{outcome.brief}}.');
    await page.getByLabel('Outcome brief for the LLM consult').first().fill('It opened.');
    await page.getByLabel('Outcome brief for the LLM consult').last().fill('It stuck.');
    await page.getByRole('button', { name: /Add effect/ }).click();
    await page.getByLabel('Effect kind').selectOption('log');
    await page.getByLabel('Log line').fill('Opened.');
    await page.getByRole('button', { name: /Add effect/ }).click();
    await page.getByLabel('Effect kind').last().selectOption('inventory');
    await page.getByLabel('Object 1').fill('Excalibur');

    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await page.getByRole('dialog').locator('.qt-badge').first().click();
    await page.getByRole('button', { name: 'Keep it here' }).click();
    await expect(page.getByText('Pascal has filed the contrivance.')).toBeVisible({ timeout: 15_000 });

    await page.getByRole('button', { name: 'Library' }).click();
    await page.getByText('E2E Grammar Contrivance').click();
    await expect(page.locator('qt-gate-section qt-joiner-toggle')).toHaveCount(1);
    await expect(page.getByLabel('Outcome brief for the LLM consult').first()).toHaveValue('It opened.');
    await expect(page.getByLabel('Log line')).toHaveValue('Opened.');
    await expect(page.getByLabel('Object 1')).toHaveValue('Excalibur');
  });

  // -------------------------------------------------------------------------
  // (i) GATED P4D333 — the Mock inventory card against an any-of aggregate gate
  // -------------------------------------------------------------------------
  test('(i) the Mock inventory card names the line that opened the gate, and refuses bad JSON', async ({ page }) => {
    test.skip(!P4D333_SPA_LANDED, P4D333_REASON);
    await newContrivance(page);
    const gate = page.locator('qt-gate-section');
    await gate.getByRole('radio', { name: 'Only show if…' }).click();
    await gate.getByRole('button', { name: 'add condition' }).click();
    await gate.getByLabel('Metadata key').fill('never');
    await gate.getByRole('button', { name: 'add condition' }).click();
    await gate.getByLabel('Gate subject').last().selectOption('inventoryAggregate');
    await gate.getByLabel('Inventory aggregate key').fill('equipped.metadata.toolVersion.max');
    await gate.getByLabel('Comparator').last().selectOption('gte');
    await gate.getByLabel('Operand number').last().fill('4');
    await gate.getByRole('button', { name: 'Joined by AND — switch to OR' }).click();

    await page.getByRole('button', { name: /add object/ }).click();
    await page.getByLabel('Mock object reference').fill('resonator');
    await page.getByLabel('Mock object metadata (JSON object)').fill('{"toolVersion": ');
    await expect(page.getByText('resonator: metadata is not valid JSON')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Roll', exact: true })).toBeDisabled();
    await page.getByLabel('Mock object metadata (JSON object)').fill('{"toolVersion": 4}');
    await expect(page.getByText(/passed on line 2/)).toBeVisible();
  });

  // -------------------------------------------------------------------------
  // (j)/(k) GATED P4D333 + P4D329 — a bench roll over an inventory outcome
  // -------------------------------------------------------------------------
  test('(j)(k) a mock sword row wins the inventory row, the body carries it, and the dry line reads equip', async ({ page }) => {
    test.skip(!P4D333_SPA_LANDED, P4D333_REASON);
    test.skip(!P4D329_SERVER_LANDED, P4D329_REASON);
    await newContrivance(page);
    await fillValidIdentity(page, 'E2E Inventory Roll', 'e2e_inventory_roll');
    const row = page.locator('qt-outcome-row').first();
    await row.getByLabel('Condition subject').first().selectOption('inventory');
    await row.getByLabel('Inventory object reference').fill('sword');
    await page.getByRole('button', { name: /Add effect/ }).click();
    await page.getByLabel('Effect kind').selectOption('inventory');
    await page.getByLabel('Object 1').fill('Excalibur');
    await page.getByRole('button', { name: /add object/ }).click();
    await page.getByLabel('Mock object reference').fill('sword');
    const request = page.waitForRequest((r) => (r.postData() ?? '').includes('customToolPreview'));
    await page.getByRole('button', { name: 'Roll', exact: true }).click();
    expect((await request).postData()).toContain('"inventory":[{"ref":"sword","state":"equipped"}]');
    await expect(page.getByText('→ inventory: equip “Excalibur” (would attempt)')).toBeVisible();
  });

  // -------------------------------------------------------------------------
  // (l) GATED P4D331 — the dry log line
  // -------------------------------------------------------------------------
  test('(l) the bench prints → log: "…" (would write)', async ({ page }) => {
    test.skip(!P4D333_SPA_LANDED, P4D333_REASON);
    test.skip(!P4D331_SERVER_LANDED, P4D331_REASON);
    await newContrivance(page);
    await fillValidIdentity(page, 'E2E Log Roll', 'e2e_log_roll');
    await page.getByRole('button', { name: /Add effect/ }).click();
    await page.getByLabel('Effect kind').selectOption('log');
    await page.getByLabel('Log line').fill('The lock gave.');
    await page.getByRole('button', { name: 'Roll', exact: true }).click();
    await expect(page.getByText('→ log: "The lock gave." (would write)')).toBeVisible();
  });

  // -------------------------------------------------------------------------
  // (m) GATED P4D330 — Choose… opens the Inventory picker
  // -------------------------------------------------------------------------
  test('(m) Choose… opens Choose an object, a pick writes the label, the chip renders and by id flips it', async ({ page }) => {
    test.skip(!P4D330_SPA_LANDED, P4D330_REASON);
    await newContrivance(page);
    await page.getByRole('button', { name: /Add effect/ }).click();
    await page.getByLabel('Effect kind').selectOption('inventory');
    await page.getByRole('button', { name: 'Choose…' }).click();
    await expect(page.getByRole('heading', { name: 'Choose an object' })).toBeVisible();
    await page.getByTestId('inventory-reference-picker-list').getByRole('button').first().click();
    await expect(page.getByTestId('inventory-reference-chip')).toBeVisible();
    await page.getByRole('button', { name: 'by name' }).click();
    await expect(page.getByRole('button', { name: 'by id' })).toBeVisible();
  });
});

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

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
  await page.locator('#wb-description').fill('A contrivance authored by the P4.D334 walk.');
  await page.getByRole('button', { name: 'add condition' }).first().click();
  await page.getByLabel('Operand number').first().fill('0.5');
  await page.getByLabel('Outcome message').first().fill('The air shimmers.');
}

function runCliWrite(cli: string, sql: string): void {
  const res = spawnSync(cli, ['db', '--data-dir', GP_INSTANCE_DIR, '--write', sql], {
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
      const res = await fetch(`${GP_BASE_URL}/health`);
      // 423 = up, instance locked (the server gets no passphrase env).
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${GP_PORT}: ${lastErr}`);
}
