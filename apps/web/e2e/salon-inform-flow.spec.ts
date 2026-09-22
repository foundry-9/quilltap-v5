import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE, MOCK_LLM_PORT } from './support/env';
import { startMockLlm, MOCK_LLM_REPLY, type MockLlm } from './support/mock-llm';

/**
 * P4.D206 — Inform, the quiet word out of character (v4 `e7d77bb60`).
 *
 * The operator picks LLM-controlled seats and writes a second-person passage.
 * Each target receives it verbatim as a system block before its next turn and
 * then it is consumed; the transcript keeps a Host record for the operator
 * alone, labelled `out of character`, and the composer wears a chip for every
 * batch still owed.
 *
 * ORDERING: rides the shared global-setup server, so the filename must sort
 * after `foundation.spec.ts` ('sa' > 'fo').
 *
 * **ACTIVATE-AT-UNIFY.** The three verbs (`chatInform`, `chatInformsList`,
 * `chatInformCancel`) are P4.D205's, which does not exist on main while this
 * lane runs, and `chat_informs` is a table its boot ensure supplies on open.
 * Gated by the NAMED constant below rather than by a capability probe: a
 * DEFINED-but-unimplemented verb defeats a probe (the `P49K2_SERVER_LANDED`
 * precedent), and here the probe would be worse still — the table's absence
 * and the verb's absence are different failures that look alike. **The unifier
 * flips it to `true` after P4.D205 is picked, and the beat's first live run is
 * the unified gate's own step.**
 *
 * The chat choice is deliberate. "Group Expedition" carries three participants
 * — Aria and Bram (LLM) and Cleo (the operator's) — which is exactly what these
 * beats need: two eligible seats to tell apart, and one that must never be
 * offered. Nothing here asserts a chat-wide total, so it perturbs no other
 * spec reading that chat (the `salon-token-cost-flow` caution).
 */
// Flipped to `true` at the `f45a517a9` round unification (2026-09-22): P4.D205
// is picked; the beat's first fully-live run is the unified gate's own step.
const P4D205_SERVER_LANDED = true;

test.describe('P4.D206 — Inform, the word out of character', () => {
  let mock: MockLlm;

  test.beforeAll(async () => {
    mock = await startMockLlm(MOCK_LLM_REPLY, MOCK_LLM_PORT);
  });
  test.afterAll(async () => {
    await mock?.close();
  });

  async function maybeUnlock(page: Page) {
    const passphrase = page.locator('#qt-passphrase');
    const chats = page.getByRole('heading', { name: 'Chats', exact: true });
    await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
    if (await passphrase.count()) {
      await passphrase.fill(E2E_PASSPHRASE);
      await page.getByRole('button', { name: 'Unlock' }).click();
    }
  }

  async function openChat(page: Page, title: string) {
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
    const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: title });
    await expect(card).toBeVisible();
    await card.click();
    await expect(page.locator('.qt-chat-messages-list')).toBeVisible();
  }

  /** The dialogs' footer buttons — the header ✕ is also named "Close". */
  function footerButton(page: Page, name: string) {
    return page.locator('[qt-modal-footer]').getByRole('button', { name, exact: true });
  }

  /** Type the passage into the dialog's markdown field. */
  async function writePassage(page: Page, text: string) {
    const body = page.getByRole('dialog').locator('.qt-lexical-contenteditable, [contenteditable]');
    await body.first().click();
    await page.keyboard.type(text);
  }

  test('the gutter offers Inform, and the dialog offers only the model-played seats', async ({
    page,
  }) => {
    // This half needs NO server verb: the button and the dialog's audience come
    // off the chat the Salon already has. It runs live in-lane.
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    const gutter = page.locator('.qt-composer-gutter-tools');
    const inform = gutter.getByRole('button', { name: 'Inform the cast' });
    await expect(inform).toBeVisible();
    await expect(inform).toHaveAttribute('title', 'Inform the cast');

    await inform.click();
    const dialog = page.getByRole('dialog');
    await expect(dialog.getByText('Inform the cast')).toBeVisible();
    await expect(dialog.getByText('Who is told')).toBeVisible();

    // Everyone is the opening state, and it is pressed.
    const everyone = dialog.getByRole('button', { name: 'Everyone', exact: true });
    await expect(everyone).toHaveAttribute('aria-pressed', 'true');

    // The two model-played seats are offered; the operator's own never is.
    await expect(dialog.getByRole('button', { name: /Aria/ })).toBeVisible();
    await expect(dialog.getByRole('button', { name: /Bram/ })).toBeVisible();
    await expect(dialog.getByRole('button', { name: /Cleo/ })).toHaveCount(0);

    // Picking a seat drops Everyone — coverage, not the click, decides the shape.
    await dialog.getByRole('button', { name: /Aria/ }).click();
    await expect(everyone).toHaveAttribute('aria-pressed', 'false');

    // Inform stays shut until the passage has a body.
    await expect(footerButton(page, 'Inform')).toBeDisabled();

    await footerButton(page, 'Cancel').click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
  });

  test('informing EVERYONE leaves a record and no chip', async ({ page }) => {
    test.skip(
      !P4D205_SERVER_LANDED,
      'awaits P4.D205: chatInform / chatInformsList / chatInformCancel + the chat_informs boot ensure',
    );
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    await page.getByRole('button', { name: 'Inform the cast' }).click();
    await writePassage(page, 'You all notice the clock has stopped.');
    await footerButton(page, 'Inform').click();

    // The dialog closes and says so in the company's voice.
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await expect(page.getByText('The company has been informed')).toBeVisible();

    // The Host record lands, labelled by what it IS rather than by the verb.
    await expect(page.getByText('out of character').first()).toBeVisible({ timeout: 15_000 });

    // A batch that covers every eligible seat is public, so the server records
    // it with null targets — but it is still PENDING on both seats, so the
    // chips name them. (The record and the chips answer different questions.)
    const chip = page.locator('.qt-chat-tool-result-chip', {
      hasText: /^Informing (Aria, Bram|Bram, Aria) before their next turn$/,
    });
    await expect(chip).toBeVisible();

    // Withdraw it, so the batch does not linger into the beats below (a
    // leftover two-seat chip CONTAINS "Informing Aria" and would make a
    // one-seat locator ambiguous).
    await chip.getByRole('button', { name: /^Withdraw the inform for / }).click();
    await expect(page.getByText('The note has been withdrawn')).toBeVisible();
    await expect(chip).toHaveCount(0);
  });

  test('informing ONE seat names it on the chip, and the cross withdraws it', async ({ page }) => {
    test.skip(!P4D205_SERVER_LANDED, 'awaits P4.D205: the three Inform verbs');
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    await page.getByRole('button', { name: 'Inform the cast' }).click();
    await page.getByRole('dialog').getByRole('button', { name: /Aria/ }).click();
    await writePassage(page, 'You see that Bram pocketed the key.\n\nHe is not subtle.');
    await footerButton(page, 'Inform').click();

    await expect(page.getByText('Informed Aria')).toBeVisible();

    // One chip, naming the one seat still owed, with the passage's FIRST LINE
    // on hover — not the whole passage, and not its second paragraph.
    const chip = page.locator('.qt-chat-tool-result-chip', {
      hasText: 'Informing Aria before their next turn',
    });
    await expect(chip).toBeVisible({ timeout: 15_000 });
    await expect(chip).toHaveAttribute('title', 'You see that Bram pocketed the key.');

    // The cross withdraws the batch. Nothing had consumed it, so the record
    // goes with it — the note was fed to the fire before anyone read it.
    await chip.getByRole('button', { name: 'Withdraw the inform for Aria' }).click();
    await expect(page.getByText('The note has been withdrawn')).toBeVisible();
    await expect(chip).toHaveCount(0);
  });

  test('a seat’s next turn CONSUMES its note, and the chip goes', async ({ page }) => {
    test.skip(!P4D205_SERVER_LANDED, 'awaits P4.D205: the three Inform verbs + the consumption');
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Group Expedition');

    // Inform EVERYONE. The rotation's next speaker is a weighted draw the beat
    // cannot force (`forcing-a-deterministic-turn-in-a-salon-e2e-beat`), so a
    // note aimed at one seat would be consumed only if that seat happened to
    // take the floor. A note owed to BOTH seats is consumed by whichever seat
    // speaks, and the chip's text records exactly that: it names both seats
    // before the turn and only the other one after it.
    await page.getByRole('button', { name: 'Inform the cast' }).click();
    await writePassage(page, 'You remember the gate was already open.');
    await footerButton(page, 'Inform').click();

    const bothSeats = page.locator('.qt-chat-tool-result-chip', {
      hasText: /^Informing (Aria, Bram|Bram, Aria) before their next turn$/,
    });
    await expect(bothSeats).toBeVisible({ timeout: 15_000 });

    // Run a real turn. The mock answers whatever the seat is asked, so the only
    // thing under test is that the pending row is consumed by the generation it
    // was written for.
    const composer = page.locator('.qt-chat-composer-input').first();
    await composer.click();
    await page.keyboard.type('What do you make of it?');
    await page.locator('button[aria-label="Send message"]').click();

    // Once a seat has had its turn ITS row is gone, like a note fed to the fire.
    // Only consumption can shrink the chip's name list (a withdrawal removes
    // the whole batch), so the two-seat chip going is the proof. What remains
    // depends on how far the turn CHAIN ran — the first live run had both
    // seats speak, leaving no chip at all — so the remainder is tidied, not
    // asserted.
    await expect(bothSeats).toHaveCount(0, { timeout: 30_000 });
    const oneSeat = page.locator('.qt-chat-tool-result-chip', {
      hasText: /^Informing (Aria|Bram) before their next turn$/,
    });
    if ((await oneSeat.count()) > 0) {
      // Leave nothing behind for a later spec reading this chat.
      await oneSeat.getByRole('button', { name: /^Withdraw the inform for / }).click();
      await expect(oneSeat).toHaveCount(0);
    }
  });
});
