import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { WardrobeItemDto } from '../core/core-contract';
import { WardrobeItemRow } from './wardrobe-item-row';

function dto(partial: Partial<WardrobeItemDto> & { id: string }): WardrobeItemDto {
  return {
    characterId: null,
    title: partial.id,
    types: ['top'],
    componentItemIds: [],
    isDefault: false,
    replace: false,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...partial,
  } as WardrobeItemDto;
}

async function render(
  item: WardrobeItemDto,
  canManage?: (i: WardrobeItemDto) => boolean,
  allItems: WardrobeItemDto[] = [],
  opts: { canArchive?: boolean } = {},
): Promise<ComponentFixture<WardrobeItemRow>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [WardrobeItemRow] });
  const fixture = TestBed.createComponent(WardrobeItemRow);
  fixture.componentRef.setInput('item', item);
  fixture.componentRef.setInput('allItems', allItems);
  fixture.componentRef.setInput('inChat', false);
  if (canManage) fixture.componentRef.setInput('canManage', canManage);
  if (opts.canArchive) fixture.componentRef.setInput('canArchive', true);
  fixture.detectChanges();
  return fixture;
}

/** Open the kebab and return the menu item labels, in order. */
function kebabLabels(fixture: ComponentFixture<WardrobeItemRow>): string[] {
  const host = fixture.nativeElement as HTMLElement;
  const kebab = Array.from(host.querySelectorAll('button')).find(
    (b) => b.getAttribute('aria-label') === 'More actions',
  ) as HTMLButtonElement;
  kebab.click();
  fixture.detectChanges();
  return Array.from(host.querySelectorAll('[role="menu"] button')).map((b) =>
    (b.textContent ?? '').trim(),
  );
}

/**
 * v4 `wardrobe-item-row.tsx` at `d7263f39`: the `isShared = !item.characterId`
 * rule became the optional `canManage` predicate. Full kebab iff manageable;
 * otherwise Move/Copy only plus the `· shared` badge.
 */
describe('WardrobeItemRow canManage (v4 :36-40, :83-85, :199, :291, :363)', () => {
  it('falls back to the character-view rule when no predicate is passed (v4 :83-85)', async () => {
    const owned = await render(dto({ id: 'i1', characterId: 'c1' }));
    expect(kebabLabels(owned)).toEqual([
      'Edit',
      '★ Mark as default outfit item',
      'Duplicate',
      'Move',
      'Copy',
      'Delete',
    ]);
    expect((owned.nativeElement as HTMLElement).textContent).not.toContain('· shared');

    const shared = await render(dto({ id: 'i2', characterId: null }));
    expect(kebabLabels(shared)).toEqual(['Move', 'Copy']);
    expect((shared.nativeElement as HTMLElement).textContent).toContain('· shared');
  });

  it('an explicit predicate OVERRIDES the character rule in both directions', async () => {
    // A shared-tier item (characterId null) browsed in its own container is
    // fully manageable — the case the container browser exists for.
    const inContainer = await render(dto({ id: 'i1', characterId: null }), () => true);
    const labels = kebabLabels(inContainer);
    expect(labels).toContain('Edit');
    expect(labels).toContain('Delete');
    expect((inContainer.nativeElement as HTMLElement).textContent).not.toContain('· shared');

    // A character-owned item the predicate refuses keeps Move/Copy only.
    const refused = await render(dto({ id: 'i2', characterId: 'c1' }), () => false);
    expect(kebabLabels(refused)).toEqual(['Move', 'Copy']);
    expect((refused.nativeElement as HTMLElement).textContent).toContain('· shared');
  });

  it('the predicate is asked about THIS row, so a mixed list badges per item', async () => {
    const local = dto({ id: 'local', characterId: null });
    const merged = dto({ id: 'merged', characterId: null });
    const inList = (i: WardrobeItemDto): boolean => i.id === 'local';
    expect(kebabLabels(await render(local, inList))).toContain('Edit');
    expect(kebabLabels(await render(merged, inList))).toEqual(['Move', 'Copy']);
  });

  it('nested composite components inherit the predicate (v4 :399)', async () => {
    const child = dto({ id: 'child', characterId: null, title: 'Cravat' });
    const parent = dto({
      id: 'parent',
      characterId: null,
      title: 'Evening Kit',
      componentItemIds: ['child'],
    });
    const fixture = await render(parent, () => true, [child]);
    const host = fixture.nativeElement as HTMLElement;
    (host.querySelector('button[aria-label="Expand components"]') as HTMLButtonElement).click();
    fixture.detectChanges();
    // The nested row exists and is NOT badged shared — it inherited the
    // predicate rather than falling back to `!characterId`.
    const nested = host.querySelector('qt-wardrobe-item-row') as HTMLElement;
    expect(nested.textContent).toContain('Cravat');
    expect(nested.textContent).not.toContain('· shared');
  });
});

/**
 * P4.D121 — the Archive / Restore affordance (v4 `d25dacc1`
 * `__tests__/wardrobe-item-row-archive.test.tsx`, transcribed onto the Angular
 * row). Three rules govern whether it appears, and they are easy to break by
 * accident:
 *
 *  1. It is OPTIONAL — v4's absent `onToggleArchived` prop, here the
 *     `canArchive` gate (default false). The outfit composer, which does the
 *     same job the outfit-selection LLM does, must never offer it.
 *  2. It lives behind `canManage`, alongside Edit and Duplicate: one character
 *     must not be able to retire a coat the whole household shares.
 *  3. The label flips with the item's state, and an archived row is badged.
 */
describe('WardrobeItemRow — the archive affordance (v4 d25dacc1)', () => {
  const ARCHIVED_AT = '2026-02-01T00:00:00.000Z';

  it('offers Archive for an active garment and emits the item', async () => {
    const fixture = await render(dto({ id: 'i1', characterId: 'c1' }), undefined, [], {
      canArchive: true,
    });
    const seen: WardrobeItemDto[] = [];
    fixture.componentInstance.toggleArchived.subscribe((i: WardrobeItemDto) => seen.push(i));
    expect(kebabLabels(fixture)).toEqual([
      'Edit',
      '★ Mark as default outfit item',
      'Duplicate',
      'Archive',
      'Move',
      'Copy',
      'Delete',
    ]);
    const entry = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('[role="menu"] button'),
    ).find((b) => (b.textContent ?? '').trim() === 'Archive') as HTMLButtonElement;
    entry.click();
    fixture.detectChanges();
    expect(seen.map((i) => i.id)).toEqual(['i1']);
  });

  it('offers "Restore from archive" for an archived garment', async () => {
    const fixture = await render(
      dto({ id: 'i1', characterId: 'c1', archivedAt: ARCHIVED_AT }),
      undefined,
      [],
      { canArchive: true },
    );
    const labels = kebabLabels(fixture);
    expect(labels).toContain('Restore from archive');
    expect(labels).not.toContain('Archive');
  });

  it('badges an archived garment in the row itself, and leaves an active one bare', async () => {
    const archived = await render(dto({ id: 'i1', archivedAt: ARCHIVED_AT }));
    expect((archived.nativeElement as HTMLElement).textContent).toContain('archived');
    const active = await render(dto({ id: 'i2' }));
    expect((active.nativeElement as HTMLElement).textContent).not.toContain('archived');
  });

  it('omits the entry entirely when the surface does not allow archiving', async () => {
    const fixture = await render(dto({ id: 'i1', characterId: 'c1' }));
    const labels = kebabLabels(fixture);
    expect(labels).not.toContain('Archive');
    // The unconditional actions are still there.
    expect(labels).toContain('Move');
  });

  it('withholds it from a garment borrowed from another tier, as Edit is withheld', async () => {
    const fixture = await render(dto({ id: 'i1', characterId: null }), () => false, [], {
      canArchive: true,
    });
    const labels = kebabLabels(fixture);
    expect(labels).not.toContain('Archive');
    expect(labels).not.toContain('Edit');
    expect(labels).toContain('Copy');
  });
});
