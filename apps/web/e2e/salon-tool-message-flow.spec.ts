import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: rides the SHARED global-setup server and unlocks it, so the filename
 * sorts AFTER foundation.spec.ts (workers: 1, alphabetical). "salon-tool-message
 * -flow" sorts after "foundation".
 *
 * P4.17 — a LIVE browser walk of the tool-result card (`qt-tool-message`) over
 * two seeded `role:'TOOL'` rows on Solo Voyage (`global-setup.ts`):
 *
 *  1. A character-initiated rng run folded into Lorian's bubble → an EMBEDDED
 *     card. Walk: collapsed by default → expand request → expand response →
 *     Success badge. This is the raw-JSON-whisper regression the dogfood finding
 *     named: the row must render as a card, never as `{"toolName":"rng",...}`.
 *  2. A user-initiated Prospero run → a collapsed announcement chip that expands
 *     to the standalone card, attributed "Charles ran".
 */

/** Unlock only when the passphrase screen is showing (the shared server stays unlocked). */
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

async function openSoloVoyage(page: Page): Promise<void> {
  await page.goto('/salon');
  await page.getByRole('link', { name: 'Solo Voyage' }).first().click();
  await expect(page.getByText('Once, above the clouds...')).toBeVisible({ timeout: 15_000 });
}

test.describe('P4.17 — the tool-result card', () => {
  test('a character-initiated tool run renders as a collapsible card, not raw JSON', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openSoloVoyage(page);

    // The rng run folded into Lorian's last bubble renders as an embedded card.
    const card = page.locator('qt-tool-message').filter({ hasText: 'Random Number Generator' });
    await expect(card).toBeVisible({ timeout: 15_000 });

    // The Success badge is on the card.
    await expect(card.locator('.qt-badge-success')).toHaveText('Success');

    // Collapsed by default — neither section's <pre> body is present.
    await expect(card.locator('pre')).toHaveCount(0);
    // ...and the raw envelope never leaks into the flow as prose.
    await expect(page.getByText('"toolName":"rng"')).toHaveCount(0);

    // Expand the request → the prompt appears.
    await card.getByRole('button', { name: 'Tool Request' }).click();
    await expect(card.locator('pre').filter({ hasText: '1d20' })).toBeVisible();

    // Expand the response → the result appears.
    await card.getByRole('button', { name: 'Tool Response' }).click();
    await expect(card.locator('.tool-response-content')).toContainText('Rolled 1d20: [17]');
  });

  test('a user-initiated Prospero run expands from a chip to an attributed card', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openSoloVoyage(page);

    // The Prospero run is a collapsed announcement chip.
    const chip = page.locator('.qt-chat-announcement-chip', { hasText: 'Prospero' });
    await expect(chip).toBeVisible({ timeout: 15_000 });

    // Expanding it reveals the standalone tool card, attributed to the operator.
    await chip.click();
    const card = page.locator('qt-tool-message').filter({ hasText: 'Charles ran' });
    await expect(card).toBeVisible();
    await expect(card.locator('.qt-badge-success')).toHaveText('Success');

    await card.getByRole('button', { name: 'Tool Response' }).click();
    await expect(card.locator('.tool-response-content')).toContainText('Found 3 references.');
  });
});
