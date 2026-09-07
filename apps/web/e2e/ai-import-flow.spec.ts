import { expect, test, type Page } from '@playwright/test';

import { createServer, type Server } from 'node:http';

import { E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { openSidebarSection } from './support/sidebar';

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
 * ⚠ The mock LLM's ONE fixed reply CANNOT satisfy the real service (measured
 * by the `p4.9k` round's §3 review against `lib/services/ai-import.service.ts`):
 * the `character_basics` step stores the parsed object whole and then requires
 * TOP-LEVEL `name` (`:905`, `:919` — a nested `character` wrapper fails it),
 * while the `system_prompts` / `wardrobe_items` / `memories` steps `.map` over
 * ARRAYS (`:491`), so a single object reply throws inside assembly and the
 * run ends `done {error}` with no `result` and "Review Results" never renders.
 * So the mock below is prompt-KEYED and NON-streaming (the runner calls
 * `send_message`, never the stream): it reads the user message the import
 * composes (`{source}\n\n---\n\n{instruction}`) and answers each step with
 * the shape that step's parse and the assembly require — the `2f4254b42`
 * unification's discharge of this beat's owed recipe. Steps are told apart by
 * a phrase unique to each instruction (`ai_import.rs`'s prompt functions).
 */
const P49K2_SERVER_LANDED = true;

const BASICS_REPLY = {
  name: 'Pennyroyal',
  title: 'The Confectioner',
  identity: 'A travelling confectioner of some renown.',
  description: 'Sweet-tempered and precise.',
  manifesto: 'Every sweet is a small kindness.',
  personality: 'Endlessly patient, quietly ambitious.',
};

/** One reply per import step, keyed by a phrase unique to its instruction. */
const STEP_REPLIES: Array<[string, unknown]> = [
  ["Extract or generate the character's basic information", BASICS_REPLY],
  [
    'Generate a first message and example dialogues',
    { firstMessage: 'Good afternoon. Might I tempt you with a marzipan?', exampleDialogues: '' },
  ],
  [
    'Create system prompts that instruct an AI',
    [{ name: 'Default', content: 'You are Pennyroyal, a confectioner. You are patient and precise.', isDefault: true }],
  ],
  [
    'Generate physical descriptions of this character',
    {
      headAndShouldersPrompt: 'a confectioner with flour-dusted cheeks',
      shortPrompt: 'a confectioner with flour-dusted cheeks',
      mediumPrompt: 'a patient confectioner with flour-dusted cheeks and steady hands',
      longPrompt: 'a patient confectioner with flour-dusted cheeks, steady hands and a kind gaze',
      completePrompt: 'a patient confectioner with flour-dusted cheeks, steady hands, a kind gaze and neat dark hair',
      fullDescription: 'Pennyroyal has flour-dusted cheeks, steady hands, a kind gaze and neat dark hair.',
    },
  ],
  ['Ground every item in the source material', []],
  [
    "Determine the character's pronouns and aliases",
    { pronouns: { subject: 'she', object: 'her', possessive: 'her' }, aliases: ['March'] },
  ],
  [
    'Generate memories that this character would have',
    [{ content: 'Pennyroyal once won the county sugar-work prize.', summary: 'The sugar-work prize.', keywords: ['prize'], importance: 0.5 }],
  ],
  [
    'Generate an example chat conversation',
    { title: 'A first tasting', messages: [{ role: 'user', content: 'Hello.' }, { role: 'assistant', content: 'Do try the marzipan.' }] },
  ],
];

function replyFor(userText: string): string {
  const hit = STEP_REPLIES.find(([phrase]) => userText.includes(phrase));
  return JSON.stringify(hit ? hit[1] : BASICS_REPLY);
}

/**
 * A prompt-keyed, NON-streaming OPENAI-compatible chat-completions mock (the
 * wizard beat's non-streaming shape, plus a body read): the last user
 * message's text picks the reply.
 */
async function startPromptKeyedMockLlm(port: number): Promise<{ url: string; close: () => Promise<void> }> {
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
    const chunks: Buffer[] = [];
    req.on('data', (c: Buffer) => chunks.push(c));
    req.on('end', () => {
      let userText = '';
      try {
        const body = JSON.parse(Buffer.concat(chunks).toString('utf8')) as {
          messages?: Array<{ role?: string; content?: unknown }>;
        };
        const users = (body.messages ?? []).filter((m) => m.role === 'user');
        const last = users[users.length - 1]?.content;
        userText = typeof last === 'string' ? last : JSON.stringify(last ?? '');
      } catch {
        userText = '';
      }
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(
        JSON.stringify({
          id: 'mock-import-1',
          object: 'chat.completion',
          model: 'mock-model',
          choices: [{ index: 0, message: { role: 'assistant', content: replyFor(userText) }, finish_reason: 'stop' }],
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

    const mockLlm = await startPromptKeyedMockLlm(MOCK_LLM_PORT);
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
        .fill('A confectioner named Pennyroyal.');
      await page.getByRole('button', { name: 'Next', exact: true }).click();

      // Step 2 (Configuration): a connection profile is pre-selected from the
      // fixture; commence generation.
      await expect(page.getByRole('heading', { name: 'Configuration' })).toBeVisible();
      await page.getByRole('button', { name: 'Generate Character' }).click();

      // Step 3 (Generation) → step 4 (Review), then commit the import. The
      // wizard advances to Review on its own when the run ends (the first live
      // run showed step 4 already on screen); the button is clicked only when
      // the generation step is still showing it.
      const importButton = page.getByRole('button', { name: 'Import Character' });
      const reviewButton = page.getByRole('button', { name: 'Review Results' });
      await expect(importButton.or(reviewButton).first()).toBeVisible({ timeout: 30_000 });
      if (await reviewButton.isVisible()) {
        await reviewButton.click();
      }
      // Scoped to the wizard: the Salon's hidden chat cards also carry the name.
      await expect(page.locator('qt-ai-import-wizard').getByText('Pennyroyal').first()).toBeVisible({
        timeout: 15_000,
      });
      await importButton.click();

      // The wizard closes and hands the summoned character back, preselected.
      await expect(page.locator('qt-ai-import-wizard')).toHaveCount(0, { timeout: 15_000 });
      const addButton = dialog.getByRole('button', { name: 'Add Character', exact: true });
      await expect(addButton).toBeEnabled({ timeout: 15_000 });
      await addButton.click();

      await expect(dialog).toHaveCount(0, { timeout: 15_000 });
      await expect(castNames).toHaveCount(before + 1, { timeout: 15_000 });
      // Not `Marchpane`: the archive beat's seeded tombstone carries that name,
    // and the roster card filters are by text (the first full-suite run's catch).
    const joinerName = 'Pennyroyal';
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
      // The import lands the character as "Pennyroyal (imported)" (the
      // `duplicate` conflict strategy's suffix), so the card's control name
      // carries it too.
      await page
        .getByRole('button', { name: new RegExp(`^Remove ${joinerName}.* from chat$`) })
        .click();
      const confirm = page.getByRole('dialog');
      await expect(confirm).toBeVisible();
      await confirm.getByRole('button', { name: 'Remove', exact: true }).click();
      await expect(page.getByRole('dialog')).toHaveCount(0, { timeout: 15_000 });
      // v4's remove is a SOFT remove (`status: 'removed'`) and neither app
      // filters removed seats out of the card list — the `salon-cast-flow`
      // precedent asserts the toast, never a card count (the first live run's
      // catch: the count stayed at `before + 1`, correctly).
      await expect(
        page
          .locator('[role="toast-container"]')
          .getByText(/Pennyroyal.* has been removed from the chat/),
      ).toBeVisible({ timeout: 15_000 });
    } finally {
      await mockLlm.close();
      testInfo.annotations.push({
        type: 'p4.9k4',
        description: 'Summon-from-Lore cast beat over the prompt-keyed non-streaming mock (first live run at the 2f4254b42 unification).',
      });
    }
  });
});
