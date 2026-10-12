import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { WardrobeSlotType } from '../../core/core-contract';
import type { CandidateGroup, CandidateItem } from './types';
import { WardrobeComponentPicker } from './wardrobe-component-picker';

/**
 * P4.D261 — v4 `cc80dc89d` `components/wardrobe/__tests__/wardrobe-picker-
 * rows.test.tsx` (`WardrobeComponentPicker candidates`) at the pin
 * `f5e953a3f`, plus the selected-chip arm of the same hunk
 * (`WardrobeComponentPicker.tsx:92-101`, `:163-183`).
 */
const LONG_TITLE = 'Midnight Lightning Flapper Dress with the Beaded Fringe and the Long Gloves';

function render(
  candidates: CandidateItem[],
  selected: CandidateItem[] = [],
  opts: {
    replace?: boolean;
    effectiveTypes?: WardrobeSlotType[];
    computedTypes?: WardrobeSlotType[];
  } = {},
): HTMLElement {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [WardrobeComponentPicker] });
  const fixture = TestBed.createComponent(WardrobeComponentPicker);
  const set = (k: string, v: unknown): void => fixture.componentRef.setInput(k, v);
  set('effectiveTypes', opts.effectiveTypes ?? []);
  set('selectedComponents', selected);
  set('componentSearch', '');
  set('candidatesLoading', false);
  set('candidates', candidates);
  set('eligibleCandidates', candidates);
  set('groupedCandidates', new Map<CandidateGroup, CandidateItem[]>([['top', candidates]]));
  set('expandedGroups', new Set<CandidateGroup>(['top']));
  set('componentItemIds', []);
  set('replace', opts.replace ?? false);
  set('computedTypes', opts.computedTypes ?? []);
  set('showComponentsError', false);
  fixture.detectChanges();
  return fixture.nativeElement as HTMLElement;
}

const borrowed: CandidateItem = {
  id: 'c1',
  title: LONG_TITLE,
  types: ['top'],
  componentItemIds: [],
  origin: { scope: 'general', id: null, name: 'Quilltap General' },
};

describe('WardrobeComponentPicker candidates (v4 wardrobe-picker-rows.test.tsx)', () => {
  it('keeps the origin chip outside the wrapping title', () => {
    const el = render([borrowed]);
    const title = [...el.querySelectorAll<HTMLElement>('ul li label span')].find(
      (s) => s.textContent!.trim() === LONG_TITLE,
    )!;
    expect(title.classList.contains('truncate')).toBe(false);
    expect(title.classList.contains('break-words')).toBe(true);
    const chip = el.querySelector<HTMLElement>('ul li .qt-badge-wardrobe-shared')!;
    expect(chip.textContent!.trim()).toBe('Shared · Quilltap General');
    expect(title.contains(chip)).toBe(false);
  });

  it('prints the meta as slot labels plus “ · bundle” (v4 `:177-180`)', () => {
    const el = render([
      { ...borrowed, origin: null, types: ['bottom', 'top'], componentItemIds: ['x'] },
    ]);
    const label = el.querySelector('ul li label')!;
    expect(label.querySelector('.qt-badge-wardrobe-shared')).toBeNull();
    const spans = [...label.querySelectorAll('span')];
    expect(spans.at(-1)!.textContent!.trim()).toBe('Top, Bottom · bundle');
  });

  it('the selected-component chip names the origin, no bare “shared” (v4 `:92-101`)', () => {
    const el = render([], [borrowed]);
    const chip = el.querySelector<HTMLElement>('.qt-badge-wardrobe-shared')!;
    expect(chip.textContent!.trim()).toBe('Shared · Quilltap General');
    expect(el.querySelector('.qt-badge-info')).toBeNull();
  });
});

/**
 * P4.D286 — v4 `9ecebf986` `WardrobeComponentPicker.tsx` at the pin
 * `4a38222e6` (no v4 jsdom case names these hunks; the markup is pinned
 * here): the coverage chips are `SlotBadge` labels (`:70`), an archived
 * candidate's meta ends ` · archived` (`:174-178`), and the slot designation
 * is ALWAYS shown (`:217-231`, bug 190 — an additive outfit designates too),
 * its sentence by `replace`, a component's slots locked with the title
 * `Covered by a component — always included`.
 */
describe('WardrobeComponentPicker — slots and archived parts (v4 9ecebf986)', () => {
  it('prints the coverage chips as slot labels', () => {
    const el = render([], [], { effectiveTypes: ['top', 'accessories'] });
    const chips = [...el.querySelectorAll('qt-slot-badge span')];
    expect(chips.map((c) => c.textContent)).toEqual(['Top', 'Accessories']);
    expect(chips[1].getAttribute('class')).toBe('qt-badge qt-badge-wardrobe-accessories');
  });

  it('marks an archived candidate “ · archived” after its slots', () => {
    const el = render([{ ...borrowed, origin: null, archived: true, componentItemIds: ['x'] }]);
    const spans = [...el.querySelector('ul li label')!.querySelectorAll('span')];
    expect(spans.at(-1)!.textContent!.trim()).toBe('Top · bundle · archived');
  });

  it('shows the designation for an additive outfit, saying what it covers', () => {
    const el = render([], [], { replace: false, effectiveTypes: ['top', 'hair'], computedTypes: ['top'] });
    expect(el.textContent).toContain(
      'Slots this outfit covers (its components’ slots are always included):',
    );
    expect(el.textContent).not.toContain('Slots this outfit clears');
    const group = el.querySelector('qt-slot-checkbox-group')!;
    const labels = [...group.querySelectorAll('label')];
    expect(labels.map((l) => l.querySelector('input')!.checked)).toEqual([
      true,
      false,
      false,
      false,
      true,
    ]);
    expect(labels[0].querySelector('input')!.disabled).toBe(true);
    expect(labels[0].getAttribute('title')).toBe('Covered by a component — always included');
    expect(labels[4].querySelector('input')!.disabled).toBe(false);
  });

  it('says what a replace outfit clears', () => {
    const el = render([], [], { replace: true });
    expect(el.textContent).toContain(
      'Slots this outfit clears (its components’ slots are always included):',
    );
  });
});
