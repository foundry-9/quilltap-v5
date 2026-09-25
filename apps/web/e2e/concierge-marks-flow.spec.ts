import { expect, request as pwRequest, test, type Page } from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so
 * its filename must sort AFTER aa-foundation.spec.ts (workers: 1, alphabetical
 * file order) — "concierge-marks-flow" ('c') sorts after "aa-foundation".
 *
 * P4.D144 — a LIVE walk of the Concierge marks and the quick-hide rule they
 * drive (v4 `c43d3b1b4`), REWRITTEN by P4.D229 for v4's three states
 * (`4d370a90f` #75; the `info` tone retired at `3b463d6b1` #76). Three beats:
 *
 *   1. The mark itself: one chat per non-default state, the asterisk's tone
 *      per state — Unmoderated the red base rule, Locked the grey `-muted`
 *      modifier, and NO `-info` modifier anywhere — no native `title`,
 *      Moderated wearing nothing, and the drawn bubble speaking the
 *      presentation table's words after the dwell (the operator's sentence:
 *      these chats were set by the operator, through the same verb the
 *      sidebar uses).
 *   2. "Dangerous Chats": the toggle hides Unmoderated ONLY — whoever set it —
 *      and leaves Locked alone, on the Salon list and on the homepage's Recent
 *      Chats. Toggling it off brings them back.
 *   3. The header pill's bubble, which v4 places BELOW the toolbar.
 *
 * ## ACTIVATE-AT-UNIFY (P4.D229)
 *
 * Everything here waits on the Concierge server chain (P4.D225 → P4.D228):
 * `main`'s server 400s the three new values the seeding dispatches, and its
 * list payloads carry the retired four-state `conciergeState`. The whole file
 * is gated by ONE constant, so it activates in one flip and gets its first live
 * run under the unifier's eye (the gated-beat first-run rot class).
 *
 * ## Recorded coverage gap — the bubble's `Categories` line and the
 * Concierge's own sentence
 *
 * Both need a chat the CONCIERGE moved (setBy `concierge`), and nothing this
 * walk can reach writes one: the flip verb stamps `operator`, and the
 * classifier / refusal-ledger switches need a real classification or refusal.
 * The unit specs pin both (`concierge-mark.spec.ts`, `conversation-header.spec
 * .ts`, `recent-chat-item.spec.ts`, the presentation oracle).
 */
const P4D228_SERVER_LANDED = false;
const GATE_REASON =
  'awaits the Concierge server chain (P4.D225→P4.D228: the three states on the flip verb and the list payloads); flipped at unification';

test.skip(!P4D228_SERVER_LANDED, GATE_REASON);

/** The states this walk drives, and what each should wear. */
const STATES = [
  { state: 'moderated', label: null, modifier: null },
  { state: 'unmoderated', label: 'Concierge: Unmoderated', modifier: null },
  { state: 'locked', label: 'Concierge: Locked', modifier: 'qt-concierge-mark-muted' },
] as const;

/** The presentation table's detail sentences, byte for byte (§B). */
const DETAIL: Record<string, string> = {
  moderated:
    'The Concierge sends everything to the usual providers first, and to the uncensored desk only when one of them refuses. After enough refusals he moves the whole chat himself.',
  unmoderated:
    'You have opened the uncensored door yourself. Nothing here goes near a moderated provider.',
  locked:
    'Only the usual providers, ever. If one refuses, the refusal stands. For the chat that must never reach an uncensored model.',
};

const HINT = "Change it from the Salon sidebar's Chat section.";

/** chatId → the state this walk left it in, so afterAll can put it back. */
const seeded = new Map<string, { title: string; state: string }>();

async function dispatch(body: Record<string, unknown>): Promise<Record<string, unknown>> {
  const ctx = await pwRequest.newContext();
  const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: body });
  const parsed = (await res.json().catch(() => null)) as {
    type?: string;
    data?: Record<string, unknown>;
  } | null;
  await ctx.dispose();
  if (!parsed || parsed.type === 'error') {
    throw new Error(`dispatch ${String(body['type'])} failed: ${JSON.stringify(parsed)}`);
  }
  return parsed.data ?? {};
}

/** The locked engine refuses every dispatch, so seeding follows an unlock. */
async function ensureUnlocked(): Promise<void> {
  const ctx = await pwRequest.newContext();
  await ctx
    .post(`${BASE_URL}/api/dispatch`, {
      data: { type: 'unlock', passphrase: E2E_PASSPHRASE },
    })
    .catch(() => undefined);
  await ctx.dispose();
}

test.beforeAll(async () => {
  await ensureUnlocked();
  // Take the two most recently active chats and put one in each non-default
  // state through the SAME manual-flip verb the sidebar control uses. Never
  // assert an absolute chat count — sibling specs seed their own.
  // `listChats` answers `Response::Chats(Vec<…>)` — the ARRAY is `data` itself,
  // not a `{chats}` envelope (that envelope is the REST edge's). The beat's
  // first live run read `data.chats` and saw "0 chats" (unification, 2026-09-02).
  const chats = (await dispatch({ type: 'listChats' })) as unknown as Array<{
    id: string;
    title: string;
  }>;
  if (!Array.isArray(chats) || chats.length < 2) {
    throw new Error(`fixture carries ${chats?.length ?? 0} chats; this walk needs 2`);
  }
  const wanted = ['unmoderated', 'locked'];
  for (let i = 0; i < wanted.length; i++) {
    const chat = chats[i];
    // `chat` is a REQUIRED sibling bag beside `conciergeState` (the P4.D141 shape);
    // the beat's second live run tripped on its absence (unification, 2026-09-02).
    await dispatch({ type: 'chatUpdate', chatId: chat.id, chat: {}, conciergeState: wanted[i] });
    seeded.set(chat.id, { title: chat.title, state: wanted[i] });
  }
});

test.afterAll(async () => {
  for (const id of seeded.keys()) {
    await dispatch({ type: 'chatUpdate', chatId: id, chat: {}, conciergeState: 'moderated' }).catch(
      () => undefined,
    );
  }
});

/** Unlock only when the passphrase screen is showing (the shared server stays unlocked). */
async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  await page.waitForLoadState('domcontentloaded');
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

/** The chat this walk put into `state`, by title. */
function titleOf(state: string): string {
  for (const [, seed] of seeded) {
    if (seed.state === state) return seed.title;
  }
  throw new Error(`nothing was seeded into ${state}`);
}

/** One Salon list card, located by its title. */
function salonCard(page: Page, title: string) {
  return page.locator('qt-chat-card', { hasText: title });
}

/** Toggle "Dangerous Chats" through the shell-footer user menu. */
async function toggleDangerousChats(page: Page, expectPressed: boolean): Promise<void> {
  const trigger = page.getByRole('button', { name: 'User menu' });
  await trigger.click();
  const toggle = page
    .locator('qt-quick-hide-menu-section')
    .getByRole('button', { name: 'Dangerous Chats' });
  await expect(toggle).toBeVisible({ timeout: 10_000 });
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-pressed', String(expectPressed));
  await trigger.click();
}

test.describe('P4.D144 — the Concierge marks on the chat lists', () => {
  test('every non-default state wears its own tone, and none wears a native title', async ({
    page,
  }) => {
    await page.goto(`${BASE_URL}/salon`);
    await maybeUnlock(page);

    for (const { state, label, modifier } of STATES) {
      if (label === null) continue;
      const card = salonCard(page, titleOf(state));
      await expect(card).toHaveCount(1, { timeout: 15_000 });

      const mark = card.locator('.qt-concierge-mark');
      await expect(mark).toHaveCount(1);
      await expect(mark).toHaveText('*');
      await expect(mark).toHaveAttribute('aria-label', label);
      // The drawn bubble replaced the native tooltip; carrying both would
      // double up on it.
      await expect(card.locator('.qt-concierge-mark[title]')).toHaveCount(0);
      if (modifier) {
        await expect(mark).toHaveClass(new RegExp(modifier));
      } else {
        // Danger is the base rule: no modifier is emitted for Unmoderated.
        await expect(card.locator('.qt-concierge-mark-muted')).toHaveCount(0);
      }
      // The retired blue `-info` modifier is gone for good (v4 `3b463d6b1`).
      await expect(card.locator('.qt-concierge-mark-info')).toHaveCount(0);
    }
  });

  test('the mark explains itself in the presentation table’s words', async ({ page }) => {
    await page.goto(`${BASE_URL}/salon`);
    await maybeUnlock(page);

    const mark = salonCard(page, titleOf('unmoderated')).locator('.qt-concierge-mark');
    await expect(mark).toHaveCount(1, { timeout: 15_000 });
    await mark.hover();

    // The 200 ms dwell passes under the auto-waiting expect; the bubble is
    // body-portalled, so it is looked up on the page, not in the card.
    const bubble = page.locator('.qt-tooltip');
    await expect(bubble).toBeVisible({ timeout: 5_000 });
    await expect(bubble).toContainText('Unmoderated');
    // The operator set it (the flip verb stamps `operator`), so the operator's sentence.
    await expect(bubble).toContainText(DETAIL['unmoderated']);
    await expect(bubble).toContainText(HINT);

    await page.mouse.move(10, 10);
    await expect(bubble).toHaveCount(0, { timeout: 5_000 });
  });
});

test.describe('P4.D144 — "Dangerous Chats" hides Unmoderated only', () => {
  test('the toggle hides Unmoderated and spares Locked', async ({ page }) => {
    await page.goto(`${BASE_URL}/salon`);
    await maybeUnlock(page);

    const unmoderated = salonCard(page, titleOf('unmoderated'));
    const locked = salonCard(page, titleOf('locked'));

    // Both are on the list to begin with.
    await expect(unmoderated).toHaveCount(1, { timeout: 15_000 });
    await expect(locked).toHaveCount(1);

    await toggleDangerousChats(page, true);

    // Unmoderated goes (whoever set it — here the operator); Locked takes the
    // ordinary desks and stays (v4 `4d370a90f`).
    await expect(unmoderated).toHaveCount(0, { timeout: 15_000 });
    await expect(locked).toHaveCount(1);

    await toggleDangerousChats(page, false);
    await expect(unmoderated).toHaveCount(1, { timeout: 15_000 });
  });

  test('the homepage’s Recent Chats obeys the same rule', async ({ page }) => {
    await page.goto(BASE_URL);
    await maybeUnlock(page);

    const section = page.locator('qt-recent-chats-section');
    await expect(section.locator('qt-recent-chat-item').first()).toBeVisible({ timeout: 15_000 });

    // Recent Chats is the twelve most recently active chats, and in a FULL
    // run sibling specs seed newer chats after beforeAll ran, so the chat it
    // opened may have scrolled off (this beat's first full-suite run — green
    // in isolation — caught exactly that). So open the door on whichever chat
    // IS on the list, through the same verb, and put it back afterwards; the
    // delta is what the beat asserts, never membership.
    const onList = await section.locator('qt-recent-chat-item').allTextContents();
    const chats = (await dispatch({ type: 'listChats' })) as unknown as Array<{
      id: string;
      title: string;
    }>;
    const skip = new Set([titleOf('locked'), titleOf('unmoderated')]);
    const target = chats.find(
      (c) => !skip.has(c.title) && onList.some((text) => text.includes(c.title)),
    );
    expect(target, 'some Moderated chat must be on Recent Chats to open').toBeTruthy();
    const restoreTo = seeded.get(target!.id)?.state ?? 'moderated';
    if (restoreTo !== 'unmoderated') {
      await dispatch({
        type: 'chatUpdate',
        chatId: target!.id,
        chat: {},
        conciergeState: 'unmoderated',
      });
      await page.reload();
      await maybeUnlock(page);
    }
    const openedRow = section.locator('qt-recent-chat-item', { hasText: target!.title });
    const lockedRow = section.locator('qt-recent-chat-item', { hasText: titleOf('locked') });
    await expect(openedRow).toHaveCount(1, { timeout: 15_000 });
    const lockedBefore = await lockedRow.count();

    try {
      await toggleDangerousChats(page, true);
      await expect(openedRow).toHaveCount(0, { timeout: 15_000 });
      await expect(lockedRow).toHaveCount(lockedBefore);

      await toggleDangerousChats(page, false);
      await expect(openedRow).toHaveCount(1, { timeout: 15_000 });
    } finally {
      if (restoreTo !== 'unmoderated') {
        await dispatch({
          type: 'chatUpdate',
          chatId: target!.id,
          chat: {},
          conciergeState: restoreTo,
        }).catch(() => undefined);
      }
    }
  });
});

test.describe('P4.D144 — the header pill explains itself below the toolbar', () => {
  test('the pill grows the drawn bubble, and carries no native title', async ({ page }) => {
    await page.goto(`${BASE_URL}/salon`);
    await maybeUnlock(page);
    await page.getByRole('link', { name: titleOf('locked') }).first().click();

    const pill = page.locator('qt-conversation-header .qt-danger-badge');
    await expect(pill).toHaveCount(1, { timeout: 15_000 });
    await expect(pill).toHaveText('Locked');
    await expect(pill).toHaveAttribute('aria-label', 'Concierge: Locked');
    await expect(pill).toHaveClass(/qt-danger-badge-muted/);
    // The native titles are retired as v4 retires them.
    await expect(page.locator('qt-conversation-header .qt-danger-badge[title]')).toHaveCount(0);

    await pill.hover();
    const bubble = page.locator('.qt-tooltip');
    await expect(bubble).toBeVisible({ timeout: 5_000 });
    await expect(bubble).toContainText('Locked');
    await expect(bubble).toContainText(DETAIL['locked']);
    await expect(bubble).toContainText(HINT);
    // v4 asks for the bubble BELOW the toolbar; the primitive may still flip it
    // away from a viewport edge, so the attribute is read rather than assumed.
    await expect(bubble).toHaveAttribute('data-placement', 'bottom');
  });
});
