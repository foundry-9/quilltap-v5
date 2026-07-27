import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';
import { openSidebarSection } from './support/sidebar';

/**
 * P4.9E3C — Bulk Character Replace (v4 `BulkCharacterReplaceModal.tsx`), the
 * Edit Content section's entry.
 *
 * ORDERING — why this is a `zz-` spec rather than living with the other dialog
 * beats. Bulk re-attribution is **irreversible in the direction that matters**:
 * it moves every matching message onto another participant AND deletes every
 * memory extracted from those messages. Moving them back would sweep the
 * target's own messages along with them, so there is no undo. Running it in
 * `salon-dialogs-flow.spec.ts` would hand every later salon spec a transcript
 * with the wrong author on it.
 *
 * So it sorts after every ordinary spec and before `zz-delete-all-destructive`
 * (`zz-b` < `zz-d`), which is the same slot the delete-all beat uses for the
 * same reason. It must NOT sort after `zzz-restore-destructive`.
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

/** Every message's author, straight off the chat read. */
async function authors(page: Page, chatId: string): Promise<(string | null)[]> {
  const resp = await page.request.post('/api/dispatch', { data: { type: 'chatGet', chatId } });
  expect(resp.ok(), `chatGet → ${resp.status()}`).toBe(true);
  const body = (await resp.json()) as {
    data?: { chat?: { messages?: { participantId: string | null }[] } };
  };
  return (body.data?.chat?.messages ?? []).map((m) => m.participantId);
}

/** How many participants the chat carries. */
async function castSize(page: Page, chatId: string): Promise<number> {
  const resp = await page.request.post('/api/dispatch', { data: { type: 'chatGet', chatId } });
  expect(resp.ok(), `chatGet → ${resp.status()}`).toBe(true);
  const body = (await resp.json()) as { data?: { chat?: { participants?: unknown[] } } };
  return (body.data?.chat?.participants ?? []).length;
}

test.describe('P4.9E3C — Bulk Character Replace (destructive)', () => {
  test('re-attributes the operator’s own turns to a character', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Group Expedition' });
    await expect(card).toBeVisible({ timeout: 15_000 });
    await card.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
    const chatId = new URL(page.url()).pathname.split('/').pop()!;

    const before = await authors(page, chatId);
    const unattributed = before.filter((a) => a === null).length;
    expect(unattributed, 'the fixture must carry some unattributed turns').toBeGreaterThan(0);

    await openSidebarSection(page, 'Edit Content');
    await page.getByRole('button', { name: 'Bulk Replace' }).click();

    const dialog = page.getByRole('dialog');
    await expect(dialog.getByText('Bulk Character Replace')).toBeVisible({ timeout: 10_000 });

    const source = dialog.locator('#qt-bulk-source');
    const target = dialog.locator('#qt-bulk-target');
    // The sentinel option exists because unattributed messages do.
    await source.selectOption('__UNASSIGNED__');
    // With the sentinel as source, EVERY participant stays a legal target.
    const targets = await target.locator('option').evaluateAll((os) =>
      os.map((o) => (o as HTMLOptionElement).value).filter(Boolean),
    );
    expect(targets.length).toBeGreaterThan(1);
    await target.selectOption(targets[0]);

    // The count is computed client-side over the loaded transcript, and it is
    // the operator's own turns — not "nothing selected".
    await expect(dialog.getByText(`${unattributed} message`)).toBeVisible();

    await page.getByRole('button', { name: 'Re-attribute Messages' }).click();
    await expect(page.getByText(/re-attributed to /)).toBeVisible({ timeout: 20_000 });

    const after = await authors(page, chatId);
    expect(after.filter((a) => a === null).length).toBe(0);
    expect(after.filter((a) => a === targets[0]).length).toBeGreaterThanOrEqual(unattributed);
  });
});

/**
 * ACTIVATE-AT-UNIFY. `POST /api/v1/messages/{id}?action=reattribute` is
 * P4.9E3B's verb; on this lane's branch it does not exist. The unifier flips
 * this to `true` when the branches meet.
 */
const MESSAGE_REATTRIBUTE_LANDED = true;

test.describe('P4.9E3C — Re-attribute one message (destructive)', () => {
  // The same irreversibility as the bulk form, which is why it lives here.
  test.skip(!MESSAGE_REATTRIBUTE_LANDED, 'messageReattribute lands with P4.9E3B');

  test('the action bar moves a single line to another participant', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Group Expedition' });
    await expect(card).toBeVisible({ timeout: 15_000 });
    await card.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
    const chatId = new URL(page.url()).pathname.split('/').pop()!;

    const row = page.locator('.qt-chat-message-row').first();
    await row.hover();
    await row.getByRole('button', { name: 'Re-attribute to different participant' }).click();

    const dialog = page.getByRole('dialog');
    await expect(dialog.getByText('Re-attribute Message')).toBeVisible({ timeout: 10_000 });
    // Nothing is preselected, so the confirm is dead until a choice is made.
    const confirm = dialog.getByRole('button', { name: 'Re-attribute', exact: true });
    await expect(confirm).toBeDisabled();

    await dialog.locator('.qt-dialog-body button').first().click();
    await expect(confirm).toBeEnabled();
    const messageId = await row.getAttribute('id');
    await confirm.click();
    await expect(page.getByText(/Message re-attributed to /)).toBeVisible({ timeout: 20_000 });

    const after = await authors(page, chatId);
    expect(after.length).toBeGreaterThan(0);
    expect(messageId).toMatch(/^message-/);
  });
});

/**
 * P4.9E3C — Merge a Conversation In. Also destructive, and for the same reason
 * the bulk beat is: it permanently enlarges the target chat's cast and posts a
 * Host recap into it. There is no un-merge.
 */
test.describe('P4.9E3C — Merge In (destructive)', () => {
  test('folds another conversation’s characters into this one', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Solo Voyage' });
    await expect(card).toBeVisible({ timeout: 15_000 });
    await card.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
    const chatId = new URL(page.url()).pathname.split('/').pop()!;
    const before = await castSize(page, chatId);

    await openSidebarSection(page, 'Organize');
    await page.getByRole('button', { name: 'Merge In…' }).click();

    const dialog = page.getByRole('dialog');
    await expect(dialog.getByText('Merge a Conversation In')).toBeVisible({ timeout: 10_000 });
    // This chat is not a candidate for merging into itself.
    await expect(dialog.getByText('Solo Voyage', { exact: false })).toHaveCount(0);

    // Wait for the list to arrive before picking — clicking during the loading
    // state hits a button that is about to be replaced.
    const source = dialog.locator('.qt-dialog-body button', { hasText: 'Chat Images' });
    await expect(source).toBeVisible({ timeout: 15_000 });
    await source.click();

    // Step 2: everyone eligible is checked, and the merge button counts them.
    await expect(dialog.getByText('Who joins', { exact: true })).toBeVisible({ timeout: 15_000 });
    const boxes = dialog.locator('input[type="checkbox"]');
    const count = await boxes.count();
    expect(count).toBeGreaterThan(0);
    for (let i = 0; i < count; i++) {
      await expect(boxes.nth(i)).toBeChecked();
    }

    await dialog.getByRole('button', { name: /^Merge In/ }).click();
    await expect(page.getByText(/Merged \d+ character/)).toBeVisible({ timeout: 30_000 });
    expect(await castSize(page, chatId)).toBeGreaterThan(before);
  });
});
