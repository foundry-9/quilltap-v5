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

  it('mounts the pull-down ABOVE the bundle cards and slot rows (v4 `aec86a613` :81)', () => {
    const fixture = render({
      items: [suit, jacket, pants],
      slots: { top: ['suit'], bottom: ['suit'], footwear: [], accessories: [], hair: [] },
      showBundleActions: true,
    });
    const root = (fixture.nativeElement as HTMLElement).querySelector('.space-y-2')!;
    const kinds = [...root.children].map((c) => c.tagName.toLowerCase());
    expect(kinds[0]).toBe('qt-outfit-quick-pick');
    expect(kinds.indexOf('qt-equipped-bundle-card')).toBe(1);
    expect(kinds.indexOf('qt-equipped-slot-row')).toBeGreaterThan(1);
  });

  it('wearing from the pull-down emits the EXISTING addToSlot output (v4 :81-84 — no new equip path)', () => {
    const fixture = render({
      items: [suit, jacket, pants],
      slots: { top: [], bottom: [], footwear: [], accessories: [], hair: [] },
      showBundleActions: true,
    });
    const emitted: { slot: string; itemId: string }[] = [];
    fixture.componentInstance.addToSlot.subscribe((e) => emitted.push(e));

    const el = fixture.nativeElement as HTMLElement;
    el.querySelector<HTMLButtonElement>('button[aria-haspopup="listbox"]')!.click();
    fixture.detectChanges();
    el.querySelector<HTMLButtonElement>('[role="option"]')!.click();
    fixture.detectChanges();

    // `types[0]` names where the gesture STARTED; every consumer re-derives the
    // real destinations through `wearItemIntoSlots`.
    expect(emitted).toEqual([{ slot: 'top', itemId: 'suit' }]);
  });

  it('the pull-down is hidden — not merely empty — when the pool holds no composites', () => {
    const fixture = render({
      items: [jacket, pants],
      slots: { top: [], bottom: [], footwear: [], accessories: [], hair: [] },
      showBundleActions: false,
    });
    const pick = (fixture.nativeElement as HTMLElement).querySelector('qt-outfit-quick-pick')!;
    // v4 renders nothing at all here; `hidden` keeps it out of the Tailwind
    // `space-y-2` sibling chain, so the first slot row's spacing matches v4's.
    expect(pick.hasAttribute('hidden')).toBe(true);
    expect(
      (fixture.nativeElement as HTMLElement).querySelector('button[aria-haspopup="listbox"]'),
    ).toBeNull();
  });

  it('an equipped SINGLE-slot composite still resolves its chip label through the composer (v4 :19-23)', () => {
    // The reason `aec86a613` filters the slot row's CANDIDATES and not its
    // `allItems` input. `groupEquippedSlots` promotes a composite to a bundle
    // card only at two-or-more occupied slots, so a one-slot composite stays in
    // `slotRemainders` and renders as a chip — whose title comes from the same
    // list the picker draws from. Narrow that list at the composer and the chip
    // reads "unknown".
    const pearls = item({
      id: 'pearls',
      title: 'Aunt Dahlia’s Pearls',
      types: ['accessories'],
      componentItemIds: ['earrings'],
    });
    const fixture = render({
      items: [pearls, jacket],
      slots: { top: [], bottom: [], footwear: [], accessories: ['pearls'], hair: [] },
      showBundleActions: true,
    });
    const el = fixture.nativeElement as HTMLElement;
    // No bundle card — one slot only.
    expect(el.querySelectorAll('qt-equipped-bundle-card')).toHaveLength(0);
    const accessories = [...el.querySelectorAll('qt-equipped-slot-row')].find((r) =>
      r.querySelector('.qt-badge-wardrobe-accessories'),
    )!;
    expect(accessories.textContent).toContain('Aunt Dahlia’s Pearls');
    expect(accessories.textContent).not.toContain('unknown');
    // …and the composite is STILL out of that row's own picker.
    [...accessories.querySelectorAll<HTMLButtonElement>('button')]
      .find((b) => b.textContent?.trim() === '+')!
      .click();
    fixture.detectChanges();
    const offered = [...accessories.querySelectorAll('ul li button')].map((b) =>
      b.querySelector('span')!.textContent!.trim(),
    );
    expect(offered).not.toContain('Aunt Dahlia’s Pearls');
  });
});
