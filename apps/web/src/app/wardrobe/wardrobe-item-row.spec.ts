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
): Promise<ComponentFixture<WardrobeItemRow>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [WardrobeItemRow] });
  const fixture = TestBed.createComponent(WardrobeItemRow);
  fixture.componentRef.setInput('item', item);
  fixture.componentRef.setInput('allItems', allItems);
  fixture.componentRef.setInput('inChat', false);
  if (canManage) fixture.componentRef.setInput('canManage', canManage);
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
