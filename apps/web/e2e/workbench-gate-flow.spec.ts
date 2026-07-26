import { expect, request as pwRequest, test, type Page } from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so
 * its filename must sort AFTER foundation.spec.ts (workers: 1, alphabetical file
 * order) — "workbench-gate-flow" ('w') sorts after "foundation" ('f').
 *
 * P4.d20 — a LIVE browser walk of the Workbench's availability gate
 * (v4 `6864bf0e`), kept apart from `workbench-flow.spec.ts` so the lane owns its
 * own beats outright:
 *   1. Authoring beat (LIVE in-lane, no server half needed): "Who may reach for
 *      it" offers three exclusive modes, a gate chip is added and typed, and the
 *      proving bench's verdict line FLIPS as the hand-typed fact sheet changes —
 *      the whole point of the client-safe evaluator being client-safe.
 *   2. Badge beat (ACTIVATE-AT-UNIFY): a gated definition saved into a store
 *      comes back from the library wearing the `gated` badge. The `gate` key on
 *      the library entry is P4.d19's (§2(a)), so in-lane this skips LOUDLY.
 *
 * Beat 2 WRITES a definition file into the seeded store. That is safe on the
 * shared fixture because global-setup copies it FRESH each run
 * (`[[e2e-fixture-before-run-order]]`), and the file is written under a name no
 * other spec reads.
 */

let workbenchBackendReady = false;
/**
 * §2 of the `231be14c` round — does the server evaluate a gate? Lane P4.d19 owns
 * that half, so in-lane a gated definition previews WITHOUT a verdict (v4's top
 * level tolerates unknown keys, so the Rust schema accepts the file and simply
 * ignores the clause) and the badge beat skips LOUDLY, naming the lane. It
 * self-activates the moment D19's arms merge.
 */
let gateBackendReady = false;

/** The name beat 2 authors. Distinct enough that no sibling spec reads it. */
const GATED_TOOL_NAME = 'e2e_gated_contrivance';

test.beforeAll(async () => {
  // Probe: is `customToolsLibrary` handled at all?
  try {
    const ctx = await pwRequest.newContext();
    const res = await ctx.post(`${BASE_URL}/api/dispatch`, {
      data: { type: 'customToolsLibrary' },
    });
    const body = (await res.json().catch(() => null)) as
      | { type?: string; data?: { message?: string } }
      | null;
    await ctx.dispose();
    const isUnknownVariant =
      body?.type === 'error' && /unknown variant/i.test(String(body?.data?.message ?? ''));
    workbenchBackendReady = body != null && !isUnknownVariant;
  } catch {
    workbenchBackendReady = false;
  }

  // §2 probe: preview a gated definition against a sheet that does NOT qualify.
  // A server without the gate answers with a run result and no verdict; only one
  // that honoured the clause can produce `gate.withheldBy`.
  try {
    const ctx = await pwRequest.newContext();
    const res = await ctx.post(`${BASE_URL}/api/dispatch`, {
      data: {
        type: 'customToolPreview',
        definition: {
          name: 'e2e_gate_probe',
          description: 'Probe.',
          availableWhen: { metadata: { rank: { gte: 3 } } },
          roll: { min: 5, max: 5 },
          outcomes: [{ when: true, message: 'Dealt.', state: 'info' }],
        },
        metadata: { rank: 1 },
      },
    });
    const body = (await res.json().catch(() => null)) as
      | { type?: string; data?: { gate?: { withheldBy?: string } } }
      | null;
    await ctx.dispose();
    gateBackendReady = body?.type !== 'error' && body?.data?.gate?.withheldBy === 'availableWhen';
  } catch {
    gateBackendReady = false;
  }
});

/** Unlock only when the passphrase screen is showing (the shared server stays unlocked). */
async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  await expect(passphrase.or(page.locator('qt-shell')).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
    await expect(page.locator('qt-shell')).toBeVisible({ timeout: 15_000 });
  }
}

/**
 * Type a complete `rank >= 3` gate into the section.
 *
 * Every locator is scoped to `qt-gate-section`: the outcome table's chips carry
 * the SAME aria-labels ("Metadata key", "Comparator", "Operand number"), and an
 * unscoped `.first()` would silently follow whichever card the DOM happened to
 * put first ([[added-affordance-breaks-sibling-by-name-locators]]).
 */
async function typeRankGate(page: Page): Promise<void> {
  const gate = page.locator('qt-gate-section');
  // role="radio", not "button": the mode controls are <button>s carrying an
  // explicit radio role, and an explicit role WINS for getByRole.
  await gate.getByRole('radio', { name: 'Only show if…' }).click();
  await gate.getByRole('button', { name: 'add condition' }).click();
  await gate.getByLabel('Metadata key').fill('rank');
  await gate.getByLabel('Comparator').selectOption('gte');
  await gate.getByLabel('Operand number').fill('3');
}

test.describe('P4.d20 — the Workbench availability gate', () => {
  test('the gate section types a test, and the bench verdict flips with the sheet', async ({
    page,
  }) => {
    test.skip(
      !workbenchBackendReady,
      'customToolsLibrary dispatch not on this server yet — activates at unification',
    );
    await page.goto('/custom-tools');
    await maybeUnlock(page);

    await page.getByRole('button', { name: 'New contrivance', exact: true }).click();
    await expect(page.getByText('The contrivance itself')).toBeVisible({ timeout: 15_000 });

    const gate = page.locator('qt-gate-section');
    await expect(gate.getByRole('heading', { name: 'Who may reach for it' })).toBeVisible();

    // Anyone, to start — every file written before the gate existed says this by
    // saying nothing, and a fresh draft agrees.
    await expect(gate.getByRole('radio', { name: 'Anyone' })).toHaveAttribute(
      'aria-checked',
      'true',
    );
    await expect(gate.getByText('Every character in the scene is offered this tool.')).toBeVisible();
    // Nothing gated, so the bench says nothing about a gate.
    await expect(page.getByText('This tool is gated')).toHaveCount(0);

    // role="radio", not "button" — the mode controls are <button>s carrying an
    // explicit radio role, and an explicit role WINS for getByRole.
    await gate.getByRole('radio', { name: 'Only show if…' }).click();
    await expect(
      gate.getByText('Offered only to a character whose sheet passes every test:'),
    ).toBeVisible();
    // A mode with no test yet is a blocking error, said beside the control.
    await expect(gate.getByText('the gate must test something')).toBeVisible();

    await gate.getByRole('button', { name: 'add condition' }).click();
    await gate.getByLabel('Metadata key').fill('rank');
    await gate.getByLabel('Comparator').selectOption('gte');

    // Half-typed: the operand is still blank, so the bench holds its tongue —
    // the form is already complaining, and the bench need not join in.
    await expect(page.getByText('This sheet would')).toHaveCount(0);

    await gate.getByLabel('Operand number').fill('3');

    // The sheet starts as `{}`: no `rank` at all, and a key the character lacks
    // never matches — so an "only show if" withholds the tool.
    await expect(page.getByText('✕ This sheet would never be offered the tool')).toBeVisible();
    await expect(page.getByText('it does not pass every “only show if” test.')).toBeVisible();
    await expect(page.getByText('The roll below is the bench indulging you.')).toBeVisible();

    // Lend the bench a qualifying sheet — worked out RIGHT HERE, no round trip.
    await page.getByLabel('Hand-typed fact sheet (JSON object)').fill('{"rank":4}');
    await expect(page.getByText('✓ This sheet would be offered the tool.')).toBeVisible();

    // And back again, on a sheet that has the key but not the standing.
    await page.getByLabel('Hand-typed fact sheet (JSON object)').fill('{"rank":1}');
    await expect(page.getByText('✕ This sheet would never be offered the tool')).toBeVisible();

    // The other clause reads the same sheet the opposite way.
    await gate.getByRole('radio', { name: 'Do not show if…' }).click();
    await page.getByLabel('Hand-typed fact sheet (JSON object)').fill('{"rank":4}');
    await expect(page.getByText('it passes the “do not show if” test.')).toBeVisible();
  });

  test('a gated definition joins the library wearing the gated badge', async ({ page }) => {
    test.skip(
      !workbenchBackendReady,
      'customToolsLibrary dispatch not on this server yet — activates at unification',
    );
    test.skip(
      !gateBackendReady,
      'the server does not evaluate availability gates yet (P4.d19 §2) — activates at unification',
    );
    await page.goto('/custom-tools');
    await maybeUnlock(page);

    await page.getByRole('button', { name: 'New contrivance', exact: true }).click();
    await expect(page.getByText('The contrivance itself')).toBeVisible({ timeout: 15_000 });

    await page.locator('#wb-title').fill('E2E Gated Contrivance');
    await page.locator('#wb-name').fill(GATED_TOOL_NAME);
    await page.locator('#wb-description').fill('A gated contrivance authored by the e2e walk.');

    await typeRankGate(page);

    // The fresh draft ships one empty non-catch-all row that must test
    // something; give it a condition and a message so the draft validates. The
    // outcome table's own operand field, NOT the gate's.
    const outcomes = page.locator('qt-outcomes-section');
    await outcomes.getByRole('button', { name: 'add condition' }).first().click();
    await outcomes.getByLabel('Operand number').first().fill('0.5');
    await page.getByLabel('Outcome message').first().fill('The probe reads {{value}}.');

    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(
      page.getByRole('heading', { name: 'Where shall Pascal keep this contrivance?' }),
    ).toBeVisible({ timeout: 15_000 });
    await page.getByRole('dialog').locator('.qt-badge').first().click();
    await page.getByRole('button', { name: 'Keep it here' }).click();
    await expect(page.getByText('Pascal has filed the contrivance.')).toBeVisible({
      timeout: 15_000,
    });

    // Back to the library: the row wears the badge, and its title says which
    // clause put it there.
    await page.getByRole('button', { name: 'Library' }).click();
    const row = page
      .locator('.qt-card')
      .filter({ hasText: 'reveal in Scriptorium' })
      .filter({ hasText: 'E2E Gated Contrivance' });
    await expect(row).toBeVisible({ timeout: 15_000 });
    await expect(row.getByText('gated', { exact: true })).toBeVisible();
    await expect(row.getByTitle(/only show if/)).toBeVisible();
  });
});
