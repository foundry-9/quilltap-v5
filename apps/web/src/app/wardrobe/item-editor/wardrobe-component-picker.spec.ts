import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { CandidateGroup, CandidateItem } from './types';
import { WardrobeComponentPicker } from './wardrobe-component-picker';

/**
 * P4.D261 — v4 `cc80dc89d` `components/wardrobe/__tests__/wardrobe-picker-
 * rows.test.tsx` (`WardrobeComponentPicker candidates`) at the pin
 * `f5e953a3f`, plus the selected-chip arm of the same hunk
 * (`WardrobeComponentPicker.tsx:92-101`, `:163-183`).
 */
const LONG_TITLE = 'Midnight Lightning Flapper Dress with the Beaded Fringe and the Long Gloves';

function render(candidates: CandidateItem[], selected: CandidateItem[] = []): HTMLElement {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [WardrobeComponentPicker] });
  const fixture = TestBed.createComponent(WardrobeComponentPicker);
  const set = (k: string, v: unknown): void => fixture.componentRef.setInput(k, v);
  set('effectiveTypes', []);
  set('selectedComponents', selected);
  set('componentSearch', '');
  set('candidatesLoading', false);
  set('candidates', candidates);
  set('eligibleCandidates', candidates);
  set('groupedCandidates', new Map<CandidateGroup, CandidateItem[]>([['top', candidates]]));
  set('expandedGroups', new Set<CandidateGroup>(['top']));
  set('componentItemIds', []);
  set('replace', false);
  set('computedTypes', []);
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
