import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { WardrobeItemDto } from '../core/core-contract';
import { OutfitComposer } from './outfit-composer';

function item(partial: Partial<WardrobeItemDto> & { id: string }): WardrobeItemDto {
  return {
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

function render(inputs: {
  items: WardrobeItemDto[];
  slots: {
    top: string[];
    bottom: string[];
    footwear: string[];
    accessories: string[];
    hair: string[];
  };
  showBundleActions: boolean;
}): ComponentFixture<OutfitComposer> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [OutfitComposer] });
  const fixture = TestBed.createComponent(OutfitComposer);
  fixture.componentRef.setInput('items', inputs.items);
  fixture.componentRef.setInput('slots', inputs.slots);
  fixture.componentRef.setInput('showBundleActions', inputs.showBundleActions);
  fixture.detectChanges();
  return fixture;
}

describe('OutfitComposer (v4 outfit-composer.tsx)', () => {
  const suit = item({
    id: 'suit',
    title: 'Suit',
    types: ['top', 'bottom'],
    componentItemIds: ['jacket', 'pants'],
  });
  const jacket = item({ id: 'jacket', title: 'Jacket' });
  const pants = item({ id: 'pants', title: 'Pants', types: ['bottom'] });

  it('renders bundle cards above the slot rows, with the composite stripped from rows', () => {
    const fixture = render({
      items: [suit, jacket, pants],
      slots: { top: ['suit'], bottom: ['suit', 'pants'], footwear: [], accessories: [], hair: [] },
      showBundleActions: true,
    });
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    // The bundle card (with actions) is present…
    expect(text).toContain('Suit');
    expect(text).toContain('Take off bundle');
    // …every slot row renders (canonical order labels)…
    for (const label of ['Top', 'Bottom', 'Footwear', 'Accessories', 'Hair']) {
      expect(text).toContain(label);
    }
    // …and the layered leaf remains a chip while the composite id is stripped.
    expect(text).toContain('Pants');
  });

  it('renders the Hair row with its rose badge, and a worn hairdo as a chip (P4.D88)', () => {
    const waves = item({ id: 'waves', title: 'Marcel Waves', types: ['hair'] });
    const fixture = render({
      items: [waves],
      slots: { top: [], bottom: [], footwear: [], accessories: [], hair: ['waves'] },
      showBundleActions: false,
    });
    const el = fixture.nativeElement as HTMLElement;
    // Five rows now — the registry drives the loop, so hair arrived with it.
    expect(el.querySelectorAll('qt-equipped-slot-row')).toHaveLength(5);
    const hairBadge = el.querySelector('.qt-badge-wardrobe-hair');
    expect(hairBadge?.textContent?.trim()).toBe('Hair');
    expect(el.textContent ?? '').toContain('Marcel Waves');
  });

  it('hides bundle actions when showBundleActions=false (v4 `showActions`)', () => {
    const fixture = render({
      items: [suit, jacket, pants],
      slots: { top: ['suit'], bottom: ['suit'], footwear: [], accessories: [], hair: [] },
      showBundleActions: false,
    });
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).not.toContain('Take off bundle');
    expect(text).not.toContain('Break apart');
  });
});
