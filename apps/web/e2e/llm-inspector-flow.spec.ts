import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so its
 * filename must sort AFTER foundation.spec.ts (foundation walks the
 * locked→unlock gate and must reach the shared server first; workers: 1,
 * alphabetical order). "llm-inspector-flow" sorts after "foundation"
 * ('ll' > 'fo').
 *
 * P4.6as — the LLM-Inspector walk: open via the toolbar button → entries render
 * with their badges → expand one → tab through request/response/usage → filter →
 * the per-message cpu icon opens scrolled + highlighted → Cmd+Shift+L closes.
 *
 * ACTIVATED AT UNIFICATION (P4.6ar∥as∥at): the walk runs LIVE over lane A's
 * `llmLogsList` verb reading the three rows `global-setup.ts` seeds into the
 * llm-logs partition. The lane-era route mock (which mirrored those rows
 * verbatim) was deleted by the unifier, as the lane's order specified.
 */

/** Unlock only when the passphrase screen is showing (the shared server stays unlocked). */
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

/** Navigate to the Solo Voyage conversation (the fixture chat the logs hang off). */
async function openSoloVoyage(page: Page): Promise<void> {
  await page.goto('/salon');
  await page.getByRole('link', { name: 'Solo Voyage' }).first().click();
  await expect(page.getByText('Well met, traveller!')).toBeVisible({ timeout: 15_000 });
}

test.describe('P4.6as — the LLM Inspector (LIVE)', () => {
  test('opens from the toolbar, renders entries, expands, tabs, filters, and closes on the shortcut', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openSoloVoyage(page);

    const panel = page.locator('.qt-slide-over-panel');
    // Mounted but closed — the slide-over animates on data-open.
    await expect(panel).toHaveAttribute('data-open', 'false');

    // 1. Open via the toolbar button (visible by default: the gate is
    //    `enabled !== false` and the fixture sets no llmLoggingSettings).
    const toolbar = page.getByRole('button', { name: 'Toggle LLM Inspector' });
    await expect(toolbar).toBeVisible({ timeout: 15_000 });
    await expect(toolbar).toHaveAttribute('title', 'LLM Inspector (Cmd+Shift+L)');
    await toolbar.click();
    await expect(panel).toHaveAttribute('data-open', 'true');

    // 2. All three entries render, oldest-first (the panel reverses the DESC
    //    rows), with their badges.
    const entries = panel.locator('.qt-inspector-entry');
    await expect(entries).toHaveCount(3);
    await expect(panel).toContainText('3 entries');
    await expect(entries.nth(0)).toContainText('Chat');
    await expect(entries.nth(1)).toContainText('Title');
    await expect(entries.nth(2)).toContainText('Memory');
    // The collapsed row's token summary + one-decimal duration.
    await expect(entries.nth(0)).toContainText('8 → 4');
    await expect(entries.nth(0)).toContainText('1.2s');

    // 3. Expand the first entry → the Request tab opens.
    await entries.nth(0).getByRole('button').first().click();
    await expect(panel.getByRole('button', { name: 'Request' })).toBeVisible();
    await expect(panel).toContainText('Hello there, captain.');
    await expect(panel).toContainText('2048');

    // 4. Tab through Response and Usage.
    await panel.getByRole('button', { name: 'Response' }).click();
    await expect(panel).toContainText('Request completed successfully');
    await expect(panel).toContainText('Response (20 chars)');
    await panel.getByRole('button', { name: 'Usage' }).click();
    await expect(panel).toContainText('Prompt');
    await expect(panel).toContainText('Total');
    // TWO decimals in the usage tab — one in the collapsed row above.
    await expect(panel).toContainText('1.23s');

    // 5. Filter to Memory: only the MEMORY_EXTRACTION entry survives.
    await panel.getByLabel('Filter log entries').selectOption('memory');
    await expect(entries).toHaveCount(1);
    await expect(panel).toContainText('1 entry');
    await expect(entries.nth(0)).toContainText('Memory');
    await panel.getByLabel('Filter log entries').selectOption('all');
    await expect(entries).toHaveCount(3);

    // 6. Cmd+Shift+L closes.
    await page.keyboard.press('Meta+Shift+L');
    await expect(panel).toHaveAttribute('data-open', 'false');
  });

  test('the per-message cpu icon opens the panel scrolled to that message’s entry', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openSoloVoyage(page);

    // Exactly the assistant messages WITH logs carry the icon. The chat-level
    // TITLE_GENERATION log has no messageId, so it contributes none.
    const row = page.locator('qt-message-row').filter({ hasText: 'Well met, traveller!' });
    const cpu = row.getByRole('button', { name: 'View LLM request/response logs' });
    await expect(cpu).toBeVisible({ timeout: 15_000 });

    await cpu.click();

    const panel = page.locator('.qt-slide-over-panel');
    await expect(panel).toHaveAttribute('data-open', 'true');
    // The entry for THAT message is the highlighted one.
    const highlighted = panel.locator('.qt-inspector-entry-highlight');
    await expect(highlighted).toHaveCount(1);
    await expect(highlighted).toHaveAttribute(
      'data-log-id',
      'e1000000-0000-4000-8000-000000000001',
    );

    // The user message never gets the icon (v4's ASSISTANT-only gate).
    await expect(
      page
        .locator('qt-message-row')
        .filter({ hasText: 'Hello there, captain.' })
        .getByRole('button', { name: 'View LLM request/response logs' }),
    ).toHaveCount(0);
  });

  /**
   * P4.D34 (v4 `101cbe3c`): the three qt-* utilities the templates reference.
   *
   * `.qt-text`, `.qt-bg-surface-hover` and both escaped hover variants were
   * referenced across the interface but defined nowhere, so they did nothing
   * silently — the Inspector's panel title fell back to inherited colour, and
   * its muted controls never brightened on hover. This pins them in the BUILT
   * stylesheet, which is where v4 verified the same fix: a plain rule inside
   * `@layer utilities` does not get a `hover:` variant generated for it, so
   * each escaped selector has to survive the pipeline on its own.
   */
  test('the qt-* utilities 101cbe3c defined survive into the built stylesheet', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openSoloVoyage(page);

    const selectors = await page.evaluate(() => {
      const found = new Set<string>();
      const walk = (rules: CSSRuleList) => {
        for (const rule of Array.from(rules)) {
          if (rule instanceof CSSStyleRule) {
            // Split the selector list: the production minifier merges rules
            // that share a declaration block, so `.hover\:qt-bg-surface-hover`
            // arrives comma-joined onto `.hover\:qt-bg-surface-alt`.
            for (const one of rule.selectorText.split(',')) found.add(one.trim());
          }
          const nested = (rule as CSSGroupingRule).cssRules;
          if (nested) walk(nested);
        }
      };
      for (const sheet of Array.from(document.styleSheets)) {
        try {
          walk(sheet.cssRules);
        } catch {
          /* cross-origin sheet — none of ours */
        }
      }
      return Array.from(found);
    });

    for (const wanted of [
      '.qt-text',
      '.qt-bg-surface-hover',
      '.hover\\:qt-text:hover',
      '.hover\\:qt-bg-surface-hover:hover',
    ]) {
      expect(selectors, `${wanted} must be in the generated CSS`).toContain(wanted);
    }

    // And it reaches the element: the Inspector's panel title carries qt-text,
    // and must resolve an opaque colour rather than inheriting whatever the
    // panel happened to set.
    await page.getByRole('button', { name: 'Toggle LLM Inspector' }).click();
    const title = page.locator('.qt-slide-over-panel h2.qt-text');
    await expect(title).toBeVisible({ timeout: 15_000 });
    const colour = await title.evaluate((el) => getComputedStyle(el).color);
    expect(colour).toMatch(/^(rgb|oklch|color)\(/);
    expect(colour).not.toMatch(/rgba?\([^)]*,\s*0\)$/);
  });
});
