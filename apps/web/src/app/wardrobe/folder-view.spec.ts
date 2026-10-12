import { describe, expect, it } from 'vitest';

import { childFolderRows, OWN_TIER, tierLabel, tierRows, viewTierOf } from './folder-view';
import type { WardrobeOrigin } from './wardrobe-container';

/**
 * v4 `lib/wardrobe/folder-view.ts` (`4a38222e6`) has no test of its own — the
 * browser test (`wardrobe-browser.folders.test.tsx`) exercises it through the
 * DOM. These rows pin each of its rules directly, one rule per case, so a
 * drift shows here before it shows as a mis-drawn browser.
 */

const PROJECT: WardrobeOrigin = { scope: 'project', id: 'p1', name: 'Blackwood' };
const GROUP: WardrobeOrigin = { scope: 'group', id: 'g1', name: 'The Sisters' };
const GENERAL: WardrobeOrigin = { scope: 'general', id: null, name: 'Quilltap General' };

interface Row {
  id: string;
  characterId?: string | null;
  archivedAt?: string | null;
  folder?: string | null;
  origin?: WardrobeOrigin | null;
}

const item = (id: string, extra: Partial<Row> = {}): Row => ({ id, characterId: 'c1', ...extra });

describe('viewTierOf (v4 :65-68)', () => {
  it('outside a character view every item is in the own tier, manageable or not', () => {
    expect(viewTierOf(item('a', { origin: PROJECT }), false, false)).toBe(OWN_TIER);
    expect(viewTierOf(item('a'), true, false)).toBe(OWN_TIER);
  });

  it('in a character view a manageable item is own; a borrowed one is keyed by its origin container', () => {
    expect(viewTierOf(item('a', { origin: PROJECT }), true, true)).toBe(OWN_TIER);
    expect(viewTierOf(item('a', { characterId: null, origin: PROJECT }), false, true)).toBe(
      'project:p1',
    );
    expect(viewTierOf(item('a', { characterId: null, origin: GROUP }), false, true)).toBe(
      'group:g1',
    );
    // An origin-less borrowed item is taken for a General archetype.
    expect(viewTierOf(item('a', { characterId: null }), false, true)).toBe('general:');
  });
});

describe('tierLabel (v4 :71-76)', () => {
  it('General for the general scope or a missing origin; "Project: …" / "Group: …" otherwise', () => {
    expect(tierLabel(GENERAL)).toBe('General');
    expect(tierLabel(null)).toBe('General');
    expect(tierLabel(undefined)).toBe('General');
    expect(tierLabel(PROJECT)).toBe('Project: Blackwood');
    expect(tierLabel(GROUP)).toBe('Group: The Sisters');
  });

  it('any other scope reads its name, falling back to Quilltap General', () => {
    expect(tierLabel({ scope: 'character', id: 'c2', name: 'Bertie' })).toBe('Bertie');
    expect(tierLabel({ scope: 'character', id: 'c2', name: '' })).toBe('Quilltap General');
  });
});

describe('childFolderRows (v4 :92-120)', () => {
  it('lists only the direct children of the current folder, from items and tree alike', () => {
    const rows = childFolderRows(
      '',
      [item('a', { folder: 'Formal/Evening' }), item('b', { folder: '' })],
      [{ path: 'Rainy days', name: 'Rainy days', itemCount: 0 }],
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
      [{ path: 'Formal', name: 'Formal', itemCount: 3 }],
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
      childFolderRows('', items, [{ path: 'hats', name: 'hats', itemCount: 7 }]),
    ).toEqual([{ path: 'hats', name: 'hats', itemCount: 7 }]);
  });

  it('a folder the server knows reads its count even when no listed item sits in it', () => {
    expect(
      childFolderRows('', [], [{ path: 'Rainy days', name: 'Rainy days', itemCount: 0 }]),
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

describe('tierRows (v4 :127-149)', () => {
  const tierOf = (i: Row) => viewTierOf(i, Boolean(i.characterId), true);

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
