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

/**
 * Close every document the named chat is carrying, so the Salon has no child
 * `document` tab to reconcile (and focus) when it mounts. Idempotent and
 * chat-scoped; uses the same verbs the composer's picker does.
 */
/** The fixture chat's id, by title. */
async function chatIdByTitle(chatTitle: string): Promise<string> {
  // `listChats` answers `{ type: 'chats', data: [...] }`, so the dispatch helper's
  // `parsed.data` IS the array — not an object with a `chats` key (the trap the
  // `f3892158d` round's seeding hit).
  const chats = (await dispatch({ type: 'listChats' })) as unknown as Array<{
    id: string;
    title?: string | null;
  }>;
  const chat = (Array.isArray(chats) ? chats : []).find((c) => c.title === chatTitle);
  if (!chat) throw new Error(`fixture carries no chat titled ${JSON.stringify(chatTitle)}`);
  return chat.id;
}

/** The `chat_documents` rows a chat is carrying — the in-chat arm's own trace. */
async function openDocumentPaths(chatId: string): Promise<string[]> {
  const open = (await dispatch({ type: 'chatOpenDocuments', chatId }))['documents'] as
    | Array<{ id: string; filePath?: string; document?: { filePath?: string } }>
    | undefined;
  return (open ?? []).map((d) => d.filePath ?? d.document?.filePath ?? '');
}

/** Close every document the chat is carrying. Idempotent; returns how many. */
async function closeOpenDocuments(chatId: string): Promise<number> {
  const open = (await dispatch({ type: 'chatOpenDocuments', chatId }))['documents'] as
    | Array<{ id: string }>
    | undefined;
  for (const doc of open ?? []) {
    await dispatch({ type: 'chatDocumentClose', chatId, chatDocumentId: doc.id });
  }
  return (open ?? []).length;
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
  // ⚠ THE PRECONDITION, established rather than assumed (P4.75).
  //
  // `resolveActiveSalon` follows the focused pane's ACTIVE TAB — not a merely
  // visible conversation (v4 `use-open-document-from-search.ts:52-64`, which v5
  // transcribes verbatim). Those two come apart whenever the chat already has an
  // open document: the Salon reconciles a `document` CHILD tab for it and
  // FOCUSES it (v4 `SalonModePanes.tsx:110-118` passes no `focus`, and v4's
  // reducer defaults `focus = action.focus ?? true`, `workspace-reducer.ts:259`
  // — v5 is byte-faithful at both), and the child renders BESIDE its parent, so
  // the conversation stays on screen while the active tab is the document. The
  // card click then takes the standalone arm, correctly, and the assertion below
  // reads a backgrounded Salon.
  //
  // That reconcile lands asynchronously, after the chat's open-document set
  // loads — later than the message list appears — so no amount of waiting on the
  // Salon makes it settle. And this fixture chat is SHARED: `salon-documents-
  // flow` opens, edits and closes a document in Solo Voyage, so whether one is
  // left open here belongs to whatever ran before. That coupling is the standing
  // `workspace-search-documents` intermittent, root-caused by P4.75 — the beat,
  // not the product.
  //
  // So close whatever this chat is carrying first. Nothing about the subject is
  // weakened: the beat is about what a Documents result does with a conversation
  // in front of the reader, and this is exactly that starting point.
  const chatId = await chatIdByTitle('Solo Voyage');
  const closed = await closeOpenDocuments(chatId);
  // eslint-disable-next-line no-console
  console.log(`[p4d122] closed ${closed} open document(s) on "Solo Voyage"`);
  expect(await openDocumentPaths(chatId)).toEqual([]);

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

  // ⚠ WHAT DISTINGUISHES THE TWO ARMS (P4.75 — this assertion used to be a
  // race, and the race is why the beat was intermittent).
  //
  // The in-chat arm creates a `chat_documents` row and posts the Librarian's
  // announcement; the silent standalone arm touches no chat at all. That row is
  // therefore the ONE unambiguous witness, it lives on the server, and it cannot
  // be won or lost by a rendering race — which is why the precondition above
  // asserts the chat starts with none.
  //
  // What this beat used to assert instead was `.qt-chat-messages-list` still
  // VISIBLE, on the reasoning that an in-chat open "splits the document in
  // beside the conversation". Measured: it does not. `openDocumentInChat` opens
  // the `document` child tab with `focus: true`, that tab's view is the portaled
  // document pane ALONE (`tab-registry.ts` maps `document` → `TabPortalHost`),
  // and the Salon tab is hidden behind it — in v4 exactly as in v5 (v4
  // `open-document-in-chat.ts` → `ws.openTab(...)`, `workspace-reducer.ts:259`
  // `focus = action.focus ?? true`, and v4's own `TabView` hides the inactive
  // parent the same way). The old assertion passed only when Playwright's first
  // visibility poll beat the tab switch — ~9 ms after the click, on the state
  // BEFORE it. That is the whole intermittent.
  const afterOpen = await openDocumentPaths(chatId);
  expect(
    afterOpen,
    `the in-chat arm should have opened ${docPath} in the chat; open documents: ${afterOpen.join(', ')}`,
  ).toContain(docPath!);

  // The pane itself came up, and the conversation was not closed out from under
  // it — its tab is still in the strip beside the document's.
  await expect(
    page.locator('qt-document-pane, .qt-document-pane, .qt-document-mode').first(),
  ).toBeVisible({ timeout: 15_000 });
  await expect(
    page.locator('.qt-tab-strip .qt-tab-label', { hasText: 'Conversation' }).first(),
  ).toBeVisible({ timeout: 15_000 });
  await expect(
    page.locator('.qt-tab-strip .qt-tab-label', { hasText: docName! }),
  ).toBeVisible({ timeout: 15_000 });
  // And nothing threw on the way.
  expect(failures, `errors during the in-chat open: ${failures.join(' | ')}`).toEqual([]);
});
