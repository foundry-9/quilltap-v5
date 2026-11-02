import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';

import AxeBuilder from '@axe-core/playwright';

import { expect, test, type Page } from '../support/fixtures';
import { E2E_PASSPHRASE } from '../support/env';
import { openSidebarSection } from '../support/sidebar';

/**
 * P4.D358 Tier 2.1 — the live accessibility judge. axe-core (`@axe-core/
 * playwright`) over a FIXED page list × the six bundled theme packs × light /
 * dark, against the SHARED global-setup server (read-only pages; R-11). Runs
 * only under the optional `a11y` Playwright project:
 *
 *     npx playwright test --project=a11y
 *
 * Every violation is keyed `pack|mode|page|rule|target` and diffed against the
 * committed `known-violations.json`; anything NOT in the file fails. Each
 * known row carries an `owner` (the lane or rule that will clear it), so none
 * is anonymous. `QT_AXE_REBASELINE=1` rewrites the file from the current run
 * (the unifier re-baselines on the union, §S.4 — siblings' new UI adds nodes).
 * `QT_AXE_OUT=<dir>` writes the raw per-page results there.
 */

const PACKS: ReadonlyArray<{ id: string; button: RegExp }> = [
  { id: 'art-deco', button: /Art Deco/ },
  { id: 'earl-grey', button: /Earl Grey/ },
  { id: 'great-estate', button: /Great Estate/ },
  { id: 'madmans-box', button: /Madman's Box/ },
  { id: 'old-school', button: /Old School/ },
  { id: 'rains', button: /Rains/ },
];
const MODES = ['light', 'dark'] as const;
const TAGS = ['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa'];

const KNOWN_FILE = resolve(__dirname, 'known-violations.json');

interface KnownRow {
  key: string;
  owner: string;
}

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

async function choosePack(page: Page, button: RegExp, id: string): Promise<void> {
  await page.goto('/settings');
  await page.getByRole('button', { name: 'Appearance' }).click();
  await page.getByRole('button', { name: button }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', id);
}

/** The fixed page list: name → how to reach it (each leaves the page settled). */
const PAGES: ReadonlyArray<{ name: string; open: (page: Page) => Promise<void> }> = [
  {
    name: 'home',
    open: async (page) => {
      await page.goto('/');
      await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
    },
  },
  {
    name: 'salon',
    open: async (page) => {
      await page.goto('/salon');
      const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: 'Solo Voyage' });
      await card.click();
      await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
    },
  },
  {
    name: 'rename-dialog',
    open: async (page) => {
      await openSidebarSection(page, 'Organize');
      await page.getByRole('button', { name: 'Rename' }).click();
      await expect(page.getByRole('dialog').getByText('Rename Chat')).toBeVisible({ timeout: 10_000 });
    },
  },
  ...(['Chat', 'Appearance', 'System'] as const).map((tab) => ({
    name: `settings-${tab.toLowerCase()}`,
    open: async (page: Page) => {
      await page.goto('/settings');
      await page.getByRole('button', { name: tab }).click();
      await expect(page.getByRole('heading', { name: 'Settings', exact: true })).toBeVisible();
    },
  })),
  {
    name: 'characters',
    open: async (page) => {
      await page.goto('/characters');
      await expect(page.locator('main')).toBeVisible();
    },
  },
];

test.describe('P4.D358 — the shell skip link (v4 app-layout.tsx:113, :116)', () => {
  test('the first Tab stop is "Skip to content", and Enter lands on <main>', async ({ page }) => {
    await page.goto('/');
    await maybeUnlock(page);
    await page.goto('/');
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
    await page.locator('body').focus();
    await page.keyboard.press('Tab');
    await expect(page.locator(':focus')).toHaveText('Skip to content');
    await page.keyboard.press('Enter');
    await expect
      .poll(() => page.evaluate(() => document.activeElement?.id))
      .toBe('main-content');
  });
});

test.describe('P4.D358 — axe sweep (WCAG 2.2 AA, every bundled pack, light and dark)', () => {
  test('no violation outside known-violations.json', async ({ page }) => {
    test.setTimeout(30 * 60_000);
    const known: KnownRow[] = JSON.parse(readFileSync(KNOWN_FILE, 'utf8'));
    const knownKeys = new Set(known.map((r) => r.key));
    const seen: string[] = [];
    const outDir = process.env['QT_AXE_OUT'];
    if (outDir) mkdirSync(outDir, { recursive: true });

    await page.goto('/');
    await maybeUnlock(page);

    try {
      for (const pack of PACKS) {
        await choosePack(page, pack.button, pack.id);
        for (const mode of MODES) {
          await page.emulateMedia({ colorScheme: mode });
          for (const p of PAGES) {
            await p.open(page);
            const results = await new AxeBuilder({ page }).withTags(TAGS).analyze();
            if (outDir) {
              writeFileSync(
                resolve(outDir, `${pack.id}.${mode}.${p.name}.json`),
                JSON.stringify(results.violations, null, 2),
              );
            }
            for (const v of results.violations) {
              for (const node of v.nodes) {
                seen.push(`${pack.id}|${mode}|${p.name}|${v.id}|${node.target.join(' ')}`);
              }
            }
          }
        }
      }
    } finally {
      // Leave the shared server on the default pack for every later spec.
      await page.goto('/settings');
      await page.getByRole('button', { name: 'Appearance' }).click();
      await page.getByRole('button', { name: /Default|Quilltap/ }).first().click().catch(() => undefined);
    }

    const unique = [...new Set(seen)].sort();
    if (process.env['QT_AXE_REBASELINE'] === '1') {
      const prior = new Map(known.map((r) => [r.key, r.owner]));
      const rows = unique.map((key) => ({
        key,
        owner: prior.get(key) ?? (key.split('|')[3] === 'color-contrast' ? 'P4.D357' : 'UNOWNED'),
      }));
      writeFileSync(KNOWN_FILE, JSON.stringify(rows, null, 2) + '\n');
      return;
    }
    const fresh = unique.filter((k) => !knownKeys.has(k));
    expect(fresh, `axe violations not in known-violations.json:\n${fresh.join('\n')}`).toEqual([]);
  });
});
