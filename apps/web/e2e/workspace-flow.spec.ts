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
 *   5. (unify) the salon funnel: the rail's Chats leaves the workspace for the
 *      standalone `/salon` list (v4-faithful); a chat click funnels back in
 *      through the redirect guard and the salon tab renders the live
 *      conversation.
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

test('a chat opened from the salon list funnels into a live workspace salon tab', async ({
  page,
}) => {
  await openWorkspace(page);
  // The rail's Chats leaves the workspace for the standalone /salon list
  // (v4-faithful: the salon LIST is not a tab kind).
  await railLink(page, 'Chats').click();
  const soloCard = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Solo Voyage' });
  await expect(soloCard).toBeVisible({ timeout: 15_000 });
  // The chat click hits /salon/:id, whose redirect guard funnels straight back
  // into the workspace as a salon tab carrying the chatId.
  await soloCard.click();
  await expect(page.locator('.qt-workspace')).toBeVisible({ timeout: 15_000 });
  await expect(page).toHaveURL(/\/workspace$/);
  // The REAL conversation renders inside the tab (activated at unification).
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
  await expect(page.locator('#wardrobe-char-select')).toBeVisible();
});

test('every bundled theme gives the workspace a distinct accent (cross-theme)', async ({
  page,
}) => {
  await openWorkspace(page);
  const THEMES = ['art-deco', 'earl-grey', 'great-estate', 'madmans-box', 'old-school', 'rains'];

  // Drive each [data-theme] root and read the ACCENT resolved through the same
  // fallback the consumers use, alongside the bare --color-primary fallback.
  // The bundled _workspace.css supplies the accents, so no runtime theme pack
  // needs to load for this — only the data-theme attribute.
  const results = await page.evaluate((themes: string[]) => {
    const root = document.documentElement;
    const prev = root.getAttribute('data-theme');
    const out: Record<string, { accent: string; fallback: string; declared: string }> = {};
    for (const t of themes) {
      root.setAttribute('data-theme', t);
      const probe = document.createElement('div');
      probe.style.color = 'var(--qt-workspace-accent, var(--color-primary))';
      document.body.appendChild(probe);
      const fallbackProbe = document.createElement('div');
      fallbackProbe.style.color = 'var(--color-primary)';
      document.body.appendChild(fallbackProbe);
      out[t] = {
        accent: getComputedStyle(probe).color,
        fallback: getComputedStyle(fallbackProbe).color,
        declared: getComputedStyle(root).getPropertyValue('--qt-workspace-accent').trim(),
      };
      probe.remove();
      fallbackProbe.remove();
    }
    if (prev) root.setAttribute('data-theme', prev);
    else root.removeAttribute('data-theme');
    return out;
  }, THEMES);

  const accents = new Set<string>();
  for (const t of THEMES) {
    const r = results[t];
    // Declared as a concrete hex (the static-hex ruling), resolving to real rgb…
    expect(r.declared, `${t} declares an accent`).toMatch(/^#[0-9a-fA-F]{3,8}$/);
    expect(r.accent, `${t} accent resolves to rgb`).toMatch(/^rgba?\(/);
    // …and differing from the default --color-primary fallback.
    expect(r.accent, `${t} accent differs from the fallback`).not.toBe(r.fallback);
    accents.add(r.accent);
  }
  // Each of the six themes has its own signature accent.
  expect(accents.size).toBe(THEMES.length);
});
