import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, mkdirSync, openSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { expect, test, request as pwRequest, type Page } from './support/fixtures';

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
 * P4.9f2 — browser walk of the wardrobe control dialog. The out-of-chat arm
 * needs only LANDED verbs (`characterWardrobe*`, `characterList`,
 * `imageProfileList`) and runs LIVE in this lane: unlock → Aria's detail →
 * the Wardrobe tab's (now enabled) button opens the dialog → create an item
 * through the editor → it appears in the character tier → mark it default →
 * compose it into the Outfit Builder (pure client state) → close → reopen and
 * the item persists.
 *
 * The in-chat staging + one-`set_all` flush arm needs lane P4.9f1's
 * `chatOutfitGet`/`chatEquip` and is ACTIVATE-AT-UNIFY: it probes the dispatch
 * surface and self-activates when the wardrobe server half lands (the
 * general-files / courier probe precedent).
 *
 * Runs against its OWN locked server + instance dir over the committed
 * `characters-*` fixture pair (Aria + her "Solo Voyage" chat) — the recipe
 * mirrors `characters-flow.spec.ts`: copy the fixture pair, write the locked
 * .dbkey, rewrite the fixture user id to SINGLE_USER_ID via
 * `quilltap db --write` BEFORE launch, then boot quilltap-web on a
 * spec-private port. No LLM send happens here, so no mock LLM is needed.
 */

/**
 * The accessory the in-chat beat puts on Aria BEFORE opening the dialog, so the
 * Live tab's seeded state is observable in the DOM. See the beat for why an
 * empty worn snapshot cannot be waited on.
 */
const SEEDED_ACCESSORY = 'Aether Scarf';

/** The garment P4.D121's archive beat creates, archives, and restores. */
const ARCHIVE_GARMENT = 'Retired Cloak';

/** The second-person guidance P4.D121's instructions beat writes and clears. */
const DRESSING_TEXT = 'You favour practical tweeds, and save the frock coat for an audience.';

/**
 * ACTIVATE-AT-UNIFY (P4.D88 → P4.D87). The hair beat round-trips a `types:
 * ['hair']` item through the server, which only accepts the fifth slot once the
 * server half (P4.D87) lands. Flip to `true` at unification.
 */
const P4D87_HAIR_SLOT_LANDED = true;

/**
 * ACTIVATE-AT-UNIFY (P4.D113 → P4.D112). The component-carrying transfer beat
 * sends `components` on `wardrobeTransferApply`, which the server half
 * (P4.D112) must accept before the components can actually travel. Flip to
 * `true` at unification. Everything the container browser can prove WITHOUT
 * the new server fields — browsing, in-place create/edit, and the radio pair
 * itself — is live in this lane and runs unconditionally.
 */
// FLIPPED at the round's unification (2026-08-25): P4.D112's server half is on
// main. P4.D130 gave this instance its Quilltap General store, which retired
// the beat's old `hasGeneralStore` park — and immediately uncovered the REAL
// blocker underneath it, so the beat now parks on `hasTransferDestinations`
// instead (see that probe for the measurement). The component-travel semantics
// stay tier-2-proven in `wardrobe_transfers_tier2_equivalence` meanwhile.
const P4D112_TRANSFER_COMPONENTS_LANDED = true;

/**
 * ACTIVATE-AT-UNIFY (P4.D121 → P4.D120). The archive beat writes
 * `{ archived: true }` through `characterWardrobeUpdate` and re-reads with
 * `includeArchived`; until the sibling server lane is on the branch a dispatch
 * verb silently IGNORES both unknown fields (memory:
 * `dispatch-verb-ignores-unknown-fields`), so the beat would fail for a reason
 * that says nothing about this lane. Flip to `true` at unification.
 */
const P4D120_SERVER_LANDED = true;

/**
 * ACTIVATE-AT-UNIFY (P4.D121 → P4.D119). The dressing-instructions round trip
 * needs the eight `*WardrobeInstructions{Get,Set}` verbs. Until they exist the
 * dispatch answers `unknown variant` and the section reads "None on file"
 * forever. Flip to `true` at unification.
 */
const P4D119_INSTRUCTIONS_LANDED = true;

const WARDROBE_PORT = 4329;
const BASE_URL = `http://127.0.0.1:${WARDROBE_PORT}`;
const INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'wardrobe-instance');
const DATA_DIR = resolve(INSTANCE_DIR, 'data');
const SERVER_LOG = resolve(ARTIFACTS_DIR, 'wardrobe-server.log');

/** Every fixture table the walk reads is filtered by userId — rewrite them all. */
const USER_TABLES = [
  'characters',
  'chats',
  'tags',
  'api_keys',
  'connection_profiles',
  'image_profiles',
  'files',
];

let server: ChildProcess | undefined;

test.describe('P4.9f2 — the wardrobe control dialog', () => {
  test.beforeAll(async () => {
    // Each `quilltap db --write` unwraps the .dbkey via PBKDF2 (≈5s); the
    // rewrites plus server boot need more than the default per-hook timeout.
    test.setTimeout(120_000);

    const web = webBinary();
    const cli = cliBinary();

    rmSync(INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(DATA_DIR, { recursive: true });
    copyFileSync(resolve(FIXTURES_DIR, 'characters-main.db'), resolve(DATA_DIR, 'quilltap.db'));
    copyFileSync(
      resolve(FIXTURES_DIR, 'characters-mount.db'),
      resolve(DATA_DIR, 'quilltap-mount-index.db'),
    );

    writeFileSync(resolve(DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));
    // P4.D130 tier 2 — give this instance the three built-in stores, which is
    // what un-parks the component-transfer and "Shared — everywhere" beats.
    //
    // MEASURED, not assumed: the committed `characters-main.db` carries no
    // `instance_settings` table at all, and `ensure_builtin_mounts`' first act
    // is v4's own `shouldRun` guard — `sqliteTableExists('instance_settings')`
    // — so boot skipped the whole provisioning unit and the instance had no
    // Quilltap General to write into.
    //
    // Creating the (empty) table here is instance MATERIALIZATION, not a
    // fixture regen: the committed pair is untouched, so the six harness
    // families and the `quilltap-web` test venue that read it keep the exact
    // bytes they were pinned against, and v5's REAL provisioning path mints the
    // stores at boot rather than a builder hand-writing rows. Verbatim the
    // salon instance's own precedent (`global-setup.ts`, the `terminal_sessions`
    // / `chat_documents` materializations alongside it).
    runCliWrite(
      cli,
      'CREATE TABLE IF NOT EXISTS instance_settings (' +
        '"key" TEXT PRIMARY KEY, "value" TEXT NOT NULL);',
    );
    // P4.69 — the same materialization, for the two tables the TRANSFER
    // destinations enumerator reads. The committed `characters-main.db` carries
    // neither, so `wardrobeTransferDestinations` failed WHOLE (not per-tier) and
    // the Move dialog could offer nothing; the component-transfer beat parked on
    // that and the Copy arm's `option[value^="character:"]` count sat vacuously
    // at 0 because the fetch never returned.
    //
    // DDL copied from `crates/quilltap-core/src/services/provisioning/
    // fresh_schema.json` (v5's D23 re-dump of v4's live `generateDDL`), so the
    // shape is v4's own rather than a hand-written approximation. Both stay
    // EMPTY: the enumerator's project/group tiers legitimately contribute no
    // destinations here, which is what makes the character-tier count below the
    // thing under test. Instance materialization, not a fixture regen — the
    // committed pair is untouched and the six harness families that read it keep
    // their pinned bytes.
    runCliWrite(
      cli,
      'CREATE TABLE IF NOT EXISTS projects (' +
        '"id" TEXT PRIMARY KEY NOT NULL, "name" TEXT NOT NULL, ' +
        '"officialMountPointId" TEXT, "createdAt" TEXT NOT NULL, ' +
        '"updatedAt" TEXT NOT NULL, "description" TEXT, "instructions" TEXT, ' +
        '"state" TEXT DEFAULT \'{}\', "allowAnyCharacter" INTEGER DEFAULT 0, ' +
        '"characterRoster" TEXT DEFAULT \'[]\', "color" TEXT, "icon" TEXT, ' +
        '"defaultDisabledTools" TEXT DEFAULT \'[]\', ' +
        '"defaultDisabledToolGroups" TEXT DEFAULT \'[]\', ' +
        '"defaultAgentModeEnabled" INTEGER, ' +
        '"defaultAvatarGenerationEnabled" INTEGER, ' +
        '"defaultImageProfileId" TEXT, "defaultRoleplayTemplateId" TEXT, ' +
        '"defaultAlertCharactersOfLanternImages" INTEGER, ' +
        '"answerConfirmationOverride" TEXT, "storyBackgroundsEnabled" INTEGER, ' +
        '"staticBackgroundImageId" TEXT, "storyBackgroundImageId" TEXT, ' +
        '"backgroundDisplayMode" TEXT DEFAULT \'theme\');',
    );
    runCliWrite(
      cli,
      'CREATE TABLE IF NOT EXISTS groups (' +
        '"id" TEXT PRIMARY KEY NOT NULL, "name" TEXT NOT NULL, ' +
        '"officialMountPointId" TEXT, "createdAt" TEXT NOT NULL, ' +
        '"updatedAt" TEXT NOT NULL, "description" TEXT, "instructions" TEXT, ' +
        '"state" TEXT DEFAULT \'{}\', "color" TEXT, "icon" TEXT);',
    );
    for (const table of USER_TABLES) {
      runCliWrite(
        cli,
        `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`,
      );
    }

    const logFd = openSync(SERVER_LOG, 'w');
    server = spawn(
      web,
      [
        '--host',
        '127.0.0.1',
        '--port',
        String(WARDROBE_PORT),
        '--data-dir',
        INSTANCE_DIR,
        '--spa-dir',
        spaDir(),
      ],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    server.unref();
    await waitForHealth();
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
    rmSync(INSTANCE_DIR, { recursive: true, force: true });
  });

  /** Unlock only when the passphrase screen is showing. */
  async function unlockIfLocked(page: Page): Promise<void> {
    const passphrase = page.locator('#qt-passphrase');
    const roster = page.getByRole('heading', { name: 'Characters', exact: true });
    await expect(passphrase.or(roster).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.isVisible()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
    await expect(roster).toBeVisible({ timeout: 10_000 });
  }

  /** Open Aria's detail view from the roster (card BODY click — finding #4). */
  async function openAriaDetail(page: Page): Promise<void> {
    const aria = page
      .locator('.character-card-grid .character-card')
      .filter({ hasText: 'Aria' })
      .first();
    await expect(aria).toBeVisible();
    await aria.locator('p.line-clamp-3').click();
    await expect(page.getByRole('heading', { name: 'Aria' })).toBeVisible();
  }

  /** Open the wardrobe dialog from the detail view's Wardrobe tab. Scoped to
   *  the detail view: the shell footer button shares the accessible name
   *  "Wardrobe" (the added-affordance locator trap). */
  async function openWardrobeDialog(page: Page): Promise<void> {
    await page
      .locator('.character-view')
      .getByRole('button', { name: 'Wardrobe', exact: true })
      .click();
    // v4's own string — the P4.9f2 stub retirement: the button is enabled.
    await page.getByRole('button', { name: 'Open wardrobe for Aria' }).click();
    await expect(page.getByRole('dialog').locator('.qt-dialog-title')).toHaveText('Wardrobe');
  }

  test('detail → Wardrobe tab → create item → mark default → compose → persists across reopen', async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await page.goto(`${BASE_URL}/characters`);
    await unlockIfLocked(page);
    await openAriaDetail(page);
    await openWardrobeDialog(page);

    // Aria is preselected in the header dropdown (the dialog opened with her
    // characterId in context).
    await expect(page.locator('#wardrobe-container-select')).toHaveValue(/./);

    // Create an item through the editor (characterWardrobeCreate — landed).
    await page.getByRole('button', { name: '+ New Item' }).click();
    await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeVisible();
    await page.locator('#wardrobe-title').fill('Brass Goggles');
    // Type(s): tick "accessories" (single-garment mode).
    // hasText is a substring match; among the four Type(s) labels only one
    // contains "accessories" (regex here would trip on un-normalized
    // whitespace — the P4.9a memory).
    await page
      .locator('label')
      .filter({ hasText: 'accessories' })
      .locator('input[type="checkbox"]')
      .click();
    await page.getByRole('button', { name: 'Create', exact: true }).click();

    // The editor closes and the item lands in the character tier list.
    await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeHidden();
    const gogglesRow = page
      .locator('.qt-card-interactive')
      .filter({ hasText: 'Brass Goggles' })
      .first();
    await expect(gogglesRow).toBeVisible();

    // Mark it default via the kebab (characterWardrobeUpdate — landed).
    await gogglesRow.getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: /Mark as default outfit item/ }).click();
    await expect(gogglesRow).toContainText('· default', { timeout: 10_000 });

    // Compose it into the Outfit Builder (pure client state — out of chat no
    // equip call ever fires; the dialog is on the Builder tab by default).
    const accessoriesRow = page.locator('.qt-card').filter({ hasText: 'Accessories' }).first();
    await accessoriesRow.getByRole('button', { name: '+', exact: true }).click();
    await accessoriesRow.getByRole('button', { name: /Brass Goggles/ }).click();
    await expect(accessoriesRow).toContainText('Brass Goggles');

    // Close, reopen: the item persisted server-side.
    await page.getByRole('button', { name: 'Done' }).click();
    await expect(page.getByRole('dialog')).toBeHidden();
    await page.getByRole('button', { name: 'Open wardrobe for Aria' }).click();
    await expect(
      page.locator('.qt-card-interactive').filter({ hasText: 'Brass Goggles' }).first(),
    ).toContainText('· default', { timeout: 10_000 });
    await page.getByRole('button', { name: 'Done' }).click();
  });

  /**
   * P4.D88 — the fifth slot, end to end through the dialog: create a hairdo,
   * see it badged in the wardrobe list, compose it into the Hair slot row, and
   * find it again after a reopen (so the `types: ['hair']` round trip really
   * reached the server).
   *
   * The Green Room half of the order's Tier-2 beat (the rose badge on a
   * decided outfit) is DEFERRED and named: the preview only paints when the
   * chat-start outfit LLM chooses, and this suite stages no cheap-LLM outfit
   * run. It is covered at component level by
   * `screens/new-chat/outfit-slots-preview.spec.ts`.
   */
  /**
   * P4.D121 — the wardrobe archive surface (v4 `d25dacc1`).
   *
   * Archive a garment from the row's kebab → it vanishes from the list because
   * the FETCH omitted it (v5's own client-side `archivedAt` filter is gone, so
   * this beat proves the server half is what hides it) → "Show archived"
   * reveals it, badged → "Restore from archive" brings it back.
   */
  test('archives a garment, hides it by fetch, reveals it badged, and restores it (ACTIVATE-AT-UNIFY, lane P4.D120)', async ({
    page,
  }) => {
    test.skip(
      !P4D120_SERVER_LANDED,
      'awaits P4.D120’s `archived` write + `includeArchived` read (wired at unification)',
    );
    test.setTimeout(90_000);
    await page.goto(`${BASE_URL}/characters`);
    await unlockIfLocked(page);
    await openAriaDetail(page);
    await openWardrobeDialog(page);

    // Create the garment this beat retires.
    await page.getByRole('button', { name: '+ New Item' }).click();
    await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeVisible();
    await page.locator('#wardrobe-title').fill(ARCHIVE_GARMENT);
    // Create is disabled until a Type is ticked (v4's own isSaveDisabled rule
    // — the first live run's catch; the sibling beats' gesture adopted).
    await page
      .locator('label')
      .filter({ hasText: 'accessories' })
      .locator('input[type="checkbox"]')
      .click();
    await page.getByRole('button', { name: 'Create', exact: true }).click();
    const row = () =>
      page.locator('.qt-card-interactive').filter({ hasText: ARCHIVE_GARMENT }).first();
    await expect(row()).toBeVisible({ timeout: 10_000 });

    // Archive it from the kebab — single click, no confirm.
    await row().getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: 'Archive', exact: true }).click();
    await expect(
      page.locator('.qt-card-interactive').filter({ hasText: ARCHIVE_GARMENT }),
    ).toHaveCount(0, { timeout: 10_000 });

    // "Show archived" re-fetches and the garment returns, badged.
    const showArchived = page
      .getByRole('dialog')
      .locator('label', { hasText: 'Show archived' })
      .locator('input[type="checkbox"]');
    await showArchived.check();
    await expect(row()).toBeVisible({ timeout: 10_000 });
    await expect(row()).toContainText('archived');

    // Restore it, then untick: it is a live garment again.
    await row().getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: 'Restore from archive' }).click();
    await expect(row()).not.toContainText('archived', { timeout: 10_000 });
    await showArchived.uncheck();
    await expect(row()).toBeVisible({ timeout: 10_000 });

    await page.getByRole('button', { name: 'Done' }).click();
  });

  /**
   * P4.D121 — the Dressing Instructions section (v4 `b86bb1a5`): collapsed by
   * default with a status note, save round-trips the file, and a blank save
   * clears it.
   */
  test('the dressing-instructions section saves, reloads and clears (ACTIVATE-AT-UNIFY, lane P4.D119)', async ({
    page,
  }) => {
    test.skip(
      !P4D119_INSTRUCTIONS_LANDED,
      'awaits P4.D119’s wardrobe-instructions verbs (wired at unification)',
    );
    test.setTimeout(90_000);
    await page.goto(`${BASE_URL}/characters`);
    await unlockIfLocked(page);
    await openAriaDetail(page);
    await openWardrobeDialog(page);

    // Scoped to the DIALOG's section: the character detail behind it mounts
    // the Aurora wardrobe TAB, which hosts its own section (v4 has both
    // mounts too) — an unscoped locator strict-mode-fails on the pair (the
    // first live run's catch).
    const section = page.getByRole('dialog').locator('qt-wardrobe-instructions-section');
    await expect(section).toBeVisible({ timeout: 10_000 });
    // Collapsed, and nothing on file yet.
    await expect(section).toContainText('None on file', { timeout: 10_000 });
    await expect(section.locator('qt-markdown-field')).toHaveCount(0);

    await section.getByRole('button', { name: /Dressing Instructions/ }).click();
    const editor = section.locator('.qt-rich-editor-content');
    await expect(editor).toBeVisible();
    await editor.click();
    await page.keyboard.type(DRESSING_TEXT);
    await section.getByRole('button', { name: 'Save Instructions' }).click();
    await expect(page.getByText('Dressing instructions saved')).toBeVisible({ timeout: 10_000 });
    await expect(section).toContainText('On file', { timeout: 10_000 });

    // Reopen the dialog: the file came back from the server.
    await page.getByRole('button', { name: 'Done' }).click();
    await page.getByRole('button', { name: 'Open wardrobe for Aria' }).click();
    // Dialog-scoped like `section` above — the tab's own section is still
    // mounted behind the dialog (and still reads "None on file": it loaded
    // before the save and does not refetch, in v4 as here).
    const reopened = page.getByRole('dialog').locator('qt-wardrobe-instructions-section');
    await expect(reopened).toContainText('On file', { timeout: 10_000 });
    await reopened.getByRole('button', { name: /Dressing Instructions/ }).click();
    await expect(reopened.locator('.qt-rich-editor-content')).toContainText(DRESSING_TEXT);

    // A blank draft CLEARS the file.
    await reopened.locator('.qt-rich-editor-content').click();
    await page.keyboard.press('ControlOrMeta+a');
    await page.keyboard.press('Backspace');
    await reopened.getByRole('button', { name: 'Save Instructions' }).click();
    await expect(page.getByText('Dressing instructions cleared')).toBeVisible({ timeout: 10_000 });
    await expect(reopened).toContainText('None on file', { timeout: 10_000 });

    await page.getByRole('button', { name: 'Done' }).click();
  });

  test('creates a hairdo, badges it, and composes it into the Hair slot (ACTIVATE-AT-UNIFY, lane P4.D87)', async ({
    page,
  }) => {
    test.skip(
      !P4D87_HAIR_SLOT_LANDED,
      'ACTIVATE-AT-UNIFY (lane P4.D87): the server rejects types:["hair"] until the server half lands',
    );
    test.setTimeout(90_000);
    await page.goto(`${BASE_URL}/characters`);
    await unlockIfLocked(page);
    await openAriaDetail(page);
    await openWardrobeDialog(page);

    await page.getByRole('button', { name: '+ New Item' }).click();
    await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeVisible();
    await page.locator('#wardrobe-title').fill('Marcel Waves');
    // Among the five Type(s) labels only one contains "hair" (substring match —
    // a regex would trip on un-normalized whitespace, the P4.9a memory).
    await page
      .locator('label')
      .filter({ hasText: 'hair' })
      .locator('input[type="checkbox"]')
      .click();
    await page.getByRole('button', { name: 'Create', exact: true }).click();

    // The row lands in the character tier wearing the rose slot badge.
    await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeHidden();
    const wavesRow = page
      .locator('.qt-card-interactive')
      .filter({ hasText: 'Marcel Waves' })
      .first();
    await expect(wavesRow).toBeVisible();
    await expect(wavesRow.locator('.qt-badge-wardrobe-hair')).toHaveText('hair');

    // The Hair slot row exists in the Outfit Builder and takes the hairdo.
    const hairRow = page.locator('.qt-card').filter({ hasText: 'Hair' }).first();
    await hairRow.getByRole('button', { name: '+', exact: true }).click();
    await hairRow.getByRole('button', { name: /Marcel Waves/ }).click();
    await expect(hairRow).toContainText('Marcel Waves');

    // Close, reopen: the hairdo persisted server-side (the round trip's proof).
    await page.getByRole('button', { name: 'Done' }).click();
    await expect(page.getByRole('dialog')).toBeHidden();
    await page.getByRole('button', { name: 'Open wardrobe for Aria' }).click();
    await expect(
      page.locator('.qt-card-interactive').filter({ hasText: 'Marcel Waves' }).first(),
    ).toBeVisible({ timeout: 10_000 });
    await page.getByRole('button', { name: 'Done' }).click();
  });

  /**
   * P4.D113 tier-2 (a) — the container browser, LIVE. The new top selector
   * lists every place a wardrobe item can live, browsing Quilltap General
   * shows exactly that container (no tier merging), the shared-wardrobe note
   * appears, the right-hand outfit column steps aside because there is nobody
   * to dress, and the editor opened from there is PINNED — its "Add to" scope
   * radiogroup is replaced by a destination note.
   *
   * ⚠ THE WRITE HALF WAS SELF-ACTIVATING AND IS NOW LIVE (P4.D130). It used to
   * park: the committed `characters-*` pair is a narrow hand-built DB with no
   * `instance_settings` table, so `ensure_builtin_mounts`' first act — v4's own
   * `shouldRun` guard, `sqliteTableExists('instance_settings')` — skipped the
   * whole provisioning unit and the instance had NO Quilltap General store to
   * write into. `beforeAll` now materializes that (empty) table, so v5's real
   * provisioning path mints the three built-in stores at boot. The probe below
   * still asks the instance directly rather than assuming, because the answer
   * is a property of boot, not of this file.
   */
  test('the container selector browses Quilltap General, and pins the editor to it (P4.D113)', async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await page.goto(`${BASE_URL}/characters`);
    await unlockIfLocked(page);
    await openAriaDetail(page);
    await openWardrobeDialog(page);

    const dialog = page.getByRole('dialog');
    const selector = dialog.locator('#wardrobe-container-select');
    // The menu lists every place an item can live, not just characters. The
    // Characters and General groups are always present; Projects/Groups appear
    // only when the instance has any, so they are not asserted by count.
    await expect(selector.locator('optgroup[label="Characters"]')).toHaveCount(1);
    await expect(selector.locator('optgroup[label="General"]')).toHaveCount(1);
    await expect(selector.locator('option[value="general:"]')).toHaveText(/Quilltap General/);

    // P4.D122 recorded a duplicate-"Quilltap General" collision on the SALON
    // e2e instance and suspected `services/builtin_mounts.rs`. Measured here on
    // a second instance that provisions through the same boot hook: exactly
    // one, so the ensure-or-adopt is idempotent and the duplicate is a property
    // of that other instance's history, not of the provisioner. Standing
    // tripwire — if the provisioner ever does start duplicating, this reddens.
    const enabledGeneral = (
      ((await dispatch(page, { type: 'mountPointList' }))['mountPoints'] ?? []) as Array<{
        name: string;
        enabled?: boolean;
      }>
    ).filter((m) => m.enabled !== false && m.name === 'Quilltap General');
    expect(enabledGeneral).toHaveLength(1);
    // The label is v4's new one.
    await expect(dialog.getByText('Wardrobe:', { exact: true })).toBeVisible();

    await selector.selectOption('general:');
    // Browsing a shared wardrobe: the note appears and — with nobody to dress
    // — the right-hand outfit column steps aside (v4 keeps it behind
    // `selectedCharacterId`, `:1260`).
    await expect(dialog.getByText('Browsing a shared wardrobe', { exact: false })).toBeVisible();
    await expect(dialog.getByRole('button', { name: 'Outfit Builder' })).toBeHidden();
    // Aria's own garments are NOT merged in — a shared container shows exactly
    // its own contents (this fixture's General store is empty, so the list is
    // the empty-filter notice rather than her wardrobe).
    await expect(
      dialog.locator('.qt-card-interactive').filter({ hasText: 'Brass Goggles' }),
    ).toHaveCount(0);

    // The editor opened here is PINNED: no "Add to" scope radiogroup, just the
    // destination note naming the container.
    await dialog.getByRole('button', { name: '+ New Item' }).click();
    await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeVisible();
    await expect(page.getByText('Every character, in every chat, can wear it.')).toBeVisible();
    await expect(page.locator('[role="radiogroup"]')).toHaveCount(0);
    await expect(page.getByText('Shared — everywhere')).toHaveCount(0);

    // The write half, self-activating (see the docblock).
    const generalWritable = await hasGeneralStore(page);
    if (generalWritable) {
      await page.locator('#wardrobe-title').fill('Domino Mask');
      await page
        .locator('label')
        .filter({ hasText: 'accessories' })
        .locator('input[type="checkbox"]')
        .click();
      await page.getByRole('button', { name: 'Create', exact: true }).click();
      await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeHidden();

      // It lands in the General list with the FULL kebab — the container
      // browser's whole point; a merged shared item in the character view gets
      // Move/Copy only.
      const maskRow = dialog
        .locator('.qt-card-interactive')
        .filter({ hasText: 'Domino Mask' })
        .first();
      await expect(maskRow).toBeVisible();
      await expect(maskRow).not.toContainText('· shared');
      await maskRow.getByRole('button', { name: 'More actions' }).click();
      await expect(page.getByRole('menuitem', { name: 'Edit' })).toBeVisible();
      await expect(page.getByRole('menuitem', { name: 'Delete' })).toBeVisible();

      // Edit IN PLACE, then reopen: the PUT reached this container's store.
      await page.getByRole('menuitem', { name: 'Edit' }).click();
      await expect(page.getByRole('heading', { name: 'Edit Wardrobe Item' })).toBeVisible();
      await page.locator('#wardrobe-title').fill('Domino Mask (lacquered)');
      await page.getByRole('button', { name: 'Update', exact: true }).click();
      await expect(page.getByRole('heading', { name: 'Edit Wardrobe Item' })).toBeHidden();
      await dialog.getByRole('button', { name: 'Done' }).click();
      await expect(dialog).toBeHidden();
      await page.getByRole('button', { name: 'Open wardrobe for Aria' }).click();
      const reopened = page.getByRole('dialog');
      await reopened.locator('#wardrobe-container-select').selectOption('general:');
      await expect(
        reopened
          .locator('.qt-card-interactive')
          .filter({ hasText: 'Domino Mask (lacquered)' })
          .first(),
      ).toBeVisible({ timeout: 10_000 });
      await reopened.getByRole('button', { name: 'Done' }).click();
      return;
    }

    await page.getByRole('button', { name: 'Cancel' }).click();
    await dialog.getByRole('button', { name: 'Done' }).click();
  });

  /**
   * P4.D113 tier-2 (c) — the component prompt and the copy+move refusal, LIVE
   * in the character view (where this fixture DOES have a writable container).
   * v4 makes the illegal combination unreachable rather than surfacing it as
   * an error: Move offers three component choices, Copy offers two, and "Move
   * the components" is absent from the copy arm entirely. That IS the refusal,
   * and it is a pure client property — no transfer is executed here.
   */
  test('a composite outfit prompts for its components, and Copy never offers a move (P4.D113)', async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await page.goto(`${BASE_URL}/characters`);
    await unlockIfLocked(page);
    await openAriaDetail(page);
    await openWardrobeDialog(page);

    const dialog = page.getByRole('dialog');

    // Build an outfit in Aria's own vault out of the item the first beat left
    // behind (this file runs serially), through the editor's bundle mode.
    await dialog.getByRole('button', { name: '+ New Item' }).click();
    await page.getByRole('tab', { name: 'Outfit bundle' }).click();
    await page.locator('#wardrobe-title').fill('Masquerade Kit');
    // The component picker groups its candidates (all groups start expanded);
    // each row is a label carrying the checkbox.
    await page
      .locator('qt-wardrobe-component-picker label')
      .filter({ hasText: 'Brass Goggles' })
      .first()
      .locator('input[type="checkbox"]')
      .click();
    await page.getByRole('button', { name: 'Create', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeHidden();

    // Outfits live behind the kind filter's second tab.
    await dialog.getByRole('tab', { name: 'Outfits' }).click();
    const kitRow = dialog
      .locator('.qt-card-interactive')
      .filter({ hasText: 'Masquerade Kit' })
      .first();
    await expect(kitRow).toBeVisible();

    // Copy: two choices, and the move is nowhere on offer.
    await kitRow.getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: 'Copy' }).click();
    const copyDialog = page.getByRole('dialog').filter({ hasText: 'Copy wardrobe item' });
    await expect(copyDialog.getByText('This outfit bundles 1 component')).toBeVisible();
    await expect(copyDialog.locator('input[name="wardrobe-transfer-components"]')).toHaveCount(2);
    await expect(copyDialog.getByText('Copy the components along with it')).toBeVisible();
    await expect(copyDialog.getByText('Move the components along with it')).toHaveCount(0);
    // The item's known home — Aria's own vault — is dropped from the list, but
    // every OTHER character's vault is on offer.
    //
    // P4.69: this asserted 0 and was vacuously green — `wardrobeTransferDestinations`
    // died whole on the missing `projects`/`groups` tables, so the select had no
    // options of any kind and "no character option" measured nothing. With both
    // tables materialized in `beforeAll` the enumerator returns the real answer:
    // the fixture's five characters minus Aria herself, whose vault is this
    // item's known home. Asserting the NAMES, not just the count, is what makes
    // the omission the thing under test rather than an arithmetic coincidence.
    const characterOptions = copyDialog.locator('option[value^="character:"]');
    await expect(characterOptions).toHaveCount(4);
    const optionText = (await characterOptions.allTextContents()).join(' | ');
    expect(optionText, `character destinations offered: ${optionText}`).not.toContain('Aria');
    // The §3 unification review: the omission alone is a NEGATIVE pin — a
    // renderer that stopped labelling the options would leave four blank rows
    // and both lines above green. The fixture's other four characters (the
    // `build-characters-fixture.ts` roster: Bram, Cleo, Dax, Echo) must be the
    // ones on offer, by name.
    for (const name of ['Bram', 'Cleo', 'Dax', 'Echo']) {
      expect(optionText, `${name}'s vault must be offered as a copy destination`).toContain(name);
    }
    await copyDialog.getByRole('button', { name: 'Cancel' }).click();

    // Move: three choices, defaulting to moving them along.
    await kitRow.getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: 'Move' }).click();
    const moveDialog = page.getByRole('dialog').filter({ hasText: 'Move wardrobe item' });
    await expect(moveDialog.locator('input[name="wardrobe-transfer-components"]')).toHaveCount(3);
    await expect(
      moveDialog.locator('input[name="wardrobe-transfer-components"]').first(),
    ).toBeChecked();
    await moveDialog.getByRole('button', { name: 'Cancel' }).click();

    await dialog.getByRole('button', { name: 'Done' }).click();
  });

  /**
   * P4.D113 tier-2 (b) — the components actually travel. ACTIVATE-AT-UNIFY:
   * `components` on `wardrobeTransferApply` is P4.D112's server half, so until
   * that lands the outfit would move alone and the assertions below would be
   * measuring the OLD behaviour rather than the new contract. The destination
   * also needs a shared container, which this fixture has none of (see the
   * first beat's docblock) — so the beat additionally probes for one.
   */
  test('moving an outfit with "Move components" carries them to the destination (ACTIVATE-AT-UNIFY, lane P4.D112)', async ({
    page,
  }) => {
    test.skip(
      !P4D112_TRANSFER_COMPONENTS_LANDED,
      'ACTIVATE-AT-UNIFY (lane P4.D112): `components` on wardrobeTransferApply is the server half',
    );
    test.setTimeout(90_000);
    await page.goto(`${BASE_URL}/characters`);
    await unlockIfLocked(page);
    // P4.69 UN-PARKED. This used to `test.skip` on the probe because the
    // committed `characters-main.db` has no `projects`/`groups` tables and
    // `wardrobeTransferDestinations` failed whole. `beforeAll` now materializes
    // both, so the probe is an ASSERTION rather than a park — if the enumerator
    // ever breaks again this beat says so instead of quietly not running.
    expect(
      await hasTransferDestinations(page),
      'wardrobeTransferDestinations must answer — see the projects/groups materialization in beforeAll',
    ).toBe(true);
    await openAriaDetail(page);
    await openWardrobeDialog(page);

    const dialog = page.getByRole('dialog');
    await dialog.getByRole('tab', { name: 'Outfits' }).click();
    const kitRow = dialog
      .locator('.qt-card-interactive')
      .filter({ hasText: 'Masquerade Kit' })
      .first();
    await expect(kitRow).toBeVisible();
    await kitRow.getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: 'Move' }).click();

    const moveDialog = page.getByRole('dialog').filter({ hasText: 'Move wardrobe item' });
    await moveDialog.locator('select').selectOption('general:');
    // "Move the components along with it" is the default; name it anyway so
    // the beat still means what it says if the default ever changes.
    await moveDialog.getByText('Move the components along with it').click();
    await moveDialog.getByRole('button', { name: 'Move item' }).click();
    await expect(moveDialog).toBeHidden({ timeout: 15_000 });

    // The outfit AND its component left Aria's vault…
    await dialog.getByRole('tab', { name: 'Items' }).click();
    await expect(
      dialog.locator('.qt-card-interactive').filter({ hasText: 'Brass Goggles' }),
    ).toHaveCount(0, { timeout: 10_000 });

    // …and both arrived in General, the outfit still resolving its piece.
    await dialog.locator('#wardrobe-container-select').selectOption('general:');
    await dialog.getByRole('tab', { name: 'Outfits' }).click();
    await expect(
      dialog.locator('.qt-card-interactive').filter({ hasText: 'Masquerade Kit' }).first(),
    ).toBeVisible({ timeout: 10_000 });
    await dialog.getByRole('tab', { name: 'Items' }).click();
    await expect(
      dialog.locator('.qt-card-interactive').filter({ hasText: 'Brass Goggles' }).first(),
    ).toBeVisible();
    await dialog.getByRole('button', { name: 'Done' }).click();
  });

  /**
   * P4.D130 — the outfit pull-down (v4 `aec86a613`), end to end.
   *
   * The composer opens with a `Wear an outfit…` pull-down above the slot rows;
   * the per-slot `+` pickers no longer offer composites. This walks the whole
   * contract on a live instance: a composite is absent from the slot picker,
   * present in the pull-down (with the slots it claims), and wearing it there
   * dissolves it into its component garments across the slots their OWN types
   * declare — through the pre-existing `addToSlot` path, since `aec86a613`
   * added no equip path at all.
   *
   * Out of chat the composer is the Outfit Builder's, whose adds are pure
   * client state (`fittingAdd` → `wearItemIntoSlots`), so nothing is equipped
   * server-side and no other beat's state moves.
   */
  test('the outfit pull-down wears a composite, which dissolves into its slots — and the slot pickers no longer offer it (P4.D130)', async ({
    page,
  }) => {
    test.setTimeout(120_000);
    await page.goto(`${BASE_URL}/characters`);
    await unlockIfLocked(page);
    await openAriaDetail(page);
    await openWardrobeDialog(page);

    const dialog = page.getByRole('dialog');

    /** Create a single-garment item in one slot. */
    async function createGarment(title: string, slot: string): Promise<void> {
      await dialog.getByRole('button', { name: '+ New Item' }).click();
      await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeVisible();
      await page.locator('#wardrobe-title').fill(title);
      // hasText is a substring match; among the five Type(s) labels each of
      // these names is unique (a regex would trip on un-normalized whitespace —
      // the P4.9a memory).
      await page
        .locator('label')
        .filter({ hasText: slot })
        .locator('input[type="checkbox"]')
        .click();
      await page.getByRole('button', { name: 'Create', exact: true }).click();
      await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeHidden();
      await expect(
        dialog.locator('.qt-card-interactive').filter({ hasText: title }).first(),
      ).toBeVisible();
    }

    // Two garments in DIFFERENT slots, so the dissolution is observable as two
    // separate rows filling rather than one id landing twice.
    await createGarment('Oilskin Slicker', 'top');
    await createGarment('Storm Boots', 'footwear');

    // …bundled into one composite.
    await dialog.getByRole('button', { name: '+ New Item' }).click();
    await page.getByRole('tab', { name: 'Outfit bundle' }).click();
    await page.locator('#wardrobe-title').fill('Squall Rig');
    for (const component of ['Oilskin Slicker', 'Storm Boots']) {
      await page
        .locator('qt-wardrobe-component-picker label')
        .filter({ hasText: component })
        .first()
        .locator('input[type="checkbox"]')
        .click();
    }
    await page.getByRole('button', { name: 'Create', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeHidden();

    // ── The composer (out of chat: the Outfit Builder's) ──
    const composer = dialog.locator('qt-outfit-composer');
    const slotRow = (slot: string) =>
      composer
        .locator('qt-equipped-slot-row')
        .filter({ has: page.locator(`.qt-badge-wardrobe-${slot}`) });

    // 1. The per-slot picker offers the GARMENT and not the composite, though
    //    the composite covers this slot too. Pre-`aec86a613` it listed both.
    const top = slotRow('top');
    await top.getByRole('button', { name: '+', exact: true }).click();
    await expect(top.getByRole('button', { name: /Oilskin Slicker/ })).toBeVisible();
    await expect(top.getByRole('button', { name: /Squall Rig/ })).toHaveCount(0);
    // Close the picker again so its rows can't be confused with the menu's.
    await top.getByRole('button', { name: '+', exact: true }).click();

    // 2. The pull-down lists it, naming the slots it claims.
    const pullDown = composer.getByRole('button', { name: 'Wear an outfit…' });
    await expect(pullDown).toBeVisible();
    await pullDown.click();
    const listbox = composer.locator('[role="listbox"]');
    const squall = listbox.getByRole('option').filter({ hasText: 'Squall Rig' });
    await expect(squall).toBeVisible();
    // `WARDROBE_SLOT_META` labels, ', '-joined — the union of its components'
    // slots, not the raw type strings.
    await expect(squall).toContainText('Top, Footwear');

    // 3. Wearing it dissolves it: the LEAVES land in the slots their own types
    //    declare, and the composite's own id is never stored (no bundle card,
    //    no "Squall Rig" chip anywhere in the composer).
    await squall.click();
    await expect(listbox).toBeHidden();
    await expect(slotRow('top')).toContainText('Oilskin Slicker');
    await expect(slotRow('footwear')).toContainText('Storm Boots');
    await expect(composer.locator('qt-equipped-bundle-card')).toHaveCount(0);
    await expect(composer).not.toContainText('Squall Rig');

    await dialog.getByRole('button', { name: 'Done' }).click();
    await expect(dialog).toBeHidden();
  });

  test('out of chat: Preview reaches the no-API-key badRequest (P4.6bf; live render is a dogfood item)', async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await page.goto(`${BASE_URL}/characters`);
    await unlockIfLocked(page);
    await openAriaDetail(page);
    await openWardrobeDialog(page);

    // Out of chat the dialog opens on the Outfit Builder tab, which hosts the
    // avatar-generation pane + its Preview button (v4 :568 — rightTab defaults
    // to 'builder' out of chat). The one committed image profile has
    // apiKeyId=null, so the (enabled) Preview button reaches v4's PRE-provider
    // badRequest — this asserts the render seam is reached with ZERO
    // image-provider spend.
    //
    // The LIVE render walk (P4.6bf wired the HostAvatarPreviewRenderer, so a
    // keyed profile makes Preview cost real money) is deliberately NOT an e2e
    // beat: the shared e2e instance has no live image provider, and standing up
    // a canned localhost provider endpoint for one beat is disproportionate.
    // It is recorded as a DOGFOOD item (the P4.6bf lane record) instead.
    const dialog = page.getByRole('dialog');
    const preview = dialog.getByRole('button', { name: 'Preview', exact: true });
    await expect(preview).toBeEnabled();
    await preview.click();

    // v4 `preview-avatar/route.ts`: `!imageProfile.apiKeyId` →
    // badRequest('Selected image profile has no API key configured'). v4 reports
    // it as a toast and the dialog carries no banner (P4.25).
    await expect(
      page
        .locator('[role="toast-container"] > div')
        .filter({ hasText: 'Selected image profile has no API key configured' }),
    ).toBeVisible({ timeout: 15_000 });

    await dialog.getByRole('button', { name: 'Done' }).click();
    await expect(dialog).toBeHidden();
  });

  test('in chat: staging + the one-shot set_all flush persists the outfit (ACTIVATE-AT-UNIFY, lane P4.9f1)', async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await page.goto(`${BASE_URL}/characters`);
    await unlockIfLocked(page);

    // Probe lane P4.9f1's dispatch surface (post-unlock — a locked server
    // answers 423). "unknown variant" → the wardrobe server half is not on
    // this build; the beat self-activates at unification.
    let equipReady = false;
    const ctx = await pwRequest.newContext();
    try {
      const res = await ctx.post(`${BASE_URL}/api/dispatch`, {
        data: { type: 'chatOutfitGet', chatId: '00000000-0000-0000-0000-000000000000' },
      });
      const body = (await res.json().catch(() => null)) as {
        type?: string;
        data?: { message?: string };
      } | null;
      const isUnknownVariant =
        body?.type === 'error' && /unknown variant/i.test(String(body?.data?.message ?? ''));
      equipReady = body != null && !isUnknownVariant;
    } catch {
      equipReady = false;
    } finally {
      await ctx.dispose();
    }
    test.skip(
      !equipReady,
      'ACTIVATE-AT-UNIFY (lane P4.9f1): chatOutfitGet/chatEquip not live — the in-chat staging + Done flush walk self-activates when the wardrobe server half lands',
    );

    // Ensure the item from the live beat exists (this file runs serially; the
    // live beat created Brass Goggles). Reach Aria's chat via her
    // Conversations tab so no chat id is hard-coded.
    await openAriaDetail(page);
    await page.getByRole('button', { name: 'Conversations' }).click();
    await page.getByRole('link', { name: /Solo Voyage/ }).click();
    await expect(page).toHaveURL(/\/salon\//);

    // ---- Give Aria something to be wearing FIRST (the deflake) -------------
    //
    // The Live tab stages onto — and captures its flush BASELINE from — the
    // worn snapshot, which lands three round trips after the dialog opens
    // (`chatOutfitGet`, then the store's two wardrobe reads). A "Wear" click
    // that arrives before it is discarded SILENTLY: the late seed overwrites
    // the staged slots, and with no baseline captured `flushStagedLiveOutfits`
    // skips the character entirely — no `set_all` goes out, Done closes as if
    // all were well, and the reopened dialog reads "Accessories · Empty". That
    // is the documented full-suite flake. Measured margin on an idle machine:
    // 3 ms (chatOutfitGet response +312 ms, click +319 ms), so any load loses
    // the race; delaying `chatOutfitGet` by 3 s reproduces it every time.
    //
    // No assertion could gate that click, because an EMPTY worn snapshot is
    // indistinguishable in the DOM from one that has not arrived yet. So seed
    // a worn accessory instead and let the walk wait for it to paint — a state
    // proof rather than a timing guess. THE SEED STAYS for that reason, and
    // only that reason.
    //
    // The lost-edit race itself is FIXED as of v4 4.8.2 (`07d4ccce`, v4 Bug 61
    // — filed from this port's own measurement, dogfood finding #78) and
    // ported here by lane P4.D72: a pre-seed click is now recorded as a
    // mutator and replayed onto the worn snapshot, and a character staged with
    // no baseline is put to the operator instead of closed as if saved. The
    // beat below ("a Wear clicked before the worn snapshot arrives survives
    // the seed") drives that fix directly by holding `chatOutfitGet` open.
    const chatId = /\/salon\/([^/?#]+)/.exec(page.url())?.[1];
    expect(chatId, 'the Solo Voyage chat id, read from the URL').toBeTruthy();

    const characters = ((await dispatch(page, { type: 'characterList' }))['characters'] ??
      []) as Array<{ id: string; name: string }>;
    const ariaId = characters.find((c) => c.name === 'Aria')?.id;
    expect(ariaId, "Aria's character id").toBeTruthy();

    const seeded = (
      await dispatch(page, {
        type: 'characterWardrobeCreate',
        characterId: ariaId,
        item: {
          title: SEEDED_ACCESSORY,
          description: null,
          imagePrompt: null,
          types: ['accessories'],
          appropriateness: null,
          isDefault: false,
          componentItemIds: [],
          replace: false,
        },
      })
    )['wardrobeItem'] as { id: string } | undefined;
    const seededId = seeded?.id;
    expect(seededId, 'the seeded accessory id').toBeTruthy();

    await dispatch(page, {
      type: 'chatEquip',
      chatId,
      characterId: ariaId,
      mode: 'set_all',
      slots: { top: [], bottom: [], footwear: [], accessories: [seededId] },
    });

    // The shell footer Wardrobe button carries the chat scope (and resolves
    // Aria as the default character). getByTitle: the footer button is the
    // only "Wardrobe"-titled control (the detail tab has no title attr).
    await page.getByTitle('Wardrobe', { exact: true }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog.locator('.qt-dialog-title')).toHaveText('Wardrobe');
    // In chat the Live tab is the default (v4 :204) and carries the staging
    // microcopy.
    await expect(dialog.getByText('Edits stage here and apply when you click Done.')).toBeVisible();

    // THE GATE (the deflake). The seeded accessory painting in the Accessories
    // slot is the proof that the worn snapshot arrived AND the Live tab seeded
    // from it — staged slots and flush baseline are captured in the same pass.
    // Only now is a staging gesture safe.
    const liveAccessories = dialog.locator('.qt-card').filter({ hasText: 'Accessories' }).first();
    await expect(liveAccessories).toContainText(SEEDED_ACCESSORY, { timeout: 15_000 });

    // Stage: Wear the goggles (a pure client-side staging gesture). A leaf item
    // layers rather than replacing (`wearItemIntoSlots`, `replace: false`), so
    // the staged slate becomes the seeded accessory PLUS the goggles.
    const gogglesRow = dialog
      .locator('.qt-card-interactive')
      .filter({ hasText: 'Brass Goggles' })
      .first();
    await gogglesRow.getByRole('button', { name: 'Wear', exact: true }).click();
    await expect(liveAccessories).toContainText('Brass Goggles');

    // Done → ONE set_all flush per dirty character, then the dialog closes.
    // Await the flush itself rather than only the close: when the staging is
    // dropped, `flushStagedLiveOutfits` finds nothing dirty and closes happily
    // WITHOUT dispatching, so the close proves nothing. This waiter names that
    // failure here instead of leaving it to surface as an empty slot below.
    const flushed = page.waitForResponse(
      (r) =>
        r.url().includes('/api/dispatch') &&
        (r.request().postData() ?? '').includes('"chatEquip"') &&
        (r.request().postData() ?? '').includes('"set_all"'),
      { timeout: 15_000 },
    );
    await page.getByRole('button', { name: 'Done' }).click();
    await flushed;
    await expect(dialog).toBeHidden({ timeout: 10_000 });

    // Reopen: the remounted dialog seeds the Live tab from the server's worn
    // snapshot — the flush persisted, and it carried the whole staged slate
    // (what the character was already wearing, plus the goggles) rather than
    // just the new item.
    await page.getByTitle('Wardrobe', { exact: true }).click();
    const accessoriesRow = page
      .getByRole('dialog')
      .locator('.qt-card')
      .filter({ hasText: 'Accessories' })
      .first();
    await expect(accessoriesRow).toContainText('Brass Goggles', { timeout: 10_000 });
    await expect(accessoriesRow).toContainText(SEEDED_ACCESSORY);
    await page.getByRole('button', { name: 'Done' }).click();
  });

  /**
   * v4 Bug 61 / dogfood finding #78, end to end (v4 4.8.2 `07d4ccce`, ported by
   * lane P4.D72). The window this drives is the one the deflake above had to
   * step around: the item list is clickable well before the worn snapshot
   * lands, and a Wear clicked in between used to be discarded by the first seed
   * — silently, with Done closing as if it had saved.
   *
   * Deterministic by construction rather than by luck: `chatOutfitGet` is held
   * open with `page.route` across the click, exactly the technique that made
   * the original flake reproduce 4/4 (see the deflake note above). The
   * assertions bracket the release, so the pre-seed paint and the post-seed
   * replay are both named.
   */
  test('in chat: a Wear clicked before the worn snapshot arrives survives the seed (v4 bug 61)', async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await page.goto(`${BASE_URL}/characters`);
    await unlockIfLocked(page);

    await openAriaDetail(page);
    await page.getByRole('button', { name: 'Conversations' }).click();
    await page.getByRole('link', { name: /Solo Voyage/ }).click();
    await expect(page).toHaveURL(/\/salon\//);

    const chatId = /\/salon\/([^/?#]+)/.exec(page.url())?.[1];
    expect(chatId, 'the Solo Voyage chat id, read from the URL').toBeTruthy();
    const characters = ((await dispatch(page, { type: 'characterList' }))['characters'] ??
      []) as Array<{ id: string; name: string }>;
    const ariaId = characters.find((c) => c.name === 'Aria')?.id;
    expect(ariaId, "Aria's character id").toBeTruthy();

    // Two fresh accessories: one Aria is already WEARING (the thing the lost
    // edit used to wipe out or fail to preserve) and one she is not (the click
    // that must survive). Fresh titles so this beat never depends on which
    // earlier beats ran.
    const create = async (title: string): Promise<string> => {
      const made = (
        await dispatch(page, {
          type: 'characterWardrobeCreate',
          characterId: ariaId,
          item: {
            title,
            description: null,
            imagePrompt: null,
            types: ['accessories'],
            appropriateness: null,
            isDefault: false,
            componentItemIds: [],
            replace: false,
          },
        })
      )['wardrobeItem'] as { id: string } | undefined;
      expect(made?.id, `the ${title} id`).toBeTruthy();
      return made!.id;
    };
    const cravatId = await create('Tessellated Cravat');
    const pinId = await create('Clockwork Pin');

    await dispatch(page, {
      type: 'chatEquip',
      chatId,
      characterId: ariaId,
      mode: 'set_all',
      slots: { top: [], bottom: [], footwear: [], accessories: [cravatId] },
    });

    // Hold the FIRST chatOutfitGet open. Everything else — the item list, the
    // chat read, the tier reads — answers normally, which is precisely the
    // asymmetry that creates the window in production.
    let releaseOutfit: (() => void) | null = null;
    const outfitHeld = new Promise<void>((resolve) => {
      releaseOutfit = resolve;
    });
    let stillHolding = true;
    await page.route('**/api/dispatch', async (route) => {
      const body = route.request().postData() ?? '';
      if (stillHolding && body.includes('"chatOutfitGet"')) {
        stillHolding = false;
        await outfitHeld;
      }
      await route.continue();
    });

    await page.getByTitle('Wardrobe', { exact: true }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog.locator('.qt-dialog-title')).toHaveText('Wardrobe');

    // The item list has painted; the worn snapshot has NOT. Click into that gap.
    const pinRow = dialog
      .locator('.qt-card-interactive')
      .filter({ hasText: 'Clockwork Pin' })
      .first();
    await expect(pinRow).toBeVisible({ timeout: 15_000 });
    await pinRow.getByRole('button', { name: 'Wear', exact: true }).click();

    // Painted against the empty fallback — there is nothing else to paint yet.
    const liveAccessories = dialog.locator('.qt-card').filter({ hasText: 'Accessories' }).first();
    await expect(liveAccessories).toContainText('Clockwork Pin');
    await expect(liveAccessories).not.toContainText('Tessellated Cravat');

    // Now let the snapshot land. Pre-fix, this seed discarded the click.
    releaseOutfit?.();
    await expect(liveAccessories).toContainText('Tessellated Cravat', { timeout: 15_000 });
    await expect(liveAccessories).toContainText('Clockwork Pin');

    // Done → ONE set_all carrying BOTH: the cravat she already had on and the
    // pin clicked mid-flight.
    const flushed = page.waitForResponse(
      (r) =>
        r.url().includes('/api/dispatch') &&
        (r.request().postData() ?? '').includes('"chatEquip"') &&
        (r.request().postData() ?? '').includes('"set_all"'),
      { timeout: 15_000 },
    );
    await page.getByRole('button', { name: 'Done' }).click();
    const flushBody = (await flushed).request().postData() ?? '';
    expect(flushBody).toContain(cravatId);
    expect(flushBody).toContain(pinId);
    await expect(dialog).toBeHidden({ timeout: 10_000 });

    await page.unroute('**/api/dispatch');

    // And it stuck: the remounted dialog seeds from the server's snapshot.
    await page.getByTitle('Wardrobe', { exact: true }).click();
    const reopened = page
      .getByRole('dialog')
      .locator('.qt-card')
      .filter({ hasText: 'Accessories' })
      .first();
    await expect(reopened).toContainText('Clockwork Pin', { timeout: 10_000 });
    await expect(reopened).toContainText('Tessellated Cravat');
    await page.getByRole('button', { name: 'Done' }).click();
  });
});

/**
 * Raw dispatch against this spec's own server, issued through the page's
 * context so it inherits the unlocked session. `page.request.*` traffic is
 * invisible to `page.waitForResponse`, so these never satisfy a waiter the
 * walk registered for a page-initiated call.
 */
async function dispatch(page: Page, req: unknown): Promise<Record<string, unknown>> {
  const res = await page.request.post(`${BASE_URL}/api/dispatch`, { data: req });
  const body = (await res.json().catch(() => null)) as { data?: Record<string, unknown> } | null;
  return body?.data ?? {};
}

/**
 * Does this instance actually HAVE a Quilltap General store to write into?
 *
 * It does, as of P4.D130: `beforeAll` materializes the `instance_settings`
 * table the committed `characters-*` pair lacks, so `ensure_builtin_mounts`
 * clears v4's `sqliteTableExists` guard and provisions the three built-in
 * stores at boot. The probe stays — it asks the instance rather than assuming,
 * and it is what the container browser's write half switches on — but it is
 * expected to answer true now, and the container-selector beat asserts the
 * store's existence outright.
 *
 * The probe writes a throwaway archetype and removes it again; a
 * `wardrobeCreate` that fails is the exact signal (`resolve_wardrobe_mount` →
 * `NoMount` → 500).
 */
async function hasGeneralStore(page: Page): Promise<boolean> {
  const created = (
    await dispatch(page, {
      type: 'wardrobeCreate',
      item: {
        title: '__p4d113_probe__',
        description: null,
        imagePrompt: null,
        types: ['accessories'],
        appropriateness: null,
        isDefault: false,
        componentItemIds: [],
        replace: false,
      },
    })
  )['wardrobeItem'] as { id?: string } | undefined;
  if (!created?.id) return false;
  await dispatch(page, { type: 'wardrobeDelete', itemId: created.id });
  return true;
}

/**
 * Can the Move/Copy dialog offer anything at all?
 *
 * P4.D130, MEASURED server-side with no browser: `wardrobeTransferDestinations`
 * answers `Failed to load transfer destinations` on this fixture — with AND
 * without the General store — because `enumerate_destinations` reads `projects`
 * and `groups`, and the committed `characters-main.db` has NEITHER table (nor
 * `instance_settings`; only `characters`). One missing table fails the whole
 * verb, so the destination `<select>` renders with no options and no
 * destination can be chosen.
 *
 * That is the SECOND, independent blocker on the component-transfer beat, and
 * it was hidden behind the General-store park until P4.D130 lifted it. It is a
 * pre-existing fixture-vintage gap, not something this lane introduced.
 *
 * CLOSED at P4.69. `beforeAll` materializes empty `projects`/`groups` from
 * `fresh_schema.json`, so the verb answers and the probe below is an assertion,
 * not a park. P4.D130's two predicted consequences both materialized and were
 * fixed with it: the Copy arm's `option[value^="character:"]` count really is 4
 * (its `0` had been vacuously green — the fetch never returned), now asserted
 * by NAME as well as count; and the move beat, run in the serial chain it
 * belongs to, walks green. No third blocker surfaced (memory note
 * `lifting-a-park-is-a-measurement` — it was expected to, and did not).
 */
async function hasTransferDestinations(page: Page): Promise<boolean> {
  const res = await page.request.post(`${BASE_URL}/api/dispatch`, {
    data: { type: 'wardrobeTransferDestinations' },
  });
  const body = (await res.json().catch(() => null)) as {
    type?: string;
    data?: { destinations?: unknown };
  } | null;
  return body?.type !== 'error' && body?.data?.destinations != null;
}

function runCliWrite(cli: string, sql: string): void {
  const res = spawnSync(cli, ['db', '--data-dir', INSTANCE_DIR, '--write', sql], {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  if (res.status !== 0) {
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
      const res = await fetch(`${BASE_URL}/health`);
      if (res.status === 423 || res.status === 200) return;
      lastErr = `health status ${res.status}`;
    } catch (e) {
      lastErr = e instanceof Error ? e.message : String(e);
    }
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(
    `wardrobe server did not become ready within 30s (${lastErr}); see ${SERVER_LOG}`,
  );
}
