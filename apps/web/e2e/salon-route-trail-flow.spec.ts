import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';
import { openSidebarSection } from './support/sidebar';
import { startMockLlm, MOCK_LLM_REPLY, type MockLlm } from './support/mock-llm';

/**
 * P4.D177 — the message route trail, live (v4 `5841a8c62`).
 *
 * ACTIVATE-AT-UNIFY behind {@link P4D173_SERVER_LANDED}: the server half
 * (`chat_messages.routeTrail`, the failover writer, the SSE `done`/chat-GET
 * carry) is P4.D171/P4.D173's. Named constant per §C.6 — never a capability
 * probe, since a DEFINED-but-refusing verb would defeat one.
 *
 * ## The purpose-built failing seat
 *
 * A genuine failover needs a connection profile that genuinely fails, with a
 * genuine understudy behind it — the same live-wire discipline
 * `salon-chain-pause-toast-flow.spec.ts` and the P4.D135 understudy round-trip
 * use elsewhere in this suite (drive the REAL thing rather than stub a
 * component). Two NEW connection profiles are created through Settings for
 * this beat alone (`OPENAI_COMPATIBLE`, distinctive names so they cannot be
 * mistaken for the seeded fixture profiles other specs enumerate by position):
 * one whose `baseUrl` points at `http://127.0.0.1:1` — a privileged port
 * nothing in this sandbox can bind, so every request fails at connect, the
 * `network` trigger class — with its fallback set to the second, which points
 * at the real in-process mock LLM. A brand-new chat (never the shared
 * `Group Expedition`/`Solo Voyage` fixture rows the rest of the suite
 * enumerates by exact count) is created, then walked through Add Character so
 * the failing-then-understudy profile can be assigned explicitly — New Chat's
 * own picker only ever auto-seeds a character's OWN default profile.
 */

const P4D173_SERVER_LANDED = false;

async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const chats = page.getByRole('heading', { name: 'Chats', exact: true });
  await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

async function openProfilesCard(page: Page): Promise<void> {
  await page.goto('/settings?tab=providers&section=connection-profiles');
  await expect(page.getByRole('button', { name: '+ Add Profile' })).toBeVisible({
    timeout: 15_000,
  });
}

/**
 * Creates one `OPENAI_COMPATIBLE` connection profile and returns its full
 * option label (`"<name> (OPENAI_COMPATIBLE: <model>)"`) — the exact text
 * `qt-add-character-profile`'s `<option>` renders, per its template
 * (`profile.name ({{ profile.provider }}: {{ profile.modelName }})`).
 */
async function createProfile(page: Page, name: string, baseUrl: string, model: string): Promise<string> {
  await openProfilesCard(page);
  await page.getByRole('button', { name: '+ Add Profile' }).click();
  await expect(page.locator('#qt-pf-provider')).toBeVisible({ timeout: 15_000 });
  await page.locator('#qt-pf-provider').selectOption('OPENAI_COMPATIBLE');
  await page.locator('#qt-pf-name').fill(name);
  await page.locator('#qt-pf-baseurl').fill(baseUrl);
  await page.locator('#qt-pf-model').fill(model);
  await page.getByRole('button', { name: 'Create Profile' }).click();
  await expect(page.locator('#qt-pf-provider')).toHaveCount(0, { timeout: 15_000 });
  return `${name} (OPENAI_COMPATIBLE: ${model})`;
}

test.describe('P4.D177 — the route trail, live', () => {
  test('a real failover writes a trail the badge renders and a reload reads back', async ({
    page,
  }, testInfo) => {
    test.skip(
      !P4D173_SERVER_LANDED,
      'awaits P4.D171/P4.D173: the routeTrail column, the failover writer, and the SSE/chat-GET carry.',
    );
    test.setTimeout(60_000);

    let mock: MockLlm | undefined;
    try {
      mock = await startMockLlm(MOCK_LLM_REPLY);

      await page.goto('/salon');
      await maybeUnlock(page);

      // Two purpose-built profiles: the primary that always fails to connect,
      // the understudy that answers for real.
      const understudyLabel = await createProfile(
        page,
        'P4D177 Route Trail Understudy',
        mock.url,
        'mock-model',
      );
      await openProfilesCard(page);
      await page.getByRole('button', { name: '+ Add Profile' }).click();
      await expect(page.locator('#qt-pf-provider')).toBeVisible({ timeout: 15_000 });
      await page.locator('#qt-pf-provider').selectOption('OPENAI_COMPATIBLE');
      await page.locator('#qt-pf-name').fill('P4D177 Route Trail Primary');
      await page.locator('#qt-pf-baseurl').fill('http://127.0.0.1:1');
      await page.locator('#qt-pf-model').fill('mock-model');
      const understudySelect = page.locator('#qt-pf-fallback');
      await understudySelect.selectOption({ label: understudyLabel });
      await page.getByRole('button', { name: 'Create Profile' }).click();
      await expect(page.locator('#qt-pf-provider')).toHaveCount(0, { timeout: 15_000 });
      const primaryLabel = 'P4D177 Route Trail Primary (OPENAI_COMPATIBLE: mock-model)';

      // A fresh chat — never the shared fixture rows.
      await page.goto('/salon');
      await page.getByRole('link', { name: 'New Chat' }).first().click();
      await expect(page.getByRole('heading', { name: 'New Chat', exact: true })).toBeVisible();
      await page.locator('.new-chat-character-picker button').first().click();
      await expect(page.getByText('Speaks First')).toBeVisible();
      const create = page.getByRole('button', { name: 'Create Chat' });
      await expect(create).toBeEnabled();
      await create.click();
      await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 20_000 });

      // Add a second character with the FAILING (→ understudy) profile, then
      // remove whoever New Chat seeded — the added seat is the sole active LLM
      // participant, so every turn routes to it deterministically.
      await openSidebarSection(page, 'Participants');
      const castNames = page.locator('qt-chat-sidebar .qt-participant-card-name');
      const seededName = ((await castNames.first().textContent()) ?? '').trim();

      await page.getByRole('button', { name: 'Add Character', exact: true }).click();
      const dialog = page.getByRole('dialog');
      await expect(dialog.getByText('Add Character to Chat')).toBeVisible();
      const tiles = dialog
        .locator('.grid button')
        .filter({ hasNotText: 'Create New NPC' })
        .filter({ hasNotText: 'Summon from Lore' });
      await expect(tiles.first()).toBeVisible({ timeout: 15_000 });
      const joinerName = ((await tiles.first().locator('.font-semibold').first().textContent()) ?? '').trim();
      await tiles.first().click();
      await expect(dialog.locator('#qt-add-character-profile')).toBeVisible();
      await dialog.locator('#qt-add-character-profile').selectOption({ label: primaryLabel });
      const add = dialog.getByRole('button', { name: 'Add Character', exact: true });
      await expect(add).toBeEnabled();
      await add.click();
      await expect(page.getByRole('dialog')).toHaveCount(0, { timeout: 15_000 });
      await expect(castNames.filter({ hasText: joinerName })).toHaveCount(1, { timeout: 15_000 });

      await page.getByRole('button', { name: `Remove ${seededName} from chat` }).click();
      await page.getByRole('dialog').getByRole('button', { name: 'Remove', exact: true }).click();
      await expect(page.getByRole('dialog')).toHaveCount(0, { timeout: 15_000 });

      // Send — the primary fails to connect, the engine falls over to the
      // understudy, and the reply streams for real.
      const composer = page.locator('.qt-chat-composer-input .qt-rich-editor-content');
      await composer.click();
      await page.keyboard.type('Good morning.');
      await page.keyboard.press('Enter');
      await expect(page.getByText(MOCK_LLM_REPLY).first()).toBeVisible({ timeout: 30_000 });

      // The badge: aria-label list, the failed row's ❌ glyph, its hover text.
      const list = page.locator('[aria-label="Models tried for this reply"]').last();
      await expect(list).toBeVisible({ timeout: 15_000 });
      await expect(list.locator('li')).toHaveCount(2);
      await expect(list.locator('[role="img"]', { hasText: '❌' })).toHaveCount(1);
      const failedTitle = await list.locator('[title*="fell over"]').getAttribute('title');
      expect(failedTitle).toContain('P4D177 Route Trail Primary');

      // A reload reads the SAME trail off the chat GET's message projection.
      await page.reload();
      await expect(page.getByText(MOCK_LLM_REPLY).first()).toBeVisible({ timeout: 15_000 });
      const reloadedList = page.locator('[aria-label="Models tried for this reply"]').last();
      await expect(reloadedList).toBeVisible({ timeout: 15_000 });
      await expect(reloadedList.locator('li')).toHaveCount(2);
      await expect(reloadedList.locator('[role="img"]', { hasText: '❌' })).toHaveCount(1);
    } finally {
      await mock?.close();
    }
  });
});
