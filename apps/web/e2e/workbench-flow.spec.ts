import { expect, request as pwRequest, test, type Page } from '@playwright/test';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so
 * its filename must sort AFTER foundation.spec.ts (workers: 1, alphabetical file
 * order) — "workbench-flow" ('w') sorts after "foundation" ('f').
 *
 * P4.6bb — a LIVE browser walk of Pascal's Workbench:
 *   1. Rail beat: the left rail's wrench navigates to `/custom-tools`, and the
 *      library lists the seeded Tools roster.
 *   2. Editor beat: opening a definition renders the editor IN PLACE (no route
 *      change) in FORM mode, with the identity fields populated.
 *   3. Bench beat: the audit returns a hit table whose SHAPE and deterministic
 *      fields hold — never a stochastic value.
 *   4. Authoring beat: create a new contrivance → save into a picked
 *      destination → it appears in the library.
 *   5. Consult beat (P4.6bc, editor-side — LIVE in-lane): enabling "Consult an
 *      LLM" raises the oracle card on the bench and the two consult subjects in
 *      the outcome table.
 *   6. Scripted-oracle beat (P4.6bc, §B — ACTIVATE-AT-UNIFY): a scripted answer
 *      rides the preview body and the answer-gated row wins.
 *
 * PROBE-GUARDED, ACTIVATE-AT-UNIFY (the `m4b-salon` / `home-flow` pattern). The
 * four §W1 verbs land in lane AY (P4.6ay unit 12's route surface); in-lane the
 * shared server answers `customToolsLibrary` with an unknown-variant error, so
 * these beats skip. They auto-activate once lane AY's verbs merge at
 * unification — no unifier wire needed beyond the merge itself.
 *
 * Beat 4 WRITES a definition file into the seeded store. That is safe on the
 * shared fixture because global-setup copies it FRESH each run
 * (`[[e2e-fixture-before-run-order]]`), and the file is written under a name no
 * other spec reads.
 */

let workbenchBackendReady = false;
/**
 * §B of the `616930db` round — does `customToolPreview` accept an `llm` body?
 * Lane D8 owns the server half, so in-lane the field is an unknown one and beat
 * 6 skips LOUDLY (never silently: the skip message names the lane). It
 * self-activates the moment D8's variant merges.
 */
let scriptedOracleReady = false;

/** The name beat 4 authors. Distinct enough that no sibling spec reads it. */
const NEW_TOOL_NAME = 'e2e_probe_contrivance';

test.beforeAll(async () => {
  // Probe: is `customToolsLibrary` handled? In-lane the Rust core has no such
  // variant, so the request fails with "unknown variant" → not ready. A success
  // envelope (or any domain error) means the handler exists → ready.
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

  // §B probe: preview a trivial consulting definition with a scripted oracle.
  // An "unknown field `llm`" rejection means lane D8 has not landed.
  try {
    const ctx = await pwRequest.newContext();
    const res = await ctx.post(`${BASE_URL}/api/dispatch`, {
      data: {
        type: 'customToolPreview',
        definition: {
          name: 'e2e_oracle_probe',
          description: 'Probe.',
          llm: { prompt: 'Say YES.', errorMessage: 'The wire went dead.' },
          outcomes: [{ when: true, message: 'Says {{llm}}.', state: 'info' }],
        },
        llm: { output: 'YES' },
      },
    });
    const body = (await res.json().catch(() => null)) as
      | { type?: string; data?: { message?: string; llm?: unknown } }
      | null;
    await ctx.dispose();
    // NB: a server that merely IGNORES the unknown `llm` field still answers
    // with a success envelope, so "no error" proves nothing. The tell is the
    // §A record coming BACK on the run result — only a server that honoured
    // the scripted oracle can produce it.
    scriptedOracleReady = body?.type !== 'error' && body?.data?.llm != null;
  } catch {
    scriptedOracleReady = false;
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

test.describe("P4.6bb — Pascal's Workbench", () => {
  test('the rail opens the Workbench and the library lists the seeded roster', async ({ page }) => {
    await page.goto('/');
    await maybeUnlock(page);

    // The rail entry (P4.6bb unit 8) is live regardless of the server verbs.
    await page.getByRole('link', { name: "Pascal's Workbench" }).click();
    await expect(page).toHaveURL(/\/custom-tools$/);
    // The heading carries a TYPOGRAPHIC apostrophe (&rsquo;, as v4 writes it),
    // while the rail label is ASCII — so this must not be a plain string match.
    await expect(page.getByRole('heading', { name: /Pascal[’']s Workbench/ })).toBeVisible();

    test.skip(
      !workbenchBackendReady,
      'customToolsLibrary dispatch not on this server yet — activates at unification',
    );

    // The seeded Tools roster (the unifier's seedPascalToolsFixture) lists.
    const rows = page.locator('.qt-card').filter({ hasText: 'reveal in Scriptorium' });
    await expect(rows.first()).toBeVisible({ timeout: 15_000 });
    expect(await rows.count()).toBeGreaterThan(0);
  });

  test('opening a definition renders the editor in place, in form mode', async ({ page }) => {
    test.skip(
      !workbenchBackendReady,
      'customToolsLibrary dispatch not on this server yet — activates at unification',
    );
    await page.goto('/custom-tools');
    await maybeUnlock(page);

    const rows = page.locator('.qt-card').filter({ hasText: 'reveal in Scriptorium' });
    await expect(rows.first()).toBeVisible({ timeout: 15_000 });
    await rows.first().getByTitle('Open on the workbench').first().click();

    // IN PLACE — v4's workspace keep-alive rule: the drill is component state,
    // never a route change per definition.
    await expect(page).toHaveURL(/\/custom-tools$/);

    // Form mode, with the identity section populated from the file.
    await expect(page.getByText('The contrivance itself')).toBeVisible({ timeout: 15_000 });
    await expect(page.getByRole('radio', { name: 'Form' })).toHaveAttribute(
      'aria-checked',
      'true',
    );
    await expect(page.locator('#wb-name')).not.toHaveValue('');
    // By ROLE, not by text: P4.6bc's consult-off hint also contains the phrase
    // "the outcome table", which makes a bare getByText ambiguous
    // ([[added-affordance-breaks-sibling-by-name-locators]]).
    await expect(page.getByRole('heading', { name: 'The outcome table' })).toBeVisible();

    // The repair banner must NOT be showing for a definition that loads.
    await expect(page.getByText('Repair mode.')).toHaveCount(0);
  });

  test('the proving bench audits a definition and reports a hit table', async ({ page }) => {
    test.skip(
      !workbenchBackendReady,
      'customToolsLibrary dispatch not on this server yet — activates at unification',
    );
    await page.goto('/custom-tools');
    await maybeUnlock(page);

    const rows = page.locator('.qt-card').filter({ hasText: 'reveal in Scriptorium' });
    await expect(rows.first()).toBeVisible({ timeout: 15_000 });
    await rows.first().getByTitle('Open on the workbench').first().click();
    await expect(page.getByText('The proving bench')).toBeVisible({ timeout: 15_000 });

    await page.getByRole('button', { name: 'Deal a thousand hands' }).click();

    // Deterministic SHAPE only — the run count is server-fixed (AUDIT_RUNS) and
    // the shares are stochastic, so neither is asserted by value.
    await expect(page.getByText(/[\d,]+ draws · values .* · mean /)).toBeVisible({
      timeout: 30_000,
    });
    // Every outcome row is accounted for, the catch-all under its own label.
    await expect(page.getByText('otherwise').last()).toBeVisible();
    await expect(page.getByText(/%$/).first()).toBeVisible();
  });

  /**
   * Beat 5 (P4.6bc) — everything editor-side about the consult walks LIVE
   * in-lane: the toggle, the card's fields, the bench's oracle card, and the
   * two consult subjects appearing in the outcome table's subject menu.
   */
  test('enabling the consult raises the oracle card and the consult subjects', async ({ page }) => {
    test.skip(
      !workbenchBackendReady,
      'customToolsLibrary dispatch not on this server yet — activates at unification',
    );
    await page.goto('/custom-tools');
    await maybeUnlock(page);

    await page.getByRole('button', { name: 'New contrivance', exact: true }).click();
    await expect(page.getByText('The contrivance itself')).toBeVisible({ timeout: 15_000 });

    // No oracle anywhere until the consult is switched on.
    await expect(page.getByRole('radio', { name: 'Scripted answer' })).toHaveCount(0);
    await expect(page.getByText('Off. Enable it and every run asks a model')).toBeVisible();

    await page.getByLabel('Consult an LLM').check();

    // The card's own fields, in v4's words.
    await expect(page.locator('#wb-llm-prompt')).toBeVisible();
    await expect(page.getByText('When the oracle is silent')).toBeVisible();
    await expect(page.getByText('the consult needs a prompt')).toBeVisible();

    await page.locator('#wb-llm-prompt').fill('In one word, YES or NO: does the mechanism yield?');
    await page.locator('#wb-llm-error').fill('The wire crackles, and no answer comes.');
    await expect(page.getByText('the consult needs a prompt')).toHaveCount(0);

    // The bench's oracle card follows the toggle.
    await expect(page.getByRole('radio', { name: 'Scripted answer' })).toBeVisible();
    await expect(page.getByRole('radio', { name: 'Silence' })).toBeVisible();
    await expect(page.getByRole('radio', { name: 'Ask it live' })).toBeVisible();

    // …and so do the two subjects in the outcome table.
    await page.getByRole('button', { name: 'add condition' }).first().click();
    const subject = page.getByLabel('Condition subject').first();
    await expect(subject.locator('option[value="llm"]')).toHaveCount(1);
    await expect(subject.locator('option[value="llm-ok"]')).toHaveCount(1);

    // Picking the answer subject offers containment, which no numeric subject does.
    await subject.selectOption('llm');
    const comparator = page.getByLabel('Comparator').first();
    await expect(comparator.locator('option[value="contains"]')).toHaveCount(1);
  });

  /**
   * Beat 6 (P4.6bc, §B) — the scripted answer rides the preview body and the
   * answer-gated row wins. ACTIVATE-AT-UNIFY: the server half is lane D8's.
   */
  test('a scripted oracle answer decides which row a preview roll lands on', async ({ page }) => {
    test.skip(
      !workbenchBackendReady,
      'customToolsLibrary dispatch not on this server yet — activates at unification',
    );
    test.skip(
      !scriptedOracleReady,
      'ACTIVATE-AT-UNIFY (P4.6bc §B): customToolPreview does not accept an `llm` body yet — lane P4.d8 owns the server half',
    );
    await page.goto('/custom-tools');
    await maybeUnlock(page);

    await page.getByRole('button', { name: 'New contrivance', exact: true }).click();
    await expect(page.getByText('The contrivance itself')).toBeVisible({ timeout: 15_000 });

    await page.locator('#wb-title').fill('E2E Oracle Contrivance');
    await page.locator('#wb-name').fill('e2e_oracle_contrivance');
    await page.locator('#wb-description').fill('A consulting contrivance for the e2e walk.');

    await page.getByLabel('Consult an LLM').check();
    await page.locator('#wb-llm-prompt').fill('In one word, YES or NO: does the mechanism yield?');
    await page.locator('#wb-llm-error').fill('The wire crackles, and no answer comes.');

    // Row 1 fires only on the consulted answer; the catch-all takes the rest.
    await page.getByRole('button', { name: 'add condition' }).first().click();
    const subject = page.getByLabel('Condition subject').first();
    await subject.selectOption('llm');
    await page.getByLabel('Comparator').first().selectOption('eq');
    await page.getByLabel('Operand text').first().fill('YES');
    await page.getByLabel('Outcome message').first().fill('Assent: {{llm}}.');

    // Script the oracle, then deal one hand.
    await page.getByLabel('Scripted oracle answer').fill('YES');
    await page.getByRole('button', { name: /Roll/ }).first().click();

    // The answer-gated row won, and the bubble records the consult.
    await expect(page.getByText(/Assent: YES/)).toBeVisible({ timeout: 30_000 });
    await expect(page.getByText(/consult answered/)).toBeVisible();
  });

  test('authoring: a new contrivance saves into a picked store and joins the library', async ({
    page,
  }) => {
    test.skip(
      !workbenchBackendReady,
      'customToolsLibrary dispatch not on this server yet — activates at unification',
    );
    await page.goto('/custom-tools');
    await maybeUnlock(page);

    // exact: the per-row "Duplicate as a new contrivance" buttons SUBSTRING-match
    // "New contrivance" once the library has rows (in-lane it was bare, so the
    // ambiguity only surfaced when the beats activated at unification).
    await page.getByRole('button', { name: 'New contrivance', exact: true }).click();
    await expect(page.getByText('The contrivance itself')).toBeVisible({ timeout: 15_000 });

    // Identity. The name slugs from the title while it is still empty (§4.1),
    // but this types it explicitly so the assertion does not ride that rule.
    await page.locator('#wb-title').fill('E2E Probe Contrivance');
    await page.locator('#wb-name').fill(NEW_TOOL_NAME);
    await page.locator('#wb-description').fill('A contrivance authored by the e2e walk.');

    // The fresh draft ships one empty non-catch-all row that must test
    // something; give it a condition and a message so the draft validates.
    await page.getByRole('button', { name: 'add condition' }).first().click();
    const operand = page.getByLabel('Operand number').first();
    await operand.fill('0.5');
    const messages = page.getByLabel('Outcome message');
    await messages.first().fill('The probe reads {{value}}.');

    // Save → the destination picker → keep it in the first offered store.
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(
      page.getByRole('heading', { name: 'Where shall Pascal keep this contrivance?' }),
    ).toBeVisible({ timeout: 15_000 });
    await page.getByRole('dialog').locator('.qt-badge').first().click();
    await page.getByRole('button', { name: 'Keep it here' }).click();

    // Filed — the voice, and the path the editor now holds.
    await expect(page.getByText('Pascal has filed the contrivance.')).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByText(`Tools/${NEW_TOOL_NAME}.tool.json`)).toBeVisible();

    // Back to the library: the new contrivance is on the table. Land on the
    // library FIRST ("New contrivance" renders only there) — a bare
    // getByText(title) is ambiguous while the editor is still up (header, the
    // title hint, the live JSON preview), and a strict-mode violation fails
    // immediately rather than retrying through the navigation.
    await page.getByRole('button', { name: 'Library' }).click();
    await expect(page.getByRole('button', { name: 'New contrivance', exact: true })).toBeVisible({
      timeout: 15_000,
    });
    await expect(
      page.getByRole('button', { name: /E2E Probe Contrivance/ }).first(),
    ).toBeVisible({ timeout: 15_000 });
  });
});
