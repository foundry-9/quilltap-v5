import { expect, test, type Page } from '@playwright/test';

import { E2E_PASSPHRASE } from './support/env';
import { startMockLlm } from './support/mock-llm';

/**
 * p4.9k4 — the Character Optimizer ("Refine from Memories") end-to-end.
 *
 * ACTIVATE-AT-UNIFY behind {@link P49K1_SERVER_LANDED}: the `characterOptimize`
 * dispatch verb + its `generatorProgress` events are P4.9K1's (a sibling lane in
 * this round, not yet on this branch). A NAMED constant, not a capability probe
 * — a probe cannot tell a verb that is DEFINED-but-refusing from one that
 * genuinely streams, and would silently activate this beat into a guaranteed
 * failure (the standing e2e rule, `character-archive-flow.spec.ts` precedent).
 *
 * The mock LLM's JSON reply below is a BEST-EFFORT shape (an object with a
 * `suggestions` array of `OptimizerSuggestion`s, matching the bug-119
 * `coerceSuggestionArray` key-ordering `['suggestions', ...]`) — the exact
 * shape the real K1 service expects from its own LLM call is server-internal
 * and unverified from the client side until P4.9K1 lands; the unifier should
 * re-check this fixture against the real service's prompt/parse pipeline
 * before trusting this beat's first live run.
 */
const P49K1_SERVER_LANDED = false;

const OPTIMIZER_MOCK_REPLY = JSON.stringify({
  suggestions: [
    {
      id: 'opt-sug-1',
      field: 'description',
      currentValue: '',
      proposedValue: 'A methodical archivist who never raises her voice.',
      rationale: 'Recurring memories show a calm, precise demeanor even under pressure.',
      significance: 0.7,
      memoryExcerpts: ['She catalogued the whole shelf without a single wasted motion.'],
    },
  ],
  analysis: {
    behavioralPatterns: [{ pattern: 'Methodical calm', evidence: 'Consistent across memories', frequency: 'often' }],
    summary: 'Aria consistently favours careful, unhurried precision.',
  },
});

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

async function openAria(page: Page): Promise<void> {
  await page.goto('/salon');
  await maybeUnlock(page);
  await page.goto('/characters');
  await expect(page.getByRole('heading', { name: 'Characters', exact: true })).toBeVisible({
    timeout: 15_000,
  });
  const card = page.locator('qt-character-card').filter({ hasText: 'Aria' }).first();
  await card.locator('h2').click();
  await expect(page.getByRole('heading', { name: 'Aria', level: 1 })).toBeVisible({ timeout: 15_000 });
}

test.describe('p4.9k4 — the character optimizer ("Refine from Memories")', () => {
  test('run → accept one suggestion → apply → the character record carries it', async ({ page }) => {
    test.skip(!P49K1_SERVER_LANDED, 'awaits P4.9K1: the characterOptimize verb + generatorProgress events');

    const mockLlm = await startMockLlm(OPTIMIZER_MOCK_REPLY);
    try {
      await openAria(page);

      await page.getByRole('button', { name: 'Refine from Memories' }).click();
      await expect(page.getByRole('heading', { name: 'Refine from Memories' })).toBeVisible();

      // Preflight: a connection profile is pre-selected from the fixture; commence.
      await page.getByRole('button', { name: 'Commence Refinement' }).click();

      // Review: exactly one suggestion arrives from the mock reply.
      await expect(page.getByText('Proposal 1 of 1')).toBeVisible({ timeout: 20_000 });
      await page.getByRole('button', { name: 'Accept', exact: true }).click();

      await page.getByRole('button', { name: 'Review & Apply Changes' }).click();
      await expect(page.getByRole('button', { name: /Apply 1 Change/ })).toBeVisible();
      await page.getByRole('button', { name: /Apply 1 Change/ }).click();

      await expect(page.getByRole('heading', { name: 'Refinements Commissioned' })).toBeVisible({
        timeout: 15_000,
      });

      // Verify through the persisted state, never the optimistic UI: reload and
      // read the Details tab's rendered Description field back.
      await page.reload();
      await expect(
        page.getByText('A methodical archivist who never raises her voice.'),
      ).toBeVisible({ timeout: 15_000 });
    } finally {
      await mockLlm.close();
    }
  });
});
