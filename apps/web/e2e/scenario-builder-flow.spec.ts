import { expect, test, type Page } from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { MOCK_LLM_REPLY, startMockLlm, type MockLlm } from './support/mock-llm';

/**
 * P4.D218 — the Host's Scenario Builder (v4 `d1c06cd9d`): "Ask the Host to set
 * the scene" on the New Chat form and in the Salon sidebar's scenario control.
 *
 * The canned LLM answers every completion with {@link MOCK_LLM_REPLY} as plain
 * text and never calls a tool, so the Host's loop takes the reply as its
 * answer and a build completes deterministically with the scene = the reply
 * (the order's §11 measurement). Beat (d) swaps in a SLOW mock so the run is
 * still out when Stop is pressed.
 *
 * ORDERING: rides the shared global-setup server, so the filename must sort
 * after `aa-foundation.spec.ts`. It sorts BEFORE `scenarios-flow.spec.ts`
 * ('scenario-' < 'scenarios'), so beat (c) deletes the General scenario it
 * files before it finishes — nothing it writes is left for that spec to count.
 * Beat (b) sends into a chat it CREATES (never a shared fixture chat: a
 * scenario change posts a Host record and recompiles every seat, which other
 * beats' chats must not wear).
 *
 * **ACTIVATE-AT-UNIFY.** The three verbs (`scenarioBuilderBuild`,
 * `scenarioBuilderAbort`, `scenarioBuilderCapabilities`) and the progress
 * event are P4.D217's, and `groupList { characterIds }` is P4.D216's — none of
 * them exist on main while this lane runs. Gated by the NAMED constant rather
 * than a capability probe: a DEFINED-but-unimplemented verb defeats a probe
 * (the `P49K2_SERVER_LANDED` / `P4D205_SERVER_LANDED` precedent). **The
 * unifier flips it to `true` after P4.D216 + P4.D217 are picked, and the beats'
 * first live run is the unified gate's own step.**
 */
const P4D217_SERVER_LANDED = false;

const GATE_REASON =
  'awaits P4.D217 (scenarioBuilderBuild / scenarioBuilderAbort / scenarioBuilderCapabilities + the scenarioBuilderProgress event) and P4.D216 (groupList { characterIds })';

async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const chats = page.getByRole('heading', { name: 'Chats', exact: true });
  await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

/** /salon → New Chat → pick the first roster character (its profile auto-seeds). */
async function openNewChatWithACharacter(page: Page): Promise<void> {
  await page.goto('/salon');
  await maybeUnlock(page);
  await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
  await page.getByRole('link', { name: 'New Chat' }).first().click();
  await expect(page.getByRole('heading', { name: 'New Chat', exact: true })).toBeVisible();
  await page.locator('.new-chat-character-picker button').first().click();
  await expect(page.getByText('Speaks First')).toBeVisible();
}

function builderDialog(page: Page) {
  return page.locator('[role="dialog"]').filter({ hasText: 'The Host sets the scene' });
}

/** The builder's footer buttons (the header ✕ is also a button). */
function builderFooterButton(page: Page, name: string) {
  return builderDialog(page)
    .locator('[qt-modal-footer]')
    .getByRole('button', { name, exact: true });
}

/** Fill the two required inputs and check a tool-using model is preselected. */
async function fillTheInputs(page: Page): Promise<void> {
  const dialog = builderDialog(page);
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText('Is the place real, or of your own world?')).toBeVisible();
  await dialog.locator('#scenario-builder-location').fill('the Lantern Inn at Vey’s Crossing');
  await dialog.locator('#scenario-builder-time').fill('an autumn evening, 1927');
  // The fixture's profile is preselected by v4's default rule.
  await expect(dialog.locator('#scenario-builder-profile')).not.toHaveValue('');
}

/** Build, and wait for the review pane to carry the canned scene. */
async function buildToReview(page: Page): Promise<void> {
  await fillTheInputs(page);
  await builderFooterButton(page, 'Set the scene').click();
  const scene = builderDialog(page).getByLabel('The scene');
  await expect(scene).toBeVisible({ timeout: 30_000 });
  await expect(scene).toContainText(MOCK_LLM_REPLY);
  await expect(
    builderDialog(page).getByText('Here is the scene as I found it.', { exact: false }),
  ).toBeVisible();
}

test.describe('P4.D218 — the Host sets the scene', () => {
  let mock: MockLlm;

  test.beforeAll(async () => {
    mock = await startMockLlm(MOCK_LLM_REPLY, MOCK_LLM_PORT);
  });
  test.afterAll(async () => {
    await mock?.close();
  });

  test('(a) New Chat: build, then Use puts the scene in the form as custom text', async ({
    page,
  }) => {
    test.skip(!P4D217_SERVER_LANDED, GATE_REASON);
    await openNewChatWithACharacter(page);

    await page.getByRole('button', { name: 'Ask the Host to set the scene' }).click();
    await buildToReview(page);

    await builderFooterButton(page, 'Use this scene').click();
    await expect(builderDialog(page)).toHaveCount(0);

    // The scene is the custom text; no preset stays selected.
    await expect(page.getByLabel('Starting scenario')).toContainText(MOCK_LLM_REPLY);
    const presets = page.locator('#new-chat-scenario-select');
    if (await presets.count()) await expect(presets).toHaveValue('__custom__');
  });

  test('(b) Salon: build, Use fills the custom box, and Change scenario has the Host announce it', async ({
    page,
  }) => {
    test.skip(!P4D217_SERVER_LANDED, GATE_REASON);
    // A chat of this beat's own (see the header).
    await openNewChatWithACharacter(page);
    await page.getByRole('button', { name: 'Create Chat' }).click();
    await expect(page).toHaveURL(/\/salon\/[0-9a-f-]{16,}/, { timeout: 20_000 });
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 20_000 });

    const expand = page.getByRole('button', { name: 'Expand chat sidebar' });
    if (await expand.count()) await expand.click();
    await page
      .locator('qt-chat-sidebar .qt-collapsible-card-header')
      .filter({ hasText: 'Chat' })
      .first()
      .click();
    const control = page.locator('qt-chat-scenario-control');
    await expect(control).toBeVisible({ timeout: 15_000 });

    await control.getByRole('button', { name: 'Ask the Host to set the scene' }).click();
    await buildToReview(page);
    await builderFooterButton(page, 'Use this scene').click();
    await expect(builderDialog(page)).toHaveCount(0);

    // Use saved nothing: the scene sits in the custom box, waiting for Change.
    await expect(control.locator('textarea')).toHaveValue(MOCK_LLM_REPLY);
    await control.getByRole('button', { name: 'Change scenario' }).click();
    await expect(page.getByText('Scenario updated')).toBeVisible({ timeout: 10_000 });

    const chip = page
      .locator('.qt-chat-announcement-chip')
      .filter({ has: page.locator('.qt-chat-system-bar-kind', { hasText: 'scenario change' }) })
      .last();
    await expect(chip).toBeVisible({ timeout: 10_000 });
    await chip.click();
    await expect(page.getByText('The Host revises the scene for the proceedings:')).toBeVisible({
      timeout: 10_000,
    });
  });

  test('(c) Save as scenario… to Quilltap General: toast, then the New Chat picker selects it', async ({
    page,
  }) => {
    test.skip(!P4D217_SERVER_LANDED, GATE_REASON);
    await openNewChatWithACharacter(page);
    await page.getByRole('button', { name: 'Ask the Host to set the scene' }).click();
    await buildToReview(page);

    await builderFooterButton(page, 'Save as scenario…').click();
    const save = page
      .locator('[role="dialog"]')
      .filter({ hasText: 'File this scene as a scenario' });
    await expect(save).toBeVisible();
    const name = `The Host’s beat ${Date.now()}`;
    await save.locator('#save-scenario-name').fill(name);
    await expect(save.locator('#save-scenario-target')).toHaveValue('general');
    await save
      .locator('[qt-modal-footer]')
      .getByRole('button', { name: 'Save', exact: true })
      .click();

    await expect(page.getByText(`“${name}” has been filed among the scenarios.`)).toBeVisible({
      timeout: 10_000,
    });
    await expect(save).toHaveCount(0);

    // The re-read General tier offers it, so the form selects it as the preset.
    await builderFooterButton(page, 'Cancel').click();
    const presets = page.locator('#new-chat-scenario-select');
    await expect(presets).toHaveValue(/^general:/, { timeout: 10_000 });
    await expect(presets.locator('option:checked')).toContainText(name);

    // Leave nothing behind for the specs that count General scenarios.
    const path = (await presets.inputValue()).slice('general:'.length);
    const res = await page.request.post(`${BASE_URL}/api/dispatch`, {
      data: { type: 'scenarioDelete', scenarioPath: path },
    });
    expect(res.ok()).toBe(true);
  });
});

test.describe('P4.D218 — Stop while the Host is out', () => {
  let slow: MockLlm;

  test.beforeAll(async () => {
    // Seven words, ~1.5 s apart: the run is still out when Stop is pressed.
    slow = await startMockLlm(MOCK_LLM_REPLY, MOCK_LLM_PORT, 1_500);
  });
  test.afterAll(async () => {
    await slow?.close();
  });

  test('(d) Stop returns to the inputs with no draft', async ({ page }) => {
    test.skip(!P4D217_SERVER_LANDED, GATE_REASON);
    await openNewChatWithACharacter(page);
    await page.getByRole('button', { name: 'Ask the Host to set the scene' }).click();
    await fillTheInputs(page);
    await builderFooterButton(page, 'Set the scene').click();

    const dialog = builderDialog(page);
    await expect(dialog.getByText('Pray bear with me; I am out making enquiries.')).toBeVisible();
    await expect(dialog.getByText('The Host is out making enquiries…').first()).toBeVisible();

    await builderFooterButton(page, 'Stop').click();

    // Back on the inputs, as filled; no draft and no error.
    await expect(dialog.locator('#scenario-builder-location')).toHaveValue(
      'the Lantern Inn at Vey’s Crossing',
    );
    await expect(dialog.getByLabel('The scene')).toHaveCount(0);
    await expect(dialog.locator('[role="alert"]')).toHaveCount(0);
    // And it stays that way once the slow reply would have finished.
    await page.waitForTimeout(12_000);
    await expect(dialog.getByLabel('The scene')).toHaveCount(0);
    await expect(builderFooterButton(page, 'Set the scene')).toBeEnabled();
  });
});
