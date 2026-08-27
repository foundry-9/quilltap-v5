import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { WardrobeItemDto, WardrobeSlotType } from '../core/core-contract';
import { OutfitQuickPick } from './outfit-quick-pick';

/**
 * v4 `components/wardrobe/outfit-quick-pick.tsx` (`aec86a613`, 138 lines).
 *
 * The pure pool split it runs on has its own recorded differential
 * (`composed-outfits.spec.ts`); what this file pins is everything the
 * transcription cannot see — the render gate, the row copy, and the two
 * document-level listeners, each cited to the v4 line that states it.
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

function render(items: WardrobeItemDto[]): ComponentFixture<OutfitQuickPick> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [OutfitQuickPick] });
  const fixture = TestBed.createComponent(OutfitQuickPick);
  fixture.componentRef.setInput('items', items);
  fixture.detectChanges();
  return fixture;
}

function el(fixture: ComponentFixture<OutfitQuickPick>): HTMLElement {
  return fixture.nativeElement as HTMLElement;
}

function openMenu(fixture: ComponentFixture<OutfitQuickPick>): void {
  el(fixture).querySelector<HTMLButtonElement>('button[aria-haspopup="listbox"]')!.click();
  fixture.detectChanges();
}

const gownComponents = ['bodice', 'skirt'];

const sunday = item({
  id: 'sunday',
  title: 'Sunday Best',
  types: ['top', 'bottom', 'footwear'],
  componentItemIds: gownComponents,
});
const alpine = item({
  id: 'alpine',
  title: 'Alpine Tweeds',
  types: ['top'],
  componentItemIds: gownComponents,
  replace: true,
});
const dress = item({ id: 'dress', title: 'Green Dress', types: ['top', 'bottom'] });
const boots = item({ id: 'boots', title: 'Ankle Boots', types: ['footwear'] });

afterEach(() => {
  // Any listener a beat left attached would leak into the next one.
  document.body.innerHTML = '';
});

describe('OutfitQuickPick (v4 outfit-quick-pick.tsx)', () => {
  it('renders nothing when the pool holds no composed outfits (v4 :74)', () => {
    const fixture = render([dress, boots]);
    const host = el(fixture);
    expect(host.querySelector('button[aria-haspopup="listbox"]')).toBeNull();
    expect(host.textContent?.trim()).toBe('');
    // v4 returns null, so the node leaves the DOM. An Angular host cannot, so
    // it is `hidden` — which also drops it out of the composer's `space-y-2`
    // sibling chain, the one layout consequence of the difference.
    expect(fixture.componentRef.location.nativeElement.hasAttribute('hidden')).toBe(true);
  });

  it('renders the toggle when the pool holds one, and drops the hidden attribute', () => {
    const fixture = render([dress, sunday]);
    const toggle = el(fixture).querySelector<HTMLButtonElement>('button[aria-haspopup="listbox"]');
    expect(toggle).not.toBeNull();
    expect(toggle!.textContent).toContain('Wear an outfit…');
    expect(toggle!.getAttribute('title')).toBe('Put on a whole outfit at once');
    expect(toggle!.getAttribute('aria-expanded')).toBe('false');
    expect(fixture.componentRef.location.nativeElement.hasAttribute('hidden')).toBe(false);
    // Closed: no listbox.
    expect(el(fixture).querySelector('[role="listbox"]')).toBeNull();
  });

  it('lists outfits title-sorted, with slot labels joined and “ · replaces” only where set (v4 :117-121)', () => {
    const fixture = render([dress, sunday, alpine, boots]);
    openMenu(fixture);
    const rows = [...el(fixture).querySelectorAll<HTMLElement>('[role="option"]')];
    // Title order: Alpine Tweeds, Sunday Best — NOT the input order.
    expect(rows.map((r) => r.querySelector('span')!.textContent)).toEqual([
      'Alpine Tweeds',
      'Sunday Best',
    ]);
    const metas = rows.map((r) => r.querySelectorAll('span')[1]!.textContent);
    // WARDROBE_SLOT_META labels, ', '-joined — not the raw type strings.
    expect(metas[0]).toBe('Top · replaces');
    expect(metas[1]).toBe('Top, Bottom, Footwear');
    expect(el(fixture).querySelector('[role="listbox"]')).not.toBeNull();
    expect(
      el(fixture).querySelector('button[aria-haspopup="listbox"]')!.getAttribute('aria-expanded'),
    ).toBe('true');
  });

  it('filters on the search box and shows v4’s empty sentence (v4 :104)', () => {
    const fixture = render([sunday, alpine]);
    openMenu(fixture);
    const input = el(fixture).querySelector<HTMLInputElement>('input[type="search"]')!;
    expect(input.getAttribute('placeholder')).toBe('Search outfits…');

    input.value = 'alp';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(el(fixture).querySelectorAll('[role="option"]')).toHaveLength(1);
    expect(el(fixture).textContent).toContain('Alpine Tweeds');

    input.value = 'nothing here';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(el(fixture).querySelectorAll('[role="option"]')).toHaveLength(0);
    expect(el(fixture).textContent).toContain('No matching outfits.');
  });

  it('emits the chosen outfit whole, then closes and clears the search (v4 :113-116)', () => {
    const fixture = render([sunday, alpine]);
    const worn: WardrobeItemDto[] = [];
    fixture.componentInstance.wear.subscribe((o) => worn.push(o));
    openMenu(fixture);
    const input = el(fixture).querySelector<HTMLInputElement>('input[type="search"]')!;
    input.value = 'sun';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    el(fixture).querySelector<HTMLButtonElement>('[role="option"]')!.click();
    fixture.detectChanges();

    expect(worn.map((o) => o.id)).toEqual(['sunday']);
    expect(el(fixture).querySelector('[role="listbox"]')).toBeNull();
    // Reopening starts from a blank filter, so both outfits are back.
    openMenu(fixture);
    expect(el(fixture).querySelectorAll('[role="option"]')).toHaveLength(2);
  });

  it('closes on an outside mousedown and stays open on an inside one (v4 :41-51)', () => {
    const fixture = render([sunday]);
    document.body.appendChild(fixture.nativeElement);
    openMenu(fixture);
    expect(el(fixture).querySelector('[role="listbox"]')).not.toBeNull();

    // Inside: the listbox itself.
    el(fixture)
      .querySelector('[role="listbox"]')!
      .dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    fixture.detectChanges();
    expect(el(fixture).querySelector('[role="listbox"]')).not.toBeNull();

    // Outside: anywhere else in the document.
    const outside = document.createElement('div');
    document.body.appendChild(outside);
    outside.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    fixture.detectChanges();
    expect(el(fixture).querySelector('[role="listbox"]')).toBeNull();
  });

  it('Escape closes the menu WITHOUT reaching an enclosing dialog’s handler (v4 :55-66)', () => {
    const fixture = render([sunday]);
    document.body.appendChild(fixture.nativeElement);

    // Stand in for the wardrobe dialog: a BUBBLE-phase document listener, which
    // is what `(document:keydown.escape)` compiles to. v4 registers its own
    // handler in the CAPTURE phase and calls stopPropagation precisely so this
    // one never runs — a bubble-phase port would dismiss the whole modal.
    const dialogClose = vi.fn();
    const dialogHandler = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') dialogClose();
    };
    document.addEventListener('keydown', dialogHandler);

    try {
      // Closed: the menu is not listening, so the dialog DOES see Escape.
      document.body.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
      );
      expect(dialogClose).toHaveBeenCalledTimes(1);

      openMenu(fixture);
      const event = new KeyboardEvent('keydown', {
        key: 'Escape',
        bubbles: true,
        cancelable: true,
      });
      document.body.dispatchEvent(event);
      fixture.detectChanges();

      expect(el(fixture).querySelector('[role="listbox"]')).toBeNull();
      // Still 1 — the menu swallowed this one.
      expect(dialogClose).toHaveBeenCalledTimes(1);
      expect(event.defaultPrevented).toBe(true);
    } finally {
      document.removeEventListener('keydown', dialogHandler);
    }
  });

  it('a non-Escape key is left entirely alone (v4 :57)', () => {
    const fixture = render([sunday]);
    document.body.appendChild(fixture.nativeElement);
    openMenu(fixture);
    const seen = vi.fn();
    const handler = (): void => seen();
    document.addEventListener('keydown', handler);
    try {
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'a', bubbles: true }));
      expect(seen).toHaveBeenCalledTimes(1);
      expect(el(fixture).querySelector('[role="listbox"]')).not.toBeNull();
    } finally {
      document.removeEventListener('keydown', handler);
    }
  });

  it('autofocuses the search box as the menu opens (v4 `autoFocus`, :96)', () => {
    const fixture = render([sunday]);
    document.body.appendChild(fixture.nativeElement);
    openMenu(fixture);
    const input = el(fixture).querySelector<HTMLInputElement>('input[type="search"]')!;
    expect(document.activeElement).toBe(input);
  });

  it('detaches both document listeners when the menu closes and on destroy', () => {
    const fixture = render([sunday]);
    document.body.appendChild(fixture.nativeElement);
    const add = vi.spyOn(document, 'addEventListener');
    const remove = vi.spyOn(document, 'removeEventListener');
    try {
      openMenu(fixture);
      expect(add.mock.calls.filter((c) => c[0] === 'mousedown')).toHaveLength(1);
      expect(add.mock.calls.filter((c) => c[0] === 'keydown' && c[2] === true)).toHaveLength(1);
      // Toggling closed detaches…
      openMenu(fixture);
      expect(remove.mock.calls.filter((c) => c[0] === 'mousedown')).toHaveLength(1);
      expect(remove.mock.calls.filter((c) => c[0] === 'keydown' && c[2] === true)).toHaveLength(1);
      // …and so does destruction, whatever state it was in.
      openMenu(fixture);
      fixture.destroy();
      expect(remove.mock.calls.filter((c) => c[0] === 'mousedown')).toHaveLength(2);
    } finally {
      add.mockRestore();
      remove.mockRestore();
    }
  });
});
