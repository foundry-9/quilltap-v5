import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { WardrobeItemDto, WardrobeSlotType } from '../core/core-contract';
import { EquippedSlotRow } from './equipped-slot-row';

/**
 * v4 `components/wardrobe/equipped-slot-row.tsx`. The file's first spec — it
 * arrives with `aec86a613`, whose whole slot-row change is the picker's
 * candidate list, so the pins here are the two halves of that change plus the
 * property v4's own docblock calls out as the reason `allItems` is still passed
 * whole.
 */

function item(partial: Partial<WardrobeItemDto> & { id: string }): WardrobeItemDto {
  return {
    title: partial.id,
    types: ['top'] as WardrobeSlotType[],
    componentItemIds: [],
    isDefault: false,
    replace: false,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...partial,
  } as WardrobeItemDto;
}

function render(inputs: {
  slot: WardrobeSlotType;
  equippedIds: string[];
  allItems: WardrobeItemDto[];
}): ComponentFixture<EquippedSlotRow> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [EquippedSlotRow] });
  const fixture = TestBed.createComponent(EquippedSlotRow);
  fixture.componentRef.setInput('slot', inputs.slot);
  fixture.componentRef.setInput('equippedIds', inputs.equippedIds);
  fixture.componentRef.setInput('allItems', inputs.allItems);
  fixture.detectChanges();
  return fixture;
}

function openPicker(fixture: ComponentFixture<EquippedSlotRow>): HTMLElement {
  const el = fixture.nativeElement as HTMLElement;
  [...el.querySelectorAll<HTMLButtonElement>('button')]
    .find((b) => b.textContent?.trim() === '+')!
    .click();
  fixture.detectChanges();
  return el;
}

function pickerTitles(el: HTMLElement): string[] {
  return [...el.querySelectorAll('ul li button')].map((b) =>
    b.querySelector('span')!.textContent!.trim(),
  );
}

const blouse = item({ id: 'blouse', title: 'Silk Blouse', types: ['top'] });
const dress = item({ id: 'dress', title: 'Green Dress', types: ['top', 'bottom'] });
const suit = item({
  id: 'suit',
  title: 'Three-Piece Suit',
  types: ['top', 'bottom'],
  componentItemIds: ['blouse', 'trousers'],
});
const trousers = item({ id: 'trousers', title: 'Tweed Trousers', types: ['bottom'] });

describe('EquippedSlotRow (v4 equipped-slot-row.tsx)', () => {
  it('offers garments only — composites are gone from the picker (v4 `aec86a613` :85)', () => {
    const el = openPicker(
      render({ slot: 'top', equippedIds: [], allItems: [blouse, dress, suit, trousers] }),
    );
    // A multi-slot LEAF stays: `types: ['top','bottom']` with no components is
    // still one thing you put on.
    expect(pickerTitles(el)).toEqual(['Silk Blouse', 'Green Dress']);
    expect(pickerTitles(el)).not.toContain('Three-Piece Suit');
  });

  it('drops the now-dead “ · composite” suffix (v4 `aec86a613` :179)', () => {
    const el = openPicker(render({ slot: 'top', equippedIds: [], allItems: [blouse, dress] }));
    expect(el.textContent).not.toContain('· composite');
    // RE-PINNED by P4.D261: v4 `cc80dc89d` prints the slot list through
    // `formatSlotLabels` (display labels, canonical order) — was the raw join.
    const metas = [...el.querySelectorAll('ul li button')].map((b) =>
      b.querySelectorAll('span')[1]!.textContent!.trim(),
    );
    expect(metas).toEqual(['Top', 'Top, Bottom']);
  });

  it('an EQUIPPED composite keeps its chip label — `allItems` is still passed whole (v4 :19-23)', () => {
    // The reason v4 filters the candidates rather than the input: the chip for
    // an already-worn composite resolves its title from the same list.
    const fixture = render({
      slot: 'top',
      equippedIds: ['suit'],
      allItems: [blouse, dress, suit, trousers],
    });
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('Three-Piece Suit');
    expect(el.textContent).toContain('· bundle');
    // …and it is still absent from the picker underneath it.
    expect(pickerTitles(openPicker(fixture))).toEqual(['Silk Blouse', 'Green Dress']);
  });

  it('still applies the slot, equipped and search filters after the garment split', () => {
    const fixture = render({
      slot: 'top',
      equippedIds: ['blouse'],
      allItems: [blouse, dress, suit, trousers],
    });
    const el = openPicker(fixture);
    // `trousers` is bottom-only; `blouse` is already on; `suit` is composite.
    expect(pickerTitles(el)).toEqual(['Green Dress']);

    const search = el.querySelector<HTMLInputElement>('input[type="search"]')!;
    search.value = 'zzz';
    search.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(el.textContent).toContain('No matching items.');
  });
});

/**
 * P4.75 (the residue-host adjudication, item 6): this host is a direct child of
 * `outfit-composer.ts`'s `space-y-2 mb-3` stack, and an Angular custom element
 * with no CSS rule renders `display: inline` — which silently ignores the
 * vertical margin `space-y-*` puts on it. Measured live: five slot-row hosts in
 * the wardrobe dialog reported `display: inline`, and the same stylesheet over
 * three 20px rows measured 60px inline vs 76px block (the two missing 8px
 * gaps). The fix is the host class + the `_surfaces.css` rule; jsdom computes no
 * cascade, so the reachable pin is the class that rule targets — the
 * `standalone-document-view.spec.ts` idiom from dogfood #97.
 */
describe('EquippedSlotRow — the host carries its own box (P4.75)', () => {
  it('stamps qt-equipped-slot-row on the host element', () => {
    const fixture = render({ slot: 'top', equippedIds: [], allItems: [blouse] });
    expect(
      (fixture.nativeElement as HTMLElement).classList.contains('qt-equipped-slot-row'),
    ).toBe(true);
  });
});

/**
 * P4.D261 — v4 `cc80dc89d` `components/wardrobe/__tests__/wardrobe-picker-
 * rows.test.tsx` (`EquippedSlotRow picker` ×3) at the pin `f5e953a3f`; the
 * markup is `equipped-slot-row.tsx:171-189`.
 */
describe('EquippedSlotRow picker — wrapped titles and origin (v4 wardrobe-picker-rows.test.tsx)', () => {
  const LONG_TITLE = 'Midnight Lightning Flapper Dress with the Beaded Fringe and the Long Gloves';
  const titleSpan = (el: HTMLElement): HTMLElement =>
    [...el.querySelectorAll<HTMLElement>('ul li button span')].find(
      (s) => s.textContent!.trim() === LONG_TITLE,
    )!;

  it('wraps the title instead of truncating it', () => {
    const el = openPicker(
      render({ slot: 'top', equippedIds: [], allItems: [item({ id: 'item-1', title: LONG_TITLE })] }),
    );
    const title = titleSpan(el);
    expect(title.classList.contains('truncate')).toBe(false);
    expect(title.classList.contains('break-words')).toBe(true);
    expect(title.classList.contains('min-w-0')).toBe(true);
  });

  it('prints slot labels and a borrowed garment’s origin', () => {
    const el = openPicker(
      render({
        slot: 'top',
        equippedIds: [],
        allItems: [
          item({
            id: 'item-1',
            title: LONG_TITLE,
            types: ['bottom', 'top'],
            origin: { scope: 'project', id: 'p1', name: 'Thornfield' },
          }),
        ],
      }),
    );
    expect(titleSpan(el).nextElementSibling!.textContent!.trim()).toBe(
      'Top, Bottom · Project · Thornfield',
    );
  });

  it('adds no origin for the character’s own garment', () => {
    const el = openPicker(
      render({
        slot: 'top',
        equippedIds: [],
        allItems: [
          item({
            id: 'item-1',
            title: LONG_TITLE,
            characterId: 'char-1',
            origin: { scope: 'character', id: 'char-1', name: 'Bertie' },
          }),
        ],
      }),
    );
    expect(titleSpan(el).nextElementSibling!.textContent!.trim()).toBe('Top');
  });
});

/**
 * P4.D261 — v4 `7c8572869` `wardrobe-item-images-ui.test.tsx` (`Picker
 * thumbnails`) at the pin `f5e953a3f`; `equipped-slot-row.tsx:183`.
 */
describe('EquippedSlotRow picker — thumbnails (v4 wardrobe-item-images-ui.test.tsx)', () => {
  it('shows a 28 px thumbnail beside a candidate that has a picture, and none otherwise', () => {
    const el = openPicker(
      render({
        slot: 'top',
        equippedIds: [],
        allItems: [
          item({ id: 'a', title: 'Pictured Blouse', imageFileId: 'file-a' }),
          item({ id: 'b', title: 'Plain Blouse', imageFileId: null }),
        ],
      }),
    );
    const thumbs = [...el.querySelectorAll('[data-testid="wardrobe-item-thumbnail"]')];
    expect(thumbs).toHaveLength(1);
    expect(thumbs[0].getAttribute('src')).toBe('/api/v1/files/file-a?action=thumbnail');
    expect(thumbs[0].getAttribute('width')).toBe('28');
    // The button's FIRST child (v4 :183).
    expect(thumbs[0].closest('button')!.firstElementChild!.contains(thumbs[0])).toBe(true);
  });
});
