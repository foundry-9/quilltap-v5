import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';
import { openSidebarSection } from './support/sidebar';

/**
 * P4.9E3C — the in-chat dialog family (v4 `app/salon/[id]/components/ChatModals.tsx`).
 *
 * ORDERING: rides the shared global-setup server, so the filename must sort
 * after `foundation.spec.ts` ('sa' > 'fo') and before the `zz…` destructives.
 *
 * ⚠ **Real-spend guard.** The e2e instance carries no API keys by design, so
 * every beat that could reach a model pins the NO-KEY arm (the avatar-preview
 * precedent): the failure is the assertion, and nothing is spent. In this file
 * that is Rename's "Use automatic naming" checkbox, whose ONLY behaviour is to
 * fire `regenerate-title`.
 */

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

async function openChat(page: Page, title: string): Promise<string> {
  await page.goto('/salon');
  await maybeUnlock(page);
  const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: title });
  await expect(card).toBeVisible({ timeout: 15_000 });
  await card.click();
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
  const id = new URL(page.url()).pathname.split('/').pop()!;
  expect(id).toBeTruthy();
  return id;
}

/** The stored title + manual-rename flag, straight off the chat read. */
async function readTitle(
  page: Page,
  chatId: string,
): Promise<{ title?: string; isManuallyRenamed?: boolean }> {
  const resp = await page.request.post('/api/dispatch', { data: { type: 'chatGet', chatId } });
  expect(resp.ok(), `chatGet → ${resp.status()}`).toBe(true);
  const body = (await resp.json()) as {
    data?: { chat?: { title?: string; isManuallyRenamed?: boolean } };
  };
  return { title: body.data?.chat?.title, isManuallyRenamed: body.data?.chat?.isManuallyRenamed };
}

test.describe('P4.9E3C — Rename Chat', () => {
  test('renames through the real chat update, and reverts the automatic-naming tick when the title cannot be generated', async ({
    page,
  }) => {
    const chatId = await openChat(page, 'Solo Voyage');
    const before = await readTitle(page, chatId);

    await openSidebarSection(page, 'Organize');
    await page.getByRole('button', { name: 'Rename' }).click();

    // The component host element is zero-sized (its card is fixed-position
    // inside it), so beats locate a dialog by its role, not by its selector.
    const dialog = page.getByRole('dialog');
    await expect(dialog.getByText('Rename Chat')).toBeVisible({ timeout: 10_000 });
    const field = dialog.locator('#chat-title');
    const auto = dialog.getByLabel('Use automatic naming');
    const save = dialog.getByRole('button', { name: 'Save' });

    // The fixture chat has never been renamed by hand, so v4 opens this dialog
    // with automatic naming ON — and in that state the field is DEAD and there
    // is no Save button at all (`ChatRenameModal.tsx:148,179`).
    await expect(auto).toBeChecked();
    await expect(field).toHaveValue(before.title!);
    await expect(field).toBeDisabled();
    await expect(save).toHaveCount(0);

    // Unticking fires nothing (:48) — it only wakes the form up.
    await auto.uncheck();
    await expect(field).toBeEnabled();
    await expect(save).toBeVisible();
    expect((await readTitle(page, chatId)).title).toBe(before.title);

    await field.fill('  Solo Voyage, Revisited  ');
    await save.click();
    await expect(page.getByText('Chat renamed')).toBeVisible({ timeout: 15_000 });
    // The trim is the assertion: v4 writes `title.trim()`, not what was typed,
    // and unticking the box is what sets `isManuallyRenamed`.
    expect(await readTitle(page, chatId)).toEqual({
      title: 'Solo Voyage, Revisited',
      isManuallyRenamed: true,
    });

    // The automatic-naming tick is the ONLY door to regenerate-title, in either
    // app. With no API key configured it must fail loudly and put itself back.
    await openSidebarSection(page, 'Organize');
    await page.getByRole('button', { name: 'Rename' }).click();
    await expect(dialog.getByText('Rename Chat')).toBeVisible({ timeout: 10_000 });
    await expect(auto).not.toBeChecked();
    await auto.check();

    await expect(dialog.locator('.qt-text-danger')).toBeVisible({ timeout: 15_000 });
    await expect(auto).not.toBeChecked();
    await expect(dialog.getByText('Rename Chat')).toBeVisible();
    // Nothing was renamed and nothing was spent.
    expect((await readTitle(page, chatId)).title).toBe('Solo Voyage, Revisited');

    // Put the title back so later beats find the card they expect. The
    // manual-rename FLAG stays set: no affordance in either app clears it
    // without regenerating, and Save is hidden while automatic naming is on.
    await field.fill(before.title!);
    await save.click();
    await expect(page.getByText('Chat renamed')).toBeVisible({ timeout: 15_000 });
    expect((await readTitle(page, chatId)).title).toBe(before.title);
  });
});
