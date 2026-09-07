import { expect, test, type Page } from '@playwright/test';

import { E2E_PASSPHRASE } from './support/env';
import { openSidebarSection } from './support/sidebar';
import { startMockLlm } from './support/mock-llm';

/**
 * p4.9k4 — "Summon from Lore" from a Salon chat's Add-Character picker: the AI
 * Import wizard conjures a character, the picker preselects it, and confirming
 * "Add Character" joins it to the cast — verified through the chat GET.
 *
 * ACTIVATE-AT-UNIFY behind {@link P49K2_SERVER_LANDED}: `aiImportStream` is
 * P4.9K2's (a sibling lane, not yet on this branch). A NAMED constant, never a
 * capability probe — the standing e2e rule for this round.
 *
 * The walk adds a character to "Group Expedition" and then REMOVES it again
 * (the `salon-cast-flow.spec.ts` precedent) — `salon-post-office-flow.spec.ts`
 * asserts v4's whisper gate on the SAME shared server keyed to this chat's
 * exact roster size, so a beat that left an extra participant behind would
 * break a sibling spec through the fixture rather than through the code.
 *
 * The mock LLM's JSON reply is a best-effort shape (a `QuilltapExport`-ish
 * object with one character); the exact shape P4.9K2's service expects from
 * its own LLM call is server-internal and unverified from the client side
 * until that lane lands — the unifier should re-check this fixture against
 * the real service's prompt/parse pipeline before trusting this beat's first
 * live run.
 */
const P49K2_SERVER_LANDED = false;

const AI_IMPORT_MOCK_REPLY = JSON.stringify({
  character: {
    name: 'Marchpane',
    identity: 'A travelling confectioner of some renown.',
    description: 'Sweet-tempered and precise.',
    personality: 'Endlessly patient, quietly ambitious.',
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

async function openChat(page: Page, title: string): Promise<void> {
  await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
  const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: title });
  await expect(card).toBeVisible();
  await card.click();
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible();
}

test.describe('p4.9k4 — Summon from Lore joins the cast', () => {
  test('conjure a character, confirm Add Character, the chat GET carries it, then clean up', async ({
    page,
  }, testInfo) => {
    test.skip(!P49K2_SERVER_LANDED, 'awaits P4.9K2: the aiImportStream verb + generatorProgress events');
    test.setTimeout(60_000);

    const mockLlm = await startMockLlm(AI_IMPORT_MOCK_REPLY);
    try {
      await page.goto('/salon');
      await maybeUnlock(page);
      await openChat(page, 'Group Expedition');
      await openSidebarSection(page, 'Participants');

      const castNames = page.locator('qt-chat-sidebar .qt-participant-card-name');
      const before = await castNames.count();

      await page.getByRole('button', { name: 'Add Character', exact: true }).click();
      const dialog = page.getByRole('dialog');
      await expect(dialog.getByText('Add Character to Chat')).toBeVisible();

      await dialog.getByRole('button', { name: 'Summon from Lore' }).click();
      // The wizard titles itself by wizard STEP, not a fixed "Summon From Lore"
      // chrome title (a documented p4.9k4 simplification vs v4's separate
      // SummonFromLoreModal wrapper) — "Source Material" is step 1's label.
      await expect(page.getByRole('heading', { name: 'Source Material' })).toBeVisible();

      // Step 1 (Source Material): paste source text rather than upload a file
      // (the textarea carries no accessible name — matched by its placeholder).
      await page
        .getByPlaceholder(/Paste character descriptions/)
        .fill('A confectioner named Marchpane.');
      await page.getByRole('button', { name: 'Next', exact: true }).click();

      // Step 2 (Configuration): a connection profile is pre-selected from the
      // fixture; commence generation.
      await expect(page.getByRole('heading', { name: 'Configuration' })).toBeVisible();
      await page.getByRole('button', { name: 'Generate Character' }).click();

      // Step 3 (Generation) → step 4 (Review), then commit the import.
      await page.getByRole('button', { name: 'Review Results' }).click({ timeout: 30_000 });
      await expect(page.getByText('Marchpane')).toBeVisible({ timeout: 15_000 });
      await page.getByRole('button', { name: 'Import Character' }).click();

      // The wizard closes and hands the summoned character back, preselected.
      await expect(page.locator('qt-ai-import-wizard')).toHaveCount(0, { timeout: 15_000 });
      const addButton = dialog.getByRole('button', { name: 'Add Character', exact: true });
      await expect(addButton).toBeEnabled({ timeout: 15_000 });
      await addButton.click();

      await expect(dialog).toHaveCount(0, { timeout: 15_000 });
      await expect(castNames).toHaveCount(before + 1, { timeout: 15_000 });
      const joinerName = 'Marchpane';
      await expect(castNames.filter({ hasText: joinerName })).toHaveCount(1);

      // Verify through the persisted state: the chat GET carries the new participant.
      const chatIdMatch = page.url().match(/\/salon\/([0-9a-f-]{36})/i);
      if (chatIdMatch) {
        const resp = await page.request.post('/api/dispatch', {
          data: { type: 'chatGet', chatId: chatIdMatch[1] },
        });
        const body = (await resp.json()) as {
          data?: { chat?: { participants?: Array<{ character?: { name?: string } }> } };
        };
        const names = (body.data?.chat?.participants ?? []).map((p) => p.character?.name);
        expect(names).toContain(joinerName);
      }

      // Clean up: remove the summoned character so the fixture's roster is
      // restored for the whisper-gate assertion in salon-post-office-flow.
      await page.getByRole('button', { name: `Remove ${joinerName} from chat` }).click();
      const confirm = page.getByRole('dialog');
      await expect(confirm).toBeVisible();
      await confirm.getByRole('button', { name: 'Remove', exact: true }).click();
      await expect(page.getByRole('dialog')).toHaveCount(0, { timeout: 15_000 });
      await expect(castNames).toHaveCount(before, { timeout: 15_000 });
    } finally {
      await mockLlm.close();
      testInfo.annotations.push({
        type: 'p4.9k4',
        description: 'Summon-from-Lore cast beat; the mock reply shape is unverified against P4.9K2.',
      });
    }
  });
});
