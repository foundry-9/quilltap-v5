import { expect, test, type Page } from '@playwright/test';

import { createServer, type Server } from 'node:http';

import { E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';

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
 * The mock LLM's ONE fixed reply must satisfy BOTH of v4's parses, because the
 * service re-asks the same model on every call (the `p4.9k` round's §3 review
 * measured this against `lib/services/character-optimizer.service.ts`):
 * `parseLLMJson<OptimizerAnalysis>(analysisRaw)` reads TOP-LEVEL
 * `behavioralPatterns` / `summary` (`:834`, `:1133`), and every sub-step's
 * `coerceSuggestionArray` takes the `suggestions` array off a wrapper object
 * (`:944-945`). So the reply is flat: `{behavioralPatterns, summary,
 * suggestions}`. Because v4 re-mints an id per suggestion per sub-step
 * (`:968`), Aria yields one proposal PER sub-step (≥ 5) — the counter is
 * asserted as `1 of N`, not `1 of 1`.
 *
 * The service's `MIN_REINFORCED_MEMORIES = 2` (`:145`) needs at least two of
 * Aria's memories with `reinforcementCount >= 2`; the salon fixture's two
 * carry the schema default `1`. No dispatch verb writes that column (v4's
 * `createMemorySchema` accepts it but the port's create bag does not carry
 * it), so global setup raises the two counts with the pre-server CLI write
 * (`global-setup.ts`, the same path every fixture migration takes) — the
 * `2f4254b42` unification's discharge of this beat's owed recipe.
 *
 * The optimizer's model calls are NON-streaming (`send_message`), so the
 * shared SSE mock cannot answer them; `startNonStreamingMockLlm` below is
 * this spec's own copy of the wizard beat's non-streaming shape, bound to
 * the fixture profile's `MOCK_LLM_PORT`.
 */
const P49K1_SERVER_LANDED = true;

const OPTIMIZER_MOCK_REPLY = JSON.stringify({
  behavioralPatterns: [{ pattern: 'Methodical calm', evidence: 'Consistent across memories', frequency: 'often' }],
  summary: 'Aria consistently favours careful, unhurried precision.',
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
});

/**
 * A non-streaming OPENAI-compatible chat-completions mock answering every
 * `POST /v1/chat/completions` with ONE fixed completion (the wizard beat's
 * shape — the shared `support/mock-llm.ts` only speaks SSE).
 */
async function startNonStreamingMockLlm(
  reply: string,
  port: number,
): Promise<{ url: string; close: () => Promise<void> }> {
  const httpServer: Server = createServer((req, res) => {
    if (req.method === 'GET' && req.url?.includes('/models')) {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ data: [{ id: 'mock-model' }] }));
      return;
    }
    if (req.method !== 'POST' || !req.url?.includes('/chat/completions')) {
      res.writeHead(404).end();
      return;
    }
    req.on('data', () => {});
    req.on('end', () => {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(
        JSON.stringify({
          id: 'mock-optimizer-1',
          object: 'chat.completion',
          model: 'mock-model',
          choices: [{ index: 0, message: { role: 'assistant', content: reply }, finish_reason: 'stop' }],
          usage: { prompt_tokens: 20, completion_tokens: 12, total_tokens: 32 },
        }),
      );
    });
  });
  const boundPort: number = await new Promise((res) => {
    httpServer.listen(port, '127.0.0.1', () => {
      const addr = httpServer.address();
      res(typeof addr === 'object' && addr ? addr.port : 0);
    });
  });
  return {
    url: `http://127.0.0.1:${boundPort}`,
    close: () => new Promise<void>((res, rej) => httpServer.close((e) => (e ? rej(e) : res()))),
  };
}

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

    const mockLlm = await startNonStreamingMockLlm(OPTIMIZER_MOCK_REPLY, MOCK_LLM_PORT);
    try {
      await openAria(page);

      await page.getByRole('button', { name: 'Refine from Memories' }).click();
      await expect(page.getByRole('heading', { name: 'Refine from Memories' })).toBeVisible();

      // Preflight: a connection profile is pre-selected from the fixture; commence.
      await page.getByRole('button', { name: 'Commence Refinement' }).click();

      // Review: exactly one suggestion arrives from the mock reply.
      await expect(page.getByText(/Proposal 1 of \d+/)).toBeVisible({ timeout: 20_000 });
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
