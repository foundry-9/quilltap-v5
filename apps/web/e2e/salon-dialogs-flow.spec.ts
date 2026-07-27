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

/** The chat's project, straight off the chat read. */
async function readProjectId(page: Page, chatId: string): Promise<string | null> {
  const resp = await page.request.post('/api/dispatch', { data: { type: 'chatGet', chatId } });
  expect(resp.ok(), `chatGet → ${resp.status()}`).toBe(true);
  const body = (await resp.json()) as { data?: { chat?: { projectId?: string | null } } };
  return body.data?.chat?.projectId ?? null;
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

/**
 * ACTIVATE-AT-UNIFY. `GET /api/v1/chats/{id}?action=export` is P4.9E3B's byte
 * route; on this lane's branch it does not exist yet. The unifier flips this to
 * `true` when the branches meet.
 */
const CHAT_EXPORT_LANDED = false;

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

test.describe('P4.9E3C — Assign to Project', () => {
  test('unfiles and re-files a chat, and the entry is offered either way', async ({ page }) => {
    const chatId = await openChat(page, 'Solo Voyage');
    const original = await readProjectId(page, chatId);
    expect(original, 'the fixture chat must start in a project').toBeTruthy();

    await openSidebarSection(page, 'Chat');
    const entry = page.locator('qt-chat-section button.qt-tool-palette-button', {
      hasText: 'Project',
    });
    await expect(entry).toBeVisible({ timeout: 10_000 });
    await expect(entry).toContainText('Project:');
    await entry.click();

    const dialog = page.getByRole('dialog');
    await expect(dialog.getByText('Assign to Project')).toBeVisible({ timeout: 10_000 });
    const picker = dialog.locator('#chat-project');
    await expect(dialog.getByText('Currently in:')).toBeVisible();
    await expect(picker).toHaveValue(original!);

    // "No project" is a real choice, not an empty state: it sends an explicit
    // null and detaches.
    await picker.selectOption('');
    await dialog.getByRole('button', { name: 'Save' }).click();
    await expect(page.getByText('Chat removed from project')).toBeVisible({ timeout: 15_000 });
    expect(await readProjectId(page, chatId)).toBeNull();

    // The entry is UNCONDITIONAL — the REDUCED version it replaces was rendered
    // only for a chat that already HAD a project, so an unfiled chat could never
    // be filed from the Salon at all. This is the state that used to be a
    // dead end.
    await openSidebarSection(page, 'Chat');
    await expect(entry).toHaveText('Project');
    await expect(entry).toHaveAttribute('title', 'Assign to project');
    await entry.click();
    await expect(dialog.getByText('Assign to Project')).toBeVisible({ timeout: 10_000 });
    await expect(dialog.getByText('Currently in:')).toHaveCount(0);
    await expect(picker).toHaveValue('');

    await picker.selectOption(original!);
    await dialog.getByRole('button', { name: 'Save' }).click();
    await expect(page.getByText(/Chat moved to /)).toBeVisible({ timeout: 15_000 });
    expect(await readProjectId(page, chatId)).toBe(original);
  });
});

test.describe('P4.9E3C — Export', () => {
  // ACTIVATE-AT-UNIFY over P4.9E3B's byte route.
  test.skip(!CHAT_EXPORT_LANDED, 'GET /chats/{id}?action=export lands with P4.9E3B');

  test('the Organize entry points at the transcript download', async ({ page }) => {
    const chatId = await openChat(page, 'Solo Voyage');
    await openSidebarSection(page, 'Organize');
    await expect(page.getByRole('button', { name: 'Export' })).toBeVisible({ timeout: 10_000 });

    // v4 navigates the window straight at the route, so what the beat can assert
    // is the response that navigation would get: the media type, the attachment
    // disposition, and that the first line is a JSONL object.
    const resp = await page.request.get(`/api/v1/chats/${chatId}?action=export`);
    expect(resp.ok(), `export → ${resp.status()}`).toBe(true);
    expect(resp.headers()['content-type']).toContain('application/x-ndjson');
    expect(resp.headers()['content-disposition']).toContain('attachment');
    expect(resp.headers()['content-disposition']).toContain('.jsonl');
    const first = (await resp.text()).split('\n')[0];
    expect(() => JSON.parse(first) as unknown).not.toThrow();
  });
});

/**
 * The impersonation round trip — the state the Hand Off dialog hangs off.
 *
 * ⚠ **The dialog itself has no beat, and cannot have one against this fixture.**
 * It triggers only when the operator stops speaking for a character who has NO
 * connection profile (`useImpersonation.ts:76-89`), every participant in the
 * committed fixture has one, and no client verb can clear a profile
 * (`chatUpdateParticipant.connectionProfileId` is non-nullable in v4's schema).
 * Reaching it needs a seeded profile-less participant; the dialog is covered by
 * five component tests instead, and this is recorded in the lane record.
 */
test.describe('P4.9E3C — speaking as a character', () => {
  test('the cast card holds the impersonation across the refetch it triggers', async ({ page }) => {
    await openChat(page, 'Solo Voyage');
    await openSidebarSection(page, 'Participants');

    const speakAs = page.locator('qt-participant-card button[title^="Speak as "]').first();
    await expect(speakAs).toBeVisible({ timeout: 10_000 });
    const name = (await speakAs.getAttribute('title'))!.replace('Speak as ', '');
    await speakAs.click();

    // Neither app's chat GET projects `impersonatingParticipantIds`, so this
    // sticking at all is the assertion: v5 used to bind the card straight to the
    // chat record, and the dispatch's own refetch wiped it a moment later.
    const stop = page.locator(`qt-participant-card button[title="Stop speaking as ${name}"]`);
    await expect(stop).toBeVisible({ timeout: 15_000 });
    await expect(stop).toBeVisible({ timeout: 5_000 });

    await stop.click();
    await expect(
      page.locator(`qt-participant-card button[title="Speak as ${name}"]`),
    ).toBeVisible({ timeout: 15_000 });
  });
});
