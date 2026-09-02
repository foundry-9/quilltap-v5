import { expect, request as pwRequest, test, type Page } from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this file rides the SHARED global-setup server and unlocks it, so
 * its filename must sort AFTER aa-foundation.spec.ts (workers: 1, alphabetical
 * file order) — "concierge-marks-flow" ('c') sorts after "aa-foundation".
 *
 * P4.D144 — a LIVE walk of the Concierge marks and the quick-hide rule they
 * now drive (v4 `c43d3b1b4`). Three beats:
 *
 *   1. The mark itself: one chat per state, the asterisk's tone class per
 *      state, no native `title` anywhere, Monitored wearing nothing, and the
 *      drawn bubble speaking the presentation table's words after the dwell.
 *   2. "Dangerous Chats": the toggle hides the uncensored ROW — Flagged and
 *      Uncensored — and leaves Vouched Safe alone, on the Salon list and on
 *      the homepage's Recent Chats. Toggling it off brings them back. This is
 *      the coverage gap this lane closes: `quick-hide-flow.spec.ts` drives the
 *      TAG arm only, and no beat anywhere asserted list danger filtering.
 *   3. The header pill's bubble, which v4 places BELOW the toolbar.
 *
 * ## ACTIVATE-AT-UNIFY
 *
 * The mark reads `conciergeState` off the LIST payload, which P4.D143 derives
 * server-side (shared contract §A). Until that lane is on the branch every
 * list row arrives stateless, no mark renders at all (by design — that is what
 * keeps this port green in the meantime), and nothing here could pass for a
 * reason that says anything about the feature. The unifier flips
 * {@link P4D143_LIST_PAYLOAD_LANDED} to `true` and RUNS these beats at first
 * activation (the gated-beat first-run rot class).
 *
 * Beat 3 does not itself depend on the payload — the header pill reads the
 * single-chat GET, which keeps the raw trio — but it is held behind the same
 * constant as its siblings so the whole file activates in ONE flip and gets its
 * first live run under the unifier's eye.
 *
 * ⚠ The SAME unification must also flip `CHATS_HAS_DANGEROUS_VERB_LANDED` in
 * `app/quick-hide/quick-hide.service.ts` (shared contract §H). The footer's
 * quick-hide section is now gated on `hasQuickHideFeatures` exactly as v4's
 * `sidebar-footer.tsx:145` gates it, and on a fixture with no flagged tag and
 * the toggle off, the only arm that can open it is the uncensored-row probe —
 * which the seeded Flagged and Uncensored chats satisfy. Without that flip,
 * beat 2 cannot reach the toggle at all.
 *
 * ## Recorded coverage gap — the bubble's `Categories` line
 *
 * v4's corpus asserts the `Categories` section on a Flagged chat, and so do
 * three v5 unit specs (`concierge-mark.spec.ts`, `conversation-header.spec.ts`,
 * `concierge-state-presentation.spec.ts`). It is NOT asserted here, because
 * nothing this walk can reach writes `chats.dangerCategories`: the column is
 * written by the classifier job and cleared by the manual flip, and `ChatPatch`
 * is an internal Rust struct rather than a request bag — no dispatch verb
 * carries the field. Asserting it would author a beat guaranteed to fail on
 * first activation for a reason that is not the feature. Seeding it needs the
 * own-server + CLI-SQL pattern of `salon-concierge-four-state-flow.spec.ts`;
 * that is the shape a follow-up would take.
 */
const P4D143_LIST_PAYLOAD_LANDED = true;

/** The three states this walk drives, and what each should wear. */
const STATES = [
  { state: 'monitored', label: null, modifier: null },
  { state: 'flagged', label: 'Concierge: Flagged', modifier: null },
  { state: 'vouched', label: 'Concierge: Vouched Safe', modifier: 'qt-concierge-mark-muted' },
  { state: 'uncensored', label: 'Concierge: Uncensored', modifier: 'qt-concierge-mark-info' },
] as const;

/** The presentation table's four detail sentences, byte for byte (§B). */
const DETAIL: Record<string, string> = {
  monitored:
    'The Concierge keeps watch, and will flip the switch himself if the conversation calls for it.',
  flagged:
    'The Concierge has this chat down as dangerous, and routes it through the uncensored providers.',
  vouched:
    'You have vouched for this chat. The Concierge stops watching; the ordinary providers still apply, and may still refuse.',
  uncensored:
    'You have sent the Concierge away and opened the uncensored door yourself. Nothing is scanned, nothing is softened — the risk is yours.',
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
  // Take the three most recently active chats and put one in each non-default
  // state through the SAME manual-flip verb the sidebar control uses. Never
  // assert an absolute chat count — sibling specs seed their own.
  // `listChats` answers `Response::Chats(Vec<…>)` — the ARRAY is `data` itself,
  // not a `{chats}` envelope (that envelope is the REST edge's). The beat's
  // first live run read `data.chats` and saw "0 chats" (unification, 2026-09-02).
  const chats = (await dispatch({ type: 'listChats' })) as unknown as Array<{
    id: string;
    title: string;
  }>;
  if (!Array.isArray(chats) || chats.length < 3) {
    throw new Error(`fixture carries ${chats?.length ?? 0} chats; this walk needs 3`);
  }
  const wanted = ['flagged', 'vouched', 'uncensored'];
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
    await dispatch({ type: 'chatUpdate', chatId: id, chat: {}, conciergeState: 'monitored' }).catch(
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
    test.skip(
      !P4D143_LIST_PAYLOAD_LANDED,
      "awaits P4.D143's conciergeState on the chat-list payload (§A)",
    );
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
        // Danger is the base rule: neither modifier is emitted for Flagged.
        await expect(card.locator('.qt-concierge-mark-muted')).toHaveCount(0);
        await expect(card.locator('.qt-concierge-mark-info')).toHaveCount(0);
      }
    }
  });

  test('the mark explains itself in the presentation table’s words', async ({ page }) => {
    test.skip(
      !P4D143_LIST_PAYLOAD_LANDED,
      "awaits P4.D143's conciergeState on the chat-list payload (§A)",
    );
    await page.goto(`${BASE_URL}/salon`);
    await maybeUnlock(page);

    const mark = salonCard(page, titleOf('uncensored')).locator('.qt-concierge-mark');
    await expect(mark).toHaveCount(1, { timeout: 15_000 });
    await mark.hover();

    // The 200 ms dwell passes under the auto-waiting expect; the bubble is
    // body-portalled, so it is looked up on the page, not in the card.
    const bubble = page.locator('.qt-tooltip');
    await expect(bubble).toBeVisible({ timeout: 5_000 });
    await expect(bubble).toContainText('Uncensored');
    await expect(bubble).toContainText(DETAIL['uncensored']);
    await expect(bubble).toContainText(HINT);

    await page.mouse.move(10, 10);
    await expect(bubble).toHaveCount(0, { timeout: 5_000 });
  });
});

test.describe('P4.D144 — "Dangerous Chats" follows the uncensored row', () => {
  test('the toggle hides Flagged and Uncensored and spares Vouched Safe', async ({ page }) => {
    test.skip(
      !P4D143_LIST_PAYLOAD_LANDED,
      "awaits P4.D143's conciergeState on the chat-list payload (§A)",
    );
    await page.goto(`${BASE_URL}/salon`);
    await maybeUnlock(page);

    const flagged = salonCard(page, titleOf('flagged'));
    const vouched = salonCard(page, titleOf('vouched'));
    const uncensored = salonCard(page, titleOf('uncensored'));

    // All three are on the list to begin with.
    await expect(flagged).toHaveCount(1, { timeout: 15_000 });
    await expect(vouched).toHaveCount(1);
    await expect(uncensored).toHaveCount(1);

    await toggleDangerousChats(page, true);

    // The uncensored row goes; the vouched chat stays, dangerous label
    // preserved underneath and all — the behaviour v4 c43d3b1b4 changed in
    // both directions.
    await expect(flagged).toHaveCount(0, { timeout: 15_000 });
    await expect(uncensored).toHaveCount(0);
    await expect(vouched).toHaveCount(1);

    await toggleDangerousChats(page, false);
    await expect(flagged).toHaveCount(1, { timeout: 15_000 });
    await expect(uncensored).toHaveCount(1);
  });

  test('the homepage’s Recent Chats obeys the same rule', async ({ page }) => {
    test.skip(
      !P4D143_LIST_PAYLOAD_LANDED,
      "awaits P4.D143's conciergeState on the chat-list payload (§A)",
    );
    await page.goto(BASE_URL);
    await maybeUnlock(page);

    const section = page.locator('qt-recent-chats-section');
    await expect(section.locator('qt-recent-chat-item').first()).toBeVisible({ timeout: 15_000 });

    // Recent Chats is the twelve most recently active chats, and in a FULL
    // run sibling specs seed newer chats after beforeAll ran, so the chat it
    // flagged may have scrolled off (this beat's first full-suite run — green
    // in isolation — caught exactly that). So flag whichever chat IS on the
    // list, through the same verb, and put it back afterwards; the delta is
    // what the beat asserts, never membership.
    const onList = await section.locator('qt-recent-chat-item').allTextContents();
    const chats = (await dispatch({ type: 'listChats' })) as unknown as Array<{
      id: string;
      title: string;
    }>;
    const skip = new Set([titleOf('vouched'), titleOf('uncensored')]);
    const target = chats.find(
      (c) => !skip.has(c.title) && onList.some((text) => text.includes(c.title)),
    );
    expect(target, 'some non-operator chat must be on Recent Chats to flag').toBeTruthy();
    const restoreTo = seeded.get(target!.id)?.state ?? 'monitored';
    if (restoreTo !== 'flagged') {
      await dispatch({ type: 'chatUpdate', chatId: target!.id, chat: {}, conciergeState: 'flagged' });
      await page.reload();
      await maybeUnlock(page);
    }
    const flaggedRow = section.locator('qt-recent-chat-item', { hasText: target!.title });
    const vouchedRow = section.locator('qt-recent-chat-item', { hasText: titleOf('vouched') });
    await expect(flaggedRow).toHaveCount(1, { timeout: 15_000 });
    const vouchedBefore = await vouchedRow.count();

    try {
      await toggleDangerousChats(page, true);
      await expect(flaggedRow).toHaveCount(0, { timeout: 15_000 });
      await expect(vouchedRow).toHaveCount(vouchedBefore);

      await toggleDangerousChats(page, false);
      await expect(flaggedRow).toHaveCount(1, { timeout: 15_000 });
    } finally {
      if (restoreTo !== 'flagged') {
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
    // Held with its siblings — see the file header. The pill itself is live.
    test.skip(!P4D143_LIST_PAYLOAD_LANDED, 'held until this file activates as a whole');
    await page.goto(`${BASE_URL}/salon`);
    await maybeUnlock(page);
    await page.getByRole('link', { name: titleOf('vouched') }).first().click();

    const pill = page.locator('qt-conversation-header .qt-danger-badge');
    await expect(pill).toHaveCount(1, { timeout: 15_000 });
    await expect(pill).toHaveText('Vouched Safe');
    await expect(pill).toHaveAttribute('aria-label', 'Concierge: Vouched Safe');
    // The four native titles are retired as v4 retires them.
    await expect(page.locator('qt-conversation-header .qt-danger-badge[title]')).toHaveCount(0);

    await pill.hover();
    const bubble = page.locator('.qt-tooltip');
    await expect(bubble).toBeVisible({ timeout: 5_000 });
    await expect(bubble).toContainText('Vouched Safe');
    await expect(bubble).toContainText(DETAIL['vouched']);
    await expect(bubble).toContainText(HINT);
    // v4 asks for the bubble BELOW the toolbar; the primitive may still flip it
    // away from a viewport edge, so the attribute is read rather than assumed.
    await expect(bubble).toHaveAttribute('data-placement', 'bottom');
  });
});
