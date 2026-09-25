import { expect, test, type APIRequestContext, type Page } from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { MOCK_LLM_REPLY, startMockLlm, type MockLlm } from './support/mock-llm';

/**
 * P4.D231 — the Host's Scenario Builder on the scenario SHELVES (v4
 * `08c49319d`): "Ask the Host to set the scene" on the General Scenarios page,
 * on a project's Scenarios card, and on the NEW group Scenarios card.
 * Launched from a shelf the builder has no "Use this scene" — its review
 * footer is Close + a primary Save as scenario… — and Save offers every home
 * with the shelf's own preselected once the list offering it has arrived.
 *
 * The canned LLM answers every completion with {@link MOCK_LLM_REPLY} as plain
 * text and never calls a tool, so a build completes deterministically with the
 * scene = the reply (P4.D218's measurement).
 *
 * ORDERING: rides the shared global-setup server, so the filename must sort
 * after `aa-foundation.spec.ts`. Every beat makes its OWN project / group and
 * deletes it before it ends; beat (a) files nothing, so no spec counting
 * General scenarios sees this one.
 *
 * Runs LIVE — this lane owns the server half too (no `…_SERVER_LANDED` gate).
 */

async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const chats = page.getByRole('heading', { name: 'Chats', exact: true });
  await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
    await expect(chats).toBeVisible({ timeout: 15_000 });
  }
}

/** Unlock first — the shared server is passphrase-locked until a spec opens it. */
async function openApp(page: Page): Promise<void> {
  await page.goto('/salon');
  await maybeUnlock(page);
}

async function dispatch(
  ctx: APIRequestContext,
  req: unknown,
): Promise<{ type?: string; data?: Record<string, unknown> }> {
  const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: req });
  expect(res.ok()).toBe(true);
  return ((await res.json().catch(() => null)) ?? {}) as {
    type?: string;
    data?: Record<string, unknown>;
  };
}

function builderDialog(page: Page) {
  return page.locator('[role="dialog"]').filter({ hasText: 'The Host sets the scene' });
}

function builderFooterButton(page: Page, name: string) {
  return builderDialog(page)
    .locator('[qt-modal-footer]')
    .getByRole('button', { name, exact: true });
}

function saveDialog(page: Page) {
  return page.locator('[role="dialog"]').filter({ hasText: 'File this scene as a scenario' });
}

/** Build from the open builder and wait for the canned scene on the review pane. */
async function buildToReview(page: Page): Promise<void> {
  const dialog = builderDialog(page);
  await expect(dialog).toBeVisible();
  await dialog.locator('#scenario-builder-location').fill('the orangery at Blandings');
  await dialog.locator('#scenario-builder-time').fill('a close August afternoon');
  await expect(dialog.locator('#scenario-builder-profile')).not.toHaveValue('');
  await builderFooterButton(page, 'Set the scene').click();
  const scene = dialog.getByLabel('The scene');
  await expect(scene).toBeVisible({ timeout: 30_000 });
  await expect(scene).toContainText(MOCK_LLM_REPLY);
}

/** Expand a collapsible card by its title prefix, if it is not already open. */
async function openCard(page: Page, titlePrefix: string, inside: string): Promise<void> {
  const host = page.getByRole('button', { name: 'Ask the Host to set the scene' });
  if (await host.count()) return;
  await page
    .locator(`${inside} .qt-collapsible-card-header`)
    .filter({ hasText: titlePrefix })
    .first()
    .click();
}

test.describe('P4.D231 — the Host on the scenario shelves', () => {
  let mock: MockLlm;

  test.beforeAll(async () => {
    mock = await startMockLlm(MOCK_LLM_REPLY, MOCK_LLM_PORT);
  });
  test.afterAll(async () => {
    await mock?.close();
  });

  test('(a) General page: the Host builds, and the review offers Close + Save as scenario… — no Use', async ({
    page,
  }) => {
    await openApp(page);
    await page.goto('/scenarios');
    await expect(page.getByRole('heading', { name: 'General Scenarios' })).toBeVisible();

    // The Host button sits BEFORE + New scenario in the toolbar's row.
    const row = page.locator('qt-scenarios-manager .flex.items-center.gap-2.flex-wrap').first();
    await expect(row.getByRole('button')).toHaveText([
      'Ask the Host to set the scene',
      '+ New scenario',
    ]);
    await row.getByRole('button', { name: 'Ask the Host to set the scene' }).click();
    // No cast, no project, no group: the General shelf opens in real mode.
    await expect(builderDialog(page).locator('input[value="real"]')).toBeChecked();
    await buildToReview(page);

    await expect(builderFooterButton(page, 'Use this scene')).toHaveCount(0);
    await expect(builderFooterButton(page, 'Cancel')).toHaveCount(0);
    await expect(builderFooterButton(page, 'Save as scenario…')).toHaveCount(1);
    await expect(builderFooterButton(page, 'Save as scenario…')).toHaveClass(/qt-button-primary/);

    // Save preselects General (the shelf's own home).
    await builderFooterButton(page, 'Save as scenario…').click();
    await expect(saveDialog(page)).toBeVisible();
    await expect(saveDialog(page).locator('#save-scenario-target')).toHaveValue('general');
    await saveDialog(page)
      .locator('[qt-modal-footer]')
      .getByRole('button', { name: 'Cancel', exact: true })
      .click();
    await expect(saveDialog(page)).toHaveCount(0);

    await builderFooterButton(page, 'Close').click();
    await expect(builderDialog(page)).toHaveCount(0);
  });

  test('(b) a project card: Save preselects "Project: <name>" once its option exists', async ({
    page,
  }) => {
    await openApp(page);
    const name = `D231 Estate ${Date.now()}`;
    const created = await dispatch(page.request, { type: 'projectCreate', project: { name } });
    const projectId = (created.data?.['project'] as { id: string }).id;
    try {
      await page.goto(`/prospero/${projectId}`);
      await expect(page.getByRole('heading', { name })).toBeVisible({ timeout: 15_000 });
      await openCard(page, 'Scenarios (', 'qt-project-scenarios-card');
      await page
        .locator('qt-project-scenarios-card')
        .getByRole('button', { name: 'Ask the Host to set the scene' })
        .click();
      // A project brings stores of its own: in-world.
      await expect(builderDialog(page).locator('input[value="in-world"]')).toBeChecked();
      await buildToReview(page);

      await builderFooterButton(page, 'Save as scenario…').click();
      const select = saveDialog(page).locator('#save-scenario-target');
      // Wait for the option to EXIST before reading the value — the flip
      // from General happens when the project list lands (v4's timing).
      await expect(
        select.locator('optgroup[label="Projects"] option', { hasText: `Project: ${name}` }),
      ).toHaveCount(1);
      await expect(select).toHaveValue(`project:${projectId}`);
      await saveDialog(page)
        .locator('[qt-modal-footer]')
        .getByRole('button', { name: 'Cancel', exact: true })
        .click();
      await builderFooterButton(page, 'Close').click();
      await expect(builderDialog(page)).toHaveCount(0);
    } finally {
      await dispatch(page.request, { type: 'projectDelete', projectId });
    }
  });

  test('(c) the group card: a save files to the group and the shelf shows the new row', async ({
    page,
  }) => {
    await openApp(page);
    const name = `D231 Aeronauts ${Date.now()}`;
    const created = await dispatch(page.request, { type: 'groupCreate', name });
    const groupId = (created.data?.['group'] as { id: string }).id;
    try {
      await page.goto(`/characters/groups/${groupId}`);
      const card = page.locator('qt-group-scenarios-card');
      // Collapsed by default, after the stores card; the count reads 0.
      await expect(card).toContainText('Scenarios (0)', { timeout: 15_000 });
      await expect(card).toContainText(
        'Reusable starting scenes offered whenever a member takes a seat',
      );
      await expect(card.getByRole('button', { name: 'Ask the Host to set the scene' })).toHaveCount(
        0,
      );
      await card.locator('.qt-collapsible-card-header').first().click();
      await expect(card).toContainText(
        "No scenarios yet. Create one and it'll be offered whenever a member of this group joins a new chat.",
      );
      await card.getByRole('button', { name: 'Ask the Host to set the scene' }).click();
      await expect(builderDialog(page).locator('input[value="in-world"]')).toBeChecked();
      await buildToReview(page);

      await builderFooterButton(page, 'Save as scenario…').click();
      const save = saveDialog(page);
      const select = save.locator('#save-scenario-target');
      await expect(
        select.locator('optgroup[label="Groups"] option', { hasText: `Group: ${name}` }),
      ).toHaveCount(1);
      await expect(select).toHaveValue(`group:${groupId}`);
      const scene = `The Host’s orangery ${Date.now()}`;
      await save.locator('#save-scenario-name').fill(scene);
      await save
        .locator('[qt-modal-footer]')
        .getByRole('button', { name: 'Save', exact: true })
        .click();
      await expect(page.getByText(`“${scene}” has been filed among the scenarios.`)).toBeVisible({
        timeout: 10_000,
      });
      await expect(save).toHaveCount(0);
      // The builder stays open after a save.
      await expect(builderDialog(page).getByLabel('The scene')).toBeVisible();
      await builderFooterButton(page, 'Close').click();
      await expect(builderDialog(page)).toHaveCount(0);

      // The silent refresh: the shelf gained the row.
      await expect(card).toContainText(scene, { timeout: 10_000 });
      await expect(card).toContainText('Scenarios (1)');
    } finally {
      await dispatch(page.request, { type: 'groupDelete', groupId });
    }
  });
});
