/** v4 `components/wardrobe/PronounAwareToggle.tsx` at `236c04e35`. */
import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { fitsPronounGuidance } from './pronoun-guidance';
import { PronounAwareToggle } from './pronoun-aware-toggle';
import type { PronounGuidanceFilter } from './pronoun-guidance-filter';

const HE = { subject: 'he', object: 'him', possessive: 'his' };
const ITEMS = [{ pronounGuidance: ['she'] }, { pronounGuidance: ['it'] }, {}];

function stubFilter(applies: boolean): PronounGuidanceFilter {
  const aware = signal(true);
  return {
    applies: signal(applies),
    aware,
    setAware: (v) => aware.set(v),
    shows: (i) => !aware() || fitsPronounGuidance(i, HE),
    offer: (items) => [...items],
    countHeldBack: (items) => items.filter((i) => !fitsPronounGuidance(i, HE)).length,
  };
}

function render(filter: PronounGuidanceFilter) {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [PronounAwareToggle] });
  const fixture = TestBed.createComponent(PronounAwareToggle);
  fixture.componentRef.setInput('filter', filter);
  fixture.componentRef.setInput('items', ITEMS);
  fixture.detectChanges();
  return fixture;
}

describe('PronounAwareToggle', () => {
  it('renders nothing when the filter does not apply (a custom character)', () => {
    expect(
      (render(stubFilter(false)).nativeElement as HTMLElement).querySelector('label'),
    ).toBeNull();
  });

  it('renders v4’s label, title and qt-checkbox, with the hidden count only while aware', () => {
    const filter = stubFilter(true);
    const fixture = render(filter);
    const label = (fixture.nativeElement as HTMLElement).querySelector('label')!;
    expect(label.className).toBe('flex items-center gap-2 qt-text-xs qt-text-secondary');
    expect(label.getAttribute('title')).toBe(
      'Offer only pieces whose pronoun guidance suits this character (pieces without guidance are always offered)',
    );
    const box = label.querySelector('input')!;
    expect(box.className).toBe('qt-checkbox');
    expect(box.checked).toBe(true);
    expect(label.textContent!.replace(/\s+/g, ' ').trim()).toBe('Pronoun-aware (2 hidden)');
    box.checked = false;
    box.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(filter.aware()).toBe(false);
    expect(label.textContent!.replace(/\s+/g, ' ').trim()).toBe('Pronoun-aware');
  });
});
