import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { WardrobeSlotType } from '../core/core-contract';
import { SlotBadge, SlotCheckboxGroup } from './slot-ui';

/**
 * P4.D286 — v4 `components/wardrobe/slot-ui.tsx` (`9ecebf986`, read at the
 * pin `4a38222e6`). No v4 jsdom test names the module; these pin its markup:
 * the badge's label + byte-order class string, the group's canonical order,
 * labels, locked state and title, toggle and focus-out.
 */
function badge(slot: WardrobeSlotType, extraClass?: string): HTMLElement {
  TestBed.resetTestingModule();
  const fixture = TestBed.createComponent(SlotBadge);
  fixture.componentRef.setInput('slot', slot);
  if (extraClass) fixture.componentRef.setInput('extraClass', extraClass);
  fixture.detectChanges();
  return (fixture.nativeElement as HTMLElement).querySelector('span')!;
}

describe('SlotBadge', () => {
  it('prints the display label with the slot’s badge class, in v4’s token order', () => {
    const el = badge('accessories');
    expect(el.textContent).toBe('Accessories');
    expect(el.getAttribute('class')).toBe('qt-badge qt-badge-wardrobe-accessories');
  });

  it('appends an extra class after the slot class', () => {
    expect(badge('hair', 'ml-1').getAttribute('class')).toBe('qt-badge qt-badge-wardrobe-hair ml-1');
  });
});

function group(value: WardrobeSlotType[], locked: WardrobeSlotType[] = [], lockedTitle?: string) {
  TestBed.resetTestingModule();
  const fixture = TestBed.createComponent(SlotCheckboxGroup);
  fixture.componentRef.setInput('value', value);
  fixture.componentRef.setInput('locked', locked);
  if (lockedTitle) fixture.componentRef.setInput('lockedTitle', lockedTitle);
  const toggled: WardrobeSlotType[] = [];
  let blurs = 0;
  fixture.componentInstance.toggle.subscribe((s) => toggled.push(s));
  fixture.componentInstance.groupBlur.subscribe(() => blurs++);
  fixture.detectChanges();
  const el = fixture.nativeElement as HTMLElement;
  return { el, toggled, blurs: () => blurs };
}

describe('SlotCheckboxGroup', () => {
  it('lists every slot by label in canonical order', () => {
    const { el } = group([]);
    expect([...el.querySelectorAll('label span')].map((s) => s.textContent)).toEqual([
      'Top',
      'Bottom',
      'Footwear',
      'Accessories',
      'Hair',
    ]);
    expect(el.querySelector('div')!.getAttribute('class')).toBe('flex flex-wrap gap-3');
  });

  it('checks the value’s slots and emits the toggled slot', () => {
    const { el, toggled } = group(['bottom']);
    const boxes = [...el.querySelectorAll<HTMLInputElement>('input[type=checkbox]')];
    expect(boxes.map((b) => b.checked)).toEqual([false, true, false, false, false]);
    boxes[3].click();
    expect(toggled).toEqual(['accessories']);
  });

  it('shows a locked slot checked, disabled, dimmed and titled', () => {
    const { el } = group([], ['top'], 'Covered by a component — always included');
    const labels = [...el.querySelectorAll('label')];
    const box = labels[0].querySelector<HTMLInputElement>('input')!;
    expect(box.checked).toBe(true);
    expect(box.disabled).toBe(true);
    expect(labels[0].getAttribute('class')).toBe(
      'inline-flex items-center gap-2 opacity-60 cursor-not-allowed',
    );
    expect(labels[0].getAttribute('title')).toBe('Covered by a component — always included');
    expect(labels[1].getAttribute('class')).toBe('inline-flex items-center gap-2 cursor-pointer');
    expect(labels[1].hasAttribute('title')).toBe(false);
  });

  it('reports focus leaving the group', () => {
    const { el, blurs } = group([]);
    el.querySelector('div')!.dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
    expect(blurs()).toBe(1);
  });
});
