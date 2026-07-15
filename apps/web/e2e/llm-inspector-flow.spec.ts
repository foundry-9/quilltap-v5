import { expect, test, type Page } from '@playwright/test';

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
 * ACTIVATE-AT-UNIFY: `llmLogsList` is lane A's verb and does not exist in this
 * lane's server, so the dispatch is route-mocked below. The mock's body is the
 * Shared contract §1 shape VERBATIM and it mirrors the three rows
 * `global-setup.ts` already seeds into the llm-logs partition (same ids, types,
 * timestamps and usage) — so at unification the unifier deletes the `mockLogs`
 * call and every assertion here holds against the live verb unchanged.
 */

/** The three seeded rows, DESC by createdAt — exactly what lane A's verb returns. */
const SEEDED_LOGS = [
  {
    id: 'e1000000-0000-4000-8000-000000000003',
    userId: 'ffffffff-ffff-ffff-ffff-ffffffffffff',
    type: 'MEMORY_EXTRACTION',
    messageId: 'd1000000-0000-4000-8000-000000000004',
    chatId: 'c1000000-0000-4000-8000-000000000001',
    provider: 'OPENAI_COMPATIBLE',
    modelName: 'mock-model',
    request: { messageCount: 3, messages: [], toolCount: 0 },
    response: { content: 'No memories extracted.', contentLength: 22 },
    usage: null,
    durationMs: 900,
    createdAt: '2026-02-01T00:00:30.000Z',
    updatedAt: '2026-02-01T00:00:30.000Z',
  },
  {
    id: 'e1000000-0000-4000-8000-000000000002',
    userId: 'ffffffff-ffff-ffff-ffff-ffffffffffff',
    type: 'TITLE_GENERATION',
    messageId: null,
    chatId: 'c1000000-0000-4000-8000-000000000001',
    provider: 'OPENAI_COMPATIBLE',
    modelName: 'mock-model',
    request: { messageCount: 2, messages: [], toolCount: 0 },
    response: { content: 'Solo Voyage', contentLength: 11 },
    usage: null,
    durationMs: 400,
    createdAt: '2026-02-01T00:00:20.000Z',
    updatedAt: '2026-02-01T00:00:20.000Z',
  },
  {
    id: 'e1000000-0000-4000-8000-000000000001',
    userId: 'ffffffff-ffff-ffff-ffff-ffffffffffff',
    type: 'CHAT_MESSAGE',
    messageId: 'd1000000-0000-4000-8000-000000000002',
    chatId: 'c1000000-0000-4000-8000-000000000001',
    provider: 'OPENAI_COMPATIBLE',
    modelName: 'mock-model',
    request: {
      messageCount: 1,
      messages: [
        {
          role: 'user',
          content: 'Hello there, captain.',
          contentLength: 21,
          hasAttachments: false,
        },
      ],
      temperature: 0.7,
      maxTokens: 2048,
      toolCount: 0,
    },
    response: { content: 'Well met, traveller!', contentLength: 20 },
    usage: { promptTokens: 8, completionTokens: 4, totalTokens: 12 },
    durationMs: 1234,
    createdAt: '2026-02-01T00:00:10.000Z',
    updatedAt: '2026-02-01T00:00:10.000Z',
  },
];

/** ACTIVATE-AT-UNIFY: delete this call and the walk runs on the real verb. */
async function mockLogs(page: Page): Promise<void> {
  await page.route('**/api/dispatch', async (route) => {
    const body = route.request().postData() ?? '';
    if (body.includes('llmLogsList')) {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          type: 'llmLogsList',
          data: {
            logs: SEEDED_LOGS,
            count: SEEDED_LOGS.length,
            total: SEEDED_LOGS.length,
            limit: 100,
            offset: 0,
          },
        }),
      });
      return;
    }
    await route.fallback();
  });
}

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

test.describe('P4.6as — the LLM Inspector (ACTIVATE-AT-UNIFY)', () => {
  test('opens from the toolbar, renders entries, expands, tabs, filters, and closes on the shortcut', async ({
    page,
  }) => {
    await mockLogs(page);
    await page.goto('/');
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
    await mockLogs(page);
    await page.goto('/');
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
});
