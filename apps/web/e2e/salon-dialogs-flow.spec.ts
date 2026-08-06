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
 * The success toast the rename raises — narrowed to the SUCCESS type, the way
 * the 2026-07-31 deflake narrowed the error probe.
 *
 * P4.40: `getByText('Chat renamed')` on its own is a strict-mode hazard. Toasts
 * dismiss on a `setTimeout` running on the page's main thread, so under load
 * that timer starves and an earlier success toast is STILL on screen when the
 * next one arrives — two matches, and the assertion fails with "resolved to 2
 * elements" (reproduced deterministically at 20x CPU throttling; the full-suite
 * failure the `f7f1a956` gate saw). `.first()` would paper over it and, worse,
 * would be satisfied by the STALE toast even if the second rename never
 * happened, so the beat pairs the probe with {@link chatUpdateCommitting} — the
 * round trip is the proof, the toast is the UI's report of it.
 */
const renamedToast = (page: Page) =>
  page.locator('[role="toast-container"] div.qt-toast-success', { hasText: 'Chat renamed' });

/**
 * The `chatUpdate` dispatch that commits `title` — matched on the RESPONSE BODY
 * so the waiter is immune to which invalidation produced it and to earlier
 * in-flight responses (the 2026-07-31 trap-10 shape). Register it BEFORE the
 * click that triggers it.
 */
const chatUpdateCommitting = (page: Page, title: string) =>
  page.waitForResponse(
    async (r) => {
      if (!r.url().includes('/api/dispatch')) return false;
      if (!(r.request().postData() ?? '').includes('"chatUpdate"')) return false;
      const text = await r.text().catch(() => '');
      return text.includes(`"title":${JSON.stringify(title)}`);
    },
    { timeout: 15_000 },
  );

/**
 * ACTIVATE-AT-UNIFY. `GET /api/v1/chats/{id}?action=export` is P4.9E3B's byte
 * route; on this lane's branch it does not exist yet. FLIPPED at the round's unification
 * (2026-07-27).
 */
const CHAT_EXPORT_LANDED = true;

/**
 * ACTIVATE-AT-UNIFY. `GET /api/v1/tools` is P4.9E3B's inventory; without it the
 * tool tree has nothing to list. FLIPPED at the round's unification (2026-07-27).
 */
const TOOLS_INVENTORY_LANDED = true;

/**
 * ACTIVATE-AT-UNIFY. `POST /api/v1/search-replace?action=preview|execute` is
 * P4.9E3B's pair. FLIPPED at the round's unification (2026-07-27).
 */
const SEARCH_REPLACE_LANDED = true;

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
    // The parent reacts to `renamed` by REFETCHING the chat (a page-side
    // chatGet), and the dialog reopened below reads `isManuallyRenamed` off
    // that refetched record. Await the refetch response that carries the flag
    // before reopening: under full-suite load a slow round trip reopens the
    // dialog on the STALE record, the box renders ticked, and the not-checked
    // assertion below loses the race (the 2026-07-31 full-suite flake,
    // reproduced deterministically with a 6 s chatGet delay).
    const refetched = page.waitForResponse(
      async (r) => {
        if (!r.url().includes('/api/dispatch')) return false;
        if (!(r.request().postData() ?? '').includes('"chatGet"')) return false;
        const text = await r.text().catch(() => '');
        return text.includes('"isManuallyRenamed":true');
      },
      { timeout: 15_000 },
    );
    const committed = chatUpdateCommitting(page, 'Solo Voyage, Revisited');
    await save.click();
    await committed;
    await expect(renamedToast(page).first()).toBeVisible({ timeout: 15_000 });
    // The trim is the assertion: v4 writes `title.trim()`, not what was typed,
    // and unticking the box is what sets `isManuallyRenamed`.
    expect(await readTitle(page, chatId)).toEqual({
      title: 'Solo Voyage, Revisited',
      isManuallyRenamed: true,
    });
    await refetched;

    // The automatic-naming tick is the ONLY door to regenerate-title, in either
    // app. With no API key configured it must fail loudly and put itself back.
    await openSidebarSection(page, 'Organize');
    await page.getByRole('button', { name: 'Rename' }).click();
    await expect(dialog.getByText('Rename Chat')).toBeVisible({ timeout: 10_000 });
    await expect(auto).not.toBeChecked();
    // `click`, not `check`: the failed regenerate REVERTS the box, and check()'s
    // own post-click verification can lose to a fast revert. The response
    // waiter is what proves the tick registered and the round trip completed.
    const regenerated = page.waitForResponse(
      (r) =>
        r.url().includes('/api/dispatch') &&
        (r.request().postData() ?? '').includes('chatRegenerateTitle'),
      { timeout: 15_000 },
    );
    await auto.click();
    await regenerated;

    // v4 reports this failure as a toast and nothing else (P4.25) — an ERROR
    // toast, specifically: the success "Chat renamed" toast above lives 3 s and
    // can still be on screen here, so an any-toast probe would match it early
    // and start the revert assertion's clock before the regenerate has even
    // answered (the flake's second arm, same repro).
    await expect(page.locator('[role="toast-container"] > div.qt-toast-error').first()).toBeVisible(
      { timeout: 15_000 },
    );
    await expect(auto).not.toBeChecked();
    await expect(dialog.getByText('Rename Chat')).toBeVisible();
    // Nothing was renamed and nothing was spent.
    expect((await readTitle(page, chatId)).title).toBe('Solo Voyage, Revisited');

    // Put the title back so later beats find the card they expect. The
    // manual-rename FLAG stays set: no affordance in either app clears it
    // without regenerating, and Save is hidden while automatic naming is on.
    await field.fill(before.title!);
    const restored = chatUpdateCommitting(page, before.title!);
    await save.click();
    await restored;
    await expect(renamedToast(page).first()).toBeVisible({ timeout: 15_000 });
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
    // `exact` matters now that "Export Markdown" sits beside it — the default
    // name match is a substring one and would resolve both buttons.
    await expect(page.getByRole('button', { name: 'Export', exact: true })).toBeVisible({
      timeout: 10_000,
    });

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

  test('the Markdown transcript entry serves a readable document', async ({ page }) => {
    const chatId = await openChat(page, 'Solo Voyage');
    await openSidebarSection(page, 'Organize');
    const entry = page.getByRole('button', { name: 'Export Markdown' });
    await expect(entry).toBeVisible({ timeout: 10_000 });
    await expect(entry).toHaveAttribute(
      'title',
      'Export the conversation as a readable Markdown transcript',
    );

    // The entry anchor-clicks the route, so what the beat asserts is the
    // response that click fetches: the media type, the attachment name built
    // from the chat's title, `no-store` (which no other arm of this fan-out
    // sends), and the document's own shape.
    const resp = await page.request.get(`/api/v1/chats/${chatId}?action=export-markdown`);
    expect(resp.ok(), `export-markdown → ${resp.status()}`).toBe(true);
    expect(resp.headers()['content-type']).toContain('text/markdown');
    expect(resp.headers()['content-disposition']).toContain('attachment');
    // The server names the file after the chat, so this ties the download name
    // to the conversation the beat actually opened.
    expect(resp.headers()['content-disposition']).toContain('Solo Voyage_transcript.md');
    expect(resp.headers()['cache-control']).toBe('no-store');

    const body = await resp.text();
    expect(body.split('\n')[0]).toBe('# Solo Voyage');
    // Every message heading is `## Speaker — timestamp` with a real EM DASH.
    expect(body).toMatch(/^## .+ \u2014 .+$/m);
    // Exactly one trailing newline.
    expect(body.endsWith('\n')).toBe(true);
    expect(body.endsWith('\n\n')).toBe(false);
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
    await expect(page.locator(`qt-participant-card button[title="Speak as ${name}"]`)).toBeVisible({
      timeout: 15_000,
    });
  });
});

test.describe('P4.9E3C — LLM Tool Settings', () => {
  // ACTIVATE-AT-UNIFY over P4.9E3B's tool inventory.
  test.skip(!TOOLS_INVENTORY_LANDED, 'GET /api/v1/tools lands with P4.9E3B');

  test('the Tools… entry opens the tree and writes both disable sets', async ({ page }) => {
    const chatId = await openChat(page, 'Solo Voyage');
    await openSidebarSection(page, 'Chat');
    await page.getByRole('button', { name: 'Tools…' }).click();

    const dialog = page.getByRole('dialog');
    await expect(dialog.getByText('LLM Tool Settings')).toBeVisible({ timeout: 15_000 });
    // The entry used to answer with a refusal naming the unported inventory.
    await expect(dialog.getByText('has not been ported')).toHaveCount(0);

    const save = dialog.getByRole('button', { name: 'Save' });
    await expect(save).toBeDisabled();

    // Toggling the built-in group off writes every member id, not a pattern.
    await dialog.getByRole('checkbox', { name: 'Toggle all Built-in Tools' }).click();
    await expect(save).toBeEnabled();
    await save.click();
    await expect(page.getByText('Tool settings saved')).toBeVisible({ timeout: 15_000 });

    const resp = await page.request.post('/api/dispatch', { data: { type: 'chatGet', chatId } });
    const body = (await resp.json()) as {
      data?: { chat?: { disabledTools?: string[]; disabledToolGroups?: string[] } };
    };
    expect((body.data?.chat?.disabledTools ?? []).length).toBeGreaterThan(0);
    expect(body.data?.chat?.disabledToolGroups ?? []).toEqual([]);

    // Put it back, so no later beat runs against a chat with its tools off.
    await openSidebarSection(page, 'Chat');
    await page.getByRole('button', { name: 'Tools…' }).click();
    await expect(dialog.getByText('LLM Tool Settings')).toBeVisible({ timeout: 15_000 });
    await dialog.getByRole('button', { name: 'Enable All' }).click();
    await dialog.getByRole('button', { name: 'Save' }).click();
    await expect(page.getByText('Tool settings saved')).toBeVisible({ timeout: 15_000 });
  });
});

test.describe('P4.9E3C — Run Tool', () => {
  // ACTIVATE-AT-UNIFY over P4.9E3B's tool inventory.
  test.skip(!TOOLS_INVENTORY_LANDED, 'GET /api/v1/tools lands with P4.9E3B');

  test('the picker lists the invocable tools and the form gates the run', async ({ page }) => {
    await openChat(page, 'Solo Voyage');
    await openSidebarSection(page, 'Chat');
    await page.getByRole('button', { name: 'Run Tool…' }).click();

    const dialog = page.getByRole('dialog');
    await expect(dialog.getByText('Run Tool', { exact: true })).toBeVisible({ timeout: 15_000 });
    await expect(dialog.getByLabel('Search tools')).toBeVisible();

    // Narrow to one tool, then step into its form.
    await dialog.getByLabel('Search tools').fill('memor');
    const first = dialog.locator('.qt-dialog-body button').first();
    await expect(first).toBeVisible({ timeout: 10_000 });
    const toolName = (await first.locator('.font-medium').first().innerText()).trim();
    await first.click();

    await expect(dialog.getByText(`Run Tool: ${toolName}`)).toBeVisible({ timeout: 10_000 });
    // The preview labels itself off the form's validity, and Back returns.
    await expect(dialog.getByText(/Arguments preview \((valid|incomplete)\)/)).toBeVisible();
    await dialog.getByRole('button', { name: 'Back' }).click();
    await expect(dialog.getByLabel('Search tools')).toBeVisible();
  });
});

test.describe('P4.9E3C — Search & Replace', () => {
  // ACTIVATE-AT-UNIFY over P4.9E3B's preview/execute pair.
  test.skip(!SEARCH_REPLACE_LANDED, 'search-replace lands with P4.9E3B');

  test('previews a search in this chat and refuses to go on without matches', async ({ page }) => {
    await openChat(page, 'Solo Voyage');
    await openSidebarSection(page, 'Edit Content');
    // `exact` matters: the Edit Content drawer also carries "Bulk Replace", and an
    // unanchored name matches both.
    await page.getByRole('button', { name: 'Replace', exact: true }).click();

    const dialog = page.getByRole('dialog');
    // v4 opens on step 2 when a scope was supplied, and the Salon always
    // supplies this chat.
    await expect(dialog.locator('#qt-sr-find')).toBeVisible({ timeout: 15_000 });
    const next = dialog.getByRole('button', { name: 'Next' });
    await expect(next).toBeDisabled();

    // A search with no matches is a real preview and still cannot proceed.
    await dialog.locator('#qt-sr-find').fill('zzz-not-in-this-transcript');
    await expect(dialog.getByText('No matches found')).toBeVisible({ timeout: 15_000 });
    await expect(next).toBeDisabled();

    // Back reaches the scope step with the chat preselected.
    await dialog.getByRole('button', { name: 'Back' }).click();
    await expect(dialog.getByText('Select Scope')).toBeVisible();
    await expect(dialog.getByLabel('Current Chat')).toBeChecked();

    // Nothing was written: this beat deliberately never executes.
    await dialog.getByRole('button', { name: 'Cancel' }).click();
    await expect(dialog.getByText('Select Scope')).toHaveCount(0);
  });
});
