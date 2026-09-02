import { expect, request as pwRequest, test, type Page } from '@playwright/test';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * P4.D122 — the global search bar's **Documents** chip (v4 `b220999d`).
 *
 * The walk the feature exists for: seed a document into a real store, open the
 * search dialog, narrow to the Documents chip, and check the card v4 renders —
 * the store-name `·` path subtitle, the Document badge, and the STANDALONE deep
 * link as the href (the safe default that notifies no chat). Then click it and
 * land in standalone Document Mode.
 *
 * The href assertion is the one that matters most: `/workspace?open=document-
 * standalone&…` is what a middle-click follows, and it is why the server hands
 * out a chat-less URL rather than a chat one.
 *
 * Like `workspace-flow.spec` and `workspace-document-standalone-flow.spec`,
 * this imports the BASE `@playwright/test` (NOT `./support/fixtures`) so the
 * workspace-tabs flag stays ON and the click exercises the richest arm — the
 * in-place `document-standalone` tab — rather than the legacy navigation.
 *
 * ORDERING: rides the shared global-setup server; the `workspace-` prefix sorts
 * it in with the other flag-ON specs (workers: 1, alphabetical), after every
 * route-mode spec. Nothing here locks the server.
 */

/** A search token no other fixture row can collide with. */
const DOC_TITLE = 'p4d122-search-target';

let storeName: string | undefined;
/** The path the server minted for the seeded document. */
let docPath: string | undefined;
let docName: string | undefined;
/** The store reference the server should address the document by. */
let expectedRef: string | undefined;

async function dispatch(body: Record<string, unknown>): Promise<Record<string, unknown>> {
  const ctx = await pwRequest.newContext();
  const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: body });
  const parsed = (await res.json().catch(() => null)) as {
    type?: string;
    data?: Record<string, unknown>;
    error?: string;
  } | null;
  await ctx.dispose();
  if (!parsed || parsed.type === 'error') {
    throw new Error(`dispatch ${String(body['type'])} failed: ${JSON.stringify(parsed)}`);
  }
  return parsed.data ?? {};
}

async function ensureUnlocked(): Promise<void> {
  const ctx = await pwRequest.newContext();
  await ctx
    .post(`${BASE_URL}/api/dispatch`, { data: { type: 'unlock', passphrase: E2E_PASSPHRASE } })
    .catch(() => undefined);
  await ctx.dispose();
}

async function openWorkspace(page: Page): Promise<void> {
  await page.goto('/');
  const passphrase = page.locator('#qt-passphrase');
  const workspace = page.locator('.qt-workspace');
  await expect(passphrase.or(workspace).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
  await expect(workspace).toBeVisible({ timeout: 15_000 });
}

/** Type the token into the toolbar bar and open the full dialog. */
async function openDialog(page: Page) {
  const bar = page.getByPlaceholder('Search... (⌘K)');
  await expect(bar).toBeVisible({ timeout: 15_000 });
  await bar.fill(DOC_TITLE);
  await page.getByText('See all results →').click();
  // `qt-search-dialog` is an Angular custom-element HOST — `display: inline`
  // with no box of its own, so it never reports "visible". Scope through it and
  // assert on the dialog's own input instead.
  const dialog = page.locator('qt-search-dialog');
  await expect(
    dialog.getByPlaceholder('Search chats, characters, messages, documents, tags, memories...'),
  ).toBeVisible({ timeout: 15_000 });
  return dialog;
}

test.beforeAll(async () => {
  await ensureUnlocked();
  const mounts = (await dispatch({ type: 'mountPointList' }))['mountPoints'] as Array<{
    id: string;
    name: string;
    mountType?: string;
    storeType?: string;
    enabled?: boolean;
  }>;
  // A database-backed, enabled, ORDINARY store — a character vault would work
  // too (the chip searches them), but its name moves with its character.
  const store = (mounts ?? []).find(
    (m) => m.enabled !== false && m.mountType === 'database' && m.storeType !== 'character',
  );
  if (!store) throw new Error('fixture carries no enabled database-backed document store');
  storeName = store.name;
  const enabled = (mounts ?? []).filter((m) => m.enabled !== false);
  const key = (n: string) => n.trim().toLowerCase();
  const sameName = enabled.filter((m) => key(m.name) === key(store.name)).length;
  const reserved = ['self', 'project', 'general'].includes(key(store.name));
  expectedRef = sameName > 1 || reserved ? store.id : store.name;
  // eslint-disable-next-line no-console
  console.log(
    `[p4d122] store ${JSON.stringify(store.name)} → ref ${JSON.stringify(expectedRef)} ` +
      `(sameName=${sameName}, reserved=${reserved})`,
  );

  // `documentOpen` with a TITLE and no filePath mints a new blank — the same
  // call the standalone picker's "New document here" makes. (With a filePath it
  // OPENS an existing file and 404s otherwise, which is v4's contract.)
  const opened = (await dispatch({
    type: 'documentOpen',
    scope: 'document_store',
    mountPoint: store.id,
  }))['document'] as { filePath: string; displayTitle: string };
  // The blank always lands as "Untitled Document.md" (the `title` field names a
  // CHAT document, not a standalone one), so rename it to the search token.
  // Idempotent: Playwright restarts the worker after a failure, so `beforeAll`
  // can run twice against the same (shared, already-seeded) server.
  let renamedPath: string;
  try {
    renamedPath = (
      (await dispatch({
        type: 'documentRename',
        scope: 'document_store',
        mountPoint: store.id,
        filePath: opened.filePath,
        newTitle: DOC_TITLE,
      }))['document'] as { filePath: string }
    ).filePath;
  } catch (err) {
    if (!String(err).includes('already exists')) throw err;
    renamedPath = `${DOC_TITLE}.md`;
  }
  docPath = renamedPath;
  docName = docPath.split('/').pop();
  if (!docName?.includes(DOC_TITLE)) {
    throw new Error(`seeded document is not searchable by its token: ${docPath}`);
  }
});

test('the Documents chip finds a stored document and its card carries the standalone deep link', async ({
  page,
}) => {
  await openWorkspace(page);
  const dialog = await openDialog(page);
  // The chip row reads the shared ALL_SEARCH_TYPES — six chips, this order.
  await expect(
    dialog.locator('button.qt-filter-chip, button.qt-filter-chip-active'),
  ).toHaveText(['Chats', 'Characters', 'Messages', 'Documents', 'Tags', 'Memories']);

  // Narrow to Documents: click every OTHER chip off (at least one must stay
  // selected, which is v4's rule and v5's).
  for (const label of ['Chats', 'Characters', 'Messages', 'Tags', 'Memories']) {
    await dialog.getByRole('button', { name: label, exact: true }).click();
  }
  await expect(
    dialog.getByRole('button', { name: 'Documents', exact: true }),
  ).toHaveClass(/qt-filter-chip-active/);

  // The group header, then the card.
  await expect(dialog.getByText('📄 Documents', { exact: false })).toBeVisible();
  const card = dialog.locator(`a[href*="${encodeURIComponent(docPath!)}"]`).first();
  await expect(card).toBeVisible();
  // The href is the STANDALONE deep link — never a chat one.
  const href = await card.getAttribute('href');
  expect(href).toContain('/workspace?open=document-standalone');
  expect(href).toContain('scope=document_store');
  // The store REFERENCE is the name, or the UUID when the name is ambiguous or
  // reserved (`docStoreAuthority`). Which arm this fixture takes is a property
  // of its store list, so the expectation is derived from that list rather than
  // assumed — and the beat reports which arm fired.
  expect(href).toContain(`mountPoint=${encodeURIComponent(expectedRef!)}`);
  // v4's card furniture: the Document badge and the `store · path` subtitle.
  await expect(card).toContainText('Document');
  await expect(card).toContainText(`${storeName} · ${docPath}`);
});

test('clicking the card opens a standalone tab — no chat is told', async ({ page }) => {
  await openWorkspace(page);
  const dialog = await openDialog(page);
  const card = dialog.locator(`a[href*="${encodeURIComponent(docPath!)}"]`).first();
  await expect(card).toBeVisible({ timeout: 15_000 });

  await card.click();
  // No Salon is focused (the workspace opens on Home), so the click takes the
  // SILENT arm: a `document-standalone` tab in place, no `chat_documents` row,
  // no Librarian announcement.
  await expect(
    page.locator('.qt-tab-strip .qt-tab-label', { hasText: docName! }),
  ).toBeVisible({ timeout: 15_000 });
  // The dialog closed behind it (the card emitted `resultClick`).
  await expect(
    page.locator('qt-search-dialog').getByPlaceholder(
      'Search chats, characters, messages, documents, tags, memories...',
    ),
  ).toHaveCount(0);
});

test('with a Salon focused the card opens IN the chat — the arm dogfood #105 broke', async ({
  page,
}) => {
  // The two beats above both run with Home focused, so both take the SILENT
  // standalone arm. The in-chat arm was never gestured — and it was broken:
  // `OpenDocumentFromSearch` is `providedIn: 'root'`, so its injector could not
  // see the Salon's component-provided `DocumentApi` and every in-chat open
  // threw NG0201 and did nothing at all (dogfood finding #105).
  await openWorkspace(page);

  // Get a real conversation focused, the way a reader would. Picked BY TITLE,
  // never positionally (P4.69): this used to take `.first()` and call it
  // `soloCard`, which is only 'Solo Voyage' while nothing newer exists — a
  // sibling spec seeding a chat in a full run silently moves it, the exact
  // shape that broke the marks beat at the `f3892158d` unification. Any of the
  // fixture's chats would serve here, so the beat asserts the one it names.
  await page.locator('.qt-nav-rail a[href="/salon"], a[href="/salon"]').first().click();
  const soloCard = page
    .locator('.chat-card-stack a.qt-entity-card')
    .filter({ has: page.getByText('Solo Voyage', { exact: true }) });
  await expect(soloCard).toBeVisible({ timeout: 15_000 });
  await soloCard.click();
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });

  const failures: string[] = [];
  page.on('pageerror', (err) => failures.push(String(err)));
  page.on('console', (msg) => {
    if (msg.type() === 'error' && /NG0201/.test(msg.text())) failures.push(msg.text());
  });

  const dialog = await openDialog(page);
  const card = dialog.locator(`a[href*="${encodeURIComponent(docPath!)}"]`).first();
  await expect(card).toBeVisible({ timeout: 15_000 });
  await card.click();

  // The in-chat open splits the document in beside the conversation — the
  // chat is still there, and the document pane came up with it.
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
  await expect(
    page.locator('qt-document-pane, .qt-document-pane, .qt-document-mode').first(),
  ).toBeVisible({ timeout: 15_000 });
  // And nothing threw on the way.
  expect(failures, `errors during the in-chat open: ${failures.join(' | ')}`).toEqual([]);
});
