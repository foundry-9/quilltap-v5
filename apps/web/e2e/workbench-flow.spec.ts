import { expect, request as pwRequest, test, type Page } from './support/fixtures';

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
/**
 * §B of the `c53510c7` state-cascade round — does `customToolPreview` honor a
 * `state` body and the `$state` schema? Lane D10 owns the server half, so in-lane
 * the `$state` definition is rejected (the Rust schema lacks `StateRef`) and the
 * mock-state beat skips LOUDLY. It self-activates when D10's arms merge.
 */
let mockStateReady = false;

/**
 * ACTIVATE-AT-UNIFY (P4.D36 unit 8, the `c4d4b0de` round): does
 * `customToolPreview` resolve an effect and hand the bench its dry run?
 *
 * A NAMED CONSTANT rather than a capability probe, per the round's shared
 * contract. It is not probe-able anyway: a server without the feature still
 * answers a success envelope for an effect-bearing definition (unknown
 * top-level keys are TOLERATED by design), so "no error" would prove nothing
 * and a missing `effects` key is indistinguishable from a run whose effects all
 * skipped. **Unifier: flip this to `true` once P4.D35's server half is merged.**
 */
const EFFECTS_PREVIEW_LANDED = false;

/** The name beat 4 authors. Distinct enough that no sibling spec reads it. */
const NEW_TOOL_NAME = 'e2e_probe_contrivance';

/** A fixed-roll, $state-gated definition: difficulty 1 clears, absent falls back. */
const STATE_GATED_DEFINITION = {
  name: 'e2e_state_gate',
  description: 'A state-gated contrivance for the e2e walk.',
  roll: { min: 5, max: 5 },
  outcomes: [
    {
      when: { gte: { $state: 'game.difficulty', fallback: 10 } },
      message: 'Cleared the gate.',
      state: 'success',
    },
    { when: true, message: 'Blocked by the gate.', state: 'info' },
  ],
};

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

  // §B state probe: preview the $state-gated definition with a mock state that
  // clears the gate. A server with the cascade resolves the operand from `state`
  // and the gated row wins; one without it rejects the `$state` definition.
  try {
    const ctx = await pwRequest.newContext();
    const res = await ctx.post(`${BASE_URL}/api/dispatch`, {
      data: {
        type: 'customToolPreview',
        definition: STATE_GATED_DEFINITION,
        state: { game: { difficulty: 1 } },
      },
    });
    const body = (await res.json().catch(() => null)) as
      | { type?: string; data?: { message?: string } }
      | null;
    await ctx.dispose();
    mockStateReady = body?.type !== 'error' && body?.data?.message === 'Cleared the gate.';
  } catch {
    mockStateReady = false;
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
    // llm/metadata eq defaults its literal to NUMBER with a type picker (v4
    // `OutcomesSection.tsx:361`/`:681` — the beat's first live run caught the
    // missing gesture); pick text before filling the answer.
    await page.getByLabel('Literal type').first().selectOption('string');
    await page.getByLabel('Operand text').first().fill('YES');
    await page.getByLabel('Outcome message').first().fill('Assent: {{llm}}.');

    // Script the oracle, then deal one hand.
    await page.getByLabel('Scripted oracle answer').fill('YES');
    await page.getByRole('button', { name: /Roll/ }).first().click();

    // The answer-gated row won, and the bubble records the consult.
    await expect(page.getByText(/Assent: YES/)).toBeVisible({ timeout: 30_000 });
    await expect(page.getByText(/consult answered/)).toBeVisible();
  });

  /**
   * Beat 7 (P4.6be, §B) — a $state operand + mock state steers a preview
   * outcome. The builder never AUTHORS $state, so the definition is pasted in
   * JSON mode; switching to Form renders it as a read-only pill and reveals the
   * bench with its mock-state card. No consult, so no spend. ACTIVATE-AT-UNIFY:
   * the server half (the $state schema + the `state` body) is lane D10's.
   */
  test('a $state operand + mock state decides which row a preview roll lands on', async ({
    page,
  }) => {
    test.skip(
      !workbenchBackendReady,
      'customToolsLibrary dispatch not on this server yet — activates at unification',
    );
    test.skip(
      !mockStateReady,
      'ACTIVATE-AT-UNIFY (P4.6be §B): customToolPreview does not honor a `state` body / `$state` schema yet — lane P4.d10 owns the server half',
    );
    await page.goto('/custom-tools');
    await maybeUnlock(page);

    await page.getByRole('button', { name: 'New contrivance', exact: true }).click();
    await expect(page.getByText('The contrivance itself')).toBeVisible({ timeout: 15_000 });

    // Author the $state definition in JSON mode (the builder never authors it).
    await page.getByRole('radio', { name: 'JSON' }).click();
    await page.getByLabel('Definition JSON').fill(JSON.stringify(STATE_GATED_DEFINITION, null, 2));

    // Back to Form: the $state operand renders as a READ-ONLY pill, and the
    // bench (with its mock-state card) reappears.
    await page.getByRole('radio', { name: 'Form' }).click();
    await expect(page.getByText('$state: game.difficulty → 10')).toBeVisible({ timeout: 15_000 });
    const mock = page.getByLabel('Mock merged state (JSON object)');
    await expect(mock).toBeVisible();

    // Mock state that clears the gate → the gated row wins.
    await mock.fill('{"game": {"difficulty": 1}}');
    await page.getByRole('button', { name: /Roll/ }).first().click();
    await expect(page.getByText('Cleared the gate.')).toBeVisible({ timeout: 30_000 });

    // Empty mock → the fallback (10) holds, and 5 ≥ 10 fails → the catch-all.
    await mock.fill('{}');
    await expect(page.getByText('No mock state supplied')).toBeVisible();
    await page.getByRole('button', { name: /Roll/ }).first().click();
    await expect(page.getByText('Blocked by the gate.').first()).toBeVisible({ timeout: 30_000 });
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

  /**
   * Beat 9 (P4.D36) — the chip label and the Side Effects card, authored.
   *
   * LIVE in-lane: every gesture here is client-side, and the live JSON preview
   * is the assertion — it shows the exact bytes Save would write, so `chipLabel`
   * and `effects` appearing there proves the whole form → draft → definition
   * path, not merely that two controls rendered.
   */
  test('the chip label and the Side Effects card author a labelled, effect-bearing draft', async ({
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

    await page.locator('#wb-title').fill('E2E Effect Contrivance');
    await page.locator('#wb-name').fill('e2e_effect_contrivance');
    await page.locator('#wb-description').fill('An effect-bearing contrivance for the e2e walk.');
    await page.locator('#wb-chip-label').fill('Lockpick — {{value}}');

    // The card sits between the builder form and the outcome table, and starts
    // by saying what an effect is for. SCOPED to the card: the Parameters card
    // opens with "None declared." too, and an unscoped match is ambiguous (the
    // beat's first live run caught it).
    const effectsCard = page
      .locator('section')
      .filter({ has: page.getByRole('heading', { name: 'Side effects' }) });
    await expect(effectsCard).toBeVisible();
    await expect(effectsCard.getByText(/None declared\. An effect lets/)).toBeVisible();

    await page.getByRole('button', { name: /Add effect/ }).click();
    // A blank target offers its two prefixes; taking one seeds the field.
    await page.getByRole('button', { name: 'state.', exact: true }).click();
    const target = page.getByLabel('Effect target');
    await expect(target).toHaveValue('state.');
    await target.fill('state.encounter.count');

    // The quoting trap, said beside the row: bare prose is not an expression.
    const value = page.getByLabel('Effect value');
    await value.fill('broken pick');
    await expect(page.getByText(/the expression does not parse/)).toBeVisible();

    // A real expression clears it.
    await value.fill('{{state.encounter.count}} + 1');
    await expect(page.getByText(/the expression does not parse/)).toHaveCount(0);

    // The outcome row the fresh draft ships must still test something.
    await page.getByRole('button', { name: 'add condition' }).first().click();
    await page.getByLabel('Operand number').first().fill('0.5');
    await page.getByLabel('Outcome message').first().fill('The lock reads {{value}}.');

    // The exact bytes Save would write — both new keys, in the schema's order.
    const preview = page.locator('pre').filter({ hasText: '"$schema"' }).first();
    await expect(preview).toContainText('"chipLabel": "Lockpick — {{value}}"');
    await expect(preview).toContainText('"target": "state.encounter.count"');
    await expect(preview).toContainText('"value": "{{state.encounter.count}} + 1"');
  });

  /**
   * Beat 10 (P4.D36) — the bench's dry run. ACTIVATE-AT-UNIFY behind
   * {@link EFFECTS_PREVIEW_LANDED}: resolving an effect is P4.D35's server half.
   * The gestures are beat 9's, so what the unifier activates has already been
   * walked; only the three assertions at the end are new.
   */
  test('the bench shows what each effect would write, and says it never applies them', async ({
    page,
  }) => {
    test.skip(
      !workbenchBackendReady,
      'customToolsLibrary dispatch not on this server yet — activates at unification',
    );
    test.skip(
      !EFFECTS_PREVIEW_LANDED,
      'ACTIVATE-AT-UNIFY (P4.D36 unit 8): customToolPreview does not resolve effects yet — lane P4.D35 owns the server half',
    );
    await page.goto('/custom-tools');
    await maybeUnlock(page);

    await page.getByRole('button', { name: 'New contrivance', exact: true }).click();
    await expect(page.getByText('The contrivance itself')).toBeVisible({ timeout: 15_000 });

    await page.locator('#wb-title').fill('E2E Dry Run');
    await page.locator('#wb-name').fill('e2e_dry_run');
    await page.locator('#wb-description').fill('A dry-run contrivance for the e2e walk.');
    await page.locator('#wb-chip-label').fill('Dry run — {{value}}');

    await page.getByRole('button', { name: /Add effect/ }).click();
    await page.getByLabel('Effect target').fill('state.encounter.count');
    await page.getByLabel('Value kind').selectOption('literal-number');
    await page.getByLabel('Effect value').fill('4');

    await page.getByRole('button', { name: 'add condition' }).first().click();
    await page.getByLabel('Operand number').first().fill('0.5');
    await page.getByLabel('Outcome message').first().fill('The lock reads {{value}}.');

    await page.getByRole('button', { name: /Roll/ }).first().click();

    // The bubble is headed by the RENDERED chip label, over the message's own
    // block — and the dry run says what would be written without writing it.
    await expect(page.getByText(/→ state\.encounter\.count = 4/)).toBeVisible({
      timeout: 30_000,
    });
    await expect(page.getByText('(would write)')).toBeVisible();
    await expect(page.getByText('The bench computes effects; it never applies them.')).toBeVisible();
  });
});
