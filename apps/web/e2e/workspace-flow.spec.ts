import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { expect, test, type Page } from '@playwright/test';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: rides the SHARED global-setup server (unlocked by foundation.spec).
 * "workspace-flow" ('w') sorts last, so it never disturbs the route-mode specs
 * that run before it.
 *
 * P4.9J1 — the tabbed workspace, flag ON. Unlike every other spec (which
 * imports `./support/fixtures` to inject the `quilltap.workspace.tabs = '0'`
 * opt-out and run v4's supported ROUTE mode), THIS file imports the BASE
 * `@playwright/test` so the flag stays ON (the default) — exercising the
 * workspace shell for real.
 *
 * Beats (the tier-1 floor):
 *   1. `/` redirects to `/workspace?open=home` → a Home tab, URL stripped clean.
 *   2. a rail click opens a second tab; re-clicking de-dupes (focuses, no dup).
 *   3. `Ctrl+Alt+\` splits off the active tab; a reload restores the layout.
 *   4. a deep-link `/settings?tab=…&section=…` redirect carries its intent and
 *      strips the URL — landing on the REAL Settings screen with the payload
 *      tab active (activated at the p4.9j unification).
 *   5. (P4.d16) the salon funnel: the rail's Chats now opens a salon-list TAB —
 *      the workspace does NOT unmount (v4 `8d86847a`) — and a chat card click
 *      inside it opens the conversation as a second tab, live.
 *   6. (unify) the characters in-tab drill: a roster card click drills to the
 *      detail IN PLACE (no navigation); back restores the list.
 *   7. (p4.9j3) the real HTML5 drag-split: a mouse/native drag of a tab onto the
 *      right drop-zone opens the split, and a pointer drag of the divider
 *      re-ratios it within [MIN, MAX] — the p4.9j1 tier-2 divider deferral.
 *   8. (p4.9j3, ACTIVATE-AT-UNIFY) the wardrobe tab renders the bare asTab
 *      surface. Until the unifier swaps the tab-registry `wardrobe` row to the
 *      new WardrobeTabView, the kind still shows the not-wired pane — the beat
 *      SKIPS while unwired and self-activates once the swap lands.
 *   9. (p4.9j3, item 6) the cross-theme workspace accent: every bundled
 *      `[data-theme]` root resolves `--qt-workspace-accent` to a distinct
 *      concrete colour that differs from the default `--color-primary` fallback
 *      (the never-run check; the ruling keeps the static hex).
 *  10. (P4.d16) a `/prospero/<id>` deep link lands DRILLED into that project
 *      inside one Projects tab, and a second deep link re-targets the same tab.
 *  11. (P4.d16) a `/characters/<id>` deep link opens the character-view tab
 *      (the detail redirect that used to render the legacy full page).
 *  12. (P4.d16 tier 2) a `/salon/new` deep link opens the v5-only salon-new tab
 *      hosting the New-Chat screen, seeded with `?characterId=`; Cancel closes
 *      the tab the way v4's modal dismisses.
 */

async function openWorkspace(page: Page): Promise<void> {
  await page.goto('/');
  // Unlock only if the passphrase screen is showing (the shared server may stay
  // unlocked across contexts).
  const passphrase = page.locator('#qt-passphrase');
  const workspace = page.locator('.qt-workspace');
  await expect(passphrase.or(workspace).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
  await expect(workspace).toBeVisible({ timeout: 15_000 });
}

const railLink = (page: Page, label: string) =>
  page.locator(`aside.qt-left-sidebar a.qt-collapsed-nav-button[aria-label="${label}"]`);
const tabs = (page: Page) => page.locator('.qt-tab-strip .qt-tab');
const tabLabel = (page: Page, text: string) =>
  page.locator('.qt-tab-strip .qt-tab-label', { hasText: text });

test('flag-on: / redirects into the workspace with a Home tab', async ({ page }) => {
  await openWorkspace(page);
  // The ?open=home intent applied, then the URL was stripped clean.
  await expect(page).toHaveURL(/\/workspace$/);
  await expect(tabLabel(page, 'Home')).toBeVisible();
  await expect(tabs(page)).toHaveCount(1);
});

test('a rail click opens a second tab; re-clicking de-dupes', async ({ page }) => {
  await openWorkspace(page);
  await railLink(page, 'Characters').click();
  await expect(tabLabel(page, 'Characters')).toBeVisible();
  await expect(tabs(page)).toHaveCount(2);
  // De-dupe: opening the same surface again just focuses it.
  await railLink(page, 'Characters').click();
  await expect(tabs(page)).toHaveCount(2);
});

test('Ctrl+Alt+\\ splits the workspace; a reload restores the layout', async ({ page }) => {
  await openWorkspace(page);
  await railLink(page, 'Characters').click();
  await expect(tabs(page)).toHaveCount(2);

  // Move focus onto the pane (off the rail anchor), then split off the active tab.
  await page.locator('.qt-workspace').click({ position: { x: 5, y: 5 } });
  await page.keyboard.press('Control+Alt+\\');
  await expect(page.locator('.qt-workspace-divider')).toBeVisible();

  // Let the debounced persist land, then reload and confirm the split layout
  // (two tabs across two panes) is restored from localStorage.
  await page.waitForTimeout(400);
  await page.reload();
  await expect(page.locator('.qt-workspace')).toBeVisible({ timeout: 15_000 });
  await expect(page.locator('.qt-workspace-divider')).toBeVisible();
  await expect(tabs(page)).toHaveCount(2);
});

test('a deep-link /settings redirect carries its intent onto the real Settings screen', async ({
  page,
}) => {
  await page.goto('/settings?tab=system&section=memory');
  const passphrase = page.locator('#qt-passphrase');
  const workspace = page.locator('.qt-workspace');
  await expect(passphrase.or(workspace).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
  await expect(workspace).toBeVisible({ timeout: 15_000 });
  // Redirected into the workspace and the intent params stripped.
  await expect(page).toHaveURL(/\/workspace$/);
  // The settings tab opened (v4 default title "The Foundry")…
  await expect(tabLabel(page, 'The Foundry')).toBeVisible();
  // …and renders the REAL Settings screen (activated at unification) with the
  // payload's tab seeded active — no not-wired pane in sight.
  await expect(page.locator('[data-not-wired]')).toHaveCount(0);
  await expect(
    page.locator('.qt-tab-group .qt-tab-active', { hasText: 'Data & System' }),
  ).toBeVisible();
});

test('the rail Chats item opens a salon-list tab, and a chat card opens the conversation', async ({
  page,
}) => {
  await openWorkspace(page);
  // P4.d16 (v4 `8d86847a`): the Chats item used to leave the workspace for the
  // standalone /salon list. It now opens the salon-list TAB — the interceptor
  // catches the rail anchor, so the workspace never unmounts.
  await railLink(page, 'Chats').click();
  await expect(tabLabel(page, 'Chats')).toBeVisible();
  await expect(page).toHaveURL(/\/workspace$/);
  await expect(tabs(page)).toHaveCount(2);

  const soloCard = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Solo Voyage' });
  await expect(soloCard).toBeVisible({ timeout: 15_000 });
  // The card is a plain /salon/:id anchor — the interceptor turns it into a
  // third tab, and the REAL conversation renders inside it.
  await soloCard.click();
  await expect(page).toHaveURL(/\/workspace$/);
  await expect(tabs(page)).toHaveCount(3);
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
  await expect(page.locator('.qt-chat-composer-input .qt-rich-editor-content')).toBeVisible();
});

test('the characters tab drills to a detail in place and back restores the roster', async ({
  page,
}) => {
  await openWorkspace(page);
  await railLink(page, 'Characters').click();
  await expect(page.getByRole('heading', { name: 'Characters', exact: true })).toBeVisible({
    timeout: 15_000,
  });
  const aria = page
    .locator('.character-card-grid .character-card')
    .filter({ hasText: 'Aria' })
    .first();
  await aria.locator('p.line-clamp-3').click();
  // The drill renders the detail IN PLACE — still /workspace, no navigation.
  await expect(page.getByRole('heading', { name: 'Aria' })).toBeVisible({ timeout: 15_000 });
  await expect(page).toHaveURL(/\/workspace$/);
  // Back restores the kept-alive roster.
  await page.getByRole('button', { name: '← Back to Characters' }).click();
  await expect(page.getByRole('heading', { name: 'Characters', exact: true })).toBeVisible();
});

test('a real HTML5 tab drag opens the split; a divider pointer-drag re-ratios it', async ({
  page,
}) => {
  await openWorkspace(page);
  await railLink(page, 'Characters').click();
  await expect(tabs(page)).toHaveCount(2);
  // Start unsplit.
  await expect(page.locator('.qt-workspace-divider')).toHaveCount(0);

  // Native HTML5 drag: mouse-synthesized DnD is flaky in Chromium, so dispatch
  // the events with a shared DataTransfer. `dragstart` flips the host's
  // draggingId signal, which renders the split drop-zone; `drop` splits.
  const dataTransfer = await page.evaluateHandle(() => new DataTransfer());
  const draggable = page
    .locator('.qt-tab-strip .qt-tab[draggable="true"]')
    .filter({ hasText: 'Characters' })
    .first();
  await draggable.dispatchEvent('dragstart', { dataTransfer });

  const dropZone = page.locator('.qt-tab-drop-zone');
  await expect(dropZone).toBeVisible();
  await dropZone.dispatchEvent('dragover', { dataTransfer });
  await dropZone.dispatchEvent('drop', { dataTransfer });
  await draggable.dispatchEvent('dragend', { dataTransfer });

  // The split opened (a second pane + the divider), still two tabs total.
  const divider = page.locator('.qt-workspace-divider');
  await expect(divider).toBeVisible();
  await expect(tabs(page)).toHaveCount(2);

  // A pointer drag of the divider to the left decreases the left-pane ratio.
  const before = Number(await divider.getAttribute('aria-valuenow'));
  expect(before).toBe(50); // DEFAULT_SPLIT_RATIO
  const box = await divider.boundingBox();
  if (!box) throw new Error('divider has no bounding box');
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x - 200, box.y + box.height / 2, { steps: 8 });
  await page.mouse.up();

  // The ratio changed and stayed within [MIN_SPLIT_RATIO, MAX_SPLIT_RATIO].
  await expect
    .poll(async () => Number(await divider.getAttribute('aria-valuenow')))
    .not.toBe(before);
  const after = Number(await divider.getAttribute('aria-valuenow'));
  expect(after).toBeGreaterThanOrEqual(20);
  expect(after).toBeLessThanOrEqual(80);
  expect(after).toBeLessThan(before); // moved left ⇒ smaller left pane
});

test('the wardrobe tab renders the bare asTab surface (activate-at-unify)', async ({ page }) => {
  await page.goto('/workspace?open=wardrobe');
  const passphrase = page.locator('#qt-passphrase');
  const workspace = page.locator('.qt-workspace');
  await expect(passphrase.or(workspace).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
  await expect(workspace).toBeVisible({ timeout: 15_000 });

  const notWired = page.locator('[data-not-wired][data-kind="wardrobe"]');
  const bare = page.locator('.qt-wardrobe-tab');
  await expect(notWired.or(bare).first()).toBeVisible({ timeout: 15_000 });

  // ACTIVATE-AT-UNIFY: while the tab-registry still points wardrobe → the
  // not-wired pane, skip. The unifier's registry swap self-activates this beat.
  if (await notWired.count()) {
    test.skip(true, 'wardrobe tab-registry swap lands at the p4.9j3 unification');
    return;
  }

  // Live: the bare tab chrome (no floating modal overlay, no footer) with the
  // wardrobe body inside, and a character auto-selected.
  await expect(bare).toBeVisible();
  await expect(page.locator('.qt-dialog-overlay')).toHaveCount(0);
  await expect(page.locator('.qt-dialog-footer')).toHaveCount(0);
  // P4.D113 renamed this control: the top selector is no longer a character
  // dropdown but a CONTAINER one (characters, Quilltap General, projects,
  // groups), so the id moved with it.
  await expect(page.locator('#wardrobe-container-select')).toBeVisible();
});

test('every bundled theme gives the workspace a distinct accent (cross-theme, p4.9j3 item 6)', async ({
  page,
}) => {
  const THEMES = ['art-deco', 'earl-grey', 'great-estate', 'madmans-box', 'old-school', 'rains'];

  // (a) The DETERMINISTIC bundled-default contract: read the committed
  // _workspace.css and confirm each [data-theme] root declares a concrete,
  // DISTINCT hex accent. This is order-independent — unlike the raw runtime
  // custom-property value, which the higher-specificity runtime theme pack
  // overrides with v4's live var() token once its <link> loads (the corrected
  // item-6 ruling). Node fs is available in the Playwright test runtime.
  const css = readFileSync(
    resolve(__dirname, '../src/styles/qt-components/_workspace.css'),
    'utf8',
  );
  const bundled = new Map<string, string>();
  for (const t of THEMES) {
    const m = css.match(
      new RegExp(`\\[data-theme='${t}'\\]\\s*\\{[^}]*?--qt-workspace-accent:\\s*([^;]+);`),
    );
    expect(m, `_workspace.css declares a bundled accent for ${t}`).not.toBeNull();
    const hex = m![1].trim();
    expect(hex, `${t} bundled accent is a concrete hex`).toMatch(/^#[0-9a-fA-F]{3,8}$/);
    bundled.set(t, hex);
  }
  expect(new Set(bundled.values()).size, 'the six bundled accents are distinct').toBe(THEMES.length);

  // (b) A browser resolution check (order-independent): each [data-theme] root
  // resolves --qt-workspace-accent to a real, non-transparent colour — whether
  // that is the bundled hex (no pack loaded) or the pack's live token (pack
  // loaded). This proves the accent wires into the DOM for all six roots.
  await openWorkspace(page);
  const resolved = await page.evaluate((themes: string[]) => {
    const root = document.documentElement;
    const prev = root.getAttribute('data-theme');
    const out: Record<string, string> = {};
    for (const t of themes) {
      root.setAttribute('data-theme', t);
      const probe = document.createElement('div');
      probe.style.color = 'var(--qt-workspace-accent, var(--color-primary))';
      document.body.appendChild(probe);
      out[t] = getComputedStyle(probe).color;
      probe.remove();
    }
    if (prev) root.setAttribute('data-theme', prev);
    else root.removeAttribute('data-theme');
    return out;
  }, THEMES);
  for (const t of THEMES) {
    expect(resolved[t], `${t} accent resolves to an rgb colour`).toMatch(/^rgba?\(/);
    expect(resolved[t], `${t} accent is not transparent`).not.toMatch(/rgba?\([^)]*,\s*0\)/);
  }
});

/**
 * P4.d16 (v4 `8d86847a`) — the deep links that used to escape the workspace.
 *
 * Beat 10 needs a project id, so it mints one over the live dispatch route (the
 * spec sorts last, so the extra row disturbs nothing) and then deep-links to it.
 */
test('a /prospero/<id> deep link opens the Projects tab drilled into that project', async ({
  page,
}) => {
  await openWorkspace(page);

  async function makeProject(name: string): Promise<string> {
    const res = await page.request.post('/api/dispatch', {
      data: { type: 'projectCreate', project: { name } },
    });
    expect(res.ok(), `projectCreate ${name}`).toBe(true);
    const body = (await res.json()) as {
      data?: { project?: { id: string } };
      project?: { id: string };
    };
    const id = body.data?.project?.id ?? body.project?.id;
    expect(id, 'the created project id').toBeTruthy();
    return id as string;
  }

  const first = await makeProject('Drift Drill One');
  const second = await makeProject('Drift Drill Two');

  // The deep link redirects into /workspace?open=prospero&projectId=… — the
  // Projects tab opens ALREADY drilled into the project's detail.
  await page.goto(`/prospero/${first}`);
  await expect(page.locator('.qt-workspace')).toBeVisible({ timeout: 15_000 });
  await expect(page).toHaveURL(/\/workspace$/);
  await expect(tabLabel(page, 'Projects')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Drift Drill One' })).toBeVisible({
    timeout: 15_000,
  });

  // A second deep link RE-TARGETS the one Projects tab (prospero stays a
  // singleton keyed by kind — the corpus pins that).
  const beforeCount = await tabs(page).count();
  await page.goto(`/prospero/${second}`);
  await expect(page.getByRole('heading', { name: 'Drift Drill Two' })).toBeVisible({
    timeout: 15_000,
  });
  await expect(tabs(page)).toHaveCount(beforeCount);
});

test('a /characters/<id> deep link opens the character detail as a tab', async ({ page }) => {
  await openWorkspace(page);
  // A real character id, over the live dispatch route (the roster's in-tab
  // cards drill through buttons, so there is no href to read).
  const res = await page.request.post('/api/dispatch', { data: { type: 'characterList' } });
  expect(res.ok(), 'characterList').toBe(true);
  const body = (await res.json()) as {
    data?: { characters?: { id: string; name: string }[] };
    characters?: { id: string; name: string }[];
  };
  const characters = body.data?.characters ?? body.characters ?? [];
  const aria = characters.find((c) => c.name === 'Aria');
  expect(aria, 'the Aria fixture character').toBeTruthy();

  await page.goto(`/characters/${aria!.id}`);
  await expect(page.locator('.qt-workspace')).toBeVisible({ timeout: 15_000 });
  await expect(page).toHaveURL(/\/workspace$/);
  await expect(page.getByRole('heading', { name: 'Aria' })).toBeVisible({ timeout: 15_000 });
});

test('a /salon/new deep link opens the New Chat tab seeded with its character', async ({
  page,
}) => {
  await openWorkspace(page);
  await page.goto('/salon/new?autonomous=1');
  await expect(page.locator('.qt-workspace')).toBeVisible({ timeout: 15_000 });
  await expect(page).toHaveURL(/\/workspace$/);
  await expect(tabLabel(page, 'New Chat')).toBeVisible();
  // The autonomous seed rode the payload, not a query param (which is stripped).
  await expect(page.getByRole('heading', { name: 'New Autonomous Room' })).toBeVisible({
    timeout: 15_000,
  });

  // Cancel closes the tab (v4's modal dismissal), leaving the workspace intact.
  const before = await tabs(page).count();
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(tabs(page)).toHaveCount(before - 1);
  await expect(page.locator('.qt-workspace')).toBeVisible();
});

/**
 * Beat 13 (P4.d16): the terminal pop-out funnel. v4's two-step opens the Salon
 * tab FIRST — it is the portal source for the live PTY — then the terminal as
 * its CHILD tab, so the pane it portals is a real, mounted conversation view.
 * The session is spawned over the live REST leg so the deep link addresses a
 * genuine PTY.
 */
test('a terminal pop-out deep link opens the Salon tab plus a child terminal tab', async ({
  page,
}) => {
  await openWorkspace(page);

  // A real chat id, off the salon list's own card link.
  await railLink(page, 'Chats').click();
  const soloCard = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Solo Voyage' });
  await expect(soloCard).toBeVisible({ timeout: 15_000 });
  const chatHref = await soloCard.getAttribute('href');
  const chatId = (chatHref ?? '').split('/').pop() as string;
  expect(chatId, 'a chat id from the salon list').toBeTruthy();

  const spawned = await page.request.post('/api/v1/terminals', {
    data: { chatId, label: 'Drift Popout', cols: 80, rows: 24 },
  });
  expect(spawned.ok(), 'spawn a real PTY').toBe(true);
  const session = (await spawned.json()) as { session?: { id: string }; id?: string };
  const sessionId = session.session?.id ?? session.id;
  expect(sessionId, 'the spawned session id').toBeTruthy();

  await page.goto(`/salon/${chatId}/terminal/${sessionId}`);
  await expect(page.locator('.qt-workspace')).toBeVisible({ timeout: 15_000 });
  await expect(page).toHaveURL(/\/workspace$/);
  // BOTH tabs — the conversation (the intent opens it with the kind's default
  // title) and its terminal child.
  await expect(tabLabel(page, 'Conversation')).toBeVisible({ timeout: 15_000 });
  await expect(tabLabel(page, 'Terminal')).toBeVisible();
  // Closing the Salon parent cascades the terminal child (the reducer rule the
  // two-step exists to preserve).
  const before = await tabs(page).count();
  await page
    .locator('.qt-tab-strip .qt-tab', { hasText: 'Conversation' })
    .locator('.qt-tab-close')
    .click();
  await expect(tabs(page)).toHaveCount(before - 2);
});
