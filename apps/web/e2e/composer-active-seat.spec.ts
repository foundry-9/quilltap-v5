import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';
import { openSidebarSection } from './support/sidebar';

/**
 * P4.81 item 8 — the composer's `[hasActiveCharacters]` input. v4's composer
 * gate is `useParticipants.hasActiveCharacters` (`useParticipants.ts:70-72` —
 * `type === 'CHARACTER' && isActive`, NO `controlledBy` filter,
 * `SalonView.tsx:1539`). `SalonConversation` used to bind its OWN narrower
 * `hasActiveCharacters` (`controlledBy === 'llm'` — the right predicate for
 * `onSidebarSkip` alone) to that same input, so a chat whose only remaining
 * ACTIVE character is user-driven left the composer reading v4's disabled
 * placeholder where v4 itself lets the operator type.
 *
 * No committed fixture chat has this shape (every seeded chat's active roster
 * includes at least one LLM-controlled character), so the beat builds one live:
 * New Chat with two characters — the roster favorite (LLM, satisfies the
 * create form's `llmSelected().length > 0` guard) plus a second character
 * flipped to "Play As (User)" — then sets the LLM seat's Participation status
 * to "Absent" (`isActive: false`; `chat-sidebar.ts`'s `activeCharacterCount`
 * excludes `controlledBy === 'user'` seats, so the sidebar's own `canRemove`
 * gate refuses to Remove the LAST LLM character — v4's last-character guard,
 * correctly keyed on LLM seats alone; the status select carries no such
 * guard), leaving exactly one ACTIVE character, and it is user-driven.
 */
test.describe('P4.81 — the composer stays enabled when the only active character is user-driven', () => {
  async function maybeUnlock(page: Page): Promise<void> {
    const passphrase = page.locator('#qt-passphrase');
    const chats = page.getByRole('heading', { name: 'Chats', exact: true });
    await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
  }

  test('a single user-controlled active seat enables the composer, not v4\'s "add a character" gate', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);

    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
    await page.getByRole('link', { name: 'New Chat' }).first().click();
    await expect(page.getByRole('heading', { name: 'New Chat', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Select Characters' })).toBeVisible();

    const roster = page.locator('.new-chat-character-picker button');
    // The LLM seat — its connection profile auto-seeds ("Speaks First"), which
    // is what the create form's `llmSelected().length > 0` guard needs; it is
    // removed from the chat below, so this is the only thing its name is for.
    const llmName = (await roster.nth(0).locator('.qt-text-primary').first().textContent())?.trim();
    await roster.nth(0).click();
    await expect(page.getByText('Speaks First')).toBeVisible();

    // The seat the beat is actually about: a second, DIFFERENT character,
    // played by the human directly (v4's `USER_CONTROLLED_PROFILE` sentinel —
    // no connection profile, `controlledBy: 'user'` at creation). The roster
    // keeps every row visible (selection only restyles it), so index 1 is a
    // second, distinct character.
    const userName = (await roster.nth(1).locator('.qt-text-primary').first().textContent())?.trim();
    await roster.nth(1).click();
    await expect(page.getByRole('heading', { name: 'Selected Characters (2)' })).toBeVisible();
    expect(userName).toBeTruthy();
    // Scoped to THIS character's own selected-cast card (not by select
    // INDEX, which a system-prompt select on the other card would shift):
    // the connection-profile select is the first `<select>` in the card.
    const userCard = page
      .locator('.new-chat-character-picker .space-y-4 > div')
      .filter({ hasText: userName! });
    await userCard.locator('select').first().selectOption({ label: 'Play As (User)' });

    const create = page.getByRole('button', { name: 'Create Chat' });
    await expect(create).toBeEnabled();
    await create.click();
    await expect(page).toHaveURL(/\/salon\/[0-9a-f-]{16,}/, { timeout: 20_000 });
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });

    // Mark the LLM seat Absent — `isActive: false`, leaving the
    // user-controlled seat as the chat's only ACTIVE character.
    await openSidebarSection(page, 'Participants');
    expect(llmName).toBeTruthy();
    const llmCard = page.locator('qt-participant-card').filter({ hasText: llmName! });
    await expect(llmCard).toBeVisible({ timeout: 10_000 });
    const statusSelect = page.getByRole('combobox', {
      name: `Participation status for ${llmName}`,
    });
    await expect(statusSelect).toBeVisible();
    await statusSelect.selectOption('absent');
    await expect(statusSelect).toHaveValue('absent');

    // v4 enables the composer here (`hasActiveCharacters` is the wide
    // predicate); the placeholder must NOT read the "no active character"
    // sentence, and the Send button must not carry that title either.
    const editor = page.locator('.qt-chat-composer-input .qt-rich-editor-content');
    await expect(editor).toBeVisible();
    const placeholder = editor.locator('.qt-rich-editor-placeholder');
    await expect(placeholder).not.toHaveAttribute(
      'data-placeholder',
      'Add a character to start chatting…',
    );

    await editor.click();
    await page.keyboard.type('Just us, then.');
    const send = page.getByRole('button', { name: 'Send message' });
    await expect(send).toBeVisible();
    await expect(send).toBeEnabled();
    await expect(send).toHaveAttribute('title', 'Send message');
  });
});
