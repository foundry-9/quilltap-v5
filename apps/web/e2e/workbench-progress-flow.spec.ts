import { expect, request as pwRequest, test, type Page } from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so
 * its filename must sort AFTER foundation.spec.ts (workers: 1, alphabetical
 * file order) — "workbench-progress-flow" ('w') sorts after "foundation" ('f').
 *
 * P4.D170 — Pascal's `progress` affordances, in the browser (v4 `25f534c0b`).
 *
 * Two kinds of beat, split by what they need:
 *
 *   1. **LIVE in-lane.** The gate chip's subject, the outcome subject, the two
 *      placeholder menu items and the bench's derived-progressions list are all
 *      CLIENT-side: `progressions/engine.ts` and `pascal/tool-gate.ts` derive
 *      and evaluate in the browser, which is the whole reason those modules are
 *      client-safe. No server half is involved, so beat (a) runs from day one.
 *   2. **ACTIVATE-AT-UNIFY behind {@link P4D169_SERVER_LANDED}.** Saving a
 *      `progress` gate into a store and reading it back needs the server to
 *      ACCEPT the key: until P4.D169 lands, `quilltap_core::pascal`'s schema
 *      still refuses `when.progress` and the gate's `progress` subject, and the
 *      roster carries no `progress` vocabulary for the run popup to name.
 *
 * A NAMED constant, never a capability probe: a probe cannot tell a verb that
 * is DEFINED-but-refusing from one that genuinely answers, and would silently
 * activate a beat into a guaranteed failure (the standing e2e rule,
 * `character-archive-flow.spec.ts` precedent).
 *
 * ⚠ `window.prompt` returns `null` in a headless browser by default — the same
 * trap `window.confirm` carries (`dogfood-walk-browser-pane-traps`). Beat (a)
 * therefore STUBS it before touching the placeholder menu; a bare click would
 * insert nothing and the assertion would be measuring the stub's absence.
 */
const P4D169_SERVER_LANDED = false;

/** The name beat (b) would author. Distinct enough that no sibling spec reads it. */
const PROGRESS_TOOL_NAME = 'e2e_progress_contrivance';

/** A cannon that finished long ago, so the derived state is stable on any clock. */
const SHEET = JSON.stringify({
  progressions: {
    cannon: {
      name: 'Cannon recharge',
      startTime: '2020-01-01T14:00:00Z',
      endTime: '2020-01-01T14:10:00Z',
      timeIncrement: 'minute',
    },
  },
});

let workbenchBackendReady = false;

/**
 * The same probe `workbench-gate-flow` runs: is `customToolsLibrary` handled at
 * all? Without it the Workbench screen has nothing to show.
 *
 * Deliberately NOT in a `beforeAll`: a dispatch against a LOCKED instance
 * answers not-ok whatever the verb, so a probe that runs before anything has
 * unlocked reads "the server does not have this feature" when the truth is "the
 * server is locked". In a full-suite run `aa-foundation.spec.ts` has already
 * unlocked; running this file ALONE is where it bites, and a beat that only
 * passes inside the suite is a beat nobody can debug. So each beat unlocks
 * through the browser first, then probes once.
 */
async function probeWorkbenchBackend(): Promise<void> {
  if (workbenchBackendReady) return;
  try {
    const ctx = await pwRequest.newContext();
    const res = await ctx.post(`${BASE_URL}/api/dispatch`, {
      data: { type: 'customToolsLibrary' },
    });
    workbenchBackendReady = res.ok();
    await ctx.dispose();
  } catch {
    workbenchBackendReady = false;
  }
}

/**
 * The screen-AGNOSTIC unlock — `workbench-gate-flow`'s, verbatim. It waits on
 * `qt-shell` rather than the Chats heading, because these beats land on
 * `/custom-tools`, where no such heading exists; the salon-shaped waiter every
 * character spec uses would time out on the very screen it just opened.
 */
async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  await expect(passphrase.or(page.locator('qt-shell')).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
    await expect(page.locator('qt-shell')).toBeVisible({ timeout: 15_000 });
  }
}

test.describe('P4.D170 — the Workbench’s progress affordances', () => {
  test('(a) a progress gate and a progress field, evaluated in the browser', async ({ page }) => {
    // `window.prompt` answers `null` headless; the placeholder insert below
    // opens one, and without this the beat would assert nothing.
    await page.addInitScript(() => {
      window.prompt = () => 'cannon.complete';
    });

    await page.goto('/custom-tools');
    await maybeUnlock(page);
    await probeWorkbenchBackend();
    test.skip(
      !workbenchBackendReady,
      'customToolsLibrary dispatch not on this server yet — activates at unification',
    );

    await page.getByRole('button', { name: 'New contrivance', exact: true }).click();
    await expect(page.getByText('The contrivance itself')).toBeVisible({ timeout: 15_000 });

    // ---- the gate's second subject ---------------------------------------
    const gate = page.locator('qt-gate-section');
    await expect(
      gate.getByText('or their timed progressions — the only things known before a roll exists.', {
        exact: false,
      }),
    ).toBeVisible();

    await gate.getByRole('radio', { name: 'Only show if…' }).click();
    await gate.getByRole('button', { name: 'add condition' }).click();

    // A fresh chip starts on metadata — what a gate has always meant.
    await expect(gate.getByLabel('Gate subject')).toHaveValue('metadata');
    await expect(gate.getByLabel('Metadata key')).toBeVisible();

    await gate.getByLabel('Gate subject').selectOption('progress');
    // The key input re-labels itself, placeholder and all.
    await expect(gate.getByLabel('Progress key')).toBeVisible();
    await expect(gate.getByLabel('Progress key')).toHaveAttribute('placeholder', 'cannon.complete');

    await gate.getByLabel('Progress key').fill('cannon.complete');
    await gate.getByLabel('Comparator').selectOption('eq');
    // A fresh chip already carries `{ kind: 'boolean', value: true }`, so this
    // asserts the seed rather than setting it — and the select's aria-label is
    // `Operand value`, not the widget's kind.
    await expect(gate.getByLabel('Operand value')).toHaveValue('true');

    // ---- the bench derives the sheet, and the verdict answers on it -------
    await expect(
      page.getByText('progress tests read the progressions key inside it.', { exact: false }),
    ).toBeVisible();

    // An empty sheet carries no progressions: no panel, and the gate fails
    // CLOSED — a character who carries no such progression is not offered it.
    await expect(page.getByText('Progressions derived from this sheet')).toHaveCount(0);
    await expect(page.getByText('✕ This sheet would never be offered the tool')).toBeVisible();

    await page.getByLabel('Hand-typed fact sheet (JSON object)').fill(SHEET);

    // The derived list: every field, worked out RIGHT HERE, no round trip.
    await expect(page.getByText('Progressions derived from this sheet')).toBeVisible();
    await expect(page.getByText('cannon.percent', { exact: true })).toBeVisible();
    await expect(page.getByText('cannon.complete', { exact: true }).first()).toBeVisible();
    await expect(page.getByText('cannon.state', { exact: true })).toBeVisible();

    // The cannon finished in 2020, so `complete` is true and the gate opens.
    await expect(page.getByText('✓ This sheet would be offered the tool.')).toBeVisible();

    // ---- the outcome subject and the two placeholder menu items ----------
    const outcomes = page.locator('qt-outcomes-section');
    await outcomes.getByRole('button', { name: 'add condition' }).first().click();
    await outcomes.getByLabel('Condition subject').first().selectOption('progress');
    await expect(outcomes.getByLabel('Progress key').first()).toBeVisible();
    await outcomes.getByLabel('Progress key').first().fill('cannon.percent');

    await outcomes
      .getByRole('button', { name: /Insert/i })
      .first()
      .click();
    await outcomes.getByRole('menuitem', { name: 'Now (epoch ms)' }).click();
    await expect(outcomes.getByRole('textbox', { name: /message/i }).first()).toHaveValue(
      /\{\{now\}\}/,
    );

    await outcomes
      .getByRole('button', { name: /Insert/i })
      .first()
      .click();
    await outcomes.getByRole('menuitem', { name: 'Progress field…' }).click();
    await expect(outcomes.getByRole('textbox', { name: /message/i }).first()).toHaveValue(
      /\{\{progress\.cannon\.complete\}\}/,
    );

    // ---- the effect prefix ------------------------------------------------
    const effects = page.locator('qt-side-effects-section');
    await effects
      .getByRole('button', { name: /Add (an )?effect/i })
      .first()
      .click();
    await effects.getByRole('button', { name: 'progress.', exact: true }).click();
    await expect(effects.getByLabel('Effect target').first()).toHaveValue('progress.');
  });

  test('(b) a progress gate survives a save and a re-open', async ({ page }) => {
    test.skip(
      !P4D169_SERVER_LANDED,
      'the server refuses a `progress` gate until P4.D169 lands — activates at unification',
    );

    await page.goto('/custom-tools');
    await maybeUnlock(page);
    await probeWorkbenchBackend();
    test.skip(
      !workbenchBackendReady,
      'customToolsLibrary dispatch not on this server yet — activates at unification',
    );

    await page.getByRole('button', { name: 'New contrivance', exact: true }).click();
    await expect(page.getByText('The contrivance itself')).toBeVisible({ timeout: 15_000 });

    await page.getByLabel('Name').fill(PROGRESS_TOOL_NAME);
    await page.getByLabel('Description').fill('A probe with a progress gate.');

    const gate = page.locator('qt-gate-section');
    await gate.getByRole('radio', { name: 'Only show if…' }).click();
    await gate.getByRole('button', { name: 'add condition' }).click();
    await gate.getByLabel('Gate subject').selectOption('progress');
    await gate.getByLabel('Progress key').fill('cannon.complete');
    await gate.getByLabel('Comparator').selectOption('eq');
    await expect(gate.getByLabel('Operand value')).toHaveValue('true');

    await page.getByRole('button', { name: /^Save/ }).click();
    await expect(
      page.getByRole('heading', { name: 'Where shall Pascal keep this contrivance?' }),
    ).toBeVisible({ timeout: 15_000 });
    await page
      .getByRole('button', { name: /^Save here/ })
      .first()
      .click();
    await expect(page.getByText('Pascal has filed the contrivance.')).toBeVisible({
      timeout: 15_000,
    });

    // Re-open it: the chip comes BACK on the progress subject, which is what
    // `gateConditionsFromGate` → `gateFromConditions` round-tripping means on a
    // real file rather than in a spec.
    await page.goto('/custom-tools');
    await page.getByText(PROGRESS_TOOL_NAME).first().click();
    await expect(page.locator('qt-gate-section').getByLabel('Gate subject')).toHaveValue(
      'progress',
    );
    await expect(page.locator('qt-gate-section').getByLabel('Progress key')).toHaveValue(
      'cannon.complete',
    );
  });
});
