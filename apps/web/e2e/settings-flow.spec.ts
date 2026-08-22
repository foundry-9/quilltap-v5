import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, openSync, rmSync, writeFileSync } from 'node:fs';
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
import { startMockLlm, type MockLlm } from './support/mock-llm';

/**
 * The LIVE Settings first-run e2e: a fresh instance walks setup → the provider
 * wizard → a validated OPENAI_COMPATIBLE key + profile against the e2e mock LLM →
 * the configured profile appears in the Providers tab.
 *
 * Live since the P4.6 unification wired the sibling lane's dispatch variants
 * (`providerList` / `apiKeyCreate` / `connectionProfileTest` / `modelFetch` /
 * `connectionProfileCreate` / `chatSettingsUpdate` — P4.6d handlers + the
 * unification's provider-actions driver). The mock serves both `GET /models`
 * (validate + models list) and streaming `POST /chat/completions`.
 */
const SETTINGS_PORT = 4321;
const SETTINGS_MOCK_PORT = 45302;
const SETTINGS_URL = `http://127.0.0.1:${SETTINGS_PORT}`;
const SETTINGS_INSTANCE = resolve(ARTIFACTS_DIR, 'settings-instance');
const SETTINGS_LOG = resolve(ARTIFACTS_DIR, 'settings-server.log');
const SETTINGS_PASSPHRASE = 'settings vertical passphrase';

let serverPid: number | undefined;
let mock: MockLlm;

test.describe('Settings vertical (fresh instance → wizard → configured profile)', () => {
  test.beforeAll(async () => {
    rmSync(SETTINGS_INSTANCE, { recursive: true, force: true });
    mkdirSync(resolve(SETTINGS_INSTANCE, 'data'), { recursive: true });
    mock = await startMockLlm(undefined, SETTINGS_MOCK_PORT);

    const web = webBinary();
    if (!existsSync(web)) {
      throw new Error(`Missing quilltap-web binary at ${web} — cargo build -p quilltap-web first`);
    }
    const env = { ...process.env };
    delete env['ENCRYPTION_MASTER_PEPPER'];
    const logFd = openSync(SETTINGS_LOG, 'w');
    const child = spawn(
      web,
      [
        '--host',
        '127.0.0.1',
        '--port',
        String(SETTINGS_PORT),
        '--data-dir',
        SETTINGS_INSTANCE,
        '--spa-dir',
        spaDir(),
      ],
      { stdio: ['ignore', logFd, logFd], detached: true, env },
    );
    child.unref();
    serverPid = child.pid;

    const deadline = Date.now() + 30_000;
    for (;;) {
      try {
        const res = await fetch(`${SETTINGS_URL}/health`);
        if (res.status === 423 || res.status === 200) break;
      } catch {
        // not up yet
      }
      if (Date.now() > deadline) {
        throw new Error(`settings server did not become ready within 30s; see ${SETTINGS_LOG}`);
      }
      await new Promise((r) => setTimeout(r, 300));
    }
  });

  test.afterAll(async () => {
    await mock?.close();
    if (serverPid !== undefined) {
      try {
        process.kill(-serverPid, 'SIGTERM');
      } catch {
        try {
          process.kill(serverPid, 'SIGTERM');
        } catch {
          // already gone
        }
      }
    }
    rmSync(SETTINGS_INSTANCE, { recursive: true, force: true });
  });

  async function completeSetup(page: Page): Promise<void> {
    await page.goto(SETTINGS_URL);
    await expect(page.getByRole('heading', { name: 'Welcome to Quilltap' })).toBeVisible();
    await page.locator('#qt-setup-pass').fill(SETTINGS_PASSPHRASE);
    await page.locator('#qt-setup-confirm').fill(SETTINGS_PASSPHRASE);
    await page.getByRole('button', { name: 'Set Up with Passphrase' }).click();
    await expect(page.getByRole('heading', { name: 'Setup Complete' })).toBeVisible();
    await page.getByRole('button', { name: 'Continue to Quilltap' }).click();
    // The fresh instance hands off to the provider wizard.
    await expect(page.getByRole('heading', { name: 'Choose Your Providers' })).toBeVisible();
  }

  test('setup → wizard → validated key + model → save → profile in the Providers tab → an OAC key attached (bug 81)', async ({
    page,
  }) => {
    await completeSetup(page);

    // Step 1: pick the OpenAI-Compatible provider (requires a base URL we can
    // point at the mock).
    await page.getByRole('button', { name: /OpenAI-Compatible/i }).click();
    await page.getByRole('button', { name: 'Next' }).click();

    // Step 2: OPENAI_COMPATIBLE's key is optional (`requiresApiKey: false`) so
    // the wizard renders no key input — just the base URL, pointed at the mock,
    // then validate (a live `connectionProfileTest` → the mock's `GET /models`).
    await expect(page.getByRole('heading', { name: 'Configure API Keys' })).toBeVisible();
    await page.getByPlaceholder('http://localhost:8080/v1').fill(mock.url);
    await page.getByRole('button', { name: 'Validate' }).click();
    await expect(page.getByText('Connection validated successfully.')).toBeVisible();
    await page.getByRole('button', { name: 'Next' }).click();

    // Step 3: the mock's models are fetched; select one.
    await expect(page.getByRole('heading', { name: 'Choose Your Models' })).toBeVisible();
    await page.getByRole('combobox').first().selectOption('mock-model');
    await page.getByRole('button', { name: 'Next' }).click();

    // Skip the optional embedding + image steps.
    await page.getByRole('button', { name: 'Skip' }).click();
    await page.getByRole('button', { name: 'Skip' }).click();

    // Confirm: save & complete.
    await expect(page.getByRole('heading', { name: 'Review & Confirm' })).toBeVisible();
    await page.getByRole('button', { name: 'Save & Complete' }).click();
    await expect(page.getByText('Configuration saved successfully.')).toBeVisible();
    await page.getByRole('button', { name: 'Continue to Quilltap' }).click();

    // The Providers tab now lists the configured profile.
    await page.goto(`${SETTINGS_URL}/settings?tab=providers&section=connection-profiles`);
    await expect(
      page.getByRole('heading', { name: 'OpenAI-Compatible - mock-model' }),
    ).toBeVisible();

    // -----------------------------------------------------------------------
    // P4.D93 (v4 bug 81) — that same OpenAI-Compatible profile can now hold an
    // API key.
    //
    // `requiresApiKey: false` used to remove the provider from the
    // Add-New-API-Key list AND the key field from the profile form, so a hosted
    // OpenAI-compatible endpoint behind a bearer token could not be configured
    // at all. Continues in this beat rather than standing alone because it needs
    // exactly what the wizard just produced: a saved OAC profile to attach to.
    //
    // The wire-level proof — a real hosted endpoint answering 200 instead of 401
    // — is a dogfood item, not a beat.
    // -----------------------------------------------------------------------
    await page.goto(`${SETTINGS_URL}/settings?tab=providers&section=api-keys`);
    await page.getByRole('button', { name: '+ Add API Key' }).click();

    // The provider list is filtered on "may hold a key", so OAC is offered.
    const provider = page.locator('#qt-key-provider');
    await expect(provider).toBeVisible();
    await expect(provider.locator('option[value="OPENAI_COMPATIBLE"]')).toHaveCount(1);
    // …and Ollama, which takes no key at all, still is not.
    await expect(provider.locator('option[value="OLLAMA"]')).toHaveCount(0);

    await page.locator('#qt-key-label').fill('Hosted OAC key');
    await provider.selectOption('OPENAI_COMPATIBLE');
    await page.locator('#qt-key-value').fill('sk-e2e-not-a-real-secret');
    await page.getByRole('button', { name: 'Create API Key' }).click();
    await expect(page.getByText('Hosted OAC key')).toBeVisible();

    // The profile form now offers the key field — UNSTARRED, with the optional
    // placeholder — and holds what is chosen.
    await page.goto(`${SETTINGS_URL}/settings?tab=providers&section=connection-profiles`);
    await page.getByRole('button', { name: 'Edit' }).first().click();
    const keySelect = page.locator('#qt-pf-key');
    await expect(keySelect).toBeVisible();
    await expect(page.locator('label[for="qt-pf-key"]')).toHaveText('API Key');
    await expect(keySelect.locator('option').first()).toHaveText(
      'None — the endpoint needs no key',
    );

    await keySelect.selectOption({ label: 'Hosted OAC key' });
    await page.getByRole('button', { name: 'Update Profile' }).click();
    await expect(keySelect).toBeHidden();

    // Re-open: the attachment survived the round trip to the server.
    await page.getByRole('button', { name: 'Edit' }).first().click();
    await expect(page.locator('#qt-pf-key')).toBeVisible();
    await expect(page.locator('#qt-pf-key option:checked')).toHaveText('Hosted OAC key');
  });
});

// ---------------------------------------------------------------------------
// P4.6r — the Templates & Prompts + Images settings verticals. Authored
// pre-unification against lane A's extended groups-projects fixture (which gains
// user roleplay templates + image profiles); the describe SKIPS when that
// fixture is absent (this worktree) and auto-activates once lane A lands it +
// the `roleplayTemplate*` / `imageProfile*` dispatch variants at unification.
// ---------------------------------------------------------------------------

const TMPL_PORT = 4326;
const TMPL_BASE_URL = `http://127.0.0.1:${TMPL_PORT}`;
const TMPL_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'settings-templates-instance');
const TMPL_DATA_DIR = resolve(TMPL_INSTANCE_DIR, 'data');
const TMPL_SERVER_LOG = resolve(ARTIFACTS_DIR, 'settings-templates-server.log');

const GP_MAIN_FIXTURE = resolve(FIXTURES_DIR, 'groups-projects-main.db');
const GP_MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'groups-projects-mount.db');
const GP_FIXTURE_READY = existsSync(GP_MAIN_FIXTURE);
const TMPL_USER_TABLES = ['roleplayTemplates', 'imageProfiles', 'apiKeys', 'characters', 'tags'];

let tmplServer: ChildProcess | undefined;

test.describe('P4.6r — Templates & Images settings verticals', () => {
  test.skip(!GP_FIXTURE_READY, 'awaits lane A groups-projects fixture (wired at unification)');

  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(TMPL_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(TMPL_DATA_DIR, { recursive: true });
    copyFileSync(GP_MAIN_FIXTURE, resolve(TMPL_DATA_DIR, 'quilltap.db'));
    if (existsSync(GP_MOUNT_FIXTURE)) {
      copyFileSync(GP_MOUNT_FIXTURE, resolve(TMPL_DATA_DIR, 'quilltap-mount-index.db'));
    }
    writeFileSync(
      resolve(TMPL_DATA_DIR, 'quilltap.dbkey'),
      makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE),
    );
    for (const table of TMPL_USER_TABLES) {
      const res = spawnSync(
        cli,
        [
          'db',
          '--data-dir',
          TMPL_INSTANCE_DIR,
          '--write',
          `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`,
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
      if (res.status !== 0) {
        const out = `${res.stdout}${res.stderr}`;
        if (!out.includes('no such table') && !out.includes('no such column: userId')) {
          throw new Error(`fixture rewrite failed (${table}):\n${res.stdout}\n${res.stderr}`);
        }
      }
    }

    const logFd = openSync(TMPL_SERVER_LOG, 'w');
    tmplServer = spawn(
      web,
      [
        '--host',
        '127.0.0.1',
        '--port',
        String(TMPL_PORT),
        '--data-dir',
        TMPL_INSTANCE_DIR,
        '--spa-dir',
        spaDir(),
      ],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    tmplServer.unref();

    const deadline = Date.now() + 30_000;
    for (;;) {
      try {
        const res = await fetch(`${TMPL_BASE_URL}/health`);
        if (res.status === 423 || res.status === 200) break;
      } catch {
        // not up yet
      }
      if (Date.now() > deadline) {
        throw new Error(`templates settings server not ready within 30s; see ${TMPL_SERVER_LOG}`);
      }
      await new Promise((r) => setTimeout(r, 300));
    }
  });

  test.afterAll(() => {
    if (tmplServer?.pid) {
      try {
        process.kill(-tmplServer.pid, 'SIGTERM');
      } catch {
        try {
          process.kill(tmplServer.pid, 'SIGTERM');
        } catch {
          // already gone
        }
      }
    }
    rmSync(TMPL_INSTANCE_DIR, { recursive: true, force: true });
  });

  async function unlockIfLocked(page: Page, ready: ReturnType<Page['getByRole']>): Promise<void> {
    const passphrase = page.locator('#qt-passphrase');
    await expect(passphrase.or(ready).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.isVisible()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
    await expect(ready).toBeVisible({ timeout: 10_000 });
  }

  test('Templates tab: create → appears → edit → delete a roleplay template', async ({ page }) => {
    test.setTimeout(60_000);
    await page.goto(`${TMPL_BASE_URL}/settings?tab=templates`);
    await unlockIfLocked(page, page.getByRole('heading', { name: 'My Templates' }));

    // Create.
    await page.getByRole('button', { name: 'Create Template', exact: true }).first().click();
    // The qt-template-form-modal HOST has no box of its own (the overlay child
    // is position-fixed), so assert on the ARIA dialog it renders.
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();
    await dialog.getByPlaceholder('My Custom RP Style').fill('Walk Template');
    // The LLM prompt is the qt-markdown-field now (P4.6aq) — a ProseMirror
    // contenteditable, so it takes real key events (fill() targets
    // input/textarea).
    await dialog.locator('.qt-rich-editor-content').first().click();
    await page.keyboard.type('Render narration in a dry, clipped register.');
    await dialog.getByRole('button', { name: 'Create Template', exact: true }).click();
    await expect(dialog).toBeHidden({ timeout: 10_000 });

    // Strict-mode scope: the new template's name also appears as an option in
    // the Default Template selector card (a section.qt-card) — the template
    // cards themselves are div.qt-card.
    const card = page.locator('div.qt-card', { hasText: 'Walk Template' });
    await expect(card).toBeVisible({ timeout: 10_000 });

    // Edit → rename.
    await card.getByRole('button', { name: 'Edit', exact: true }).click();
    const editDialog = page.getByRole('dialog');
    await expect(editDialog).toBeVisible();
    const nameInput = editDialog.getByPlaceholder('My Custom RP Style');
    await nameInput.fill('Walk Template Renamed');
    await editDialog.getByRole('button', { name: 'Save Changes' }).click();
    await expect(editDialog).toBeHidden({ timeout: 10_000 });
    await expect(page.locator('div.qt-card', { hasText: 'Walk Template Renamed' })).toBeVisible({
      timeout: 10_000,
    });

    // Delete (inline confirm).
    const renamed = page.locator('div.qt-card', { hasText: 'Walk Template Renamed' });
    await renamed.getByRole('button', { name: 'Delete', exact: true }).click();
    await renamed.getByRole('button', { name: 'Confirm', exact: true }).click();
    await expect(page.locator('div.qt-card', { hasText: 'Walk Template Renamed' })).toHaveCount(0, {
      timeout: 10_000,
    });
  });

  test('Images tab: the Image Profiles card lists the fixture profiles', async ({ page }) => {
    test.setTimeout(60_000);
    await page.goto(`${TMPL_BASE_URL}/settings?tab=images`);
    await unlockIfLocked(page, page.getByRole('heading', { name: /Image Generation Profiles/ }));
    // The card fetched the image-profiles listing (proves the lane-A variant is
    // live) and rendered the New Profile affordance.
    await expect(page.getByRole('button', { name: 'New Profile' })).toBeVisible({
      timeout: 10_000,
    });
  });

  test('Images tab: the Default Aesthetics card loads both fields and saves the lantern one', async ({
    page,
  }) => {
    test.setTimeout(60_000);

    // ACTIVATED AT UNIFICATION (P4.6ar∥as∥at): the beat runs LIVE over lane A's
    // `systemImageAestheticsGet`/`Set` verbs (the lane-era fulfil-mock was
    // deleted, as the lane's order specified). The route below only RECORDS the
    // save payload — every dispatch continues to the real server — so the §2
    // shape assertion survives activation, and the reload round-trip at the end
    // proves the persistence the mock never could.
    const saves: Array<Record<string, unknown>> = [];
    await page.route('**/api/dispatch', async (route) => {
      const body = route.request().postDataJSON() as { type?: string } | null;
      if (body?.type === 'systemImageAestheticsSet') {
        saves.push(body as Record<string, unknown>);
      }
      return route.fallback();
    });

    // Deep-link the section: a CLOSED collapsible renders NO content, so the
    // card must be force-opened to be walkable at all (the P4.6ap lesson).
    await page.goto(`${TMPL_BASE_URL}/settings?tab=images&section=default-aesthetics`);
    await unlockIfLocked(page, page.getByRole('heading', { name: 'Default Aesthetics' }));

    // Both fields loaded (a fresh instance has no stored file → `{content: ''}`)
    // and the editors mounted, which only happens once the load settles.
    const card = page.locator('qt-default-aesthetics-card');
    const lantern = card.locator('qt-aesthetic-editor-field').first();
    const aurora = card.locator('qt-aesthetic-editor-field').nth(1);
    await expect(lantern).toContainText('Default Image Aesthetic');
    await expect(aurora).toContainText('Default Character Aesthetic');
    await expect(lantern.locator('.qt-rich-editor-content')).toBeVisible({ timeout: 10_000 });

    // The load is not an edit, so Save is unreachable until the user types
    // (v4 AestheticEditorField.tsx:127).
    const save = lantern.getByRole('button', { name: 'Save' });
    await expect(save).toBeDisabled();

    // Type into the lantern field — real key events on the ProseMirror
    // contenteditable (the P4.6ag idiom; no literal markdown chars, which would
    // arrive as escaping artifacts).
    await lantern.locator('.qt-rich-editor-content').click();
    await page.keyboard.type('Muted sepia tones, brass fittings.');
    await expect(save).toBeEnabled();
    await save.click();

    // v4 :131 — the success span lands, and the dispatch carried §2's shape.
    await expect(lantern).toContainText('Saved', { timeout: 10_000 });
    expect(saves).toEqual([
      {
        type: 'systemImageAestheticsSet',
        kind: 'lantern',
        content: 'Muted sepia tones, brass fittings.',
      },
    ]);

    // The reload round-trip (grown at unification — the mock could never prove
    // persistence): the content survives into a fresh load of the page, and an
    // untouched reload leaves Save disabled again (the load is not an edit).
    await page.goto(`${TMPL_BASE_URL}/settings?tab=images&section=default-aesthetics`);
    await expect(page.getByRole('heading', { name: 'Default Aesthetics' })).toBeVisible({
      timeout: 10_000,
    });
    const lanternReloaded = page
      .locator('qt-default-aesthetics-card')
      .locator('qt-aesthetic-editor-field')
      .first();
    await expect(lanternReloaded.locator('.qt-rich-editor-content')).toContainText(
      'Muted sepia tones, brass fittings.',
      { timeout: 10_000 },
    );
    await expect(lanternReloaded.getByRole('button', { name: 'Save' })).toBeDisabled();
  });

  // -------------------------------------------------------------------------
  // P4.D102 — the image Fetch Models control and the NanoGPT provider entries.
  //
  // ACTIVATE-AT-UNIFY. Both beats need a sibling lane's server half and are
  // gated on the constants below (the `P4D97_THINKING_WIRE_LANDED` mechanism):
  //
  //   P4D100_LIST_MODELS_LANDED — P4.D100's `imageProfileListModels` verb.
  //     Until it lands the dispatch answers a typed refusal, the client falls
  //     into its catch-branch, and the builtin label reads the same in BOTH
  //     worlds — so the assertion could not tell a working fetch from a broken
  //     one. That is exactly why it is gated rather than merely tolerant.
  //
  //   P4D101_NANOGPT_LANDED — P4.D101's NANOGPT manifest. Until it lands the
  //     providers listing has no NanoGPT row, and the picker falls back to
  //     `FALLBACK_PROVIDERS` (which this lane DID give a NanoGPT row), so a
  //     naive assertion would pass against the fallback and prove nothing about
  //     the server. The beat therefore asserts the LIVE registry path.
  // -------------------------------------------------------------------------

  const P4D100_LIST_MODELS_LANDED = true;
  const P4D101_NANOGPT_LANDED = true;

  test('Images tab: the Fetch Models control reports where the model list came from', async ({
    page,
  }) => {
    test.skip(!P4D100_LIST_MODELS_LANDED, 'awaits P4.D100 `imageProfileListModels` (wired at unification)');
    test.setTimeout(60_000);

    await page.goto(`${TMPL_BASE_URL}/settings?tab=images`);
    await unlockIfLocked(page, page.getByRole('heading', { name: /Image Generation Profiles/ }));
    await page.getByRole('button', { name: 'New Profile' }).click();

    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible({ timeout: 10_000 });

    // Keyless: v4's third built-in sentence, and the button refuses with its
    // own title rather than querying a provider it has no credential for.
    const fetchButton = dialog.getByRole('button', { name: 'Fetch Models' });
    await expect(fetchButton).toBeDisabled();
    await expect(fetchButton).toHaveAttribute('title', 'Select an API key first');
    await expect(dialog).toContainText(
      "Showing the plugin's built-in model list — select an API key and Fetch Models to query the provider.",
      { timeout: 10_000 },
    );

    // The OFFLINE discriminator: switch the provider to Google and read the
    // model list's ORDER. The keyless auto-load still calls the live verb, and
    // the server's built-in answer is the PLUGIN's `supportedModels` — imagen
    // first (`imagen-4, imagen-4-fast, gemini-…`) — while the client's
    // catch-branch fallback is the registry's `defaultModels`, which for
    // Google orders gemini first. So in the pre-P4.D100 refusal world this
    // select's first option reads `gemini-2.5-flash-image`, and only the
    // landed verb can put `imagen-4` there. (The original draft of this beat
    // selected the fixture's API key and fetched live — but the fixture's one
    // key is OPENAI_COMPATIBLE, which no image provider accepts, and a keyed
    // fetch is an outbound call to a real provider host, which has no place
    // in the offline suite. The keyed arms are label-pinned at component tier
    // and the real-key smoke is the round's 💸 dogfood item.)
    const providerSelect = dialog.locator('select').first();
    await providerSelect.selectOption('GOOGLE');
    await expect(dialog).toContainText(
      "Showing the plugin's built-in model list — select an API key and Fetch Models to query the provider.",
      { timeout: 10_000 },
    );
    const modelSelect = dialog.locator('select').nth(2);
    await expect(modelSelect.locator('option').first()).toHaveText('imagen-4', {
      timeout: 10_000,
    });
    await expect(modelSelect.locator('option')).toHaveCount(4);
    // And the fetched-list state is labeled builtin, never provider.
    await expect(dialog).not.toContainText('fetched from the provider');
  });

  test('Images tab: NanoGPT reaches the image picker from the live provider registry', async ({
    page,
  }) => {
    test.skip(!P4D101_NANOGPT_LANDED, 'awaits P4.D101 NANOGPT manifest (wired at unification)');
    test.setTimeout(60_000);

    await page.goto(`${TMPL_BASE_URL}/settings?tab=images`);
    await unlockIfLocked(page, page.getByRole('heading', { name: /Image Generation Profiles/ }));
    await page.getByRole('button', { name: 'New Profile' }).click();

    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible({ timeout: 10_000 });

    // The provider select is the FIRST one; assert the live registry answered
    // rather than the client fallback. The registry's label is the plugin's
    // manifest title, which is NOT this lane's `FALLBACK_PROVIDERS` label — so
    // a fallback render fails this assertion instead of quietly passing it.
    const providerSelect = dialog.locator('select').first();
    await expect(providerSelect.locator('option', { hasText: /NanoGPT/ })).toHaveCount(1, {
      timeout: 10_000,
    });

    // Selecting it brings v4's Default Size panel with it (P4.D102 Tier 2).
    await providerSelect.selectOption('NANOGPT');
    await expect(dialog).toContainText(
      "Common sizes across NanoGPT's image models; each model maps to its nearest native resolution",
      { timeout: 10_000 },
    );
    const sizeSelect = dialog.locator('select').nth(3);
    await expect(sizeSelect).toHaveValue('1024x1024');
  });

});

function withoutPepper(): NodeJS.ProcessEnv {
  const env = { ...process.env };
  delete env['ENCRYPTION_MASTER_PEPPER'];
  return env;
}

// ---------------------------------------------------------------------------
// P4.6t — the Settings → Memory tab (the Commonplace Book cards). Authored
// pre-unification against lane A's NEW `memories-{main,mount}.db` fixture; the
// describe SKIPS when that fixture is absent (this worktree) and auto-activates
// once lane A lands it + the `memory*` config dispatch variants at unification.
// ---------------------------------------------------------------------------

const MEMSET_PORT = 4328;
const MEMSET_BASE_URL = `http://127.0.0.1:${MEMSET_PORT}`;
const MEMSET_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'settings-memory-instance');
const MEMSET_DATA_DIR = resolve(MEMSET_INSTANCE_DIR, 'data');
const MEMSET_SERVER_LOG = resolve(ARTIFACTS_DIR, 'settings-memory-server.log');
const MEMSET_MAIN_FIXTURE = resolve(FIXTURES_DIR, 'memories-main.db');
const MEMSET_MOUNT_FIXTURE = resolve(FIXTURES_DIR, 'memories-mount.db');
const MEMSET_FIXTURE_READY = existsSync(MEMSET_MAIN_FIXTURE);
const MEMSET_USER_TABLES = ['characters', 'memories', 'tags', 'chats'];

let memSetServer: ChildProcess | undefined;

test.describe('P4.6t — Settings Memory tab (Commonplace Book cards)', () => {
  test.skip(!MEMSET_FIXTURE_READY, 'awaits lane A memories fixture (wired at unification)');

  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(MEMSET_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(MEMSET_DATA_DIR, { recursive: true });
    copyFileSync(MEMSET_MAIN_FIXTURE, resolve(MEMSET_DATA_DIR, 'quilltap.db'));
    if (existsSync(MEMSET_MOUNT_FIXTURE)) {
      copyFileSync(MEMSET_MOUNT_FIXTURE, resolve(MEMSET_DATA_DIR, 'quilltap-mount-index.db'));
    }
    writeFileSync(
      resolve(MEMSET_DATA_DIR, 'quilltap.dbkey'),
      makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE),
    );
    for (const table of MEMSET_USER_TABLES) {
      const res = spawnSync(
        cli,
        [
          'db',
          '--data-dir',
          MEMSET_INSTANCE_DIR,
          '--write',
          `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`,
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
      if (res.status !== 0) {
        const out = `${res.stdout}${res.stderr}`;
        if (!out.includes('no such table') && !out.includes('no such column: userId')) {
          throw new Error(
            `memories fixture rewrite failed (${table}):\n${res.stdout}\n${res.stderr}`,
          );
        }
      }
    }

    const logFd = openSync(MEMSET_SERVER_LOG, 'w');
    memSetServer = spawn(
      web,
      [
        '--host',
        '127.0.0.1',
        '--port',
        String(MEMSET_PORT),
        '--data-dir',
        MEMSET_INSTANCE_DIR,
        '--spa-dir',
        spaDir(),
      ],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    memSetServer.unref();

    const deadline = Date.now() + 30_000;
    for (;;) {
      try {
        const res = await fetch(`${MEMSET_BASE_URL}/health`);
        if (res.status === 423 || res.status === 200) break;
      } catch {
        // not up yet
      }
      if (Date.now() > deadline) {
        throw new Error(`settings memory server not ready within 30s; see ${MEMSET_SERVER_LOG}`);
      }
      await new Promise((r) => setTimeout(r, 300));
    }
  });

  test.afterAll(() => {
    if (memSetServer?.pid) {
      try {
        process.kill(-memSetServer.pid, 'SIGTERM');
      } catch {
        try {
          process.kill(memSetServer.pid, 'SIGTERM');
        } catch {
          // already gone
        }
      }
    }
    rmSync(MEMSET_INSTANCE_DIR, { recursive: true, force: true });
  });

  async function unlockIfLocked(page: Page, ready: ReturnType<Page['getByRole']>): Promise<void> {
    const passphrase = page.locator('#qt-passphrase');
    await expect(passphrase.or(ready).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.isVisible()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
    await expect(ready).toBeVisible({ timeout: 10_000 });
  }

  test('the Commonplace Book cards render over the fixture', async ({ page }) => {
    test.setTimeout(60_000);
    await page.goto(`${MEMSET_BASE_URL}/settings?tab=memory`);
    await unlockIfLocked(page, page.getByRole('heading', { name: 'Repair Missing Embeddings' }));
    // Embedding Profiles is first and open by default (its inner h2 doubles the
    // collapsible title — `.first()`); the rest render collapsed but titled.
    await expect(page.getByRole('heading', { name: 'Embedding Profiles' }).first()).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Memory Housekeeping' })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Recall Relevance' })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Memory Deduplication' })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Regenerate Memories' })).toBeVisible();
    await expect(
      page.getByRole('heading', { name: 'Regenerate Conversation Summaries' }),
    ).toBeVisible();
  });

  test('a Recall Relevance toggle round-trips through the server', async ({ page }) => {
    test.setTimeout(60_000);
    // Deep-link force-opens the Recall Relevance card (it is not the first card).
    await page.goto(`${MEMSET_BASE_URL}/settings?tab=memory&section=memory-recall`);
    await unlockIfLocked(page, page.getByRole('heading', { name: 'Recall Relevance' }));

    const checkbox = page
      .locator('label', { hasText: 'Follow the threads between memories' })
      .locator('input[type="checkbox"]');
    await expect(checkbox).toBeVisible({ timeout: 10_000 });
    const wasChecked = await checkbox.isChecked();
    await checkbox.click();
    // The memoryRecallConfigSet round-trip lands and the checkbox reflects it.
    await expect(checkbox).toBeChecked({ checked: !wasChecked });

    // The new value persists across a full reload (server state).
    await page.reload();
    const reloaded = page
      .locator('label', { hasText: 'Follow the threads between memories' })
      .locator('input[type="checkbox"]');
    await expect(reloaded).toBeChecked({ checked: !wasChecked, timeout: 10_000 });
  });
});
