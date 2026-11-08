import { describe, expect, it } from 'vitest';

import { childFolderRows, OWN_TIER, tierLabel, tierRows, type BrowserOrigin } from './folder-view';

/**
 * v4 `lib/folder-browser/view.ts` at the pin `624d0d25c` (`b2b783b47` — the
 * shared view model moved out of `lib/wardrobe/folder-view.ts`, which keeps
 * only `viewTierOf`; v5's `wardrobe/folder-view.spec.ts` keeps that one).
 * These rows pin each rule directly, one rule per case (moved here with the
 * module, `git mv`), plus v4's own `__tests__/unit/lib/folder-browser/
 * view.test.ts` (4 cases) at the end. The row-for-row differential is
 * `item-browser.oracle.spec.ts`.
 */

const PROJECT: BrowserOrigin = { scope: 'project', id: 'p1', name: 'Blackwood' };
const GROUP: BrowserOrigin = { scope: 'group', id: 'g1', name: 'The Sisters' };
const GENERAL: BrowserOrigin = { scope: 'general', id: null, name: 'Quilltap General' };

interface Row {
  id: string;
  characterId?: string | null;
  archivedAt?: string | null;
  folder?: string | null;
  origin?: BrowserOrigin | null;
}

const item = (id: string, extra: Partial<Row> = {}): Row => ({ id, characterId: 'c1', ...extra });

describe('tierLabel (v4 view.ts)', () => {
  it('General for the general scope or a missing origin; "Project: …" / "Group: …" otherwise', () => {
    expect(tierLabel(GENERAL)).toBe('General');
    expect(tierLabel(null)).toBe('General');
    expect(tierLabel(undefined)).toBe('General');
    expect(tierLabel(PROJECT)).toBe('Project: Blackwood');
    expect(tierLabel(GROUP)).toBe('Group: The Sisters');
  });

  it('any other scope reads its name, falling back to `generalName` — `General` by default (a byte shift at the pin)', () => {
    expect(tierLabel({ scope: 'character', id: 'c2', name: 'Bertie' })).toBe('Bertie');
    // v4 `lib/wardrobe/folder-view.ts:75` at `236c04e35` fell back to
    // `Quilltap General`; the shared `tierLabel` at the pin takes a
    // `generalName = 'General'` its callers never pass.
    expect(tierLabel({ scope: 'character', id: 'c2', name: '' })).toBe('General');
    expect(tierLabel({ scope: 'character', id: 'c2', name: '' }, 'Quilltap General')).toBe('Quilltap General');
    // The general scope ignores `generalName` (v4 returns the literal).
    expect(tierLabel(GENERAL, 'Quilltap General')).toBe('General');
  });
});

describe('childFolderRows (v4 view.ts)', () => {
  it('lists only the direct children of the current folder, from items and tree alike', () => {
    const rows = childFolderRows(
      '',
      [item('a', { folder: 'Formal/Evening' }), item('b', { folder: '' })],
      [{ path: 'Rainy days', itemCount: 0 }],
    );
    expect(rows.map((r) => r.path)).toEqual(['Formal', 'Rainy days']);
    expect(childFolderRows('Formal', [item('a', { folder: 'Formal/Evening/Gala' })]).map(
      (r) => r.path,
    )).toEqual(['Formal/Evening']);
  });

  it('never lists the current folder itself, nor a folder outside it', () => {
    const rows = childFolderRows('Formal', [
      item('a', { folder: 'Formal' }),
      item('b', { folder: 'Formalwear' }),
      item('c', { folder: 'Casual/Formal' }),
    ]);
    expect(rows).toEqual([]);
  });

  it('dedupes case-insensitively — the first spelling wins (the tree is read first)', () => {
    const rows = childFolderRows(
      '',
      [item('a', { folder: 'formal' })],
      [{ path: 'Formal', itemCount: 3 }],
    );
    expect(rows).toEqual([{ path: 'Formal', name: 'Formal', itemCount: 3 }]);
  });

  it('counts from the server when it knows the folder, else the live items DIRECTLY inside', () => {
    const items = [
      item('a', { folder: 'Hats' }),
      item('b', { folder: 'Hats', archivedAt: '2026-01-01T00:00:00.000Z' }),
      item('c', { folder: 'Hats/Summer' }),
      item('d', { folder: 'hats' }),
    ];
    // Not in the tree: two live items directly in `Hats` (any casing); the
    // archived one and the nested one don't count.
    expect(childFolderRows('', items)).toEqual([{ path: 'Hats', name: 'Hats', itemCount: 2 }]);
    // In the tree: the server's count, whatever the list holds.
    expect(
      childFolderRows('', items, [{ path: 'hats', itemCount: 7 }]),
    ).toEqual([{ path: 'hats', name: 'hats', itemCount: 7 }]);
  });

  it('a folder the server knows reads its count even when no listed item sits in it', () => {
    expect(
      childFolderRows('', [], [{ path: 'Rainy days', itemCount: 0 }]),
    ).toEqual([{ path: 'Rainy days', name: 'Rainy days', itemCount: 0 }]);
  });

  it('sorts by name case- and accent-insensitively (localeCompare, sensitivity base)', () => {
    const rows = childFolderRows('', [
      item('a', { folder: 'beta' }),
      item('b', { folder: 'Alpha' }),
      item('c', { folder: 'Écharpes' }),
      item('d', { folder: 'delta' }),
    ]);
    expect(rows.map((r) => r.name)).toEqual(['Alpha', 'beta', 'delta', 'Écharpes']);
  });

  it('a null or absent folder is the root and implies no row', () => {
    expect(childFolderRows('', [item('a', { folder: null }), item('b')])).toEqual([]);
  });
});

describe('tierRows (v4 view.ts)', () => {
  // A character view's tierOf (v5's `wardrobe/folder-view.ts` `viewTierOf`):
  // own when the item is the character's, else the origin container's key.
  const tierOf = (i: Row) =>
    i.characterId ? OWN_TIER : i.origin && i.origin.scope !== 'general' ? `${i.origin.scope}:${i.origin.id}` : 'general:';

  it('skips the own tier and keeps first-seen order — the pool’s own order, nearest first', () => {
    const rows = tierRows(
      [
        item('own'),
        item('g', { characterId: null, origin: GROUP }),
        item('p', { characterId: null, origin: PROJECT }),
        item('x', { characterId: null, origin: GENERAL }),
        item('g2', { characterId: null, origin: GROUP }),
      ],
      tierOf,
    );
    expect(rows).toEqual([
      { key: 'group:g1', label: 'Group: The Sisters', scope: 'group', itemCount: 2 },
      { key: 'project:p1', label: 'Project: Blackwood', scope: 'project', itemCount: 1 },
      { key: 'general:', label: 'General', scope: 'general', itemCount: 1 },
    ]);
  });

  it('an archived item opens or joins a row but is never counted', () => {
    const rows = tierRows(
      [
        item('a', { characterId: null, origin: PROJECT, archivedAt: '2026-01-01T00:00:00.000Z' }),
        item('b', { characterId: null, origin: PROJECT }),
      ],
      tierOf,
    );
    expect(rows).toEqual([
      { key: 'project:p1', label: 'Project: Blackwood', scope: 'project', itemCount: 1 },
    ]);
    expect(
      tierRows(
        [item('a', { characterId: null, origin: GROUP, archivedAt: '2026-01-01T00:00:00.000Z' })],
        tierOf,
      ),
    ).toEqual([{ key: 'group:g1', label: 'Group: The Sisters', scope: 'group', itemCount: 0 }]);
  });

  it('an origin-less borrowed item opens a General row with the general scope fallback', () => {
    expect(tierRows([item('a', { characterId: null })], tierOf)).toEqual([
      { key: 'general:', label: 'General', scope: 'general', itemCount: 1 },
    ]);
  });
});

// ---- v4 `__tests__/unit/lib/folder-browser/view.test.ts` (transcribed) ------

describe('v4 view.test.ts — childFolderRows', () => {
  it('unions the tree with item folders, one row per direct child', () => {
    const rows = childFolderRows(
      '',
      [{ folder: 'formal/Evening' }, { folder: 'Casual' }, { folder: '' }],
      [
        { path: 'Formal', itemCount: 4 },
        { path: 'Empty', itemCount: 0 },
      ],
    );
    expect(rows.map((r) => [r.path, r.itemCount])).toEqual([
      ['Casual', 1],
      ['Empty', 0],
      ['Formal', 4],
    ]);
  });

  it('counts only live items directly inside when the server does not know the folder', () => {
    const rows = childFolderRows('Formal', [
      { folder: 'Formal/Evening' },
      { folder: 'Formal/Evening', archivedAt: '2026-01-01' },
      { folder: 'Formal/Evening/Gala' },
    ]);
    expect(rows).toEqual([{ path: 'Formal/Evening', name: 'Evening', itemCount: 1 }]);
  });
});

describe('v4 view.test.ts — tierRows', () => {
  const QUEST = { scope: 'project' as const, id: 'p1', name: 'Quest' };
  const GEN = { scope: 'general' as const, id: null, name: 'Quilltap General' };

  it('skips the own tier and counts live borrowed items in first-seen order', () => {
    const items = [
      { id: 'a', origin: QUEST },
      { id: 'b', origin: GEN },
      { id: 'c', origin: QUEST, archivedAt: '2026-01-01' },
      { id: 'd', origin: QUEST },
      { id: 'e' },
    ] as Array<{ id: string; origin?: BrowserOrigin; archivedAt?: string }>;
    const tierOf = (i: (typeof items)[number]) => (i.origin ? `${i.origin.scope}:${i.origin.id ?? ''}` : OWN_TIER);
    expect(tierRows(items, tierOf)).toEqual([
      { key: 'project:p1', label: 'Project: Quest', scope: 'project', itemCount: 2 },
      { key: 'general:', label: 'General', scope: 'general', itemCount: 1 },
    ]);
  });

  it('labels each scope', () => {
    expect(tierLabel({ scope: 'group', id: 'g', name: 'Crew' })).toBe('Group: Crew');
    expect(tierLabel(null)).toBe('General');
  });
});
